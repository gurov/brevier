//! Временная папка теста, которая убирает за собой.
//!
//! Тесты хранилища, архива, кэша и прогресса пишут на диск. Чистка в начале
//! теста (так было) убирала только след прошлого прогона того же процесса,
//! то есть ничего: имя несёт pid. Каждый `cargo test` оставлял во временном
//! каталоге по папке на тест — к 15 октября 2026 их набралось 2649.
//! Чистка в конце теста не срабатывает, когда тест падает. `Drop` срабатывает
//! и при панике.
//!
//! Держать в именованной переменной: `let dir = …`, не `let _ = …` — иначе
//! папка уходит сразу, а то, что тест в неё пишет, остаётся мусором.

use std::ffi::OsStr;
use std::ops::Deref;
use std::path::{Path, PathBuf};

pub(crate) struct Scratch(PathBuf);

impl Scratch {
    /// Пустая папка `brevier-<name>-<pid>-<поток>`. Сама не создаётся:
    /// её заводит тот, кто пишет, — как и в жизни.
    pub(crate) fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "brevier-{name}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        Self(dir)
    }
}

impl Deref for Scratch {
    type Target = Path;

    fn deref(&self) -> &Path {
        &self.0
    }
}

impl AsRef<Path> for Scratch {
    fn as_ref(&self) -> &Path {
        &self.0
    }
}

/// Ради `From<&Scratch> for PathBuf`: `Archive::at(&dir)` и прочие
/// `impl Into<PathBuf>` берут папку, не забирая её.
impl AsRef<OsStr> for Scratch {
    fn as_ref(&self) -> &OsStr {
        self.0.as_os_str()
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
