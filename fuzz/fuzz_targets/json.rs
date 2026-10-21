//! Свой разбор JSON (`src/json.rs`): им читаются состояние страниц Хабра,
//! JSON Feed и ответы API хостингов — всё чужое.
#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let text = String::from_utf8_lossy(data);
    let _ = brevier::json::parse(&text);
    let _ = brevier::json::parse_start(&text);
});
