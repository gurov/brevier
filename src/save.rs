//! Сохранить прочитанное: markdown, а если в статье есть картинки — zip.
//!
//! Смысл ровно в том, ради чего markdown выбран внутренним представлением:
//! статья уже сведена к тексту, и «сохранить как» достаётся почти даром.
//! Сохраняем то же, что читатель видел на экране, — без скриптов, стилей
//! и хвостов, которые вычло извлечение.
//!
//! Картинки кладём как есть, байт в байт: перекодировать чужую иллюстрацию,
//! чтобы положить её в архив, незачем. Ссылки в тексте переписываем на файлы
//! внутри архива, иначе сохранённая статья остаётся привязанной к сети.
//!
//! Ядро, а не интерфейс: диалог выбора файла — дело окна, а что и как лечь
//! на диск — дело продукта.

use std::io::Write;
use std::path::{Path, PathBuf};

use crate::error::Error;
use crate::fetch::UserAgent;
use crate::media::{self, MAX_IMAGE, Source};
use crate::{Document, markdown};

/// Папка с картинками внутри архива.
const IMAGES: &str = "images";

/// Что получилось. Число картинок нужно окну: читателю говорят, что именно
/// сохранилось, а не «готово».
#[derive(Debug, Clone)]
pub struct Saved {
    pub path: PathBuf,
    pub images: usize,
    /// Сколько картинок не отдал сервер. Не ошибка: текст всё равно сохранён.
    pub missed: usize,
}

/// Как назвать файл по умолчанию: имя статьи и расширение по содержимому.
///
/// Расширение выбирает не читатель, а документ: есть картинки — архив,
/// нет — просто текст.
pub fn suggested_name(document: &Document) -> String {
    let stem = slug(&document.title);
    if has_images(document) {
        format!("{stem}.zip")
    } else {
        format!("{stem}.md")
    }
}

/// Сохранить статью по указанному пути.
///
/// Что писать, решает расширение: путь `…zip` — архив с картинками,
/// любой другой — один файл markdown. Читатель мог переименовать файл
/// в диалоге, и его выбор старше нашего предложения.
pub fn write(path: &Path, document: &Document, ua: UserAgent) -> Result<Saved, Error> {
    let zip = path
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("zip"));

    if !zip {
        std::fs::write(path, document.markdown.as_bytes()).map_err(Error::Convert)?;
        return Ok(Saved {
            path: path.to_path_buf(),
            images: 0,
            missed: 0,
        });
    }

    let (markdown, images, missed) = gather(document, ua);
    let stem = path
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_else(|| slug(&document.title));

    let file = std::fs::File::create(path).map_err(Error::Convert)?;
    let mut archive = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);

    let put = |archive: &mut zip::ZipWriter<std::fs::File>, name: &str, bytes: &[u8]| {
        archive
            .start_file(name, options)
            .and_then(|()| archive.write_all(bytes).map_err(Into::into))
            .map_err(|e| Error::Convert(std::io::Error::other(e)))
    };

    put(&mut archive, &format!("{stem}.md"), markdown.as_bytes())?;
    for (name, bytes) in &images {
        put(&mut archive, name, bytes)?;
    }
    archive
        .finish()
        .map_err(|e| Error::Convert(std::io::Error::other(e)))?;

    Ok(Saved {
        path: path.to_path_buf(),
        images: images.len(),
        missed,
    })
}

/// Скачать картинки статьи и переписать ссылки на них.
///
/// Что не отдалось — не беда: ссылка остаётся сетевой, а статья сохраняется
/// всё равно. Потерять текст из-за одной битой картинки было бы обидно.
fn gather(document: &Document, ua: UserAgent) -> (String, Vec<(String, Vec<u8>)>, usize) {
    let mut markdown = document.markdown.clone();
    let mut images = Vec::new();
    let mut missed = 0;

    for (number, url) in markdown::images(&document.markdown).iter().enumerate() {
        let Some(source) = media::resolve(&document.address, url) else {
            missed += 1;
            continue;
        };
        let Some((bytes, mime)) = read(&source, ua) else {
            missed += 1;
            continue;
        };

        let name = format!("{IMAGES}/{:02}-{}", number + 1, file_name(url, &mime));
        markdown = relink(&markdown, url, &name);
        images.push((name, bytes));
    }
    (markdown, images, missed)
}

fn read(source: &Source, ua: UserAgent) -> Option<(Vec<u8>, String)> {
    match source {
        Source::Web(url) => crate::fetch::binary(url, ua, "image/*", MAX_IMAGE)
            .ok()
            .map(|blob| (blob.bytes, blob.mime)),
        Source::File(path) => std::fs::read(path).ok().map(|bytes| (bytes, String::new())),
    }
}

/// Заменить сетевой адрес картинки на файл в архиве.
///
/// По тексту, а не через печать разметки: обратная печать comrak экранирует
/// живой текст (`публикация\!`) — по этой же причине её нет и в тракте вывода.
fn relink(markdown: &str, url: &str, name: &str) -> String {
    markdown
        .replace(&format!("]({url})"), &format!("]({name})"))
        .replace(&format!("]({url} "), &format!("]({name} "))
}

/// Имя файла картинки. Из адреса, если там есть на что смотреть, иначе
/// по типу от сервера: без расширения архив открывается, но картинки в нём
/// не показываются.
fn file_name(url: &str, mime: &str) -> String {
    let path = url.split(['?', '#']).next().unwrap_or(url);
    let tail = path.rsplit('/').next().unwrap_or_default();

    let name: String = tail
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_') {
                c
            } else {
                '-'
            }
        })
        .collect();
    let name = name.trim_matches(['-', '.']).to_owned();

    let stem = if name.is_empty() {
        "image".to_owned()
    } else {
        name
    };
    if Path::new(&stem).extension().is_some() {
        return stem;
    }
    match extension(mime) {
        Some(extension) => format!("{stem}.{extension}"),
        None => stem,
    }
}

fn extension(mime: &str) -> Option<&'static str> {
    match mime.split(';').next().unwrap_or(mime).trim() {
        "image/png" => Some("png"),
        "image/jpeg" => Some("jpg"),
        "image/gif" => Some("gif"),
        "image/webp" => Some("webp"),
        "image/svg+xml" => Some("svg"),
        "image/avif" => Some("avif"),
        _ => None,
    }
}

fn has_images(document: &Document) -> bool {
    markdown::images(&document.markdown)
        .iter()
        .any(|url| media::resolve(&document.address, url).is_some())
}

/// Имя файла из заголовка статьи. Кириллицу оставляем: файловые системы
/// её держат, а транслитерация делает имя нечитаемым.
fn slug(title: &str) -> String {
    let mut out = String::with_capacity(title.len());
    for ch in title.chars() {
        if ch.is_alphanumeric() {
            out.push(ch);
        } else if !out.ends_with(' ') {
            out.push(' ');
        }
    }
    let out = out.trim().replace(' ', "-");
    let out: String = out.chars().take(80).collect();

    if out.is_empty() {
        "article".to_owned()
    } else {
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_name_comes_from_the_title() {
        assert_eq!(
            slug("Week of 29 August – 4 September 2026"),
            "Week-of-29-August-4-September-2026"
        );
        assert_eq!(slug("Подводные камни: часть 2"), "Подводные-камни-часть-2");
        assert_eq!(slug("***"), "article");
    }

    #[test]
    fn an_image_file_keeps_its_own_name() {
        assert_eq!(
            file_name("https://e.com/a/chart.svg?v=2", "image/svg+xml"),
            "chart.svg"
        );
        assert_eq!(
            file_name("https://e.com/img/12345", "image/png"),
            "12345.png"
        );
        assert_eq!(file_name("https://e.com/", "image/jpeg"), "image.jpg");
    }

    #[test]
    fn a_local_article_lands_in_the_archive_with_its_pictures() {
        use std::io::Read;

        let dir = std::env::temp_dir().join(format!("brevier-save-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("img")).unwrap();
        std::fs::write(dir.join("img/chart.png"), b"not really a png, but bytes").unwrap();

        let document = Document {
            address: crate::Address::File(dir.join("article.md")),
            title: "Как это работает".to_owned(),
            markdown: "# Как это работает\n\n![схема](img/chart.png)\n".to_owned(),
            kind: crate::Kind::Article,
        };
        assert_eq!(suggested_name(&document), "Как-это-работает.zip");

        let path = dir.join("out.zip");
        let saved = write(&path, &document, UserAgent::Honest).unwrap();
        assert_eq!((saved.images, saved.missed), (1, 0));

        let mut archive = zip::ZipArchive::new(std::fs::File::open(&path).unwrap()).unwrap();
        let names: Vec<String> = archive.file_names().map(str::to_owned).collect();
        assert!(names.contains(&"out.md".to_owned()), "{names:?}");
        assert!(
            names.contains(&"images/01-chart.png".to_owned()),
            "{names:?}"
        );

        let mut text = String::new();
        archive
            .by_name("out.md")
            .unwrap()
            .read_to_string(&mut text)
            .unwrap();
        // Ссылка в сохранённом тексте ведёт внутрь архива, а не в интернет.
        assert!(text.contains("](images/01-chart.png)"), "{text}");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn without_pictures_it_is_just_a_file() {
        let document = Document {
            address: crate::Address::Web("https://e.com/a".to_owned()),
            title: "Plain".to_owned(),
            markdown: "# Plain\n\nтекст\n".to_owned(),
            kind: crate::Kind::Article,
        };
        assert_eq!(suggested_name(&document), "Plain.md");

        let path = std::env::temp_dir().join(format!("brevier-{}.md", std::process::id()));
        let saved = write(&path, &document, UserAgent::Honest).unwrap();
        assert_eq!(saved.images, 0);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), document.markdown);
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn links_in_the_text_point_inside_the_archive() {
        let md = "текст ![схема](https://e.com/a.png) и ![он же](https://e.com/a.png \"подпись\")";
        let out = relink(md, "https://e.com/a.png", "images/01-a.png");
        assert!(!out.contains("https://e.com/a.png"), "{out}");
        assert_eq!(out.matches("images/01-a.png").count(), 2);
    }
}
