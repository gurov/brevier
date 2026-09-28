//! Правила под хост — одной небольшой таблицей.
//!
//! Решено роадмапом («Per-host rules exist, in one small table»): для
//! открытого веба правила пишутся по форме, а треды и хосты документации
//! получают именованные правила, у каждого — страницы в проверке. Системы
//! плагинов вокруг нет и не будет: строка таблицы — функция здесь.
//!
//! Сейчас в таблице reddit и поисковик. Страницы reddit без JavaScript пусты
//! (8 КБ заглушки), `old.reddit.com` уводит на вход, `.json` отвечает 403 —
//! а лента `.rss` у сабреддита и у треда открыта любому клиенту, с автором
//! и полным текстом каждой реплики. Цена — вложенности ответов в ленте нет,
//! и жёсткий лимит: второй запрос за несколько секунд получает 429.
//!
//! Поисковик — DuckDuckGo Lite (#25): выдача простым HTML по GET, без
//! скриптов и без ключа. Из проверенных 28 сентября 2026 он один отдаёт
//! хорошую выдачу, в том числе по-русски: Google требует JavaScript,
//! обычный DuckDuckGo — POST, у Bing ссылки завёрнуты в свой счётчик,
//! а у JSON-поисковиков либо ключ с картой, либо маленький индекс.
//! Выдача — таблица, и обычным трактом десять результатов слипаются в одну
//! строку, поэтому её разбирает правило ниже.

use dom_query::Document;
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

/// Где искать: DuckDuckGo Lite. Страница выдачи — по GET, запрос в `q`.
const SEARCH: &str = "https://lite.duckduckgo.com/lite/";

/// Адрес выдачи по запросу.
pub fn search_url(query: &str) -> String {
    let mut url = Url::parse(SEARCH).expect("адрес поисковика разбирается");
    url.query_pairs_mut().append_pair("q", query.trim());
    url.into()
}

/// Запрос, если адрес — выдача нашего поисковика. По нему же страница
/// получает заголовок, и выдача узнаётся, откуда бы адрес ни пришёл:
/// из адресной строки, истории или сессии.
pub fn search_query(url: &str) -> Option<String> {
    let parsed = Url::parse(url).ok()?;
    let ours = Url::parse(SEARCH).ok()?;
    if parsed.host_str()? != ours.host_str()? || parsed.path().trim_end_matches('/') != "/lite" {
        return None;
    }
    parsed
        .query_pairs()
        .find(|(key, _)| key == "q")
        .map(|(_, value)| value.trim().to_owned())
        .filter(|query| !query.is_empty())
}

/// Один результат выдачи.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hit {
    pub title: String,
    /// Адрес самой страницы, уже без счётчика поисковика.
    pub url: String,
    /// Подводка — как её дал поисковик, без выделения слов запроса.
    pub snippet: String,
    /// Адрес, как его показывает выдача: хост и путь, без схемы.
    pub shown: String,
    /// Дата, если выдача её знает (`2026-08-04`).
    pub date: Option<String>,
}

/// Что пришло в ответ на запрос.
#[derive(Debug, PartialEq, Eq)]
pub enum Results {
    Hits(Vec<Hit>),
    /// Поисковик ничего не нашёл.
    Nothing,
    /// Поисковик принял нас за бота и показал загадку. Решать её мы
    /// не станем: это работа для браузера читателя.
    Challenge,
}

/// Разобрать страницу выдачи DuckDuckGo Lite.
///
/// Результат — строка таблицы со ссылкой `a.result-link`, за ней строки
/// с подводкой (`td.result-snippet`), показанным адресом (`span.link-text`)
/// и иногда датой (`span.timestamp`) — до следующей ссылки. Реклама ведёт
/// не через `uddg`, а через `y.js`, и отсеивается на развороте ссылки.
pub fn results(html: &str) -> Results {
    let doc = Document::from(html);
    let mut hits = Vec::new();
    for link in doc.select("a.result-link").nodes() {
        let Some(url) = link.attr("href").and_then(|href| unwrap_redirect(&href)) else {
            continue;
        };
        let mut hit = Hit {
            title: squeeze(&link.text()),
            url,
            snippet: String::new(),
            shown: String::new(),
            date: None,
        };
        let row = std::iter::successors(link.parent(), |node| node.parent())
            .find(|node| node.node_name().as_deref() == Some("tr"));
        let mut next = row.and_then(|row| row.next_element_sibling());
        while let Some(tr) = next {
            let has = |selector: &str| {
                tr.descendants()
                    .into_iter()
                    .find(|node| node.is(selector))
                    .map(|node| squeeze(&node.text()))
            };
            if has("a.result-link").is_some() {
                break;
            }
            if let Some(snippet) = has("td.result-snippet") {
                hit.snippet = snippet;
            }
            if let Some(shown) = has("span.link-text") {
                hit.shown = shown;
            }
            if let Some(date) = has("span.timestamp") {
                hit.date = date.get(..10).map(str::to_owned);
            }
            next = tr.next_element_sibling();
        }
        if !hit.title.is_empty() {
            hits.push(hit);
        }
    }
    if !hits.is_empty() {
        Results::Hits(hits)
    } else if doc
        .select(".anomaly-modal__modal, .anomaly-modal__puzzle")
        .exists()
    {
        Results::Challenge
    } else {
        Results::Nothing
    }
}

/// Адрес страницы из ссылки выдачи: `//duckduckgo.com/l/?uddg=<адрес>` —
/// счётчик переходов, настоящий адрес в `uddg`. Разворачиваем сами, чтобы
/// подсказка у ссылки и история показывали сайт, а не счётчик. Ссылка
/// рекламы (`y.js`) адреса в `uddg` не несёт — её отбрасываем.
fn unwrap_redirect(href: &str) -> Option<String> {
    let href = href.trim();
    let absolute = match href.strip_prefix("//") {
        Some(rest) => format!("https://{rest}"),
        None => href.to_owned(),
    };
    let parsed = Url::parse(&absolute).ok()?;
    let target = if parsed
        .host_str()
        .is_some_and(|host| host.ends_with("duckduckgo.com"))
    {
        parsed
            .query_pairs()
            .find(|(key, _)| key == "uddg")
            .map(|(_, value)| value.into_owned())?
    } else {
        absolute
    };
    let target_url = Url::parse(&target).ok()?;
    matches!(target_url.scheme(), "http" | "https").then_some(target)
}

fn squeeze(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Выдача markdown-ом — тот же вид, что у ленты: заголовок — запрос,
/// результат — заголовок второго уровня ссылкой (полка показывает их
/// оглавлением), под ним хост и дата курсивом, дальше подводка. Хост,
/// а не весь адрес: адрес видно в подсказке у ссылки, а строка под
/// заголовком нужна, чтобы узнать сайт с одного взгляда.
pub fn search_markdown(query: &str, hits: &[Hit]) -> String {
    use crate::feed::{block, destination, heading, inline};

    let mut out = format!("# {}\n\n", heading(query));
    if hits.is_empty() {
        out.push_str("Nothing found. Try other words.\n");
        return out;
    }
    out.push_str("*Results from DuckDuckGo*\n\n");
    for hit in hits {
        out.push_str(&format!(
            "## [{}]({})\n\n",
            heading(&hit.title),
            destination(&hit.url)
        ));
        let host = Url::parse(&hit.url)
            .ok()
            .and_then(|url| {
                url.host_str()
                    .map(|host| host.trim_start_matches("www.").to_owned())
            })
            .unwrap_or_else(|| hit.shown.clone());
        let meta: Vec<String> = [Some(host), hit.date.clone()]
            .into_iter()
            .flatten()
            .filter(|text| !text.is_empty())
            .map(|text| inline(&text))
            .collect();
        if !meta.is_empty() {
            out.push_str(&format!("*{}*\n\n", meta.join(" · ")));
        }
        if !hit.snippet.is_empty() {
            out.push_str(&block(&hit.snippet));
            out.push_str("\n\n");
        }
    }
    out.truncate(out.trim_end().len());
    out.push('\n');
    out
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

    #[test]
    fn a_query_becomes_the_address_of_the_results_and_back() {
        let url = search_url(" как работает borrow checker ");
        assert_eq!(
            url,
            "https://lite.duckduckgo.com/lite/?q=%D0%BA%D0%B0%D0%BA+%D1%80%D0%B0%D0%B1%D0%BE%D1%82%D0%B0%D0%B5%D1%82+borrow+checker"
        );
        assert_eq!(
            search_query(&url).as_deref(),
            Some("как работает borrow checker")
        );
        assert_eq!(search_query("https://lite.duckduckgo.com/lite/"), None);
        assert_eq!(search_query("https://duckduckgo.com/?q=rust"), None);
        assert_eq!(search_query("https://example.org/lite/?q=rust"), None);
    }

    /// Фикстура — настоящая выдача 28 сентября 2026: поменяет DuckDuckGo
    /// разметку — упадёт этот тест, а не чтение.
    #[test]
    fn duckduckgo_lite_results_become_hits() {
        let Results::Hits(hits) = results(include_str!("../tests/fixtures/ddg-lite-results.html"))
        else {
            panic!("выдача не разобралась");
        };
        assert_eq!(hits.len(), 10);
        assert_eq!(
            hits[0],
            Hit {
                title: "Borrowing - Rust By Example".to_owned(),
                url: "https://doc.rust-lang.org/beta/rust-by-example/scope/borrow.html".to_owned(),
                snippet: "Borrowing Most of the time, we'd like to access data without taking \
                    ownership over it. To accomplish this, Rust uses a borrowing mechanism. \
                    Instead of passing objects by value (T), objects can be passed by reference \
                    (&T). The compiler statically guarantees (via its borrow checker) that \
                    references always point to valid objects."
                    .to_owned(),
                shown: "doc.rust-lang.org/beta/rust-by-example/scope/borrow.html".to_owned(),
                date: None,
            }
        );
        // Ни одного адреса счётчика: все ссылки ведут на сами сайты.
        assert!(hits.iter().all(|hit| !hit.url.contains("duckduckgo.com")));
        let last = hits.last().unwrap();
        assert_eq!(last.date.as_deref(), Some("2026-08-04"));
    }

    #[test]
    fn a_duckduckgo_puzzle_is_a_challenge_not_an_empty_list() {
        assert_eq!(
            results(include_str!("../tests/fixtures/ddg-lite-challenge.html")),
            Results::Challenge
        );
        assert_eq!(
            results("<html><body><table></table></body></html>"),
            Results::Nothing
        );
    }

    #[test]
    fn results_read_as_a_list_of_links() {
        let hits = vec![Hit {
            title: "Rust [book]".to_owned(),
            url: "https://www.example.org/a".to_owned(),
            snippet: "1. Borrowing *is* explained".to_owned(),
            shown: "example.org/a".to_owned(),
            date: Some("2026-08-04".to_owned()),
        }];
        assert_eq!(
            search_markdown("borrow checker", &hits),
            "# borrow checker\n\n*Results from DuckDuckGo*\n\n\
             ## [Rust \\[book\\]](https://www.example.org/a)\n\n\
             *example.org · 2026-08-04*\n\n\
             1\\. Borrowing \\*is\\* explained\n"
        );
        assert_eq!(
            search_markdown("qzx", &[]),
            "# qzx\n\nNothing found. Try other words.\n"
        );
    }

    #[test]
    fn an_ad_is_not_a_result() {
        assert_eq!(
            unwrap_redirect(
                "//duckduckgo.com/y.js?ad_domain=example.com&u3=https%3A%2F%2Fad.example"
            ),
            None
        );
        assert_eq!(
            unwrap_redirect("//duckduckgo.com/l/?uddg=https%3A%2F%2Fexample.org%2Fa&rut=1"),
            Some("https://example.org/a".to_owned())
        );
        assert_eq!(
            unwrap_redirect("https://example.org/direct"),
            Some("https://example.org/direct".to_owned())
        );
        assert_eq!(unwrap_redirect("javascript:void(0)"), None);
    }
}
