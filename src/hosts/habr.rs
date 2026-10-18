//! Хабр (#40): комментарии под статьёй и страница комментариев — тредом.
//!
//! В странице статьи комментариев нет: сервер кладёт в неё счётчик и пустое
//! место `#publication-comments`, а дерево приносит скрипт — JSON-ом
//! из `kek/v2/articles/<id>/comments/` (у постов — `threads/<id>/…`).
//! Тот же запрос делаем и мы, один на статью и только когда счётчик
//! не ноль: на 331 комментарий это 435 КБ (88 КБ сжатыми), а страница
//! `/comments/`, где их рисует сервер, весит 2 МБ. Комментарии встают под
//! статьёй разделом «331 comments» в общем виде треда (`thread.rs`): автор,
//! день ссылкой на комментарий, текст, ответ — цитатой. Якорь комментария —
//! `comment_<id>`, как у самого Хабра: ссылка из его ленты комментариев
//! (`/articles/<id>/#comment_<n>`) приходит прямо к нему, а
//! `#publication-comments` из списков — к разделу. Запрос не удался —
//! статья остаётся статьёй, а под ней ссылка на страницу комментариев.
//!
//! Страница `/comments/` (на неё ведут списки и поиск) — тред без статьи
//! из того, что сервер уже положил: состояние приложения,
//! `window.__PINIA_STATE__`, то же дерево под другими именами (`refs`
//! и `root` вместо `comments` и `threads`). Второй запрос ей не нужен;
//! заголовок ведёт к статье. Сгенерированные классы вёрстки (`data-v-…`)
//! не трогаем вовсе.
//!
//! Оценок нет, как и у других тредов. Удалённое Хабр подписывает своей
//! фразой («НЛО прилетело…») — она и остаётся, курсивом, без автора.

use std::collections::{HashMap, HashSet};

use dom_query::Document;
use url::Url;

use super::thread::{Page, Reply, count_heading, squeeze, title_of, write_replies};
use crate::feed::{date_text, destination, heading, inline, reply_markdown};
use crate::fetch::UserAgent;
use crate::json::{self, Value};

const HOST: &str = "habr.com";

/// Где начинается состояние приложения в скрипте страницы.
const STATE: &str = "window.__PINIA_STATE__";

/// Якорь раздела — тот, на который Хабр ссылается сам.
const SECTION_ANCHOR: &str = "publication-comments";

/// Публикация по адресу: статья, новость (их и блоги компаний API зовёт
/// `articles`) или пост (`threads`).
#[derive(Debug, PartialEq, Eq)]
struct Publication {
    id: String,
    post: bool,
    /// Страница комментариев (`…/<id>/comments/`), а не сама публикация.
    comments: bool,
}

impl Publication {
    fn of(url: &str) -> Option<Publication> {
        let parsed = Url::parse(url).ok()?;
        let host = parsed.host_str()?.to_ascii_lowercase();
        if host.trim_start_matches("www.") != HOST {
            return None;
        }
        let mut parts: Vec<&str> = parsed.path_segments()?.filter(|s| !s.is_empty()).collect();
        // Язык впереди: `/ru/…`, `/en/…`.
        if parts
            .first()
            .is_some_and(|part| part.len() == 2 && part.bytes().all(|b| b.is_ascii_lowercase()))
        {
            parts.remove(0);
        }
        // Блог компании: `/companies/<имя>/articles/<id>/`.
        if parts.first() == Some(&"companies") && parts.len() > 3 {
            parts.drain(..2);
        }
        let comments = parts.last() == Some(&"comments");
        if comments {
            parts.pop();
        }
        let [kind, id] = parts.as_slice() else {
            return None;
        };
        if id.is_empty() || !id.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        let post = match *kind {
            "articles" | "news" => false,
            // У поста своей страницы комментариев нет: они под ним самим.
            "posts" if !comments => true,
            _ => return None,
        };
        Some(Publication {
            id: (*id).to_owned(),
            post,
            comments,
        })
    }

    fn source(&self) -> String {
        let kind = if self.post { "threads" } else { "articles" };
        format!("https://{HOST}/kek/v2/{kind}/{}/comments/", self.id)
    }
}

/// Страница комментариев Хабра — дёшево, по адресу, до разбора.
pub fn is_comments_page(url: &str) -> bool {
    Publication::of(url).is_some_and(|publication| publication.comments)
}

/// Раздел комментариев под статью Хабра, markdown-ом, — или `None`, если
/// это не статья Хабра или комментариев у неё нет. Ходит в сеть: тот же
/// запрос, что сделал бы скрипт страницы.
pub fn comments(url: &str, html: &str, ua: UserAgent) -> Option<String> {
    let publication = Publication::of(url).filter(|publication| !publication.comments)?;
    // Счётчика не нашли — спрашиваем API: пусть лучше скажет «ни одного»,
    // чем комментарии пропадут молча.
    let count = state_in(html).and_then(|state| count(&state, &publication.id));
    if count == Some(0) {
        return None;
    }
    let tree = crate::fetch::binary(
        &publication.source(),
        ua,
        "application/json",
        crate::fetch::MAX_BODY,
    )
    .ok()
    .and_then(|blob| json::parse(&String::from_utf8_lossy(&blob.bytes)));
    match tree {
        Some(tree) => section(&tree, url),
        // У поста страницы комментариев нет — и ссылаться не на что.
        None if !publication.post => count.map(|count| elsewhere(count, url)),
        None => None,
    }
}

/// Раздел из ответа API: якорь, «N comments», дерево.
fn section(tree: &Value, page: &str) -> Option<String> {
    let replies = replies(tree.get("comments")?, roots(tree.get("threads")), page);
    if replies.is_empty() {
        return None;
    }
    let mut out = format!("<a id=\"{SECTION_ANCHOR}\"></a>\n\n");
    out.push_str(&count_heading(replies.len(), "comment"));
    write_replies(&replies, &mut out);
    Some(out)
}

/// Комментарии есть, а не пришли: раздел со ссылкой на их страницу —
/// её Brevier прочтёт и без API (см. [`read`]).
fn elsewhere(count: usize, page: &str) -> String {
    let mut out = format!("<a id=\"{SECTION_ANCHOR}\"></a>\n\n");
    out.push_str(&count_heading(count, "comment"));
    let target = match Url::parse(page) {
        Ok(mut url) => {
            let path = format!("{}/comments/", url.path().trim_end_matches('/'));
            url.set_path(&path);
            url.set_query(None);
            url.set_fragment(None);
            url.to_string()
        }
        Err(_) => page.to_owned(),
    };
    out.push_str(&format!(
        "They did not load with the article: [open the comments page]({}).\n",
        destination(&target)
    ));
    out
}

/// Страница `/comments/` — тред: заголовок ссылкой на статью, автор и день
/// статьи, дерево. Состояния приложения в странице нет — `None`, и её
/// читает общий тракт.
pub fn read(doc: &Document, url: &str) -> Option<Page> {
    let publication = Publication::of(url).filter(|publication| publication.comments)?;
    let state = doc
        .select("script:not([src])")
        .nodes()
        .iter()
        .find_map(|script| state_in(&script.text()))?;
    let tree = state
        .get("comments")?
        .get("articleComments")?
        .get(&publication.id)?;
    let replies = replies(tree.get("refs")?, roots(tree.get("root")), url);
    let article = state
        .get("articlesList")
        .and_then(|list| list.get("articlesList"))
        .and_then(|list| list.get(&publication.id));

    let title = article
        .and_then(|article| article.text("titleHtml"))
        .map(html_text)
        .filter(|title| !title.is_empty())
        .or_else(|| title_of(&doc.root()))
        .unwrap_or_else(|| "Habr".to_owned());
    let mut out = match Url::parse(url).and_then(|url| url.join("..")) {
        Ok(article) => format!(
            "# [{}]({})\n\n",
            heading(&title),
            destination(article.as_str())
        ),
        Err(_) => format!("# {}\n\n", heading(&title)),
    };
    let meta: Vec<String> = [
        article
            .and_then(|article| article.get("author"))
            .and_then(|author| author.text("alias"))
            .map(str::to_owned),
        article
            .and_then(|article| article.text("timePublished"))
            .map(date_text),
    ]
    .into_iter()
    .flatten()
    .map(|text| inline(&text))
    .collect();
    if !meta.is_empty() {
        out.push_str(&format!("*{}*\n\n", meta.join(" · ")));
    }
    out.push_str(&count_heading(replies.len(), "comment"));
    write_replies(&replies, &mut out);
    Some(Page {
        title,
        markdown: out,
        thread: true,
        site: None,
    })
}

/// Состояние приложения из текста скрипта или всей страницы: то место,
/// где ему присваивают значение, а не где о нём спрашивают.
fn state_in(text: &str) -> Option<Value> {
    text.match_indices(STATE).find_map(|(at, _)| {
        let rest = text[at + STATE.len()..].trim_start().strip_prefix('=')?;
        json::parse_start(rest)
    })
}

/// Сколько комментариев у публикации, по её карточке в состоянии страницы.
fn count(state: &Value, id: &str) -> Option<usize> {
    let statistics = state
        .get("articlesList")?
        .get("articlesList")?
        .get(id)?
        .get("statistics")?;
    match statistics.get("commentsCount")? {
        Value::Number(number) => number.parse().ok(),
        _ => None,
    }
}

/// Номера комментариев верхнего уровня, по порядку.
fn roots(list: Option<&Value>) -> Vec<&str> {
    list.map(Value::as_array)
        .unwrap_or_default()
        .iter()
        .filter_map(id_of)
        .collect()
}

/// Номер — строкой, но и числом не ошибка.
fn id_of(value: &Value) -> Option<&str> {
    match value {
        Value::String(id) | Value::Number(id) => Some(id),
        _ => None,
    }
}

/// Дерево по порядку чтения: верхний уровень, под каждым — ответы
/// в глубину. Глубину считаем обходом, а не берём из `level`: так уровень
/// не разойдётся с тем, под кем реплика стоит. Обход — стеком, а не
/// рекурсией: цепочку ответов любой длины пишет не наш код. Чего от корней
/// не достать (корня нет в списке, родителя — в дереве), идёт следом
/// своими ветками.
fn replies(map: &Value, roots: Vec<&str>, page: &str) -> Vec<Reply> {
    let Value::Object(pairs) = map else {
        return Vec::new();
    };
    let by_id: HashMap<&str, &Value> = pairs
        .iter()
        .map(|(id, comment)| (id.as_str(), comment))
        .collect();
    let orphans = pairs.iter().filter_map(|(id, comment)| {
        let parent = comment.get("parentId").and_then(id_of);
        parent
            .is_none_or(|parent| !by_id.contains_key(parent))
            .then_some(id.as_str())
    });
    let base = Url::parse(page).ok();

    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for root in roots.into_iter().chain(orphans) {
        let mut stack = vec![(root, 0)];
        while let Some((id, depth)) = stack.pop() {
            let Some(comment) = by_id.get(id) else {
                continue;
            };
            if !seen.insert(id) {
                continue;
            }
            out.push(reply(id, comment, depth, base.as_ref(), page));
            let children = comment.get("children").map(Value::as_array);
            for child in children.unwrap_or_default().iter().rev() {
                if let Some(child) = id_of(child) {
                    stack.push((child, depth + 1));
                }
            }
        }
    }
    out
}

fn reply(id: &str, comment: &Value, depth: usize, base: Option<&Url>, page: &str) -> Reply {
    let anchor = format!("comment_{id}");
    let link = base.map(|base| {
        let mut link = base.clone();
        link.set_fragment(Some(&anchor));
        link.to_string()
    });
    let shown = comment
        .text("status")
        .is_none_or(|status| status == "published")
        && comment.get("isSuspended") != Some(&Value::Bool(true));
    let message = comment.text("message");
    let body = if shown {
        message.and_then(|html| body(html, page))
    } else {
        // Удалённое и скрытое — фраза Хабра вместо текста, как пометка.
        message
            .map(html_text)
            .filter(|said| !said.is_empty())
            .map(|said| format!("*{}*", inline(&said)))
    };
    Reply {
        depth,
        author: comment
            .get("author")
            .and_then(|author| author.text("alias"))
            .filter(|_| shown)
            .map(str::to_owned),
        date: comment.text("timePublished").map(date_text),
        link,
        on: None,
        body,
        anchor: Some(anchor),
    }
}

/// Текст комментария. Он приходит строкой JSON, а не в странице, и глубину
/// его разметки до сих пор никто не мерил: слишком глубокий — текстом.
fn body(html: &str, page: &str) -> Option<String> {
    if crate::nesting::html_too_deep(html) {
        let text = html_text(html);
        return (!text.is_empty()).then(|| inline(&text));
    }
    reply_markdown(html, page)
}

fn html_text(html: &str) -> String {
    squeeze(&Document::fragment(html).root().text())
}

#[cfg(test)]
mod tests {
    use super::*;

    const ARTICLE: &str = "https://habr.com/ru/articles/100/";

    /// Комментарий в форме API: номер, родитель, дети, автор, текст.
    fn comment(id: u32, parent: Option<u32>, children: &[u32], who: &str, text: &str) -> String {
        let parent = parent.map_or("null".to_owned(), |parent| format!("\"{parent}\""));
        let children: Vec<String> = children.iter().map(|id| format!("\"{id}\"")).collect();
        format!(
            r#""{id}": {{"id": "{id}", "parentId": {parent}, "level": 0,
                "timePublished": "2026-10-15T21:35:10+00:00", "isSuspended": false,
                "status": "published", "score": 3, "message": "<p>{text}</p>",
                "author": {{"alias": "{who}", "fullname": "Имя"}},
                "children": [{}]}}"#,
            children.join(", ")
        )
    }

    fn tree_of(comments: &[String], threads: &str) -> String {
        format!(
            r#"{{"comments": {{{}}}, "threads": {threads}, "pinnedCommentIds": []}}"#,
            comments.join(", ")
        )
    }

    fn tree() -> String {
        let comments = [
            comment(1, None, &[2, 4], "ann", "Первый"),
            comment(2, Some(1), &[3], "bob", "Ответ"),
            comment(3, Some(2), &[], "ann", "Ответ на ответ"),
            comment(4, Some(1), &[], "cid", "Второй ответ"),
            comment(5, None, &[], "dan", "Ещё один"),
        ];
        tree_of(&comments, r#"["1", "5"]"#)
    }

    #[test]
    fn the_comments_go_under_the_article_as_a_thread() {
        let tree = json::parse(&tree()).unwrap();
        let out = section(&tree, ARTICLE).unwrap();
        assert!(out.starts_with("<a id=\"publication-comments\"></a>\n\n## 5 comments\n\n"));
        let order: Vec<&str> = [
            "Первый",
            "Ответ",
            "Ответ на ответ",
            "Второй ответ",
            "Ещё один",
        ]
        .into_iter()
        .map(|text| &out[out.find(text).unwrap()..])
        .collect();
        assert!(order.windows(2).all(|pair| pair[0].len() > pair[1].len()));
        assert!(out.contains(
            "<a id=\"comment_1\"></a>**ann** · \
             [15 October 2026](https://habr.com/ru/articles/100/#comment_1)\n\nПервый"
        ));
        assert!(out.contains("> > <a id=\"comment_3\"></a>**ann**"));
        assert!(out.contains("> <a id=\"comment_4\"></a>**cid**"));
        assert!(out.contains("\n\n<a id=\"comment_5\"></a>**dan**"));
    }

    #[test]
    fn a_deleted_comment_keeps_its_place_and_habr_s_words() {
        let raw = tree().replace(
            r#""status": "published", "score": 3, "message": "<p>Ответ</p>""#,
            r#""status": "author_deactivated", "score": 3,
               "message": "НЛО прилетело и опубликовало эту надпись здесь""#,
        );
        let out = section(&json::parse(&raw).unwrap(), ARTICLE).unwrap();
        assert!(out.contains(
            "> <a id=\"comment_2\"></a>[15 October 2026](https://habr.com/ru/articles/100/#comment_2)\n\
             >\n> *НЛО прилетело и опубликовало эту надпись здесь*"
        ));
        assert!(!out.contains("**bob**"));
        // Ответ на удалённое остаётся под ним.
        assert!(out.contains("> > <a id=\"comment_3\"></a>**ann**"));
    }

    #[test]
    fn what_the_roots_miss_still_comes_and_a_loop_ends() {
        // Пятого нет в корнях, третий называет ребёнком первого.
        let comments = [
            comment(1, None, &[2, 4], "ann", "Первый"),
            comment(2, Some(1), &[3], "bob", "Ответ"),
            comment(3, Some(2), &[1], "ann", "Ответ на ответ"),
            comment(4, Some(1), &[], "cid", "Второй ответ"),
            comment(5, None, &[], "dan", "Ещё один"),
        ];
        let raw = tree_of(&comments, r#"["1"]"#);
        let out = section(&json::parse(&raw).unwrap(), ARTICLE).unwrap();
        assert!(out.starts_with("<a id=\"publication-comments\"></a>\n\n## 5 comments"));
        assert_eq!(out.matches("**ann**").count(), 2);
        assert!(out.contains("\n\n<a id=\"comment_5\"></a>**dan**"));
    }

    #[test]
    fn no_comments_no_section() {
        let tree = json::parse(r#"{"comments": {}, "threads": []}"#).unwrap();
        assert_eq!(section(&tree, ARTICLE), None);
        let tree = json::parse(r#"{"comments": [], "threads": []}"#).unwrap();
        assert_eq!(section(&tree, ARTICLE), None);
    }

    #[test]
    fn comments_that_did_not_come_leave_a_way_to_them() {
        let out = elsewhere(331, ARTICLE);
        assert!(out.contains("## 331 comments\n\n"));
        assert!(out.contains("(https://habr.com/ru/articles/100/comments/)"));
        let out = elsewhere(2, "https://habr.com/ru/news/100?utm=x#comment_5");
        assert!(out.contains("(https://habr.com/ru/news/100/comments/)"));
    }

    #[test]
    fn the_count_and_the_tree_come_from_the_page_s_own_state() {
        let html = r#"<script>if (window.__PINIA_STATE__) {}</script>
            <script>window.__PINIA_STATE__ = {"articlesList": {"articlesList":
            {"100": {"statistics": {"commentsCount": 331}}}}};(function(){var s=1})()</script>"#;
        let state = state_in(html).unwrap();
        assert_eq!(count(&state, "100"), Some(331));
        assert_eq!(count(&state, "101"), None);
        assert_eq!(state_in("<script>window.__NUXT__={}</script>"), None);
    }

    #[test]
    fn the_comments_page_reads_as_a_thread_from_its_state() {
        let comments = tree()
            .replace("\"comments\":", "\"refs\":")
            .replace("\"threads\":", "\"root\":");
        let html = format!(
            "<html><head><title>Статья / Комментарии / Хабр</title></head><body>\
             <div class=\"tm-comment-thread\">разметка</div>\
             <script>window.__PINIA_STATE__={{\"articlesList\":{{\"articlesList\":{{\"100\":{{\
             \"titleHtml\":\"Как мы &quot;переехали&quot;\",\"timePublished\":\"2026-10-15T10:00:00+00:00\",\
             \"author\":{{\"alias\":\"writer\"}}}}}}}},\
             \"comments\":{{\"articleComments\":{{\"100\":{}}}}}}};(function(){{}})()</script>\
             </body></html>",
            comments
        );
        let url = "https://habr.com/ru/articles/100/comments/";
        let page = read(&Document::from(html.as_str()), url).unwrap();
        assert!(page.thread);
        assert_eq!(page.title, "Как мы \"переехали\"");
        assert!(page.markdown.starts_with(
            "# [Как мы \"переехали\"](https://habr.com/ru/articles/100/)\n\n\
             *writer · 15 October 2026*\n\n## 5 comments\n\n"
        ));
        assert!(
            page.markdown.contains(
                "[15 October 2026](https://habr.com/ru/articles/100/comments/#comment_1)"
            )
        );
        // Без состояния страница — общему тракту.
        let bare = "<html><body><p>Комментарии</p></body></html>";
        assert_eq!(read(&Document::from(bare), url), None);
    }

    #[test]
    fn the_addresses_of_publications_and_their_comments() {
        let of = |url: &str| Publication::of(url).map(|p| (p.id, p.post, p.comments));
        let some = |id: &str, post, comments| Some((id.to_owned(), post, comments));
        assert_eq!(
            of("https://habr.com/ru/articles/100/"),
            some("100", false, false)
        );
        assert_eq!(
            of("https://habr.com/en/articles/100/#comment_5"),
            some("100", false, false)
        );
        assert_eq!(
            of("https://habr.com/ru/news/100/"),
            some("100", false, false)
        );
        assert_eq!(
            of("https://habr.com/ru/posts/100/"),
            some("100", true, false)
        );
        assert_eq!(
            of("https://habr.com/ru/companies/selectel/articles/100/"),
            some("100", false, false)
        );
        assert_eq!(
            of("https://habr.com/ru/companies/selectel/news/100/comments/"),
            some("100", false, true)
        );
        assert_eq!(
            of("https://habr.com/ru/articles/100/comments/"),
            some("100", false, true)
        );
        for not in [
            "https://habr.com/ru/posts/100/comments/",
            "https://habr.com/ru/articles/",
            "https://habr.com/ru/articles/top/yearly/",
            "https://habr.com/ru/companies/selectel/articles/",
            "https://habr.com/ru/users/someone/",
            "https://example.com/ru/articles/100/",
            "https://habr.com/ru/hubs/rust/articles/",
        ] {
            assert_eq!(of(not), None, "{not}");
        }
        assert_eq!(
            Publication::of("https://habr.com/ru/posts/7/")
                .unwrap()
                .source(),
            "https://habr.com/kek/v2/threads/7/comments/"
        );
        assert_eq!(
            Publication::of("https://habr.com/ru/news/7/")
                .unwrap()
                .source(),
            "https://habr.com/kek/v2/articles/7/comments/"
        );
        assert!(is_comments_page(
            "https://habr.com/ru/articles/100/comments/"
        ));
        assert!(!is_comments_page(ARTICLE));
    }
}
