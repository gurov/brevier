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
//!
//! Рассказ — о том, что можно сделать отсюда, а не о замысле: что набрать
//! в адресной строке (примеры кликаются) и пять вещей, которых не видно,
//! пока не наткнёшься. Язык простой, короткими фразами: читают это
//! и те, для кого английский не родной (переписано 9 октября 2026).

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

/// В строке клавиш клавиша и её подпись держатся неразрывным пробелом:
/// иначе строка рвётся между «**Ctrl+H**» и «history».
pub const MARKDOWN: &str = "\
# Brevier

Brevier shows web pages as plain, easy-to-read text. It leaves out scripts
and the site's own design, so every site looks the same.

## Type in the address bar

- an address: [danluu.com](https://danluu.com/)
- words, to search the web: [borrow checker](https://lite.duckduckgo.com/lite/?q=borrow+checker)
- a GitHub or GitLab repository, to read its docs: [gh:rust-lang/book](gh:rust-lang/book)
- a site's feed, to see its latest posts: [blog.rust-lang.org/feed.xml](https://blog.rust-lang.org/feed.xml)
- the path to a Markdown file on this computer

## Good to know

- Some sites need JavaScript and show nothing here. **Ctrl+O** opens the page
  in your usual browser.
- On most pages, a panel on the right lists the parts of the page, the site's
  feeds and the site's own menu.
- When a page names the next one, it ends with a **Next:** link. **Space** at
  the very end goes there.
- **Back** returns you to the place where you stopped reading.
- Pages you read this week open from a copy on this computer. **Ctrl+R** loads
  a fresh one.

**Ctrl+L**\u{a0}address · **Ctrl+T**\u{a0}new\u{a0}tab · **Ctrl+F**\u{a0}find
· **Ctrl+D**\u{a0}bookmark · **Ctrl+H**\u{a0}history · **Ctrl+S**\u{a0}save
· **Tab**\u{a0}next\u{a0}link
";

/// Та же страница для сенсорного экрана. Клавиш на телефоне нет, а путь
/// к файлу там не напечатаешь: файлы приложения другим не видны. Зато есть
/// «поделиться» — им ссылка и попадает в Brevier из любого приложения.
pub const TOUCH: &str = "\
# Brevier

Brevier shows web pages as plain, easy-to-read text. It leaves out scripts
and the site's own design, so every site looks the same.

## Type in the address bar

- an address: [danluu.com](https://danluu.com/)
- words, to search the web: [borrow checker](https://lite.duckduckgo.com/lite/?q=borrow+checker)
- a GitHub or GitLab repository, to read its docs: [gh:rust-lang/book](gh:rust-lang/book)
- a site's feed, to see its latest posts: [blog.rust-lang.org/feed.xml](https://blog.rust-lang.org/feed.xml)

Links shared from other apps open here too.

## Good to know

- Some sites need JavaScript and show nothing here. **Open in your browser**
  in the menu sends the page to your usual browser.
- The list button at the top shows the parts of the page, the site's feeds
  and the site's own menu.
- When a page names the next one, it ends with a **Next:** link.
- Press and hold a link to open it in a new tab.
- Pages you read this week open from a copy on your phone. **Reload** in the
  menu loads a fresh one.

Find on page, bookmarks, history and settings are in the menu. Pinch the page
to make it bigger or smaller.
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

    /// Пример поиска — ссылка на ту же выдачу, что даёт адресная строка:
    /// поменяется поисковик — тест напомнит поменять и пример.
    #[test]
    fn the_search_example_is_a_real_search() {
        let link = format!("({})", crate::hosts::search_url("borrow checker"));
        assert!(MARKDOWN.contains(&link), "{link}");
        assert!(TOUCH.contains(&link), "{link}");
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
