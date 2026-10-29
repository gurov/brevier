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
use crate::notes::{self, Note, Notes};
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

/// Картинка заметки, что едет с файлом (#42): ссылка на неё в тексте
/// (`images/<имя>`) и сам файл на диске. Картинки статьи `save` достаёт
/// сам, а эти — у читателя, и из сети их не скачать.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attached {
    pub link: String,
    pub path: PathBuf,
}

impl Attached {
    /// Картинки этих заметок, по разу каждая.
    pub fn of(notes: &[&Note], store: &Notes) -> Vec<Attached> {
        let mut out: Vec<Attached> = Vec::new();
        for name in notes.iter().flat_map(|note| &note.pictures) {
            let link = format!("{}/{name}", notes::EXPORTED);
            if out.iter().any(|known| known.link == link) {
                continue;
            }
            if let Some(path) = store.picture(name) {
                out.push(Attached { link, path });
            }
        }
        out
    }
}

/// Сохранить статью по указанному пути.
///
/// Что писать, решает расширение: путь `…zip` — архив с картинками,
/// любой другой — один файл markdown. Читатель мог переименовать файл
/// в диалоге, и его выбор старше нашего предложения.
pub fn write(path: &Path, document: &Document, ua: UserAgent) -> Result<Saved, Error> {
    write_with(path, document, &[], ua)
}

/// То же, со своими картинками заметок (`notes::appended` уже сослался
/// на них в тексте). В `.zip` они ложатся рядом с картинками статьи;
/// рядом с `.md` — в папку `images/`: иначе их ссылки вели бы в никуда.
pub fn write_with(
    path: &Path,
    document: &Document,
    attached: &[Attached],
    ua: UserAgent,
) -> Result<Saved, Error> {
    if !is_zip(path) {
        std::fs::write(path, document.markdown.as_bytes()).map_err(Error::Convert)?;
        let images = beside(path, attached)?;
        return Ok(Saved {
            path: path.to_path_buf(),
            images,
            missed: 0,
        });
    }

    let (markdown, mut images, missed) = gather(document, attached, ua);
    images.extend(read_attached(attached)?);
    let stem = path
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_else(|| slug(&document.title));
    let files = [(format!("{stem}.md"), markdown.into_bytes())];
    pack(path, &files, &images)?;

    Ok(Saved {
        path: path.to_path_buf(),
        images: images.len(),
        missed,
    })
}

fn is_zip(path: &Path) -> bool {
    path.extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("zip"))
}

/// Архив: файлы markdown, затем картинки.
fn pack(
    path: &Path,
    files: &[(String, Vec<u8>)],
    images: &[(String, Vec<u8>)],
) -> Result<(), Error> {
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

    for (name, bytes) in files.iter().chain(images) {
        put(&mut archive, name, bytes)?;
    }
    archive
        .finish()
        .map_err(|e| Error::Convert(std::io::Error::other(e)))?;
    Ok(())
}

/// Картинки заметок с диска — в архив под их ссылками.
fn read_attached(attached: &[Attached]) -> Result<Vec<(String, Vec<u8>)>, Error> {
    attached
        .iter()
        .map(|picture| {
            let bytes = std::fs::read(&picture.path).map_err(Error::Convert)?;
            Ok((picture.link.clone(), bytes))
        })
        .collect()
}

/// Картинки заметок — рядом с файлом `.md`, по их ссылкам. Имя у картинки —
/// по содержимому, и такое же имя рядом — та же картинка: не переписываем.
fn beside(path: &Path, attached: &[Attached]) -> Result<usize, Error> {
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    for picture in attached {
        let target = dir.join(&picture.link);
        if target.is_file() {
            continue;
        }
        if let Some(folder) = target.parent() {
            std::fs::create_dir_all(folder).map_err(Error::Convert)?;
        }
        std::fs::copy(&picture.path, &target).map_err(Error::Convert)?;
    }
    Ok(attached.len())
}

/// Как назвать выгрузку заметок: одна страница — по её заголовку,
/// `.zip`, если есть картинки; несколько — архив, по файлу на страницу.
pub fn notes_name(pages: &[Vec<&Note>]) -> String {
    let pictures = pages.iter().flatten().any(|note| !note.pictures.is_empty());
    match pages {
        [one] => {
            let title = one
                .iter()
                .max_by_key(|note| note.id)
                .map(|note| note.title.as_str())
                .unwrap_or_default();
            let stem = format!("{} notes", if title.is_empty() { "Page" } else { title });
            format!("{}.{}", slug(&stem), if pictures { "zip" } else { "md" })
        }
        _ => "Brevier-notes.zip".to_owned(),
    }
}

/// Выгрузить заметки (#42), каждая страница — `notes::digest`. `.zip` —
/// по файлу markdown на страницу и картинки в `images/`: распакованный
/// в хранилище Obsidian, он читается как есть. Иначе — один файл markdown,
/// страницы в нём подряд, а картинки — в папку `images/` рядом.
pub fn notes(path: &Path, pages: &[Vec<&Note>], store: &Notes) -> Result<Saved, Error> {
    let all: Vec<&Note> = pages.iter().flatten().copied().collect();
    let attached = Attached::of(&all, store);
    if !is_zip(path) {
        let text = match pages {
            [one] => notes::digest(one),
            _ => notes::digest_pages(pages),
        };
        std::fs::write(path, text).map_err(Error::Convert)?;
        let images = beside(path, &attached)?;
        return Ok(Saved {
            path: path.to_path_buf(),
            images,
            missed: 0,
        });
    }
    let mut files: Vec<(String, Vec<u8>)> = Vec::new();
    for page in pages {
        let title = page
            .iter()
            .max_by_key(|note| note.id)
            .map(|note| note.title.as_str())
            .unwrap_or_default();
        let stem = slug(if title.is_empty() { "Page" } else { title });
        let mut name = format!("{stem}.md");
        let mut number = 1;
        while files.iter().any(|(known, _)| known == &name) {
            number += 1;
            name = format!("{stem}-{number}.md");
        }
        files.push((name, notes::digest(page).into_bytes()));
    }
    let images = read_attached(&attached)?;
    pack(path, &files, &images)?;
    Ok(Saved {
        path: path.to_path_buf(),
        images: images.len(),
        missed: 0,
    })
}

/// Скачать картинки статьи и переписать ссылки на них. Картинки заметок
/// (`attached`) не трогаем: их кладут с диска.
///
/// Что не отдалось — не беда: ссылка остаётся сетевой, а статья сохраняется
/// всё равно. Потерять текст из-за одной битой картинки было бы обидно.
fn gather(
    document: &Document,
    attached: &[Attached],
    ua: UserAgent,
) -> (String, Vec<(String, Vec<u8>)>, usize) {
    let mut markdown = document.markdown.clone();
    let mut images = Vec::new();
    let mut missed = 0;

    for (number, url) in markdown::images(&document.markdown).iter().enumerate() {
        if attached.iter().any(|picture| &picture.link == url) {
            continue;
        }
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

/// Байты картинки, которую читатель сохраняет себе (#38): из памяти
/// вкладки, если она там (`held`), из недельной копии, иначе из сети.
/// Картинка на экране уже скачана, и второй раз за ней обычно не ходим.
pub fn picture(
    source: &Source,
    held: Option<std::sync::Arc<Vec<u8>>>,
    ua: UserAgent,
) -> Result<Vec<u8>, Error> {
    if let Some(bytes) = held {
        return Ok(bytes.as_ref().clone());
    }
    if let Source::Web(url) = source
        && let Some(bytes) = crate::cache::Cache::open().image(url)
    {
        return Ok(bytes);
    }
    let (bytes, _) = media::grab(source, ua)?;
    if bytes.is_empty() {
        return Err(Error::Media("the server sent nothing".to_owned()));
    }
    Ok(bytes)
}

/// Как назвать сохранённую картинку (#38): имя — из адреса, расширение —
/// по самим байтам. CDN отдаёт webp по адресу `….jpg`, а галерея, поверив
/// имени, такой файл не откроет.
pub fn picture_name(source: &Source, bytes: &[u8]) -> String {
    let tail = match source {
        Source::Web(url) => {
            let path = url.split(['?', '#']).next().unwrap_or(url);
            crate::address::percent_decoded(path.rsplit('/').next().unwrap_or_default())
        }
        Source::File(path) => path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default(),
    };
    // Своё расширение — короткое слово после последней точки: `photo.php`,
    // `chart.jpg`. «v2.0-final» расширения не имеет, и имя остаётся целым.
    let stem = match tail.rsplit_once('.') {
        Some((stem, extension))
            if !stem.is_empty()
                && (1..=5).contains(&extension.len())
                && extension.chars().all(|c| c.is_ascii_alphanumeric()) =>
        {
            stem
        }
        _ => tail.as_str(),
    };
    let mut name = String::with_capacity(stem.len());
    for ch in stem.chars() {
        if ch.is_alphanumeric() || matches!(ch, '-' | '_') {
            name.push(ch);
        } else if !name.ends_with('-') {
            name.push('-');
        }
    }
    let name: String = name.trim_matches('-').chars().take(80).collect();
    let name = if name.is_empty() {
        "image".to_owned()
    } else if device(&name) {
        format!("{name}-image")
    } else {
        name
    };
    match picture_type(bytes) {
        Some((extension, _)) => format!("{name}.{extension}"),
        None => name,
    }
}

/// Тип картинки по подписи в первых байтах: расширение и MIME. `None` —
/// не узнали; сохранить можно и так, только без расширения.
pub fn picture_type(bytes: &[u8]) -> Option<(&'static str, &'static str)> {
    let starts = |magic: &[u8]| bytes.starts_with(magic);
    if starts(b"\x89PNG\r\n\x1a\n") {
        Some(("png", "image/png"))
    } else if starts(b"\xff\xd8\xff") {
        Some(("jpg", "image/jpeg"))
    } else if starts(b"GIF87a") || starts(b"GIF89a") {
        Some(("gif", "image/gif"))
    } else if starts(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") {
        Some(("webp", "image/webp"))
    } else if bytes.get(4..8) == Some(b"ftyp")
        && matches!(bytes.get(8..12), Some(b"avif") | Some(b"avis"))
    {
        Some(("avif", "image/avif"))
    } else if media::is_svg(bytes, None) {
        Some(("svg", "image/svg+xml"))
    } else {
        None
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
    } else if device(&out) {
        format!("{out}-article")
    } else {
        out
    }
}

/// Имя устройства Windows: `CON.md`, `aux.zip` там не файл, а консоль
/// и порт — в любом регистре и с любым расширением. Статья с заголовком
/// «Con» на Linux сохранилась бы, но с флешки на Windows уже не открылась бы.
fn device(name: &str) -> bool {
    let upper = name.to_ascii_uppercase();
    match upper.as_str() {
        "CON" | "PRN" | "AUX" | "NUL" => true,
        _ => {
            (upper.starts_with("COM") || upper.starts_with("LPT"))
                && upper.len() == 4
                && upper.ends_with(|c: char| ('1'..='9').contains(&c))
        }
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
    fn a_windows_device_name_is_not_a_file_name() {
        assert_eq!(slug("Con"), "Con-article");
        assert_eq!(slug("nul"), "nul-article");
        assert_eq!(slug("COM1"), "COM1-article");
        assert_eq!(slug("Aux: a review"), "Aux-a-review");
        assert_eq!(slug("COM10"), "COM10");
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
    fn a_saved_picture_takes_its_type_from_the_bytes() {
        let png = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR";
        let webp = b"RIFF\x10\0\0\0WEBPVP8 ";
        let web = |url: &str| Source::Web(url.to_owned());
        // CDN отдаёт webp по адресу `.jpg`: расширение — по байтам.
        assert_eq!(
            picture_name(&web("https://e.com/a/photo.jpg?w=800"), webp),
            "photo.webp"
        );
        assert_eq!(
            picture_name(&web("https://e.com/img/12345"), png),
            "12345.png"
        );
        assert_eq!(
            picture_name(&web("https://e.com/render.php?id=7"), png),
            "render.png"
        );
        assert_eq!(
            picture_name(&web("https://e.com/v2.0-final"), png),
            "v2-0-final.png"
        );
        assert_eq!(picture_name(&web("https://e.com/"), png), "image.png");
        assert_eq!(
            picture_name(&web("https://e.com/con.png"), png),
            "con-image.png"
        );
        assert_eq!(
            picture_name(
                &web("https://e.com/%D1%81%D1%85%D0%B5%D0%BC%D0%B0%201.png"),
                png
            ),
            "схема-1.png"
        );
        assert_eq!(
            picture_name(
                &Source::File("/home/r/notes/chart.svg".into()),
                b"<svg xmlns=\"\"/>"
            ),
            "chart.svg"
        );
        // Не узнали — имя без расширения, но не пустое.
        assert_eq!(picture_name(&web("https://e.com/x.bin"), b"what"), "x");
        assert_eq!(
            picture_type(b"\xff\xd8\xff\xe0"),
            Some(("jpg", "image/jpeg"))
        );
        assert_eq!(
            picture_type(b"\0\0\0\x1cftypavif"),
            Some(("avif", "image/avif"))
        );
    }

    #[test]
    fn a_held_picture_is_not_fetched_again() {
        // Адрес никуда не ведёт: ответ пришёл бы ошибкой сети.
        let source = Source::Web("http://127.0.0.1:9/never.png".to_owned());
        let held = std::sync::Arc::new(b"bytes".to_vec());
        assert_eq!(
            picture(&source, Some(held), UserAgent::Honest).unwrap(),
            b"bytes"
        );
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
            served: false,
            site: Vec::new(),
            feeds: Vec::new(),
            lang: None,
            next: None,
            archived: None,
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
            served: false,
            site: Vec::new(),
            feeds: Vec::new(),
            lang: None,
            next: None,
            archived: None,
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

    fn noted(dir: &Path) -> (Notes, u64, u64) {
        let mut store = Notes::at(dir.join("data/notes.tsv"));
        let quote = |text: &str| notes::select(text, 0, text.chars().count()).unwrap();
        let stamp = crate::store::Stamp::of_unix(1_760_000_000, 0);
        let one = store.add("https://e.com/a", "Lectures", quote("first"), "", stamp);
        let two = store.add("https://e.org/b", "Lectures", quote("second"), "why", stamp);
        let dot = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR\0\0\0\x01\0\0\0\x01\x08\x06\0\0\0";
        store.attach(one, dot).unwrap();
        (store, one, two)
    }

    #[test]
    fn notes_go_out_as_markdown_with_their_pictures_beside() {
        let dir = crate::scratch::Scratch::new("save-notes");
        let (store, one, _) = noted(&dir);
        let page = vec![store.get(one).unwrap()];
        assert_eq!(
            notes_name(std::slice::from_ref(&page)),
            "Lectures-notes.zip"
        );

        let out = dir.join("vault/Lectures.md");
        std::fs::create_dir_all(out.parent().unwrap()).unwrap();
        let saved = notes(&out, &[page], &store).unwrap();
        assert_eq!(saved.images, 1);
        let text = std::fs::read_to_string(&out).unwrap();
        let name = &store.get(one).unwrap().pictures[0];
        assert!(text.starts_with("---\ntitle: \"Lectures\"\n"), "{text}");
        assert!(text.contains(&format!("![](images/{name})")), "{text}");
        assert!(dir.join("vault/images").join(name).is_file());
    }

    #[test]
    fn all_notes_go_out_as_a_zip_with_a_file_for_each_page() {
        let dir = crate::scratch::Scratch::new("save-notes-zip");
        let (store, one, two) = noted(&dir);
        let pages = vec![vec![store.get(one).unwrap()], vec![store.get(two).unwrap()]];
        assert_eq!(notes_name(&pages), "Brevier-notes.zip");
        let out = dir.join("notes.zip");
        let saved = notes(&out, &pages, &store).unwrap();
        assert_eq!(saved.images, 1);
        let mut archive = zip::ZipArchive::new(std::fs::File::open(&out).unwrap()).unwrap();
        let mut names: Vec<String> = archive.file_names().map(str::to_owned).collect();
        names.sort();
        let picture = format!("images/{}", store.get(one).unwrap().pictures[0]);
        assert_eq!(
            names,
            vec![
                "Lectures-2.md".to_owned(),
                "Lectures.md".to_owned(),
                picture
            ]
        );
        let mut second = String::new();
        std::io::Read::read_to_string(&mut archive.by_name("Lectures-2.md").unwrap(), &mut second)
            .unwrap();
        assert!(
            second.contains(
                "> second\n>\n> — [on the page](https://e.org/b#:~:text=second)\n\nwhy\n"
            ),
            "{second}"
        );

        // Одним файлом — разделами, свойства только у файла с одной страницей.
        let out = dir.join("all.md");
        notes(&out, &pages, &store).unwrap();
        let text = std::fs::read_to_string(&out).unwrap();
        assert!(
            text.starts_with("# Your notes\n\n## [Lectures](https://e.com/a)\n"),
            "{text}"
        );
        assert!(!text.contains("---\ntitle"), "{text}");
    }

    #[test]
    fn a_saved_article_takes_the_pictures_of_its_notes_from_the_disk() {
        let dir = crate::scratch::Scratch::new("save-noted");
        let (store, one, _) = noted(&dir);
        let list = vec![store.get(one).unwrap()];
        let attached = Attached::of(&list, &store);
        let document = Document {
            address: crate::Address::Web("https://e.com/a".to_owned()),
            title: "Lectures".to_owned(),
            markdown: notes::appended("# Lectures\n\nfirst words\n", &list),
            kind: crate::Kind::Article,
            served: false,
            site: Vec::new(),
            feeds: Vec::new(),
            lang: None,
            next: None,
            archived: None,
        };
        let out = dir.join("a.zip");
        // Сети нет: картинка заметки берётся с диска, а не с e.com.
        let saved = write_with(&out, &document, &attached, UserAgent::Honest).unwrap();
        assert_eq!((saved.images, saved.missed), (1, 0));
        let archive = zip::ZipArchive::new(std::fs::File::open(&out).unwrap()).unwrap();
        assert!(archive.file_names().any(|name| name == attached[0].link));
    }
}
