//! RSS, Atom и JSON Feed до модели страницы.
#![no_main]

use brevier::page::Page;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let text = String::from_utf8_lossy(data);
    if let Ok(document) = brevier::from_feed(&text, "https://example.com/feed.xml") {
        let page = Page::of(&document);
        let faults = page.faults();
        assert!(faults.is_empty(), "{faults:?}");
    }
});
