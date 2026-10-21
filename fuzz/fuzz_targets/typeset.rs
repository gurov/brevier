//! Переносы и неразрывные пробелы (`typeset`): набор только добавляет
//! невидимое — снятый обратно (`plain`), текст тот же, что до набора.
#![no_main]

use libfuzzer_sys::fuzz_target;

const LANGS: [&str; 4] = ["en", "ru", "de", "uk"];

fuzz_target!(|data: &[u8]| {
    let Some((&pick, rest)) = data.split_first() else {
        return;
    };
    let text = String::from_utf8_lossy(rest);
    let lang = LANGS[pick as usize % LANGS.len()];
    let set = brevier::typeset::typeset(&text, lang);
    assert_eq!(
        brevier::typeset::plain(&set),
        brevier::typeset::plain(&text)
    );
});
