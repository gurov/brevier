//! Ошибки типизированы с самого начала.
//!
//! На M0 они уходят в stderr и в код возврата — прогон корпуса разбирается
//! скриптом. На M1 из этих же вариантов растут сообщения в окне: «недоступен
//! хост», «не тот content-type», «пустое извлечение» (см. TODO, M1).

use std::fmt;

#[derive(Debug)]
pub enum Error {
    /// Строку не удалось разобрать как URL.
    BadUrl(String),
    /// Схема не http(s). `file:`, `ftp:`, `gemini:` — всё мимо.
    UnsupportedScheme(String),
    /// Хост не нашёлся, соединение не встало, TLS не сошёлся, таймаут.
    Network(Box<ureq::Error>),
    /// Сервер ответил не-2xx.
    HttpStatus(u16),
    /// Тип содержимого, который мы не читаем: PDF, картинка, видео.
    UnsupportedContentType(String),
    /// Тело больше лимита.
    TooLarge(u64),
    /// Readability не нашёл на странице статьи.
    EmptyExtraction,
    /// HTML → Markdown.
    Convert(std::io::Error),
}

impl Error {
    /// Код возврата процесса. Прогон корпуса на M0 отличает «сайт не пустил»
    /// от «извлечение не сработало» — это разные величины.
    pub fn exit_code(&self) -> u8 {
        match self {
            Error::BadUrl(_) | Error::UnsupportedScheme(_) => 1,
            Error::Network(_) => 2,
            Error::HttpStatus(_) => 3,
            Error::UnsupportedContentType(_) | Error::TooLarge(_) => 4,
            Error::EmptyExtraction => 5,
            Error::Convert(_) => 6,
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::BadUrl(u) => write!(f, "not a URL: {u}"),
            Error::UnsupportedScheme(s) => {
                write!(f, "unsupported scheme `{s}:`, only http and https")
            }
            Error::Network(e) => write!(f, "{e}"),
            Error::HttpStatus(c) => write!(f, "server answered {c}"),
            Error::UnsupportedContentType(t) => write!(f, "unsupported content type `{t}`"),
            Error::TooLarge(n) => write!(f, "response body over the {n} byte limit"),
            Error::EmptyExtraction => write!(f, "no article found on the page"),
            Error::Convert(e) => write!(f, "html to markdown: {e}"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Network(e) => Some(e),
            Error::Convert(e) => Some(e),
            _ => None,
        }
    }
}

impl From<ureq::Error> for Error {
    fn from(e: ureq::Error) -> Self {
        match e {
            ureq::Error::StatusCode(code) => Error::HttpStatus(code),
            ureq::Error::BodyExceedsLimit(n) => Error::TooLarge(n),
            other => Error::Network(Box::new(other)),
        }
    }
}
