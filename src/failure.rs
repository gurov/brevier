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

use crate::address::{Address, Internal};
use crate::archive::{self, Archive};
use crate::{Error, hosts};

/// Ошибка, переведённая с языка тракта на язык читателя.
///
/// Разные причины требуют разных ответов: сертификат лечится установкой корня
/// в систему, 403 — входом, которого у нас нет и не будет, пустое
/// извлечение — скриптами. Системный браузер предлагаем при любом отказе,
/// где есть что ему отдать: он умеет то, от чего Brevier отказался, и это
/// единственный выход со страницы, которая не показалась. Не предлагаем
/// только на адресе, который не разобрался, — отдавать нечего.
#[derive(Debug, Clone)]
pub struct Failure {
    pub headline: &'static str,
    pub detail: String,
    /// Стоит ли предлагать открыть страницу в системном браузере.
    pub offer_browser: bool,
    /// Копии страницы (#9): своя из архива, Wayback — кнопками под текстом,
    /// в этом порядке. Открываются в самом Brevier.
    pub ways: Vec<Way>,
}

/// Другой путь к тексту страницы, которая не показалась: её копия.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Way {
    /// Подпись кнопки.
    pub label: &'static str,
    pub address: String,
}

pub fn describe(error: &Error) -> Failure {
    let failure = |headline, detail: String, offer_browser| Failure {
        headline,
        detail,
        offer_browser,
        ways: Vec::new(),
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
        Error::HostingLimit => failure(
            "The hosting is counting our requests",
            "Listing a directory asks the hosting's API, and GitHub allows sixty such requests an hour without a token. Everything else in a repository is read from the CDN, which has no limit — so files and READMEs still open. Wait a while for listings."
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
            format!("The server sent “{kind}”. Brevier reads html, markdown, plain text and RSS, Atom or JSON feeds; PDF, video and images are work for the system browser."),
            true,
        ),
        Error::TooLarge(limit) => failure(
            "The page is too big",
            format!("The body did not fit the {limit} byte limit."),
            true,
        ),
        Error::EmptyExtraction => failure(
            "There is no article on this page",
            "That is how sites assembled by JavaScript look, and pages with nothing to read but a form. Brevier shows an article or a list of links, or says plainly that there is neither."
                .to_owned(),
            true,
        ),
        Error::Convert(e) => failure("Could not make sense of the page", format!("{e}"), true),
        Error::Feed(e) => failure(
            "The feed cannot be read",
            format!("It is a feed, but broken beyond what Brevier repairs: {e}."),
            true,
        ),
        Error::SearchChallenge => failure(
            "The search engine wants proof you are human",
            "DuckDuckGo shows a puzzle instead of results to searches it takes for a bot — usually after several in a row. Brevier does not solve puzzles: open the search in your browser, or try again in a few minutes."
                .to_owned(),
            true,
        ),
        // Заслон, а не пустая страница (#23): скрипт ставит куку и грузит
        // страницу заново. Обходить его, исполняя или читая скрипт, не станем —
        // браузер читателя проходит его сам.
        Error::ScriptGate => failure(
            "The site lets in only browsers that run JavaScript",
            "Instead of the page it sent a script that sets a cookie and loads the page again — a check that the client runs scripts, with nothing to read behind it yet. Brevier runs no JavaScript; your browser passes the check."
                .to_owned(),
            true,
        ),
        // Снимки у Wayback есть почти у всего, но у мёртвой страницы
        // последние — часто снимки её 404: такие не предлагаем вовсе.
        Error::NotInWayback => failure(
            "The Wayback Machine has no copy",
            "It has not saved this page, or saved only errors in its place. In your browser it shows every visit it made to this address."
                .to_owned(),
            true,
        ),
        Error::Media(what) => failure(
            "The image cannot be shown",
            format!("{what}. The text of the article is not affected."),
            true,
        ),
    }
}

/// Отказ на странице, которую открывали (#9): к объяснению — её копии.
///
/// Своя копия из архива — при любом отказе: она на диске читателя, сети
/// ей не нужно, и прочитанное однажды остаётся его. Копия Wayback — только
/// когда страницы, похоже, больше нет: 404, 410, сервер падает или
/// не отвечает. На 403 она была бы обходом решения сайта, а не выходом.
///
/// Кнопкой, а не подменой: копия — не страница, и показать одну вместо
/// другой, не спросив, значило бы притворяться. Есть ли снимок у Wayback,
/// заранее не спрашиваем: это запрос к третьей стороне с адресом страницы,
/// о котором читатель не просил. Спрашивает кнопка — `brevier:wayback/…`,
/// и снимка нет — так и скажем (`Error::NotInWayback`).
pub fn describe_page(error: &Error, address: &Address, archive: &Archive) -> Failure {
    let mut failure = describe(error);
    if let Some(copy) = archive.latest(address) {
        failure.detail.push_str(&format!(
            "\n\nYou read this page on {}, and your archive keeps a copy.",
            archive::long_date(&copy.read)
        ));
        failure.ways.push(Way {
            label: "Open your copy",
            address: archive::copy_address(&copy.path),
        });
    }
    if gone(error)
        && let Address::Web(url) = address
        && let Some(page) = hosts::for_wayback(url)
    {
        failure
            .detail
            .push_str("\n\nThe Wayback Machine at archive.org may have saved a copy.");
        failure.ways.push(Way {
            label: "Read the Wayback copy",
            address: Address::Internal(Internal::Wayback(page)).display(),
        });
    }
    failure
}

/// Похоже ли, что страницы больше нет: адрес пуст, сервер падает или
/// хоста не слышно. Сертификат — не то: сайт есть, ему не доверяет система.
fn gone(error: &Error) -> bool {
    match error {
        Error::HttpStatus(404 | 410) => true,
        Error::HttpStatus(code) => *code >= 500,
        Error::Network(e) => !is_certificate_problem(&e.to_string()),
        _ => false,
    }
}

/// У `ureq` причина отказа TLS не вынесена в тип — она приходит текстом
/// от `rustls`. Смотрим на текст: другого способа отличить недоверенный
/// сертификат от оборванного соединения сейчас нет.
fn is_certificate_problem(message: &str) -> bool {
    let message = message.to_lowercase();
    message.contains("certificate") || message.contains("unknownissuer")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Document;
    use crate::address::parse;
    use crate::markdown::Kind;
    use crate::store::Stamp;

    fn archive_with(url: &str) -> (Archive, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!(
            "brevier-failure-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        let archive = Archive::at(&dir);
        let document = Document {
            address: parse(url).unwrap(),
            title: "Latency".to_owned(),
            markdown: "# Latency\n\nText.\n".to_owned(),
            kind: Kind::Article,
            served: false,
            site: Vec::new(),
            feeds: Vec::new(),
            lang: None,
            next: None,
            archived: None,
        };
        archive
            .keep(&document, &Stamp::parse("2026-10-07T09:00:00Z").unwrap())
            .unwrap();
        (archive, dir)
    }

    /// Страницы нет, а читатель её читал: своя копия первой, Wayback второй,
    /// и о каждой — фраза под объяснением.
    #[test]
    fn a_page_that_is_gone_offers_your_copy_and_the_wayback_one() {
        let url = "https://danluu.com/keyboard-latency/";
        let (archive, dir) = archive_with(url);
        let failure = describe_page(
            &Error::HttpStatus(404),
            &parse(&format!("{url}#computers")).unwrap(),
            &archive,
        );
        assert_eq!(
            failure.ways,
            vec![
                Way {
                    label: "Open your copy",
                    address: "brevier:archive/danluu.com/2026-10-07-latency.md.lz4".to_owned(),
                },
                Way {
                    label: "Read the Wayback copy",
                    address: format!("brevier:wayback/{url}"),
                },
            ]
        );
        assert!(
            failure
                .detail
                .contains("You read this page on 7 October 2026")
        );
        assert!(failure.detail.contains("The Wayback Machine"));
        assert!(failure.offer_browser);
        let _ = std::fs::remove_dir_all(dir);
    }

    /// Сайт не пустил — Wayback не выход, а своя копия — да. Нет копии —
    /// нет и фразы о ней.
    #[test]
    fn a_closed_door_offers_only_your_own_copy() {
        let url = "https://danluu.com/keyboard-latency/";
        let (archive, dir) = archive_with(url);
        let closed = describe_page(&Error::HttpStatus(403), &parse(url).unwrap(), &archive);
        assert_eq!(closed.ways.len(), 1);
        assert_eq!(closed.ways[0].label, "Open your copy");
        assert!(!closed.detail.contains("Wayback"));

        let unread = parse("https://danluu.com/input-lag/").unwrap();
        let gone = describe_page(&Error::HttpStatus(503), &unread, &archive);
        assert_eq!(gone.ways.len(), 1);
        assert_eq!(gone.ways[0].label, "Read the Wayback copy");
        assert!(!gone.detail.contains("You read"));
        let _ = std::fs::remove_dir_all(dir);
    }
}
