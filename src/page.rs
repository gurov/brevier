//! Страница, разложенная для показа: текст, стили по участкам, ссылки,
//! якоря, оглавление и места под картинки и таблицы.
//!
//! Это то, что ядро отдаёт интерфейсу вместо строки markdown. Раньше обход
//! дерева жил в окне (`Writer` в `src/ui/gtk.rs`) и сразу писал в буфер GTK;
//! второму интерфейсу пришлось бы повторить его целиком — и два вида одной
//! статьи разошлись бы с первой же правки. Теперь обход здесь, один на оба
//! интерфейса, а интерфейс только переводит имена стилей в свои средства:
//! теги буфера у GTK, спаны у Android. Правка вида статьи — правка здесь.
//!
//! Имена стилей те же, что у тегов окна (`body`, `h2`, `list1`, `quote1`,
//! `codeblock`, `kw`…): это и есть контракт — окно накладывает участок тегом
//! с тем же именем.
//!
//! Смещения — в символах (скалярах Unicode), как у `GtkTextBuffer`. Android
//! считает в единицах UTF-16; перевод делает [`Page::to_json`].
//!
//! На месте картинки и таблицы в тексте стоит один знак объекта
//! (`U+FFFC`) — ровно так, как их ставит в буфер GTK. Смещения ссылок,
//! заголовков и совпадений поиска от этого не едут, а интерфейс знает,
//! куда положить виджет.

use comrak::nodes::{AstNode, ListType, NodeValue, TableAlignment};
use comrak::{Arena, parse_document};

use crate::Document;
use crate::address::Address;
use crate::code;
use crate::media::{self, Source};
use crate::outline::{MAX_WAYPOINTS, MIN_HEADINGS, anchor, lead};
use crate::typeset::{self, Typesetter};

/// Уровни вложенности списка, которые различаются отступом. Глубже —
/// тот же отступ: в колонке в 65 знаков лесенка съела бы текст.
pub const LIST_LEVELS: u8 = 3;
/// То же для цитат.
pub const QUOTE_LEVELS: u8 = 3;
/// Короче этого оглавление не нужно: страница вся под рукой.
pub const MIN_DOC_CHARS: usize = 4000;
/// Сколько знаков считать картинке, решая, длинна ли страница: в тексте
/// она один знак, а на экране — треть экрана.
pub const IMAGE_CHARS: usize = 500;
/// Сколько совпадений поиска подсвечивать. Больше глаз не отличит,
/// а тысячи подсветок стоят перерисовки.
pub const MAX_HITS: usize = 2000;
/// Знак объекта: место картинки или таблицы в тексте.
pub const OBJECT: char = '\u{FFFC}';

/// Чем набран участок текста.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Style {
    /// Проза: кегль текста и воздух между строками.
    Body,
    /// Строфа: строки того же кегля, что и проза, но плотнее и с висячим
    /// отступом на переносе длинной строки — печатная вёрстка стиха, а не
    /// абзац (#21). Признаётся по форме — жёсткий перенос внутри абзаца
    /// (`NodeValue::LineBreak`), который в обычной прозе почти не встречается,
    /// а после `extract::verse` это ровно строки стихотворения.
    Verse,
    /// Заголовок, уровень 1..=6.
    Heading(u8),
    Em,
    Strong,
    /// Код в строке.
    Code,
    /// Строка блока кода.
    CodeBlock,
    /// Поле панели кода сверху и снизу: пустая строка с той же подложкой.
    Pad,
    Keyword,
    Literal,
    Number,
    Comment,
    /// Цитата, уровень 1..=[`QUOTE_LEVELS`].
    Quote(u8),
    /// Пункт списка, уровень 1..=[`LIST_LEVELS`].
    List(u8),
    /// Блок кода внутри цитаты уровня 1..=[`QUOTE_LEVELS`] (#13): только
    /// поле — под текстом цитаты. Сам тег цитаты принёс бы коду курсив.
    Inset(u8),
    /// Блок кода внутри пункта списка уровня 1..=[`LIST_LEVELS`]: только
    /// поле — под текстом пункта, правее маркера. Сам тег пункта принёс бы
    /// воздух между строками.
    ItemInset(u8),
    /// Линейка цитаты уровня 1..=[`QUOTE_LEVELS`] — и больше ничего:
    /// на строках кода внутри цитаты она не должна обрываться.
    Rule(u8),
    Link,
    /// Приглушённое: маркер сноски, черта, подпись к картинке.
    Dim,
    /// Подпись оповещения (`> [!NOTE]`).
    Alert,
    /// Метка сноски в тексте.
    NoteRef,
    /// Сама сноска под статьёй.
    Note,
}

impl Style {
    /// Имя стиля — то же, что у тега в окне.
    pub fn name(self) -> String {
        match self {
            Style::Body => "body".to_owned(),
            Style::Verse => "verse".to_owned(),
            Style::Heading(level) => format!("h{level}"),
            Style::Em => "em".to_owned(),
            Style::Strong => "strong".to_owned(),
            Style::Code => "code".to_owned(),
            Style::CodeBlock => "codeblock".to_owned(),
            Style::Pad => "pad".to_owned(),
            Style::Keyword => "kw".to_owned(),
            Style::Literal => "lit".to_owned(),
            Style::Number => "num".to_owned(),
            Style::Comment => "com".to_owned(),
            Style::Quote(level) => format!("quote{level}"),
            Style::List(level) => format!("list{level}"),
            Style::Inset(level) => format!("inset{level}"),
            Style::ItemInset(level) => format!("iteminset{level}"),
            Style::Rule(level) => format!("rule{level}"),
            Style::Link => "link".to_owned(),
            Style::Dim => "dim".to_owned(),
            Style::Alert => "alert".to_owned(),
            Style::NoteRef => "noteref".to_owned(),
            Style::Note => "note".to_owned(),
        }
    }

    fn is_heading(self) -> bool {
        matches!(self, Style::Heading(_))
    }

    fn is_verse(self) -> bool {
        matches!(self, Style::Verse)
    }
}

/// Участок текста и чем он набран. Порядок стилей — порядок наложения:
/// позже наложенный главнее (подпись оповещения поверх курсива цитаты).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Run {
    pub start: usize,
    pub end: usize,
    pub styles: Vec<Style>,
}

/// Ссылка: где в тексте и куда ведёт.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Link {
    pub start: usize,
    pub end: usize,
    pub target: String,
}

/// Строка оглавления: что показать и куда это в тексте.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mark {
    pub level: u8,
    pub title: String,
    pub offset: usize,
    /// Настоящий заголовок автора или веха, которую поставили мы.
    pub heading: bool,
}

/// Что стоит на месте знака объекта.
#[derive(Debug, Clone, PartialEq)]
pub enum Block {
    /// Картинка. `inline` — внутри строки текста (обычно формула): роста
    /// она с текст, а не с колонку, и подписи у неё нет.
    Image {
        source: Source,
        alt: String,
        inline: bool,
    },
    Table(Table),
}

/// Таблица. Текст её живёт в ячейках, а не в тексте страницы: колонок
/// в тексте нет. Цена та же, что в окне, — поиск по странице её не видит.
#[derive(Debug, Clone, PartialEq)]
pub struct Table {
    /// Выравнивание столбцов, как его объявила сама таблица.
    pub alignments: Vec<Align>,
    pub rows: Vec<Row>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Align {
    Left,
    Center,
    Right,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    pub header: bool,
    pub cells: Vec<Cell>,
}

/// Ячейка: свой маленький текст со своими участками и ссылками.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Cell {
    pub text: String,
    pub runs: Vec<Run>,
    pub links: Vec<Link>,
}

/// Объект и его место — смещение знака объекта в тексте.
#[derive(Debug, Clone, PartialEq)]
pub struct Placed {
    pub at: usize,
    pub block: Block,
}

/// Разложенная страница.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Page {
    pub text: String,
    pub runs: Vec<Run>,
    pub links: Vec<Link>,
    /// Якоря заголовков и сносок — приведённые [`outline::anchor`].
    pub anchors: Vec<(String, usize)>,
    /// Оглавление, уже отобранное: заголовки или вехи.
    pub contents: Vec<Mark>,
    pub blocks: Vec<Placed>,
}

impl Page {
    /// Разложить документ.
    pub fn of(document: &Document) -> Page {
        let arena = Arena::new();
        let root = parse_document(&arena, &document.markdown, &crate::markdown::options());
        // Словарь переносов грузим один раз на страницу, не на слово. `None` —
        // язык неизвестен или ему нечего делать: тогда текст идёт как есть.
        let typesetter = document.lang.as_deref().and_then(Typesetter::for_language);

        let mut writer = Writer {
            page: Page::default(),
            chars: 0,
            marks: Vec::new(),
            base: &document.address,
            depth: 0,
            quotes: 0,
            inset: None,
            typeset: typesetter.as_ref(),
        };
        if let Some(archived) = &document.archived {
            writer.archived(archived);
        }
        // Где начинается сам текст: строка над копией из архива — не его часть.
        let start = writer.chars;
        for node in root.children() {
            writer.block(node, &[]);
        }
        if let Some(next) = &document.next {
            writer.next(next);
        }

        let mut page = writer.page;
        // Картинка занимает экран, но в тексте это один знак: страница
        // из десяти карточек с фотографиями «коротка» по знакам и осталась бы
        // без оглавления. Считаем картинке её место.
        let images = page
            .blocks
            .iter()
            .filter(|placed| matches!(placed.block, Block::Image { .. }))
            .count();
        let space = writer.chars + images * IMAGE_CHARS;
        page.contents = contents_of(writer.marks, space, start);
        page
    }

    /// Страница-сообщение: заголовок и пояснение. Так показывается отказ —
    /// тем же трактом, что и статья, чтобы и шрифт, и поля были те же.
    pub fn message(headline: &str, detail: &str) -> Page {
        let mut page = Page::default();
        let mut chars = 0;
        let mut put = |text: &str, styles: Vec<Style>| {
            let start = chars;
            chars += text.chars().count();
            page.text.push_str(text);
            page.runs.push(Run {
                start,
                end: chars,
                styles,
            });
        };
        put(headline, vec![Style::Heading(2)]);
        if !detail.is_empty() {
            put("\n\n", vec![]);
            put(detail, vec![Style::Body]);
        }
        page
    }

    /// Страница в JSON для интерфейса на другой платформе.
    ///
    /// Смещения переведены в единицы UTF-16 — так считают строки Java
    /// и JavaScript. Своего разборщика JSON в ядре нет и не нужно: писать
    /// его проще, чем тянуть `serde` в тракт, где его нет.
    pub fn to_json(&self) -> String {
        let units = Units::of(&self.text);
        let mut out = String::with_capacity(self.text.len() * 2);
        out.push_str("{\"text\":");
        json_string(&mut out, &self.text);

        out.push_str(",\"runs\":");
        runs_json(&mut out, &self.runs, &units);

        out.push_str(",\"links\":");
        links_json(&mut out, &self.links, &units);

        out.push_str(",\"anchors\":[");
        for (index, (name, at)) in self.anchors.iter().enumerate() {
            if index > 0 {
                out.push(',');
            }
            out.push('[');
            json_string(&mut out, name);
            out.push_str(&format!(",{}]", units.at(*at)));
        }
        out.push(']');

        out.push_str(",\"contents\":[");
        for (index, mark) in self.contents.iter().enumerate() {
            if index > 0 {
                out.push(',');
            }
            out.push_str(&format!("{{\"level\":{},\"title\":", mark.level));
            json_string(&mut out, &mark.title);
            out.push_str(&format!(
                ",\"at\":{},\"heading\":{}}}",
                units.at(mark.offset),
                mark.heading
            ));
        }
        out.push(']');

        out.push_str(",\"blocks\":[");
        for (index, placed) in self.blocks.iter().enumerate() {
            if index > 0 {
                out.push(',');
            }
            out.push_str(&format!("{{\"at\":{},", units.at(placed.at)));
            match &placed.block {
                Block::Image {
                    source,
                    alt,
                    inline,
                } => {
                    out.push_str("\"kind\":\"image\",\"source\":");
                    json_string(&mut out, &source.display());
                    out.push_str(",\"alt\":");
                    json_string(&mut out, alt);
                    out.push_str(&format!(",\"inline\":{inline}}}"));
                }
                Block::Table(table) => {
                    out.push_str("\"kind\":\"table\",\"align\":[");
                    for (index, align) in table.alignments.iter().enumerate() {
                        if index > 0 {
                            out.push(',');
                        }
                        out.push_str(match align {
                            Align::Left => "\"left\"",
                            Align::Center => "\"center\"",
                            Align::Right => "\"right\"",
                        });
                    }
                    out.push_str("],\"rows\":[");
                    for (index, row) in table.rows.iter().enumerate() {
                        if index > 0 {
                            out.push(',');
                        }
                        out.push_str(&format!("{{\"header\":{},\"cells\":[", row.header));
                        for (index, cell) in row.cells.iter().enumerate() {
                            if index > 0 {
                                out.push(',');
                            }
                            let units = Units::of(&cell.text);
                            out.push_str("{\"text\":");
                            json_string(&mut out, &cell.text);
                            out.push_str(",\"runs\":");
                            runs_json(&mut out, &cell.runs, &units);
                            out.push_str(",\"links\":");
                            links_json(&mut out, &cell.links, &units);
                            out.push('}');
                        }
                        out.push_str("]}");
                    }
                    out.push_str("]}");
                }
            }
        }
        out.push_str("]}");
        out
    }
}

/// Оглавление из того, что встретилось при раскладке.
///
/// Заголовки берём как есть — их место в тексте известно точно, поэтому
/// прыжок попадает в заголовок, а не примерно туда. Если заголовков мало,
/// вехами служат начала абзацев, расставленные по документу примерно
/// поровну. Короткая страница не получает оглавления вовсе.
pub fn contents_of(marks: Vec<Mark>, total: usize, start: usize) -> Vec<Mark> {
    if total < MIN_DOC_CHARS {
        return Vec::new();
    }

    let mut headings: Vec<Mark> = marks.iter().filter(|mark| mark.heading).cloned().collect();
    // Название статьи — не раздел: оно и так наверху. Наверху — значит
    // в начале текста, а не страницы: над копией из архива стоит строка
    // о ней (`start`).
    if matches!(headings.first(), Some(first) if first.level == 1 && first.offset == start) {
        headings.remove(0);
    }
    if headings.len() >= MIN_HEADINGS {
        return headings;
    }

    let leads: Vec<&Mark> = marks.iter().filter(|mark| !mark.heading).collect();
    if leads.is_empty() {
        return Vec::new();
    }
    let wanted = (total / MIN_DOC_CHARS.max(1) + 1).clamp(2, MAX_WAYPOINTS);

    let mut chosen: Vec<Mark> = Vec::with_capacity(wanted);
    let mut taken = 0usize;
    for step in 0..wanted {
        let target = (total * (step * 2 + 1) / (wanted * 2)) as i64;
        let Some((index, mark)) = leads
            .iter()
            .enumerate()
            .skip(taken)
            .min_by_key(|(_, mark)| (mark.offset as i64 - target).abs())
        else {
            break;
        };
        taken = index + 1;
        chosen.push((*mark).clone());
    }
    chosen
}

/// Совпадения поиска по «чистому» тексту: без мягких переносов и с обычным
/// пробелом вместо неразрывного, — иначе «в лесу» не нашлось бы в
/// «в\u{a0}ле\u{ad}су». Смещения — в символах исходного текста, чтобы
/// подсветить найденное на месте. Регистр не важен.
pub fn hits(full: &str, needle: &str) -> Vec<(usize, usize)> {
    let wanted: Vec<char> = typeset::plain(needle).chars().map(lower1).collect();
    if wanted.is_empty() {
        return Vec::new();
    }

    let mut plain: Vec<char> = Vec::with_capacity(full.len());
    let mut map: Vec<usize> = Vec::with_capacity(full.len());
    for (offset, ch) in full.chars().enumerate() {
        match ch {
            // Мягкий перенос и знак объекта в поиске не участвуют.
            '\u{00AD}' | OBJECT => {}
            '\u{00A0}' => {
                plain.push(' ');
                map.push(offset);
            }
            _ => {
                plain.push(lower1(ch));
                map.push(offset);
            }
        }
    }

    let mut hits = Vec::new();
    let mut i = 0;
    while i + wanted.len() <= plain.len() {
        if plain[i..i + wanted.len()] == wanted[..] {
            // Конец — сразу за последним совпавшим символом: так подсветка
            // накрывает и мягкий перенос внутри слова, если он там.
            hits.push((map[i], map[i + wanted.len() - 1] + 1));
            if hits.len() >= MAX_HITS {
                break;
            }
            i += wanted.len();
        } else {
            i += 1;
        }
    }
    hits
}

/// Первый символ нижнего регистра — один к одному, чтобы карта смещений
/// не разъехалась. Расширяющиеся отображения (редкие) сводим к первому.
fn lower1(ch: char) -> char {
    ch.to_lowercase().next().unwrap_or(ch)
}

/// Ссылка внутрь открытой страницы: `#anchor` или полный адрес с решёткой,
/// совпадающий с тем, что уже открыто. Возвращает якорь, приведённый так же,
/// как якоря страницы, — сравнивать его можно прямо со списком.
pub fn fragment_of(target: &str, here: Option<&str>) -> Option<String> {
    if let Some(fragment) = target.strip_prefix('#') {
        return (!fragment.is_empty()).then(|| anchor(fragment));
    }
    let (page, fragment) = target.split_once('#')?;
    let here = here?;
    let here = here.split('#').next().unwrap_or(here);
    (!fragment.is_empty() && page.trim_end_matches('/') == here.trim_end_matches('/'))
        .then(|| anchor(fragment))
}

/// Решётка в адресе, если она там есть: по ней прокручивают страницу
/// после загрузки. Приведена, как и якоря страницы.
pub fn anchor_in(address: &Address) -> Option<String> {
    match address {
        Address::Web(url) => url
            .split_once('#')
            .map(|(_, fragment)| fragment)
            .filter(|fragment| !fragment.is_empty())
            .map(anchor),
        _ => None,
    }
}

/// Исходник формулы из `alt`: MathJax заворачивает его в `{\displaystyle …}`,
/// и читателю эта обёртка не нужна. Им формула показывается, пока картинки
/// нет.
pub fn formula(alt: &str) -> String {
    let text = alt.trim();
    let inner = text
        .strip_prefix('{')
        .and_then(|text| text.strip_suffix('}'))
        .map(|text| text.trim())
        .and_then(|text| text.strip_prefix("\\displaystyle").or(Some(text)))
        .unwrap_or(text);
    let inner = inner.trim();
    if inner.is_empty() {
        "formula".to_owned()
    } else {
        inner.to_owned()
    }
}

struct Writer<'a> {
    page: Page,
    /// Длина текста в символах: смещения считаются в них.
    chars: usize,
    /// Все заголовки и вехи подряд; оглавление выбирается из них в конце.
    marks: Vec<Mark>,
    /// Адрес документа: от него разворачиваются относительные ссылки картинок.
    base: &'a Address,
    /// Глубина вложенности списка: от неё отступ пункта.
    depth: u8,
    /// Глубина вложенности цитаты: от неё отступ и место линейки.
    quotes: u8,
    /// Поле, под которым стоит текст ближайшей цитаты или пункта, —
    /// его берёт блок кода внутри них (#13). `None` — у края колонки.
    inset: Option<Style>,
    /// Типографика по языку страницы, если он известен. Расставляет мягкие
    /// переносы и клеит однобуквенные предлоги — только в прозе, не в коде
    /// и не в заголовках (иначе поехали бы якоря, что считаются по тексту).
    typeset: Option<&'a Typesetter>,
}

impl Writer<'_> {
    fn put(&mut self, text: &str, styles: &[Style]) {
        if text.is_empty() {
            return;
        }
        let start = self.chars;
        self.chars += text.chars().count();
        self.page.text.push_str(text);
        // Соседние куски одного набора — один участок: интерфейсу меньше
        // спанов, а смысл тот же.
        if let Some(last) = self.page.runs.last_mut()
            && last.end == start
            && last.styles == styles
        {
            last.end = self.chars;
            return;
        }
        self.page.runs.push(Run {
            start,
            end: self.chars,
            styles: styles.to_vec(),
        });
    }

    /// Строка под текстом: следующая страница, которую назвала сама страница
    /// (`rel="next"`, #35). Дочитал — и дальше не надо искать её в меню.
    /// Ссылка обычная: её открывают щелчком, Tab и Enter, нажатием на телефоне.
    fn next(&mut self, next: &crate::Link) {
        if !self.starts_line() {
            self.put("\n", &[Style::Body]);
        }
        self.put("Next: ", &[Style::Body, Style::Dim]);
        let start = self.chars;
        self.put(&next.title, &[Style::Body, Style::Link]);
        self.page.links.push(Link {
            start,
            end: self.chars,
            target: next.address.clone(),
        });
        self.put("\n", &[Style::Body]);
    }

    /// Строка над копией из архива (#8): чья это копия и откуда. Блёклая,
    /// как «Next:», — это не слова автора; адрес — ссылкой на страницу
    /// в сети, какой она стала.
    fn archived(&mut self, archived: &crate::Archived) {
        self.put(
            &format!("Your copy from {} · ", archived.read),
            &[Style::Body, Style::Dim],
        );
        let start = self.chars;
        self.put(
            &crate::store::source_of(&archived.source),
            &[Style::Body, Style::Link],
        );
        self.page.links.push(Link {
            start,
            end: self.chars,
            target: archived.source.clone(),
        });
        self.put("\n\n", &[Style::Body]);
    }

    /// Где сейчас конец текста: в символах и в байтах.
    fn here(&self) -> (usize, usize) {
        (self.chars, self.page.text.len())
    }

    fn starts_line(&self) -> bool {
        self.page.text.is_empty() || self.page.text.ends_with('\n')
    }

    /// Место под объект: своя строка. Картинка и таблица — блоки; внутри
    /// абзаца их ставят редко, а разорванная надвое строка читается плохо.
    fn object(&mut self, block: Block) {
        if !self.starts_line() {
            self.put("\n", &[]);
        }
        let at = self.chars;
        self.put(&OBJECT.to_string(), &[]);
        self.page.blocks.push(Placed { at, block });
        self.put("\n", &[Style::Body]);
    }

    /// Картинка: иллюстрация — своей строкой, формула — прямо в строке.
    fn image(&mut self, source: Source, alt: String, inline: bool) {
        if inline {
            let at = self.chars;
            self.put(&OBJECT.to_string(), &[]);
            self.page.blocks.push(Placed {
                at,
                block: Block::Image {
                    source,
                    alt,
                    inline,
                },
            });
        } else {
            self.object(Block::Image {
                source,
                alt,
                inline,
            });
        }
    }

    fn table<'n>(&mut self, node: &'n AstNode<'n>, alignments: &[TableAlignment]) {
        let mut rows = Vec::new();
        for row in node.children() {
            let NodeValue::TableRow(header) = row.data.borrow().value else {
                continue;
            };
            let cells: Vec<Cell> = row.children().map(cell_of).collect();
            if !cells.is_empty() {
                rows.push(Row { header, cells });
            }
        }
        if rows.is_empty() {
            return;
        }
        let alignments = alignments
            .iter()
            .map(|align| match align {
                TableAlignment::Right => Align::Right,
                TableAlignment::Center => Align::Center,
                _ => Align::Left,
            })
            .collect();
        self.object(Block::Table(Table { alignments, rows }));
    }

    fn block<'n>(&mut self, node: &'n AstNode<'n>, outer: &[Style]) {
        match &node.data.borrow().value {
            NodeValue::Heading(heading) => {
                let level = heading.level.clamp(1, 6);
                let mut styles = outer.to_vec();
                styles.push(Style::Heading(level));
                let (start, from) = self.here();
                self.inlines(node, &styles);
                // Знак объекта — место формулы в строке, а не буква: в подпись
                // на полке он попал бы квадратиком.
                let title = without_objects(&self.page.text[from..]);
                self.page.anchors.push((anchor(&title), start));
                self.marks.push(Mark {
                    level,
                    title,
                    offset: start,
                    heading: true,
                });
                self.put("\n", &[]);
            }
            NodeValue::Paragraph => {
                // Строфа опознаётся по форме: жёсткий перенос внутри абзаца
                // почти никогда не бывает в обычной прозе, а после
                // `extract::verse` это ровно строки стихотворения.
                let body = if is_verse(node) {
                    Style::Verse
                } else {
                    Style::Body
                };
                let mut styles = outer.to_vec();
                styles.push(body);
                let (start, from) = self.here();
                self.inlines(node, &styles);
                // Веха берётся по чистому тексту: проза уже с мягкими
                // переносами, а они и раздули бы длину, и попали бы в подпись.
                let text = typeset::plain(&without_objects(&self.page.text[from..]));
                // Вехой может быть только настоящий абзац: у короткой
                // строки начало ничего не говорит.
                if text.chars().count() >= 120 {
                    self.marks.push(Mark {
                        level: 1,
                        title: lead(&text),
                        offset: start,
                        heading: false,
                    });
                }
                self.put("\n", &[body]);
                // Строфу от следующей строфы и от прозы отделяет пустая
                // строка, как в печатной вёрстке стиха: каждая строка стиха
                // уже свой абзац буфера, и воздуха абзаца на это не хватит.
                if body == Style::Verse && node.next_sibling().is_some() {
                    self.put("\n", &[Style::Verse]);
                }
            }
            NodeValue::CodeBlock(code) => {
                let text = code.literal.trim_end_matches('\n');
                // Внутри цитаты или пункта панель встаёт под их текст,
                // а линейки цитат идут мимо неё не обрываясь (#13).
                let frame: Vec<Style> = self
                    .inset
                    .into_iter()
                    .chain((1..=self.quotes.min(QUOTE_LEVELS)).map(Style::Rule))
                    .collect();
                let with = |first: &[Style]| [first, &frame].concat();
                self.put("\n", &with(&[Style::Pad]));

                let mut at = 0;
                for span in code::spans(text, &code.info) {
                    if span.start > at {
                        self.put(&text[at..span.start], &with(&[Style::CodeBlock]));
                    }
                    let paint = match span.kind {
                        code::Kind::Comment => Style::Comment,
                        code::Kind::Literal => Style::Literal,
                        code::Kind::Number => Style::Number,
                        code::Kind::Keyword => Style::Keyword,
                    };
                    self.put(
                        &text[span.start..span.end],
                        &with(&[Style::CodeBlock, paint]),
                    );
                    at = span.end;
                }
                if at < text.len() {
                    self.put(&text[at..], &with(&[Style::CodeBlock]));
                }

                self.put("\n", &with(&[Style::CodeBlock]));
                self.put("\n", &with(&[Style::Pad]));
            }
            NodeValue::BlockQuote => {
                self.quotes += 1;
                let level = self.quotes.min(QUOTE_LEVELS);
                let mut styles = outer.to_vec();
                styles.push(Style::Quote(level));
                let was = self.inset.replace(Style::Inset(level));
                for child in node.children() {
                    self.block(child, &styles);
                }
                self.inset = was;
                self.quotes -= 1;
            }
            // Оповещение (`> [!NOTE]`) — та же цитата, но с подписью, чем
            // она является. Github рисует её коробкой в цвет; цвет тут был бы
            // чужой типографикой, а подпись — смыслом.
            NodeValue::Alert(alert) => {
                self.quotes += 1;
                let mut styles = outer.to_vec();
                styles.push(Style::Quote(self.quotes.min(QUOTE_LEVELS)));

                let title = alert
                    .title
                    .clone()
                    .unwrap_or_else(|| alert.alert_type.default_title().to_owned());
                let mut titled = styles.clone();
                titled.push(Style::Alert);
                self.put(&title, &titled);
                self.put("\n", &titled);

                let was = self
                    .inset
                    .replace(Style::Inset(self.quotes.min(QUOTE_LEVELS)));
                for child in node.children() {
                    self.block(child, &styles);
                }
                self.inset = was;
                self.quotes -= 1;
            }
            NodeValue::List(list) => {
                let ordered = matches!(list.list_type, ListType::Ordered);
                let mut number = list.start;

                self.depth += 1;
                let level = Style::List(self.depth.min(LIST_LEVELS));
                let was = self
                    .inset
                    .replace(Style::ItemInset(self.depth.min(LIST_LEVELS)));
                for item in node.children() {
                    let mut styles = outer.to_vec();
                    styles.push(Style::Body);
                    styles.push(level);

                    let marker = match &item.data.borrow().value {
                        // Пункт списка задач: галочка вместо маркера — так его
                        // и рисуют везде, где markdown вообще про них знает.
                        NodeValue::TaskItem(done) => {
                            if done.symbol.is_some() {
                                "☑  ".to_owned()
                            } else {
                                "☐  ".to_owned()
                            }
                        }
                        _ if ordered => {
                            let marker = format!("{number}.  ");
                            number += 1;
                            marker
                        }
                        _ => "•  ".to_owned(),
                    };
                    self.put(&marker, &styles);

                    let start = self.chars;
                    for child in item.children() {
                        match &child.data.borrow().value {
                            NodeValue::Paragraph => {
                                self.inlines(child, &styles);
                                self.put("\n", &styles);
                            }
                            // Вложенный список, блок кода или цитата внутри
                            // пункта — обычный блок, только глубже.
                            _ => self.block(child, outer),
                        }
                    }
                    if self.chars == start {
                        self.put("\n", &styles);
                    }
                }
                self.inset = was;
                self.depth -= 1;

                if self.depth == 0 {
                    self.put("\n", &[Style::Body]);
                }
            }
            NodeValue::FootnoteDefinition(note) => {
                let mut styles = outer.to_vec();
                styles.push(Style::Body);
                styles.push(Style::Note);
                let start = self.chars;
                self.page
                    .anchors
                    .push((anchor(&format!("fn-{}", note.name)), start));

                let mut marker = styles.clone();
                marker.push(Style::Dim);
                self.put(&format!("{}.  ", note.name), &marker);

                for child in node.children() {
                    match &child.data.borrow().value {
                        NodeValue::Paragraph => {
                            self.inlines(child, &styles);
                            self.put(" ", &styles);
                        }
                        _ => self.block(child, outer),
                    }
                }

                // Дорога назад. Без неё сноска — тупик: истории внутри
                // страницы нет, и читатель возвращается прокруткой наугад.
                let back = self.chars;
                let mut arrow = styles.clone();
                arrow.push(Style::Link);
                // Стрелка простая, а не «↩»: у той есть эмодзи-вариант,
                // и система рисует её цветной картинкой посреди текста.
                self.put("↑", &arrow);
                self.page.links.push(Link {
                    start: back,
                    end: self.chars,
                    target: format!("#fnref-{}", note.name),
                });
                self.put("\n", &styles);
            }
            NodeValue::ThematicBreak => self.put("* * *\n\n", &[Style::Dim]),
            NodeValue::Table(table) => {
                let alignments = table.alignments.clone();
                self.table(node, &alignments);
            }
            _ => {
                for child in node.children() {
                    self.block(child, outer);
                }
            }
        }
    }

    fn inlines<'n>(&mut self, node: &'n AstNode<'n>, styles: &[Style]) {
        for child in node.children() {
            match &child.data.borrow().value {
                NodeValue::Text(text) => match self.typeset {
                    // Проза: переносы и неразрывные пробелы. Заголовки мимо —
                    // по их тексту считаются якоря и оглавление, и мягкий
                    // перенос сломал бы совпадение якоря. Стих мимо тоже:
                    // короткая строка переносов почти не просит, а перенос
                    // посреди строки стиха выглядит чужеродно (#21).
                    Some(ts)
                        if !styles
                            .iter()
                            .any(|style| style.is_heading() || style.is_verse()) =>
                    {
                        let shaped = ts.shape(text);
                        self.put(&shaped, styles);
                    }
                    _ => self.put(text, styles),
                },
                NodeValue::Code(code) => {
                    let mut with = styles.to_vec();
                    with.push(Style::Code);
                    self.put(&code.literal, &with);
                }
                NodeValue::Emph => {
                    let mut with = styles.to_vec();
                    with.push(Style::Em);
                    self.inlines(child, &with);
                }
                NodeValue::Strong => {
                    let mut with = styles.to_vec();
                    with.push(Style::Strong);
                    self.inlines(child, &with);
                }
                NodeValue::Link(link) => {
                    let start = self.chars;
                    let mut with = styles.to_vec();
                    with.push(Style::Link);
                    self.inlines(child, &with);
                    self.page.links.push(Link {
                        start,
                        end: self.chars,
                        target: link.url.clone(),
                    });
                }
                NodeValue::Image(image) => {
                    let alt = plain_text(child).trim().to_owned();
                    // Картинка одна в абзаце — иллюстрация; окружённая
                    // текстом — часть строки. Вторым способом в вебе набирают
                    // формулы: википедия печатает их картинками MathJax.
                    let inline = !stands_alone(child);
                    match media::resolve(self.base, &image.url) {
                        Some(source) => self.image(source, alt, inline),
                        // Чего сами не достанем (`data:`, `blob:`) — оставляем
                        // строкой: честнее пустой рамки.
                        None => {
                            let mut with = styles.to_vec();
                            with.push(Style::Dim);
                            let label = if alt.is_empty() {
                                "[image]".to_owned()
                            } else {
                                format!("[image: {alt}]")
                            };
                            self.put(&label, &with);
                        }
                    }
                }
                // Сноска: метка ведёт вниз, к тексту сноски, и обратно —
                // за это отвечает якорь, поставленный здесь же.
                NodeValue::FootnoteReference(note) => {
                    let start = self.chars;
                    let mut with = styles.to_vec();
                    with.push(Style::NoteRef);
                    with.push(Style::Link);
                    self.put(&note.name, &with);
                    // Якорь ставится на первой ссылке: к ней и возвращает
                    // стрелка снизу, если на сноску ссылались не раз.
                    self.page
                        .anchors
                        .push((anchor(&format!("fnref-{}", note.name)), start));
                    self.page.links.push(Link {
                        start,
                        end: self.chars,
                        target: format!("#fn-{}", note.name),
                    });
                }
                NodeValue::SoftBreak => self.put(" ", styles),
                NodeValue::LineBreak => self.put("\n", styles),
                _ => self.inlines(child, styles),
            }
        }
    }
}

/// Текст без знаков объекта — для подписей на полке. Так же текст отдаёт
/// и буфер GTK (`text` без скрытого): картинки в нём нет.
fn without_objects(text: &str) -> String {
    text.chars().filter(|ch| *ch != OBJECT).collect()
}

/// Абзац — стих: жёсткие переносы делят его на строки, и строки в основном
/// короткие (#21). `extract::verse` склеивает строфу ровно этим знаком.
///
/// Одного переноса мало: `<br>` в живом вебе ставят и между длинными
/// строками прозы (цитата у fs.blog, «заголовок пункта» и описание под ним
/// у nngroup), и вёрстка стиха — без переносов по слогам и с висячим
/// отступом — такой прозе чужая. Строка стиха редко длиннее
/// [`VERSE_LINE`] знаков, строка прозы между `<br>` — почти всегда длиннее.
fn is_verse<'n>(node: &'n AstNode<'n>) -> bool {
    let mut lines = vec![0usize];
    for child in node.children() {
        match &child.data.borrow().value {
            NodeValue::LineBreak => lines.push(0),
            _ => {
                if let Some(last) = lines.last_mut() {
                    *last += plain_text(child).chars().count();
                }
            }
        }
    }
    lines.retain(|&chars| chars > 0);
    let short = lines.iter().filter(|&&chars| chars <= VERSE_LINE).count();
    lines.len() >= 2 && short * 4 >= lines.len() * 3
}

/// Самая длинная строка, которую ещё считаем строкой стиха.
const VERSE_LINE: usize = 60;

/// Ячейка таблицы: курсив, полужирный, код и ссылки — в свой маленький текст.
fn cell_of<'n>(node: &'n AstNode<'n>) -> Cell {
    let mut cell = Cell::default();
    let mut chars = 0;
    fill_cell(node, &mut cell, &mut chars, &[]);
    cell
}

fn fill_cell<'n>(node: &'n AstNode<'n>, cell: &mut Cell, chars: &mut usize, styles: &[Style]) {
    let put = |cell: &mut Cell, chars: &mut usize, text: &str, styles: &[Style]| {
        if text.is_empty() {
            return;
        }
        let start = *chars;
        *chars += text.chars().count();
        cell.text.push_str(text);
        if let Some(last) = cell.runs.last_mut()
            && last.end == start
            && last.styles == styles
        {
            last.end = *chars;
            return;
        }
        cell.runs.push(Run {
            start,
            end: *chars,
            styles: styles.to_vec(),
        });
    };
    for child in node.children() {
        match &child.data.borrow().value {
            NodeValue::Text(text) => put(cell, chars, text, styles),
            NodeValue::Code(code) => {
                let mut with = styles.to_vec();
                with.push(Style::Code);
                put(cell, chars, &code.literal, &with);
            }
            NodeValue::Emph => {
                let mut with = styles.to_vec();
                with.push(Style::Em);
                fill_cell(child, cell, chars, &with);
            }
            NodeValue::Strong => {
                let mut with = styles.to_vec();
                with.push(Style::Strong);
                fill_cell(child, cell, chars, &with);
            }
            NodeValue::Link(link) => {
                let start = *chars;
                let mut with = styles.to_vec();
                with.push(Style::Link);
                fill_cell(child, cell, chars, &with);
                cell.links.push(Link {
                    start,
                    end: *chars,
                    target: link.url.clone(),
                });
            }
            NodeValue::Image(_) => put(cell, chars, &plain_text(child), styles),
            NodeValue::SoftBreak | NodeValue::LineBreak => put(cell, chars, " ", styles),
            _ => fill_cell(child, cell, chars, styles),
        }
    }
}

/// Стоит ли картинка в абзаце одна.
///
/// Соседи-пробелы не в счёт: `![схема](url)` на своей строке приходит
/// с переводами строк по краям, и это всё равно иллюстрация.
fn stands_alone<'n>(image: &'n AstNode<'n>) -> bool {
    let empty = |node: Option<&'n AstNode<'n>>| match node {
        None => true,
        Some(node) => match &node.data.borrow().value {
            NodeValue::Text(text) => text.trim().is_empty(),
            NodeValue::SoftBreak | NodeValue::LineBreak => true,
            _ => false,
        },
    };
    empty(image.previous_sibling()) && empty(image.next_sibling())
}

/// Текст узла без разметки — для подписей картинок и ячеек таблицы.
fn plain_text<'n>(node: &'n AstNode<'n>) -> String {
    let mut out = String::new();
    for child in node.descendants() {
        match &child.data.borrow().value {
            NodeValue::Text(text) => out.push_str(text),
            NodeValue::Code(code) => out.push_str(&code.literal),
            NodeValue::SoftBreak | NodeValue::LineBreak => out.push(' '),
            _ => {}
        }
    }
    out
}

// ── JSON ────────────────────────────────────────────────────────────────────

/// Перевод смещений из символов в единицы UTF-16.
struct Units(Vec<usize>);

impl Units {
    fn of(text: &str) -> Self {
        let mut table = Vec::with_capacity(text.len() + 1);
        let mut at = 0;
        for ch in text.chars() {
            table.push(at);
            at += ch.len_utf16();
        }
        table.push(at);
        Units(table)
    }

    fn at(&self, chars: usize) -> usize {
        self.0
            .get(chars)
            .copied()
            .unwrap_or_else(|| self.0.last().copied().unwrap_or(0))
    }
}

/// Перевести смещения в символах в единицы UTF-16 для произвольного текста —
/// тем же счётом, что и в [`Page::to_json`]. Нужен поиску: он ищет в символах,
/// а подсвечивает интерфейс, считающий в UTF-16.
pub fn utf16_offsets(text: &str, spans: &[(usize, usize)]) -> Vec<(usize, usize)> {
    let units = Units::of(text);
    spans
        .iter()
        .map(|(from, to)| (units.at(*from), units.at(*to)))
        .collect()
}

fn runs_json(out: &mut String, runs: &[Run], units: &Units) {
    out.push('[');
    for (index, run) in runs.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        out.push_str(&format!("[{},{},[", units.at(run.start), units.at(run.end)));
        for (index, style) in run.styles.iter().enumerate() {
            if index > 0 {
                out.push(',');
            }
            out.push('"');
            out.push_str(&style.name());
            out.push('"');
        }
        out.push_str("]]");
    }
    out.push(']');
}

fn links_json(out: &mut String, links: &[Link], units: &Units) {
    out.push('[');
    for (index, link) in links.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        out.push_str(&format!(
            "[{},{},",
            units.at(link.start),
            units.at(link.end)
        ));
        json_string(out, &link.target);
        out.push(']');
    }
    out.push(']');
}

/// Строка JSON в кавычках. Экранируем только то, что обязаны: кавычку,
/// обратную черту и управляющие знаки; остальное едет как есть, в UTF-8.
pub fn json_string(out: &mut String, text: &str) {
    out.push('"');
    for ch in text.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            ch if (ch as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", ch as u32)),
            ch => out.push(ch),
        }
    }
    out.push('"');
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Kind;

    fn doc(markdown: &str) -> Document {
        Document {
            address: Address::Web("https://example.com/a/page".to_owned()),
            title: String::new(),
            markdown: markdown.to_owned(),
            kind: Kind::Article,
            served: false,
            site: Vec::new(),
            feeds: Vec::new(),
            lang: None,
            next: None,
            archived: None,
        }
    }

    /// Над копией из архива — блёклая строка: когда прочитана и откуда,
    /// откуда — ссылкой на страницу в сети.
    #[test]
    fn an_archived_copy_says_whose_copy_it_is() {
        let mut document = doc("---\ntitle: \"Latency\"\n---\n\n# Latency\n\nText.\n");
        document.archived = Some(crate::Archived {
            source: "https://danluu.com/keyboard-latency/".to_owned(),
            read: "9 October 2026".to_owned(),
        });
        let page = Page::of(&document);
        assert!(
            page.text
                .starts_with("Your copy from 9 October 2026 · danluu.com\n\nLatency\n"),
            "{:?}",
            page.text
        );
        let link = &page.links[0];
        assert_eq!(link.target, "https://danluu.com/keyboard-latency/");
        // Название и над копией остаётся названием, а не разделом оглавления:
        // полка копии — та же, что у самой страницы.
        let long = format!(
            "# Latency\n\n{}",
            (1..=6)
                .map(|n| format!("## Part {n}\n\n{}\n\n", "Words of the article. ".repeat(40)))
                .collect::<String>()
        );
        let titles = |document: &Document| -> Vec<String> {
            Page::of(document)
                .contents
                .iter()
                .map(|mark| mark.title.clone())
                .collect()
        };
        let live = doc(&long);
        let mut copy = doc(&long);
        copy.archived = document.archived.clone();
        assert_eq!(titles(&copy), titles(&live));
        assert_eq!(titles(&copy)[0], "Part 1");
        assert_eq!(
            &page.text[..]
                .chars()
                .skip(link.start)
                .take(link.end - link.start)
                .collect::<String>(),
            "danluu.com"
        );
    }

    #[test]
    fn the_next_page_is_a_line_under_the_text() {
        let mut document = doc("# Part one\n\nThe text of part one.\n");
        document.next = Some(crate::Link {
            title: "Part two".to_owned(),
            address: "https://e.com/2".to_owned(),
        });
        let page = Page::of(&document);
        assert!(
            page.text
                .ends_with("The text of part one.\nNext: Part two\n"),
            "{:?}",
            page.text
        );
        let link = page.links.last().unwrap();
        assert_eq!(link.target, "https://e.com/2");
        let shown: String = page
            .text
            .chars()
            .skip(link.start)
            .take(link.end - link.start)
            .collect();
        assert_eq!(shown, "Part two");
        // Без следующей страницы строки нет.
        assert!(
            !Page::of(&doc("# Part one\n\nText.\n"))
                .text
                .contains("Next:")
        );
    }

    fn styles_at(page: &Page, offset: usize) -> Vec<String> {
        page.runs
            .iter()
            .find(|run| run.start <= offset && offset < run.end)
            .map(|run| run.styles.iter().map(|style| style.name()).collect())
            .unwrap_or_default()
    }

    fn offset_of(page: &Page, needle: &str) -> usize {
        let byte = page.text.find(needle).expect(needle);
        page.text[..byte].chars().count()
    }

    #[test]
    fn a_heading_and_a_paragraph_carry_the_window_tags() {
        let page = Page::of(&doc("## Раздел\n\nАбзац *с курсивом* и `кодом`.\n"));
        assert_eq!(page.text, "Раздел\nАбзац с курсивом и кодом.\n");
        assert_eq!(styles_at(&page, 0), vec!["h2"]);
        assert_eq!(
            styles_at(&page, offset_of(&page, "курсив")),
            vec!["body", "em"]
        );
        assert_eq!(
            styles_at(&page, offset_of(&page, "кодом")),
            vec!["body", "code"]
        );
        assert_eq!(page.anchors, vec![("раздел".to_owned(), 0)]);
    }

    #[test]
    fn links_are_placed_where_their_text_is() {
        let page = Page::of(&doc("Смотри [здесь](https://e.com/x) и дальше.\n"));
        let at = offset_of(&page, "здесь");
        assert_eq!(
            page.links,
            vec![Link {
                start: at,
                end: at + 5,
                target: "https://e.com/x".to_owned(),
            }]
        );
        assert_eq!(styles_at(&page, at), vec!["body", "link"]);
    }

    #[test]
    fn lists_quotes_and_code_get_their_levels() {
        let page = Page::of(&doc(
            "- один\n  - два\n\n> цитата\n\n```rust\nfn main() {}\n```\n",
        ));
        assert_eq!(
            styles_at(&page, offset_of(&page, "один")),
            vec!["body", "list1"]
        );
        assert_eq!(
            styles_at(&page, offset_of(&page, "два")),
            vec!["body", "list2"]
        );
        assert_eq!(
            styles_at(&page, offset_of(&page, "цитата")),
            vec!["quote1", "body"]
        );
        assert_eq!(
            styles_at(&page, offset_of(&page, "fn")),
            vec!["codeblock", "kw"]
        );
        assert!(page.text.contains("•  один"));
    }

    #[test]
    fn code_inside_an_item_or_a_quote_stands_under_its_text() {
        let page = Page::of(&doc(
            "1. Пункт:\n\n   ```\n   первый\n   ```\n\n   - глубже:\n\n     ```\n     второй\n     ```\n\n\
             > Цитата:\n>\n> ```\n> третий\n> ```\n\n\
             > [!NOTE]\n> ```\n> четвёртый\n> ```\n\n\
             - Пункт в цитате? Нет, в списке:\n\n  > ```\n  > пятый\n  > ```\n\n\
             ```\nшестой\n```\n",
        ));
        assert_eq!(
            styles_at(&page, offset_of(&page, "первый")),
            vec!["codeblock", "iteminset1"]
        );
        assert_eq!(
            styles_at(&page, offset_of(&page, "второй")),
            vec!["codeblock", "iteminset2"]
        );
        assert_eq!(
            styles_at(&page, offset_of(&page, "третий")),
            vec!["codeblock", "inset1", "rule1"]
        );
        assert_eq!(
            styles_at(&page, offset_of(&page, "четвёртый")),
            vec!["codeblock", "inset1", "rule1"]
        );
        // Ближайший контейнер решает поле, линейка — от всех цитат вокруг.
        assert_eq!(
            styles_at(&page, offset_of(&page, "пятый")),
            vec!["codeblock", "inset1", "rule1"]
        );
        // Поля панели идут вместе с кодом.
        let pad = offset_of(&page, "первый") - 1;
        assert_eq!(styles_at(&page, pad), vec!["pad", "iteminset1"]);
        // Вне контейнеров — у края, как и было.
        assert_eq!(
            styles_at(&page, offset_of(&page, "шестой")),
            vec!["codeblock"]
        );
    }

    #[test]
    fn an_illustration_takes_its_own_line_and_a_formula_stays_inline() {
        let page = Page::of(&doc(
            "Текст до.\n\n![Схема](pic.png)\n\nФормула ![x^2](f.svg) в строке.\n",
        ));
        assert_eq!(page.blocks.len(), 2);
        let first = &page.blocks[0];
        assert!(matches!(
            &first.block,
            Block::Image { source: Source::Web(url), alt, inline: false }
                if url == "https://example.com/a/pic.png" && alt == "Схема"
        ));
        // Знак объекта стоит на своей строке.
        let chars: Vec<char> = page.text.chars().collect();
        assert_eq!(chars[first.at], OBJECT);
        assert_eq!(chars[first.at - 1], '\n');
        assert_eq!(chars[first.at + 1], '\n');
        // А формула — посреди строки.
        let second = &page.blocks[1];
        assert!(matches!(&second.block, Block::Image { inline: true, .. }));
        assert_eq!(chars[second.at - 1], ' ');
    }

    #[test]
    fn a_table_is_an_object_with_rich_cells() {
        let page = Page::of(&doc(
            "| Имя | Число |\n|:--|--:|\n| **жир** | [1](https://e.com) |\n",
        ));
        let [placed] = page.blocks.as_slice() else {
            panic!("{:?}", page.blocks)
        };
        let Block::Table(table) = &placed.block else {
            panic!()
        };
        assert_eq!(table.alignments, vec![Align::Left, Align::Right]);
        assert!(table.rows[0].header);
        let cell = &table.rows[1].cells[0];
        assert_eq!(cell.text, "жир");
        assert_eq!(cell.runs[0].styles, vec![Style::Strong]);
        let link = &table.rows[1].cells[1];
        assert_eq!(link.links[0].target, "https://e.com");
    }

    #[test]
    fn footnotes_link_down_and_back() {
        let page = Page::of(&doc("Текст[^1].\n\n[^1]: Сноска.\n"));
        let targets: Vec<&str> = page.links.iter().map(|link| link.target.as_str()).collect();
        assert_eq!(targets, vec!["#fn-1", "#fnref-1"]);
        let names: Vec<&str> = page.anchors.iter().map(|(name, _)| name.as_str()).collect();
        assert!(
            names.contains(&"fnref-1") && names.contains(&"fn-1"),
            "{names:?}"
        );
    }

    #[test]
    fn a_short_page_has_no_contents_and_a_long_one_has_headings() {
        assert!(
            Page::of(&doc("# A\n\n## B\n\n## C\n\n## D\n"))
                .contents
                .is_empty()
        );

        let para = "Длинный абзац статьи, в котором знаков хватает на несколько строк \
нашей меры, иначе страница выйдет короткой и оглавления не получит.";
        let text = [para; 12].join("\n\n");
        let page = Page::of(&doc(&format!(
            "# Название\n\n{text}\n\n## Первый\n\n{text}\n\n## Второй\n\n{text}\n\n## Третий\n"
        )));
        let titles: Vec<&str> = page
            .contents
            .iter()
            .map(|mark| mark.title.as_str())
            .collect();
        assert_eq!(titles, vec!["Первый", "Второй", "Третий"]);
    }

    #[test]
    fn a_formula_does_not_leak_into_shelf_titles() {
        let lead = "начало абзаца, которое станет вехой на полке, потому что заголовков \
здесь нет, а абзац длинный и тянется на несколько строк нашей меры.";
        // Каждый абзац начинается с формулы: какой бы из них ни стал вехой,
        // знак объекта перед его первым словом.
        let text = vec![format!("![{{b}}](https://e.org/b.svg) {lead}"); 30].join("\n\n");
        let page = Page::of(&doc(&format!(
            "## Сумма ![{{x}}](https://e.org/x.svg) ряда\n\n{text}\n"
        )));
        assert_eq!(page.anchors[0], ("сумма-ряда".to_owned(), 0));
        assert!(!page.contents.is_empty());
        for mark in &page.contents {
            assert!(
                mark.title.trim_start().starts_with("начало абзаца"),
                "{:?}",
                mark.title
            );
        }
    }

    #[test]
    fn front_matter_is_not_drawn() {
        let page = Page::of(&doc(
            "---\ndescription: A page\ntitle: Its title\n---\n\n# Heading\n\nText.\n",
        ));
        assert_eq!(page.text, "Heading\nText.\n");
        assert_eq!(page.anchors, vec![("heading".to_owned(), 0)]);
    }

    #[test]
    fn prose_is_typeset_but_headings_are_not() {
        let mut document = doc("## Разделение\n\nРазделение в лесу продолжается долго.\n");
        document.lang = Some("ru".to_owned());
        let page = Page::of(&document);
        let heading = page.text.lines().next().unwrap();
        assert_eq!(heading, "Разделение");
        assert!(page.text.contains("в\u{a0}ле"), "{:?}", page.text);
        assert!(page.text.contains('\u{ad}'), "{:?}", page.text);
    }

    /// Проза между `<br>` — не стих: строки длинные.
    #[test]
    fn long_lines_with_hard_breaks_stay_prose() {
        let long = "Having no funding was a huge advantage for me, and a year after I started the dot-com boom happened.";
        let page = Page::of(&doc(&format!("{long}\\\n{long}\n")));
        assert_eq!(styles_at(&page, 0), vec!["body"]);
        // Две строки — уже строфа, если они короткие.
        let page = Page::of(&doc(
            "На берегу пустынных волн\\\nСтоял он, дум великих полн,\n",
        ));
        assert_eq!(styles_at(&page, 0), vec!["verse"]);
        // Одна строка с переносом на конце — не строфа.
        let page = Page::of(&doc("Коротко.\n"));
        assert_eq!(styles_at(&page, 0), vec!["body"]);
    }

    #[test]
    fn verse_is_its_own_style_and_is_not_typeset() {
        let mut document = doc("Разделение в лесу.  \nПродолжается долго.\n\n\
             Обычный текст: разделение в лесу продолжается долго.\n");
        document.lang = Some("ru".to_owned());
        let page = Page::of(&document);
        let lines: Vec<&str> = page.text.lines().collect();

        // Строфа без переносов и без неразрывных пробелов (#21)…
        assert_eq!(lines[..2], ["Разделение в лесу.", "Продолжается долго."]);
        assert_eq!(
            styles_at(&page, offset_of(&page, "Продолжается")),
            vec!["verse"]
        );
        // Строфу от прозы отделяет пустая строка…
        assert_eq!(lines[2], "");
        // …а проза рядом типографится, как раньше.
        let prose = lines[3];
        let prose_at = lines[0].chars().count() + lines[1].chars().count() + 3;
        assert_eq!(styles_at(&page, prose_at), vec!["body"]);
        assert!(prose.contains("в\u{a0}ле"), "{prose:?}");
        assert!(prose.contains('\u{ad}'), "{prose:?}");
    }

    /// Сквозь весь тракт: стих из веба — строки FictionBook и строки через
    /// `<br>` — доезжает до модели страницы строфой, строка к строке.
    #[test]
    fn verse_from_the_web_reaches_the_page_line_by_line() {
        let prose = "Кеннет Эрроу доказал теорему о невозможности коллективного \
            выбора, и это перевернуло теорию общественного благосостояния. \
            Ниже разбирается, что именно утверждает теорема и почему её \
            следствия так неудобны для любой процедуры голосования.";
        let shapes = [
            "<z><o></o><v>На берегу пустынных волн</v><c></c>\n\
             <v>Стоял он, дум великих полн,</v><c></c></z>",
            "<div class=\"poem\"><p>На берегу пустынных волн<br>\n\
             Стоял он, дум великих полн,</p></div>",
        ];
        for shape in shapes {
            let html = format!(
                "<html><body><article><h1>Стихи</h1><p>{prose}</p>{shape}<p>{prose}</p></article></body></html>"
            );
            let document = crate::from_html(&html, "https://example.org/a").unwrap();
            let page = Page::of(&document);
            assert!(
                page.text
                    .contains("На берегу пустынных волн\nСтоял он, дум великих полн,\n"),
                "{shape}: {:?}",
                page.text
            );
            assert_eq!(
                styles_at(&page, offset_of(&page, "Стоял")),
                vec!["verse"],
                "{shape}"
            );
        }
    }

    #[test]
    fn search_looks_through_soft_hyphens_and_nbsp() {
        let buffer = "я\u{00A0}иду в\u{00A0}ле\u{00AD}су";
        assert_eq!(hits(buffer, "в лесу"), vec![(6, 13)]);
        assert_eq!(hits(buffer, "ЛЕСУ"), vec![(8, 13)]);
        assert!(hits(buffer, "").is_empty());
        assert!(hits(buffer, "море").is_empty());
    }

    #[test]
    fn fragments_are_recognised_on_the_same_page_only() {
        let here = Some("https://e.com/a#old");
        assert_eq!(fragment_of("#Раздел", here), Some("раздел".to_owned()));
        assert_eq!(
            fragment_of("https://e.com/a/#b", here),
            Some("b".to_owned())
        );
        assert_eq!(fragment_of("https://e.com/other#b", here), None);
        assert_eq!(fragment_of("https://e.com/a", here), None);
    }

    #[test]
    fn json_counts_offsets_in_utf16() {
        // «𝔸» — два знака UTF-16: всё, что после него, сдвигается на один.
        let page = Page::of(&doc("𝔸 [ссылка](https://e.com)\n"));
        let json = page.to_json();
        assert!(
            json.contains("\"links\":[[3,9,\"https://e.com\"]]"),
            "{json}"
        );
        assert!(json.starts_with("{\"text\":\"𝔸 ссылка\\n\""), "{json}");
    }

    #[test]
    fn a_message_is_a_heading_and_a_paragraph() {
        let page = Page::message("Не открылось", "Сервер ответил 403.");
        assert_eq!(page.text, "Не открылось\n\nСервер ответил 403.");
        assert_eq!(styles_at(&page, 0), vec!["h2"]);
    }

    #[test]
    fn the_formula_wrapper_is_dropped() {
        assert_eq!(formula("{\\displaystyle b^2}"), "b^2");
        assert_eq!(formula(""), "formula");
    }

    #[test]
    fn utf16_offsets_follow_the_text() {
        assert_eq!(utf16_offsets("𝔸b", &[(1, 2)]), vec![(2, 3)]);
    }
}
