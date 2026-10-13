//! Общее для площадок обсуждений (#11): страница, разобранная правилом,
//! реплика и то, как реплики пишутся markdown-ом. Свою разметку каждая
//! площадка разбирает сама (`hacker_news`, `lobsters`); здесь — вид треда
//! у читателя, чтобы он был одним и тем же, откуда бы тред ни пришёл.
//!
//! Отступ ответа — цитатой, как у общего тракта по распоркам, и не глубже
//! [`MAX_NEST`]: в колонке в 65 знаков десятый уровень съел бы саму реплику.
//! Ответ глубже говорит, кому отвечает, иначе его не отличить от ответа
//! соседу. Курсив цитаты на треде оставлен нарочно (мейнтейнер, 13 октября
//! 2026).

use dom_query::{NodeRef, Selection};
use url::Url;

use crate::extract::Link;
use crate::feed::{destination, inline};
use crate::markdown::MAX_NEST;

/// Страница площадки, разобранная правилом.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Page {
    pub title: String,
    pub markdown: String,
    /// Тред читают — это статья. Список историй или реплик — список ссылок:
    /// его открывают ради нового, и недельная копия была бы враньём.
    pub thread: bool,
    /// Навигация сайта, если правило знает её лучше общего (`extract::site`).
    pub site: Option<Vec<Link>>,
}

/// Реплика дерева ответов или списка реплик.
pub(crate) struct Reply {
    pub depth: usize,
    pub author: Option<String>,
    pub date: Option<String>,
    pub link: Option<String>,
    /// История, к которой реплика, — в списках реплик; в треде её нет,
    /// там это сама страница.
    pub on: Option<(String, Option<String>)>,
    pub body: Option<String>,
    /// Имя якоря реплики — то, что площадка ставит после решётки в ссылке
    /// на реплику внутри треда: ссылка с ним прокручивает к месту.
    pub anchor: Option<String>,
}

/// Заголовок над репликами: «12 comments», «1 reply», «No comments yet».
pub(crate) fn count_heading(count: usize, word: &str) -> String {
    match count {
        0 => format!("## No {} yet\n\n", plural(word)),
        1 => format!("## 1 {word}\n\n"),
        count => format!("## {count} {}\n\n", plural(word)),
    }
}

fn plural(word: &str) -> String {
    match word {
        "reply" => "replies".to_owned(),
        word => format!("{word}s"),
    }
}

/// Реплики подряд: шапка — автор, кому отвечает (если отступ кончился),
/// день ссылкой на реплику, история; под ней текст; уровень — цитатой.
pub(crate) fn write_replies(replies: &[Reply], out: &mut String) {
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
        // Якорь — сырым HTML: другого способа поставить его в markdown нет,
        // и так же ставят якоря в README (`<a id="…"></a>`).
        let mark = reply
            .anchor
            .as_ref()
            .filter(|name| {
                name.chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-'))
            })
            .map(|name| format!("<a id=\"{name}\"></a>"))
            .unwrap_or_default();
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
        let mut text = format!("{mark}{}", head.join(" · "));
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

/// `<title>` страницы, если он есть и не пуст.
pub(crate) fn title_of(root: &NodeRef) -> Option<String> {
    first(root, "title")
        .map(|node| squeeze(&node.text()))
        .filter(|title| !title.is_empty())
}

/// Адрес от страницы. Только http и https: `javascript:` никуда не ведёт.
pub(crate) fn absolute(base: &Url, href: &str) -> Option<String> {
    let url = base.join(href.trim()).ok()?;
    matches!(url.scheme(), "http" | "https").then(|| url.to_string())
}

pub(crate) fn first<'a>(node: &NodeRef<'a>, selector: &str) -> Option<NodeRef<'a>> {
    Selection::from(*node)
        .select(selector)
        .nodes()
        .first()
        .cloned()
}

pub(crate) fn all<'a>(node: &NodeRef<'a>, selector: &str) -> Vec<NodeRef<'a>> {
    Selection::from(*node).select(selector).nodes().to_vec()
}

pub(crate) fn squeeze(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}
