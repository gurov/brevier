//! Чужой MathML через `math`: текстом или рисунком (#16). Рисует молодой
//! крейт, а разметку пишет страница: ловим панику (крючок libFuzzer видит её
//! и сквозь `catch_unwind`), зависание и рост памяти.
//!
//! Корпус — формулы со страниц, по файлу на `<math>`:
//!
//!     cargo +nightly fuzz run math fuzz/corpus/math -- -max_total_time=600 -max_len=8192
#![no_main]

use libfuzzer_sys::fuzz_target;

const PROSE: &str = "Абзац текста, достаточно длинный, чтобы извлечение признало страницу статьёй \
    и дошло до формулы, а не отказалось от неё раньше времени из-за нехватки слов на странице.";

fuzz_target!(|data: &[u8]| {
    let Ok(formula) = std::str::from_utf8(data) else {
        return;
    };
    let html = format!(
        "<html><body><article><h1>Формула</h1><p>{PROSE}</p><p>{formula}</p><p>{PROSE}</p></article></body></html>"
    );
    let _ = brevier::extract::extract(&html, "https://example.com/a-post");
});
