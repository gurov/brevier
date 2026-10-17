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
        let (mut chars, mut bytes) = (0, 0);
        for (byte, _) in self.text.match_indices(note.quote.as_str()) {
            chars += self.text[bytes..byte].chars().count();
            bytes = byte;
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
        let before = self.entries.len();
        self.entries.retain(|note| note.id != id);
        let removed = self.entries.len() != before;
        if removed {
            self.write();
        }
        removed
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
            out.push_str(&entries(&notes));
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

/// Заметки markdown-ом: цитата цитатой, приписанное — абзацем под ней.
/// Читается где угодно; Obsidian и Logseq берут такое как есть.
pub fn entries(notes: &[&Note]) -> String {
    let mut out = String::new();
    for note in notes {
        let lines: Vec<String> = note
            .quote
            .split('\n')
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(|line| format!("> {}", crate::feed::block(line)))
            .collect();
        out.push_str(&lines.join("\n>\n"));
        out.push_str("\n\n");
        if !note.text.is_empty() {
            out.push_str(&note.text);
            out.push_str("\n\n");
        }
    }
    out
}

/// Статья с заметками в конце — для сохранения: файл несёт заметки в себе,
/// и показать их можно без Brevier. Без заметок — как была.
pub fn appended(markdown: &str, notes: &[&Note]) -> String {
    if notes.is_empty() {
        return markdown.to_owned();
    }
    let mut notes = notes.to_vec();
    notes.sort_by_key(|note| note.at);
    format!(
        "{}\n\n---\n\n## Your notes\n\n{}",
        markdown.trim_end(),
        entries(&notes).trim_end()
    ) + "\n"
}

fn line_of(note: &Note) -> String {
    format!(
        "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\n",
        note.stamp.text(),
        note.id,
        store::escape(&note.address),
        note.at,
        store::escape(&note.title),
        store::escape(&note.before),
        store::escape(&note.quote),
        store::escape(&note.after),
        store::escape(&note.text),
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
}
