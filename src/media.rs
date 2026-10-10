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

/// Как вписывать картинку в страницу.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fit {
    /// Иллюстрация: занимает колонку. Мелкую векторную растягиваем — на то
    /// она и векторная.
    Column,
    /// Своя величина: формула в строке текста должна быть ростом с текст,
    /// а не с колонку. Уменьшаем, если не влезает, но не увеличиваем.
    Natural,
}

/// Во что вписываем картинку. Собрано в одну структуру, потому что все
/// числа приходят из типографики окна и меняются вместе.
#[derive(Debug, Clone, Copy)]
pub struct Look {
    /// Ширина колонки в точках.
    pub width: u32,
    /// Цвет бумаги под прозрачным; см. [`flatten`].
    pub paper: [u8; 3],
    /// Кегль текста в точках. В svg размеры бывают в `em` и `ex` — MathJax
    /// именно так и печатает формулы, — и считаться они обязаны от текста,
    /// рядом с которым картинка стоит.
    pub font_size: f32,
    pub fit: Fit,
    /// Сколько пикселей экрана на точку окна (`scale_factor`, #15). Ширина
    /// и кегль — в точках, а разбираем картинку в пикселях экрана: на экране
    /// 2× разобранная в точках выходит мылом. Телефон считает в пикселях
    /// экрана сразу — у него единица.
    pub density: f32,
}

/// Готовая к показу картинка: непрозрачный RGBA8 в нужной ширине.
///
/// Непрозрачный намеренно — см. [`flatten`].
pub struct Raster {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    /// Сколько пикселей растра на точку окна: показывать его надо размером
    /// `width / density`. Не всегда плотность экрана — мелкую картинку
    /// не растягиваем, и её пиксель остаётся точкой.
    pub density: f32,
    /// Сколько нижних рядов — под базовой линией строки (#14). У формулы
    /// это объявляет MathJax; у остального ноль.
    pub depth: u32,
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
    // Файл компьютера — только из документа с диска. Страница из сети
    // на диск не ходит: на Windows `file:////хост/…` — это чужой сетевой
    // диск, и одного показа страницы хватило бы, чтобы Windows отдала ему
    // хеш пароля.
    if let Some(rest) = src.strip_prefix("file:")
        && rest.starts_with("//")
    {
        return match base {
            Address::File(_) => Some(Source::File(crate::address::file_path(src, rest))),
            _ => None,
        };
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
        // Страницу программы пишем мы сами, и относительных картинок
        // в ней нет — разворачивать нечего.
        Address::Internal(_) => None,
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

/// Достать и разобрать картинку.
pub fn load(source: &Source, ua: UserAgent, look: Look) -> Result<Raster, Error> {
    let (bytes, mime) = grab(source, ua)?;
    decode(&bytes, mime.as_deref(), look)
}

/// Достать сырые байты картинки и объявленный тип, не разбирая их.
///
/// Вынесено из [`load`] ради окна: оно кладёт сырые байты в кэш вкладки,
/// чтобы «назад» декодировал картинку из памяти, а не тянул её из сети
/// заново. Тип отдаём для верного разбора, но на слово ему не верим —
/// [`decode`] всё равно смотрит в сами байты.
pub fn grab(source: &Source, ua: UserAgent) -> Result<(Vec<u8>, Option<String>), Error> {
    match source {
        Source::Web(url) => {
            let blob = fetch::binary(url, ua, ACCEPT, MAX_IMAGE)?;
            Ok((blob.bytes, Some(blob.mime)))
        }
        Source::File(path) => Ok((std::fs::read(path).map_err(Error::Convert)?, None)),
    }
}

/// Разобрать байты картинки. Тип берём из заголовка, но не верим ему
/// на слово: сервер ошибается, а подпись svg видна в самих байтах.
pub fn decode(bytes: &[u8], mime: Option<&str>, look: Look) -> Result<Raster, Error> {
    if bytes.is_empty() {
        return Err(Error::Media("the server sent nothing".to_owned()));
    }
    if is_svg(bytes, mime) {
        vector(bytes, look)
    } else {
        raster(bytes, look)
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

fn raster(bytes: &[u8], look: Look) -> Result<Raster, Error> {
    let width = look.width;
    let mut reader = image::ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| Error::Media(e.to_string()))?;
    reader.limits(limits());

    let source = reader.decode().map_err(|e| Error::Media(e.to_string()))?;
    let (w, h) = (source.width().max(1), source.height().max(1));

    // В колонке картинка не шире меры и не крупнее своей натуры — как её
    // показал бы и браузер. На плотном экране под ту же величину нужно больше
    // пикселей: берём сколько есть в файле, но не больше, чем покажет экран.
    let shown = w.min(width);
    let width = ((shown as f32 * look.density).round() as u32).clamp(1, w);
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
        rgba: flatten(rgba.into_raw(), look.paper),
        density: width as f32 / shown as f32,
        depth: 0,
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
fn vector(bytes: &[u8], look: Look) -> Result<Raster, Error> {
    let options = usvg::Options {
        fontdb: fonts(),
        // Кегль страницы: от него считаются `em` и `ex`, а формулы MathJax
        // размечены именно ими.
        font_size: look.font_size,
        ..Default::default()
    };
    let tree = usvg::Tree::from_data(bytes, &options).map_err(|e| Error::Media(e.to_string()))?;

    let size = tree.size();
    if size.width() < 1.0 || size.height() < 1.0 {
        return Err(Error::Media("the image has no size".to_owned()));
    }
    let room = look.width as f32 / size.width();
    let scale = match look.fit {
        Fit::Column => room.min(MAX_SVG_SCALE),
        // Формула уже нужного роста: трогаем, только если не влезает.
        Fit::Natural => room.min(1.0),
    };
    // Величина — в точках окна, рисуем — в пикселях экрана: вектор от этого
    // только выигрывает.
    let scale = scale * look.density;
    let w = ((size.width() * scale).round() as u32).clamp(1, MAX_SIDE);
    let h = ((size.height() * scale).round() as u32).clamp(1, MAX_SIDE);

    let mut pixmap = tiny_skia::Pixmap::new(w, h)
        .ok_or_else(|| Error::Media("no room for the canvas".to_owned()))?;
    // Бумагой — до отрисовки: дальше по всему холсту альфа единица, и премножение
    // tiny-skia совпадает с обычным RGBA. Иначе пришлось бы делить обратно.
    pixmap.fill(tiny_skia::Color::from_rgba8(
        look.paper[0],
        look.paper[1],
        look.paper[2],
        255,
    ));
    resvg::render(
        &tree,
        tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );

    let raster = Raster {
        width: w,
        height: h,
        rgba: pixmap.take(),
        density: look.density,
        depth: 0,
    };
    Ok(match look.fit {
        Fit::Natural => trim(
            Raster {
                depth: (sink(bytes, look.font_size) * scale).round() as u32,
                ..raster
            },
            look.paper,
        ),
        Fit::Column => raster,
    })
}

/// Насколько формула уходит под базовую линию, в точках окна (#14).
///
/// MathJax пишет это на корне svg: `style="vertical-align: -0.671ex"`.
/// Браузер опускает картинку на столько, а `GtkTextView` ставит холст нижним
/// краем на базовую линию — и индекс снизу, дробь, хвост у `p` повисали над
/// строкой. `ex` — половина кегля, как и у usvg, которым считается остальной
/// размер, иначе глубина разошлась бы с ростом. Подъём (значение больше нуля)
/// не берём: это уже не вынос, а картинка, поднятая над строкой.
fn sink(bytes: &[u8], font_size: f32) -> f32 {
    let head = String::from_utf8_lossy(&bytes[..bytes.len().min(4096)]);
    let Some(start) = head.find("<svg") else {
        return 0.0;
    };
    let tag = &head[start..];
    let tag = &tag[..tag.find('>').unwrap_or(tag.len())];
    let Some(value) = tag
        .split_once("vertical-align")
        .and_then(|(_, rest)| rest.trim_start().strip_prefix(':'))
        .map(str::trim_start)
    else {
        return 0.0;
    };
    let end = value
        .find(|c: char| !(c.is_ascii_digit() || matches!(c, '.' | '-' | '+')))
        .unwrap_or(value.len());
    let Ok(number) = value[..end].parse::<f32>() else {
        return 0.0;
    };
    let unit = &value[end..];
    let shift = if unit.starts_with("ex") {
        number * font_size / 2.0
    } else if unit.starts_with("em") {
        number * font_size
    } else {
        number
    };
    (-shift).max(0.0)
}

/// Срезать пустые поля формулы сверху и снизу.
///
/// MathJax печатает формулу с запасом под базовой линией и объявляет его
/// через `vertical-align`. Пустой низ — запас, где выноса у этой формулы
/// нет: срезаем его, и глубина (`depth`) убывает на столько же — рисунок
/// стоит на базовой линии там же, где стоял, а строку зря не раздвигает.
/// У формулы без объявленной глубины так она и садится на линию. Пустые
/// ряды сверху срезаны заодно, иначе формула раздувает межстрочный интервал.
///
/// Больше трети высоты не срезаем: пустая картинка должна остаться картинкой,
/// а не исчезнуть.
fn trim(raster: Raster, paper: [u8; 3]) -> Raster {
    let row = raster.width as usize * 4;
    let blank = |line: usize| {
        raster.rgba[line * row..(line + 1) * row]
            .as_chunks::<4>()
            .0
            .iter()
            .all(|pixel| pixel[..3] == paper[..])
    };

    let limit = (raster.height as usize) / 3;
    let mut top = 0;
    while top < limit && blank(top) {
        top += 1;
    }
    let mut bottom = raster.height as usize;
    while bottom > top + 1 && raster.height as usize - bottom < limit && blank(bottom - 1) {
        bottom -= 1;
    }
    if top == 0 && bottom == raster.height as usize {
        return raster;
    }

    let cut = raster.height - bottom as u32;
    Raster {
        width: raster.width,
        height: (bottom - top) as u32,
        rgba: raster.rgba[top * row..bottom * row].to_vec(),
        density: raster.density,
        depth: raster.depth.saturating_sub(cut),
    }
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
            // На Android `fontdb` системных шрифтов не ищет вовсе — у него нет
            // для этой платформы ветки, и текст в svg (подписи значков, схемы)
            // пропадал. Шрифты там лежат в одном месте, а родовые имена
            // называем те, что есть на любом Android.
            #[cfg(target_os = "android")]
            {
                db.load_fonts_dir("/system/fonts");
                db.set_sans_serif_family("Roboto");
                db.set_serif_family("Noto Serif");
                db.set_monospace_family("Droid Sans Mono");
            }
            Arc::new(db)
        })
        .clone()
}

/// Прозрачное кладём на бумагу.
///
/// Схемы и логотипы верстают с прозрачным фоном и чёрными линиями: в браузере
/// под ними светлая страница. На тёмной теме такая картинка превращается
/// в чёрное на чёрном — то есть исчезает. Подложка — то же, что делает браузер,
/// только явно и цветом нашей бумаги, а не белым: белая карточка посреди
/// слоновой кости заметна.
///
/// Цвет всегда светлый, даже когда читатель выбрал тёмную тему: схема
/// нарисована тёмным по светлому, и другого выхода у неё нет.
fn flatten(mut rgba: Vec<u8>, paper: [u8; 3]) -> Vec<u8> {
    for pixel in rgba.as_chunks_mut::<4>().0 {
        let alpha = u32::from(pixel[3]);
        if alpha == 255 {
            continue;
        }
        for channel in 0..3 {
            let value = u32::from(pixel[channel]);
            let under = u32::from(paper[channel]);
            pixel[channel] = ((value * alpha + under * (255 - alpha)) / 255) as u8;
        }
        pixel[3] = 255;
    }
    rgba
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Бумага, на которую в окне ложатся картинки.
    const PAPER: [u8; 3] = [250, 245, 234];

    fn look(width: u32, fit: Fit) -> Look {
        Look {
            width,
            paper: PAPER,
            font_size: 22.0,
            fit,
            density: 1.0,
        }
    }

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

    /// Страница из сети не читает картинками диск и чужие сетевые шары.
    #[test]
    fn only_a_local_document_reaches_files() {
        assert_eq!(resolve(&web("https://e.com/a"), "file:///etc/passwd"), None);
        assert_eq!(
            resolve(&web("https://e.com/a"), "file:////attacker/share/x.png"),
            None
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_local_document_names_files_by_url() {
        assert_eq!(
            resolve(
                &Address::File(PathBuf::from("/docs/readme.md")),
                "file:///docs/My%20Chart.png"
            ),
            Some(Source::File(PathBuf::from("/docs/My Chart.png")))
        );
    }

    #[cfg(windows)]
    #[test]
    fn a_local_document_names_files_by_url() {
        assert_eq!(
            resolve(
                &Address::File(PathBuf::from(r"C:\docs\readme.md")),
                "file:///C:/docs/My%20Chart.png"
            ),
            Some(Source::File(PathBuf::from(r"C:\docs\My Chart.png")))
        );
    }

    #[test]
    fn a_local_document_looks_next_to_itself() {
        assert_eq!(
            resolve(
                &Address::File(PathBuf::from("/docs/guide/readme.md")),
                "img/a.png"
            ),
            Some(Source::File(PathBuf::from("/docs/guide/img/a.png")))
        );
    }

    #[test]
    fn what_we_cannot_fetch_is_refused_outright() {
        assert_eq!(
            resolve(&web("https://e.com/a"), "data:image/png;base64,AAA"),
            None
        );
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
    fn a_formula_is_the_size_of_the_text_around_it() {
        // MathJax печатает формулы в `ex`: рост зависит от кегля страницы,
        // а не от колонки. Заодно проверяем, что вектор в своей величине
        // не растягивается.
        let svg = br#"<svg xmlns="http://www.w3.org/2000/svg" width="2ex" height="4ex"
                       viewBox="0 0 20 40"><rect width="20" height="40"/></svg>"#;
        let big = decode(svg, None, look(1000, Fit::Natural)).unwrap();
        let small = decode(
            svg,
            None,
            Look {
                font_size: 11.0,
                ..look(1000, Fit::Natural)
            },
        )
        .unwrap();

        assert!(
            big.height > small.height,
            "кегль не влияет на формулу: {} против {}",
            big.height,
            small.height
        );
        assert!(
            big.width < 100,
            "формулу растянуло до колонки: {}",
            big.width
        );
    }

    #[test]
    fn a_formula_sits_on_the_baseline() {
        // Внизу картинки пустая полоса — запас MathJax под базовой линией.
        // В своей величине она срезается, в колонке остаётся как есть.
        let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="30"
                        viewBox="0 0 10 30"><rect width="10" height="22" fill="#000"/></svg>"##;
        let natural = decode(svg, None, look(500, Fit::Natural)).unwrap();
        let column = decode(svg, None, look(10, Fit::Column)).unwrap();

        assert_eq!(natural.height, 22, "пустой низ не срезан");
        assert_eq!(column.height, 30, "в колонке резать нечего");
    }

    #[test]
    fn a_formula_knows_how_deep_it_goes() {
        // Кегль 20: `ex` — 10 точек. Формула ростом 4ex, из них 1ex под
        // базовой линией, и рисунок доходит до самого низа — вынос настоящий.
        let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="1ex" height="4ex"
                        style="vertical-align: -1ex;" viewBox="0 0 10 40">
                        <rect width="10" height="40" fill="#000"/></svg>"##;
        let raster = decode(svg, None, formula_look(1.0)).unwrap();
        assert_eq!((raster.height, raster.depth), (40, 10));
    }

    #[test]
    fn an_empty_reserve_under_the_baseline_comes_off_the_depth() {
        // Тот же запас в 1ex, но рисунок кончается на 5 точек выше низа:
        // пустые ряды срезаны, и глубина убыла ровно на них — рисунок стоит
        // на базовой линии там же, где стоял.
        let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="1ex" height="4ex"
                        style="vertical-align: -1ex;" viewBox="0 0 10 40">
                        <rect width="10" height="35" fill="#000"/></svg>"##;
        let raster = decode(svg, None, formula_look(1.0)).unwrap();
        assert_eq!((raster.height, raster.depth), (35, 5));
    }

    #[test]
    fn a_dense_screen_gets_a_formula_in_its_own_pixels() {
        let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="1ex" height="4ex"
                        style="vertical-align: -1ex;" viewBox="0 0 10 40">
                        <rect width="10" height="40" fill="#000"/></svg>"##;
        let raster = decode(svg, None, formula_look(2.0)).unwrap();
        assert_eq!((raster.width, raster.height, raster.depth), (20, 80, 20));
        assert_eq!(raster.density, 2.0);
    }

    #[test]
    fn a_picture_is_not_blown_up_for_a_dense_screen() {
        let png = |width, height| {
            let mut bytes = Vec::new();
            image::RgbaImage::new(width, height)
                .write_to(
                    &mut std::io::Cursor::new(&mut bytes),
                    image::ImageFormat::Png,
                )
                .unwrap();
            bytes
        };
        let dense = Look {
            density: 2.0,
            ..look(100, Fit::Column)
        };
        // Мельче колонки: пикселей больше, чем в файле, не взять — картинка
        // остаётся своей величины, точка в пиксель.
        let small = decode(&png(30, 20), None, dense).unwrap();
        assert_eq!((small.width, small.density), (30, 1.0));
        // Шире колонки: в колонку, но пикселями экрана, а не точками окна.
        let large = decode(&png(300, 200), None, dense).unwrap();
        assert_eq!((large.width, large.height, large.density), (200, 133, 2.0));
    }

    /// Формула в своей величине при кегле 20 — `ex` в 10 точек.
    fn formula_look(density: f32) -> Look {
        Look {
            font_size: 20.0,
            density,
            ..look(500, Fit::Natural)
        }
    }

    #[test]
    fn transparency_ends_up_on_the_paper() {
        // Чёрный, полностью прозрачный, — становится цветом бумаги.
        assert_eq!(
            flatten(vec![0, 0, 0, 0], PAPER),
            PAPER.iter().copied().chain([255]).collect::<Vec<u8>>()
        );
        // Непрозрачное не трогаем.
        assert_eq!(flatten(vec![10, 20, 30, 255], PAPER), vec![10, 20, 30, 255]);
    }

    #[test]
    fn a_vector_is_drawn_at_the_asked_width() {
        let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="50"
                       viewBox="0 0 100 50"><rect width="100" height="50" fill="#333"/></svg>"##;
        let raster = decode(svg, Some("image/svg+xml"), look(200, Fit::Column)).unwrap();
        // Растягиваем не более чем вдвое.
        assert_eq!((raster.width, raster.height), (200, 100));
        assert_eq!(raster.rgba.len() as u32, raster.width * raster.height * 4);
    }
}
