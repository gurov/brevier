//! Картинки: скачать, декодировать, свести к пикселям.
//!
//! Живёт в ядре, а не в интерфейсе: декодирование и масштабирование от тулкита
//! не зависят, а окну остаётся завернуть готовые пиксели в текстуру. Декодеры
//! свои, а не системные — это записано в архитектуре: после отказа от JS
//! картинка остаётся единственной серьёзной поверхностью атаки, и разбирать
//! её должен memory-safe код. Отсюда же лимиты на размер: их выставляем сами,
//! а не полагаемся на умолчания.
//!
//! Растр разбирает `image`, вектор — `resvg`. Оба чистый Rust.

use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use resvg::tiny_skia;
use resvg::usvg;

use crate::address::Address;
use crate::error::Error;
use crate::fetch::{self, UserAgent};

/// Потолок на картинку. Читалка, а не качалка: фотография со страницы,
/// не влезающая в 8 МиБ, — это уже не иллюстрация.
pub const MAX_IMAGE: u64 = 8 * 1024 * 1024;

/// Потолки декодера. Заголовок картинки объявляет размер до того, как
/// придут пиксели, и «100000×100000» стоит гигабайты памяти — на этом
/// строится классическая decompression bomb. Верхняя граница памяти
/// и сторон ставится нами, а не декодером.
const MAX_SIDE: u32 = 16_384;
const MAX_ALLOC: u64 = 256 * 1024 * 1024;

/// Насколько разрешаем растянуть вектор. Схема шириной 300 точек на нашей
/// мере смотрится крупно, но растягивать её в четыре раза — уже не иллюстрация,
/// а плакат.
const MAX_SVG_SCALE: f32 = 2.0;

const ACCEPT: &str = "image/*";

/// Готовая к показу картинка: непрозрачный RGBA8 в нужной ширине.
///
/// Непрозрачный намеренно — см. [`flatten`].
pub struct Raster {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// Откуда брать картинку. Разрешается от адреса документа, а не от того,
/// что напечатано в теге: в родном `text/markdown` и в локальном файле
/// ссылки на картинки относительные, разворачивать их некому.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    Web(String),
    File(PathBuf),
}

impl Source {
    /// Как показать источник читателю — им же открывают картинку снаружи.
    pub fn display(&self) -> String {
        match self {
            Source::Web(url) => url.clone(),
            Source::File(path) => path.display().to_string(),
        }
    }
}

/// Куда ведёт `src` картинки в документе, открытом по адресу `base`.
///
/// `None` значит «сами не достанем»: `data:`, `blob:` и прочее, что не файл
/// и не http. Честный отказ лучше пустой картинки.
pub fn resolve(base: &Address, src: &str) -> Option<Source> {
    let src = src.trim();
    if src.is_empty() {
        return None;
    }
    if let Some(rest) = src.strip_prefix("file://") {
        return Some(Source::File(PathBuf::from(rest)));
    }
    if src.starts_with("http://") || src.starts_with("https://") {
        return Some(Source::Web(src.to_owned()));
    }
    // Схема есть, но не наша: data:, blob:, javascript:.
    if has_scheme(src) {
        return None;
    }

    match base {
        Address::Web(page) => url::Url::parse(page)
            .ok()?
            .join(src)
            .ok()
            .filter(|url| matches!(url.scheme(), "http" | "https"))
            .map(|url| Source::Web(url.to_string())),
        Address::Repo(repo) => url::Url::parse(&repo.web_url())
            .ok()?
            .join(src)
            .ok()
            .map(|url| Source::Web(url.to_string())),
        Address::File(path) => {
            let dir = path.parent().unwrap_or_else(|| Path::new("."));
            Some(Source::File(dir.join(src)))
        }
    }
}

fn has_scheme(src: &str) -> bool {
    match src.split_once(':') {
        Some((scheme, _)) => {
            !scheme.is_empty()
                && scheme
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
        }
        None => false,
    }
}

/// Достать и разобрать картинку, ужав её до ширины `width` в точках.
pub fn load(source: &Source, ua: UserAgent, width: u32) -> Result<Raster, Error> {
    let (bytes, mime) = match source {
        Source::Web(url) => {
            let blob = fetch::binary(url, ua, ACCEPT, MAX_IMAGE)?;
            (blob.bytes, Some(blob.mime))
        }
        Source::File(path) => (std::fs::read(path).map_err(Error::Convert)?, None),
    };
    decode(&bytes, mime.as_deref(), width)
}

/// Разобрать байты картинки. Тип берём из заголовка, но не верим ему
/// на слово: сервер ошибается, а подпись svg видна в самих байтах.
pub fn decode(bytes: &[u8], mime: Option<&str>, width: u32) -> Result<Raster, Error> {
    if bytes.is_empty() {
        return Err(Error::Media("the server sent nothing".to_owned()));
    }
    if is_svg(bytes, mime) {
        vector(bytes, width)
    } else {
        raster(bytes, width)
    }
}

fn is_svg(bytes: &[u8], mime: Option<&str>) -> bool {
    if mime.is_some_and(|mime| mime.contains("svg")) {
        return true;
    }
    let head = &bytes[..bytes.len().min(512)];
    let head = String::from_utf8_lossy(head);
    let head = head.trim_start();
    head.starts_with("<svg") || (head.starts_with("<?xml") && head.contains("<svg"))
}

fn raster(bytes: &[u8], width: u32) -> Result<Raster, Error> {
    let mut reader = image::ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| Error::Media(e.to_string()))?;
    reader.limits(limits());

    let source = reader.decode().map_err(|e| Error::Media(e.to_string()))?;
    let (w, h) = (source.width().max(1), source.height().max(1));

    // Увеличивать растр незачем: на мере он станет мылом. Уменьшаем
    // качественным фильтром — картинка в статье одна-две, время терпит.
    let source = if w > width {
        let height = ((u64::from(h) * u64::from(width)) / u64::from(w)).max(1) as u32;
        source.resize_exact(width, height, image::imageops::FilterType::Lanczos3)
    } else {
        source
    };

    let rgba = source.to_rgba8();
    let (width, height) = (rgba.width(), rgba.height());
    Ok(Raster {
        width,
        height,
        rgba: flatten(rgba.into_raw()),
    })
}

fn limits() -> image::Limits {
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_SIDE);
    limits.max_image_height = Some(MAX_SIDE);
    limits.max_alloc = Some(MAX_ALLOC);
    limits
}

/// Вектор рисуем сразу в нужном размере: это его преимущество перед растром,
/// и терять его на масштабировании готовых пикселей глупо.
///
/// Тёмную тему схемы не увидят: `@media (prefers-color-scheme: dark)` внутри
/// svg resvg не разбирает. Поэтому вектор кладём на белое, как и всё остальное.
fn vector(bytes: &[u8], width: u32) -> Result<Raster, Error> {
    let options = usvg::Options {
        fontdb: fonts(),
        ..Default::default()
    };
    let tree = usvg::Tree::from_data(bytes, &options).map_err(|e| Error::Media(e.to_string()))?;

    let size = tree.size();
    if size.width() < 1.0 || size.height() < 1.0 {
        return Err(Error::Media("the image has no size".to_owned()));
    }
    let scale = (width as f32 / size.width()).min(MAX_SVG_SCALE);
    let w = ((size.width() * scale).round() as u32).clamp(1, MAX_SIDE);
    let h = ((size.height() * scale).round() as u32).clamp(1, MAX_SIDE);

    let mut pixmap =
        tiny_skia::Pixmap::new(w, h).ok_or_else(|| Error::Media("no room for the canvas".to_owned()))?;
    // Белым — до отрисовки: дальше по всему холсту альфа единица, и премножение
    // tiny-skia совпадает с обычным RGBA. Иначе пришлось бы делить обратно.
    pixmap.fill(tiny_skia::Color::WHITE);
    resvg::render(
        &tree,
        tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );

    Ok(Raster {
        width: w,
        height: h,
        rgba: pixmap.take(),
    })
}

/// Шрифты для текста внутри svg. Системные: свои две гарнитуры комплекта
/// схему не нарисуют — там встречается всё, от Helvetica до иероглифов.
/// Читается база один раз: обход шрифтовых каталогов стоит десятки
/// миллисекунд, а картинок на странице бывает десяток.
fn fonts() -> Arc<usvg::fontdb::Database> {
    static FONTS: OnceLock<Arc<usvg::fontdb::Database>> = OnceLock::new();
    FONTS
        .get_or_init(|| {
            let mut db = usvg::fontdb::Database::new();
            db.load_system_fonts();
            Arc::new(db)
        })
        .clone()
}

/// Прозрачное кладём на белое.
///
/// Схемы и логотипы верстают с прозрачным фоном и чёрными линиями: в браузере
/// под ними белая страница. На тёмной теме такая картинка превращается
/// в чёрное на чёрном — то есть исчезает. Белая подложка — то же, что делает
/// браузер, только явно.
fn flatten(mut rgba: Vec<u8>) -> Vec<u8> {
    for pixel in rgba.chunks_exact_mut(4) {
        let alpha = u32::from(pixel[3]);
        if alpha == 255 {
            continue;
        }
        for channel in 0..3 {
            let value = u32::from(pixel[channel]);
            pixel[channel] = ((value * alpha + 255 * (255 - alpha)) / 255) as u8;
        }
        pixel[3] = 255;
    }
    rgba
}

#[cfg(test)]
mod tests {
    use super::*;

    fn web(url: &str) -> Address {
        Address::Web(url.to_owned())
    }

    #[test]
    fn relative_source_resolves_against_the_page() {
        assert_eq!(
            resolve(&web("https://e.com/posts/one/"), "chart.svg"),
            Some(Source::Web("https://e.com/posts/one/chart.svg".to_owned()))
        );
        assert_eq!(
            resolve(&web("https://e.com/posts/one/"), "/img/a.png"),
            Some(Source::Web("https://e.com/img/a.png".to_owned()))
        );
    }

    #[test]
    fn a_local_document_looks_next_to_itself() {
        assert_eq!(
            resolve(&Address::File(PathBuf::from("/docs/guide/readme.md")), "img/a.png"),
            Some(Source::File(PathBuf::from("/docs/guide/img/a.png")))
        );
    }

    #[test]
    fn what_we_cannot_fetch_is_refused_outright() {
        assert_eq!(resolve(&web("https://e.com/a"), "data:image/png;base64,AAA"), None);
        assert_eq!(resolve(&web("https://e.com/a"), ""), None);
    }

    #[test]
    fn svg_is_recognised_by_its_bytes_not_only_by_the_header() {
        let svg = br#"<?xml version="1.0"?><svg xmlns="http://www.w3.org/2000/svg"></svg>"#;
        assert!(is_svg(svg, None));
        assert!(is_svg(b"<svg viewBox='0 0 1 1'></svg>", Some("text/plain")));
        assert!(!is_svg(b"\x89PNG\r\n\x1a\n", Some("image/png")));
    }

    #[test]
    fn transparency_ends_up_on_white() {
        // Чёрный, полностью прозрачный, — на белом становится белым.
        assert_eq!(flatten(vec![0, 0, 0, 0]), vec![255, 255, 255, 255]);
        // Непрозрачное не трогаем.
        assert_eq!(flatten(vec![10, 20, 30, 255]), vec![10, 20, 30, 255]);
    }

    #[test]
    fn a_vector_is_drawn_at_the_asked_width() {
        let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="50"
                       viewBox="0 0 100 50"><rect width="100" height="50" fill="#333"/></svg>"##;
        let raster = decode(svg, Some("image/svg+xml"), 200).unwrap();
        // Растягиваем не более чем вдвое.
        assert_eq!((raster.width, raster.height), (200, 100));
        assert_eq!(raster.rgba.len() as u32, raster.width * raster.height * 4);
    }
}
