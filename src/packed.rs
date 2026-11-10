//! Архив `.zip` со статьёй — свой (`save`) или чужой с markdown внутри —
//! читается как папка, без распаковки. Путь внутри архива пишется
//! продолжением пути к нему: `…/Статья.zip/images/01-a.png`. Так картинки
//! markdown разворачиваются обычным `dir.join(src)` (`media::resolve`),
//! а с диска и из архива их достаёт одно и то же [`read`].
//!
//! Зачем: Flatpak и Android получают от файлового менеджера только сам
//! открытый файл, без папки рядом, и распакованный md остаётся без картинок.
//! Архив — один файл, и в нём всё.
//!
//! Чужой архив — чужие данные: читаем только нужный файл, не больше `limit`
//! распакованных байт (бомба из нулей объявляет один размер, а распаковывается
//! в другой), и на диск ничего не пишем.

use std::path::Path;

/// Начало zip: локальный заголовок файла или конец пустого архива.
pub fn is_archive(bytes: &[u8]) -> bool {
    bytes.starts_with(b"PK\x03\x04") || bytes.starts_with(b"PK\x05\x06")
}

/// Файл с диска, а нет его там — из архива на его пути. Обычный файл
/// читается как прежде, без предела: предел — только распаковке.
pub fn read(path: &Path, limit: u64) -> std::io::Result<Vec<u8>> {
    match std::fs::read(path) {
        Ok(bytes) => Ok(bytes),
        Err(error) => {
            #[cfg(feature = "save")]
            if let Some((archive, name)) = inside(path) {
                return entry(&archive, &name, limit);
            }
            let _ = limit;
            Err(error)
        }
    }
}

/// Архив и имя файла в нём, если `path` ведёт внутрь архива: первый
/// существующий предок — файл zip, а остаток — обычные имена (`..` в zip
/// не бывает, и через него из архива не выйти).
#[cfg(feature = "save")]
fn inside(path: &Path) -> Option<(std::path::PathBuf, String)> {
    let archive = path.ancestors().skip(1).find(|up| up.exists())?;
    if !archive.is_file() || !starts_as_archive(archive) {
        return None;
    }
    let rest = path.strip_prefix(archive).ok()?;
    let names: Option<Vec<&str>> = rest
        .components()
        .map(|part| match part {
            std::path::Component::Normal(name) => name.to_str(),
            _ => None,
        })
        .collect();
    let names = names.filter(|names| !names.is_empty())?;
    Some((archive.to_path_buf(), names.join("/")))
}

#[cfg(feature = "save")]
fn starts_as_archive(path: &Path) -> bool {
    use std::io::Read;
    let mut head = [0u8; 4];
    std::fs::File::open(path)
        .and_then(|mut file| file.read_exact(&mut head))
        .is_ok()
        && is_archive(&head)
}

#[cfg(feature = "save")]
fn open(archive: &Path) -> std::io::Result<zip::ZipArchive<std::io::BufReader<std::fs::File>>> {
    let file = std::fs::File::open(archive)?;
    zip::ZipArchive::new(std::io::BufReader::new(file)).map_err(broken)
}

#[cfg(feature = "save")]
fn broken(error: zip::result::ZipError) -> std::io::Error {
    match error {
        zip::result::ZipError::FileNotFound => std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "there is no such file in the archive",
        ),
        zip::result::ZipError::Io(error) => error,
        other => std::io::Error::new(std::io::ErrorKind::InvalidData, other),
    }
}

/// Один файл архива, распакованный не больше чем в `limit` байт.
#[cfg(feature = "save")]
fn entry(archive: &Path, name: &str, limit: u64) -> std::io::Result<Vec<u8>> {
    use std::io::Read;
    let mut zip = open(archive)?;
    let file = zip.by_name(name).map_err(broken)?;
    if file.is_dir() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::IsADirectory,
            "this is a folder in the archive",
        ));
    }
    let mut out = Vec::new();
    file.take(limit.saturating_add(1)).read_to_end(&mut out)?;
    if out.len() as u64 > limit {
        return Err(std::io::Error::new(
            std::io::ErrorKind::FileTooLarge,
            format!("the file in the archive is bigger than {limit} bytes"),
        ));
    }
    Ok(out)
}

/// Страницы архива — файлы markdown ближе всех к его корню, по имени.
/// Свой архив кладёт статью в корень, а выгрузка заметок — по файлу
/// на страницу; распакованная и снова сжатая папка лежит в архиве уровнем
/// ниже. Служебное macOS (`__MACOSX/`, `._имя`) и скрытое — не страницы.
#[cfg(feature = "save")]
pub fn pages(archive: &Path) -> std::io::Result<Vec<String>> {
    let zip = open(archive)?;
    let mut found: Vec<String> = zip
        .file_names()
        .filter_map(Result::ok)
        .filter(|name| is_page(name))
        .map(|name| name.into_owned())
        .collect();
    let Some(depth) = found.iter().map(|name| name.matches('/').count()).min() else {
        return Ok(found);
    };
    found.retain(|name| name.matches('/').count() == depth);
    found.sort();
    Ok(found)
}

#[cfg(feature = "save")]
fn is_page(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    (lower.ends_with(".md") || lower.ends_with(".markdown"))
        && name
            .split('/')
            .all(|part| !part.is_empty() && !part.starts_with('.') && part != "__MACOSX")
}

#[cfg(all(test, feature = "save"))]
mod tests {
    use super::*;
    use crate::scratch::Scratch;
    use std::io::Write;

    fn pack(path: &Path, files: &[(&str, &[u8])]) {
        let mut zip = zip::ZipWriter::new(std::fs::File::create(path).unwrap());
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        for (name, bytes) in files {
            if name.ends_with('/') {
                zip.add_directory(*name, options).unwrap();
            } else {
                zip.start_file(*name, options).unwrap();
                zip.write_all(bytes).unwrap();
            }
        }
        zip.finish().unwrap();
    }

    #[test]
    fn a_picture_is_read_from_inside_the_archive() {
        let dir = Scratch::new("packed-read");
        std::fs::create_dir_all(&dir).unwrap();
        let archive = dir.join("Статья.zip");
        pack(
            &archive,
            &[
                ("Статья.md", b"# Title\n\n![](images/01-a.png)\n"),
                ("images/01-a.png", b"picture bytes"),
            ],
        );
        let picture = archive.join("images/01-a.png");
        assert_eq!(read(&picture, 1024).unwrap(), b"picture bytes");
        let missing = read(&archive.join("images/02-b.png"), 1024).unwrap_err();
        assert_eq!(missing.kind(), std::io::ErrorKind::NotFound);
        // Предел — распакованным байтам, не объявленным в архиве.
        let big = read(&picture, 4).unwrap_err();
        assert_eq!(big.kind(), std::io::ErrorKind::FileTooLarge);
        // Папка архива — не файл, а из архива наверх не выйти.
        assert!(read(&archive.join("images"), 1024).is_err());
        assert!(read(&archive.join("../Статья.zip/Статья.md"), 1024).is_err());
    }

    #[test]
    fn a_missing_file_next_to_a_plain_file_stays_missing() {
        let dir = Scratch::new("packed-plain");
        std::fs::create_dir_all(&dir).unwrap();
        let page = dir.join("page.md");
        std::fs::write(&page, b"PK\x03\x04 is how this text starts").unwrap();
        let error = read(&page.join("images/a.png"), 1024).unwrap_err();
        assert_ne!(error.kind(), std::io::ErrorKind::FileTooLarge);
        assert!(read(&dir.join("images/a.png"), 1024).is_err());
    }

    #[test]
    fn a_saved_article_opens_with_its_pictures() {
        use crate::{Address, UserAgent, media};
        let dir = Scratch::new("packed-open");
        std::fs::create_dir_all(&dir).unwrap();
        let archive = dir.join("Saved.zip");
        pack(
            &archive,
            &[
                ("Saved.md", b"# Saved\n\n![](images/01-a.png)\n"),
                ("images/01-a.png", b"picture bytes"),
            ],
        );
        let document = crate::open(&Address::File(archive.clone()), UserAgent::Honest).unwrap();
        assert_eq!(document.title, "Saved");
        assert_eq!(document.address, Address::File(archive.join("Saved.md")));
        let source = media::resolve(&document.address, "images/01-a.png").unwrap();
        let (bytes, _) = media::grab(&source, UserAgent::Honest).unwrap();
        assert_eq!(bytes, b"picture bytes");
        // Нет файла — отказ картинки, а не «не понял страницу».
        let gone = media::resolve(&document.address, "images/02-b.png").unwrap();
        assert!(matches!(
            media::grab(&gone, UserAgent::Honest),
            Err(crate::Error::Media(_))
        ));
    }

    #[test]
    fn an_archive_of_several_pages_lists_them() {
        use crate::{Address, Kind, UserAgent};
        let dir = Scratch::new("packed-list");
        std::fs::create_dir_all(&dir).unwrap();
        let archive = dir.join("Brevier notes.zip");
        pack(
            &archive,
            &[("One [1].md", b"# One\n"), ("Two.md", b"# Two\n")],
        );
        let listing = crate::open(&Address::File(archive.clone()), UserAgent::Honest).unwrap();
        assert_eq!(listing.kind, Kind::Listing);
        let links: Vec<&str> = listing
            .markdown
            .lines()
            .filter_map(|line| line.split_once("](").map(|(_, rest)| rest))
            .collect();
        assert_eq!(links.len(), 2, "{}", listing.markdown);
        assert!(
            listing.markdown.contains(r"[One \[1\].md]"),
            "{}",
            listing.markdown
        );
        // Скобки в пути (`ThreadId(5)` во временной папке) прячут адрес в `<…>`.
        let first = links[0]
            .trim_end_matches(')')
            .trim_start_matches('<')
            .trim_end_matches('>');
        let address = crate::address::parse(first).unwrap();
        assert_eq!(address, Address::File(archive.join("One [1].md")));
        assert_eq!(
            crate::open(&address, UserAgent::Honest).unwrap().title,
            "One"
        );

        let empty = dir.join("pictures.zip");
        pack(&empty, &[("x.png", b"")]);
        assert!(matches!(
            crate::open(&Address::File(empty), UserAgent::Honest),
            Err(crate::Error::UnsupportedContentType(_))
        ));
    }

    #[test]
    fn pages_are_the_markdown_nearest_the_root() {
        let dir = Scratch::new("packed-pages");
        std::fs::create_dir_all(&dir).unwrap();
        let saved = dir.join("saved.zip");
        pack(
            &saved,
            &[
                ("__MACOSX/._a.md", b""),
                ("Folder/", b""),
                ("Folder/b.md", b"# B"),
                ("Folder/a.MD", b"# A"),
                ("Folder/deeper/c.md", b"# C"),
                ("Folder/.hidden.md", b""),
                ("Folder/images/x.png", b""),
            ],
        );
        assert_eq!(pages(&saved).unwrap(), ["Folder/a.MD", "Folder/b.md"]);
        let empty = dir.join("empty.zip");
        pack(&empty, &[("images/x.png", b"")]);
        assert!(pages(&empty).unwrap().is_empty());
    }
}
