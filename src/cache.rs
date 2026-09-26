//! Прочитанное на неделю: страница открывается с диска, а не из сети.
//!
//! Решено 25 сентября 2026 указанием мейнтейнера: страницу, открытую
//! недавно, показывать из сохранённого, а не скачивать и разбирать заново.
//! Браузеры делают похожее своим HTTP-кэшем, но по правилам сервера
//! (`Cache-Control`), и статья без них уходит в сеть каждый раз. Правило
//! здесь своё и одно: неделя с того часа, как страницу скачали.
//!
//! **Что кладём** — документ целиком, уже разобранный: markdown, заголовок,
//! навигацию сайта, язык. С диска не повторяются ни запрос, ни извлечение,
//! ни конвертация; остаётся одна отрисовка. Картинки — сырыми байтами под
//! адресом источника, как и в памяти вкладки.
//!
//! **Чего не кладём.** Ленту (`Kind::Listing`): главная блога и каталог
//! репозитория меняются, ради нового на них и приходят, и недельная лента
//! была бы враньём. Свои страницы — история обязана показывать диск. Локальные
//! файлы — диск и так рядом, а правку читатель ждёт увидеть сразу.
//!
//! **Честность.** Страница из копии говорит об этом строкой состояния
//! ([`copy_note`]), а свежую загрузку даёт перезагрузка. «Забыть всё»
//! опустошает и это: копии страниц — такой же след прочитанного, как журнал.
//!
//! **Формат** — как у остального хранилища, текст. Файл на страницу, имя —
//! хеш адреса; шапка строками `ключ<TAB>значение`, пустая строка, дальше
//! markdown как есть. Копию можно открыть любым редактором.
//!
//! **Пишет только интерфейс.** cli — инструмент конвейера, и `--check`
//! с корпусом обязаны видеть сеть, а не вчерашний слепок.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::address::{self, Address};
use crate::extract::Link;
use crate::markdown::Kind;
use crate::{Document, store};

/// Сколько живёт копия. Неделя — слово мейнтейнера: статью дочитывают
/// за несколько дней, а через неделю она скорее новость, чем закладка.
pub const KEEP_FOR: Duration = Duration::from_secs(7 * 24 * 3600);

/// Потолки на диск. Страница в markdown — десятки килобайт, так что 64 МиБ
/// это тысячи страниц; картинки тяжелее, и потолок у них свой.
const PAGES_BUDGET: u64 = 64 * 1024 * 1024;
const IMAGES_BUDGET: u64 = 256 * 1024 * 1024;

/// Первая строка файла страницы. Формат сменится — сменится и она, а старые
/// копии просто не прочтутся и уйдут с первой уборкой.
const PAGE_MAGIC: &str = "brevier-page 1";

/// Копия страницы и когда её скачали.
#[derive(Debug, Clone)]
pub struct Saved {
    pub document: Document,
    pub saved: SystemTime,
}

/// Кэш прочитанного: две папки, страницы и картинки. `None` — положить
/// некуда (нет домашней папки); тогда кэша просто нет, а чтение идёт как шло.
#[derive(Debug, Clone)]
pub struct Cache {
    pages: Option<PathBuf>,
    images: Option<PathBuf>,
}

impl Cache {
    /// Кэш на своём месте — в папке кэша по XDG (`store::cache_dir`).
    pub fn open() -> Self {
        Self::within(store::cache_dir())
    }

    /// Кэш в названной папке — для тестов.
    pub fn at(dir: impl Into<PathBuf>) -> Self {
        Self::within(Some(dir.into()))
    }

    fn within(dir: Option<PathBuf>) -> Self {
        Self {
            pages: dir.as_ref().map(|dir| dir.join("pages")),
            images: dir.map(|dir| dir.join("images")),
        }
    }

    /// Свежая копия страницы, если она есть.
    pub fn page(&self, address: &Address) -> Option<Saved> {
        self.page_at(address, SystemTime::now())
    }

    fn page_at(&self, address: &Address, now: SystemTime) -> Option<Saved> {
        let key = key(address)?;
        let path = self.pages.as_ref()?.join(file_name(&key, "page"));
        let text = fs::read_to_string(path).ok()?;
        let saved = read_page(&text, &key)?;
        fresh(saved.saved, now).then_some(saved)
    }

    /// Положить страницу. Под двумя ключами, если адрес переехал: тем, что
    /// просили (по нему придут в следующий раз), и итоговым.
    pub fn keep(&self, asked: &Address, document: &Document) {
        self.keep_at(asked, document, SystemTime::now());
    }

    fn keep_at(&self, asked: &Address, document: &Document, now: SystemTime) {
        let Some(dir) = &self.pages else { return };
        if !worth_keeping(document) {
            return;
        }
        let mut keys: Vec<String> = [asked, &document.address]
            .into_iter()
            .filter_map(key)
            .collect();
        keys.dedup();
        for key in keys {
            let text = write_page(&key, document, now);
            write_atomically(&dir.join(file_name(&key, "page")), text.as_bytes());
        }
    }

    /// Сырые байты картинки, если её копия свежа.
    pub fn image(&self, url: &str) -> Option<Vec<u8>> {
        self.image_at(url, SystemTime::now())
    }

    fn image_at(&self, url: &str, now: SystemTime) -> Option<Vec<u8>> {
        let path = self.images.as_ref()?.join(file_name(url, "img"));
        let modified = fs::metadata(&path).ok()?.modified().ok()?;
        if !fresh(modified, now) {
            return None;
        }
        let bytes = fs::read(path).ok()?;
        // Первая строка — адрес: имя файла — хеш, и совпадение хешей
        // не должно подсунуть чужую картинку.
        let split = bytes.iter().position(|&b| b == b'\n')?;
        (&bytes[..split] == url.as_bytes()).then(|| bytes[split + 1..].to_vec())
    }

    /// Положить картинку сырыми байтами.
    pub fn keep_image(&self, url: &str, bytes: &[u8]) {
        let Some(dir) = &self.images else { return };
        let mut out = Vec::with_capacity(url.len() + 1 + bytes.len());
        out.extend_from_slice(url.as_bytes());
        out.push(b'\n');
        out.extend_from_slice(bytes);
        write_atomically(&dir.join(file_name(url, "img")), &out);
    }

    /// Уборка: выбросить устаревшее, затем старое сверх потолка. Зовёт
    /// интерфейс на запуске, в фоне.
    pub fn prune(&self) {
        self.prune_at(SystemTime::now());
    }

    fn prune_at(&self, now: SystemTime) {
        if let Some(dir) = &self.pages {
            prune_dir(dir, now, PAGES_BUDGET);
        }
        if let Some(dir) = &self.images {
            prune_dir(dir, now, IMAGES_BUDGET);
        }
    }

    /// Забыть всё: копии страниц и картинок — след прочитанного.
    pub fn forget(&self) {
        for dir in [&self.pages, &self.images].into_iter().flatten() {
            let _ = fs::remove_dir_all(dir);
        }
    }
}

/// Строка состояния для страницы из копии: откуда она и какой давности.
/// Как получить свежую — дописывает интерфейс: клавиша на десктопе, пункт
/// меню на телефоне.
pub fn copy_note(saved: SystemTime) -> String {
    copy_note_at(saved, SystemTime::now())
}

fn copy_note_at(saved: SystemTime, now: SystemTime) -> String {
    let seconds = now.duration_since(saved).unwrap_or_default().as_secs();
    let minutes = seconds / 60;
    let hours = minutes / 60;
    let days = hours / 24;
    let age = match (days, hours, minutes) {
        (0, 0, 0) => "a moment ago".to_owned(),
        (0, 0, 1) => "a minute ago".to_owned(),
        (0, 0, m) => format!("{m} minutes ago"),
        (0, 1, _) => "an hour ago".to_owned(),
        (0, h, _) => format!("{h} hours ago"),
        (1, _, _) => "yesterday".to_owned(),
        (d, _, _) => format!("{d} days ago"),
    };
    format!("From your copy, saved {age}.")
}

/// Годится ли документ в кэш: статья из сети или из репозитория.
fn worth_keeping(document: &Document) -> bool {
    document.kind == Kind::Article && matches!(document.address, Address::Web(_) | Address::Repo(_))
}

/// Ключ копии — адрес без решётки: якорь дело читателя, страница та же.
/// У своих страниц и файлов ключа нет — их не кладём.
fn key(address: &Address) -> Option<String> {
    match address {
        Address::Web(url) => Some(url.split('#').next().unwrap_or(url).to_owned()),
        Address::Repo(_) => Some(address.display()),
        Address::File(_) | Address::Internal(_) => None,
    }
}

fn fresh(saved: SystemTime, now: SystemTime) -> bool {
    now.duration_since(saved)
        .map(|age| age < KEEP_FOR)
        // Копия «из будущего» — часы переводили; считаем её свежей.
        .unwrap_or(true)
}

/// Имя файла: FNV-1a от ключа. Адрес в имя файла не годится — он длиннее
/// любого предела и полон запрещённых знаков; сам ключ лежит внутри файла.
fn file_name(key: &str, extension: &str) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in key.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    format!("{hash:016x}.{extension}")
}

/// Табуляция и перевод строки в поле шапки ломали бы разбор; в заголовке
/// они и так ничего не значат.
fn field(text: &str) -> String {
    text.replace(['\t', '\n', '\r'], " ")
}

fn write_page(key: &str, document: &Document, now: SystemTime) -> String {
    let saved = now.duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
    let mut out = format!("{PAGE_MAGIC}\nkey\t{}\n", field(key));
    match &document.address {
        Address::Repo(_) => {
            out.push_str(&format!("address\trepo\t{}\n", document.address.display()))
        }
        address => out.push_str(&format!("address\tweb\t{}\n", field(&address.external()))),
    }
    out.push_str(&format!("saved\t{saved}\n"));
    out.push_str(&format!("title\t{}\n", field(&document.title)));
    out.push_str(&format!("served\t{}\n", u8::from(document.served)));
    if let Some(lang) = &document.lang {
        out.push_str(&format!("lang\t{}\n", field(lang)));
    }
    for link in &document.site {
        out.push_str(&format!(
            "site\t{}\t{}\n",
            field(&link.address),
            field(&link.title)
        ));
    }
    for link in &document.feeds {
        out.push_str(&format!(
            "feed\t{}\t{}\n",
            field(&link.address),
            field(&link.title)
        ));
    }
    out.push('\n');
    out.push_str(&document.markdown);
    out
}

/// Разобрать копию. `None` — не наш формат, чужой ключ или битая шапка:
/// такую копию просто не видим, а уборка её выбросит.
fn read_page(text: &str, expected: &str) -> Option<Saved> {
    let (head, markdown) = text.split_once("\n\n")?;
    let mut lines = head.lines();
    if lines.next()? != PAGE_MAGIC {
        return None;
    }

    let mut key = None;
    let mut address = None;
    let mut saved = None;
    let mut title = String::new();
    let mut served = false;
    let mut lang = None;
    let mut site = Vec::new();
    let mut feeds = Vec::new();
    for line in lines {
        let (name, value) = line.split_once('\t')?;
        match name {
            "key" => key = Some(value),
            "address" => {
                address = match value.split_once('\t')? {
                    // Веб храним как есть, а не разбором: ссылка на github,
                    // открытая страницей, разбором стала бы репозиторием.
                    ("web", url) => Some(Address::Web(url.to_owned())),
                    ("repo", shown) => address::parse(shown).ok(),
                    _ => None,
                }
            }
            "saved" => saved = value.parse::<u64>().ok(),
            "title" => title = value.to_owned(),
            "served" => served = value == "1",
            "lang" => lang = Some(value.to_owned()),
            "site" | "feed" => {
                let (address, title) = value.split_once('\t')?;
                let link = Link {
                    title: title.to_owned(),
                    address: address.to_owned(),
                };
                if name == "site" {
                    site.push(link);
                } else {
                    feeds.push(link);
                }
            }
            _ => {}
        }
    }
    if key? != expected {
        return None;
    }

    Some(Saved {
        document: Document {
            address: address?,
            title,
            markdown: markdown.to_owned(),
            kind: Kind::Article,
            served,
            site,
            feeds,
            lang,
        },
        saved: UNIX_EPOCH + Duration::from_secs(saved?),
    })
}

/// Записать целиком или никак: сначала во временный файл, потом переименовать.
/// Окон бывает несколько, и копию одной страницы могут писать двое сразу —
/// поэтому у временного файла имя своё у каждого писателя.
fn write_atomically(path: &Path, bytes: &[u8]) {
    static WRITER: AtomicU64 = AtomicU64::new(0);
    let Some(dir) = path.parent() else { return };
    if fs::create_dir_all(dir).is_err() {
        return;
    }
    let temporary = path.with_extension(format!(
        "{}-{}.tmp",
        std::process::id(),
        WRITER.fetch_add(1, Ordering::Relaxed)
    ));
    if fs::write(&temporary, bytes).is_ok() && fs::rename(&temporary, path).is_err() {
        let _ = fs::remove_file(&temporary);
    }
}

/// Выбросить устаревшее, а если и после этого тяжело — самое старое, пока
/// не влезет в потолок. Время — по дате изменения: копию пишут целиком,
/// и она совпадает с часом скачивания.
fn prune_dir(dir: &Path, now: SystemTime, budget: u64) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut kept: Vec<(SystemTime, u64, PathBuf)> = Vec::new();
    for entry in entries.flatten() {
        let Ok(meta) = entry.metadata() else { continue };
        if !meta.is_file() {
            continue;
        }
        let modified = meta.modified().unwrap_or(UNIX_EPOCH);
        if fresh(modified, now) {
            kept.push((modified, meta.len(), entry.path()));
        } else {
            let _ = fs::remove_file(entry.path());
        }
    }
    // Свежее первым: оно и остаётся.
    kept.sort_by_key(|entry| std::cmp::Reverse(entry.0));
    let mut total = 0u64;
    for (_, size, path) in kept {
        total += size;
        if total > budget {
            let _ = fs::remove_file(path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temporary(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "brevier-cache-{name}-{}-{nanos}",
            std::process::id()
        ))
    }

    fn article(url: &str) -> Document {
        Document {
            address: Address::Web(url.to_owned()),
            title: "A title\twith a tab".to_owned(),
            markdown: "# A title\n\nFirst paragraph.\n\n\nAfter a gap.\n".to_owned(),
            kind: Kind::Article,
            served: true,
            site: vec![Link {
                title: "Home".to_owned(),
                address: "https://e.com/".to_owned(),
            }],
            feeds: vec![Link {
                title: "Blog » Feed".to_owned(),
                address: "https://e.com/feed/".to_owned(),
            }],
            lang: Some("ru".to_owned()),
        }
    }

    #[test]
    fn a_page_comes_back_as_it_went() {
        let dir = temporary("roundtrip");
        let cache = Cache::at(&dir);
        let page = Address::Web("https://e.com/a".to_owned());
        cache.keep(&page, &article("https://e.com/a"));

        let saved = cache.page(&page).expect("копия есть");
        let document = saved.document;
        assert_eq!(document.address, page);
        // Табуляция в заголовке — пробел: иначе шапку не разобрать.
        assert_eq!(document.title, "A title with a tab");
        // Тело — байт в байт, с пустыми строками внутри.
        assert_eq!(document.markdown, article("").markdown);
        assert!(document.served);
        assert_eq!(document.lang.as_deref(), Some("ru"));
        assert_eq!(document.site, article("").site);
        // Ленты страницы — тоже: полка из копии та же, что из сети.
        assert_eq!(document.feeds, article("").feeds);
        // Решётка — дело читателя: страница та же.
        assert!(
            cache
                .page(&Address::Web("https://e.com/a#part".to_owned()))
                .is_some()
        );
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn a_moved_page_is_kept_under_both_addresses() {
        let dir = temporary("moved");
        let cache = Cache::at(&dir);
        let asked = Address::Web("http://e.com/old".to_owned());
        cache.keep(&asked, &article("https://e.com/new"));
        assert!(cache.page(&asked).is_some());
        assert!(
            cache
                .page(&Address::Web("https://e.com/new".to_owned()))
                .is_some()
        );
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn a_week_old_copy_is_not_shown() {
        let dir = temporary("stale");
        let cache = Cache::at(&dir);
        let page = Address::Web("https://e.com/a".to_owned());
        let then = SystemTime::now() - KEEP_FOR - Duration::from_secs(60);
        cache.keep_at(&page, &article("https://e.com/a"), then);
        assert!(cache.page(&page).is_none());

        let lately = SystemTime::now() - KEEP_FOR + Duration::from_secs(3600);
        cache.keep_at(&page, &article("https://e.com/a"), lately);
        assert!(cache.page(&page).is_some());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn a_listing_and_our_own_pages_are_not_kept() {
        let dir = temporary("listing");
        let cache = Cache::at(&dir);
        let page = Address::Web("https://e.com/".to_owned());
        let mut listing = article("https://e.com/");
        listing.kind = Kind::Listing;
        cache.keep(&page, &listing);
        assert!(cache.page(&page).is_none());

        let history = Address::Internal(address::Internal::History);
        let mut own = article("https://e.com/");
        own.address = history.clone();
        cache.keep(&history, &own);
        assert!(cache.page(&history).is_none());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn a_github_page_stays_a_web_page() {
        // Страница хостинга, открытая веб-страницей (исходник, картинка):
        // разбор её адреса дал бы репозиторий, а копия обязана вернуть веб.
        let dir = temporary("blob");
        let cache = Cache::at(&dir);
        let url = "https://github.com/o/n/blob/HEAD/src/main.rs";
        let page = Address::Web(url.to_owned());
        cache.keep(&page, &article(url));
        assert_eq!(cache.page(&page).unwrap().document.address, page);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn a_copy_under_another_key_is_not_taken() {
        let dir = temporary("collision");
        let cache = Cache::at(&dir);
        let a = Address::Web("https://e.com/a".to_owned());
        let b = Address::Web("https://e.com/b".to_owned());
        cache.keep(&b, &article("https://e.com/b"));
        // Будто хеши совпали: под именем `a` лежит копия `b`.
        let pages = dir.join("pages");
        fs::rename(
            pages.join(file_name("https://e.com/b", "page")),
            pages.join(file_name("https://e.com/a", "page")),
        )
        .unwrap();
        assert!(cache.page(&a).is_none());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn images_are_kept_as_raw_bytes() {
        let dir = temporary("images");
        let cache = Cache::at(&dir);
        let url = "https://e.com/a.png";
        let bytes = b"\x89PNG\r\n\x1a\n\nnewlines inside".to_vec();
        cache.keep_image(url, &bytes);
        assert_eq!(cache.image(url), Some(bytes));
        assert_eq!(cache.image("https://e.com/b.png"), None);
        // Через неделю картинки нет, как и страницы.
        assert_eq!(cache.image_at(url, SystemTime::now() + KEEP_FOR), None);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn pruning_drops_the_old_and_forgetting_drops_everything() {
        let dir = temporary("prune");
        let cache = Cache::at(&dir);
        let page = Address::Web("https://e.com/a".to_owned());
        cache.keep(&page, &article("https://e.com/a"));
        cache.keep_image("https://e.com/a.png", b"bytes");

        // Сегодня уборке выбрасывать нечего.
        cache.prune();
        assert!(cache.page(&page).is_some());
        // Через неделю — всё.
        cache.prune_at(SystemTime::now() + KEEP_FOR + Duration::from_secs(1));
        assert_eq!(fs::read_dir(dir.join("pages")).unwrap().count(), 0);
        assert_eq!(fs::read_dir(dir.join("images")).unwrap().count(), 0);

        cache.keep(&page, &article("https://e.com/a"));
        cache.forget();
        assert!(cache.page(&page).is_none());
        assert!(!dir.join("pages").exists());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn pruning_keeps_the_freshest_within_the_budget() {
        let dir = temporary("budget");
        for (name, age) in [("old", 30), ("new", 10)] {
            let path = dir.join(name);
            fs::create_dir_all(&dir).unwrap();
            fs::write(&path, vec![0u8; 600]).unwrap();
            let file = fs::File::options().write(true).open(&path).unwrap();
            file.set_modified(SystemTime::now() - Duration::from_secs(age))
                .unwrap();
        }
        prune_dir(&dir, SystemTime::now(), 1000);
        assert!(dir.join("new").exists());
        assert!(!dir.join("old").exists());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn the_note_says_how_old_the_copy_is() {
        let now = SystemTime::now();
        let ago = |seconds: u64| copy_note_at(now - Duration::from_secs(seconds), now);
        assert_eq!(ago(5), "From your copy, saved a moment ago.");
        assert_eq!(ago(90), "From your copy, saved a minute ago.");
        assert_eq!(ago(20 * 60), "From your copy, saved 20 minutes ago.");
        assert_eq!(ago(3 * 3600), "From your copy, saved 3 hours ago.");
        assert_eq!(ago(30 * 3600), "From your copy, saved yesterday.");
        assert_eq!(ago(4 * 24 * 3600), "From your copy, saved 4 days ago.");
    }
}
