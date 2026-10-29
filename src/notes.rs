//! Заметки к странице (#39): выделил место, отметил, приписал пару слов.
//! Открыл ту же страницу — отметки на месте; сохранил её — заметки едут
//! в файл вместе с текстом.
//!
//! Решено с мейнтейнером 17 октября 2026: как в читалках книг (CoolReader),
//! но для веба. Заметки на полке — своей группой выше оглавления (оно бывает
//! длинным, и заметки под ним терялись бы), на полосе внизу — квадратиками,
//! рядом с точками разделов.
//!
//! **Привязка — цитатой, а не смещением.** Страница собирается заново
//! из сайта, который меняется, и смещение пережило бы только тот же текст.
//! Заметка хранит саму цитату, по [`CONTEXT`] знаков до и после неё
//! и смещение как подсказку — модель W3C Web Annotation (`TextQuoteSelector`
//! и `TextPositionSelector`), та же, что у Hypothesis. Цитата нашлась одна —
//! она; несколько — та, у которой совпало окружение, при равенстве —
//! ближайшая к подсказке. Не нашлась — заметка остаётся «без места»: на полке
//! и на странице заметок она есть, со своей цитатой, и молча не пропадает.
//!
//! **Текст «чистый»**, как у поиска по странице: мягкие переносы и знаки
//! объектов не считаются, неразрывный пробел — пробел. Переносы ставит
//! ядро по языку, и выделенное с ними совпадает с тем же текстом без них.
//!
//! **Считает ядро**, как и прогресс чтения: интерфейс сообщает выделение
//! в знаках текста страницы (`Page::text`; телефон переводит свои UTF-16)
//! и получает места заметок, чтобы их закрасить.
//!
//! **Файл свой:** `notes.tsv` рядом с журналом, строка на заметку.
//! «Забыть всё» его не трогает: заметки читатель написал сам, как
//! и закладки. Страница с заметками держит свою последнюю копию в архиве
//! навсегда — как страница из закладок: у заметок всегда есть текст,
//! к которому они написаны.
//!
//! **Картинки — тоже заметка читателя** (#42, 28 октября 2026): карту
//! или схему, найденную где-то ещё, кладут к абзацу. В статью она
//! не встаёт: слова и картинки автора остаются авторскими. Модель
//! W3C Web Annotation это и позволяет — тело заметки бывает картинкой.
//! Файл — в папке [`PICTURES`] рядом с `notes.tsv`, имя — по содержимому,
//! в строке заметки — только имя. Живёт, пока на него ссылается хоть одна
//! заметка; «забыть всё» его не трогает, как и сами заметки.
//!
//! **Выгрузка** — [`digest`]: заметки страницы конспектом в markdown,
//! который Obsidian и Affine читают как есть, со ссылкой на место цитаты
//! в обычном браузере.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;

use crate::page::OBJECT;
use crate::store::{self, Stamp};

/// Знаков окружения до и после цитаты: хватает, чтобы различить два
/// одинаковых предложения, и не жалко хранить.
pub const CONTEXT: usize = 32;

/// Длиннее цитата — уже не место в тексте, а сам текст.
pub const MAX_QUOTE: usize = 4000;

/// Сколько знаков цитаты показывать строкой полки.
pub const PREVIEW: usize = 90;

/// Папка картинок заметок — рядом с `notes.tsv`.
pub const PICTURES: &str = "notes";

/// Потолок картинки в заметке — уже после уменьшения: тот же, что
/// у картинки со страницы (`media::MAX_IMAGE`).
pub const MAX_PICTURE: usize = 8 * 1024 * 1024;

/// Ссылка на место цитаты: длинная цитата — столько слов с начала
/// и столько с конца.
const FRAGMENT_WORDS: usize = 4;

/// Заметка: где она и что в ней.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Note {
    pub id: u64,
    pub stamp: Stamp,
    /// Адрес страницы без `#якоря` — как в журнале и закладках.
    pub address: String,
    /// Заголовок страницы, когда заметку оставили: страница заметок пишет
    /// его, не открывая саму страницу.
    pub title: String,
    /// Смещение начала цитаты в знаках текста страницы — подсказка.
    pub at: usize,
    pub before: String,
    pub quote: String,
    pub after: String,
    /// Что приписал читатель. Пусто — просто отметка.
    pub text: String,
    /// Картинки читателя: имена файлов в папке [`PICTURES`], по порядку.
    pub pictures: Vec<String>,
}

impl Note {
    /// Есть что показать рядом с текстом: слова или картинка. Без них
    /// заметка — просто отметка, и место ей — подложка в самом тексте.
    pub fn says(&self) -> bool {
        !self.text.is_empty() || !self.pictures.is_empty()
    }
}

/// Выделенное, готовое стать заметкой.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Selection {
    pub at: usize,
    pub before: String,
    pub quote: String,
    pub after: String,
}

/// Почему выделенное заметкой не стало.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    /// Ничего, кроме пробелов, переносов и картинок.
    Empty,
    /// Длиннее [`MAX_QUOTE`].
    TooLong,
}

impl Refusal {
    /// Что сказать читателю.
    pub fn headline(self) -> &'static str {
        match self {
            Refusal::Empty => "Select some text first",
            Refusal::TooLong => "Select less: a note marks a passage, not a page",
        }
    }
}

/// Почему картинка в заметку не легла.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unkept {
    /// Не PNG, JPEG, GIF или WebP — или не картинка вовсе.
    NotAPicture,
    /// Больше [`MAX_PICTURE`] и после уменьшения.
    TooLarge,
    /// Заметки уже нет или файл не записался.
    Nowhere,
}

impl Unkept {
    /// Что сказать читателю.
    pub fn headline(self) -> &'static str {
        match self {
            Unkept::NotAPicture => "A note takes PNG, JPEG, GIF and WebP pictures",
            Unkept::TooLarge => "The picture is too large for a note",
            Unkept::Nowhere => "The picture could not be kept",
        }
    }
}

/// Тип картинки по подписи в первых байтах — расширение файла. Только
/// растр, который разбирает `media`: svg из буфера обмена — редкость,
/// а вектор — своя поверхность атаки (`media::vector`).
pub fn picture_kind(bytes: &[u8]) -> Option<&'static str> {
    let starts = |magic: &[u8]| bytes.starts_with(magic);
    if starts(b"\x89PNG\r\n\x1a\n") {
        Some("png")
    } else if starts(b"\xff\xd8\xff") {
        Some("jpg")
    } else if starts(b"GIF87a") || starts(b"GIF89a") {
        Some("gif")
    } else if starts(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") {
        Some("webp")
    } else {
        None
    }
}

/// Имя картинки — по содержимому: та же картинка у двух заметок — один
/// файл, и имя не выдаёт ничего, кроме себя.
fn picture_name(bytes: &[u8], kind: &str) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    format!("{hash:016x}.{kind}")
}

/// Наше ли это имя: шестнадцать цифр и расширение картинки. Строку
/// `notes.tsv` правят руками, и `../` из неё не должно увести из папки.
pub fn is_picture_name(name: &str) -> bool {
    let Some((stem, kind)) = name.split_once('.') else {
        return false;
    };
    stem.len() == 16
        && stem.bytes().all(|b| b.is_ascii_hexdigit())
        && matches!(kind, "png" | "jpg" | "gif" | "webp")
}

/// Картинка для заметки — уменьшенная, если «совсем большая» (решение
/// мейнтейнера 29 октября 2026: только такие). Что такое «совсем» —
/// у окна и телефона своё: [`media::Shrink`]. Не картинка или не влезла
/// и после — отказ.
#[cfg(feature = "images")]
pub fn fitted(bytes: &[u8], limit: crate::media::Shrink) -> Result<Vec<u8>, Unkept> {
    if picture_kind(bytes).is_none() {
        return Err(Unkept::NotAPicture);
    }
    let bytes = match crate::media::shrink(bytes, limit) {
        Ok(Some(smaller)) => smaller,
        Ok(None) => bytes.to_vec(),
        Err(_) => return Err(Unkept::NotAPicture),
    };
    if bytes.len() > MAX_PICTURE {
        return Err(Unkept::TooLarge);
    }
    Ok(bytes)
}

/// Где заметка на открытой странице: `[from, to)` в знаках текста страницы.
/// `None` — такой цитаты на странице больше нет.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Spot {
    pub id: u64,
    pub place: Option<(usize, usize)>,
}

/// Знак в «чистом» тексте: мягкий перенос и объект — ничто, узкие
/// и неразрывные пробелы — пробел.
fn clean(ch: char) -> Option<char> {
    match ch {
        '\u{00AD}' | OBJECT => None,
        '\u{00A0}' | '\u{202F}' | '\u{2007}' => Some(' '),
        _ => Some(ch),
    }
}

/// Цитата и окружение выделения `[from, to)` в знаках страницы. Пробелы
/// по краям выделения — не часть цитаты: палец и мышь цепляют их сами.
pub fn select(page: &str, from: usize, to: usize) -> Result<Selection, Refusal> {
    let chars: Vec<char> = page.chars().collect();
    let (mut from, mut to) = (from.min(chars.len()), to.min(chars.len()));
    let blank = |ch: char| clean(ch).is_none_or(char::is_whitespace);
    while from < to && blank(chars[from]) {
        from += 1;
    }
    while to > from && blank(chars[to - 1]) {
        to -= 1;
    }
    if from >= to {
        return Err(Refusal::Empty);
    }
    let quote: String = chars[from..to].iter().filter_map(|&ch| clean(ch)).collect();
    if quote.chars().count() > MAX_QUOTE {
        return Err(Refusal::TooLong);
    }
    let before: Vec<char> = chars[..from]
        .iter()
        .rev()
        .filter_map(|&ch| clean(ch))
        .take(CONTEXT)
        .collect();
    let after: String = chars[to..]
        .iter()
        .filter_map(|&ch| clean(ch))
        .take(CONTEXT)
        .collect();
    Ok(Selection {
        at: from,
        before: before.into_iter().rev().collect(),
        quote,
        after,
    })
}

/// Найти заметки на странице. Порядок ответа — порядок `notes`.
pub fn place(page: &str, notes: &[&Note]) -> Vec<Spot> {
    if notes.is_empty() {
        return Vec::new();
    }
    let plain = Plain::of(page);
    notes
        .iter()
        .map(|note| Spot {
            id: note.id,
            place: plain.find(note),
        })
        .collect()
}

/// Текст страницы для поиска цитат: «чистый» и с картой обратно в знаки
/// страницы. Строится раз на страницу, а не на заметку.
struct Plain {
    text: String,
    /// Для каждого знака `text` — его номер в тексте страницы.
    map: Vec<usize>,
}

impl Plain {
    fn of(page: &str) -> Self {
        let mut text = String::with_capacity(page.len());
        let mut map = Vec::with_capacity(page.len());
        for (offset, ch) in page.chars().enumerate() {
            if let Some(ch) = clean(ch) {
                text.push(ch);
                map.push(offset);
            }
        }
        Plain { text, map }
    }

    fn find(&self, note: &Note) -> Option<(usize, usize)> {
        if note.quote.is_empty() {
            return None;
        }
        let length = note.quote.chars().count();
        // (совпало окружения, расстояние до подсказки, номер знака)
        let mut best: Option<(usize, usize, usize)> = None;
        let mut consider = |byte: usize, chars: usize| {
            let around = common_suffix(&self.text[..byte], &note.before)
                + common_prefix(&self.text[byte + note.quote.len()..], &note.after);
            let distance = self.map[chars].abs_diff(note.at);
            let better = match best {
                None => true,
                Some((score, far, _)) => around > score || (around == score && distance < far),
            };
            if better {
                best = Some((around, distance, chars));
            }
        };
        let (mut chars, mut bytes) = (0, 0);
        for (byte, _) in self.text.match_indices(note.quote.as_str()) {
            chars += self.text[bytes..byte].chars().count();
            bytes = byte;
            consider(byte, chars);
        }
        // И само место, где заметку оставили. `match_indices` перекрытий
        // не видит, а цитата внутри ряда одинаковых знаков («……», «====»)
        // перекрывается сама с собой: настоящее место оказывалось между
        // найденными, и заметка вставала на соседнее (нашёл фаззинг).
        // Перебирать каждую позицию — квадрат на таком ряду; подсказки
        // хватает, пока страница та же.
        if let Ok(at) = self.map.binary_search(&note.at)
            && let Some((byte, _)) = self.text.char_indices().nth(at)
            && self.text[byte..].starts_with(note.quote.as_str())
        {
            consider(byte, at);
        }
        let (_, _, start) = best?;
        Some((self.map[start], self.map[start + length - 1] + 1))
    }
}

fn common_suffix(text: &str, wanted: &str) -> usize {
    text.chars()
        .rev()
        .zip(wanted.chars().rev())
        .take_while(|(a, b)| a == b)
        .count()
}

fn common_prefix(text: &str, wanted: &str) -> usize {
    text.chars()
        .zip(wanted.chars())
        .take_while(|(a, b)| a == b)
        .count()
}

/// Цитата строкой полки: начало, одной строкой, с многоточием, если длиннее.
pub fn preview(quote: &str) -> String {
    let flat = quote.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() <= PREVIEW {
        return flat;
    }
    let cut: String = flat.chars().take(PREVIEW).collect();
    // По слову, а не посреди него.
    let cut = match cut.rfind(' ') {
        Some(space) if space > PREVIEW / 2 => cut[..space].to_owned(),
        _ => cut,
    };
    format!("{}…", cut.trim_end_matches([',', '.', ';', ':', ' ']))
}

fn without_fragment(address: &str) -> &str {
    address.split('#').next().unwrap_or(address)
}

/// Страницы, чья последняя копия в архиве живёт навсегда: закладки
/// и страницы с заметками. Этим списком архив убирает старое и «забывает всё».
pub fn lasting() -> HashSet<String> {
    let mut addresses = store::Marks::open().addresses();
    addresses.extend(Notes::open().addresses());
    addresses
}

/// `notes.tsv`: все заметки, в порядке появления.
#[derive(Debug, Default)]
pub struct Notes {
    path: Option<PathBuf>,
    entries: Vec<Note>,
}

impl Notes {
    pub fn open() -> Self {
        match store::data_dir() {
            Some(dir) => Self::at(dir.join("notes.tsv")),
            None => Self::default(),
        }
    }

    pub fn at(path: impl Into<PathBuf>) -> Self {
        let path = path.into();
        let text = fs::read_to_string(&path).unwrap_or_default();
        Self {
            entries: text.lines().filter_map(parse_line).collect(),
            path: Some(path),
        }
    }

    pub fn all(&self) -> &[Note] {
        &self.entries
    }

    /// Заметки страницы, в порядке появления.
    pub fn of(&self, address: &str) -> Vec<&Note> {
        let address = without_fragment(address);
        self.entries
            .iter()
            .filter(|note| note.address == address)
            .collect()
    }

    pub fn get(&self, id: u64) -> Option<&Note> {
        self.entries.iter().find(|note| note.id == id)
    }

    /// Папка картинок заметок.
    pub fn pictures_dir(&self) -> Option<PathBuf> {
        Some(self.path.as_ref()?.parent()?.join(PICTURES))
    }

    /// Файл картинки по имени из заметки. Чужое имя — `None`.
    pub fn picture(&self, name: &str) -> Option<PathBuf> {
        if !is_picture_name(name) {
            return None;
        }
        Some(self.pictures_dir()?.join(name))
    }

    /// Положить картинку к заметке. Байты — как есть: уменьшает, если
    /// надо, [`fitted`]. Ответ — имя файла.
    pub fn attach(&mut self, id: u64, bytes: &[u8]) -> Result<String, Unkept> {
        let kind = picture_kind(bytes).ok_or(Unkept::NotAPicture)?;
        if bytes.len() > MAX_PICTURE {
            return Err(Unkept::TooLarge);
        }
        if self.get(id).is_none() {
            return Err(Unkept::Nowhere);
        }
        let name = picture_name(bytes, kind);
        let path = self.picture(&name).ok_or(Unkept::Nowhere)?;
        // Та же картинка уже лежит — у другой заметки или от прошлого раза.
        if !path.is_file() {
            let dir = path.parent().ok_or(Unkept::Nowhere)?;
            fs::create_dir_all(dir).map_err(|_| Unkept::Nowhere)?;
            // Сначала рядом, потом под своё имя: оборванная запись
            // не оставит полкартинки под настоящим именем.
            let part = dir.join(format!("{name}.part"));
            if fs::write(&part, bytes)
                .and_then(|()| fs::rename(&part, &path))
                .is_err()
            {
                let _ = fs::remove_file(&part);
                return Err(Unkept::Nowhere);
            }
        }
        let Some(note) = self.entries.iter_mut().find(|note| note.id == id) else {
            return Err(Unkept::Nowhere);
        };
        if !note.pictures.contains(&name) {
            note.pictures.push(name.clone());
        }
        self.write();
        Ok(name)
    }

    /// Убрать картинку из заметки. Файл уходит, если больше ни у кого.
    pub fn detach(&mut self, id: u64, name: &str) -> bool {
        let Some(note) = self.entries.iter_mut().find(|note| note.id == id) else {
            return false;
        };
        let before = note.pictures.len();
        note.pictures.retain(|known| known != name);
        if note.pictures.len() == before {
            return false;
        }
        self.write();
        self.forget_unused(&[name.to_owned()]);
        true
    }

    /// Стереть файлы картинок, на которые больше не ссылается ни одна
    /// заметка.
    fn forget_unused(&self, names: &[String]) {
        for name in names {
            if self.entries.iter().any(|note| note.pictures.contains(name)) {
                continue;
            }
            if let Some(path) = self.picture(name) {
                let _ = fs::remove_file(path);
            }
        }
    }

    /// Новая заметка. Ответ — её номер.
    pub fn add(
        &mut self,
        address: &str,
        title: &str,
        selection: Selection,
        text: &str,
        stamp: Stamp,
    ) -> u64 {
        let id = self.entries.iter().map(|note| note.id).max().unwrap_or(0) + 1;
        self.entries.push(Note {
            id,
            stamp,
            address: without_fragment(address).to_owned(),
            title: title.split_whitespace().collect::<Vec<_>>().join(" "),
            at: selection.at,
            before: selection.before,
            quote: selection.quote,
            after: selection.after,
            text: text.trim().to_owned(),
            pictures: Vec::new(),
        });
        self.write();
        id
    }

    /// Переписать то, что приписано. Пусто — заметка становится отметкой.
    pub fn edit(&mut self, id: u64, text: &str) -> bool {
        let Some(note) = self.entries.iter_mut().find(|note| note.id == id) else {
            return false;
        };
        note.text = text.trim().to_owned();
        self.write();
        true
    }

    pub fn remove(&mut self, id: u64) -> bool {
        let Some(index) = self.entries.iter().position(|note| note.id == id) else {
            return false;
        };
        let gone = self.entries.remove(index);
        self.write();
        self.forget_unused(&gone.pictures);
        true
    }

    /// Адреса страниц с заметками: архив держит их копии.
    pub fn addresses(&self) -> HashSet<String> {
        self.entries
            .iter()
            .map(|note| note.address.clone())
            .collect()
    }

    /// Страница `brevier:notes`: заметки по страницам, свежие страницы
    /// сверху, внутри страницы — в порядке текста. `copies` — последние
    /// копии страниц в архиве (`Archive::latest_of`), как у закладок.
    pub fn page(&self, copies: &HashMap<String, crate::archive::Copy>) -> String {
        let mut out = String::from("# Your notes\n\n");
        if self.entries.is_empty() {
            out.push_str(
                "Nothing here yet. Select a passage in an article and choose **Highlight** \
                 to mark it, or **Note** to write a few words next to it. The page keeps \
                 its marks, and they are listed here and on the shelf.\n",
            );
            return out;
        }
        let mut pages: Vec<&str> = Vec::new();
        for note in self.entries.iter().rev() {
            if !pages.contains(&note.address.as_str()) {
                pages.push(&note.address);
            }
        }
        for address in pages {
            let mut notes = self.of(address);
            notes.sort_by_key(|note| note.at);
            let Some(last) = notes.iter().max_by_key(|note| note.id) else {
                continue;
            };
            let title = if last.title.is_empty() {
                address.to_owned()
            } else {
                last.title.clone()
            };
            out.push_str(&format!("## {}\n\n", store::link(&title, address)));
            let mut line = format!(
                "{} · {}",
                store::source_of(address),
                crate::archive::long_date(&last.stamp)
            );
            if let Some(copy) = copies.get(address) {
                line.push_str(&format!(
                    " · {}",
                    store::link(
                        &format!("your copy from {}", crate::archive::long_date(&copy.read)),
                        &crate::archive::copy_address(&copy.path),
                    )
                ));
            }
            out.push_str(&format!("{line}\n\n"));
            // Картинки — из своей папки: `media::resolve` пускает к ним
            // только эту страницу.
            out.push_str(&entries(&notes, PICTURES));
        }
        if let Some(path) = &self.path {
            out.push_str(&format!(
                "---\n\nThe notes are a plain text file: {}.\n",
                store::where_it_is(path),
            ));
        }
        out
    }

    fn write(&self) {
        let Some(path) = &self.path else { return };
        if let Some(dir) = path.parent()
            && fs::create_dir_all(dir).is_err()
        {
            return;
        }
        let text: String = self.entries.iter().map(line_of).collect();
        let _ = fs::write(path, text);
    }
}

/// Заметки markdown-ом: цитата цитатой, приписанное — абзацем под ней,
/// картинки — ссылками в папку `folder`. Читается где угодно; Obsidian
/// и Logseq берут такое как есть.
pub fn entries(notes: &[&Note], folder: &str) -> String {
    let mut out = String::new();
    for note in notes {
        out.push_str(&quoted(&note.quote, None));
        out.push_str(&said(note, folder));
    }
    out
}

/// Цитата цитатой markdown, а под ней, если есть, ссылка на её место.
fn quoted(quote: &str, place: Option<&str>) -> String {
    let mut lines: Vec<String> = quote
        .split('\n')
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(|line| format!("> {}", crate::feed::block(line)))
        .collect();
    if let Some(place) = place {
        lines.push(format!(
            "> — [on the page]({})",
            crate::feed::destination(place)
        ));
    }
    lines.join("\n>\n") + "\n\n"
}

/// Что читатель приписал: слова абзацем, картинки — каждая своим.
fn said(note: &Note, folder: &str) -> String {
    let mut out = String::new();
    if !note.text.is_empty() {
        out.push_str(&note.text);
        out.push_str("\n\n");
    }
    for name in &note.pictures {
        out.push_str(&format!("![]({folder}/{name})\n\n"));
    }
    out
}

/// Статья с заметками в конце — для сохранения: файл несёт заметки в себе,
/// и показать их можно без Brevier. Без заметок — как была. Картинки
/// заметок — ссылками в `images/`, куда их кладёт `save`.
pub fn appended(markdown: &str, notes: &[&Note]) -> String {
    if notes.is_empty() {
        return markdown.to_owned();
    }
    let mut notes = notes.to_vec();
    notes.sort_by_key(|note| note.at);
    format!(
        "{}\n\n---\n\n## Your notes\n\n{}",
        markdown.trim_end(),
        entries(&notes, EXPORTED).trim_end()
    ) + "\n"
}

/// Папка картинок рядом с выгруженным или сохранённым файлом — та же,
/// куда `save` кладёт картинки статьи.
pub const EXPORTED: &str = "images";

/// Заметки одной страницы конспектом (#42): свойства для Obsidian сверху,
/// заголовок, затем заметки в порядке текста — цитата со ссылкой на своё
/// место на странице, слова, картинки (`images/…`). Заметки без места
/// идут тоже: цитата у них своя.
pub fn digest(notes: &[&Note]) -> String {
    let Some((title, address)) = named(notes) else {
        return String::new();
    };
    let first = notes.iter().map(|note| note.stamp.days).min().unwrap_or(0);
    let (year, month, day) = store::civil_from_days(first);
    let head = format!(
        "---\ntitle: {}\nsource: {}\ncreated: {year:04}-{month:02}-{day:02}\n---\n\n# {}\n\n",
        yaml(title),
        yaml(address),
        crate::feed::heading(title),
    );
    head + &digested(notes)
}

/// Заметки нескольких страниц одним файлом: свойства у файла markdown
/// одни, поэтому страница — разделом со ссылкой, как на `brevier:notes`.
pub fn digest_pages(pages: &[Vec<&Note>]) -> String {
    let mut out = String::from("# Your notes\n\n");
    for notes in pages {
        let Some((title, address)) = named(notes) else {
            continue;
        };
        out.push_str(&format!("## {}\n\n", store::link(title, address)));
        out.push_str(&digested(notes));
        out.push('\n');
    }
    out.trim_end().to_owned() + "\n"
}

/// Заголовок и адрес страницы заметок: заголовок — у самой свежей.
fn named<'a>(notes: &[&'a Note]) -> Option<(&'a str, &'a str)> {
    let last = notes.iter().max_by_key(|note| note.id)?;
    let title = if last.title.is_empty() {
        last.address.as_str()
    } else {
        last.title.as_str()
    };
    Some((title, last.address.as_str()))
}

/// Сами заметки конспекта, в порядке текста.
fn digested(notes: &[&Note]) -> String {
    let mut notes = notes.to_vec();
    notes.sort_by_key(|note| note.at);
    let mut out = String::new();
    for note in &notes {
        let place = fragment_link(&note.address, &note.quote);
        out.push_str(&quoted(&note.quote, place.as_deref()));
        out.push_str(&said(note, EXPORTED));
    }
    out.trim_end().to_owned() + "\n"
}

/// Строка YAML в кавычках: заголовок с двоеточием или `#` иначе
/// сломал бы свойства.
fn yaml(text: &str) -> String {
    let mut out = String::from("\"");
    for ch in text.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            ch if ch.is_control() => out.push(' '),
            ch => out.push(ch),
        }
    }
    out.push('"');
    out
}

/// Ссылка на место цитаты в обычном браузере: `#:~:text=` (Text Fragments —
/// Chrome, Safari, Firefox со 131-го). Короткая цитата — целиком, длинная —
/// началом и концом по [`FRAGMENT_WORDS`] слов: браузер сверяет слова
/// со страницей, и цитата в абзац ломалась бы на первом расхождении нашего
/// текста с сайтом. Не нашёл — откроет страницу сначала, ничего не сломав.
/// Только для сайтов: у файла и своих страниц браузера нет.
pub fn fragment_link(address: &str, quote: &str) -> Option<String> {
    if !(address.starts_with("https://") || address.starts_with("http://")) {
        return None;
    }
    // Маркеры пунктов и тире по краям — наши или из CSS, в тексте
    // страницы их нет.
    let words: Vec<&str> = quote.split_whitespace().collect();
    let worded = |word: &&str| word.chars().any(char::is_alphanumeric);
    let start = words.iter().position(worded)?;
    let end = words.iter().rposition(worded)? + 1;
    let words = &words[start..end];
    let directive = if words.len() <= 2 * FRAGMENT_WORDS {
        directive_text(&words.join(" "))
    } else {
        format!(
            "{},{}",
            directive_text(&words[..FRAGMENT_WORDS].join(" ")),
            directive_text(&words[words.len() - FRAGMENT_WORDS..].join(" ")),
        )
    };
    Some(format!("{address}#:~:text={directive}"))
}

/// Текст для `#:~:text=`: дефис, запятая и `&` в нём — синтаксис, скобки
/// оборвали бы ссылку markdown. Кодируем всё, кроме букв и цифр ASCII.
fn directive_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'~') {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

fn line_of(note: &Note) -> String {
    format!(
        "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\n",
        note.stamp.text(),
        note.id,
        store::escape(&note.address),
        note.at,
        store::escape(&note.title),
        store::escape(&note.before),
        store::escape(&note.quote),
        store::escape(&note.after),
        store::escape(&note.text),
        note.pictures.join(" "),
    )
}

fn parse_line(line: &str) -> Option<Note> {
    let mut fields = line.split('\t');
    let stamp = Stamp::parse(fields.next()?)?;
    let id = fields.next()?.parse().ok()?;
    let address = store::unescape(fields.next()?);
    let at = fields.next()?.parse().ok()?;
    let title = store::unescape(fields.next()?);
    let before = store::unescape(fields.next()?);
    let quote = store::unescape(fields.next()?);
    let after = store::unescape(fields.next()?);
    let text = store::unescape(fields.next().unwrap_or_default());
    // Десятое поле — с картинками (#42); у строк постарше его нет.
    let pictures = fields
        .next()
        .unwrap_or_default()
        .split(' ')
        .filter(|name| is_picture_name(name))
        .map(str::to_owned)
        .collect();
    (!address.is_empty() && !quote.is_empty()).then_some(Note {
        id,
        stamp,
        address,
        title,
        at,
        before,
        quote,
        after,
        text,
        pictures,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scratch::Scratch;

    fn note(selection: Selection, id: u64) -> Note {
        Note {
            id,
            stamp: Stamp::of_unix(0, 0),
            address: "https://e.com/a".to_owned(),
            title: "A".to_owned(),
            at: selection.at,
            before: selection.before,
            quote: selection.quote,
            after: selection.after,
            text: String::new(),
            pictures: Vec::new(),
        }
    }

    fn chars(text: &str, part: &str) -> (usize, usize) {
        let byte = text.find(part).unwrap();
        let from = text[..byte].chars().count();
        (from, from + part.chars().count())
    }

    #[test]
    fn a_note_finds_its_quote_on_the_same_page() {
        let page = "Первый абзац о лесе.\nВторой абзац о море.\n";
        let (from, to) = chars(page, "о море");
        let selection = select(page, from, to).unwrap();
        assert_eq!(selection.quote, "о море");
        // Окружение — последние CONTEXT знаков перед цитатой.
        assert_eq!(selection.before, "рвый абзац о лесе.\nВторой абзац ");
        assert_eq!(selection.before.chars().count(), CONTEXT);
        assert_eq!(selection.after, ".\n");
        let spots = place(page, &[&note(selection, 1)]);
        assert_eq!(spots[0].place, Some((from, to)));
    }

    #[test]
    fn hyphens_and_no_break_spaces_do_not_hide_the_quote() {
        // Выделили на странице с переносами — нашлось на ней же и без них.
        let typeset = "Ко\u{ad}роткий\u{a0}текст про лес.";
        let (from, to) = chars(typeset, "Ко\u{ad}роткий\u{a0}текст");
        let selection = select(typeset, from, to).unwrap();
        assert_eq!(selection.quote, "Короткий текст");
        let plain = "Короткий текст про лес.";
        let spots = place(plain, &[&note(selection.clone(), 1)]);
        assert_eq!(spots[0].place, Some((0, 14)));
        // И обратно: место на странице с переносами накрывает их.
        let spots = place(typeset, &[&note(selection, 1)]);
        assert_eq!(spots[0].place, Some((from, to)));
    }

    /// Цитата внутри ряда одинаковых знаков перекрывается сама с собой:
    /// поиск без перекрытий находил её на шаг длины цитаты, мимо места,
    /// где заметку оставили (нашёл фаззинг, `fuzz/fuzz_targets/notes.rs`).
    #[test]
    fn a_note_inside_a_run_of_equal_signs_stays_where_it_was_left() {
        let page = format!("Перед рядом {} и после.", "=".repeat(40));
        let from = "Перед рядом ".chars().count() + 7;
        let selection = select(&page, from, from + 10).unwrap();
        assert_eq!(selection.quote, "==========");
        let spots = place(&page, &[&note(selection, 1)]);
        assert_eq!(spots[0].place, Some((from, from + 10)));
    }

    #[test]
    fn of_two_equal_passages_the_context_picks_one() {
        let page = "Alpha said: it rains. Beta said: it rains. Gamma.";
        let (from, to) = chars(page, "Beta said: it rains");
        let (from, to) = (from + "Beta said: ".len(), to);
        let selection = select(page, from, to).unwrap();
        assert_eq!(selection.quote, "it rains");
        let spots = place(page, &[&note(selection.clone(), 1)]);
        assert_eq!(spots[0].place, Some((from, to)));
        // Страница поменялась: перед цитатами вставили абзац. Окружение
        // то же — место находится заново, хотя подсказка уже мимо.
        let changed = format!("A new opening paragraph.\n{page}");
        let shift = "A new opening paragraph.\n".len();
        let spots = place(&changed, &[&note(selection, 1)]);
        assert_eq!(spots[0].place, Some((from + shift, to + shift)));
    }

    #[test]
    fn a_quote_gone_from_the_page_has_no_place() {
        let page = "The sentence that will be edited away.";
        let selection = select(page, 4, 12).unwrap();
        let spots = place("Something else entirely.", &[&note(selection, 7)]);
        assert_eq!(spots, vec![Spot { id: 7, place: None }]);
    }

    #[test]
    fn blank_edges_are_not_part_of_the_quote() {
        let page = "one  two \u{fffc} three";
        assert_eq!(select(page, 3, 9).unwrap().quote, "two");
        assert_eq!(select(page, 3, 5), Err(Refusal::Empty));
        let long = "x".repeat(MAX_QUOTE + 1);
        assert_eq!(select(&long, 0, MAX_QUOTE + 1), Err(Refusal::TooLong));
    }

    #[test]
    fn notes_survive_the_file() {
        let dir = Scratch::new("notes");
        let path = dir.join("notes.tsv");
        let mut notes = Notes::at(&path);
        let page = "Tabs\tand\nnewlines \\ in a quote.";
        let selection = select(page, 0, page.chars().count()).unwrap();
        let id = notes.add(
            "https://e.com/a#part",
            "A  title",
            selection,
            "  my note\nsecond line ",
            Stamp::of_unix(1_760_000_000, 10_800),
        );
        let again = Notes::at(&path);
        let stored = again.get(id).unwrap();
        assert_eq!(stored.address, "https://e.com/a");
        assert_eq!(stored.title, "A title");
        assert_eq!(stored.quote, page);
        assert_eq!(stored.text, "my note\nsecond line");
        assert_eq!(again.of("https://e.com/a").len(), 1);

        let mut notes = Notes::at(&path);
        assert!(notes.edit(id, ""));
        assert_eq!(Notes::at(&path).get(id).unwrap().text, "");
        assert!(notes.remove(id));
        assert!(Notes::at(&path).all().is_empty());
    }

    #[test]
    fn a_saved_article_carries_its_notes() {
        let page = "# Title\n\nSome text.\n";
        let mut first = note(select("Some text.", 0, 4).unwrap(), 1);
        first.at = 9;
        first.text = "Why *some*?".to_owned();
        let mut second = note(select("two lines\nof quote", 0, 18).unwrap(), 2);
        second.at = 3;
        let saved = appended(page, &[&first, &second]);
        assert_eq!(
            saved,
            "# Title\n\nSome text.\n\n---\n\n## Your notes\n\n\
             > two lines\n>\n> of quote\n\n> Some\n\nWhy *some*?\n"
        );
        assert_eq!(appended(page, &[]), page);
    }

    #[test]
    fn the_notes_page_groups_by_page() {
        let dir = Scratch::new("notes-page");
        let mut notes = Notes::at(dir.join("notes.tsv"));
        assert!(notes.page(&HashMap::new()).contains("Nothing here yet"));
        let quote = |text: &str| select(text, 0, text.chars().count()).unwrap();
        let stamp = Stamp::of_unix(1_760_000_000, 0);
        notes.add("https://a.org/one", "One", quote("first"), "", stamp);
        notes.add(
            "https://b.org/two",
            "Two [draft]",
            quote("second"),
            "a thought",
            stamp,
        );
        let page = notes.page(&HashMap::new());
        // Свежая страница — сверху; заголовок ссылкой, скобки экранированы.
        let two = page
            .find("## [Two \\[draft\\]](https://b.org/two)")
            .unwrap();
        let one = page.find("## [One](https://a.org/one)").unwrap();
        assert!(two < one, "{page}");
        assert!(page.contains("> second\n\na thought\n"), "{page}");
        assert!(page.contains("b.org · "), "{page}");
    }

    #[test]
    fn a_long_quote_is_cut_by_the_word_on_the_shelf() {
        assert_eq!(preview("short\nquote"), "short quote");
        let long = "word ".repeat(40);
        let shown = preview(&long);
        assert!(shown.ends_with("word…"), "{shown}");
        assert!(shown.chars().count() <= PREVIEW + 1);
    }

    /// PNG в один пиксель: подписи хватает, чтобы назваться картинкой.
    const DOT: &[u8] = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR\0\0\0\x01\0\0\0\x01\x08\x06\0\0\0";

    #[test]
    fn a_picture_lives_as_long_as_a_note_needs_it() {
        let dir = Scratch::new("notes-pictures");
        let path = dir.join("notes.tsv");
        let mut notes = Notes::at(&path);
        let stamp = Stamp::of_unix(1_760_000_000, 0);
        let quote = |text: &str| select(text, 0, text.chars().count()).unwrap();
        let one = notes.add("https://e.com/a", "A", quote("first"), "", stamp);
        let two = notes.add("https://e.com/a", "A", quote("second"), "", stamp);

        assert_eq!(notes.attach(one, b"GIF but not"), Err(Unkept::NotAPicture));
        assert_eq!(notes.attach(99, DOT), Err(Unkept::Nowhere));
        let name = notes.attach(one, DOT).unwrap();
        assert!(is_picture_name(&name), "{name}");
        assert!(name.ends_with(".png"));
        let file = dir.join(PICTURES).join(&name);
        assert_eq!(fs::read(&file).unwrap(), DOT);
        // Та же картинка у второй заметки — тот же файл.
        assert_eq!(notes.attach(two, DOT).unwrap(), name);
        // Без заметки картинка не бывает отметкой.
        assert!(notes.get(one).unwrap().says());

        let again = Notes::at(&path);
        assert_eq!(again.get(one).unwrap().pictures, vec![name.clone()]);
        assert_eq!(again.picture(&name), Some(file.clone()));

        // Первая заметка ушла — файл остался у второй; ушла и картинка
        // второй — файла нет.
        assert!(notes.remove(one));
        assert!(file.is_file());
        assert!(notes.detach(two, &name));
        assert!(!file.exists());
        assert!(!notes.get(two).unwrap().says());
        assert!(!notes.detach(two, &name));
    }

    #[test]
    fn only_our_own_names_lead_to_a_picture() {
        assert!(is_picture_name("0123456789abcdef.jpg"));
        assert!(!is_picture_name("../../0123456789abcdef.jpg"));
        assert!(!is_picture_name("0123456789abcdef.svg"));
        assert!(!is_picture_name("0123456789abcdef.png/x"));
        assert!(!is_picture_name("notes.tsv"));
        let notes = Notes::at("/nowhere/notes.tsv");
        assert_eq!(notes.picture("../secret.png"), None);
    }

    #[test]
    fn a_line_from_before_pictures_still_reads() {
        let old = "2026-10-17T10:00:00+03:00\t3\thttps://e.com/a\t5\tA\t\tquote\t\tsaid";
        let note = parse_line(old).unwrap();
        assert_eq!(note.text, "said");
        assert!(note.pictures.is_empty());
        let mut note = note;
        note.pictures = vec!["0123456789abcdef.jpg".to_owned()];
        assert_eq!(parse_line(line_of(&note).trim_end()), Some(note));
    }

    #[test]
    fn a_digest_links_each_quote_to_its_place() {
        let page = "Литовские племена жили к западу. Короткая.";
        let mut long = note(select(page, 0, 32).unwrap(), 1);
        long.text = "Карта — ниже.".to_owned();
        long.pictures = vec!["0123456789abcdef.jpg".to_owned()];
        long.title = "Платонов: лекции".to_owned();
        let mut short = note(select(page, 33, 42).unwrap(), 2);
        short.at = 33;
        short.title = "Платонов: \"лекции\"".to_owned();
        let digest = digest(&[&short, &long]);
        assert_eq!(
            digest,
            "---\ntitle: \"Платонов: \\\"лекции\\\"\"\nsource: \"https://e.com/a\"\n\
             created: 1970-01-01\n---\n\n# Платонов: \"лекции\"\n\n\
             > Литовские племена жили к западу.\n>\n\
             > — [on the page](https://e.com/a#:~:text=%D0%9B%D0%B8%D1%82%D0%BE%D0%B2%D1%81%D0%BA%D0%B8%D0%B5%20%D0%BF%D0%BB%D0%B5%D0%BC%D0%B5%D0%BD%D0%B0%20%D0%B6%D0%B8%D0%BB%D0%B8%20%D0%BA%20%D0%B7%D0%B0%D0%BF%D0%B0%D0%B4%D1%83.)\n\n\
             Карта — ниже.\n\n![](images/0123456789abcdef.jpg)\n\n\
             > Короткая.\n>\n> — [on the page](https://e.com/a#:~:text=%D0%9A%D0%BE%D1%80%D0%BE%D1%82%D0%BA%D0%B0%D1%8F.)\n"
        );
    }

    #[test]
    fn a_long_quote_links_by_its_first_and_last_words() {
        let quote = "• one two three four five six seven eight nine-ten, eleven —";
        assert_eq!(
            fragment_link("https://e.com/a", quote).unwrap(),
            "https://e.com/a#:~:text=one%20two%20three%20four,\
             seven%20eight%20nine%2Dten%2C%20eleven"
        );
        assert_eq!(fragment_link("file:///home/a.md", "one"), None);
        assert_eq!(fragment_link("https://e.com/a", "— •"), None);
    }

    #[test]
    fn a_saved_article_links_the_pictures_of_its_notes() {
        let mut first = note(select("Some text.", 0, 4).unwrap(), 1);
        first.pictures = vec!["0123456789abcdef.png".to_owned()];
        let saved = appended("# Title\n", &[&first]);
        assert!(
            saved.ends_with("> Some\n\n![](images/0123456789abcdef.png)\n"),
            "{saved}"
        );
    }
}
