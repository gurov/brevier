//! Картинки страниц, что хранятся навсегда (#42).
//!
//! Закладка обещает копию навсегда, и страница с заметками тоже
//! (`notes::lasting`), но до сих пор копия была одним текстом: картинки
//! умирали вместе с сайтом, а в документации, схемах и снимках экрана
//! они — половина страницы. Теперь у такой страницы картинки лежат в архиве
//! (`Archive::keep_picture`), а показ копии, да и самой страницы, берёт их
//! оттуда раньше сети.
//!
//! **Новых запросов нет.** В архив кладётся то, что уже скачано для показа:
//! картинка, пришедшая к странице, хранящейся навсегда ([`offer`]), и, когда
//! страницу только что положили в закладки или написали к ней заметку, те
//! её картинки, что уже лежат в памяти вкладки или в недельном кэше
//! ([`gather`]). Не скачанная картинка ради архива не скачивается, а
//! выключенные картинки так и остаются выключенными. Переключатель архива
//! выключает и это.

use crate::Document;
use crate::address::{Address, ArchivePage, Internal};
use crate::archive::Archive;
use crate::cache::Cache;
use crate::media::{self, Source};

/// Чья это страница для архива: у копии (`brevier:archive/<путь>`) — та,
/// с которой её сняли; у веб-страницы и репозитория — она сама.
pub fn page_of(address: &Address) -> Option<String> {
    match address {
        Address::Internal(Internal::Archive(ArchivePage::Copy(path))) => {
            Archive::open().source_of(path)
        }
        Address::Web(_) | Address::Repo(_) => Some(address.display()),
        _ => None,
    }
}

/// Картинка страницы `page` из архива, если её туда клали.
pub fn from_archive(page: &Address, url: &str) -> Option<Vec<u8>> {
    let source = page_of(page)?;
    Archive::open().picture(&source, url)
}

/// Картинка, скачанная для страницы `page`: страница хранится навсегда —
/// картинку в архив, байт в байт.
pub fn offer(page: &Address, url: &str, bytes: &[u8]) {
    let Some(source) = page_of(page) else {
        return;
    };
    let source = source.split('#').next().unwrap_or_default().to_owned();
    if !crate::store::Settings::load().archive || !crate::notes::lasting().contains(&source) {
        return;
    }
    let ext = crate::save::picture_type(bytes).map_or("bin", |(ext, _)| ext);
    Archive::open().keep_picture(&source, url, bytes, ext);
}

/// Страницу только что положили в закладки или написали к ней заметку:
/// её картинки, что уже под рукой (`held` — память вкладки, затем недельный
/// кэш), — в архив. Чего под рукой нет, то не скачиваем.
pub fn gather(document: &Document, held: impl Fn(&str) -> Option<Vec<u8>>) {
    let disk = Cache::open();
    for link in crate::markdown::images(&document.markdown) {
        let Some(Source::Web(url)) = media::resolve(&document.address, &link) else {
            continue;
        };
        if let Some(bytes) = held(&url).or_else(|| disk.image(&url)) {
            offer(&document.address, &url, &bytes);
        }
    }
}
