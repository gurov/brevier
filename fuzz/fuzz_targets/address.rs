//! Адресная строка: всё, что читатель может набрать или вставить, и всё,
//! что приходит ссылкой снаружи.
#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let text = String::from_utf8_lossy(data);
    if let Ok(address) = brevier::address::parse(&text) {
        let _ = address.display();
        let _ = address.external();
        let _ = brevier::page::anchor_in(&address);
        let _ = brevier::address::may_follow(None, &address);
    }
});
