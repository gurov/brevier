//! Ошибки на языке читателя.
//!
//! `Error` писан для stderr и прогона корпуса: там нужен код возврата
//! и короткая английская строка. В окне нужно другое — что случилось,
//! почему и что теперь делать. Тексты живут в ядре: они одни и те же
//! для любого интерфейса.

use crate::Error;

/// Ошибка, переведённая с языка тракта на язык читателя.
///
/// `brevier::Error` писан для stderr и прогона корпуса: там нужен код возврата
/// и короткая английская строка. В окне нужно другое — что случилось, почему
/// и что теперь делать. Разные причины требуют разных ответов: сертификат
/// лечится установкой корня в систему, 403 не лечится ничем, а пустое
/// извлечение — повод сразу предложить системный браузер.
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
            "Пустой адрес",
            "Напечатайте адрес страницы, `gh:owner/repo` или путь к файлу `.md`."
                .to_owned(),
            false,
        ),
        Error::BadUrl(what) => failure(
            "Это не похоже на адрес",
            format!("Brevier не понял, что открывать: «{what}». Полный адрес выглядит так: https://example.com/статья."),
            false,
        ),
        Error::UnsupportedScheme(scheme) => failure(
            "Такие адреса Brevier не открывает",
            format!("Схема «{scheme}:» не поддерживается — Brevier ходит только по http и https."),
            true,
        ),
        // Проверку сертификата не обходим никогда, поэтому объясняем причину:
        // это не «сайт сломался», а нехватка корня в хранилище самой системы,
        // и лечится она установкой корня, а не флагом в читалке.
        Error::Network(e) if is_certificate_problem(&e.to_string()) => failure(
            "Сертификату сайта нет доверия",
            "Он подписан центром, которого нет в хранилище вашей операционной системы. Brevier доверяет тем же корням, что и вся система, и проверку не обходит. Если этот центр вам известен — поставьте его корень в систему."
                .to_owned(),
            true,
        ),
        Error::Network(e) => failure(
            "Не удалось соединиться",
            format!("{e}. Возможно, хост недоступен или нет сети."),
            true,
        ),
        Error::HttpStatus(401 | 403) => failure(
            "Сайт не пустил",
            "Страница закрыта для незалогиненных или отсечена защитой от ботов. Brevier не умеет логиниться — это осознанно."
                .to_owned(),
            true,
        ),
        Error::HttpStatus(404 | 410) => failure(
            "Страницы нет",
            "Сервер отвечает, что по этому адресу ничего не лежит.".to_owned(),
            true,
        ),
        Error::HttpStatus(429) => failure(
            "Слишком часто",
            "Сайт просит подождать: запросов с вашего адреса пришло больше, чем он готов принять."
                .to_owned(),
            true,
        ),
        Error::HttpStatus(code) if *code >= 500 => failure(
            "Сервер сайта отвечает ошибкой",
            format!("Код {code}. Это не у вас — попробуйте позже."),
            true,
        ),
        Error::HttpStatus(code) => failure(
            "Сервер ответил не тем",
            format!("Код {code}."),
            true,
        ),
        Error::UnsupportedContentType(kind) => failure(
            "Это не страница",
            format!("Сервер отдал «{kind}». Brevier читает html, markdown и простой текст; PDF, видео и картинки — работа для системного браузера."),
            true,
        ),
        Error::TooLarge(limit) => failure(
            "Страница слишком большая",
            format!("Тело ответа не влезло в предел {limit} байт."),
            true,
        ),
        Error::EmptyExtraction => failure(
            "Статьи на странице нет",
            "Так выглядят ленты, каталоги и сайты, которые собираются джаваскриптом. Brevier показывает статью или честно говорит, что её нет."
                .to_owned(),
            true,
        ),
        Error::Convert(e) => failure(
            "Не удалось разобрать страницу",
            format!("{e}"),
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
