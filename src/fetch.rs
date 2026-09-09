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

const ACCEPT: &str =
    "text/html, application/xhtml+xml, text/markdown;q=0.9, text/plain;q=0.8, */*;q=0.1";

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
}

impl ContentKind {
    fn from_mime(mime: &str) -> Option<Self> {
        match mime {
            "text/html" | "application/xhtml+xml" => Some(ContentKind::Html),
            "text/markdown" | "text/x-markdown" => Some(ContentKind::Markdown),
            "text/plain" => Some(ContentKind::Text),
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
}

pub fn fetch(url: &str, ua: UserAgent) -> Result<Page, Error> {
    let mut target = url.to_owned();
    let mut hops = 0;

    loop {
        let page = fetch_once(&target, ua)?;

        match meta_refresh(&page) {
            Some(next) if hops < MAX_META_REFRESH && next != page.url => {
                hops += 1;
                target = next;
            }
            // Бюджет кончился или редиректа нет — отдаём что есть.
            _ => return Ok(page),
        }
    }
}

fn fetch_once(url: &str, ua: UserAgent) -> Result<Page, Error> {
    let parsed = Url::parse(url).map_err(|_| Error::BadUrl(url.to_owned()))?;
    match parsed.scheme() {
        "http" | "https" => {}
        other => return Err(Error::UnsupportedScheme(other.to_owned())),
    }

    let mut res: Response<Body> = agent(ua).get(parsed.as_str()).call()?;

    let final_url = res.get_uri().to_string();
    let mime = res
        .body()
        .mime_type()
        .unwrap_or("text/html")
        .to_ascii_lowercase();
    let kind = ContentKind::from_mime(&mime).ok_or(Error::UnsupportedContentType(mime))?;

    // read_to_string перекодирует из charset заголовка (фича `charset`):
    // cp1251 и прочий доюникодный веб никуда не делся.
    let body = res
        .body_mut()
        .with_config()
        .limit(MAX_BODY)
        .read_to_string()?;

    Ok(Page {
        url: final_url,
        kind,
        body,
    })
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

fn agent(ua: UserAgent) -> Agent {
    let tls = TlsConfig::builder()
        // Никогда не трогать disable_verification, даже временно.
        .root_certs(RootCerts::PlatformVerifier)
        .build();

    Config::builder()
        .user_agent(ua.as_str())
        .accept(ACCEPT)
        .tls_config(tls)
        .max_redirects(MAX_REDIRECTS)
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
    fn non_http_schemes_are_refused() {
        let err = fetch("gemini://example.com/", UserAgent::Honest).unwrap_err();
        assert!(matches!(err, Error::UnsupportedScheme(s) if s == "gemini"));
    }
}
