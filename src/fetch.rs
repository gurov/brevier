//! Загрузка страницы. Доверие к сертификатам делегировано ОС
//! (`rustls-platform-verifier`), своего root store в бинарнике нет —
//! проверяется одной командой: `cargo tree | grep webpki-roots` пусто.

use std::time::Duration;

use dom_query::Document;
use ureq::Agent;
use ureq::config::Config;
use ureq::http::response::Response;
use ureq::tls::{RootCerts, TlsConfig};
use ureq::{Body, ResponseExt};
use url::Url;

use crate::error::Error;
use crate::feed;

/// Потолок на тело ответа. Читалка, а не качалка: страница, не влезающая
/// в 8 МиБ, почти наверняка не статья.
pub const MAX_BODY: u64 = 8 * 1024 * 1024;

const TIMEOUT: Duration = Duration::from_secs(30);
const MAX_REDIRECTS: u32 = 10;

/// Сколько раз идём за `<meta http-equiv="refresh">`. Больше одного шага
/// в жизни почти не встречается, запас — от закольцованных страниц.
const MAX_META_REFRESH: u8 = 3;

/// Задержка, до которой считаем meta refresh редиректом, а не «страница
/// обновится через полминуты».
const META_REFRESH_MAX_DELAY: f64 = 5.0;

/// Markdown просим первым. Сайт, отдающий текст в markdown, отдаёт его точным
/// и без вёрстки вокруг — читать это лучше, чем извлекать из HTML. HTML идёт
/// следом, `q=0.9`: у кого markdown-двойника нет (почти у всех), тот вернёт
/// HTML как и прежде.
const ACCEPT: &str =
    "text/markdown, text/html;q=0.9, application/xhtml+xml;q=0.9, text/plain;q=0.8, */*;q=0.1";

/// Какой User-Agent отправляем — открытый вопрос из TODO, и он прямо двигает
/// число на гейте M0. Поэтому оба варианта живут в коде: корпус гоняется дважды,
/// а дельта между прогонами и есть цена честной политики.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum UserAgent {
    /// Как есть. Часть сайтов ответит 403 через Cloudflare — это и меряем.
    #[default]
    Honest,
    /// Маскировка под браузер: серая зона и гонка вооружений навсегда.
    Browser,
}

impl UserAgent {
    pub fn as_str(self) -> &'static str {
        match self {
            UserAgent::Honest => "Brevier/0.1",
            UserAgent::Browser => {
                "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 \
                 (KHTML, like Gecko) Chrome/140.0.0.0 Safari/537.36"
            }
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "honest" => Some(UserAgent::Honest),
            "browser" => Some(UserAgent::Browser),
            _ => None,
        }
    }
}

/// Что мы вообще соглашаемся читать. Всё остальное — чужой проект:
/// PDF, картинки, видео.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContentKind {
    Html,
    /// `text/markdown` (RFC 7763) — родной формат, конвертация не нужна.
    Markdown,
    Text,
    /// Лента RSS, Atom или JSON Feed. Отдают её под полудюжиной типов,
    /// а общие `application/xml` и `application/json` бывают чем угодно,
    /// поэтому решает начало тела (`feed::is_feed`), а тип — лишь повод
    /// посмотреть.
    Feed,
}

impl ContentKind {
    fn from_mime(mime: &str) -> Option<Self> {
        match mime {
            "text/html" | "application/xhtml+xml" => Some(ContentKind::Html),
            "text/markdown" | "text/x-markdown" => Some(ContentKind::Markdown),
            "text/plain" => Some(ContentKind::Text),
            "application/rss+xml"
            | "application/atom+xml"
            | "application/rdf+xml"
            | "application/xml"
            | "text/xml"
            // JSON Feed: свой тип есть, но чаще отдают просто JSON.
            | "application/feed+json"
            | "application/json" => Some(ContentKind::Feed),
            _ => None,
        }
    }
}

#[derive(Debug)]
pub struct Page {
    /// URL после редиректов. Относительные ссылки разрешаются от него,
    /// а не от того, что набрал пользователь.
    pub url: String,
    pub kind: ContentKind,
    pub body: String,
    /// Сколько раз адрес переехал, прежде чем отдал страницу: переходы HTTP
    /// и `<meta http-equiv=refresh>` вместе. Читателю это стоит времени,
    /// а `--check` говорит о длинной цепочке автору.
    pub redirects: u8,
}

/// Что скачали, кроме текста. Картинку разбирает `media`, а тракт загрузки —
/// тот же, что у страниц: те же таймауты, те же корни, тот же User-Agent.
#[derive(Debug)]
pub struct Blob {
    pub url: String,
    /// Тип, как его назвал сервер. Верить ему на слово нельзя, но знать полезно.
    pub mime: String,
    pub bytes: Vec<u8>,
}

pub fn fetch(url: &str, ua: UserAgent) -> Result<Page, Error> {
    let mut target = url.to_owned();
    let mut hops = 0;
    let mut redirects = 0u8;

    loop {
        let mut page = fetch_once(&target, ua)?;
        redirects = redirects.saturating_add(page.redirects);

        match meta_refresh(&page) {
            Some(next) if hops < MAX_META_REFRESH && next != page.url => {
                hops += 1;
                redirects = redirects.saturating_add(1);
                target = next;
            }
            // Бюджет кончился или редиректа нет — отдаём что есть.
            _ => {
                page.redirects = redirects;
                return Ok(page);
            }
        }
    }
}

/// Читаемое представление страницы: то же, что [`fetch`], но если пришёл HTML,
/// объявивший markdown-двойника (`<link rel="alternate" type="text/markdown">`),
/// берём двойника — точный текст автора, которому извлечение уже не нужно.
/// Второй запрос ради этого — оправданный размен: решение «Запросы меряются
/// разумом, а не счётчиком».
///
/// Адрес остаётся исходный: читатель открывал эту страницу, а не файл рядом
/// с ней. `--check`, `--raw`, `--html`, `--links` и `--nav` сюда не ходят —
/// им нужен сам HTML, а не его замена.
pub fn readable(url: &str, ua: UserAgent) -> Result<Page, Error> {
    // Читалка книги (#20): порцию текста берём отдельным запросом, как её
    // берёт скрипт сайта.
    if let Some(part) = crate::hosts::book_part(url) {
        return book(url, part, ua);
    }
    // Хост из таблицы (`hosts`): страницу читаем её лентой. Адрес остаётся
    // тем, что открывали, — как и у markdown-двойника.
    if let Some(feed) = crate::hosts::feed_for(url) {
        match fetch(&feed, ua) {
            Ok(page) if page.kind == ContentKind::Feed => {
                return Ok(Page {
                    url: url.to_owned(),
                    ..page
                });
            }
            // Лимит — ответ про сайт целиком: страница ответила бы тем же
            // или пустой заглушкой, а читателю честнее «подождите».
            Err(Error::HttpStatus(429)) => return Err(Error::HttpStatus(429)),
            // Ленты у страницы нет (тред удалён, адрес не тот) — идём
            // за самой страницей, как шли без таблицы.
            _ => {}
        }
    }
    let page = fetch(url, ua)?;
    let Some(alternate) = alternate_markdown(&page) else {
        return Ok(page);
    };

    match fetch(&alternate, ua) {
        // Двойник обязан быть markdown; вернул сервер иное — остаёмся при HTML
        // и извлекаем как обычно.
        Ok(md) if md.kind == ContentKind::Markdown => Ok(Page {
            url: page.url,
            kind: md.kind,
            body: md.body,
            redirects: page.redirects,
        }),
        _ => Ok(page),
    }
}

/// Порция книги из читалки (`hosts::Book`): страница читалки, а в ней
/// вместо первой порции — нужная. Первую сервер кладёт в страницу сам;
/// не положил (так было до конца сентября 2026) — берём и её. Номер за
/// последней порцией — последняя, как листает и сайт. Не читалка
/// (`#bid` нет) — страница идёт как есть, обычным трактом.
fn book(url: &str, part: usize, ua: UserAgent) -> Result<Page, Error> {
    let mut page = fetch(url, ua)?;
    if page.kind != ContentKind::Html {
        return Ok(page);
    }
    let Some(mut book) = crate::hosts::Book::of(&page.body) else {
        return Ok(page);
    };
    if book.parts() == 0
        && let Some(map) = book.map_url(&page.url)
    {
        book.set_map(&fetch(&map, ua)?.body);
    }
    let part = part.min(book.parts().saturating_sub(1));
    let text = match book.part_url(&page.url, part) {
        Some(text) if part > 0 || !book.served => Some(fetch(&text, ua)?.body),
        _ => None,
    };
    page.body = crate::hosts::book_page(&page.body, url, &book, part, text.as_deref());
    Ok(page)
}

/// Скачать что-то нетекстовое: картинку. Content-type не проверяем здесь —
/// это дело того, кто заказывал: `media` умеет отличить svg от png в байтах,
/// а сервера ошибаются в заголовке чаще, чем хотелось бы.
pub fn binary(url: &str, ua: UserAgent, accept: &str, limit: u64) -> Result<Blob, Error> {
    let parsed = target(url)?;

    let mut res: Response<Body> = agent(ua, accept).get(parsed.as_str()).call()?;
    let final_url = res.get_uri().to_string();
    let mime = res
        .body()
        .mime_type()
        .unwrap_or_default()
        .to_ascii_lowercase();
    let bytes = res.body_mut().with_config().limit(limit).read_to_vec()?;

    Ok(Blob {
        url: final_url,
        mime,
        bytes,
    })
}

fn fetch_once(url: &str, ua: UserAgent) -> Result<Page, Error> {
    let parsed = target(url)?;

    let mut res: Response<Body> = agent(ua, ACCEPT).get(parsed.as_str()).call()?;

    let final_url = res.get_uri().to_string();
    // В истории переходов первый адрес — сам запрос, поэтому переездов
    // на один меньше, чем адресов.
    let redirects = res
        .get_redirect_history()
        .map(|history| history.len().saturating_sub(1))
        .unwrap_or(0)
        .min(u8::MAX as usize) as u8;
    let mime = res
        .body()
        .mime_type()
        .unwrap_or("text/html")
        .to_ascii_lowercase();
    let kind = ContentKind::from_mime(&mime).ok_or(Error::UnsupportedContentType(mime.clone()))?;

    let (kind, body) = if kind == ContentKind::Feed {
        // XML читаем байтами: кодировку ленты чаще называет её собственное
        // объявление, чем заголовок ответа, а `read_to_string` знает только
        // заголовок.
        let charset = res.body().charset().map(str::to_owned);
        let bytes = res.body_mut().with_config().limit(MAX_BODY).read_to_vec()?;
        let body = feed::decode(&bytes, charset.as_deref());
        // `application/xml` оказался не лентой — это не текст для чтения.
        if !feed::is_feed(&body) {
            return Err(Error::UnsupportedContentType(mime));
        }
        (kind, body)
    } else {
        // read_to_string перекодирует из charset заголовка (фича `charset`):
        // cp1251 и прочий доюникодный веб никуда не делся.
        let body = res
            .body_mut()
            .with_config()
            .limit(MAX_BODY)
            .read_to_string()?;
        // Ленту отдают и `text/html`, и `text/plain` (сырые файлы хостингов):
        // по телу видно, что это она.
        let kind = if feed::is_feed(&body) {
            ContentKind::Feed
        } else {
            kind
        };
        (kind, body)
    };

    Ok(Page {
        url: final_url,
        kind,
        body,
        redirects,
    })
}

/// Что отправляем серверу. Схема проверяется здесь, и здесь же отрезается
/// решётка: якорь — дело читателя, серверу его не показывают, а в строке
/// запроса он ломает разбор адреса.
fn target(url: &str) -> Result<Url, Error> {
    let mut parsed = Url::parse(url).map_err(|_| Error::BadUrl(url.to_owned()))?;
    match parsed.scheme() {
        "http" | "https" => {}
        other => return Err(Error::UnsupportedScheme(other.to_owned())),
    }
    parsed.set_fragment(None);
    Ok(parsed)
}

/// Страница-редирект: `<meta http-equiv="refresh" content="0; url=...">`.
///
/// Так живут переехавшие адреса — blog.rust-lang.org отдаёт такую страницу
/// на старых ссылках. Без этого шага читатель получает «статью» из одной
/// строчки «Click here», причём с кодом 0: и корпус, и человек считают,
/// что всё хорошо.
fn meta_refresh(page: &Page) -> Option<String> {
    if page.kind != ContentKind::Html || !contains_ignore_case(&page.body, "http-equiv") {
        return None;
    }

    let document = Document::from(page.body.as_str());
    let base = Url::parse(&page.url).ok()?;

    for node in document.select("meta[http-equiv]").nodes() {
        let equiv = node.attr("http-equiv").unwrap_or_default();
        if !equiv.trim().eq_ignore_ascii_case("refresh") {
            continue;
        }
        let Some(content) = node.attr("content") else {
            continue;
        };
        if let Some(target) = parse_refresh(&content)
            && let Ok(absolute) = base.join(target)
        {
            return Some(absolute.to_string());
        }
    }
    None
}

/// markdown-двойник страницы: `<link rel="alternate" type="text/markdown" href>`
/// в шапке. Сайты начали отдавать точный текст в markdown, и это ровно то,
/// о чём просит манифест; если ссылка есть, [`readable`] по ней и идёт.
fn alternate_markdown(page: &Page) -> Option<String> {
    if page.kind != ContentKind::Html || !contains_ignore_case(&page.body, "alternate") {
        return None;
    }

    let document = Document::from(page.body.as_str());
    let base = Url::parse(&page.url).ok()?;

    for node in document.select("link[rel][type][href]").nodes() {
        let rel = node.attr("rel").unwrap_or_default();
        if !rel
            .split_whitespace()
            .any(|token| token.eq_ignore_ascii_case("alternate"))
        {
            continue;
        }
        let mime = node.attr("type").unwrap_or_default();
        if !mime.trim().eq_ignore_ascii_case("text/markdown") {
            continue;
        }
        let Some(href) = node.attr("href") else {
            continue;
        };
        if let Ok(absolute) = base.join(href.trim()) {
            return Some(absolute.to_string());
        }
    }
    None
}

/// `0; url=/new/place` → `/new/place`. Задержку длиннее
/// [`META_REFRESH_MAX_DELAY`] игнорируем: это не переезд, а автолистание.
fn parse_refresh(content: &str) -> Option<&str> {
    let (delay, rest) = content.split_once(';')?;
    let delay: f64 = delay.trim().parse().ok()?;
    if delay > META_REFRESH_MAX_DELAY {
        return None;
    }

    let rest = rest.trim_start();
    if !contains_ignore_case(rest, "url") {
        return None;
    }
    let value = rest.split_once('=')?.1.trim();
    let value = value.trim_matches(|c| c == '\'' || c == '"').trim();

    (!value.is_empty()).then_some(value)
}

fn contains_ignore_case(haystack: &str, needle: &str) -> bool {
    haystack
        .as_bytes()
        .windows(needle.len())
        .any(|window| window.eq_ignore_ascii_case(needle.as_bytes()))
}

fn agent(ua: UserAgent, accept: &str) -> Agent {
    let tls = TlsConfig::builder()
        // Никогда не трогать disable_verification, даже временно.
        .root_certs(RootCerts::PlatformVerifier)
        .build();

    Config::builder()
        .user_agent(ua.as_str())
        .accept(accept)
        .tls_config(tls)
        .max_redirects(MAX_REDIRECTS)
        // Цепочку переходов меряет `--check`; стоит это список адресов
        // на запрос, а не запрос.
        .save_redirect_history(true)
        .timeout_global(Some(TIMEOUT))
        .build()
        .new_agent()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mime_with_parameters_is_recognised() {
        // ureq отдаёт mime_type() уже без `; charset=...`, но регистр бывает любой.
        assert_eq!(ContentKind::from_mime("text/html"), Some(ContentKind::Html));
        assert_eq!(ContentKind::from_mime("application/pdf"), None);
    }

    fn html_page(body: &str) -> Page {
        Page {
            url: "https://e.com/old/page.html".to_owned(),
            kind: ContentKind::Html,
            body: body.to_owned(),
            redirects: 0,
        }
    }

    #[test]
    fn meta_refresh_is_followed_and_resolved() {
        let page = html_page(r#"<meta http-equiv="Refresh" content="0; url=/new/place">"#);
        assert_eq!(
            meta_refresh(&page).as_deref(),
            Some("https://e.com/new/place")
        );
    }

    #[test]
    fn quoted_and_spaced_refresh_targets() {
        let page = html_page(r#"<meta http-equiv="refresh" content="2 ; URL='https://x.org/a'">"#);
        assert_eq!(meta_refresh(&page).as_deref(), Some("https://x.org/a"));
    }

    #[test]
    fn slow_refresh_is_not_a_redirect() {
        // «Страница обновится через полминуты» — не переезд, читателю не мешает.
        let page = html_page(r#"<meta http-equiv="refresh" content="30; url=/loop">"#);
        assert_eq!(meta_refresh(&page), None);
    }

    #[test]
    fn other_meta_tags_are_left_alone() {
        let page = html_page(r#"<meta http-equiv="content-type" content="text/html"><p>текст</p>"#);
        assert_eq!(meta_refresh(&page), None);
    }

    #[test]
    fn alternate_markdown_link_is_found_and_resolved() {
        let page = html_page(r#"<link rel="alternate" type="text/markdown" href="page.md">"#);
        assert_eq!(
            alternate_markdown(&page).as_deref(),
            Some("https://e.com/old/page.md")
        );
    }

    #[test]
    fn a_non_markdown_alternate_is_ignored() {
        // Фид — тоже `alternate`, но не наш формат.
        let page = html_page(r#"<link rel="alternate" type="application/rss+xml" href="/f.xml">"#);
        assert_eq!(alternate_markdown(&page), None);
    }

    #[test]
    fn the_fragment_never_reaches_the_server() {
        let asked = target("https://e.com/a/b?q=1#place").unwrap();
        assert_eq!(asked.as_str(), "https://e.com/a/b?q=1");
    }

    #[test]
    fn non_http_schemes_are_refused() {
        let err = fetch("gemini://example.com/", UserAgent::Honest).unwrap_err();
        assert!(matches!(err, Error::UnsupportedScheme(s) if s == "gemini"));
    }
}
