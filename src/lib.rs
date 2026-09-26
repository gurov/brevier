//! Brevier — чтение без JavaScript.
//!
//! Ядро отделено от интерфейса намеренно: markdown, извлечение, адресная
//! строка и история не знают, кто их рисует. Тулкит для рустовых GUI
//! выбирался замером — доступность против качества текста, — и смена выбора
//! должна стоить «переписать вид», а не «переписать всё».

pub mod address;
/// Вход для Android: ядро общей библиотекой плюс JNI, две функции наружу.
/// Только под Android — платформа сидит на краю, как и тулкит.
#[cfg(target_os = "android")]
pub mod android;
/// Прочитанное на неделю: страница открывается с диска, а не из сети.
/// Пишет и читает интерфейс; cli и корпус кэша не видят.
pub mod cache;
/// Проверка страницы на пригодность к чтению: `brevier --check <url>`.
/// Гоняет обычный тракт и печатает отчёт со счётом.
pub mod check;
pub mod code;
pub mod error;
pub mod extract;
pub mod failure;
/// Ленты RSS и Atom: адрес ленты открывается списком ссылок с датами,
/// а ленты, объявленные страницей, едут на полку.
pub mod feed;
pub mod fetch;
pub mod history;
/// Правила под хост — одной небольшой таблицей: сейчас reddit, чьи
/// страницы читаются через ленту.
pub mod hosts;
/// Текст начальной страницы. В ядре по той же причине, что и `failure`.
pub mod intro;
/// JSON в дерево — своим разбором, для JSON Feed.
pub mod json;
pub mod markdown;
/// Картинки. За фичей `images`: корпусу M0 декодеры не нужны, а лишний
/// код в бинарнике про безопасность — лишняя поверхность.
#[cfg(feature = "images")]
pub mod media;
pub mod outline;
/// Страница, разложенная для показа: текст, стили, ссылки, якоря, места
/// картинок и таблиц. Её рисует интерфейс — любой. Нужны и картинки
/// (откуда их брать), и типографика (переносы в прозе).
#[cfg(all(feature = "images", feature = "typeset"))]
pub mod page;
/// Цвета страницы: бумага, краска, ссылка, подсветка кода. Часть
/// типографики, а не оформления окна, — поэтому в ядре.
pub mod palette;
/// Режим репозитория: документация читается из репозитория напрямую.
pub mod repo;
/// Сохранение статьи на диск. За фичей `save`: zip нужен окну, не корпусу.
#[cfg(feature = "save")]
pub mod save;
/// Что и как храним на диске: история посещённого, а дальше закладки
/// и сессия. Одно хранилище и один формат — иначе их заведётся два,
/// с разной судьбой.
pub mod store;
/// Типографика по языку: переносы и неразрывные пробелы. За фичей `typeset`
/// (её тянет `ui`): cli печатает markdown как есть, а паттерны переносов —
/// вес, не нужный ни ему, ни корпусу.
#[cfg(feature = "typeset")]
pub mod typeset;

use std::path::Path;

pub use address::Address;
pub use error::Error;
pub use extract::Link;
pub use fetch::UserAgent;
pub use history::History;
pub use markdown::Kind;

use address::{Internal, Repo};
use fetch::ContentKind;

/// Прочитанный документ в том виде, в каком его показывает окно.
#[derive(Debug, Clone)]
pub struct Document {
    /// Адрес, по которому документ открыт. После редиректов — итоговый.
    pub address: Address,
    pub title: String,
    /// Тело в markdown: и переваренная веб-страница, и родной `.md`.
    pub markdown: String,
    /// Статья или список ссылок. Знать это нужно интерфейсу: список ссылок
    /// читатель открывает не для чтения, а чтобы уйти дальше.
    pub kind: Kind,
    /// Сайт отдал текст markdown-ом сам — прямо по `Accept` или по ссылке
    /// `<link rel="alternate" type="text/markdown">`. Тогда извлечения не было
    /// вовсе, и интерфейс говорит об этом строкой: читатель получил точный
    /// текст автора, а не нашу реконструкцию.
    pub served: bool,
    /// Навигация самого сайта — его меню и подвал. В текст статьи это
    /// не идёт (меню посреди прозы — дефект), но и терять его нельзя:
    /// без JS страница остаётся набором ссылок, и с главной иначе некуда
    /// пойти. Показывать решает интерфейс.
    pub site: Vec<Link>,
    /// Ленты, которые страница объявила в шапке (`<link rel="alternate"
    /// type="application/rss+xml">`). Полка показывает их группой «This site
    /// has a feed»: подписка — дело будущего, а открыть ленту можно сейчас.
    pub feeds: Vec<Link>,
    /// Язык страницы (`<html lang>`), если объявлен. Окно берёт по нему
    /// переносы; без языка их нет.
    pub lang: Option<String>,
}

/// Установить провайдер шифров. `rustls` собран без встроенного, выбираем явно;
/// вызывать один раз при старте программы.
pub fn init_crypto() {
    let _ = rustls::crypto::ring::default_provider().install_default();
}

/// Открыть адрес.
pub fn open(address: &Address, ua: UserAgent) -> Result<Document, Error> {
    match address {
        Address::Web(url) => open_web(url, ua),
        Address::Repo(repo) => open_repo(repo, ua),
        Address::File(path) => open_file(path),
        Address::Internal(page) => open_internal(page, ua),
    }
}

/// Страница самой программы. Истории и закладкам сети не нужно, зато
/// есть диск: читаем его заново, а не из памяти окна, — так страница верна
/// и тогда, когда программа открыта дважды. Проверка ходит в сеть, как
/// `--check`, — за той страницей, которую проверяет.
fn open_internal(page: &Internal, ua: UserAgent) -> Result<Document, Error> {
    let (title, markdown) = match page {
        Internal::History => ("History".to_owned(), store::Store::open().page()),
        Internal::Bookmarks => ("Bookmarks".to_owned(), store::Marks::open().page()),
        Internal::Check(None) => ("Check".to_owned(), check::about().to_owned()),
        Internal::Check(Some(url)) => {
            let report = check::check(url, ua)?;
            let host = url::Url::parse(url)
                .ok()
                .and_then(|parsed| parsed.host_str().map(str::to_owned))
                .unwrap_or_else(|| url.clone());
            (format!("Check · {host}"), report.to_markdown())
        }
    };
    Ok(Document {
        address: Address::Internal(page.clone()),
        title,
        markdown,
        // Ссылок тут список, но это не лента: читатель открыл историю
        // намеренно, и говорить ему «это список ссылок» незачем.
        kind: Kind::Article,
        served: false,
        site: Vec::new(),
        feeds: Vec::new(),
        lang: None,
    })
}

fn open_web(url: &str, ua: UserAgent) -> Result<Document, Error> {
    // `readable`, а не `fetch`: если страница объявила markdown-двойника,
    // читателю достаётся он — точный текст без извлечения.
    let page = fetch::readable(url, ua)?;
    let address = Address::Web(page.url.clone());

    match page.kind {
        // Родной формат и простой текст отдаём как есть: переписывать текст
        // автора незачем. Markdown сайт отдал сам — об этом скажет интерфейс.
        ContentKind::Markdown | ContentKind::Text => Ok(Document {
            title: heading_of(&page.body).unwrap_or_else(|| page.url.clone()),
            markdown: page.body,
            kind: Kind::Article,
            served: page.kind == ContentKind::Markdown,
            site: Vec::new(),
            feeds: Vec::new(),
            // Родной текст без HTML — языка мы не знаем.
            lang: None,
            address,
        }),
        ContentKind::Html => from_html(&page.body, &page.url),
        ContentKind::Feed => from_feed(&page.body, &page.url),
    }
}

/// Лента: список записей со ссылками и датами. Это «список ссылок», тот же
/// вид, что у главной блога, — и так же не кладётся в недельную копию:
/// ленту открывают ради нового. Лента обсуждения одной страницы — тред,
/// его читают, а не выбирают из него: это статья.
pub fn from_feed(xml: &str, url: &str) -> Result<Document, Error> {
    let parsed = feed::parse(xml, url)?;
    let thread = feed::thread(&parsed);
    let title = thread
        .as_ref()
        .filter(|thread| thread.post)
        .and_then(|_| parsed.entries.first()?.title.clone())
        .unwrap_or_else(|| parsed.title.clone());
    Ok(Document {
        address: Address::Web(url.to_owned()),
        title,
        markdown: feed::to_markdown(&parsed),
        kind: if thread.is_some() {
            Kind::Article
        } else {
            Kind::Listing
        },
        served: false,
        site: Vec::new(),
        feeds: Vec::new(),
        lang: None,
    })
}

/// Страница, которая уже на руках: HTML пришёл не из сети, а из stdin
/// или из файла. Адрес обязателен и здесь — по нему разворачиваются
/// относительные ссылки и решается, что это за документ.
pub fn from_html(html: &str, url: &str) -> Result<Document, Error> {
    let article = extract::extract(html, url)?;
    let title = article.title.clone();
    let site = article.site.clone();
    let feeds = article.feeds.clone();
    let lang = article.lang.clone();
    let reading = markdown::from_article(&article)?;

    Ok(Document {
        markdown: reading.markdown,
        kind: reading.kind,
        // HTML на руках всегда проходит извлечение: markdown-двойника ищет
        // тракт из сети (`fetch::readable`), а не этот путь.
        served: false,
        site,
        feeds,
        lang,
        title: if title.trim().is_empty() {
            url.to_owned()
        } else {
            title
        },
        address: Address::Web(url.to_owned()),
    })
}

/// Файл из репозитория. Конвертации здесь нет — формат родной; вся работа
/// в том, чтобы найти файл и вернуть его ссылкам контекст репозитория.
fn open_repo(repo: &Repo, ua: UserAgent) -> Result<Document, Error> {
    // Исходники и картинки режим репозитория не показывает: это не документы.
    // Отдаём их страницей хостинга — честная деградация, а не заглушка.
    if let Some(path) = &repo.path
        && repo::target(path) == repo::Target::Other
    {
        return open_web(&repo.blob_url(path), ua);
    }

    let loaded = repo::open(repo, ua)?;
    let address = Address::Repo(Repo {
        path: Some(loaded.path.clone()),
        // Каталог остаётся каталогом и в адресной строке, и в истории:
        // иначе «назад» вернуло бы README вместо списка, из которого ушли.
        listing: loaded.kind == Kind::Listing,
        source: None,
        ..repo.clone()
    });

    Ok(Document {
        title: heading_of(&loaded.markdown)
            .unwrap_or_else(|| format!("{}/{}/{}", repo.owner, repo.name, loaded.path)),
        markdown: loaded.markdown,
        // Каталог без README приезжает списком ссылок, а не статьёй:
        // читатель открыл его, чтобы выбрать, куда идти дальше.
        kind: loaded.kind,
        // Режим репозитория — свой тракт, а не «сайт отдал markdown»:
        // об этом читатель и так знает по адресу.
        served: false,
        // У репозитория своя навигация — точки входа в документацию,
        // и их ищет окно отдельно (`seek_entries`).
        site: Vec::new(),
        feeds: Vec::new(),
        // README — родной markdown, `<html lang>` в нём нет.
        lang: None,
        address,
    })
}

fn open_file(path: &Path) -> Result<Document, Error> {
    let bytes = std::fs::read(path).map_err(Error::Convert)?;
    // Лента на диске: скачанная, сохранённая, открытая из файлового
    // менеджера. Кодировку она называет сама, в объявлении XML.
    let decoded = feed::decode(&bytes, None);
    if feed::is_feed(&decoded) {
        let mut document = from_feed(&decoded, &path.display().to_string())?;
        document.address = Address::File(path.to_path_buf());
        return Ok(document);
    }
    // XML и JSON, которые не лента, — не текст для чтения: показывать их
    // исходником значило бы выдать разметку за статью.
    let extension = path
        .extension()
        .map(|extension| extension.to_string_lossy().to_ascii_lowercase());
    if let Some(kind @ ("xml" | "rss" | "atom" | "json")) = extension.as_deref() {
        let mime = if kind == "json" {
            "application/json"
        } else {
            "application/xml"
        };
        return Err(Error::UnsupportedContentType(mime.to_owned()));
    }
    let body = String::from_utf8(bytes).map_err(|error| {
        Error::Convert(std::io::Error::new(std::io::ErrorKind::InvalidData, error))
    })?;
    let title = heading_of(&body)
        .or_else(|| {
            path.file_name()
                .map(|name| name.to_string_lossy().into_owned())
        })
        .unwrap_or_default();

    Ok(Document {
        address: Address::File(path.to_path_buf()),
        title,
        markdown: body,
        kind: Kind::Article,
        served: false,
        site: Vec::new(),
        feeds: Vec::new(),
        lang: None,
    })
}

/// Заголовок документа — первый `# ` в тексте.
/// Ищем в тексте после шапки YAML: строка `# …` в самой шапке — комментарий
/// YAML, а не заголовок. Нет заголовка в тексте — берём `title:` из шапки.
fn heading_of(markdown: &str) -> Option<String> {
    markdown::body(markdown)
        .lines()
        .find_map(|line| {
            line.strip_prefix("# ")
                .map(str::trim)
                .filter(|text| !text.is_empty())
                .map(str::to_owned)
        })
        .or_else(|| markdown::front_matter_title(markdown))
}
