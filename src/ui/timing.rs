//! Замер запуска окна: `BREVIER_TIMING=1` — и окно пишет в stderr, сколько
//! прошло от создания процесса до каждой вехи (`main`, регистрация на шине,
//! окно собрано, первый кадр).
//!
//! Зачем: на Windows окно встаёт секундами, и читатель, не дождавшись,
//! запускает его ещё раз. Что лечить — библиотеки и антивирус до `main`
//! или нашу работу после, — без чисел не решить, а снять их можно только
//! на чужой машине. Отсчёт идёт от создания процесса, а не от `main`:
//! загрузка DLL до `main` — первый подозреваемый.
//!
//! На Windows окно — программа без консоли: вывод виден, только если его
//! перенаправить (`brevier-ui.exe > timing.txt 2>&1`).

use std::sync::OnceLock;
use std::time::{Duration, Instant, SystemTime};

struct Clock {
    main: Instant,
    /// От создания процесса до `main`; `None` — система не сказала.
    before_main: Option<Duration>,
}

static CLOCK: OnceLock<Option<Clock>> = OnceLock::new();

/// Завести часы в самом начале `main` и записать первую веху.
pub fn start() {
    let clock = CLOCK.get_or_init(|| {
        std::env::var_os("BREVIER_TIMING")
            .is_some_and(|value| !value.is_empty() && value != "0")
            .then(|| Clock {
                main: Instant::now(),
                before_main: since_creation(),
            })
    });
    if let Some(clock) = clock {
        match clock.before_main {
            Some(_) => mark("main"),
            None => mark("main (the process start is unknown, counting from here)"),
        }
    }
}

/// Веха: сколько миллисекунд прошло от создания процесса.
pub fn mark(what: &str) {
    if let Some(Some(clock)) = CLOCK.get() {
        let total = clock.before_main.unwrap_or_default() + clock.main.elapsed();
        eprintln!(
            "brevier-ui timing [{}]: {:>6} ms  {what}",
            std::process::id(),
            total.as_millis()
        );
    }
}

/// Включён ли замер — чтобы не собирать строку вехи зря.
pub fn on() -> bool {
    matches!(CLOCK.get(), Some(Some(_)))
}

/// Когда создан этот процесс, по часам системы (не сказала — «сейчас»).
/// Часы общие для всех процессов машины: так главный экземпляр сравнит
/// чужой запуск со своим первым кадром.
pub fn launched_at() -> SystemTime {
    let now = SystemTime::now();
    since_creation()
        .and_then(|age| now.checked_sub(age))
        .unwrap_or(now)
}

#[cfg(windows)]
fn since_creation() -> Option<Duration> {
    use std::ffi::c_void;

    #[repr(C)]
    #[derive(Default, Clone, Copy)]
    struct FileTime {
        low: u32,
        high: u32,
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetCurrentProcess() -> *mut c_void;
        fn GetProcessTimes(
            process: *mut c_void,
            creation: *mut FileTime,
            exit: *mut FileTime,
            kernel: *mut FileTime,
            user: *mut FileTime,
        ) -> i32;
        fn GetSystemTimePreciseAsFileTime(now: *mut FileTime);
    }

    let mut creation = FileTime::default();
    let mut exit = FileTime::default();
    let mut kernel = FileTime::default();
    let mut user = FileTime::default();
    let mut now = FileTime::default();
    // Безопасно: дескриптор своего процесса — псевдодескриптор, закрывать
    // его не нужно; структуры той раскладки, что ждёт Windows.
    let done = unsafe {
        let done = GetProcessTimes(
            GetCurrentProcess(),
            &mut creation,
            &mut exit,
            &mut kernel,
            &mut user,
        );
        GetSystemTimePreciseAsFileTime(&mut now);
        done
    };
    if done == 0 {
        return None;
    }
    // FILETIME — сотни наносекунд.
    let ticks = |time: FileTime| (u64::from(time.high) << 32) | u64::from(time.low);
    let elapsed = ticks(now).checked_sub(ticks(creation))?;
    Some(Duration::from_nanos(elapsed.saturating_mul(100)))
}

#[cfg(target_os = "linux")]
fn since_creation() -> Option<Duration> {
    let stat = std::fs::read_to_string("/proc/self/stat").ok()?;
    // Имя программы стоит в скобках и может содержать пробелы: поля
    // считаем после последней закрывающей. Первое после неё — третье поле,
    // время старта — двадцать второе.
    let rest = stat.get(stat.rfind(')')? + 2..)?;
    let started: u64 = rest.split(' ').nth(19)?.parse().ok()?;
    let uptime: f64 = std::fs::read_to_string("/proc/uptime")
        .ok()?
        .split(' ')
        .next()?
        .parse()
        .ok()?;
    // Время старта — в тиках USER_HZ, а он у Linux часть ABI: 100 везде.
    let started = started as f64 / 100.0;
    Some(Duration::from_secs_f64((uptime - started).max(0.0)))
}

#[cfg(not(any(windows, target_os = "linux")))]
fn since_creation() -> Option<Duration> {
    None
}
