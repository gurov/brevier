//! Mastodon (#37): публичный статус — тредом.
//!
//! Страница статуса без JavaScript пуста: в ней приложение и `og:description`
//! с текстом записи, ответов нет вовсе. Сам тред тот же сервер отдаёт без
//! входа: `/api/v1/statuses/<id>` — запись, `…/context` — что было до неё
//! и ответы. Два запроса на тред, те же, что сделал бы скрипт страницы.
//!
//! Узнаём по форме, а не по хосту — серверов тысячи: адрес `/@имя/<цифры>`
//! и в странице приложение Mastodon (`<script id="initial-state">`) с ссылкой
//! на ActivityPub-двойника. Так выглядят сам Mastodon и его форки (glitch-soc,
//! Hometown). У Akkoma и Pleroma адрес и страница другие — их правило
//! не берёт, пока не попросят.
//!
//! Вид — общий для тредов (`thread.rs`): запись, под ней ответы с автором
//! и днём, ответ на ответ — цитатой. Два отличия от других площадок:
//!
//! - **Продолжение автора — часть записи.** Длинное в Mastodon пишут цепочкой
//!   ответов самому себе («1/7»); в виде треда седьмая часть ушла бы на седьмой
//!   уровень. Ответы автора себе по цепочке от записи встают абзацами под ней,
//!   а ответы на любую часть — обычными ответами.
//! - **Запись в ответ на другую** показывает сначала себя, потом раздел
//!   «In reply to» с тем, что было до неё: читатель открыл её, а не начало
//!   разговора.
//!
//! Без входа сервер отдаёт не больше [`SHOWN_TO_GUESTS`] ответов — упёрлись
//! в предел, говорим об этом строкой. API не ответил (сервер закрыл его
//! от гостей) — остаётся запись из `og:description` и строка о том, что
//! ответов не дали.
//!
//! Вложения: картинка — картинкой с описанием автора, остальное и то, что
//! автор пометил как деликатное, — ссылкой. Предупреждение о содержании —
//! курсивом над текстом, варианты опроса — списком.

use std::collections::{HashMap, HashSet};

use dom_query::Document;
use url::Url;

use super::thread::{Page, Reply, absolute, count_heading, squeeze, write_replies};
use crate::feed::{block, date_text, destination, heading, inline, reply_markdown};
use crate::fetch::UserAgent;
use crate::json::{self, Value};

/// Сколько ответов сервер отдаёт читателю без входа (предел самого Mastodon).
const SHOWN_TO_GUESTS: usize = 60;

/// Сколько знаков записи идёт в название страницы.
const TITLE_CHARS: usize = 60;

/// Статус по адресу страницы: сервер и номер записи на нём.
#[derive(Debug, PartialEq, Eq)]
struct Status {
    origin: String,
    id: String,
}

impl Status {
    fn of(url: &str) -> Option<Status> {
        let parsed = Url::parse(url).ok()?;
        if !matches!(parsed.scheme(), "http" | "https") {
            return None;
        }
        let parts: Vec<&str> = parsed.path_segments()?.filter(|p| !p.is_empty()).collect();
        let [name, id] = parts.as_slice() else {
            return None;
        };
        let numbered = !id.is_empty() && id.len() <= 32 && id.bytes().all(|b| b.is_ascii_digit());
        (name.len() > 1 && name.starts_with('@') && numbered).then(|| Status {
            origin: parsed.origin().ascii_serialization(),
            id: (*id).to_owned(),
        })
    }

    fn source(&self, tail: &str) -> String {
        format!("{}/api/v1/statuses/{}{tail}", self.origin, self.id)
    }
}

/// Страница статуса Mastodon — дёшево, по адресу и сырому HTML, до разбора.
/// Старые версии пишут атрибуты в одинарных кавычках.
pub fn is_status(url: &str, html: &str) -> bool {
    Status::of(url).is_some()
        && (html.contains("id=\"initial-state\"") || html.contains("id='initial-state'"))
        && html.contains("application/activity+json")
}

/// Тред статуса и язык записи — или `None`, если страница не статус Mastodon.
/// Ходит в сеть, поэтому зовётся из тракта сети, как комментарии Хабра.
pub fn thread(url: &str, html: &str, ua: UserAgent) -> Option<(Page, Option<String>)> {
    if !is_status(url, html) {
        return None;
    }
    let status = Status::of(url)?;
    let ask = |tail: &str| {
        crate::fetch::binary(
            &status.source(tail),
            ua,
            "application/json",
            crate::fetch::MAX_BODY,
        )
        .ok()
        .and_then(|blob| json::parse(&String::from_utf8_lossy(&blob.bytes)))
    };
    match ask("").filter(|post| post.text("id").is_some()) {
        Some(post) => {
            let lang = post.text("language").map(str::to_owned);
            Some((page(&post, ask("/context").as_ref(), url), lang))
        }
        None => described(html).map(|page| (page, None)),
    }
}

/// Тред из ответов API: запись с продолжением автора, что было до неё, ответы.
fn page(post: &Value, context: Option<&Value>, url: &str) -> Page {
    let list = |name: &str| {
        context
            .and_then(|context| context.get(name))
            .map(Value::as_array)
            .unwrap_or_default()
    };
    let (before, after) = (list("ancestors"), list("descendants"));
    let name = author(post).unwrap_or_else(|| "Mastodon".to_owned());

    let mut children: HashMap<&str, Vec<&Value>> = HashMap::new();
    for reply in after {
        if let Some(parent) = reply.text("in_reply_to_id") {
            children.entry(parent).or_default().push(reply);
        }
    }

    // Цепочка автора: запись и его ответы самому себе, один за другим.
    let me = account_id(post);
    let mut seen: HashSet<&str> = post.text("id").into_iter().collect();
    let mut parts = vec![post];
    while let Some(next) = parts
        .last()
        .and_then(|last| last.text("id"))
        .and_then(|id| children.get(id))
        .and_then(|kids| kids.iter().find(|kid| account_id(kid) == me))
        .filter(|_| me.is_some())
        && let Some(id) = next.text("id")
        && seen.insert(id)
    {
        parts.push(next);
    }

    let mut out = format!("# {}\n\n", heading(&name));
    let meta: Vec<String> = [handle(post, url), post.text("created_at").map(date_text)]
        .into_iter()
        .flatten()
        .map(|text| inline(&text))
        .collect();
    if !meta.is_empty() {
        out.push_str(&format!("*{}*\n\n", meta.join(" · ")));
    }
    for part in &parts {
        if let Some(text) = body(part, url) {
            out.push_str(&text);
            out.push_str("\n\n");
        }
    }

    if !before.is_empty() {
        let earlier: Vec<Reply> = before.iter().map(|status| reply(status, 0, url)).collect();
        out.push_str("## In reply to\n\n");
        write_replies(&earlier, &mut out);
        out.push('\n');
    }

    // Ответы — в глубину, стеком: цепочку любой длины пишет не наш код.
    // Сначала ответы на части записи, потом те, чей родитель до нас не дошёл.
    let mut replies = Vec::new();
    let roots = parts.iter().filter_map(|part| part.text("id"));
    let mut stack: Vec<(&Value, usize)> = Vec::new();
    for root in roots.collect::<Vec<_>>() {
        for kid in children.get(root).into_iter().flatten().rev() {
            stack.push((kid, 0));
        }
        walk(&mut stack, &children, &mut seen, &mut replies, url);
    }
    for orphan in after {
        stack.push((orphan, 0));
        walk(&mut stack, &children, &mut seen, &mut replies, url);
    }

    out.push_str(&count_heading(replies.len(), "reply"));
    write_replies(&replies, &mut out);
    if after.len() >= SHOWN_TO_GUESTS {
        out.push_str(&format!(
            "\n*The server gives a reader who is not signed in {SHOWN_TO_GUESTS} replies \
             at most: there may be more.*\n"
        ));
    }
    Page {
        title: titled(&name, post),
        markdown: out,
        thread: true,
        site: Some(Vec::new()),
    }
}

fn walk<'a>(
    stack: &mut Vec<(&'a Value, usize)>,
    children: &HashMap<&str, Vec<&'a Value>>,
    seen: &mut HashSet<&'a str>,
    out: &mut Vec<Reply>,
    page: &str,
) {
    while let Some((status, depth)) = stack.pop() {
        let Some(id) = status.text("id") else {
            continue;
        };
        if !seen.insert(id) {
            continue;
        }
        out.push(reply(status, depth, page));
        for kid in children.get(id).into_iter().flatten().rev() {
            stack.push((kid, depth + 1));
        }
    }
}

fn reply(status: &Value, depth: usize, page: &str) -> Reply {
    let base = Url::parse(page).ok();
    Reply {
        depth,
        author: author(status),
        date: status.text("created_at").map(date_text),
        // Ответ с другого сервера ведёт на свой сервер: там его тред.
        link: status
            .text("url")
            .or_else(|| status.text("uri"))
            .and_then(|link| absolute(base.as_ref()?, link)),
        on: None,
        body: body(status, page),
        anchor: None,
    }
}

fn account_id(status: &Value) -> Option<&str> {
    status.get("account")?.text("id")
}

/// Имя автора, как он назвал себя сам; не назвал — имя учётной записи.
fn author(status: &Value) -> Option<String> {
    let account = status.get("account")?;
    account
        .text("display_name")
        .or_else(|| account.text("acct"))
        .map(squeeze)
}

/// Кто автор в федерации: «@имя on сервер». Адрес целиком (`@имя@сервер`)
/// markdown принял бы за почту и сделал ссылкой `mailto:`. У своих сервер
/// в `acct` не назван — он тот же, что у страницы.
fn handle(status: &Value, page: &str) -> Option<String> {
    let acct = status.get("account")?.text("acct")?;
    Some(match acct.split_once('@') {
        Some((name, host)) => format!("@{name} on {host}"),
        None => format!("@{acct} on {}", Url::parse(page).ok()?.host_str()?),
    })
}

/// Название страницы: автор и начало записи.
fn titled(name: &str, post: &Value) -> String {
    let text = post
        .text("content")
        .filter(|html| !crate::nesting::html_too_deep(html))
        .map(|html| {
            let doc = Document::fragment(html);
            // «RE: адрес» цитируемой записи — не начало текста.
            doc.select("p.quote-inline").remove();
            squeeze(&doc.root().text())
        })
        .unwrap_or_default();
    if text.is_empty() {
        return name.to_owned();
    }
    let mut start: String = text.chars().take(TITLE_CHARS).collect();
    if start.len() < text.len() {
        start = format!("{}…", start.trim_end());
    }
    format!("{name}: {start}")
}

/// Всё, что автор положил в запись: предупреждение, текст, опрос, вложения.
fn body(status: &Value, page: &str) -> Option<String> {
    let mut parts = Vec::new();
    if let Some(warning) = status.text("spoiler_text") {
        parts.push(format!("*{}*", inline(&squeeze(warning))));
    }
    if let Some(text) = status.text("content").and_then(|html| said(html, page)) {
        parts.push(text);
    }
    let options: Vec<String> = status
        .get("poll")
        .and_then(|poll| poll.get("options"))
        .map(Value::as_array)
        .unwrap_or_default()
        .iter()
        .filter_map(|option| option.text("title"))
        .map(|title| format!("- {}", inline(&squeeze(title))))
        .collect();
    if !options.is_empty() {
        parts.push(options.join("\n"));
    }
    let base = Url::parse(page).ok();
    let sensitive = status.get("sensitive") == Some(&Value::Bool(true));
    let attached = status.get("media_attachments").map(Value::as_array);
    for item in attached.unwrap_or_default() {
        let address = item
            .text("url")
            .or_else(|| item.text("remote_url"))
            .and_then(|link| absolute(base.as_ref()?, link));
        let Some(address) = address else {
            continue;
        };
        let told = item.text("description").map(squeeze).unwrap_or_default();
        let label = match (item.text("type"), sensitive) {
            (Some("image"), false) => {
                parts.push(format!("![{}]({})", inline(&told), destination(&address)));
                continue;
            }
            (Some("image"), true) => "Image, marked sensitive",
            (Some("video" | "gifv"), _) => "Video",
            (Some("audio"), _) => "Audio",
            _ => "Attachment",
        };
        let text = if told.is_empty() {
            label.to_owned()
        } else {
            format!("{label}: {told}")
        };
        parts.push(format!("[{}]({})", inline(&text), destination(&address)));
    }
    (!parts.is_empty()).then(|| parts.join("\n\n"))
}

/// Текст записи. Адрес в тексте Mastodon режет на три `span`: схема и хвост
/// спрятаны стилем (`invisible`), середина с многоточием из CSS (`ellipsis`).
/// Без стилей вышел бы адрес целиком — показываем как сайт: середину
/// и многоточие. Слишком глубокая разметка — текстом, как у Хабра.
fn said(html: &str, page: &str) -> Option<String> {
    if crate::nesting::html_too_deep(html) {
        let text = squeeze(&Document::fragment(html).root().text());
        return (!text.is_empty()).then(|| block(&text));
    }
    if !html.contains("invisible") {
        return reply_markdown(html, page);
    }
    let doc = Document::fragment(html);
    doc.select("a span.invisible").remove();
    for span in doc.select("a span.ellipsis").nodes() {
        span.set_text(format!("{}…", span.text()));
    }
    reply_markdown(&doc.html(), page)
}

/// Запись без API: текст из `og:description`, автор из `og:title`.
fn described(html: &str) -> Option<Page> {
    if crate::nesting::html_too_deep(html) {
        return None;
    }
    let doc = Document::from(html);
    let meta = |name: &str| {
        doc.select(&format!("meta[property=\"og:{name}\"]"))
            .nodes()
            .first()
            .and_then(|node| node.attr("content"))
            .map(|text| text.trim().to_owned())
            .filter(|text| !text.is_empty())
    };
    let text = meta("description")?;
    let name = meta("title").unwrap_or_else(|| "Mastodon".to_owned());
    let mut out = format!("# {}\n\n", heading(&squeeze(&name)));
    for line in text.lines().map(squeeze).filter(|line| !line.is_empty()) {
        out.push_str(&block(&line));
        out.push_str("\n\n");
    }
    out.push_str("*The server did not give Brevier the replies to this post.*\n");
    Some(Page {
        title: squeeze(&name),
        markdown: out,
        thread: true,
        site: Some(Vec::new()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAGE: &str = "https://example.social/@ann/111";

    fn status(id: &str, parent: Option<&str>, who: &str, html: &str) -> String {
        let parent = parent.map_or("null".to_owned(), |id| format!("\"{id}\""));
        format!(
            r#"{{"id":"{id}","in_reply_to_id":{parent},"created_at":"2026-11-10T16:27:27.257Z",
            "language":"en","url":"https://example.social/@{who}/{id}","spoiler_text":"",
            "sensitive":false,"content":"{html}","media_attachments":[],"poll":null,
            "account":{{"id":"acc-{who}","acct":"{who}","display_name":"{who} Name"}}}}"#
        )
    }

    fn built(post: &str, before: &[String], after: &[String]) -> Page {
        let context = format!(
            r#"{{"ancestors":[{}],"descendants":[{}]}}"#,
            before.join(","),
            after.join(",")
        );
        page(
            &json::parse(post).expect("запись"),
            json::parse(&context).as_ref(),
            PAGE,
        )
    }

    #[test]
    fn a_status_is_known_by_its_address_and_its_page() {
        let html = r#"<link href="x" rel="alternate" type="application/activity+json">
            <script id="initial-state" type="application/json">{}</script>"#;
        assert!(is_status(PAGE, html));
        assert!(is_status(
            "https://example.social/@bob@other.example/42",
            &html.replace('"', "'")
        ));
        // Профиль, чужой путь, не Mastodon.
        assert!(!is_status("https://example.social/@ann", html));
        assert!(!is_status("https://example.social/@ann/111/embed", html));
        assert!(!is_status("https://example.social/tags/111", html));
        assert!(!is_status(PAGE, "<html><body><p>текст</p></body></html>"));
    }

    #[test]
    fn the_api_is_asked_on_the_same_server() {
        let status = Status::of("https://example.social:8443/@ann/111?x=1#y").unwrap();
        assert_eq!(
            status.source("/context"),
            "https://example.social:8443/api/v1/statuses/111/context"
        );
    }

    #[test]
    fn the_authors_own_chain_continues_the_post_and_replies_nest() {
        let post = status("111", None, "ann", "<p>First part.</p>");
        let after = [
            status("112", Some("111"), "ann", "<p>Second part.</p>"),
            status("113", Some("111"), "bob", "<p>Nice.</p>"),
            status("114", Some("113"), "ann", "<p>Thanks.</p>"),
            status("115", Some("112"), "eve", "<p>To the second.</p>"),
            status("116", Some("999"), "dan", "<p>Parent is gone.</p>"),
        ];
        let page = built(&post, &[], &after);
        assert_eq!(page.title, "ann Name: First part.");
        assert_eq!(
            page.markdown,
            "# ann Name\n\n\
             *@ann on example.social · 10 November 2026*\n\n\
             First part.\n\n\
             Second part.\n\n\
             ## 4 replies\n\n\
             **bob Name** · [10 November 2026](https://example.social/@bob/113)\n\n\
             Nice.\n\n\
             > **ann Name** · [10 November 2026](https://example.social/@ann/114)\n>\n\
             > Thanks.\n\n\
             **eve Name** · [10 November 2026](https://example.social/@eve/115)\n\n\
             To the second.\n\n\
             **dan Name** · [10 November 2026](https://example.social/@dan/116)\n\n\
             Parent is gone.\n"
        );
    }

    #[test]
    fn a_reply_shows_itself_first_and_then_what_it_answers() {
        let post = status("111", Some("100"), "ann", "<p>My answer.</p>");
        let before = [status("100", None, "bob", "<p>The question.</p>")];
        let page = built(&post, &before, &[]);
        let answer = page.markdown.find("My answer.").unwrap();
        let section = page.markdown.find("## In reply to").unwrap();
        let question = page.markdown.find("The question.").unwrap();
        assert!(answer < section && section < question);
        assert!(page.markdown.ends_with("## No replies yet\n"));
    }

    #[test]
    fn a_link_reads_as_the_site_shows_it_and_attachments_follow() {
        let html = r#"<p>See <a href=\"https://example.org/a/very/long/path\"><span class=\"invisible\">https://</span><span class=\"ellipsis\">example.org/a/very</span><span class=\"invisible\">/long/path</span></a></p>"#;
        let post = status("111", None, "ann", html)
            .replace(r#""spoiler_text":"""#, r#""spoiler_text":"spiders""#)
            .replace(
                r#""media_attachments":[]"#,
                r#""media_attachments":[{"type":"image","url":"https://files.example/a.png","description":"A web [close]"},{"type":"video","url":"https://files.example/b.mp4","description":null}]"#,
            )
            .replace(
                r#""poll":null"#,
                r#""poll":{"options":[{"title":"Yes","votes_count":1},{"title":"No","votes_count":2}]}"#,
            );
        let text = body(&json::parse(&post).unwrap(), PAGE).unwrap();
        assert_eq!(
            text,
            "*spiders*\n\n\
             See [example.org/a/very…](https://example.org/a/very/long/path)\n\n\
             - Yes\n- No\n\n\
             ![A web \\[close\\]](https://files.example/a.png)\n\n\
             [Video](https://files.example/b.mp4)"
        );
        // Деликатное автор спрятал — и мы не показываем без спроса.
        let hidden = post.replace(r#""sensitive":false"#, r#""sensitive":true"#);
        let text = body(&json::parse(&hidden).unwrap(), PAGE).unwrap();
        assert!(
            text.contains(
                "[Image, marked sensitive: A web \\[close\\]](https://files.example/a.png)"
            )
        );
        assert!(!text.contains("!["));
    }

    #[test]
    fn a_script_address_is_never_a_link() {
        let post = status("111", None, "ann", "<p>Hi.</p>");
        let bad = status("112", Some("111"), "bob", "<p>Yo.</p>")
            .replace("https://example.social/@bob/112", "javascript:alert(1)");
        let page = built(&post, &[], &[bad]);
        assert!(!page.markdown.contains("javascript"));
    }

    #[test]
    fn the_guests_limit_is_said_aloud() {
        let post = status("111", None, "ann", "<p>Hi.</p>");
        let after: Vec<String> = (0..SHOWN_TO_GUESTS)
            .map(|n| status(&format!("2{n:02}"), Some("111"), "bob", "<p>Yo.</p>"))
            .collect();
        let page = built(&post, &[], &after);
        assert!(page.markdown.contains("## 60 replies"));
        assert!(page.markdown.contains("there may be more"));
    }

    #[test]
    fn without_the_api_the_post_comes_from_the_page() {
        let html = r##"<html><head><meta content="Ann (@ann@example.social)" property="og:title">
            <meta content="# First line.

Second line." property="og:description"></head><body></body></html>"##;
        let page = described(html).unwrap();
        assert_eq!(page.title, "Ann (@ann@example.social)");
        assert_eq!(
            page.markdown,
            "# Ann (@ann@example.social)\n\n\\# First line.\n\nSecond line.\n\n\
             *The server did not give Brevier the replies to this post.*\n"
        );
        assert!(described("<html><head></head><body></body></html>").is_none());
    }
}
