//! Форумы на Discourse (#11): тред и список тем — своим правилом.
//!
//! Discourse стоит на тысячах хостов, поэтому строка таблицы узнаёт не хост,
//! а программу: `<meta name="generator" content="Discourse …">` и страницу
//! для роботов (`body.crawler`) — её Discourse отдаёт клиенту без JavaScript.
//! Свежие версии (с осени 2026) отдают по `Accept: text/markdown` сам
//! markdown, с автором и датой у каждого поста, — тогда сюда ничего
//! не приходит: родной текст сайта лучше любого разбора.
//!
//! Страница для роботов тоже простая — посты в `div.topic-body.crawler-post`,
//! — но общий тракт склеивал автора и дату с текстом поста в одну строку,
//! а у части постов шапку терял. Тред у Discourse плоский: кто кому
//! отвечает, в этой странице не сказано, и отступов нет. Двадцать постов
//! на страницу, дальше — `rel="next"`, и пробел в конце листает дальше.

use dom_query::{Document, NodeRef};
use url::Url;

use super::thread::{
    Page, Reply, absolute, all, count_heading, first, squeeze, title_of, write_replies,
};
use crate::feed::{date_text, destination, heading, inline, reply_markdown};

/// Признак программы в шапке страницы — дёшево, по сырому HTML, до разбора.
pub fn may_be(html: &str) -> bool {
    html.contains("content=\"Discourse ") || html.contains("content='Discourse ")
}

/// Тред или список тем — или `None`, если это не страница Discourse для
/// роботов или она не той формы.
pub fn read(doc: &Document, url: &str) -> Option<Page> {
    let root = doc.root();
    let generator = first(&root, "meta[name=generator]")
        .and_then(|meta| meta.attr("content"))
        .is_some_and(|content| content.starts_with("Discourse "));
    if !generator || first(&root, "body.crawler").is_none() {
        return None;
    }
    let base = Url::parse(url).ok()?;
    let posts = all(&root, "div.topic-body.crawler-post");
    if !posts.is_empty() {
        return Some(thread(&root, &posts, &base));
    }
    topics(&root, &base)
}

/// Пост: номер в теме, автор, день, текст.
struct Post {
    position: Option<String>,
    author: Option<String>,
    date: Option<String>,
    body: Option<String>,
}

impl Post {
    fn of(post: &NodeRef, base: &Url) -> Post {
        let meta = first(post, ".crawler-post-meta");
        let text_of = |selector: &str| {
            meta.as_ref()
                .and_then(|meta| first(meta, selector))
                .map(|node| squeeze(&node.text()))
                .filter(|text| !text.is_empty())
        };
        Post {
            position: text_of("[itemprop=position]"),
            author: text_of(".creator [itemprop=name]").or_else(|| text_of(".creator a")),
            date: meta
                .as_ref()
                .and_then(|meta| first(meta, "time.post-time"))
                .and_then(|time| time.attr("datetime"))
                .map(|stamp| date_text(&stamp)),
            body: first(post, ".post").and_then(|text| {
                unbox(&text);
                reply_markdown(&text.inner_html(), base.as_str())
            }),
        }
    }
}

/// Превью ссылки (`aside.onebox`) — ссылкой-абзацем. Discourse рисует его
/// карточкой с заголовком `<h3>`, и заголовок уезжал на полку разделом
/// темы, которого в ней нет.
fn unbox(post: &NodeRef) {
    for card in all(post, "aside.onebox") {
        let link = first(&card, "h3 a").or_else(|| first(&card, "header a"));
        let Some((title, href)) = link.and_then(|link| {
            let title = squeeze(&link.text());
            let href = link.attr("href")?.to_string();
            (!title.is_empty()).then_some((title, href))
        }) else {
            continue;
        };
        let escape = |text: &str| {
            text.replace('&', "&amp;")
                .replace('<', "&lt;")
                .replace('>', "&gt;")
                .replace('"', "&quot;")
        };
        card.replace_with_html(format!(
            "<p><a href=\"{}\">{}</a></p>",
            escape(&href),
            escape(&title)
        ));
    }
}

/// Тема: заголовок, раздел и метки, первый пост — текстом темы, дальше
/// ответы подряд. На второй и следующих страницах первого поста нет.
fn thread(root: &NodeRef, posts: &[NodeRef], base: &Url) -> Page {
    let topic = first(root, "#topic-title h1 a");
    let title = topic
        .as_ref()
        .map(|link| squeeze(&link.text()))
        .filter(|title| !title.is_empty())
        .or_else(|| title_of(root))
        .unwrap_or_else(|| "Discussion".to_owned());
    // Адрес темы — ссылка заголовка; пост в ней — номером через косую.
    let topic_url = topic
        .and_then(|link| link.attr("href"))
        .and_then(|href| absolute(base, &href))
        .map(|url| url.trim_end_matches('/').to_owned());

    let mut posts: Vec<Post> = posts.iter().map(|post| Post::of(post, base)).collect();
    let opener = (posts
        .first()
        .is_some_and(|post| post.position.as_deref() == Some("1")))
    .then(|| posts.remove(0));

    let mut out = format!("# {}\n\n", heading(&title));
    let mut meta: Vec<String> = all(root, "#topic-title .category-name")
        .iter()
        .chain(all(root, "#topic-title .discourse-tag").iter())
        .map(|node| squeeze(&node.text()))
        .filter(|text| !text.is_empty())
        .map(|text| inline(&text))
        .collect();
    if let Some(opener) = &opener {
        meta.extend(
            [&opener.author, &opener.date]
                .into_iter()
                .flatten()
                .map(|text| inline(text)),
        );
    }
    if !meta.is_empty() {
        out.push_str(&format!("*{}*\n\n", meta.join(" · ")));
    }
    if let Some(body) = opener.as_ref().and_then(|opener| opener.body.as_ref()) {
        out.push_str(body);
        out.push_str("\n\n");
    }

    out.push_str(&if opener.is_some() {
        count_heading(posts.len(), "reply")
    } else {
        "## Replies, continued\n\n".to_owned()
    });
    let replies: Vec<Reply> = posts
        .into_iter()
        .map(|post| Reply {
            depth: 0,
            link: topic_url
                .as_ref()
                .zip(post.position.as_ref())
                .map(|(topic, position)| format!("{topic}/{position}")),
            author: post.author,
            date: post.date,
            on: None,
            body: post.body,
            anchor: None,
        })
        .collect();
    write_replies(&replies, &mut out);
    Page {
        title,
        markdown: out,
        thread: true,
        site: None,
    }
}

/// Список тем: последние, раздел, лучшие. Тема — заголовок второго уровня
/// ссылкой, под ним строка курсивом — раздел, метки, число ответов, —
/// и начало первого поста.
fn topics(root: &NodeRef, base: &Url) -> Option<Page> {
    let rows = all(root, "tr.topic-list-item");
    if rows.is_empty() {
        return None;
    }
    let title = title_of(root).unwrap_or_else(|| "Discussions".to_owned());
    let mut out = format!("# {}\n\n", heading(&title));
    for row in &rows {
        let Some(link) = first(row, "a.raw-topic-link") else {
            continue;
        };
        let name = squeeze(&link.text());
        let Some(address) = link.attr("href").and_then(|href| absolute(base, &href)) else {
            continue;
        };
        out.push_str(&format!(
            "## [{}]({})\n\n",
            heading(&name),
            destination(&address)
        ));
        let mut meta: Vec<String> = all(row, ".category-name, .discourse-tag")
            .iter()
            .map(|node| squeeze(&node.text()))
            .filter(|text| !text.is_empty())
            .map(|text| inline(&text))
            .collect();
        if let Some(replies) = first(row, "td.replies .posts")
            .map(|node| squeeze(&node.text()))
            .filter(|count| !count.is_empty() && count.chars().all(|c| c.is_ascii_digit()))
        {
            meta.push(match replies.as_str() {
                "0" => "no replies yet".to_owned(),
                "1" => "1 reply".to_owned(),
                count => format!("{count} replies"),
            });
        }
        if !meta.is_empty() {
            out.push_str(&format!("*{}*\n\n", meta.join(" · ")));
        }
        if let Some(excerpt) = first(row, "p.excerpt")
            .and_then(|excerpt| reply_markdown(&excerpt.inner_html(), base.as_str()))
        {
            out.push_str(&excerpt);
            out.push_str("\n\n");
        }
    }
    out.truncate(out.trim_end().len());
    out.push('\n');
    Some(Page {
        title,
        markdown: out,
        thread: false,
        site: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const TOPIC: &str = "https://forum.example.org/t/open-with/119080";

    fn page(main: &str) -> String {
        format!(
            "<html lang=\"en\"><head><title>Open with | Example Forum</title>\
             <meta name=\"generator\" content=\"Discourse 2026.9.0-latest - https://github.com/discourse/discourse\">\
             </head><body class=\"crawler \"><header><a href=\"/\">Example Forum</a></header>\
             <div id=\"main-outlet\" class=\"wrap\" role=\"main\">{main}</div></body></html>"
        )
    }

    fn post(position: usize, who: &str, day: &str, text: &str) -> String {
        format!(
            "<div id=\"post_{position}\" class=\"topic-body crawler-post\"><div class=\"crawler-post-meta\">\
             <span class=\"creator\" itemprop=\"author\"><a rel=\"nofollow\" href=\"/u/{who}\"><span itemprop=\"name\">{who}</span></a></span>\
             <span class=\"crawler-post-infos\"><time datetime=\"2026-10-{day}T22:32:28Z\" class=\"post-time\">October {day}, 2026, 10:32pm</time>\
             <span itemprop=\"position\">{position}</span></span></div>\
             <div class=\"post\" itemprop=\"text\">{text}</div>\
             <div class=\"crawler-linkback-list\"></div><div class=\"post-likes\">4 Likes</div></div>"
        )
    }

    fn topic(posts: &str) -> String {
        page(&format!(
            "<div id=\"topic-title\"><h1><a href=\"/t/open-with/119080\">Open with… does not open the file</a></h1>\
             <div class=\"topic-category\"><span class=\"category-name\">Bug reports</span></div>\
             <div class=\"topic-category\"><a href=\"/tag/tracked\" class=\"discourse-tag\">tracked</a></div></div>{posts}"
        ))
    }

    #[test]
    fn a_topic_is_its_first_post_and_every_reply_with_author_and_day() {
        let html = topic(
            &[
                post(1, "mikar", "05", "<p><strong>Steps</strong></p><h3><a name=\"p-1\" class=\"anchor\" href=\"#p-1\"></a>Expected</h3><p>It opens.</p>"),
                post(2, "WhiteNoise", "06", "<p>Perhaps we will change it later.</p>"),
            ]
            .concat(),
        );
        let page = read(&Document::from(html), TOPIC).unwrap();
        assert!(page.thread);
        assert_eq!(page.title, "Open with… does not open the file");
        assert_eq!(
            page.markdown,
            "# Open with… does not open the file\n\n\
             *Bug reports · tracked · mikar · 5 October 2026*\n\n\
             **Steps**\n\n### Expected\n\nIt opens.\n\n\
             ## 1 reply\n\n\
             **WhiteNoise** · [6 October 2026](https://forum.example.org/t/open-with/119080/2)\n\n\
             Perhaps we will change it later.\n"
        );
    }

    #[test]
    fn a_link_preview_is_a_link_not_a_heading() {
        let html = topic(&[
            post(1, "mikar", "05", "<p>Hi.</p>"),
            post(2, "noa", "06", "<p>Try this:</p><aside class=\"onebox allowlistedgeneric\">\
                <header class=\"source\"><a href=\"https://swiss.example/\">swiss.example</a></header>\
                <article class=\"onebox-body\"><h3><a href=\"https://swiss.example/send\">Send large files</a></h3>\
                <p>Securely and free.</p></article></aside>"),
        ].concat());
        let md = read(&Document::from(html), TOPIC).unwrap().markdown;
        assert!(
            md.ends_with("Try this:\n\n[Send large files](https://swiss.example/send)\n"),
            "{md}"
        );
        assert!(!md.contains("### "), "{md}");
    }

    #[test]
    fn a_later_page_has_no_first_post() {
        let html = topic(&post(21, "noa", "07", "<p>Later.</p>"));
        let md = read(&Document::from(html), &format!("{TOPIC}?page=2"))
            .unwrap()
            .markdown;
        assert!(md.contains("*Bug reports · tracked*\n\n## Replies, continued\n\n**noa** · [7 October 2026](https://forum.example.org/t/open-with/119080/21)"));
    }

    #[test]
    fn a_list_of_topics_reads_as_a_list_of_links() {
        let html = page(
            "<table class=\"topic-list\"><tbody><tr class=\"topic-list-item\"><td class=\"main-link\">\
             <span class=\"link-top-line\"><a href=\"https://forum.example.org/t/start-here/1\" class=\"title raw-link raw-topic-link\">Start here!</a></span>\
             <div class=\"link-bottom-line\"><a href=\"/c/news/67\" class=\"badge-wrapper bullet\"><span class=\"category-name\">News</span></a></div>\
             <p class=\"excerpt\">We are glad you&rsquo;re here.</p></td>\
             <td class=\"replies\"><span class=\"posts\" title=\"posts\">3</span></td>\
             <td class=\"views\"><span class=\"views\">120</span></td><td>February 1, 2013</td></tr></tbody></table>",
        );
        let page = read(&Document::from(html), "https://forum.example.org/latest").unwrap();
        assert!(!page.thread);
        assert_eq!(
            page.markdown,
            "# Open with \\| Example Forum\n\n\
             ## [Start here!](https://forum.example.org/t/start-here/1)\n\n\
             *News · 3 replies*\n\nWe are glad you’re here.\n"
        );
    }

    #[test]
    fn only_discourse_s_page_for_crawlers() {
        let html = topic(&post(1, "mikar", "05", "<p>Hi.</p>"));
        assert!(may_be(&html));
        // Не Discourse — не наше дело, как бы ни выглядела разметка.
        let other = html.replace("content=\"Discourse ", "content=\"WordPress ");
        assert!(!may_be(&other));
        assert!(read(&Document::from(other), TOPIC).is_none());
        // Страница для браузера со скриптами — не та форма.
        let app = html.replace("class=\"crawler \"", "class=\"\"");
        assert!(read(&Document::from(app), TOPIC).is_none());
        assert_eq!(
            crate::from_html(&html, TOPIC).unwrap().kind,
            crate::markdown::Kind::Article
        );
    }
}
