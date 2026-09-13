//! Brevier — чтение без JavaScript.
//!
//! Ядро отделено от интерфейса намеренно: markdown, извлечение, адресная
//! строка и история не знают, кто их рисует. Тулкит для рустовых GUI
//! выбирался замером — доступность против качества текста, — и смена выбора
//! должна стоить «переписать вид», а не «переписать всё».

pub mod address;
pub mod code;
pub mod error;
pub mod extract;
pub mod failure;
pub mod fetch;
pub mod history;
/// Текст начальной страницы. В ядре по той же причине, что и `failure`.
pub mod intro;
pub mod markdown;
/// Картинки. За фичей `images`: корпусу M0 декодеры не нужны, а лишний
/// код в бинарнике про безопасность — лишняя поверхность.
#[cfg(feature = "images")]
pub mod media;
pub mod outline;
/// Режим репозитория: документация читается из репозитория напрямую.
pub mod repo;
/// Сохранение статьи на диск. За фичей `save`: zip нужен окну, не корпусу.
#[cfg(feature = "save")]
pub mod save;
/// Что и как храним на диске: история посещённого, а дальше закладки
/// и сессия. Одно хранилище и один формат — иначе их заведётся два,
/// с разной судьбой.
pub mod store;

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
    /// Навигация самого сайта — его меню и подвал. В текст статьи это
    /// не идёт (меню посреди прозы — дефект), но и терять его нельзя:
    /// без JS страница остаётся набором ссылок, и с главной иначе некуда
    /// пойти. Показывать решает интерфейс.
    pub site: Vec<Link>,
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
        Address::Internal(page) => Ok(open_internal(*page)),
    }
}

/// Страница самой программы. Сети здесь нет, зато есть диск: историю
/// читаем заново, а не из памяти окна, — так страница верна и тогда,
/// когда программа открыта дважды.
fn open_internal(page: Internal) -> Document {
    let (title, markdown) = match page {
        Internal::History => ("History", store::Store::open().page()),
        Internal::Bookmarks => ("Bookmarks", store::Marks::open().page()),
    };
    Document {
        address: Address::Internal(page),
        title: title.to_owned(),
        markdown,
        // Ссылок тут список, но это не лента: читатель открыл историю
        // намеренно, и говорить ему «это список ссылок» незачем.
        kind: Kind::Article,
        site: Vec::new(),
    }
}

fn open_web(url: &str, ua: UserAgent) -> Result<Document, Error> {
    let page = fetch::fetch(url, ua)?;
    let address = Address::Web(page.url.clone());

    match page.kind {
        // Родной формат и простой текст отдаём как есть: переписывать текст
        // автора незачем.
        ContentKind::Markdown | ContentKind::Text => Ok(Document {
            title: heading_of(&page.body).unwrap_or_else(|| page.url.clone()),
            markdown: page.body,
            kind: Kind::Article,
            site: Vec::new(),
            address,
        }),
        ContentKind::Html => from_html(&page.body, &page.url),
    }
}

/// Страница, которая уже на руках: HTML пришёл не из сети, а из stdin
/// или из файла. Адрес обязателен и здесь — по нему разворачиваются
/// относительные ссылки и решается, что это за документ.
pub fn from_html(html: &str, url: &str) -> Result<Document, Error> {
    let article = extract::extract(html, url)?;
    let title = article.title.clone();
    let site = article.site.clone();
    let reading = markdown::from_article(&article)?;

    Ok(Document {
        markdown: reading.markdown,
        kind: reading.kind,
        site,
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
        // У репозитория своя навигация — точки входа в документацию,
        // и их ищет окно отдельно (`seek_entries`).
        site: Vec::new(),
        address,
    })
}

fn open_file(path: &Path) -> Result<Document, Error> {
    let body = std::fs::read_to_string(path).map_err(Error::Convert)?;
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
        site: Vec::new(),
    })
}

/// Заголовок документа — первый `# ` в тексте.
fn heading_of(markdown: &str) -> Option<String> {
    markdown.lines().find_map(|line| {
        line.strip_prefix("# ")
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .map(str::to_owned)
    })
}
