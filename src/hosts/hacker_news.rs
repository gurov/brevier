//! Hacker News (#11): тред и списки историй — своим правилом, мимо Readability.
//!
//! Общий тракт на треде теряет автора и время каждой реплики — шапку
//! комментария Readability срезает как обвязку (0 авторов из 91), — и
//! выбрасывает короткие реплики из одной ссылки правилом про долю ссылок
//! (7 из 91): на статье оно право, на треде убивает то, что ведёт дальше.
//! Главная и прочие списки историй выходили пустыми — табличная вёрстка
//! без единого абзаца. Порог извлечения ради этого не трогаем, его закрепил
//! корпус; страницу HN разбираем сами: вся она — таблицы одного вида,
//! и всё нужное в них лежит под своими классами.
//!
//! Признак — хост и форма. `table.fatitem` — страница записи (истории или
//! реплики) с деревом ответов; строки `tr.athing.submission` без неё —
//! список историй. Остальное (профиль, `newcomments`) идёт общим трактом.
//!
//! Вложенность — атрибут `indent` у ячейки отступа (ширина распорки по 40 px —
//! запасной путь), отступ — цитатой, как у общего тракта, и не глубже
//! [`MAX_NEST`]; реплика глубже говорит, кому отвечает, иначе её не отличить
//! от ответа соседу. Время — день из `title` у `span.age` (ISO, UTC),
//! а не «3 hours ago»: копия в архиве через месяц врала бы.

use dom_query::{Document, NodeRef, Selection};
use url::Url;

use crate::extract::Link;
use crate::feed::{date_text, destination, heading, inline, reply_markdown};
use crate::markdown::MAX_NEST;

const HOST: &str = "news.ycombinator.com";

/// Шаг распорки отступа — на случай, если у ячейки нет `indent`.
const SPACER_STEP: usize = 40;

/// Страница HN, разобранная правилом.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Page {
    pub title: String,
    pub markdown: String,
    /// Тред читают — это статья. Список историй — список ссылок.
    pub thread: bool,
}

/// Адрес на хосте HN. Какая это страница, решает форма (см. [`read`]).
pub fn is_hacker_news(url: &str) -> bool {
    Url::parse(url)
        .ok()
        .and_then(|url| url.host_str().map(|host| host.eq_ignore_ascii_case(HOST)))
        .unwrap_or(false)
}

/// Тред, список историй или список реплик — или `None`, если страница
/// не той формы и её читает общий тракт.
pub fn read(doc: &Document, url: &str) -> Option<Page> {
    if !is_hacker_news(url) {
        return None;
    }
    let base = Url::parse(url).ok()?;
    // Ссылка «reply» под каждой репликой — для вошедших; читателю не нужна.
    doc.select("div.reply").remove();
    let root = doc.root();
    match first(&root, "table.fatitem") {
        Some(item) => Some(thread(&root, &item, &base)),
        None => stories(&root, &base).or_else(|| comments(&root, &base)),
    }
}

/// Навигация сайта для полки: меню в шапке и свои страницы из подвала.
/// Общее правило (`extract::site`) тут не годится: у каждой реплики
/// `span.navs` со ссылками «parent», «next», и на треде их сотни.
/// «submit» и «login» — для вошедших, их не берём.
pub fn site(doc: &Document, url: &str) -> Vec<Link> {
    let Ok(base) = Url::parse(url) else {
        return Vec::new();
    };
    let root = doc.root();
    let mut links = Vec::new();
    let menu = first(&root, "span.pagetop").map(|top| all(&top, "a"));
    let footer = first(&root, "span.yclinks").map(|bottom| all(&bottom, "a"));
    for link in menu.into_iter().chain(footer).flatten() {
        let title = squeeze(&link.text());
        let Some(address) = link.attr("href").and_then(|href| absolute(&base, &href)) else {
            continue;
        };
        let own = Url::parse(&address)
            .ok()
            .is_some_and(|url| url.host_str() == Some(HOST));
        let personal = link.attr("rel").is_some_and(|rel| rel.contains("nofollow"));
        if own
            && !personal
            && !title.is_empty()
            && !links.iter().any(|seen: &Link| seen.address == address)
        {
            links.push(Link { title, address });
        }
    }
    links
}

/// История в списке и в шапке треда.
struct Story {
    id: String,
    title: String,
    /// Куда ведёт заголовок. У «Ask HN» — на саму запись.
    link: Option<String>,
    site: Option<String>,
    points: Option<String>,
    author: Option<String>,
    date: Option<String>,
    /// Подпись ссылки на обсуждение: «300 comments», «discuss».
    comments: Option<String>,
}

impl Story {
    fn of(row: &NodeRef, base: &Url) -> Option<Story> {
        let title_link = first(row, ".titleline > a")?;
        let title = squeeze(&title_link.text());
        if title.is_empty() {
            return None;
        }
        let sub = row
            .next_element_sibling()
            .and_then(|tr| first(&tr, "td.subtext"));
        let text_of = |selector: &str| {
            sub.as_ref()
                .and_then(|sub| first(sub, selector))
                .map(|node| squeeze(&node.text()))
                .filter(|text| !text.is_empty())
        };
        let comments = sub.as_ref().and_then(|sub| {
            all(sub, "a")
                .into_iter()
                .map(|a| squeeze(&a.text()))
                .find(|label| {
                    label == "discuss"
                        || label.ends_with(" comment")
                        || label.ends_with(" comments")
                })
        });
        Some(Story {
            id: row.attr("id").map(|id| id.to_string()).unwrap_or_default(),
            title,
            link: title_link
                .attr("href")
                .and_then(|href| absolute(base, &href)),
            site: first(row, ".sitestr").map(|node| squeeze(&node.text())),
            points: text_of(".score"),
            author: text_of("a.hnuser"),
            date: sub.as_ref().and_then(age),
            comments,
        })
    }

    /// Адрес обсуждения истории.
    fn item(&self) -> String {
        item_url(&self.id)
    }
}

/// Список историй: главная, `newest`, `ask`, `show`, `jobs`, `front`.
/// Тот же вид, что у ленты: история — заголовок второго уровня ссылкой
/// (полка показывает их оглавлением), под ним строка курсивом — сайт, очки,
/// автор, день и ссылка на обсуждение.
fn stories(root: &NodeRef, base: &Url) -> Option<Page> {
    let list: Vec<Story> = all(root, "tr.athing.submission")
        .iter()
        .filter_map(|row| Story::of(row, base))
        .collect();
    if list.is_empty() {
        return None;
    }
    let title = page_title(root);
    let mut out = format!("# {}\n\n", heading(&title));
    for story in &list {
        let link = story.link.clone().unwrap_or_else(|| story.item());
        out.push_str(&format!(
            "## [{}]({})\n\n",
            heading(&story.title),
            destination(&link)
        ));
        let mut meta: Vec<String> = [&story.site, &story.points, &story.author, &story.date]
            .into_iter()
            .flatten()
            .map(|text| inline(text))
            .collect();
        if let Some(label) = &story.comments {
            let label = if label == "discuss" {
                "no comments yet"
            } else {
                label
            };
            meta.push(format!(
                "[{}]({})",
                inline(label),
                destination(&story.item())
            ));
        }
        if !meta.is_empty() {
            out.push_str(&format!("*{}*\n\n", meta.join(" · ")));
        }
    }
    out.truncate(out.trim_end().len());
    out.push('\n');
    Some(Page {
        title,
        markdown: out,
        thread: false,
    })
}

/// Реплика дерева ответов или списка реплик.
struct Reply {
    depth: usize,
    author: Option<String>,
    date: Option<String>,
    link: Option<String>,
    /// История, к которой реплика, — в списках реплик (`newcomments`,
    /// реплики одного автора); в треде её нет, там это сама страница.
    on: Option<(String, Option<String>)>,
    body: Option<String>,
}

impl Reply {
    fn of(row: &NodeRef, base: &Url) -> Reply {
        Reply {
            depth: depth(row),
            author: first(row, ".comhead a.hnuser").map(|node| squeeze(&node.text())),
            date: first(row, ".comhead").as_ref().and_then(age),
            link: row.attr("id").map(|id| item_url(&id)),
            on: on_story(row, base),
            body: body(row, base.as_str()),
        }
    }
}

/// Строки реплик: `tr.athing` со шапкой реплики, кроме самой записи
/// в `table.fatitem`.
fn replies(root: &NodeRef, base: &Url) -> Vec<Reply> {
    all(root, "tr.athing")
        .iter()
        .filter(|row| first(row, ".comhead").is_some())
        .filter(|row| !row.ancestors_it(None).any(|up| up.is("table.fatitem")))
        .map(|row| Reply::of(row, base))
        .collect()
}

/// Тред: шапка записи — история с текстом или одна реплика со своей
/// ссылкой на родителя, — дальше дерево ответов.
fn thread(root: &NodeRef, item: &NodeRef, base: &Url) -> Page {
    let (title, mut out, word) = match first(item, "tr.athing.submission") {
        Some(row) => story_head(item, &row, root, base),
        None => comment_head(item, base),
    };
    let replies = replies(root, base);
    out.push_str(&match replies.len() {
        0 => format!("## No {word}s yet\n\n"),
        1 => format!("## 1 {word}\n\n"),
        count => format!("## {count} {}\n\n", plural(word)),
    });
    write_replies(&replies, &mut out);
    Page {
        title,
        markdown: out,
        thread: true,
    }
}

/// Список реплик без записи: свежие (`newcomments`), реплики одного автора
/// (`threads?id=…`). У каждой — история, к которой она. Список, а не статья:
/// открывают его ради нового, и недельная копия была бы враньём.
fn comments(root: &NodeRef, base: &Url) -> Option<Page> {
    let replies = replies(root, base);
    if replies.is_empty() {
        return None;
    }
    let title = page_title(root);
    let mut out = format!("# {}\n\n", heading(&title));
    write_replies(&replies, &mut out);
    Some(Page {
        title,
        markdown: out,
        thread: false,
    })
}

/// Реплики подряд: шапка — автор, кому отвечает (если отступ кончился),
/// день ссылкой на реплику, история; под ней текст; уровень — цитатой.
fn write_replies(replies: &[Reply], out: &mut String) {
    // Кто на каком уровне ответил последним: родитель реплики глубже
    // отступа — последний автор уровнем выше.
    let mut authors: Vec<Option<String>> = Vec::new();
    for reply in replies {
        authors.truncate(reply.depth);
        let parent = reply
            .depth
            .checked_sub(1)
            .and_then(|level| authors.get(level).cloned().flatten());
        authors.resize(reply.depth, None);
        authors.push(reply.author.clone());

        let mut head = Vec::new();
        if let Some(author) = &reply.author {
            head.push(format!("**{}**", inline(author)));
        }
        if reply.depth > MAX_NEST
            && let Some(parent) = parent
        {
            head.push(format!("replying to {}", inline(&parent)));
        }
        let when = reply.date.as_deref().unwrap_or("link");
        match &reply.link {
            Some(link) => head.push(format!("[{}]({})", inline(when), destination(link))),
            None if reply.date.is_some() => head.push(inline(when)),
            None => {}
        }
        match &reply.on {
            Some((story, Some(link))) => {
                head.push(format!("on [{}]({})", inline(story), destination(link)));
            }
            Some((story, None)) => head.push(format!("on {}", inline(story))),
            None => {}
        }
        let mut text = head.join(" · ");
        if let Some(body) = &reply.body {
            if !text.is_empty() {
                text.push_str("\n\n");
            }
            text.push_str(body);
        }
        // Между репликами — пустая строка без знака цитаты: со знаком
        // соседние ответы одного уровня склеились бы в одну цитату.
        out.push_str(&quote(&text, reply.depth.min(MAX_NEST)));
        out.push_str("\n\n");
    }
    out.truncate(out.trim_end().len());
    out.push('\n');
}

/// История, к которой реплика: `.onstory` в её шапке. Полное название —
/// в `title` ссылки, в тексте оно обрезано многоточием.
fn on_story(row: &NodeRef, base: &Url) -> Option<(String, Option<String>)> {
    let link = first(row, ".onstory a")?;
    let title = link
        .attr("title")
        .map(|title| squeeze(&title))
        .filter(|title| !title.is_empty())
        .unwrap_or_else(|| squeeze(&link.text()));
    (!title.is_empty()).then(|| {
        (
            title,
            link.attr("href").and_then(|href| absolute(base, &href)),
        )
    })
}

/// Шапка треда истории: заголовок ссылкой туда, куда ведёт история (у «Ask
/// HN» — без ссылки: она вела бы сюда же), строка курсивом и текст записи.
fn story_head(
    item: &NodeRef,
    row: &NodeRef,
    root: &NodeRef,
    base: &Url,
) -> (String, String, &'static str) {
    let Some(story) = Story::of(row, base) else {
        let title = page_title(root);
        return (
            title.clone(),
            format!("# {}\n\n", heading(&title)),
            "comment",
        );
    };
    let mut out = match &story.link {
        Some(link) if *link != story.item() => {
            format!("# [{}]({})\n\n", heading(&story.title), destination(link))
        }
        _ => format!("# {}\n\n", heading(&story.title)),
    };
    let meta: Vec<String> = [&story.site, &story.points, &story.author, &story.date]
        .into_iter()
        .flatten()
        .map(|text| inline(text))
        .collect();
    if !meta.is_empty() {
        out.push_str(&format!("*{}*\n\n", meta.join(" · ")));
    }
    if let Some(text) = first(item, ".toptext")
        && let Some(body) = reply_markdown(&text.inner_html(), base.as_str())
    {
        out.push_str(&body);
        out.push_str("\n\n");
    }
    (story.title, out, "comment")
}

/// Шапка страницы одной реплики: кто и о чём, ссылки на родителя и на весь
/// тред, сама реплика.
fn comment_head(item: &NodeRef, base: &Url) -> (String, String, &'static str) {
    let author = first(item, ".comhead a.hnuser").map(|node| squeeze(&node.text()));
    let story = on_story(item, base);
    let title = match (&author, &story) {
        (Some(author), Some((story, _))) => format!("{author} on “{story}”"),
        (Some(author), None) => format!("Comment by {author}"),
        _ => "Comment".to_owned(),
    };
    let mut out = format!("# {}\n\n", heading(&title));

    let mut meta = Vec::new();
    if let Some(date) = first(item, ".comhead").as_ref().and_then(age) {
        meta.push(inline(&date));
    }
    let parent = all(item, ".navs a")
        .into_iter()
        .find(|link| squeeze(&link.text()) == "parent")
        .and_then(|link| link.attr("href"))
        .and_then(|href| absolute(base, &href));
    if let Some(parent) = parent {
        meta.push(format!("[parent]({})", destination(&parent)));
    }
    if let Some((_, Some(link))) = &story {
        meta.push(format!("[whole thread]({})", destination(link)));
    }
    if !meta.is_empty() {
        out.push_str(&format!("*{}*\n\n", meta.join(" · ")));
    }
    if let Some(body) = body(item, base.as_str()) {
        out.push_str(&body);
        out.push_str("\n\n");
    }
    (title, out, "reply")
}

/// Текст реплики. Без `.commtext` HN кладёт в неё одно слово в скобках —
/// «[flagged]», «[deleted]»: его и показываем, курсивом, как пометку.
fn body(row: &NodeRef, page: &str) -> Option<String> {
    if let Some(text) = first(row, ".commtext") {
        return reply_markdown(&text.inner_html(), page);
    }
    let said = squeeze(&first(row, "div.comment")?.immediate_text());
    (!said.is_empty()).then(|| format!("*{}*", inline(&said)))
}

/// Уровень реплики: `indent` у ячейки отступа, без него — ширина распорки.
fn depth(row: &NodeRef) -> usize {
    let Some(cell) = first(row, "td.ind") else {
        return 0;
    };
    if let Some(level) = cell
        .attr("indent")
        .and_then(|level| level.trim().parse().ok())
    {
        return level;
    }
    first(&cell, "img")
        .and_then(|img| img.attr("width"))
        .and_then(|width| width.trim().parse::<usize>().ok())
        .map_or(0, |width| width / SPACER_STEP)
}

/// День из `span.age`: в `title` — время по ISO (у новых страниц за ним
/// ещё секунды Unix через пробел).
fn age(node: &NodeRef) -> Option<String> {
    let stamp = first(node, "span.age")?.attr("title")?;
    let stamp = stamp.split_whitespace().next()?;
    Some(date_text(stamp))
}

/// Цитата нужной глубины: строка за строкой, пустые — одним знаком.
fn quote(text: &str, depth: usize) -> String {
    if depth == 0 {
        return text.to_owned();
    }
    let mark = "> ".repeat(depth);
    text.lines()
        .map(|line| {
            if line.trim().is_empty() {
                mark.trim_end().to_owned()
            } else {
                format!("{mark}{line}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn plural(word: &str) -> String {
    match word {
        "reply" => "replies".to_owned(),
        word => format!("{word}s"),
    }
}

fn page_title(root: &NodeRef) -> String {
    first(root, "title")
        .map(|node| squeeze(&node.text()))
        .filter(|title| !title.is_empty())
        .unwrap_or_else(|| "Hacker News".to_owned())
}

fn item_url(id: &str) -> String {
    format!("https://{HOST}/item?id={id}")
}

/// Адрес от страницы. Только http и https: `javascript:` никуда не ведёт.
fn absolute(base: &Url, href: &str) -> Option<String> {
    let url = base.join(href.trim()).ok()?;
    matches!(url.scheme(), "http" | "https").then(|| url.to_string())
}

fn first<'a>(node: &NodeRef<'a>, selector: &str) -> Option<NodeRef<'a>> {
    Selection::from(*node)
        .select(selector)
        .nodes()
        .first()
        .cloned()
}

fn all<'a>(node: &NodeRef<'a>, selector: &str) -> Vec<NodeRef<'a>> {
    Selection::from(*node).select(selector).nodes().to_vec()
}

fn squeeze(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    const THREAD: &str = "https://news.ycombinator.com/item?id=100";

    /// Шапка сайта и подвал — как у HN: меню, «submit» для вошедших, вход.
    fn page(title: &str, body: &str) -> String {
        format!(
            "<html lang=\"en\" op=\"item\"><head><title>{title}</title></head><body><center>\
             <table id=\"hnmain\"><tr><td><table><tr><td><span class=\"pagetop\">\
             <b class=\"hnname\"><a href=\"news\">Hacker News</a></b>\
             <a href=\"newest\">new</a> | <a href=\"front\">past</a> | \
             <a href=\"submit\" rel=\"nofollow\">submit</a></span></td>\
             <td><span class=\"pagetop\"><a href=\"login?goto=news\">login</a></span></td>\
             </tr></table></td></tr><tr><td>{body}</td></tr></table>\
             <span class=\"yclinks\"><a href=\"newsguidelines.html\">Guidelines</a> | \
             <a href=\"https://www.ycombinator.com/legal/\">Legal</a> | \
             <a href=\"mailto:hn@ycombinator.com\">Contact</a></span></center></body></html>"
        )
    }

    /// Строка истории и строка под ней — очки, автор, время, обсуждение.
    fn story_rows(id: u32, title: &str, href: &str, site: Option<&str>, comments: &str) -> String {
        let site = site
            .map(|site| {
                format!(
                    "<span class=\"sitebit comhead\"> (<a href=\"from?site={site}\">\
                     <span class=\"sitestr\">{site}</span></a>)</span>"
                )
            })
            .unwrap_or_default();
        format!(
            "<tr class=\"athing submission\" id=\"{id}\"><td class=\"title\"><span class=\"rank\">1.</span></td>\
             <td class=\"title\"><span class=\"titleline\"><a href=\"{href}\">{title}</a>{site}</span></td></tr>\
             <tr><td colspan=\"2\"></td><td class=\"subtext\"><span class=\"subline\">\
             <span class=\"score\" id=\"score_{id}\">302 points</span> by \
             <a href=\"user?id=ada\" class=\"hnuser\">ada</a> \
             <span class=\"age\" title=\"2026-08-19T10:15:00 1755598500\"><a href=\"item?id={id}\">55 days ago</a></span> | \
             <a href=\"hide?id={id}\">hide</a> | <a href=\"item?id={id}\">{comments}</a></span></td></tr>"
        )
    }

    /// Реплика дерева: отступ, шапка, текст или пометка, «reply».
    fn comment(id: u32, indent: usize, who: &str, text: &str) -> String {
        let body = if text.starts_with('[') {
            text.to_owned()
        } else {
            format!("<div class=\"commtext c00\">{text}</div>")
        };
        format!(
            "<tr class=\"athing comtr\" id=\"{id}\"><td><table border=\"0\"><tr>\
             <td class=\"ind\" indent=\"{indent}\"><img src=\"s.gif\" height=\"1\" width=\"{}\"></td>\
             <td class=\"default\"><div><span class=\"comhead\">\
             <a href=\"user?id={who}\" class=\"hnuser\">{who}</a> \
             <span class=\"age\" title=\"2026-08-20T08:00:00\"><a href=\"item?id={id}\">54 days ago</a></span>\
             <span class=\"navs\"> | <a href=\"#1\" class=\"clicky\">parent</a> | <a href=\"#2\" class=\"clicky\">next</a></span>\
             <span class=\"onstory\"></span></span></div><br>\
             <div class=\"comment\">{body}<div class=\"reply\"><p><font size=\"1\"><u>\
             <a href=\"reply?id={id}\">reply</a></u></font></div></div></td></tr></table></td></tr>",
            indent * 40
        )
    }

    fn read_page(html: &str, url: &str) -> Option<Page> {
        read(&Document::from(html), url)
    }

    fn thread_html() -> String {
        let comments = [
            comment(1, 0, "leumon", "Suspicious: <a href=\"https:&#x2F;&#x2F;example.com&#x2F;captcha\" rel=\"nofollow\">https:&#x2F;&#x2F;example.com&#x2F;captcha</a>"),
            comment(2, 1, "bob", "First paragraph.<p>Second &amp; last."),
            comment(3, 2, "cy", "Deeper."),
            comment(4, 3, "dee", "Third level."),
            comment(5, 4, "eve", "Fourth level answers dee."),
            comment(6, 0, "fay", "[flagged]"),
        ]
        .concat();
        page(
            "Air Theremin | Hacker News",
            &format!(
                "<table class=\"fatitem\" border=\"0\">{}\
                 <tr><td colspan=\"2\"></td><td><div class=\"toptext\"></div></td></tr></table><br>\
                 <table border=\"0\" class=\"comment-tree\">{comments}</table>",
                story_rows(
                    100,
                    "Air Theremin",
                    "https://theremin.example.com/",
                    Some("example.com"),
                    "6&nbsp;comments"
                )
            ),
        )
    }

    #[test]
    fn a_thread_keeps_every_author_time_and_link_only_reply() {
        let page = read_page(&thread_html(), THREAD).unwrap();
        assert!(page.thread);
        assert_eq!(page.title, "Air Theremin");
        let md = &page.markdown;
        assert!(md.starts_with(
            "# [Air Theremin](https://theremin.example.com/)\n\n\
             *example.com · 302 points · ada · 19 August 2026*\n\n## 6 comments\n\n"
        ));
        // Реплика из одной ссылки — общий тракт её выбрасывал.
        assert!(md.contains(
            "**leumon** · [20 August 2026](https://news.ycombinator.com/item?id=1)\n\n\
             Suspicious: [https://example.com/captcha](https://example.com/captcha)"
        ));
        assert!(md.contains(
            "> **bob** · [20 August 2026](https://news.ycombinator.com/item?id=2)\n>\n\
             > First paragraph.\n>\n> Second & last."
        ));
        // Ссылки «reply», «parent», «next» читателю не нужны.
        assert!(!md.contains("reply?id"));
        assert!(!md.contains("parent"));
        assert!(md.ends_with(
            "**fay** · [20 August 2026](https://news.ycombinator.com/item?id=6)\n\n*\\[flagged\\]*\n"
        ));
    }

    #[test]
    fn deeper_than_the_indent_a_reply_says_whom_it_answers() {
        let md = read_page(&thread_html(), THREAD).unwrap().markdown;
        // Третий уровень — последний, что отступает.
        assert!(md.contains("> > > **dee** · [20 August 2026]"));
        assert!(md.contains(
            "> > > **eve** · replying to dee · [20 August 2026](https://news.ycombinator.com/item?id=5)"
        ));
        // Мельче — родителя видно по отступу, и он не назван.
        assert!(!md.contains("replying to bob"));
        // Соседние реплики — разные цитаты: между ними пустая строка без знака.
        assert!(md.contains("Deeper.\n\n> > > **dee**"));
    }

    #[test]
    fn an_ask_hn_has_its_text_and_no_link_to_itself() {
        let html = page(
            "Ask HN | Hacker News",
            &format!(
                "<table class=\"fatitem\" border=\"0\">{}\
                 <tr><td colspan=\"2\"></td><td><div class=\"toptext\">Is it viable?\
                 <p>I flash routers. See <a href=\"item?id=7\">this</a>.</div></td></tr></table><br>\
                 <table border=\"0\" class=\"comment-tree\"></table>",
                story_rows(100, "Ask HN: Routers?", "item?id=100", None, "discuss")
            ),
        );
        let page = read_page(&html, THREAD).unwrap();
        assert_eq!(
            page.markdown,
            "# Ask HN: Routers?\n\n*302 points · ada · 19 August 2026*\n\n\
             Is it viable?\n\nI flash routers. See [this](https://news.ycombinator.com/item?id=7).\n\n\
             ## No comments yet\n"
        );
    }

    #[test]
    fn a_list_of_stories_reads_as_a_list_of_links() {
        let html = page(
            "Hacker News",
            &format!(
                "<table border=\"0\" class=\"itemlist\">{}{}\
                 <tr class=\"athing submission\" id=\"300\"><td class=\"title\"><span class=\"titleline\">\
                 <a href=\"https://jobs.example.com/\">Example (YC S22) is hiring</a></span></td></tr>\
                 <tr><td colspan=\"2\"></td><td class=\"subtext\"><span class=\"age\" title=\"2026-10-11T09:00:00\">\
                 <a href=\"item?id=300\">2 days ago</a></span></td></tr>\
                 <tr><td><a href=\"news?p=2\" class=\"morelink\" rel=\"next\">More</a></td></tr></table>",
                story_rows(
                    100,
                    "Gemini & co",
                    "https://deepmind.example.com/",
                    Some("example.com"),
                    "300&nbsp;comments"
                ),
                story_rows(200, "Ask HN: Anyone?", "item?id=200", None, "discuss")
            ),
        );
        let page = read_page(&html, "https://news.ycombinator.com/news").unwrap();
        assert!(!page.thread);
        assert_eq!(
            page.markdown,
            "# Hacker News\n\n\
             ## [Gemini \\& co](https://deepmind.example.com/)\n\n\
             *example.com · 302 points · ada · 19 August 2026 · [300 comments](https://news.ycombinator.com/item?id=100)*\n\n\
             ## [Ask HN: Anyone?](https://news.ycombinator.com/item?id=200)\n\n\
             *302 points · ada · 19 August 2026 · [no comments yet](https://news.ycombinator.com/item?id=200)*\n\n\
             ## [Example (YC S22) is hiring](https://jobs.example.com/)\n\n\
             *11 October 2026*\n"
        );
    }

    #[test]
    fn a_comment_s_own_page_names_its_story_and_parent() {
        let html = page(
            "Comment | Hacker News",
            &format!(
                "<table class=\"fatitem\" border=\"0\"><tr class=\"athing\" id=\"50\"><td class=\"ind\"></td>\
                 <td class=\"default\"><div><span class=\"comhead\"><a href=\"user?id=mia\" class=\"hnuser\">mia</a> \
                 <span class=\"age\" title=\"2026-10-13T10:16:31\"><a href=\"item?id=50\">7 hours ago</a></span>\
                 <span class=\"navs\"> | <a href=\"item?id=40\">parent</a> | <a href=\"item?id=100#50\">context</a>\
                 <span class=\"onstory\"> |  on: <a href=\"item?id=100\" title=\"Fridges defrost across Europe\">Fridges defrost acr...</a>\
                 </span></span></span></div><br><div class=\"comment\">[flagged]<div class=\"reply\"></div></div></td></tr></table><br>\
                 <table border=\"0\" class=\"comment-tree\">{}</table>",
                comment(60, 0, "noa", "A reply.")
            ),
        );
        let page = read_page(&html, "https://news.ycombinator.com/item?id=50").unwrap();
        assert!(page.thread);
        assert_eq!(page.title, "mia on “Fridges defrost across Europe”");
        assert_eq!(
            page.markdown,
            "# mia on “Fridges defrost across Europe”\n\n\
             *13 October 2026 · [parent](https://news.ycombinator.com/item?id=40) · \
             [whole thread](https://news.ycombinator.com/item?id=100)*\n\n\
             *\\[flagged\\]*\n\n## 1 reply\n\n\
             **noa** · [20 August 2026](https://news.ycombinator.com/item?id=60)\n\nA reply.\n"
        );
    }

    #[test]
    fn a_list_of_comments_says_what_each_is_about() {
        let html = page(
            "New Comments | Hacker News",
            "<table><tr class=\"athing\" id=\"70\"><td class=\"ind\"></td><td class=\"default\"><div>\
             <span class=\"comhead\"><a href=\"user?id=lin\" class=\"hnuser\">lin</a> \
             <span class=\"age\" title=\"2026-10-13T17:50:40\"><a href=\"item?id=70\">0 minutes ago</a></span>\
             <span class=\"navs\"> | <a href=\"item?id=65\">parent</a><span class=\"onstory\"> |  on: \
             <a href=\"item?id=60\" title=\"Firefox Enterprise\">Firefox Enterprise</a></span></span></span></div><br>\
             <div class=\"comment\"><div class=\"commtext c00\">Diversify.</div><div class=\"reply\"></div></div></td></tr>\
             <tr class=\"spacer\" style=\"height:15px\"></tr></table>",
        );
        let page = read_page(&html, "https://news.ycombinator.com/newcomments").unwrap();
        assert!(!page.thread);
        assert_eq!(
            page.markdown,
            "# New Comments \\| Hacker News\n\n\
             **lin** · [13 October 2026](https://news.ycombinator.com/item?id=70) · \
             on [Firefox Enterprise](https://news.ycombinator.com/item?id=60)\n\nDiversify.\n"
        );
    }

    #[test]
    fn only_hacker_news_and_only_its_known_forms() {
        assert!(read_page(&thread_html(), "https://example.com/item?id=100").is_none());
        // Профиль — не тред и не список: его читает общий тракт.
        let profile = page(
            "Profile: ada | Hacker News",
            "<table><tr><td>user:</td><td><a href=\"user?id=ada\" class=\"hnuser\">ada</a></td></tr></table>",
        );
        assert!(read_page(&profile, "https://news.ycombinator.com/user?id=ada").is_none());
        assert!(is_hacker_news("https://NEWS.ycombinator.com/item?id=1"));
        assert!(!is_hacker_news("https://ycombinator.com/item?id=1"));
    }

    #[test]
    fn the_core_reads_a_thread_as_an_article_and_a_list_as_links() {
        use crate::markdown::Kind;

        let thread = crate::from_html(&thread_html(), THREAD).unwrap();
        assert_eq!(thread.kind, Kind::Article);
        assert_eq!(thread.title, "Air Theremin");
        assert_eq!(thread.lang.as_deref(), Some("en"));
        assert!(thread.site.iter().all(|link| link.title != "parent"));

        let list = page(
            "Hacker News",
            &format!(
                "<table class=\"itemlist\">{}<tr><td>\
                 <a href=\"news?p=2\" class=\"morelink\" rel=\"next\">More</a></td></tr></table>",
                story_rows(
                    100,
                    "Gemini",
                    "https://deepmind.example.com/",
                    None,
                    "3&nbsp;comments"
                )
            ),
        );
        let list = crate::from_html(&list, "https://news.ycombinator.com/news").unwrap();
        assert_eq!(list.kind, Kind::Listing);
        // «More» внизу — следующая страница: пробел в конце листает дальше.
        assert_eq!(
            list.next.map(|next| next.address),
            Some("https://news.ycombinator.com/news?p=2".to_owned())
        );
    }

    #[test]
    fn the_shelf_gets_the_menu_not_the_reply_links() {
        let doc = Document::from(thread_html());
        let links: Vec<(String, String)> = site(&doc, THREAD)
            .into_iter()
            .map(|link| (link.title, link.address))
            .collect();
        let expected = [
            ("Hacker News", "https://news.ycombinator.com/news"),
            ("new", "https://news.ycombinator.com/newest"),
            ("past", "https://news.ycombinator.com/front"),
            (
                "Guidelines",
                "https://news.ycombinator.com/newsguidelines.html",
            ),
        ];
        assert_eq!(
            links,
            expected
                .iter()
                .map(|(title, address)| (title.to_string(), address.to_string()))
                .collect::<Vec<_>>()
        );
    }
}
