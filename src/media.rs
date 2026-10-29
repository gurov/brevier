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
        // Страница заметок показывает картинки читателя (#42) — из своей
        // папки и только по нашему имени файла.
        Address::Internal(crate::address::Internal::Notes) => {
            let name = src
                .strip_prefix(crate::notes::PICTURES)?
                .strip_prefix('/')?;
            crate::notes::is_picture_name(name).then_some(())?;
            Some(Source::File(
                crate::store::data_dir()?
                    .join(crate::notes::PICTURES)
                    .join(name),
            ))
        }
        // Прочие страницы программы пишем мы сами, и относительных картинок
        // в них нет — разворачивать нечего.
        Address::Internal(_) => None,
    }
}

/// Когда картинка для заметки «совсем большая» (#42): тяжелее `bytes`
/// и длиннее `side` по большей стороне. Такую сводим к `side`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Shrink {
    pub bytes: usize,
    pub side: u32,
}

impl Shrink {
    /// Телефон: снимок камеры (4000×3000, 3–6 МБ) — до 2560 по большей
    /// стороне, а снимок экрана (1080×2400) остаётся байт в байт. Копия
    /// приложения у Google — до 25 МБ на всё, с закладками и заметками.
    pub const PHONE: Shrink = Shrink {
        bytes: 2 * 1024 * 1024,
        side: 2560,
    };
    /// Окно: только то, что иначе не влезло бы в заметку вовсе.
    pub const WINDOW: Shrink = Shrink {
        bytes: crate::notes::MAX_PICTURE,
        side: 4096,
    };
}

/// Размер картинки, как она встанет (с поворотом из EXIF), — по заголовку,
/// не разбирая пикселей: окно ставит место под картинку заметки до того,
/// как она разобрана, и соседние заметки не прыгают.
pub fn size(bytes: &[u8]) -> Option<(u32, u32)> {
    use image::ImageDecoder;
    let mut reader = image::ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .ok()?;
    reader.limits(limits());
    let mut decoder = reader.into_decoder().ok()?;
    let (w, h) = decoder.dimensions();
    let turned = matches!(
        decoder.orientation().ok()?,
        image::metadata::Orientation::Rotate90
            | image::metadata::Orientation::Rotate270
            | image::metadata::Orientation::Rotate90FlipH
            | image::metadata::Orientation::Rotate270FlipH
    );
    Some(if turned { (h, w) } else { (w, h) })
}

/// Уменьшить картинку, если она совсем большая: `None` — оставить как есть.
/// Непрозрачная едет в JPEG, с прозрачностью — в PNG; поворот из EXIF
/// применяем сразу, потому что заново записанный файл его теряет.
pub fn shrink(bytes: &[u8], limit: Shrink) -> Result<Option<Vec<u8>>, Error> {
    use image::ImageDecoder;

    let over = |len: usize| len > limit.bytes || len > crate::notes::MAX_PICTURE;
    if !over(bytes.len()) {
        return Ok(None);
    }
    let fail = |e: image::ImageError| Error::Media(e.to_string());
    let mut reader = image::ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| Error::Media(e.to_string()))?;
    reader.limits(limits());
    let mut decoder = reader.into_decoder().map_err(fail)?;
    let (w, h) = decoder.dimensions();
    let longest = w.max(h);
    // Тяжёлая, но не длинная — снимок экрана в PNG: оставить, если влезает.
    if longest <= limit.side && bytes.len() <= crate::notes::MAX_PICTURE {
        return Ok(None);
    }
    let orientation = decoder.orientation().map_err(fail)?;
    let mut picture = image::DynamicImage::from_decoder(decoder).map_err(fail)?;
    if longest > limit.side {
        picture = picture.resize(
            limit.side,
            limit.side,
            image::imageops::FilterType::Lanczos3,
        );
    }
    picture.apply_orientation(orientation);
    let mut out = std::io::Cursor::new(Vec::new());
    let clear = picture.color().has_alpha() && picture.to_rgba8().pixels().any(|p| p[3] < 255);
    if clear {
        picture
            .write_to(&mut out, image::ImageFormat::Png)
            .map_err(fail)?;
    } else {
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 88)
            .encode_image(&picture.to_rgb8())
            .map_err(fail)?;
    }
    Ok(Some(out.into_inner()))
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

pub(crate) fn is_svg(bytes: &[u8], mime: Option<&str>) -> bool {
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

    // Поворот из EXIF — как у браузеров (`image-orientation: from-image`):
    // снимок с телефона иначе лежит на боку, а такие кладут в заметки (#42).
    let fail = |e: image::ImageError| Error::Media(e.to_string());
    let mut decoder = reader.into_decoder().map_err(fail)?;
    let orientation = image::ImageDecoder::orientation(&mut decoder).map_err(fail)?;
    let mut source = image::DynamicImage::from_decoder(decoder).map_err(fail)?;
    source.apply_orientation(orientation);
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
    // usvg строит дерево рекурсивно: на вложенных `<g>` кончился бы стек.
    if crate::nesting::xml_too_deep(&String::from_utf8_lossy(bytes)) {
        return Err(Error::Media("the image is nested too deeply".to_owned()));
    }
    let options = usvg::Options {
        fontdb: fonts(),
        // Кегль страницы: от него считаются `em` и `ex`, а формулы MathJax
        // размечены именно ими.
        font_size: look.font_size,
        // `<image href>` внутри схемы — только `data:`. Обычный обработчик
        // usvg читает любой путь с диска как есть: схема с чужой страницы
        // вставила бы в себя файл читателя, `/dev/zero` съел бы всю память,
        // а на Windows `\\хост\шара\…` — чужой сетевой диск, которому Windows
        // сама отдаёт хеш пароля, стоит странице открыться.
        image_href_resolver: usvg::ImageHrefResolver {
            resolve_data: usvg::ImageHrefResolver::default_data_resolver(),
            resolve_string: Box::new(|_, _| None),
        },
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

    /// Шум не сжимается: такая картинка весит, сколько в ней пикселей.
    fn noise(w: u32, h: u32) -> Vec<u8> {
        let mut seed: u32 = 7;
        let picture = image::RgbImage::from_fn(w, h, |_, _| {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let [_, r, g, b] = seed.to_le_bytes();
            image::Rgb([r, g, b])
        });
        let mut out = std::io::Cursor::new(Vec::new());
        picture.write_to(&mut out, image::ImageFormat::Png).unwrap();
        out.into_inner()
    }

    fn sides(bytes: &[u8]) -> (u32, u32) {
        let picture = image::load_from_memory(bytes).unwrap();
        (picture.width(), picture.height())
    }

    #[test]
    fn only_a_very_large_picture_is_made_smaller() {
        // Пороги поменьше настоящих: в отладочной сборке шум в мегапиксели
        // сжимается долго, а правило то же.
        let limit = Shrink {
            bytes: 100_000,
            side: 256,
        };
        let photo = noise(600, 400);
        assert!(photo.len() > limit.bytes);
        let smaller = shrink(&photo, limit).unwrap().unwrap();
        assert!(smaller.starts_with(b"\xff\xd8\xff"), "непрозрачная — JPEG");
        assert_eq!(sides(&smaller), (256, 171));
        // Тяжёлая, но не длинная — как снимок экрана: как есть.
        let screen = noise(200, 250);
        assert!(screen.len() > limit.bytes);
        assert_eq!(shrink(&screen, limit).unwrap(), None);
        // Лёгкая — как есть, какой бы длинной ни была.
        assert_eq!(shrink(&photo, Shrink::WINDOW).unwrap(), None);
        assert_eq!(shrink(b"not a picture at all", limit).unwrap(), None);
    }

    /// Снимок с телефона лежит в файле боком, а стоять велит EXIF:
    /// браузеры его слушают, и мы тоже.
    #[test]
    fn a_photo_stands_the_way_its_exif_says() {
        use image::ImageEncoder;
        let picture = image::RgbImage::from_pixel(4, 2, image::Rgb([200, 30, 30]));
        let mut bytes = Vec::new();
        let mut encoder = image::codecs::jpeg::JpegEncoder::new(&mut bytes);
        // TIFF с одним полем: Orientation = 6, повернуть на 90° по часовой.
        let exif = b"II\x2a\x00\x08\x00\x00\x00\x01\x00\x12\x01\x03\x00\x01\x00\x00\x00\
                     \x06\x00\x00\x00\x00\x00\x00\x00"
            .to_vec();
        encoder.set_exif_metadata(exif).unwrap();
        encoder
            .write_image(&picture, 4, 2, image::ExtendedColorType::Rgb8)
            .unwrap();
        let raster = decode(&bytes, None, look(100, Fit::Column)).unwrap();
        assert_eq!((raster.width, raster.height), (2, 4));
        assert_eq!(size(&bytes), Some((2, 4)));
        // И уменьшенная копия стоит так же, уже без EXIF.
        let limit = Shrink { bytes: 10, side: 2 };
        let smaller = shrink(&bytes, limit).unwrap().unwrap();
        assert_eq!(sides(&smaller), (1, 2));
    }

    #[test]
    fn the_notes_page_shows_only_the_pictures_of_notes() {
        let notes = Address::Internal(crate::address::Internal::Notes);
        let history = Address::Internal(crate::address::Internal::History);
        let own = "notes/0123456789abcdef.png";
        match resolve(&notes, own) {
            Some(Source::File(path)) => assert!(path.ends_with(own), "{path:?}"),
            // Без папки данных показывать нечего.
            None => assert!(crate::store::data_dir().is_none()),
            other => panic!("{other:?}"),
        }
        assert_eq!(resolve(&notes, "notes/../settings.tsv"), None);
        assert_eq!(resolve(&notes, "../notes/0123456789abcdef.png"), None);
        assert_eq!(resolve(&history, own), None);
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

    /// Схема из сети не достаёт картинкой файл с диска.
    #[test]
    fn a_vector_does_not_read_files() {
        let path = std::env::temp_dir().join(format!("brevier-red-{}.svg", std::process::id()));
        std::fs::write(
            &path,
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"><rect width="10" height="10" fill="#ff0000"/></svg>"##,
        )
        .unwrap();
        let svg = format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="10" height="10"><image xlink:href="{}" width="10" height="10"/></svg>"#,
            path.display()
        );
        let raster = decode(
            svg.as_bytes(),
            Some("image/svg+xml"),
            look(10, Fit::Natural),
        )
        .unwrap();
        std::fs::remove_file(&path).unwrap();
        assert!(raster.rgba.chunks(4).all(|pixel| pixel[0..3] == PAPER));
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
