//! Lobsters (#11): тред и списки историй — своим правилом, мимо Readability.
//!
//! Общий тракт на треде Lobsters берёт за статью одну реплику, самую
//! длинную, а остальной тред теряет целиком. Разметка у Lobsters смысловая,
//! и разбирать её просто: история — `li.story` в `ol.stories`, реплика —
//! `div.comment` с `.byline` и `.comment_text`, ответы — вложенные
//! `ol.comments`.
//!
//! Признак — хост и форма. Одна история с репликами (`#story_comments`) или
//! с текстом (`.story_content`) — тред; истории списком — список историй;
//! реплики без историй — список реплик (`/comments`, реплики автора).
//! Ссылка на реплику (`/c/<id>`) уводит на тред с решёткой: якоря у реплики
//! в markdown нет, и тред открывается сначала.

use dom_query::{Document, NodeRef};
use url::Url;

use super::thread::{
    Page, Reply, absolute, all, count_heading, first, squeeze, title_of, write_replies,
};
use crate::extract::Link;
use crate::feed::{date_text, destination, heading, inline, reply_markdown};

const HOST: &str = "lobste.rs";

/// Адрес на Lobsters. Какая это страница, решает форма (см. [`read`]).
pub fn is_lobsters(url: &str) -> bool {
    Url::parse(url)
        .ok()
        .and_then(|url| url.host_str().map(|host| host.eq_ignore_ascii_case(HOST)))
        .unwrap_or(false)
}

/// Тред, список историй или список реплик — или `None`, если страница
/// не той формы и её читает общий тракт.
pub fn read(doc: &Document, url: &str) -> Option<Page> {
    if !is_lobsters(url) {
        return None;
    }
    let base = Url::parse(url).ok()?;
    let root = doc.root();
    let stories = all(&root, "ol.stories li.story");
    let discussed =
        first(&root, "#story_comments").is_some() || first(&root, ".story_content").is_some();
    let mut page = match stories.as_slice() {
        [story] if discussed => thread(&root, story, &base),
        [] => comments(&root, &base),
        list => listing(&root, list, &base),
    }?;
    page.site = Some(site(&root, &base));
    Some(page)
}

/// Навигация сайта для полки: главная, меню в шапке, меню в подвале.
/// Без входа и поиска: вход для вошедших, а поиск — форма, которую
/// Brevier не отправляет. Общее правило брало их, и ещё «Page 2».
fn site(root: &NodeRef, base: &Url) -> Vec<Link> {
    let mut links = vec![Link {
        title: "Lobsters".to_owned(),
        address: format!("https://{HOST}/"),
    }];
    let menu = all(root, "header nav li:not(.push-right) a");
    let footer = all(root, "footer nav a");
    for link in menu.iter().chain(&footer) {
        let title = squeeze(&link.text());
        let Some(address) = link.attr("href").and_then(|href| absolute(base, &href)) else {
            continue;
        };
        let own = Url::parse(&address)
            .ok()
            .is_some_and(|url| url.host_str() == Some(HOST) && url.path() != "/search");
        if own && !title.is_empty() && !links.iter().any(|seen| seen.address == address) {
            links.push(Link { title, address });
        }
    }
    links
}

/// История в списке и в шапке треда.
struct Story {
    title: String,
    link: Option<String>,
    domain: Option<String>,
    tags: Vec<String>,
    votes: Option<String>,
    author: Option<String>,
    date: Option<String>,
    /// Подпись ссылки на обсуждение и адрес обсуждения.
    comments: Option<(String, String)>,
}

impl Story {
    fn of(story: &NodeRef, base: &Url) -> Option<Story> {
        let title_link = first(story, "a.u-url")?;
        let title = squeeze(&title_link.text());
        if title.is_empty() {
            return None;
        }
        let byline = first(story, ".byline");
        let text_of = |node: Option<NodeRef>| {
            node.map(|node| squeeze(&node.text()))
                .filter(|text| !text.is_empty())
        };
        // Счёт скрыт у свежих («~») — тогда его нет вовсе.
        let votes = text_of(first(story, ".voters .upvoter"))
            .filter(|votes| votes.chars().all(|c| c.is_ascii_digit()))
            .map(|votes| match votes.as_str() {
                "1" => "1 vote".to_owned(),
                votes => format!("{votes} votes"),
            });
        let comments = first(story, ".comments_label a").and_then(|link| {
            let label = squeeze(&link.text());
            let address = link.attr("href").and_then(|href| absolute(base, &href))?;
            let label = if label.starts_with(|c: char| c.is_ascii_digit()) {
                label
            } else {
                "no comments yet".to_owned()
            };
            Some((label, address))
        });
        Some(Story {
            title,
            link: title_link
                .attr("href")
                .and_then(|href| absolute(base, &href)),
            domain: text_of(first(story, "a.domain")),
            tags: all(story, "ul.tags a")
                .iter()
                .map(|tag| squeeze(&tag.text()))
                .filter(|tag| !tag.is_empty())
                .collect(),
            votes,
            author: byline
                .as_ref()
                .and_then(|byline| text_of(first(byline, "a.u-author"))),
            date: byline.as_ref().and_then(time),
            comments,
        })
    }

    /// Строка курсивом: сайт, метки, голоса, автор, день.
    fn meta(&self) -> Vec<String> {
        let tags = (!self.tags.is_empty()).then(|| self.tags.join(", "));
        [&self.domain, &tags, &self.votes, &self.author, &self.date]
            .into_iter()
            .flatten()
            .map(|text| inline(text))
            .collect()
    }
}

/// Список историй: главная, свежие, метка, домен. Вид — как у ленты:
/// история — заголовок второго уровня ссылкой, под ним строка курсивом
/// со ссылкой на обсуждение.
fn listing(root: &NodeRef, rows: &[NodeRef], base: &Url) -> Option<Page> {
    let list: Vec<Story> = rows.iter().filter_map(|row| Story::of(row, base)).collect();
    if list.is_empty() {
        return None;
    }
    let title = title_of(root).unwrap_or_else(|| "Lobsters".to_owned());
    let mut out = format!("# {}\n\n", heading(&title));
    for story in &list {
        let link = story
            .link
            .clone()
            .or_else(|| story.comments.as_ref().map(|(_, address)| address.clone()));
        match link {
            Some(link) => out.push_str(&format!(
                "## [{}]({})\n\n",
                heading(&story.title),
                destination(&link)
            )),
            None => out.push_str(&format!("## {}\n\n", heading(&story.title))),
        }
        let mut meta = story.meta();
        if let Some((label, address)) = &story.comments {
            meta.push(format!("[{}]({})", inline(label), destination(address)));
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
        site: None,
    })
}

/// Тред: история — заголовок ссылкой туда, куда она ведёт (у текстовой —
/// без ссылки: она вела бы сюда же), строка курсивом, текст, если есть, —
/// и дерево ответов.
fn thread(root: &NodeRef, row: &NodeRef, base: &Url) -> Option<Page> {
    let story = Story::of(row, base)?;
    let here = |link: &str| {
        Url::parse(link)
            .ok()
            .is_some_and(|link| link.host_str() == base.host_str() && link.path() == base.path())
    };
    let mut out = match &story.link {
        Some(link) if !here(link) => {
            format!("# [{}]({})\n\n", heading(&story.title), destination(link))
        }
        _ => format!("# {}\n\n", heading(&story.title)),
    };
    let meta = story.meta();
    if !meta.is_empty() {
        out.push_str(&format!("*{}*\n\n", meta.join(" · ")));
    }
    if let Some(text) = first(root, ".story_content .story_text")
        && let Some(body) = reply_markdown(&text.inner_html(), base.as_str())
    {
        out.push_str(&body);
        out.push_str("\n\n");
    }
    let replies = replies(root, base);
    out.push_str(&count_heading(replies.len(), "comment"));
    write_replies(&replies, &mut out);
    Some(Page {
        title: story.title,
        markdown: out,
        thread: true,
        site: None,
    })
}

/// Список реплик без истории: свежие (`/comments`), реплики автора.
/// У каждой — история, к которой она.
fn comments(root: &NodeRef, base: &Url) -> Option<Page> {
    let replies = replies(root, base);
    if replies.is_empty() {
        return None;
    }
    let title = title_of(root).unwrap_or_else(|| "Lobsters".to_owned());
    let mut out = format!("# {}\n\n", heading(&title));
    write_replies(&replies, &mut out);
    Some(Page {
        title,
        markdown: out,
        thread: false,
        site: None,
    })
}

fn replies(root: &NodeRef, base: &Url) -> Vec<Reply> {
    all(root, "div.comment")
        .iter()
        .filter(|comment| first(comment, ".byline").is_some())
        .map(|comment| reply(comment, base))
        .collect()
}

fn reply(comment: &NodeRef, base: &Url) -> Reply {
    let byline = first(comment, ".byline");
    let links = byline
        .as_ref()
        .map(|byline| all(byline, "a"))
        .unwrap_or_default();
    let href = |link: &NodeRef| link.attr("href").map(|href| href.to_string());
    // Аватар — тоже ссылка на автора, но скрытая от дикторов; имя — вторая.
    let author = links
        .iter()
        .filter(|link| link.attr("aria-hidden").is_none())
        .find(|link| href(link).is_some_and(|href| href.starts_with("/~")))
        .map(|link| squeeze(&link.text()))
        .filter(|name| !name.is_empty());
    let link = links
        .iter()
        .filter_map(href)
        .find(|href| href.starts_with("/c/"))
        .and_then(|href| absolute(base, &href));
    let on = links
        .iter()
        .find(|link| href(link).is_some_and(|href| href.starts_with("/s/")))
        .map(|link| {
            (
                squeeze(&link.text()),
                href(link).and_then(|href| absolute(base, &href)),
            )
        })
        .filter(|(title, _)| !title.is_empty());
    Reply {
        depth: depth(comment),
        author,
        date: byline.as_ref().and_then(time),
        link,
        on,
        body: first(comment, ".comment_text")
            .and_then(|text| reply_markdown(&text.inner_html(), base.as_str())),
    }
}

/// Уровень реплики: сколько реплик над ней. Реплика лежит в своём `li`,
/// ответы на неё — во вложенном `ol` того же `li`; значит, предки —
/// это те `li` выше своего, у которых есть свой `div.comment`.
fn depth(comment: &NodeRef) -> usize {
    comment
        .ancestors_it(None)
        .filter(|node| node.node_name().as_deref() == Some("li"))
        .skip(1)
        .filter(|li| {
            li.element_children()
                .iter()
                .any(|child| child.is("div.comment"))
        })
        .count()
}

/// День из `<time datetime>` — «2026-10-12 09:49:36», время UTC.
fn time(node: &NodeRef) -> Option<String> {
    let stamp = first(node, "time")?.attr("datetime")?;
    Some(date_text(&stamp))
}

#[cfg(test)]
mod tests {
    use super::*;

    const THREAD: &str = "https://lobste.rs/s/abc123/a_story";

    fn page(title: &str, main: &str) -> String {
        format!(
            "<html lang=\"en\"><head><title>{title}</title></head><body>\
             <header id=\"header\"><h1 id=\"logo\"><a href=\"/\" title=\"Lobsters\"></a></h1>\
             <nav aria-label=\"Menu\"><ul><li><a href=\"/active\">Active</a></li>\
             <li><a href=\"/search\">Search</a></li>\
             <li class=\"push-right\"><a href=\"/login\">Login</a></li></ul></nav></header>\
             <main id=\"inside\">{main}</main>\
             <footer><nav><a href=\"/about\">About</a></nav></footer></body></html>"
        )
    }

    /// История: голоса, ссылка, метки, сайт, автор, время, обсуждение.
    fn story(id: &str, title: &str, href: &str, domain: Option<&str>, comments: &str) -> String {
        let domain = domain
            .map(|domain| format!("<a class=\"domain\" href=\"/domains/{domain}\">{domain}</a>"))
            .unwrap_or_default();
        format!(
            "<li id=\"story_{id}\" class=\"story\"><div class=\"story_liner h-entry\">\
             <div class=\"voters\"><a class=\"upvoter\" href=\"/login\">61</a></div>\
             <div class=\"details\"><span role=\"heading\" class=\"link h-cite u-repost-of\">\
             <a class=\"u-url\" href=\"{href}\">{title}</a></span>\
             <ul class=\"tags\"><li><a class=\"tag\" href=\"/t/linux\">linux</a></li>\
             <li><a class=\"tag\" href=\"/t/ask\">ask</a></li></ul>{domain}\
             <div class=\"byline\"><a tabindex=\"-1\" aria-hidden=\"true\" href=\"/~ada\"><img class=\"avatar\" src=\"/avatars/ada-16.png\"></a>\
             <span>via</span> <a class=\"u-author h-card\" href=\"/~ada\">ada</a> \
             <time title=\"2026-10-12 05:11:29\" datetime=\"2026-10-12 05:11:29\">2026-10-12 05:11:29</time>\
             <details class=\"caches\"><summary>caches</summary><ul><li><a href=\"https://web.archive.org/web/3/x\">Archive.org</a></li></ul></details>\
             <span class=\"comments_label\"><a href=\"/s/{id}/a_story\">{comments}</a></span>\
             </div></div></div></li>"
        )
    }

    /// Реплика с ответами внутри своего `li`.
    fn comment(id: &str, who: &str, text: &str, replies: &str) -> String {
        format!(
            "<li class=\"comments_subtree\"><input id=\"comment_folder_{id}\" class=\"comment_folder_button\" type=\"checkbox\">\
             <div id=\"c_{id}\" data-shortid=\"{id}\" class=\"comment\">\
             <div class=\"voters\"><a class=\"upvoter\" href=\"/login\">5</a></div>\
             <div class=\"details\"><div class=\"byline\"><a name=\"c_{id}\"></a><span>\
             <a tabindex=\"-1\" aria-hidden=\"true\" href=\"/~{who}\"><img class=\"avatar\" src=\"/avatars/{who}-16.png\"></a>\
             <a href=\"/~{who}\">{who}</a> \
             <a href=\"/c/{id}\"><time datetime=\"2026-10-12 09:49:36\">2026-10-12 09:49:36</time></a></span>\
             <span class=\"flagger flagger_stub\"></span></div>\
             <div role=\"heading\" class=\"comment_text\">{text}</div></div></div>\
             <ol class=\"comments\">{replies}</ol></li>"
        )
    }

    fn thread_html(text: &str) -> String {
        let tree = comment(
            "a1",
            "bmo",
            "<p>Read the <a href=\"https://example.com/patch\">patch</a>.</p><blockquote><p>No reference.</p></blockquote>",
            &comment(
                "b2",
                "simonw",
                "<p>Gross.</p>",
                &comment(
                    "c3",
                    "cy",
                    "<p>Third.</p>",
                    &comment(
                        "d4",
                        "dee",
                        "<p>Fourth.</p>",
                        &comment("e5", "eve", "<p>Fifth.</p>", ""),
                    ),
                ),
            ),
        ) + &comment(
            "f6",
            "fay",
            "<p><span class=\"na\">[Comment removed by author]</span></p>",
            "",
        );
        page(
            "A story | Lobsters",
            &format!(
                "<ol class=\"stories\">{}</ol><div class=\"story_content\">{text}</div>\
                 <ol class=\"comments comments1\"><li class=\"comments_subtree\">\
                 <div class=\"comment_form_container\"><form><textarea placeholder=\"You must be logged in\"></textarea></form></div></li>\
                 <li class=\"comments_subtree\" id=\"comments-abc123\"><ol class=\"comments\" id=\"story_comments\">{tree}</ol></li></ol>",
                story(
                    "abc123",
                    "Linus and Claude",
                    "https://www.phoronix.com/news/x",
                    Some("phoronix.com"),
                    "6 comments"
                )
            ),
        )
    }

    fn read_page(html: &str, url: &str) -> Option<Page> {
        read(&Document::from(html), url)
    }

    #[test]
    fn a_thread_keeps_every_comment_with_its_author_and_day() {
        let page = read_page(&thread_html(""), THREAD).unwrap();
        assert!(page.thread);
        assert_eq!(page.title, "Linus and Claude");
        let md = &page.markdown;
        assert!(md.starts_with(
            "# [Linus and Claude](https://www.phoronix.com/news/x)\n\n\
             *phoronix.com · linux, ask · 61 votes · ada · 12 October 2026*\n\n## 6 comments\n\n\
             **bmo** · [12 October 2026](https://lobste.rs/c/a1)\n\n\
             Read the [patch](https://example.com/patch).\n\n> No reference.\n\n\
             > **simonw** · [12 October 2026](https://lobste.rs/c/b2)\n>\n> Gross.\n\n"
        ));
        assert!(md.contains("> > > **dee** · [12 October 2026]"));
        assert!(md.contains(
            "> > > **eve** · replying to dee · [12 October 2026](https://lobste.rs/c/e5)"
        ));
        // Удалённая реплика держит своё место в дереве.
        assert!(md.ends_with(
            "**fay** · [12 October 2026](https://lobste.rs/c/f6)\n\n\\[Comment removed by author\\]\n"
        ));
        // Форма ответа и копии в архивах читателю не нужны.
        assert!(!md.contains("logged in"));
        assert!(!md.contains("Archive.org"));
    }

    #[test]
    fn a_text_story_has_its_text_and_no_link_to_itself() {
        let html =
            thread_html("<div class=\"story_text\"><p>What are you doing this week?</p></div>")
                .replace("https://www.phoronix.com/news/x", "/s/abc123/a_story");
        let md = read_page(&html, THREAD).unwrap().markdown;
        assert!(md.starts_with(
            "# Linus and Claude\n\n*phoronix.com · linux, ask · 61 votes · ada · 12 October 2026*\n\n\
             What are you doing this week?\n\n## 6 comments\n\n"
        ));
    }

    #[test]
    fn a_list_of_stories_reads_as_a_list_of_links() {
        let html = page(
            "Lobsters",
            &format!(
                "<ol class=\"stories list\">{}{}</ol><div class=\"morelink\"><a rel=\"next\" href=\"/page/2\">Page 2 &gt;&gt;</a></div>",
                story(
                    "aa",
                    "A plea",
                    "https://blogs.example.org/plea/",
                    Some("blogs.example.org"),
                    "3 comments"
                ),
                story("bb", "Ask: tools?", "/s/bb/a_story", None, "no comments")
                    .replace(">61<", ">~<")
            ),
        );
        let page = read_page(&html, "https://lobste.rs/").unwrap();
        assert!(!page.thread);
        assert_eq!(
            page.markdown,
            "# Lobsters\n\n\
             ## [A plea](https://blogs.example.org/plea/)\n\n\
             *blogs.example.org · linux, ask · 61 votes · ada · 12 October 2026 · [3 comments](https://lobste.rs/s/aa/a_story)*\n\n\
             ## [Ask: tools?](https://lobste.rs/s/bb/a_story)\n\n\
             *linux, ask · ada · 12 October 2026 · [no comments yet](https://lobste.rs/s/bb/a_story)*\n"
        );
        let document = crate::from_html(&html, "https://lobste.rs/").unwrap();
        assert_eq!(
            document.next.map(|next| next.address),
            Some("https://lobste.rs/page/2".to_owned())
        );
    }

    #[test]
    fn a_list_of_comments_says_what_each_is_about() {
        let html = page(
            "Newest Comments | Lobsters",
            &format!(
                "<ol class=\"comments comments1\">{}</ol>",
                comment("r1", "lin", "<p>Agreed.</p>", "").replace(
                    "<span class=\"flagger flagger_stub\"></span>",
                    "| on: <a href=\"/s/xyz/reverts\">The reverts will continue</a>"
                )
            ),
        );
        let page = read_page(&html, "https://lobste.rs/comments").unwrap();
        assert!(!page.thread);
        assert_eq!(
            page.markdown,
            "# Newest Comments \\| Lobsters\n\n\
             **lin** · [12 October 2026](https://lobste.rs/c/r1) · \
             on [The reverts will continue](https://lobste.rs/s/xyz/reverts)\n\nAgreed.\n"
        );
    }

    #[test]
    fn the_shelf_gets_the_menu_without_login_or_search() {
        let page = read_page(&thread_html(""), THREAD).unwrap();
        let titles: Vec<String> = page
            .site
            .unwrap()
            .into_iter()
            .map(|link| link.title)
            .collect();
        assert_eq!(titles, ["Lobsters", "Active", "About"]);
        assert!(read_page(&thread_html(""), "https://example.com/s/abc123").is_none());
    }
}
