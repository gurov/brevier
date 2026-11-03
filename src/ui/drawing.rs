//! Окно Windows, умершее, не нарисовав первого кадра, следующий запуск
//! рисует без видеокарты (#61).
//!
//! GTK выбирает, чем рисовать, при первом окне и на старых драйверах там же
//! и падает, в своём C-коде: `gl_supported_platform` зовёт `strstr` от
//! `glGetString(GL_RENDERER)`, а драйвер вернул NULL. Изнутри такое падение
//! не поймать, а заранее не узнать: драйвер отвечает только из живого
//! контекста GL. Поэтому по следу. Главный экземпляр, начиная, кладёт
//! пометку, первый кадр её снимает. Пометка пережила процесс — прошлый
//! запуск умер до первого кадра, и этот рисует через cairo
//! (`GSK_RENDERER=cairo` — обход, который у читателя и сработал). Дошёл
//! с cairo до кадра — решение остаётся (`drawing.txt`): драйвер сам
//! не починится, а падать через раз хуже, чем рисовать без видеокарты.
//!
//! Свой `GSK_RENDERER` — решение читателя: тогда не трогаем ничего.
//! Ложная тревога — процесс убили, пока окно вставало, — стоит рисования
//! без видеокарты; вернуть её — удалить `drawing.txt` или `GSK_RENDERER=gl`.
//! Только Windows: на Linux GL выбирает рантайм, и таких падений там
//! не видели.

use std::path::PathBuf;
use std::sync::OnceLock;

/// Пометка «окно встаёт» — рядом с сессией, как пометка страницы (#57).
const STARTING: &str = "starting.txt";
const STARTING_TEXT: &str =
    "# Brevier's window is starting; the file goes away when it is drawn.\n";
/// Решение рисовать без видеокарты.
const KEPT: &str = "drawing.txt";
const KEPT_TEXT: &str = "# Brevier's window closed before it was drawn, so Brevier draws it \
without the graphics card (cairo). Delete this file to try the graphics card again.\n";

/// Чем рисует этот запуск.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Choice {
    /// Не наше дело: не Windows или `GSK_RENDERER` задал читатель.
    Untouched,
    /// Как решит GTK — видеокартой, если сможет.
    Gpu,
    /// Через cairo; `new` — решено только что, по пометке прошлого запуска.
    Cairo { new: bool },
}

static CHOICE: OnceLock<Choice> = OnceLock::new();
/// Первый кадр уже был: снимать пометку и говорить о cairo — один раз.
static DRAWN: OnceLock<()> = OnceLock::new();

/// Решение по трём фактам: задал ли читатель своё, решено ли раньше
/// и осталась ли пометка недорисованного запуска.
fn decide(users_own: bool, kept: bool, unfinished: bool) -> Choice {
    if users_own {
        Choice::Untouched
    } else if kept {
        Choice::Cairo { new: false }
    } else if unfinished {
        Choice::Cairo { new: true }
    } else {
        Choice::Gpu
    }
}

fn file(name: &str) -> Option<PathBuf> {
    brevier::store::data_dir().map(|dir| dir.join(name))
}

/// Решить в начале `main`, пока поток один: окружение меняем только так.
///
/// # Safety
///
/// Других потоков в процессе быть не должно: ставится `GSK_RENDERER`.
pub unsafe fn choose() {
    let choice = if cfg!(windows) {
        let users_own = std::env::var_os("GSK_RENDERER").is_some_and(|value| !value.is_empty());
        let exists = |name| file(name).is_some_and(|path| path.exists());
        decide(users_own, exists(KEPT), exists(STARTING))
    } else {
        Choice::Untouched
    };
    if let Choice::Cairo { .. } = choice {
        // Безопасно по договору функции: поток один.
        unsafe { std::env::set_var("GSK_RENDERER", "cairo") };
    }
    let _ = CHOICE.set(choice);
}

fn choice() -> Choice {
    CHOICE.get().copied().unwrap_or(Choice::Untouched)
}

/// Главный экземпляр начинает: пометка до первого кадра. Второй сюда
/// не доходит — он отдаёт адрес и выходит, так и не рисуя.
pub fn starting() {
    if choice() == Choice::Untouched {
        return;
    }
    if let Some(path) = file(STARTING) {
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(path, STARTING_TEXT);
    }
}

/// Первый кадр нарисован: пометку долой. Если cairo решён только что —
/// запомнить и вернуть, что сказать читателю.
pub fn drawn() -> Option<&'static str> {
    if DRAWN.set(()).is_err() {
        return None;
    }
    let choice = choice();
    if choice == Choice::Untouched {
        return None;
    }
    if let Some(path) = file(STARTING) {
        let _ = std::fs::remove_file(path);
    }
    if choice != (Choice::Cairo { new: true }) {
        return None;
    }
    if let Some(path) = file(KEPT) {
        let _ = std::fs::write(path, KEPT_TEXT);
    }
    Some(
        "Brevier closed before its window appeared last time, so it now draws without \
         the graphics card.",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_readers_own_renderer_wins() {
        assert_eq!(decide(true, true, true), Choice::Untouched);
    }

    #[test]
    fn a_start_that_never_drew_moves_to_cairo_and_stays() {
        assert_eq!(decide(false, false, false), Choice::Gpu);
        assert_eq!(decide(false, false, true), Choice::Cairo { new: true });
        assert_eq!(decide(false, true, false), Choice::Cairo { new: false });
        assert_eq!(decide(false, true, true), Choice::Cairo { new: false });
    }
}
