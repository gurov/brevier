//! Заметка (#39): выделение → цитата с окружением → место на той же
//! странице. Оставленная заметка обязана найтись там, где её оставили.
//! Первые четыре байта — начало и длина выделения.
#![no_main]

use brevier::notes::{self, Note};
use brevier::store::Stamp;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Some((head, rest)) = data.split_at_checked(4) else {
        return;
    };
    let text = String::from_utf8_lossy(rest);
    let total = text.chars().count();
    let from = u16::from_le_bytes([head[0], head[1]]) as usize % (total + 1);
    let to = from + u16::from_le_bytes([head[2], head[3]]) as usize % 256;
    let Ok(selection) = notes::select(&text, from, to) else {
        return;
    };
    let note = Note {
        id: 1,
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
    };
    let spots = notes::place(&text, &[&note]);
    let placed = spots[0].place;
    assert_eq!(placed.map(|(start, _)| start), Some(note.at), "{placed:?}");
    let (start, end) = placed.unwrap();
    assert!(start < end && end <= total);
});
