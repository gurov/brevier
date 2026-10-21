//! Чужой HTML от байтов до модели страницы: кодировка, проверка глубины,
//! извлечение, правила площадок, конвертер, раскладка. Первый байт выбирает
//! адрес — от него зависят правила под хост (Хабр, HN, Lobsters) и выдача
//! поиска.
//!
//! Гонять (nightly; корпус — сохранённые страницы, `corpus/frozen.py`):
//!
//!     cargo +nightly fuzz run html fuzz/corpus/html -- -max_total_time=600 -rss_limit_mb=2048
//!
//! Падение кладёт вход в `fuzz/artifacts/<цель>/`; повторить —
//! `cargo +nightly fuzz run <цель> <файл>`. Корпус и находки в git не едут:
//! в корпусе чужие страницы.
#![no_main]

use brevier::page::Page;
use libfuzzer_sys::fuzz_target;

const ADDRESSES: [&str; 7] = [
    "https://example.com/blog/a-post",
    "https://example.com/",
    "https://habr.com/ru/articles/1079940/",
    "https://habr.com/ru/articles/1079940/comments/",
    "https://news.ycombinator.com/item?id=1",
    "https://lobste.rs/s/abc123/a_story",
    "https://lite.duckduckgo.com/lite/?q=borrow+checker",
];

fuzz_target!(|data: &[u8]| {
    let Some((&pick, rest)) = data.split_first() else {
        return;
    };
    let url = ADDRESSES[pick as usize % ADDRESSES.len()];
    let html = brevier::fetch::decode(rest, None, brevier::fetch::ContentKind::Html);
    let document = if url.contains("duckduckgo") {
        brevier::from_search(&html, url)
    } else {
        brevier::from_html(&html, url)
    };
    if let Ok(document) = document {
        let page = Page::of(&document);
        let faults = page.faults();
        assert!(faults.is_empty(), "{faults:?}");
        let _ = page.to_json();
    }
});
