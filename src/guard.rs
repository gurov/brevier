//! Страница, уронившая Brevier, не роняет его снова на каждом запуске (#57).
//!
//! Сессия поднимает переднюю вкладку сразу, и если падение — на ней (на
//! раскладке, на картинке, на нехватке памяти), то оно повторится и на
//! следующем запуске, и на всех после. Читателю остаётся стереть данные
//! приложения, а с ними историю, закладки, заметки и архив.
//!
//! Поймать такое падение изнутри нельзя: нехватку памяти система лечит
//! SIGKILL, неудачное выделение памяти в Rust — abort, сбой в C-коде GTK —
//! сигнал. Поэтому наоборот: пока ядро работает над страницей (открывает,
//! раскладывает, разбирает её картинки), на диске лежит пометка с её
//! адресом, а кончило — снимает. Пометка, пережившая процесс, значит, что он
//! умер посреди работы, и эту страницу следующий запуск сам не открывает:
//! показывает карточку (`failure::closed_last_time`) — открыть ли снова.
//!
//! Включает сторожа интерфейс (`watch`); cli и корпус его не видят.
//! Ложная тревога возможна: систему, убившую приложение в фоне посреди
//! загрузки, от падения не отличить. Цена — одна лишняя кнопка.

use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};

/// Файл пометки — рядом с сессией, и он объясняет себя сам, как она.
const FILE: &str = "working.txt";
const HEADER: &str = "# Brevier is working on this page; the line goes away when it is done.\n";

#[derive(Default)]
struct Watch {
    /// Куда писать пометку; `None` — сторож не включён.
    path: Option<PathBuf>,
    /// Сколько работ идёт сейчас: страницы и картинки, в любых потоках.
    busy: usize,
    /// Две последние страницы, над которыми шла работа: картинки — их.
    /// Две — потому что просили одну, а после перенаправления раскладывают
    /// другую (`Page::guarded` знает уже итоговый адрес).
    pages: Vec<String>,
    /// Что лежит в файле сейчас, чтобы не писать то же самое снова.
    written: Vec<String>,
    /// Страницы, посреди работы над которыми умер прошлый запуск.
    suspects: Vec<String>,
}

static WATCH: Mutex<Watch> = Mutex::new(Watch {
    path: None,
    busy: 0,
    pages: Vec::new(),
    written: Vec::new(),
    suspects: Vec::new(),
});

fn watch_lock() -> MutexGuard<'static, Watch> {
    // Паника в чужом потоке посреди работы не должна выключать сторожа.
    WATCH
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Включить сторожа и прочитать пометку, оставленную прошлым запуском.
/// Зовёт интерфейс один раз, на старте.
pub fn watch() {
    if let Some(dir) = crate::store::data_dir() {
        let _ = std::fs::create_dir_all(&dir);
        watch_lock().start(dir.join(FILE));
    }
}

/// Не открывать ли страницу самим. `true` — один раз, для той, на которой
/// умер прошлый запуск: дальше её открывают как любую.
pub fn suspected(address: &str) -> bool {
    watch_lock().suspected(address)
}

/// Работа над страницей: открыть, разложить. Пометка держится, пока жив
/// ответ.
#[must_use]
pub fn opening(address: &str) -> Busy {
    watch_lock().begin(Some(address));
    Busy(())
}

/// Работа над картинкой — той страницы, что открывали последней.
#[must_use]
pub fn picture() -> Busy {
    watch_lock().begin(None);
    Busy(())
}

/// Приложение падает прямо сейчас, и его код знает, на какой странице:
/// пометка — сразу (необработанное исключение на Android).
pub fn fell(address: &str) {
    watch_lock().fell(address);
}

/// Идущая работа; кончилась — пометка снимается, если других нет.
pub struct Busy(());

impl Drop for Busy {
    fn drop(&mut self) {
        watch_lock().end();
    }
}

impl Watch {
    fn start(&mut self, path: PathBuf) {
        self.suspects = std::fs::read_to_string(&path)
            .map(|text| {
                text.lines()
                    .map(str::trim)
                    .filter(|line| !line.is_empty() && !line.starts_with('#'))
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default();
        let _ = std::fs::remove_file(&path);
        self.path = Some(path);
        self.busy = 0;
        self.written.clear();
    }

    /// Карточка — одна: на первой из подозреваемых страниц, которую станут
    /// открывать. Вторая — та же страница под другим адресом или соседка.
    fn suspected(&mut self, address: &str) -> bool {
        if self.suspects.iter().any(|suspect| suspect == address) {
            self.suspects.clear();
            return true;
        }
        false
    }

    fn begin(&mut self, page: Option<&str>) {
        self.busy += 1;
        if let Some(page) = page {
            self.remember(page);
        }
        self.sync();
    }

    fn remember(&mut self, page: &str) {
        if self.pages.last().map(String::as_str) != Some(page) {
            self.pages.retain(|known| known != page);
            self.pages.push(page.to_owned());
            if self.pages.len() > 2 {
                self.pages.remove(0);
            }
        }
    }

    fn end(&mut self) {
        self.busy = self.busy.saturating_sub(1);
        self.sync();
    }

    fn fell(&mut self, address: &str) {
        self.remember(address);
        self.busy += 1;
        self.sync();
    }

    /// Привести файл к тому, что идёт сейчас: работа есть — адрес её
    /// страницы, нет — файла нет.
    fn sync(&mut self) {
        let Some(path) = &self.path else {
            return;
        };
        let wanted = if self.busy > 0 {
            self.pages.clone()
        } else {
            Vec::new()
        };
        if wanted == self.written {
            return;
        }
        if wanted.is_empty() {
            let _ = std::fs::remove_file(path);
        } else {
            let mut text = String::from(HEADER);
            for page in &wanted {
                text.push_str(page);
                text.push('\n');
            }
            let _ = std::fs::write(path, text);
        }
        self.written = wanted;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scratch::Scratch;

    #[test]
    fn the_mark_lives_while_the_page_is_worked_on() {
        let dir = Scratch::new("guard-work");
        std::fs::create_dir_all(&*dir).unwrap();
        let path = dir.join(FILE);
        let mut watch = Watch::default();
        watch.start(path.clone());

        watch.begin(Some("https://example.com/a"));
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.ends_with("https://example.com/a\n"));
        watch.end();
        assert!(!path.exists());

        // Картинки приходят после страницы — и пометка снова её.
        watch.begin(None);
        watch.begin(None);
        watch.end();
        assert!(path.exists());
        watch.end();
        assert!(!path.exists());
    }

    #[test]
    fn a_run_that_died_working_holds_the_page_back_once() {
        let dir = Scratch::new("guard-died");
        std::fs::create_dir_all(&*dir).unwrap();
        let path = dir.join(FILE);
        let mut dying = Watch::default();
        dying.start(path.clone());
        dying.begin(Some("https://example.com/heavy"));
        // Процесс умер: `end` не случилось.

        let mut next = Watch::default();
        next.start(path.clone());
        assert!(!path.exists());
        assert!(!next.suspected("https://example.com/other"));
        assert!(next.suspected("https://example.com/heavy"));
        assert!(!next.suspected("https://example.com/heavy"));
    }

    /// Просили один адрес, разложили другой (перенаправление) — карточку
    /// получит тот, что откроют первым, и только один.
    #[test]
    fn a_redirected_page_is_known_by_both_addresses() {
        let dir = Scratch::new("guard-redirect");
        std::fs::create_dir_all(&*dir).unwrap();
        let path = dir.join(FILE);
        let mut dying = Watch::default();
        dying.start(path.clone());
        dying.begin(Some("http://example.com/old"));
        dying.begin(Some("https://example.com/new"));
        let mut next = Watch::default();
        next.start(path);
        assert!(next.suspected("http://example.com/old"));
        assert!(!next.suspected("https://example.com/new"));
    }

    #[test]
    fn a_fall_marks_the_page_at_once() {
        let dir = Scratch::new("guard-fell");
        std::fs::create_dir_all(&*dir).unwrap();
        let path = dir.join(FILE);
        let mut watch = Watch::default();
        watch.start(path.clone());
        watch.fell("https://example.com/b");
        let mut next = Watch::default();
        next.start(path);
        assert!(next.suspected("https://example.com/b"));
    }

    #[test]
    fn an_idle_watch_writes_nothing() {
        let mut watch = Watch::default();
        watch.begin(Some("https://example.com/c"));
        watch.end();
        assert!(watch.written.is_empty());
    }
}
