//! Печать: что из страницы идёт на бумагу и как она делится на листы.
//!
//! Лист рисует интерфейс (окно — `src/ui/paper.rs`), а решения, которые
//! от тулкита не зависят, — здесь, рядом с моделью страницы, и проверяются
//! тестами без GTK:
//!
//! - **Сноски-адреса.** По бумаге не щёлкнешь, поэтому ссылка получает номер
//!   верхним индексом, а адреса идут списком в конце. Одинаковым адресам —
//!   один номер; ссылке внутри страницы и ссылке, которая сама и есть адрес,
//!   номер не нужен.
//! - **Приложение**: заметки читателя и список адресов — markdown, который
//!   интерфейс раскладывает тем же трактом (`Page::of`), что и статью.
//! - **Разбивка на листы** по высотам кусков: строк, картинок, рядов таблиц.
//!   Первая строка абзаца не остаётся одна внизу листа, последняя — одна
//!   наверху, заголовок не отрывается от текста под ним.
//!
//! Кегль на бумаге — не экранный: 16,5 пт экрана на бумаге — крупная печать,
//! а мера в 36 кеглей не влезла бы и в A4. Набор читателя — гарнитура,
//! мера в кеглях, интерлиньяж, выключка, воздух — остаётся как есть,
//! кегль переводится в книжный: наш экранный становится [`TEXT_SIZE`],
//! крупный читателя — пропорционально крупнее.

use std::collections::HashMap;
use std::ops::Range;

use crate::notes::Note;
use crate::outline::{self, Type};
use crate::page::{Block, Page, fragment_of};
use crate::typeset;

/// Кегль текста на бумаге при нашем экранном кегле, в пунктах. Книжный:
/// мера в 36 кеглей — 14 см, на A4 и Letter остаются поля в три с лишним
/// сантиметра.
pub const TEXT_SIZE: f32 = 11.0;

/// Во сколько раз бумага мельче экрана при том же наборе читателя.
pub fn scale(typography: &Type) -> f32 {
    TEXT_SIZE / outline::TEXT_SIZE * typography.scale()
}

/// Номера ссылок страницы для списка адресов в конце.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Endnotes {
    /// Номер ссылки текста — по её месту в `Page::links`.
    pub text: HashMap<usize, usize>,
    /// Номер ссылки в ячейке таблицы.
    pub cells: HashMap<CellLink, usize>,
    /// Адреса по порядку: номер `n` — это `targets[n - 1]`.
    pub targets: Vec<String>,
}

/// Ссылка в ячейке: блок страницы (место в `Page::blocks`), ряд, ячейка
/// и ссылка в ней.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CellLink {
    pub block: usize,
    pub row: usize,
    pub cell: usize,
    pub link: usize,
}

/// Пронумеровать ссылки страницы в порядке чтения — текст и таблицы вперемежку,
/// как они стоят. `here` — адрес самой страницы (ссылка на её же раздел номера
/// не получает), `range` — что печатается: вся страница или выделенное.
pub fn endnotes(page: &Page, here: &str, range: Range<usize>) -> Endnotes {
    let mut out = Endnotes::default();
    let mut targets: Vec<String> = Vec::new();
    let mut numbers: HashMap<String, usize> = HashMap::new();
    let mut number = |target: &str, text: &str| -> Option<usize> {
        if !wants_number(target, text, here) {
            return None;
        }
        let next = numbers.len() + 1;
        let number = *numbers.entry(target.to_owned()).or_insert(next);
        if number == next {
            targets.push(target.to_owned());
        }
        Some(number)
    };

    // Ссылки текста и таблицы — двумя упорядоченными списками; сливаем по месту.
    let tables: Vec<(usize, usize)> = page
        .blocks
        .iter()
        .enumerate()
        .filter(|(_, placed)| matches!(placed.block, Block::Table(_)))
        .map(|(index, placed)| (placed.at, index))
        .filter(|(at, _)| range.contains(at))
        .collect();
    let mut tables = tables.into_iter().peekable();
    let chars: Vec<char> = page.text.chars().collect();
    let mut cells = Vec::new();
    for (index, link) in page.links.iter().enumerate() {
        while let Some(&(at, block)) = tables.peek()
            && at < link.start
        {
            tables.next();
            cells.extend(number_cells(page, block, &mut number));
        }
        if link.start < range.start || link.end > range.end || link.end > chars.len() {
            continue;
        }
        let text: String = chars[link.start..link.end].iter().collect();
        if let Some(n) = number(&link.target, &text) {
            out.text.insert(index, n);
        }
    }
    for (_, block) in tables {
        cells.extend(number_cells(page, block, &mut number));
    }
    out.cells = cells.into_iter().collect();
    out.targets = targets;
    out
}

fn number_cells(
    page: &Page,
    block: usize,
    number: &mut impl FnMut(&str, &str) -> Option<usize>,
) -> Vec<(CellLink, usize)> {
    let Some(Block::Table(table)) = page.blocks.get(block).map(|placed| &placed.block) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (row_index, row) in table.rows.iter().enumerate() {
        for (cell_index, cell) in row.cells.iter().enumerate() {
            for (link_index, link) in cell.links.iter().enumerate() {
                let text: String = cell
                    .text
                    .chars()
                    .skip(link.start)
                    .take(link.end.saturating_sub(link.start))
                    .collect();
                if let Some(n) = number(&link.target, &text) {
                    let at = CellLink {
                        block,
                        row: row_index,
                        cell: cell_index,
                        link: link_index,
                    };
                    out.push((at, n));
                }
            }
        }
    }
    out
}

/// Нужен ли ссылке номер. Не нужен: ссылке на место этой же страницы, своим
/// страницам Brevier, ссылке без слов (картинка-ссылка — номер встал бы под
/// картинкой, отдельно от всего) и ссылке, которая сама и есть адрес.
fn wants_number(target: &str, text: &str, here: &str) -> bool {
    if target.is_empty() || target.starts_with("brevier:") {
        return false;
    }
    if fragment_of(target, Some(here)).is_some() {
        return false;
    }
    let text = typeset::plain(text);
    let words: String = text.chars().filter(|&c| c != crate::page::OBJECT).collect();
    if words.trim().is_empty() {
        return false;
    }
    bare(&words) != bare(target)
}

/// Адрес без того, что в подписи ссылки обычно опускают: схемы, `www.`,
/// косой черты в конце, регистра.
fn bare(address: &str) -> String {
    let address = address.trim();
    let address = ["https://", "http://", "mailto:"]
        .iter()
        .find_map(|scheme| {
            address
                .get(..scheme.len())
                .filter(|head| head.eq_ignore_ascii_case(scheme))
                .map(|_| &address[scheme.len()..])
        })
        .unwrap_or(address);
    let address = address.strip_prefix("www.").unwrap_or(address);
    address.trim_end_matches('/').to_lowercase()
}

/// Приложение к статье на бумаге — markdown: заметки читателя (цитата,
/// приписанное, картинки — именами файлов рядом с адресом приложения)
/// и адреса ссылок по номерам. Пусто — печатать нечего.
pub fn appendix(notes: &[&Note], targets: &[String]) -> String {
    let mut parts = Vec::new();
    if !notes.is_empty() {
        let mut notes = notes.to_vec();
        notes.sort_by_key(|note| note.at);
        parts.push(format!(
            "## Your notes\n\n{}",
            crate::notes::entries(&notes, ".").trim_end()
        ));
    }
    if !targets.is_empty() {
        let list: Vec<String> = targets
            .iter()
            .enumerate()
            .map(|(index, target)| {
                format!(
                    "{}. {}",
                    index + 1,
                    crate::store::link(&shown_target(target), target)
                )
            })
            .collect();
        parts.push(format!("## Links\n\n{}", list.join("\n")));
    }
    if parts.is_empty() {
        return String::new();
    }
    format!("---\n\n{}\n", parts.join("\n\n"))
}

/// Как напечатать адрес: целиком, кроме `mailto:` — почту читают без него.
fn shown_target(target: &str) -> String {
    target.strip_prefix("mailto:").unwrap_or(target).to_owned()
}

/// Кусок, из которых складывается лист: строка, картинка, ряд таблицы.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Piece {
    /// Высота, в тех же единицах, что и лист.
    pub height: f64,
    /// Воздух над куском. Наверху листа его нет.
    pub above: f64,
    /// Не отрывать от следующего: первая строка абзаца от второй,
    /// предпоследняя от последней, заголовок от текста.
    pub keep: bool,
    /// До какой высоты кусок можно ужать, чтобы он встал в остаток листа:
    /// картинке, которой не хватило немного места, лучше стать чуть меньше,
    /// чем оставить пол-листа пустым. Строка не ужимается — `least == height`.
    pub least: f64,
}

/// Разложить куски по листам высотой `height`: какие куски на каком листе.
///
/// Лист набирается, пока кусок влезает. Не влез, но влезает ужатым
/// (`least`) — встаёт ужатым до остатка листа и лист кончается на нём:
/// поэтому ужатый кусок всегда последний на своём листе, и рисующему
/// хватает остатка, чтобы узнать, насколько. Не влез и так — лист кончается
/// перед ним, а если предыдущий кусок просит не отрывать его от этого,
/// то раньше — перед всей сцепкой. Сцепка длиннее листа (или кусок выше
/// листа) рвётся там, где кончилось место: лучше разорвать, чем потерять.
pub fn paginate(pieces: &[Piece], height: f64) -> Vec<Range<usize>> {
    const SLACK: f64 = 0.01;
    let mut starts = vec![0];
    let mut start = 0;
    let mut used = 0.0;
    let mut index = 0;
    while index < pieces.len() {
        let piece = pieces[index];
        if index == start {
            used = piece.height;
            index += 1;
            continue;
        }
        let need = piece.above + piece.height;
        if used + need <= height + SLACK {
            used += need;
            index += 1;
            continue;
        }
        if piece.least < piece.height && used + piece.above + piece.least <= height + SLACK {
            index += 1;
            starts.push(index);
            start = index;
            continue;
        }
        let mut cut = index;
        while cut > start + 1 && pieces[cut - 1].keep {
            cut -= 1;
        }
        if pieces[cut - 1].keep {
            cut = index;
        }
        starts.push(cut);
        start = cut;
        index = cut;
    }
    let mut pages: Vec<Range<usize>> = starts.windows(2).map(|pair| pair[0]..pair[1]).collect();
    if let Some(&last) = starts.last() {
        pages.push(last..pieces.len());
    }
    pages.retain(|page| !page.is_empty());
    pages
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Document;
    use crate::address::Address;
    use crate::page::Page;

    fn page_of(markdown: &str) -> Page {
        let document = Document {
            address: Address::Web("https://example.com/post".to_owned()),
            title: String::new(),
            markdown: markdown.to_owned(),
            kind: crate::Kind::Article,
            served: false,
            site: Vec::new(),
            feeds: Vec::new(),
            lang: None,
            next: None,
            archived: None,
        };
        Page::of(&document)
    }

    fn piece(height: f64, keep: bool) -> Piece {
        Piece {
            height,
            above: 0.0,
            keep,
            least: height,
        }
    }

    #[test]
    fn links_are_numbered_once_per_address_in_reading_order() {
        let page = page_of(
            "Read [the guide](https://a.com/guide), then [this](https://b.com/), \
             and [the guide again](https://a.com/guide).",
        );
        let all = 0..page.text.chars().count();
        let notes = endnotes(&page, "https://example.com/post", all);
        assert_eq!(notes.targets, vec!["https://a.com/guide", "https://b.com/"]);
        let mut numbers: Vec<_> = notes.text.into_iter().collect();
        numbers.sort();
        assert_eq!(numbers, vec![(0, 1), (1, 2), (2, 1)]);
    }

    #[test]
    fn a_link_that_is_its_own_address_or_points_here_gets_no_number() {
        let page = page_of(
            "See <https://www.example.org/x/>, [example.org/x](https://example.org/x), \
             [below](#notes), [the same page](https://example.com/post#notes) \
             and [elsewhere](https://example.net/).\n\n## Notes\n\nEnd.",
        );
        let all = 0..page.text.chars().count();
        let notes = endnotes(&page, "https://example.com/post", all);
        assert_eq!(notes.targets, vec!["https://example.net/"]);
        assert_eq!(notes.text.len(), 1);
    }

    #[test]
    fn only_the_printed_part_counts() {
        let page = page_of("[One](https://one.com/) and [two](https://two.com/).");
        let two = page.links[1].start;
        let notes = endnotes(
            &page,
            "https://example.com/post",
            two..page.text.chars().count(),
        );
        assert_eq!(notes.targets, vec!["https://two.com/"]);
        assert_eq!(notes.text.get(&1), Some(&1));
        assert!(!notes.text.contains_key(&0));
    }

    #[test]
    fn table_links_take_their_place_in_the_reading_order() {
        let page = page_of(
            "Before [a](https://a.com/).\n\n\
             | Name | Where |\n|---|---|\n| B | [b](https://b.com/) |\n\n\
             After [c](https://c.com/).",
        );
        let all = 0..page.text.chars().count();
        let notes = endnotes(&page, "https://example.com/post", all);
        assert_eq!(
            notes.targets,
            vec!["https://a.com/", "https://b.com/", "https://c.com/"]
        );
        assert_eq!(notes.cells.len(), 1);
        assert_eq!(notes.cells.values().next(), Some(&2));
    }

    #[test]
    fn the_appendix_lists_notes_then_addresses() {
        assert_eq!(appendix(&[], &[]), "");
        let text = appendix(
            &[],
            &["https://a.com/x".to_owned(), "mailto:me@b.com".to_owned()],
        );
        assert_eq!(
            text,
            "---\n\n## Links\n\n1. [https://a.com/x](https://a.com/x)\n\
             2. [me@b.com](mailto:me@b.com)\n"
        );
        let page = page_of(&text);
        assert!(page.text.contains("https://a.com/x"));
        assert!(page.text.contains("me@b.com"));
    }

    #[test]
    fn pages_fill_until_the_next_piece_does_not_fit() {
        let pieces = vec![piece(10.0, false); 7];
        assert_eq!(paginate(&pieces, 30.0), vec![0..3, 3..6, 6..7]);
        assert_eq!(paginate(&[], 30.0), Vec::<Range<usize>>::new());
    }

    #[test]
    fn air_above_counts_except_at_the_top_of_a_page() {
        let mut pieces = vec![piece(10.0, false); 4];
        for piece in &mut pieces {
            piece.above = 5.0;
        }
        // 10, 5+10 = 25; ещё 15 не влезает в 30. Наверху второго листа воздуха нет.
        assert_eq!(paginate(&pieces, 30.0), vec![0..2, 2..4]);
    }

    #[test]
    fn a_kept_piece_moves_with_the_next_one() {
        // Заголовок (держится за строку под ним) последним на листе уходит
        // на следующий вместе с ней.
        let pieces = vec![
            piece(10.0, false),
            piece(10.0, false),
            piece(10.0, true),
            piece(10.0, false),
        ];
        assert_eq!(paginate(&pieces, 30.0), vec![0..2, 2..4]);
    }

    #[test]
    fn a_chain_longer_than_a_page_breaks_where_the_room_ends() {
        let pieces = vec![piece(10.0, true); 5];
        assert_eq!(paginate(&pieces, 30.0), vec![0..3, 3..5]);
        // Кусок выше листа стоит один.
        let tall = vec![piece(10.0, false), piece(50.0, false), piece(10.0, false)];
        assert_eq!(paginate(&tall, 30.0), vec![0..1, 1..2, 2..3]);
    }

    #[test]
    fn a_picture_that_almost_fits_shrinks_and_ends_the_page() {
        let mut pieces = vec![piece(10.0, false), piece(25.0, false), piece(10.0, false)];
        pieces[1].least = 18.0;
        // 10 + 25 не влезает в 30, а 10 + 18 влезает: картинка остаётся
        // на первом листе и кончает его.
        assert_eq!(paginate(&pieces, 30.0), vec![0..2, 2..3]);
        // 10 + 22 не влезает: картинка уходит на второй лист в полный рост.
        pieces[1].least = 22.0;
        assert_eq!(paginate(&pieces, 30.0), vec![0..1, 1..2, 2..3]);
        // Последний кусок, ужатый, — пустого листа за ним нет.
        assert_eq!(paginate(&pieces[..2], 30.0).len(), 2);
        pieces[1].least = 18.0;
        assert_eq!(paginate(&pieces[..2], 30.0), vec![0..2]);
    }

    #[test]
    fn paper_type_is_book_sized() {
        assert!((scale(&Type::USUAL) * outline::TEXT_SIZE - TEXT_SIZE).abs() < 0.01);
        let large = Type {
            size: outline::TEXT_SIZE * 2.0,
            ..Type::USUAL
        };
        assert!((scale(&large) * outline::TEXT_SIZE - 2.0 * TEXT_SIZE).abs() < 0.01);
    }
}
