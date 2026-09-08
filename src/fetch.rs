//! Загрузка страницы. Доверие к сертификатам делегировано ОС
//! (`rustls-platform-verifier`), своего root store в бинарнике нет —
//! проверяется одной командой: `cargo tree | grep webpki-roots` пусто.

use std::time::Duration;

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

    #[test]
    fn non_http_schemes_are_refused() {
        let err = fetch("gemini://example.com/", UserAgent::Honest).unwrap_err();
        assert!(matches!(err, Error::UnsupportedScheme(s) if s == "gemini"));
    }
}
