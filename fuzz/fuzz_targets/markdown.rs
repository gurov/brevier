//! Markdown до модели страницы: разбор (comrak с нашими расширениями),
//! раскладка, переносы по языку, оглавление, вехи. Так открывается и
//! `.md` с диска, и документация из репозитория. Первый байт — язык.
#![no_main]

use brevier::page::Page;
use brevier::{Address, Document, Kind};
use libfuzzer_sys::fuzz_target;

const LANGS: [Option<&str>; 4] = [None, Some("en"), Some("ru"), Some("de")];

fuzz_target!(|data: &[u8]| {
    let Some((&pick, rest)) = data.split_first() else {
        return;
    };
    let markdown = String::from_utf8_lossy(rest).into_owned();
    let _ = brevier::markdown::plain_text(&markdown);
    let document = Document {
        address: Address::Web("https://example.com/docs/guide.md".to_owned()),
        title: String::new(),
        markdown,
        kind: Kind::Article,
        served: false,
        site: Vec::new(),
        feeds: Vec::new(),
        lang: LANGS[pick as usize % LANGS.len()].map(str::to_owned),
        next: None,
        archived: None,
    };
    let page = Page::of(&document);
    let faults = page.faults();
    assert!(faults.is_empty(), "{faults:?}");
    let _ = page.to_json();
});
