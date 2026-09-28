//! Начальная страница: что это за программа и для чего.
//!
//! Текст живёт в ядре, как и тексты ошибок: он один и тот же для любого
//! интерфейса, и перевод встанет ровно сюда. Написан markdown-ом,
//! и это не лень, а следствие решения о внутреннем представлении:
//! начальная страница идёт через тот же рендерер, что и статья, поэтому
//! показывает читателю ровно ту типографику, которую продукт обещает.
//!
//! Коротко по существу: читатель открыл окно, чтобы читать, а не изучать
//! программу. Вернувшемуся читателю нужнее всего то, где он был, — поэтому
//! сверху недавнее, а рассказ о программе под ним (#27).

use crate::store::Store;

pub const TITLE: &str = "Brevier";

/// Начальная страница целиком: недавнее из журнала, если оно есть, и под
/// ним рассказ о программе. `touch` — вариант для сенсорного экрана.
/// Собирается на каждый показ: журнал растёт, пока окно открыто.
pub fn page(store: &Store, touch: bool) -> String {
    let about = if touch { TOUCH } else { MARKDOWN };
    let recent = store.recent_page();
    if recent.is_empty() {
        about.to_owned()
    } else {
        format!("{recent}\n{about}")
    }
}

pub const MARKDOWN: &str = "\
# Brevier

A reader for the web, without JavaScript.

Brevier fetches a page, throws away the site's scripts, styling and furniture,
and sets what is left in typography chosen by you rather than by the site.
It reads markdown documentation straight out of repositories, too.

## Start with

- [danluu.com/keyboard-latency/](https://danluu.com/keyboard-latency/) — an article
- [gh:BurntSushi/ripgrep](gh:BurntSushi/ripgrep) — a repository
- the path to a `.md` file on this machine

## Worth knowing

Pages that need JavaScript will not render here. That is the point, not a
defect — when it happens, **Ctrl+O** hands the address to your usual browser.

**Ctrl+L** address · **Ctrl+T** new tab · **Ctrl+H** history · **Ctrl+F** find
· **Ctrl+S** save
";

/// Та же страница для сенсорного экрана. Клавиш на телефоне нет, а путь
/// к файлу там не напечатаешь: файлы приложения другим не видны. Зато есть
/// «поделиться» — им ссылка и попадает в Brevier из любого приложения.
pub const TOUCH: &str = "\
# Brevier

A reader for the web, without JavaScript.

Brevier fetches a page, throws away the site's scripts, styling and furniture,
and sets what is left in typography chosen by you rather than by the site.
It reads markdown documentation straight out of repositories, too.

## Start with

- [danluu.com/keyboard-latency/](https://danluu.com/keyboard-latency/) — an article
- [gh:BurntSushi/ripgrep](gh:BurntSushi/ripgrep) — a repository
- a link shared to Brevier from any other app

## Worth knowing

Pages that need JavaScript will not render here. That is the point, not a
defect — when it happens, **Open in your browser** in the menu hands the
address to your usual browser.

The list button at the top is the contents; find on page, zoom, history and
bookmarks are in the menu. Pinch the page to change its size; long-press
a link to open it in a new tab.
";

#[cfg(test)]
mod tests {
    use super::*;
    use crate::address::Address;

    fn journal(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "brevier-intro-{}-{name}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        dir.join("history.tsv")
    }

    #[test]
    fn a_first_run_gets_the_introduction_alone() {
        let store = Store::at(journal("first"));
        assert_eq!(page(&store, false), MARKDOWN);
        assert_eq!(page(&store, true), TOUCH);
    }

    #[test]
    fn recent_pages_stand_above_the_introduction() {
        let mut store = Store::at(journal("back"));
        store.record(&Address::Web("https://sive.rs/".to_owned()), "sivers", 0);
        let text = page(&store, true);
        assert!(text.starts_with("### Recently read\n\n- [sivers](https://sive.rs/)\n"));
        assert!(text.find("(brevier:history)").unwrap() < text.find("# Brevier").unwrap());
        assert!(text.ends_with(TOUCH));
    }
}
