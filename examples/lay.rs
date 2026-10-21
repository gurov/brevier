//! Тракт окна и телефона на странице с диска — для замороженного набора
//! (`corpus/frozen.py`). cli модели страницы не строит: раскладка, переносы,
//! вехи, поиск и заметки живут только в интерфейсах, и прогон корпуса их
//! не видит. Здесь — то же, что окно и телефон делают с открытой страницей,
//! кроме сети и рисования, плюс проверка того, что модель сходится сама
//! с собой: смещения в тексте, объекты на своих знаках, заметка находит
//! то место, где её оставили.
//!
//!     cargo build --release --example lay --features typeset,images
//!     target/release/examples/lay <url> <out.md> <out.json> < page.html
//!
//! Код возврата: 0 — разложено; код ошибки cli — тракт отказал (пустое
//! извлечение и прочее — исход, а не сбой); 70 — модель не сошлась, что
//! именно — в stderr; 101 — паника, как у любой программы на Rust.

use std::io::Read;
use std::process::ExitCode;

use brevier::page::{self, Block, Page, json_string};
use brevier::reading::{self, Progress};
use brevier::store::Stamp;
use brevier::{Document, Kind, notes};

/// Экран для вех внутри раздела — в знаках, как у телефона средней меры.
const SCREEN: usize = 1500;
/// Сколько слов страницы искать — поровну по всей странице. Не каждое
/// n-е: на длинной странице поиск раскладчика занял бы больше, чем сама
/// раскладка, и «медленная страница» в отчёте значила бы медленный тест.
const NEEDLES: usize = 40;
/// Слова, на которых поиск без учёта регистра обычно и ломается: строчная
/// форма длиннее заглавной или наоборот.
const AWKWARD: [&str; 5] = ["İ", "ß", "ǅ", "ﬃ", "Σ"];
/// Длина выделения под заметку — как у пальца: полстроки.
const NOTE_SPAN: usize = 40;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [url, md_out, json_out] = args.as_slice() else {
        eprintln!("usage: lay <url> <out.md> <out.json> < page.html");
        return ExitCode::from(2);
    };
    let mut bytes = Vec::new();
    if let Err(error) = std::io::stdin().read_to_end(&mut bytes) {
        eprintln!("stdin: {error}");
        return ExitCode::from(1);
    }
    // Как `brevier --stdin`: кодировку — из самой страницы (#32).
    let html = brevier::fetch::decode(&bytes, None, brevier::fetch::ContentKind::Html);
    let document = match brevier::from_html(&html, url) {
        Ok(document) => document,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::from(error.exit_code());
        }
    };
    // Не `guarded`: паника здесь — то, что прогон ищет.
    let page = Page::of(&document);
    let mut broken = Vec::new();
    check_model(&page, &mut broken);
    let waypoints = waypoints(&page);
    exercise_reading(url, &page);
    check_find(&page, &mut broken);
    check_notes(&page, &mut broken);

    let json = model_json(&document, &page, &waypoints);
    if let Err(error) =
        std::fs::write(md_out, &document.markdown).and_then(|()| std::fs::write(json_out, json))
    {
        eprintln!("write: {error}");
        return ExitCode::from(1);
    }
    if broken.is_empty() {
        return ExitCode::SUCCESS;
    }
    for problem in &broken {
        eprintln!("{problem}");
    }
    ExitCode::from(70)
}

/// Смещения модели — внутри текста, участки не вывернуты, на месте каждого
/// объекта — знак объекта. Окно кладёт текст в буфер как есть и ставит
/// виджеты по этим смещениям: разойдись они — GTK упадёт или нарисует
/// не там.
fn check_model(page: &Page, broken: &mut Vec<String>) {
    let chars: Vec<char> = page.text.chars().collect();
    let total = chars.len();
    for run in &page.runs {
        if run.start > run.end || run.end > total {
            broken.push(format!("run {}..{} of {total}", run.start, run.end));
        }
    }
    for link in &page.links {
        if link.start > link.end || link.end > total {
            broken.push(format!("link {}..{} of {total}", link.start, link.end));
        }
    }
    for (name, at) in &page.anchors {
        if *at > total {
            broken.push(format!("anchor {name} at {at} of {total}"));
        }
    }
    for mark in page.contents.iter().chain(&page.leads) {
        if mark.offset > total {
            broken.push(format!(
                "mark {:?} at {} of {total}",
                mark.title, mark.offset
            ));
        }
    }
    let objects = chars.iter().filter(|&&ch| ch == page::OBJECT).count();
    if objects != page.blocks.len() {
        broken.push(format!(
            "{objects} object signs for {} blocks",
            page.blocks.len()
        ));
    }
    for placed in &page.blocks {
        if chars.get(placed.at) != Some(&page::OBJECT) {
            broken.push(format!("block at {} is not on an object sign", placed.at));
        }
        if let Block::Table(table) = &placed.block {
            for cell in table.rows.iter().flat_map(|row| &row.cells) {
                let length = cell.text.chars().count();
                let spans = cell.runs.iter().map(|run| (run.start, run.end));
                let spans = spans.chain(cell.links.iter().map(|link| (link.start, link.end)));
                for (start, end) in spans {
                    if start > end || end > length {
                        broken.push(format!("cell span {start}..{end} of {length}"));
                    }
                }
            }
        }
    }
}

/// Вехи внутри длинных разделов — как их делит полка (#19, #24).
fn waypoints(page: &Page) -> Vec<(page::Mark, String)> {
    let total = page.text.chars().count();
    let mut bounds: Vec<(usize, u8)> = page
        .contents
        .iter()
        .map(|mark| (mark.offset, mark.level))
        .collect();
    if bounds.first().is_none_or(|&(at, _)| at > 0) {
        bounds.insert(0, (0, 1));
    }
    let mut out = Vec::new();
    for (index, &(from, level)) in bounds.iter().enumerate() {
        let to = bounds.get(index + 1).map_or(total, |&(at, _)| at);
        for mark in reading::inner_waypoints(&page.leads, from, to, SCREEN, level) {
            let place = reading::place(mark.offset, total);
            out.push((mark, place));
        }
    }
    out
}

/// Прогресс чтения: счёт, доли полки, запись и подъём записанного.
fn exercise_reading(url: &str, page: &Page) {
    let mut progress = Progress::new(url, &page.text, &page.anchors);
    let total = progress.total();
    for from in (0..total).step_by(SCREEN) {
        progress.tick(from, (from + SCREEN).min(total), 5);
    }
    let starts: Vec<usize> = page.contents.iter().map(|mark| mark.offset).collect();
    let _ = progress.shares(&starts);
    let saved = progress.saved(Stamp {
        days: 0,
        seconds: 0,
        offset: 0,
    });
    let mut again = Progress::new(url, &page.text, &page.anchors);
    again.restore(&saved);
    let _ = again.resume_at(&saved);
    let _ = reading::offer(&again, &saved);
}

/// Поиск по странице и по ячейкам: совпадения — внутри текста и не пустые.
fn check_find(page: &Page, broken: &mut Vec<String>) {
    let words: Vec<&str> = page.text.split_whitespace().collect();
    let mut needles: Vec<String> = words
        .iter()
        .step_by(words.len().div_ceil(NEEDLES).max(1))
        .map(|word| word.to_string())
        .collect();
    needles.extend(AWKWARD.iter().map(|word| word.to_string()));
    let mut texts = vec![page.text.as_str()];
    for placed in &page.blocks {
        if let Block::Table(table) = &placed.block {
            texts.extend(
                table
                    .rows
                    .iter()
                    .flat_map(|row| &row.cells)
                    .map(|cell| cell.text.as_str()),
            );
        }
    }
    for text in texts {
        let total = text.chars().count();
        for needle in &needles {
            for (from, to) in page::hits(text, needle) {
                if from >= to || to > total {
                    broken.push(format!("find {needle:?}: {from}..{to} of {total}"));
                }
            }
        }
        let _ = page::utf16_offsets(text, &page::hits(text, "the"));
    }
}

/// Заметка, оставленная на месте начала абзаца, находит это же место.
fn check_notes(page: &Page, broken: &mut Vec<String>) {
    let mut made = Vec::new();
    for (index, lead) in page.leads.iter().enumerate().step_by(7) {
        let Ok(selection) = notes::select(&page.text, lead.offset, lead.offset + NOTE_SPAN) else {
            continue;
        };
        made.push(notes::Note {
            id: index as u64,
            stamp: Stamp {
                days: 0,
                seconds: 0,
                offset: 0,
            },
            address: String::new(),
            title: String::new(),
            at: selection.at,
            before: selection.before,
            quote: selection.quote,
            after: selection.after,
            text: String::new(),
        });
    }
    let refs: Vec<&notes::Note> = made.iter().collect();
    for (note, spot) in made.iter().zip(notes::place(&page.text, &refs)) {
        match spot.place {
            Some((from, _)) if from == note.at => {}
            other => broken.push(format!("note at {} placed at {other:?}", note.at)),
        }
    }
}

/// Что интерфейсы получают от ядра об этой странице, одним JSON: модель
/// (как её видит телефон), полка сайта, ленты и вехи.
fn model_json(document: &Document, page: &Page, waypoints: &[(page::Mark, String)]) -> String {
    let mut out = String::from("{\"title\":");
    json_string(&mut out, &document.title);
    out.push_str(",\"kind\":");
    json_string(
        &mut out,
        if document.kind == Kind::Listing {
            "listing"
        } else {
            "article"
        },
    );
    out.push_str(",\"lang\":");
    json_string(&mut out, document.lang.as_deref().unwrap_or(""));
    for (name, links) in [("site", &document.site), ("feeds", &document.feeds)] {
        out.push_str(&format!(",\n\"{name}\":["));
        for (index, link) in links.iter().enumerate() {
            if index > 0 {
                out.push(',');
            }
            out.push('\n');
            json_string(&mut out, &format!("{:?}", link));
        }
        out.push(']');
    }
    out.push_str(",\n\"next\":");
    json_string(&mut out, &format!("{:?}", document.next));
    out.push_str(",\n\"waypoints\":[");
    for (index, (mark, place)) in waypoints.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        out.push('\n');
        json_string(
            &mut out,
            &format!("{} {} {}", place, mark.level, mark.title),
        );
    }
    out.push_str("],\n\"page\":");
    out.push_str(&page.to_json());
    out.push_str("}\n");
    out
}
