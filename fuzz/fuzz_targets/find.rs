//! Поиск по странице (`page::hits`): мягкие переносы и неразрывные пробелы
//! не мешают, регистр не важен — и всё это со смещениями обратно в текст.
//! Совпадение — непустое и внутри текста. Первый байт — длина искомого.
#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Some((&length, rest)) = data.split_first() else {
        return;
    };
    let length = (length as usize % 24).min(rest.len());
    let needle = String::from_utf8_lossy(&rest[..length]);
    let text = String::from_utf8_lossy(&rest[length..]);
    let total = text.chars().count();
    let hits = brevier::page::hits(&text, &needle);
    for &(from, to) in &hits {
        assert!(from < to && to <= total, "{from}..{to} of {total}");
    }
    let _ = brevier::page::utf16_offsets(&text, &hits);
});
