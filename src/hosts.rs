//! Правила под хост — одной небольшой таблицей.
//!
//! Решено роадмапом («Per-host rules exist, in one small table»): для
//! открытого веба правила пишутся по форме, а треды и хосты документации
//! получают именованные правила, у каждого — страницы в проверке. Системы
//! плагинов вокруг нет и не будет: строка таблицы — функция здесь.
//!
//! Сейчас в таблице reddit. Его страницы без JavaScript пусты (8 КБ
//! заглушки), `old.reddit.com` уводит на вход, `.json` отвечает 403 — а лента
//! `.rss` у сабреддита и у треда открыта любому клиенту, с автором и полным
//! текстом каждой реплики. Цена — вложенности ответов в ленте нет, и жёсткий
//! лимит: второй запрос за несколько секунд получает 429.

use url::Url;

/// Хосты reddit, чьи страницы читаются через ленту.
const REDDIT: &[&str] = &[
    "reddit.com",
    "www.reddit.com",
    "old.reddit.com",
    "new.reddit.com",
    "np.reddit.com",
];

/// Адрес, которым страницу читать вместо неё самой, — если хост в таблице.
/// Адрес во вкладке и в истории остаётся тем, что открывали.
pub fn feed_for(url: &str) -> Option<String> {
    let parsed = Url::parse(url).ok()?;
    let host = parsed.host_str()?.to_ascii_lowercase();
    if !REDDIT.contains(&host.as_str()) {
        return None;
    }
    let path = parsed.path().trim_end_matches('/');
    let parts: Vec<&str> = path.split('/').filter(|part| !part.is_empty()).collect();
    // Сабреддит (`/r/rust`) и тред в нём (`/r/rust/comments/<id>/…`).
    // Лента уже лентой — `/r/rust/.rss` — идёт своим путём.
    let wanted = matches!(parts.as_slice(), ["r", _] | ["r", _, "comments", _, ..]);
    (wanted && !path.ends_with(".rss")).then(|| format!("https://www.reddit.com{path}/.rss"))
}

/// Тело реплики без обвязки хоста. reddit кладёт текст автора в `div.md`,
/// а за ним — «submitted by /u/… [link] [comments]»: это подпись, которую
/// тред и так показывает строкой над репликой. У поста-ссылки текста нет
/// вовсе, есть миниатюра и та же подпись; тогда остаётся сама ссылка.
pub fn reply_html(html: &str) -> String {
    if !html.contains("class=\"md\"") && !html.contains("submitted by") {
        return html.to_owned();
    }
    let doc = dom_query::Document::fragment(html);
    let text = doc.select("div.md");
    if text.exists() {
        return text.first().inner_html().to_string();
    }
    for node in doc.select("a[href]").nodes() {
        if node.text().trim() == "[link]"
            && let Some(href) = node.attr("href")
        {
            return format!("<p><a href=\"{href}\">{href}</a></p>");
        }
    }
    html.to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reddit_pages_are_read_through_their_feeds() {
        assert_eq!(
            feed_for("https://www.reddit.com/r/rust/comments/1wkmzun/no_more_code_dumps/"),
            Some(
                "https://www.reddit.com/r/rust/comments/1wkmzun/no_more_code_dumps/.rss".to_owned()
            )
        );
        assert_eq!(
            feed_for("https://old.reddit.com/r/rust/?sort=new"),
            Some("https://www.reddit.com/r/rust/.rss".to_owned())
        );
        // Уже лента, чужой хост, страница не треда — мимо.
        assert_eq!(feed_for("https://www.reddit.com/r/rust/.rss"), None);
        assert_eq!(feed_for("https://example.org/r/rust/"), None);
        assert_eq!(feed_for("https://www.reddit.com/user/someone/"), None);
    }

    #[test]
    fn a_reddit_reply_loses_its_signature() {
        let post = "<!-- SC_OFF --><div class=\"md\"><p>Text <a href=\"/r/rust\">r/rust</a>.</p></div>\
            <!-- SC_ON --> &#32; submitted by &#32; <a href=\"https://www.reddit.com/user/x\"> /u/x </a> \
            <br/> <span><a href=\"https://t.co/\">[link]</a></span>";
        assert_eq!(
            reply_html(post),
            "<p>Text <a href=\"/r/rust\">r/rust</a>.</p>"
        );

        let link = "<table><tr><td><a href=\"https://r.it/t\"><img src=\"thumb.jpg\"></a></td>\
            <td> submitted by <a href=\"/u/x\">/u/x</a> <span><a href=\"https://blog.example/post\">[link]</a></span>\
            <span><a href=\"https://r.it/t\">[comments]</a></span></td></tr></table>";
        assert_eq!(
            reply_html(link),
            "<p><a href=\"https://blog.example/post\">https://blog.example/post</a></p>"
        );
        assert_eq!(reply_html("<p>plain</p>"), "<p>plain</p>");
    }
}
