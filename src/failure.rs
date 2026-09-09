//! Ошибки на языке читателя.
//!
//! `Error` писан для stderr и прогона корпуса: там нужен код возврата
//! и короткая строка. В окне нужно другое — что случилось, почему и что
//! теперь делать. Тексты живут в ядре: они одни и те же для любого
//! интерфейса.
//!
//! Язык по умолчанию английский — это язык продукта. Перевод, когда до него
//! дойдут руки, встанет ровно сюда: наружу отсюда торчат готовые строки,
//! а не куски, которые интерфейс склеивает сам.

use crate::Error;

/// Ошибка, переведённая с языка тракта на язык читателя.
///
/// Разные причины требуют разных ответов: сертификат лечится установкой корня
/// в систему, 403 не лечится ничем, а пустое извлечение — повод сразу
/// предложить системный браузер.
#[derive(Debug, Clone)]
pub struct Failure {
    pub headline: &'static str,
    pub detail: String,
    /// Стоит ли предлагать открыть страницу в системном браузере.
    pub offer_browser: bool,
}

pub fn describe(error: &Error) -> Failure {
    let failure = |headline, detail: String, offer_browser| Failure {
        headline,
        detail,
        offer_browser,
    };

    match error {
        Error::BadUrl(what) if what.trim().is_empty() => failure(
            "Empty address",
            "Type the address of a page, or the path to a `.md` file.".to_owned(),
            false,
        ),
        Error::BadUrl(what) => failure(
            "That does not look like an address",
            format!("Brevier could not tell what to open: “{what}”. A full address looks like https://example.com/article."),
            false,
        ),
        Error::UnsupportedScheme(scheme) => failure(
            "Brevier does not open such addresses",
            format!("The “{scheme}:” scheme is not supported — Brevier speaks http and https only."),
            true,
        ),
        // Проверку сертификата не обходим никогда, поэтому объясняем причину:
        // это не «сайт сломался», а нехватка корня в хранилище самой системы,
        // и лечится она установкой корня, а не флагом в читалке.
        Error::Network(e) if is_certificate_problem(&e.to_string()) => failure(
            "The site's certificate is not trusted",
            "It is signed by an authority your operating system does not know. Brevier trusts the same roots as the rest of the system and never works around the check. If you do know that authority, install its root into the system."
                .to_owned(),
            true,
        ),
        Error::Network(e) => failure(
            "Could not connect",
            format!("{e}. The host may be down, or there is no network."),
            true,
        ),
        Error::HttpStatus(401 | 403) => failure(
            "The site would not let us in",
            "The page is closed to visitors who are not logged in, or a bot filter turned us away. Brevier cannot log in — that is deliberate."
                .to_owned(),
            true,
        ),
        Error::HttpStatus(404 | 410) => failure(
            "The page is not there",
            "The server says nothing lives at this address.".to_owned(),
            true,
        ),
        Error::HttpStatus(429) => failure(
            "Too often",
            "The site asks you to wait: it has had more requests from your address than it is willing to take."
                .to_owned(),
            true,
        ),
        Error::HttpStatus(code) if *code >= 500 => failure(
            "The site's server answers with an error",
            format!("Code {code}. This one is not on you — try again later."),
            true,
        ),
        Error::HttpStatus(code) => failure(
            "The server answered with something else",
            format!("Code {code}."),
            true,
        ),
        Error::UnsupportedContentType(kind) => failure(
            "This is not a page",
            format!("The server sent “{kind}”. Brevier reads html, markdown and plain text; PDF, video and images are work for the system browser."),
            true,
        ),
        Error::TooLarge(limit) => failure(
            "The page is too big",
            format!("The body did not fit the {limit} byte limit."),
            true,
        ),
        Error::EmptyExtraction => failure(
            "There is no article on this page",
            "That is how feeds, catalogues and sites assembled by JavaScript look. Brevier shows an article or says plainly that there is none."
                .to_owned(),
            true,
        ),
        Error::Convert(e) => failure("Could not make sense of the page", format!("{e}"), true),
        Error::Media(what) => failure(
            "The image cannot be shown",
            format!("{what}. The text of the article is not affected."),
            true,
        ),
    }
}

/// У `ureq` причина отказа TLS не вынесена в тип — она приходит текстом
/// от `rustls`. Смотрим на текст: другого способа отличить недоверенный
/// сертификат от оборванного соединения сейчас нет.
fn is_certificate_problem(message: &str) -> bool {
    let message = message.to_lowercase();
    message.contains("certificate") || message.contains("unknownissuer")
}
