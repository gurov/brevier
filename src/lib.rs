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

use std::path::Path;

pub use address::Address;
pub use error::Error;
pub use fetch::UserAgent;
pub use history::History;
pub use markdown::Kind;

use address::Repo;
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
            address,
        }),
        ContentKind::Html => {
            let article = extract::extract(&page.body, &page.url)?;
            let title = article.title.clone();
            let reading = markdown::from_article(&article)?;
            Ok(Document {
                markdown: reading.markdown,
                kind: reading.kind,
                title: if title.trim().is_empty() {
                    page.url.clone()
                } else {
                    title
                },
                address,
            })
        }
    }
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
        source: None,
        ..repo.clone()
    });

    Ok(Document {
        title: heading_of(&loaded.markdown)
            .unwrap_or_else(|| format!("{}/{}/{}", repo.owner, repo.name, loaded.path)),
        markdown: loaded.markdown,
        kind: Kind::Article,
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
