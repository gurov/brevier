//! Печать (Ctrl+P): статья на бумаге набором читателя.
//!
//! `GtkTextView` печатать не умеет, поэтому лист набираем сами — Pango
//! по той же модели страницы (`page::Page`), что и экран, через контекст
//! печати GTK: диалог, принтер, «в файл PDF» — его. Вид берём у тех же
//! тегов, что рисуют статью: [`tags`](super::tags) заводит их в отдельном
//! буфере в масштабе бумаги, а [`Look::of`] читает их свойства — правка
//! вида статьи доходит до бумаги сама. Свойство тега, которого бумага
//! не понимает, ловит тест `paper_knows_every_tag_property`.
//!
//! Что от тулкита не зависит — сноски-адреса, приложение с заметками,
//! разбивка на листы, кегль бумаги, — в ядре (`brevier::paper`).
//!
//! Бумага всегда светлая и без фона: тёмная тема и тон бумаги — экранные.
//! Подложки, которые несут смысл (панель кода, отмеченное), остаются.

use std::collections::HashMap;
use std::ops::Range;
use std::path::PathBuf;
use std::sync::Arc;

use gtk::glib::translate::from_glib;
use gtk::prelude::*;
use gtk::{cairo, gdk, glib, pango};

use brevier::media::{self, Raster, Source};
use brevier::notes::{self, Note};
use brevier::outline::{INDENT, NOTEREF_RISE, NOTEREF_SIZE, RULE_INSET, RULE_WIDTH, RULE_X, Type};
use brevier::page::{self, Block, Page, Style};
use brevier::palette::{Paper, Theme, colors};
use brevier::paper::{self as sheet, Endnotes, Piece};
use brevier::store::Printing;
use brevier::{Address, Document, UserAgent};

/// Во сколько раз крупнее заводятся теги бумаги: `tags` округляет поля
/// и отступы до целых пикселей, а на бумаге кегль мельче экранного,
/// и округление съело бы до трети пикселя на каждом.
const FINE: f64 = 4.0;
/// Пунктов в пикселе CSS: размеры в `tags` — пиксели экрана в 96 точек
/// на дюйм, бумага меряется пунктами (72 на дюйм).
const PX: f64 = 0.75;
/// Поля листа, пункты: сверху и снизу — с колонтитулом внутри, по бокам —
/// только когда мера шире листа (на A5); обычно колонка стоит посередине.
const MARGIN: f64 = 15.0 / 25.4 * 72.0;
/// Колонтитул мельче текста.
const RUNNING_SIZE: f64 = 0.75;
/// Растр картинок на бумаге: пикселей на пиксель CSS. Снимки — 300 точек
/// на дюйм, формулы — 600: у формулы тонкие линии, а места она мало берёт.
const PHOTO_DENSITY: f32 = 300.0 / 96.0;
const FORMULA_DENSITY: f32 = 600.0 / 96.0;
/// Поля и промежутки таблицы — те же, что у сетки окна (`grid`, `.table`).
const TABLE_COLUMNS: f64 = 20.0;
const TABLE_ROWS: f64 = 7.0;
const TABLE_ABOVE: f64 = 10.0;
const TABLE_BELOW: f64 = 14.0;
/// Подпись к картинке — как `.caption` в окне.
const CAPTION_SIZE: f64 = 0.85;
const CAPTION_GAP: f64 = 4.0;
/// Текст ячеек таблицы мельче текста — как сноски.
const CELL_SIZE: f64 = 0.9;
/// Насколько можно ужать картинку, чтобы она встала в остаток листа,
/// а не уехала на следующий, оставив за собой пустоту.
const FIGURE_LEAST: f64 = 0.75;

/// Что напечатать: собрано со вкладки до диалога.
pub struct Job {
    pub document: Document,
    pub page: Page,
    /// Заметки этой страницы и их места в тексте страницы — в том же порядке.
    pub notes: Vec<Note>,
    pub places: Vec<Option<(usize, usize)>>,
    /// Выделенное в статье: его можно напечатать отдельно.
    pub selection: Option<(usize, usize)>,
    pub options: Printing,
    pub typography: Type,
    /// Высокий контраст читателя: бумага берёт его краску.
    pub high: bool,
    /// Сегодня — для колонтитула.
    pub date: String,
    /// Где лежат картинки заметок.
    pub note_pictures: Option<PathBuf>,
}

impl Job {
    fn theme(&self) -> Theme {
        Theme {
            dark: false,
            high: self.high,
            paper: Paper::White,
            dark_paper: Paper::Usual,
        }
    }

    /// Мера бумаги в пунктах — в кеглях читателя.
    pub fn measure(&self) -> f64 {
        f64::from(self.typography.measure) * body_size(&self.typography)
    }
}

/// Кегль текста на бумаге, пункты.
fn body_size(typography: &Type) -> f64 {
    f64::from(sheet::TEXT_SIZE * typography.scale())
}

/// Сколько пунктов бумаги в пикселе экрана при нашем наборе: этим
/// множатся поля и линейки, что в окне заданы пикселями без масштаба.
fn unit(typography: &Type) -> f64 {
    PX * f64::from(sheet::scale(typography))
}

// ── картинки ────────────────────────────────────────────────────────────────

/// Картинка, разобранная для бумаги.
pub struct Picture {
    surface: cairo::ImageSurface,
    /// Размер на бумаге, пункты.
    width: f64,
    height: f64,
    /// Сколько ниже базовой линии (формула).
    depth: f64,
}

/// Ключ картинки: одна и та же бывает и в строке, и своим абзацем —
/// разбираются они по-разному.
fn key(source: &Source, inline: bool) -> (String, bool) {
    (source.display(), inline)
}

/// Что разобрать: источник, в строке ли, и уже скачанные байты, если они
/// есть у вкладки.
pub struct Wanted {
    pub source: Source,
    pub inline: bool,
    pub bytes: Option<Arc<Vec<u8>>>,
}

/// Картинки статьи и заметок, которые понадобятся на бумаге. Чужие
/// картинки — только если читатель их не выключил: выключенные не
/// разбираются вовсе. Свои формулы и картинки заметок — всегда.
pub fn wanted(
    job: &Job,
    images: bool,
    blobs: impl Fn(&str) -> Option<Arc<Vec<u8>>>,
) -> Vec<Wanted> {
    let mut out: Vec<Wanted> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for placed in &job.page.blocks {
        let Block::Image { source, inline, .. } = &placed.block else {
            continue;
        };
        if !images && !matches!(source, Source::Formula(_)) {
            continue;
        }
        if !seen.insert(key(source, *inline)) {
            continue;
        }
        let bytes = match source {
            Source::Web(url) => blobs(url),
            _ => None,
        };
        out.push(Wanted {
            source: source.clone(),
            inline: *inline,
            bytes,
        });
    }
    if job.options.notes
        && let Some(dir) = &job.note_pictures
    {
        for note in &job.notes {
            for name in &note.pictures {
                let source = Source::File(dir.join(name));
                if seen.insert(key(&source, false)) {
                    out.push(Wanted {
                        source,
                        inline: false,
                        bytes: None,
                    });
                }
            }
        }
    }
    out
}

/// Разобранная картинка до холста cairo: холст не переходит между потоками.
pub struct Decoded {
    key: (String, bool),
    raster: Raster,
}

/// Достать и разобрать картинки — в рабочем потоке. Байты — из памяти
/// вкладки, недельной копии, архива, а в сеть — только когда их нигде нет
/// (картинка ещё не доехала). Ширина — мера бумаги, плотность — печатная.
pub fn decode(
    wanted: Vec<Wanted>,
    address: Address,
    measure: f64,
    typography: Type,
    high: bool,
) -> Vec<Decoded> {
    let theme = Theme {
        dark: false,
        high,
        paper: Paper::White,
        dark_paper: Paper::Usual,
    };
    let ink = brevier::palette::rgb(colors(theme).ink);
    let body_px = body_size(&typography) / PX;
    let threads = std::thread::available_parallelism()
        .map_or(2, |count| count.get())
        .clamp(1, 4);
    let chunks: Vec<Vec<Wanted>> = {
        let mut chunks: Vec<Vec<Wanted>> = (0..threads).map(|_| Vec::new()).collect();
        for (index, item) in wanted.into_iter().enumerate() {
            chunks[index % threads].push(item);
        }
        chunks
    };
    std::thread::scope(|scope| {
        let workers: Vec<_> = chunks
            .into_iter()
            .map(|chunk| {
                let address = address.clone();
                std::thread::Builder::new()
                    .stack_size(super::CORE_STACK)
                    .spawn_scoped(scope, move || {
                        chunk
                            .into_iter()
                            .filter_map(|item| {
                                let (bytes, mime) = bytes_of(&item, &address)?;
                                let formula = matches!(item.source, Source::Formula(_));
                                let look = media::Look {
                                    width: (measure / PX).round().max(1.0) as u32,
                                    paper: [255, 255, 255],
                                    ink: if formula { ink } else { [0, 0, 0] },
                                    font_size: body_px as f32,
                                    fit: if item.inline {
                                        media::Fit::Natural
                                    } else {
                                        media::Fit::Column
                                    },
                                    density: if formula {
                                        FORMULA_DENSITY
                                    } else {
                                        PHOTO_DENSITY
                                    },
                                };
                                let raster = media::decode(&bytes, mime.as_deref(), look).ok()?;
                                Some(Decoded {
                                    key: key(&item.source, item.inline),
                                    raster,
                                })
                            })
                            .collect::<Vec<_>>()
                    })
                    .ok()
            })
            .collect();
        workers
            .into_iter()
            .flatten()
            .flat_map(|worker| worker.join().unwrap_or_default())
            .collect()
    })
}

fn bytes_of(item: &Wanted, page: &Address) -> Option<(Vec<u8>, Option<String>)> {
    if let Some(bytes) = &item.bytes {
        return Some((bytes.as_ref().clone(), None));
    }
    if let Source::Web(url) = &item.source {
        if let Some(bytes) = brevier::cache::Cache::open().image(url) {
            return Some((bytes, None));
        }
        if let Some(bytes) = brevier::pictures::from_archive(page, url) {
            return Some((bytes, None));
        }
    }
    media::grab(&item.source, UserAgent::Honest).ok()
}

/// Холсты cairo из разобранного — в главном потоке.
pub fn pictures(decoded: Vec<Decoded>) -> HashMap<(String, bool), Picture> {
    decoded
        .into_iter()
        .filter_map(|item| Some((item.key, picture(item.raster)?)))
        .collect()
}

fn picture(raster: Raster) -> Option<Picture> {
    let (width, height) = (raster.width as i32, raster.height as i32);
    let mut surface = cairo::ImageSurface::create(cairo::Format::Rgb24, width, height).ok()?;
    let stride = surface.stride() as usize;
    {
        let mut data = surface.data().ok()?;
        for (row, pixels) in raster
            .rgba
            .chunks_exact(raster.width as usize * 4)
            .enumerate()
        {
            let line = &mut data[row * stride..row * stride + raster.width as usize * 4];
            // Rgb24 у cairo — слово `0x00RRGGBB` в порядке байтов машины.
            for column in 0..raster.width as usize {
                let pixel = &pixels[column * 4..column * 4 + 3];
                let value =
                    (u32::from(pixel[0]) << 16) | (u32::from(pixel[1]) << 8) | u32::from(pixel[2]);
                line[column * 4..column * 4 + 4].copy_from_slice(&value.to_ne_bytes());
            }
        }
    }
    surface.mark_dirty();
    let density = f64::from(raster.density.max(0.01));
    Some(Picture {
        surface,
        width: f64::from(raster.width) / density * PX,
        height: f64::from(raster.height) / density * PX,
        depth: f64::from(raster.depth) / density * PX,
    })
}

// ── вид: свойства тегов окна ─────────────────────────────────────────────────

/// Что задают теги участка — уже сведённое по приоритетам, в пунктах.
#[derive(Clone, Default)]
struct Look {
    family: Option<String>,
    size: Option<f64>,
    weight: Option<i32>,
    style: Option<pango::Style>,
    ink: Option<gdk::RGBA>,
    back: Option<gdk::RGBA>,
    underline: Option<pango::Underline>,
    underline_ink: Option<gdk::RGBA>,
    /// Pango-единицы пунктов.
    rise: Option<i32>,
    tracking: Option<i32>,
    left: Option<f64>,
    indent: Option<f64>,
    above: Option<f64>,
    below: Option<f64>,
    inside: Option<f64>,
    justify: Option<gtk::Justification>,
    tabs: Option<Vec<f64>>,
    panel: Option<gdk::RGBA>,
}

/// Пиксели тега бумаги — в пункты.
fn points(px: i32) -> f64 {
    f64::from(px) / FINE * PX
}

/// Pango-единицы тега бумаги (`rise`, `letter-spacing`) — в Pango-единицы пунктов.
fn pango_points(units: i32) -> i32 {
    (f64::from(units) / FINE * PX).round() as i32
}

impl Look {
    fn of(tag: &gtk::TextTag) -> Look {
        let when = |set: bool| set.then_some(());
        Look {
            family: when(tag.is_family_set())
                .and_then(|_| tag.family())
                .map(String::from),
            size: when(tag.is_size_set()).map(|_| tag.size_points() / FINE),
            weight: when(tag.is_weight_set()).map(|_| tag.weight()),
            style: when(tag.is_style_set()).map(|_| tag.style()),
            ink: when(tag.is_foreground_set()).and_then(|_| tag.foreground_rgba()),
            back: when(tag.is_background_set()).and_then(|_| tag.background_rgba()),
            underline: when(tag.is_underline_set()).map(|_| tag.underline()),
            underline_ink: when(tag.is_underline_rgba_set()).and_then(|_| tag.underline_rgba()),
            rise: when(tag.is_rise_set()).map(|_| pango_points(tag.rise())),
            tracking: when(tag.is_letter_spacing_set()).map(|_| pango_points(tag.letter_spacing())),
            left: when(tag.is_left_margin_set()).map(|_| points(tag.left_margin())),
            indent: when(tag.is_indent_set()).map(|_| points(tag.indent())),
            above: when(tag.is_pixels_above_lines_set()).map(|_| points(tag.pixels_above_lines())),
            below: when(tag.is_pixels_below_lines_set()).map(|_| points(tag.pixels_below_lines())),
            inside: when(tag.is_pixels_inside_wrap_set()).map(|_| points(tag.pixels_inside_wrap())),
            justify: when(tag.is_justification_set()).map(|_| tag.justification()),
            tabs: when(tag.is_tabs_set())
                .and_then(|_| tag.tabs())
                .map(|tabs| {
                    let pixels = tabs.is_positions_in_pixels();
                    (0..tabs.size())
                        .map(|index| {
                            let (_, at) = tabs.tab(index);
                            if pixels {
                                points(at)
                            } else {
                                points(at) / f64::from(pango::SCALE)
                            }
                        })
                        .collect()
                }),
            panel: when(tag.is_paragraph_background_set())
                .and_then(|_| tag.paragraph_background_rgba()),
        }
    }

    /// Наложить `top` поверх: заданное в нём главнее.
    fn under(mut self, top: &Look) -> Look {
        macro_rules! take {
            ($($field:ident),*) => {
                $(if top.$field.is_some() { self.$field = top.$field.clone(); })*
            };
        }
        take!(
            family,
            size,
            weight,
            style,
            ink,
            back,
            underline,
            underline_ink,
            rise,
            tracking,
            left,
            indent,
            above,
            below,
            inside,
            justify,
            tabs,
            panel
        );
        self
    }
}

/// Теги окна в масштабе бумаги и сведённый вид по наборам стилей.
struct Looks {
    table: gtk::TextTagTable,
    base: Look,
    known: HashMap<Vec<String>, Look>,
}

impl Looks {
    fn new(job: &Job) -> Looks {
        let buffer = gtk::TextBuffer::new(None);
        let scale = FINE as f32 * sheet::scale(&job.typography);
        super::tags(&buffer, job.theme(), scale, &job.typography);
        let colors = colors(job.theme());
        let base = Look {
            family: Some(job.typography.face.family().to_owned()),
            size: Some(body_size(&job.typography)),
            weight: Some(400),
            style: Some(pango::Style::Normal),
            ink: gdk::RGBA::parse(colors.ink).ok(),
            ..Look::default()
        };
        Looks {
            table: buffer.tag_table(),
            base,
            known: HashMap::new(),
        }
    }

    /// Вид участка с этими тегами: по возрастанию приоритета, как у GTK.
    fn of(&mut self, names: &[String]) -> Look {
        if let Some(look) = self.known.get(names) {
            return look.clone();
        }
        let mut tags: Vec<gtk::TextTag> = names
            .iter()
            .filter_map(|name| self.table.lookup(name))
            .collect();
        tags.sort_by_key(|tag| tag.priority());
        let look = tags
            .iter()
            .fold(self.base.clone(), |look, tag| look.under(&Look::of(tag)));
        self.known.insert(names.to_vec(), look.clone());
        look
    }

    fn color(&self, name: &str) -> Option<gdk::RGBA> {
        self.table
            .lookup(name)
            .and_then(|tag| tag.foreground_rgba().or_else(|| tag.background_rgba()))
    }
}

fn names(styles: &[Style]) -> Vec<String> {
    styles.iter().map(|style| style.name()).collect()
}

fn font(look: &Look) -> pango::FontDescription {
    let mut desc = pango::FontDescription::new();
    if let Some(family) = &look.family {
        desc.set_family(family);
    }
    if let Some(size) = look.size {
        desc.set_size((size * f64::from(pango::SCALE)).round() as i32);
    }
    if let Some(weight) = look.weight {
        desc.set_weight(unsafe { from_glib(weight) });
    }
    if let Some(style) = look.style {
        desc.set_style(style);
    }
    desc
}

fn channels(color: &gdk::RGBA) -> (u16, u16, u16) {
    let channel = |value: f32| (value.clamp(0.0, 1.0) * 65535.0).round() as u16;
    (
        channel(color.red()),
        channel(color.green()),
        channel(color.blue()),
    )
}

/// Атрибуты Pango участка `range` (байты текста абзаца).
fn attributes(list: &pango::AttrList, range: Range<usize>, look: &Look) {
    let put = |mut attr: pango::Attribute| {
        attr.set_start_index(range.start as u32);
        attr.set_end_index(range.end as u32);
        list.insert(attr);
    };
    put(pango::AttrFontDesc::new(&font(look)).into());
    if let Some(ink) = &look.ink {
        let (r, g, b) = channels(ink);
        put(pango::AttrColor::new_foreground(r, g, b).into());
    }
    if let Some(back) = &look.back {
        let (r, g, b) = channels(back);
        put(pango::AttrColor::new_background(r, g, b).into());
    }
    if let Some(underline) = look.underline {
        put(pango::AttrInt::new_underline(underline).into());
    }
    if let Some(ink) = &look.underline_ink {
        let (r, g, b) = channels(ink);
        put(pango::AttrColor::new_underline_color(r, g, b).into());
    }
    if let Some(rise) = look.rise {
        put(pango::AttrInt::new_rise(rise).into());
    }
    if let Some(tracking) = look.tracking {
        put(pango::AttrInt::new_letter_spacing(tracking).into());
    }
}

// ── раскладка ───────────────────────────────────────────────────────────────

/// Строка абзаца: какая она в раскладке и где.
struct LineBox {
    index: i32,
    /// Её полоса по высоте в раскладке абзаца, пункты.
    top: f64,
    bottom: f64,
    baseline: f64,
    /// Начало строки от левого края раскладки (отступ, выключка).
    x: f64,
}

/// Картинка в строке: строка, место в ней и ключ.
struct InLine {
    line: i32,
    x: f64,
    key: (String, bool),
}

enum Body {
    Text {
        layout: pango::Layout,
        lines: Vec<LineBox>,
        inline: Vec<InLine>,
    },
    Picture {
        key: (String, bool),
        width: f64,
        height: f64,
        caption: Option<pango::Layout>,
        /// С подписью.
        total: f64,
    },
    /// Картинки нет — рамка с подписью, как в окне до загрузки.
    Missing {
        label: pango::Layout,
        height: f64,
    },
    Table(Grid),
}

/// Абзац на бумаге.
struct Para {
    body: Body,
    /// Заголовок держится за то, что под ним.
    heading: bool,
    /// Абзац кончается двоеточием — подводит к тому, что ниже, и тоже
    /// держится за него.
    lead_in: bool,
    /// Пустая строка: наверху листа её не печатаем.
    blank: bool,
    /// Отступ слева от колонки и воздух над и под абзацем, пункты.
    left: f64,
    above: f64,
    below: f64,
    panel: Option<gdk::RGBA>,
    /// Уровни цитаты, у которых слева линейка.
    quotes: Vec<u8>,
}

/// Таблица: ячейки раскладками с шириной столбца, высоты рядов.
struct Grid {
    cells: Vec<Vec<(pango::Layout, f64)>>,
    rows: Vec<f64>,
    headers: Vec<bool>,
    /// Левый край относительно колонки (широкая таблица выходит в поля)
    /// и ужатие, если не влезла и в лист.
    x: f64,
    scale: f64,
    width: f64,
}

/// Кусок листа — что рисовать.
#[derive(Clone, Copy)]
enum Item {
    Line { para: usize, line: usize },
    Whole { para: usize },
    Row { para: usize, row: usize },
}

/// Лист целиком: абзацы, куски, листы.
pub struct Sheet {
    job: Job,
    pictures: HashMap<(String, bool), Picture>,
    paras: Vec<Para>,
    items: Vec<Item>,
    pieces: Vec<Piece>,
    pages: Vec<Range<usize>>,
    /// Геометрия листа, пункты.
    width: f64,
    height: f64,
    column: f64,
    column_x: f64,
    content_top: f64,
    content_bottom: f64,
    rule: gdk::RGBA,
    dim: gdk::RGBA,
    ink: gdk::RGBA,
    title: String,
    address: String,
}

impl Sheet {
    pub fn new(job: Job, pictures: HashMap<(String, bool), Picture>) -> Sheet {
        let colors = colors(job.theme());
        let parse = |hex: &str| gdk::RGBA::parse(hex).unwrap_or(gdk::RGBA::BLACK);
        let mut rule = parse(colors.dim);
        rule.set_alpha(0.5);
        let title = if job.document.title.trim().is_empty() {
            job.document.address.display()
        } else {
            job.document.title.trim().to_owned()
        };
        let address = job.document.address.display();
        Sheet {
            pictures,
            paras: Vec::new(),
            items: Vec::new(),
            pieces: Vec::new(),
            pages: Vec::new(),
            width: 0.0,
            height: 0.0,
            column: 0.0,
            column_x: 0.0,
            content_top: 0.0,
            content_bottom: 0.0,
            rule,
            dim: parse(colors.dim),
            ink: parse(colors.ink),
            title,
            address,
            job,
        }
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    /// Разложить по листу этого контекста. `selection` — печатать только
    /// выделенное. Отвечает, сколько вышло листов.
    pub fn lay(&mut self, context: &gtk::PrintContext, selection: bool) -> usize {
        self.width = context.width();
        self.height = context.height();
        let pango = context.create_pango_context();
        let printable = (self.width - 2.0 * MARGIN).max(72.0);
        self.column = self.job.measure().min(printable);
        self.column_x = (self.width - self.column) / 2.0;
        let running = if self.job.options.running {
            self.running_height(&pango)
        } else {
            0.0
        };
        self.content_top = MARGIN + running;
        self.content_bottom = self.height - MARGIN - running;

        let text_len = self.job.page.text.chars().count();
        let range = match (selection, self.job.selection) {
            (true, Some((from, to))) => from..to,
            _ => 0..text_len,
        };
        let here = self.job.document.address.external();
        let endnotes = if self.job.options.links {
            sheet::endnotes(&self.job.page, &here, range.clone())
        } else {
            Endnotes::default()
        };

        // Заметки: отмеченное на своих местах и сами заметки в конце —
        // только те, что попали в печатаемое.
        let mut marks = Vec::new();
        let mut appended: Vec<&Note> = Vec::new();
        if self.job.options.notes {
            for (note, place) in self.job.notes.iter().zip(&self.job.places) {
                match place {
                    Some((from, to)) if *to > range.start && *from < range.end => {
                        marks.push((*from, *to, note.says()));
                        appended.push(note);
                    }
                    None if !selection => appended.push(note),
                    _ => {}
                }
            }
        }
        let appendix = sheet::appendix(&appended, &endnotes.targets);

        let mut looks = Looks::new(&self.job);
        self.paras.clear();
        self.items.clear();
        self.pieces.clear();
        let page = std::mem::take(&mut self.job.page);
        self.lay_page(&pango, &mut looks, &page, range, &endnotes, &marks);
        self.job.page = page;
        if !appendix.is_empty() {
            let base = self
                .job
                .note_pictures
                .clone()
                .unwrap_or_default()
                .join("notes.md");
            let extra = Page::of(&Document {
                address: Address::File(base),
                title: String::new(),
                markdown: appendix,
                kind: brevier::Kind::Article,
                served: false,
                site: Vec::new(),
                feeds: Vec::new(),
                // Без переносов: в приложении адреса, и дефис переноса
                // в адресе на бумаге не отличить от настоящего.
                lang: None,
                next: None,
                archived: None,
            });
            let all = 0..extra.text.chars().count();
            self.lay_page(&pango, &mut looks, &extra, all, &Endnotes::default(), &[]);
        }
        self.pages = sheet::paginate(&self.pieces, self.content_bottom - self.content_top);
        self.pages.len().max(1)
    }

    fn running_height(&self, pango: &pango::Context) -> f64 {
        let layout = self.running_layout(pango, "Ag");
        let (_, height) = layout.size();
        f64::from(height) / f64::from(pango::SCALE) * 2.0
    }

    fn running_layout(&self, pango: &pango::Context, text: &str) -> pango::Layout {
        let layout = pango::Layout::new(pango);
        let look = Look {
            family: Some(self.job.typography.face.family().to_owned()),
            size: Some(body_size(&self.job.typography) * RUNNING_SIZE),
            ..Look::default()
        };
        layout.set_font_description(Some(&font(&look)));
        layout.set_text(text);
        layout
    }

    /// Разложить страницу модели в абзацы и куски.
    fn lay_page(
        &mut self,
        pango: &pango::Context,
        looks: &mut Looks,
        page: &Page,
        range: Range<usize>,
        endnotes: &Endnotes,
        marks: &[(usize, usize, bool)],
    ) {
        let chars: Vec<char> = page.text.chars().collect();
        let blocks: HashMap<usize, usize> = page
            .blocks
            .iter()
            .enumerate()
            .map(|(index, placed)| (placed.at, index))
            .collect();
        // Номер ссылки — после её последнего знака.
        let mut after: HashMap<usize, Vec<usize>> = HashMap::new();
        for (index, link) in page.links.iter().enumerate() {
            if let Some(&number) = endnotes.text.get(&index) {
                after.entry(link.end).or_default().push(number);
            }
        }
        let mut start = 0;
        while start <= chars.len() {
            let end = chars[start..]
                .iter()
                .position(|&c| c == '\n')
                .map_or(chars.len(), |at| start + at);
            let (from, to) = (start.max(range.start), end.min(range.end));
            let inside = from < to || (start == end && range.contains(&start));
            if inside {
                let styles = styles_at(page, if start < end { start } else { end });
                let para = self.paragraph(
                    pango,
                    looks,
                    page,
                    &chars,
                    from..to,
                    &styles,
                    &blocks,
                    &after,
                    endnotes,
                    marks,
                );
                if let Some(para) = para {
                    self.push(para);
                }
            }
            if end >= chars.len() {
                break;
            }
            start = end + 1;
        }
    }

    /// Положить абзац и его куски.
    fn push(&mut self, para: Para) {
        let index = self.paras.len();
        // Воздух под предыдущим абзацем — над этим, как у `GtkTextView`.
        let first_above = self.paras.last().map_or(0.0, |last| last.below) + para.above;
        let heading = para.heading;
        // Подводка к картинке или таблице («Table 3-1: …», «Here's an example:»)
        // из одной строки не остаётся внизу листа без того, что она подводит.
        if matches!(
            para.body,
            Body::Table(_) | Body::Picture { .. } | Body::Missing { .. }
        ) && let Some(Body::Text { lines, .. }) = self.paras.last().map(|last| &last.body)
            && lines.len() == 1
            && let Some(last) = self.pieces.last_mut()
        {
            last.keep = true;
        }
        let lead_in = para.lead_in;
        match &para.body {
            Body::Text { lines, .. } => {
                let count = lines.len();
                for (line, boxed) in lines.iter().enumerate() {
                    // Не оставлять первую строку одну внизу листа, последнюю —
                    // одну наверху; заголовок — с тем, что под ним.
                    let keep = heading
                        || (count >= 2 && (line == 0 || line + 2 == count))
                        || (lead_in && line + 1 == count);
                    self.items.push(Item::Line { para: index, line });
                    let height = boxed.bottom - boxed.top;
                    self.pieces.push(Piece {
                        height,
                        above: if line == 0 { first_above } else { 0.0 },
                        keep,
                        least: height,
                    });
                }
            }
            Body::Table(grid) => {
                for (row, height) in grid.rows.iter().enumerate() {
                    self.items.push(Item::Row { para: index, row });
                    self.pieces.push(Piece {
                        height: *height * grid.scale,
                        above: if row == 0 { first_above } else { 0.0 },
                        keep: grid.headers[row],
                        least: *height * grid.scale,
                    });
                }
            }
            Body::Picture { height, total, .. } => {
                self.items.push(Item::Whole { para: index });
                self.pieces.push(Piece {
                    height: *total,
                    above: first_above,
                    keep: false,
                    // Ужимается картинка, подпись — нет.
                    least: total - height * (1.0 - FIGURE_LEAST),
                });
            }
            Body::Missing { .. } => {
                self.items.push(Item::Whole { para: index });
                self.pieces.push(Piece {
                    height: para.height(),
                    above: first_above,
                    keep: false,
                    least: para.height(),
                });
            }
        }
        self.paras.push(para);
    }

    #[allow(clippy::too_many_arguments)]
    fn paragraph(
        &self,
        pango: &pango::Context,
        looks: &mut Looks,
        page: &Page,
        chars: &[char],
        range: Range<usize>,
        styles: &[Style],
        blocks: &HashMap<usize, usize>,
        after: &HashMap<usize, Vec<usize>>,
        endnotes: &Endnotes,
        marks: &[(usize, usize, bool)],
    ) -> Option<Para> {
        let look = looks.of(&names(styles));
        let quotes = styles
            .iter()
            .filter_map(|style| match style {
                Style::Quote(level) | Style::Rule(level) => Some(*level),
                _ => None,
            })
            .collect();
        let lead_in = chars[range.clone()]
            .iter()
            .rev()
            .find(|c| !c.is_whitespace())
            .is_some_and(|&c| c == ':');
        let blank = chars[range.clone()].iter().all(|c| c.is_whitespace());
        let made = |body: Body, above: f64, below: f64| Para {
            body,
            heading: styles
                .iter()
                .any(|style| matches!(style, Style::Heading(_))),
            lead_in,
            blank,
            left: look.left.unwrap_or(0.0),
            above: look.above.unwrap_or(0.0) + above,
            below: look.below.unwrap_or(0.0) + below,
            panel: look.panel,
            quotes,
        };
        let width = (self.column - look.left.unwrap_or(0.0)).max(36.0);

        // Картинка или таблица своим абзацем.
        if range.len() == 1
            && chars[range.start] == page::OBJECT
            && let Some(&block) = blocks.get(&range.start)
        {
            match &page.blocks[block].block {
                Block::Image {
                    source,
                    alt,
                    inline: false,
                } => {
                    let body = self.figure(pango, &key(source, false), alt, width);
                    return Some(made(body, 0.0, 0.0));
                }
                Block::Table(table) => {
                    let unit = unit(&self.job.typography);
                    let body = Body::Table(self.grid(pango, looks, table, block, endnotes));
                    return Some(made(body, TABLE_ABOVE * unit, TABLE_BELOW * unit));
                }
                Block::Image { .. } => {}
            }
        }

        // Текст: участки по стилям, отмеченному и номерам ссылок.
        let mut text = String::new();
        let list = pango::AttrList::new();
        let mut inline = Vec::new();
        let mut shapes: Vec<(usize, (String, bool))> = Vec::new();
        let mut offset = range.start;
        while offset < range.end {
            let here = styles_at(page, offset);
            let marked = marks
                .iter()
                .filter(|(from, to, _)| *from <= offset && offset < *to)
                .fold((false, false), |(_, said), (_, _, says)| {
                    (true, said || *says)
                });
            // Участок — до смены стиля, отметки, конца или объекта.
            let mut end = offset + 1;
            while end < range.end
                && chars[end] != page::OBJECT
                && chars[end - 1] != page::OBJECT
                && !after.contains_key(&end)
                && styles_at(page, end) == here
                && marks.iter().all(|(from, to, _)| *from != end && *to != end)
            {
                end += 1;
            }
            let mut tags = names(&here);
            if marked.0 {
                tags.push("highlight".to_owned());
            }
            if marked.1 {
                tags.push("noted".to_owned());
            }
            let look = looks.of(&tags);
            if chars[offset] == page::OBJECT {
                let placed = blocks.get(&offset).map(|&block| &page.blocks[block].block);
                let (picture, alt) = match placed {
                    Some(Block::Image { source, alt, .. }) => {
                        let wanted = key(source, true);
                        (
                            self.pictures.get(&wanted).map(|p| (wanted, p)),
                            alt.as_str(),
                        )
                    }
                    _ => (None, ""),
                };
                let start = text.len();
                match picture {
                    Some((wanted, picture)) => {
                        text.push(page::OBJECT);
                        shapes.push((start, wanted));
                        let scale = f64::from(pango::SCALE);
                        let rect = pango::Rectangle::new(
                            0,
                            (-(picture.height - picture.depth) * scale).round() as i32,
                            (picture.width * scale).round() as i32,
                            (picture.height * scale).round() as i32,
                        );
                        attributes(&list, start..text.len(), &look);
                        let mut shape: pango::Attribute =
                            pango::AttrShape::new(&rect, &rect).into();
                        shape.set_start_index(start as u32);
                        shape.set_end_index(text.len() as u32);
                        list.insert(shape);
                    }
                    None => {
                        // Не разобралась — словами, приглушённо.
                        text.push_str(if alt.trim().is_empty() {
                            "▢"
                        } else {
                            alt.trim()
                        });
                        let mut dim = tags.clone();
                        dim.push("dim".to_owned());
                        attributes(&list, start..text.len(), &looks.of(&dim));
                    }
                }
            } else {
                let start = text.len();
                text.extend(&chars[offset..end]);
                attributes(&list, start..text.len(), &look);
                // Адрес подписью ссылки переносится без дефиса: дефис в адресе
                // на бумаге не отличить от настоящего.
                if here.contains(&Style::Link) && looks_like_address(&chars[offset..end]) {
                    let mut plain: pango::Attribute =
                        pango::AttrInt::new_insert_hyphens(false).into();
                    plain.set_start_index(start as u32);
                    plain.set_end_index(text.len() as u32);
                    list.insert(plain);
                }
            }
            if let Some(numbers) = after.get(&end) {
                // Номер сноски — верхним индексом, без вида ссылки и отметки.
                let mut tags: Vec<String> = names(&styles_at(page, end - 1))
                    .into_iter()
                    .filter(|name| name != "link")
                    .collect();
                tags.push("noteref".to_owned());
                let start = text.len();
                let shown: Vec<String> = numbers.iter().map(usize::to_string).collect();
                text.push_str(&shown.join(","));
                attributes(&list, start..text.len(), &looks.of(&tags));
            }
            offset = end;
        }

        let layout = pango::Layout::new(pango);
        layout.set_font_description(Some(&font(&looks.base)));
        layout.set_text(&text);
        layout.set_attributes(Some(&list));
        layout.set_width((width * f64::from(pango::SCALE)).round() as i32);
        layout.set_wrap(pango::WrapMode::WordChar);
        let scale = f64::from(pango::SCALE);
        layout.set_indent((look.indent.unwrap_or(0.0) * scale).round() as i32);
        layout.set_spacing((look.inside.unwrap_or(0.0) * scale).round() as i32);
        match look.justify.unwrap_or(gtk::Justification::Left) {
            gtk::Justification::Fill => layout.set_justify(true),
            gtk::Justification::Right => layout.set_alignment(pango::Alignment::Right),
            gtk::Justification::Center => layout.set_alignment(pango::Alignment::Center),
            _ => {}
        }
        if let Some(stops) = &look.tabs {
            let mut tabs = pango::TabArray::new(stops.len() as i32, true);
            for (index, at) in stops.iter().enumerate() {
                tabs.set_tab(index as i32, pango::TabAlign::Left, at.round() as i32);
            }
            layout.set_tabs(Some(&tabs));
        }

        let mut lines = Vec::new();
        let mut iter = layout.iter();
        let mut index = 0;
        loop {
            let (top, bottom) = iter.line_yrange();
            let (_, logical) = iter.line_extents();
            lines.push(LineBox {
                index,
                top: f64::from(top) / scale,
                bottom: f64::from(bottom) / scale,
                baseline: f64::from(iter.baseline()) / scale,
                x: f64::from(logical.x()) / scale,
            });
            index += 1;
            if !iter.next_line() {
                break;
            }
        }
        for (at, wanted) in shapes {
            let (line, x) = layout.index_to_line_x(at as i32, false);
            inline.push(InLine {
                line,
                x: f64::from(x) / scale,
                key: wanted,
            });
        }
        let body = Body::Text {
            layout,
            lines,
            inline,
        };
        Some(made(body, 0.0, 0.0))
    }

    /// Картинка своим абзацем: по середине колонки, не шире её и не выше
    /// листа, подпись — под ней. Нет картинки — рамка с подписью.
    fn figure(
        &self,
        pango: &pango::Context,
        wanted: &(String, bool),
        alt: &str,
        width: f64,
    ) -> Body {
        let unit = unit(&self.job.typography);
        let body = body_size(&self.job.typography);
        let caption_look = Look {
            family: Some(self.job.typography.face.family().to_owned()),
            size: Some(body * CAPTION_SIZE),
            ..Look::default()
        };
        let caption = |text: &str, wrap: f64, center: bool| {
            let layout = pango::Layout::new(pango);
            layout.set_font_description(Some(&font(&caption_look)));
            layout.set_text(text);
            layout.set_width((wrap * f64::from(pango::SCALE)).round() as i32);
            layout.set_wrap(pango::WrapMode::WordChar);
            if center {
                layout.set_alignment(pango::Alignment::Center);
            }
            layout
        };
        let Some(picture) = self.pictures.get(wanted) else {
            let label = caption(
                if alt.trim().is_empty() {
                    "image"
                } else {
                    alt.trim()
                },
                width - 28.0 * unit,
                true,
            );
            let (_, height) = label.size();
            return Body::Missing {
                height: f64::from(height) / f64::from(pango::SCALE) + 40.0 * unit,
                label,
            };
        };
        let room = (self.content_bottom - self.content_top) * 0.9;
        let caption =
            (!alt.trim().is_empty()).then(|| caption(alt.trim(), width, picture.width < width));
        let caption_height = caption.as_ref().map_or(0.0, |layout| {
            f64::from(layout.size().1) / f64::from(pango::SCALE) + CAPTION_GAP * unit
        });
        let fit = (width / picture.width)
            .min((room - caption_height).max(36.0) / picture.height)
            .min(1.0);
        Body::Picture {
            key: wanted.clone(),
            width: picture.width * fit,
            height: picture.height * fit,
            caption,
            total: picture.height * fit + caption_height,
        }
    }

    /// Таблица: столбцы по содержимому, ряды по самой высокой ячейке.
    fn grid(
        &self,
        pango: &pango::Context,
        looks: &mut Looks,
        table: &page::Table,
        block: usize,
        endnotes: &Endnotes,
    ) -> Grid {
        let unit = unit(&self.job.typography);
        let scale = f64::from(pango::SCALE);
        let base = looks.base.clone();
        let cell_look = Look {
            size: base.size.map(|size| size * CELL_SIZE),
            ..base
        };
        let link = looks.color("link");
        let columns = table
            .rows
            .iter()
            .map(|row| row.cells.len())
            .max()
            .unwrap_or(1)
            .max(1);
        let mut natural = vec![0.0f64; columns];
        let mut least = vec![0.0f64; columns];
        let mut layouts: Vec<Vec<pango::Layout>> = Vec::new();
        for (row_index, row) in table.rows.iter().enumerate() {
            let mut out = Vec::new();
            for (column, cell) in row.cells.iter().enumerate() {
                let numbers = |link: usize| {
                    endnotes
                        .cells
                        .get(&sheet::CellLink {
                            block,
                            row: row_index,
                            cell: column,
                            link,
                        })
                        .copied()
                };
                let markup = cell_markup(cell, link.as_ref(), &numbers, NOTEREF_RISE as f64 * unit);
                let layout = pango::Layout::new(pango);
                let mut look = cell_look.clone();
                if row.header {
                    look.weight = Some(500);
                }
                layout.set_font_description(Some(&font(&look)));
                layout.set_markup(&markup);
                layout.set_alignment(match table.alignments.get(column) {
                    Some(page::Align::Right) => pango::Alignment::Right,
                    Some(page::Align::Center) => pango::Alignment::Center,
                    _ => pango::Alignment::Left,
                });
                let (width, _) = layout.size();
                natural[column] = natural[column].max(f64::from(width) / scale);
                layout.set_wrap(pango::WrapMode::Word);
                layout.set_width(1);
                let (width, _) = layout.size();
                least[column] = least[column].max(f64::from(width) / scale);
                layout.set_wrap(pango::WrapMode::WordChar);
                out.push(layout);
            }
            layouts.push(out);
        }
        let gap = TABLE_COLUMNS * unit;
        let gaps = gap * (columns - 1) as f64;
        let printable = self.width - 2.0 * MARGIN;
        let want: f64 = natural.iter().sum::<f64>() + gaps;
        let least_sum: f64 = least.iter().sum::<f64>() + gaps;
        let (widths, total) = if want <= self.column {
            // Влезает — остаток меры последнему столбцу, как в окне.
            let mut widths = natural.clone();
            if let Some(last) = widths.last_mut() {
                *last += self.column - want;
            }
            (widths, self.column)
        } else {
            let room = if least_sum <= self.column {
                self.column
            } else {
                printable.max(self.column)
            };
            let spare = (room - least_sum).max(0.0);
            let stretch: f64 = natural.iter().zip(&least).map(|(n, l)| n - l).sum();
            let widths: Vec<f64> = natural
                .iter()
                .zip(&least)
                .map(|(n, l)| {
                    if stretch > 0.0 {
                        l + spare * (n - l) / stretch
                    } else {
                        *l
                    }
                })
                .collect();
            let total = widths.iter().sum::<f64>() + gaps;
            (widths, total)
        };
        let shrink = if total > printable {
            printable / total
        } else {
            1.0
        };
        let mut cells = Vec::new();
        let mut rows = Vec::new();
        let mut headers = Vec::new();
        let count = layouts.len();
        for (row_index, out) in layouts.into_iter().enumerate() {
            let mut height: f64 = 0.0;
            let mut row = Vec::new();
            for (column, layout) in out.into_iter().enumerate() {
                layout.set_width((widths[column] * scale).round() as i32);
                height = height.max(f64::from(layout.size().1) / scale);
                row.push((layout, widths[column]));
            }
            let header = table.rows[row_index].header;
            if row_index + 1 < count {
                height += TABLE_ROWS * unit;
            }
            if header {
                // Черта под шапкой и воздух под ней, как у сетки окна.
                height += unit + TABLE_ROWS * unit;
            }
            rows.push(height);
            headers.push(header);
            cells.push(row);
        }
        let x = if total * shrink > self.column {
            -((total * shrink - self.column) / 2.0).min(self.column_x - MARGIN)
        } else {
            0.0
        };
        Grid {
            cells,
            rows,
            headers,
            x,
            scale: shrink,
            width: total,
        }
    }

    // ── листы ──────────────────────────────────────────────────────────────

    pub fn draw(&self, context: &gtk::PrintContext, number: usize) {
        let cr = context.cairo_context();
        let pango = context.create_pango_context();
        if self.job.options.running {
            self.draw_running(&cr, &pango, number);
        }
        let Some(range) = self.pages.get(number).cloned() else {
            return;
        };
        // Пустые строки наверху листа не печатаются: лист начинается с текста.
        let start = (range.start..range.end)
            .find(|&index| !self.blank(index))
            .unwrap_or(range.end);
        let range = start..range.end;
        let mut y = self.content_top;
        // Где на этом листе каждый абзац: для линеек цитаты.
        let mut spans: Vec<(usize, f64, f64)> = Vec::new();
        for index in range.clone() {
            let piece = self.pieces[index];
            let first = index == range.start;
            if !first {
                y += piece.above;
            }
            let item = self.items[index];
            let para_index = match item {
                Item::Line { para, .. } | Item::Whole { para } | Item::Row { para, .. } => para,
            };
            let para = &self.paras[para_index];
            // Воздух абзаца, что на этом листе, — его: панель кода
            // и линейка цитаты идут и по нему.
            let starts = match item {
                Item::Line { line, .. } => line == 0,
                Item::Row { row, .. } => row == 0,
                Item::Whole { .. } => true,
            };
            let ends = match (&para.body, item) {
                (Body::Text { lines, .. }, Item::Line { line, .. }) => line + 1 == lines.len(),
                (Body::Table(grid), Item::Row { row, .. }) => row + 1 == grid.rows.len(),
                _ => true,
            };
            let next_here = index + 1 < range.end;
            let top = if starts && !first { y - para.above } else { y };
            let bottom = if ends && next_here {
                y + piece.height + para.below
            } else {
                y + piece.height
            };
            if let Some(panel) = &para.panel {
                set_color(&cr, panel);
                cr.rectangle(
                    self.column_x + para.left,
                    top,
                    self.column - para.left,
                    bottom - top,
                );
                let _ = cr.fill();
            }
            match spans.last_mut() {
                Some((which, _, end)) if *which == para_index => *end = bottom,
                _ => spans.push((para_index, top, bottom)),
            }
            // Ужатая картинка (`paginate`) — последняя на листе: ей остаток.
            let room = self.content_bottom - y;
            self.draw_item(&cr, para, item, y, room);
            y += piece.height;
        }
        for (para, top, bottom) in spans {
            self.draw_rules(&cr, &self.paras[para], top, bottom);
        }
    }

    /// Пустая строка без панели (у панели кода пустые строки — её поля).
    fn blank(&self, index: usize) -> bool {
        let para = match self.items[index] {
            Item::Line { para, .. } => &self.paras[para],
            _ => return false,
        };
        para.blank && para.panel.is_none()
    }

    fn draw_item(&self, cr: &cairo::Context, para: &Para, item: Item, y: f64, room: f64) {
        let x = self.column_x + para.left;
        match (&para.body, item) {
            (
                Body::Text {
                    layout,
                    lines,
                    inline,
                },
                Item::Line { line, .. },
            ) => {
                let boxed = &lines[line];
                let Some(line_layout) = layout.line_readonly(boxed.index) else {
                    return;
                };
                set_color(cr, &self.ink);
                let baseline = y + boxed.baseline - boxed.top;
                cr.move_to(x + boxed.x, baseline);
                pangocairo::functions::show_layout_line(cr, &line_layout);
                for picture in inline.iter().filter(|picture| picture.line == boxed.index) {
                    if let Some(drawn) = self.pictures.get(&picture.key) {
                        self.paint(
                            cr,
                            drawn,
                            x + boxed.x + picture.x,
                            baseline - (drawn.height - drawn.depth),
                            drawn.width,
                            drawn.height,
                        );
                    }
                }
            }
            (
                Body::Picture {
                    key,
                    width,
                    height,
                    caption,
                    total,
                },
                _,
            ) => {
                let column = self.column - para.left;
                let fit = if *total > room + 0.01 {
                    ((room - (total - height)) / height).clamp(FIGURE_LEAST, 1.0)
                } else {
                    1.0
                };
                let (width, height) = (&(width * fit), &(height * fit));
                if let Some(picture) = self.pictures.get(key) {
                    self.paint(cr, picture, x + (column - width) / 2.0, y, *width, *height);
                }
                // Подпись — во всю колонку; под узкой картинкой — по середине
                // (так её и разложили).
                if let Some(caption) = caption {
                    let gap = CAPTION_GAP * unit(&self.job.typography);
                    set_color(cr, &self.dim);
                    cr.move_to(x, y + height + gap);
                    pangocairo::functions::show_layout(cr, caption);
                }
            }
            (Body::Missing { label, height }, _) => {
                let unit = unit(&self.job.typography);
                let column = self.column - para.left;
                set_color(cr, &self.dim);
                cr.set_line_width((unit).max(0.5));
                cr.set_dash(&[3.0 * unit, 2.0 * unit], 0.0);
                rounded(
                    cr,
                    x,
                    y + 6.0 * unit,
                    column,
                    height - 12.0 * unit,
                    6.0 * unit,
                );
                let _ = cr.stroke();
                cr.set_dash(&[], 0.0);
                let (_, label_height) = label.size();
                let label_height = f64::from(label_height) / f64::from(pango::SCALE);
                cr.move_to(x + 14.0 * unit, y + (height - label_height) / 2.0);
                pangocairo::functions::show_layout(cr, label);
            }
            (Body::Table(grid), Item::Row { row, .. }) => self.draw_row(cr, grid, row, x, y),
            _ => {}
        }
    }

    fn draw_row(&self, cr: &cairo::Context, grid: &Grid, row: usize, x: f64, y: f64) {
        let unit = unit(&self.job.typography);
        let _ = cr.save();
        cr.translate(x + grid.x, y);
        cr.scale(grid.scale, grid.scale);
        let mut left = 0.0;
        let mut tallest: f64 = 0.0;
        for (layout, width) in &grid.cells[row] {
            set_color(cr, &self.ink);
            cr.move_to(left, 0.0);
            pangocairo::functions::show_layout(cr, layout);
            tallest = tallest.max(f64::from(layout.size().1) / f64::from(pango::SCALE));
            left += width + TABLE_COLUMNS * unit;
        }
        if grid.headers[row] {
            set_color(cr, &self.rule);
            let at = tallest + TABLE_ROWS * unit;
            cr.rectangle(0.0, at, grid.width, unit.max(0.5));
            let _ = cr.fill();
        }
        let _ = cr.restore();
    }

    /// Линейка слева от цитаты — по уровню, как в окне (`article.rs`).
    fn draw_rules(&self, cr: &cairo::Context, para: &Para, top: f64, bottom: f64) {
        if para.quotes.is_empty() {
            return;
        }
        let unit = unit(&self.job.typography);
        set_color(cr, &self.rule);
        for level in &para.quotes {
            let x = self.column_x + (RULE_X + INDENT * f32::from(level - 1)) as f64 * unit;
            let inset = f64::from(RULE_INSET) * unit;
            cr.rectangle(
                x,
                top + inset,
                f64::from(RULE_WIDTH) * unit,
                (bottom - top - 2.0 * inset).max(unit),
            );
            let _ = cr.fill();
        }
    }

    fn paint(
        &self,
        cr: &cairo::Context,
        picture: &Picture,
        x: f64,
        y: f64,
        width: f64,
        height: f64,
    ) {
        let (pixels_wide, pixels_high) = (picture.surface.width(), picture.surface.height());
        if pixels_wide == 0 || pixels_high == 0 {
            return;
        }
        let _ = cr.save();
        cr.translate(x, y);
        cr.scale(
            width / f64::from(pixels_wide),
            height / f64::from(pixels_high),
        );
        if cr.set_source_surface(&picture.surface, 0.0, 0.0).is_ok() {
            cr.source().set_filter(cairo::Filter::Good);
            let _ = cr.paint();
        }
        let _ = cr.restore();
    }

    /// Колонтитулы: на первом листе — адрес и дата, на остальных — заголовок;
    /// внизу — номер листа из скольких.
    fn draw_running(&self, cr: &cairo::Context, pango: &pango::Context, number: usize) {
        let total = self.pages.len().max(1);
        let left = self.column_x;
        let width = self.column;
        let scale = f64::from(pango::SCALE);
        set_color(cr, &self.dim);
        let top = MARGIN;
        let (head, date) = if number == 0 {
            (self.address.as_str(), Some(self.job.date.as_str()))
        } else {
            (self.title.as_str(), None)
        };
        let date_width = date.map_or(0.0, |date| {
            let layout = self.running_layout(pango, date);
            let (w, _) = layout.size();
            let w = f64::from(w) / scale;
            cr.move_to(left + width - w, top);
            pangocairo::functions::show_layout(cr, &layout);
            w
        });
        let layout = self.running_layout(pango, head);
        let room = width
            - if date_width > 0.0 {
                date_width + 18.0
            } else {
                0.0
            };
        layout.set_width((room.max(36.0) * scale).round() as i32);
        layout.set_ellipsize(if number == 0 {
            pango::EllipsizeMode::Middle
        } else {
            pango::EllipsizeMode::End
        });
        cr.move_to(left, top);
        pangocairo::functions::show_layout(cr, &layout);

        let footer = self.running_layout(pango, &format!("{} / {total}", number + 1));
        let (w, h) = footer.size();
        cr.move_to(
            left + (width - f64::from(w) / scale) / 2.0,
            self.height - MARGIN - f64::from(h) / scale,
        );
        pangocairo::functions::show_layout(cr, &footer);
    }
}

impl Para {
    fn height(&self) -> f64 {
        match &self.body {
            Body::Picture { total, .. } => *total,
            Body::Missing { height, .. } => *height,
            Body::Text { lines, .. } => lines.last().map_or(0.0, |line| line.bottom),
            Body::Table(grid) => grid.rows.iter().sum::<f64>() * grid.scale,
        }
    }
}

fn set_color(cr: &cairo::Context, color: &gdk::RGBA) {
    cr.set_source_rgba(
        f64::from(color.red()),
        f64::from(color.green()),
        f64::from(color.blue()),
        f64::from(color.alpha()),
    );
}

fn rounded(cr: &cairo::Context, x: f64, y: f64, width: f64, height: f64, radius: f64) {
    let radius = radius.min(width / 2.0).min(height / 2.0).max(0.0);
    let degrees = std::f64::consts::PI / 180.0;
    cr.new_sub_path();
    cr.arc(x + width - radius, y + radius, radius, -90.0 * degrees, 0.0);
    cr.arc(
        x + width - radius,
        y + height - radius,
        radius,
        0.0,
        90.0 * degrees,
    );
    cr.arc(
        x + radius,
        y + height - radius,
        radius,
        90.0 * degrees,
        180.0 * degrees,
    );
    cr.arc(
        x + radius,
        y + radius,
        radius,
        180.0 * degrees,
        270.0 * degrees,
    );
    cr.close_path();
}

/// Похоже на адрес: без пробелов и с косой чертой или `@`.
fn looks_like_address(text: &[char]) -> bool {
    !text.iter().any(|c| c.is_whitespace()) && text.iter().any(|&c| c == '/' || c == '@')
}

/// Стили знака модели: участок, в который он попал.
fn styles_at(page: &Page, offset: usize) -> Vec<Style> {
    let index = page.runs.partition_point(|run| run.start <= offset);
    index
        .checked_sub(1)
        .map(|index| &page.runs[index])
        .filter(|run| offset < run.end)
        .map(|run| run.styles.clone())
        .unwrap_or_default()
}

/// Ячейка разметкой Pango для бумаги: как `markup_of` окна, но ссылка —
/// подчёркнутый цвет ссылки (`<a>` знает только `GtkLabel`), а за ссылкой
/// с номером — номер верхним индексом.
fn cell_markup(
    cell: &page::Cell,
    link: Option<&gdk::RGBA>,
    numbers: &dyn Fn(usize) -> Option<usize>,
    rise: f64,
) -> String {
    let color = link.map_or("#1a4f8b".to_owned(), |rgba| {
        let (r, g, b) = channels(rgba);
        format!("#{:02x}{:02x}{:02x}", r >> 8, g >> 8, b >> 8)
    });
    super::cell_markup(
        cell,
        &|_| format!("<span underline=\"single\" foreground=\"{color}\">"),
        "</span>",
        &|index| {
            numbers(index)
                .map(|number| {
                    format!(
                        "<span size=\"{}%\" rise=\"{}\">{number}</span>",
                        (NOTEREF_SIZE * 100.0).round() as i32,
                        (rise * f64::from(pango::SCALE)).round() as i32,
                    )
                })
                .unwrap_or_default()
        },
    )
}

/// Сегодняшнее число словами — для колонтитула.
pub fn today() -> String {
    let offset = glib::DateTime::now_local()
        .map(|now| (now.utc_offset().0 / 1_000_000) as i32)
        .unwrap_or(0);
    brevier::archive::long_date(&brevier::store::Stamp::now(offset))
}

/// Места заметок на странице — так же, как их закрашивает окно.
pub fn places(page: &Page, notes: &[Note]) -> Vec<Option<(usize, usize)>> {
    let refs: Vec<&Note> = notes.iter().collect();
    notes::place(&page.text, &refs)
        .into_iter()
        .map(|spot| spot.place)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Свойства тегов окна, которые бумага переносит на лист. Новое свойство
    /// у тега — новая строка в [`Look::of`], иначе бумага молча его потеряет.
    const KNOWN: [&str; 18] = [
        "family",
        "size",
        "weight",
        "style",
        "foreground",
        "background",
        "underline",
        "underline-rgba",
        "rise",
        "letter-spacing",
        "left-margin",
        "indent",
        "pixels-above-lines",
        "pixels-below-lines",
        "pixels-inside-wrap",
        "justification",
        "tabs",
        "paragraph-background",
    ];

    /// Одним тестом: GTK поднимается один раз на процесс и в одном потоке.
    /// Без экрана GTK не поднять — тогда проверять нечего (CI окно не гоняет).
    ///
    /// Вторая половина — для глаз: `BREVIER_PAPER` — адреса или файлы через
    /// пробел, `BREVIER_PAPER_OUT` — куда положить PDF. Каждая страница
    /// печатается целиком, с заметкой на третьем абзаце, и ещё раз — только
    /// выделенное (второй экран текста).
    #[test]
    fn paper_knows_every_tag_property() {
        if gtk::init().is_err() {
            return;
        }
        let buffer = gtk::TextBuffer::new(None);
        let theme = Theme {
            dark: false,
            high: false,
            paper: Paper::White,
            dark_paper: Paper::Usual,
        };
        super::super::tags(&buffer, theme, 1.0, &Type::USUAL);
        let mut unknown = Vec::new();
        buffer.tag_table().foreach(|tag| {
            for spec in tag.list_properties() {
                let name = spec.name();
                if let Some(base) = name.strip_suffix("-set")
                    && tag.property::<bool>(name)
                    && !KNOWN.contains(&base)
                {
                    unknown.push(format!("{:?}: {base}", tag.name()));
                }
            }
        });
        assert!(unknown.is_empty(), "the paper ignores {unknown:?}");

        if let (Ok(pages), Ok(out)) = (
            std::env::var("BREVIER_PAPER"),
            std::env::var("BREVIER_PAPER_OUT"),
        ) {
            for (index, target) in pages.split_whitespace().enumerate() {
                print_for_eyes(target, &PathBuf::from(&out), index);
            }
        }
    }

    fn print_for_eyes(target: &str, out: &std::path::Path, index: usize) {
        brevier::init_crypto();
        let address = brevier::address::parse(target).expect("an address");
        let document = brevier::open(&address, UserAgent::Honest).expect("the page");
        let page = Page::guarded(&document);
        // Заметка на третьем абзаце: первые слова его — цитатой.
        let mut notes = Vec::new();
        let paragraphs: Vec<(usize, &str)> = {
            let mut at = 0;
            page.text
                .split('\n')
                .map(|line| {
                    let start = at;
                    at += line.chars().count() + 1;
                    (start, line)
                })
                .filter(|(_, line)| line.chars().count() > 80)
                .collect()
        };
        if let Some((at, line)) = paragraphs.get(2) {
            let quote: String = line.chars().take(50).collect();
            notes.push(Note {
                id: 1,
                stamp: brevier::store::Stamp::now(0),
                address: document.address.display(),
                title: document.title.clone(),
                at: *at,
                before: String::new(),
                quote: brevier::typeset::plain(&quote),
                after: String::new(),
                text: "A note of mine, printed at the end.".to_owned(),
                pictures: Vec::new(),
            });
        }
        let places = places(&page, &notes);
        let total = page.text.chars().count();
        let job = |selection| Job {
            document: document.clone(),
            page: page.clone(),
            notes: notes.clone(),
            places: places.clone(),
            selection,
            options: Printing::default(),
            typography: Type::USUAL,
            high: false,
            date: today(),
            note_pictures: None,
        };
        let first = job(None);
        let wanted_pictures = wanted(&first, true, |_| None);
        let decoded = decode(
            wanted_pictures,
            document.address.clone(),
            first.measure(),
            Type::USUAL,
            false,
        );
        let pictures = pictures(decoded);
        let pictures_again: HashMap<(String, bool), Picture> = HashMap::new();
        let _ = std::fs::create_dir_all(out);
        for (sheet, name, selection) in [
            (
                Sheet::new(first, pictures),
                format!("{index}-all.pdf"),
                false,
            ),
            (
                Sheet::new(job(Some((total / 5, total / 5 * 2))), pictures_again),
                format!("{index}-selection.pdf"),
                true,
            ),
        ] {
            let sheet = std::rc::Rc::new(std::cell::RefCell::new(sheet));
            let operation = gtk::PrintOperation::new();
            operation.set_unit(gtk::Unit::Points);
            operation.set_use_full_page(true);
            let setup = gtk::PageSetup::new();
            setup.set_paper_size(&gtk::PaperSize::new(Some("iso_a4")));
            operation.set_default_page_setup(Some(&setup));
            let settings = gtk::PrintSettings::new();
            if selection {
                settings.set_print_pages(gtk::PrintPages::Selection);
            }
            operation.set_print_settings(Some(&settings));
            {
                let sheet = sheet.clone();
                operation.connect_begin_print(move |operation, context| {
                    let selection = operation
                        .print_settings()
                        .is_some_and(|s| s.print_pages() == gtk::PrintPages::Selection);
                    let pages = sheet.borrow_mut().lay(context, selection);
                    operation.set_n_pages(pages as i32);
                });
            }
            {
                let sheet = sheet.clone();
                operation.connect_draw_page(move |_, context, number| {
                    sheet.borrow().draw(context, number as usize);
                });
            }
            operation.set_export_filename(out.join(&name));
            operation
                .run(gtk::PrintOperationAction::Export, None::<&gtk::Window>)
                .expect("a PDF");
        }
    }
}
