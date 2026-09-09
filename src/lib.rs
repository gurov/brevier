//! Brevier — чтение без JavaScript.
//!
//! Ядро отделено от интерфейса намеренно: markdown, извлечение, адресная
//! строка и история не знают, кто их рисует. Тулкит для рустовых GUI пока
//! выбирается замером (см. `TODO.md`, спайк доступности), и смена выбора
//! должна стоить «переписать вид», а не «переписать всё».

pub mod address;
pub mod code;
pub mod error;
pub mod failure;
pub mod extract;
pub mod fetch;
pub mod history;
pub mod markdown;
/// Картинки. За фичей `images`: корпусу M0 декодеры не нужны, а лишний
/// код в бинарнике про безопасность — лишняя поверхность.
#[cfg(feature = "images")]
pub mod media;
pub mod outline;
/// Сохранение статьи на диск. За фичей `save`: zip нужен окну, не корпусу.
#[cfg(feature = "save")]
pub mod save;

use std::path::Path;

pub use address::Address;
pub use error::Error;
pub use fetch::UserAgent;
pub use history::History;

use fetch::ContentKind;

/// Прочитанный документ в том виде, в каком его показывает окно.
#[derive(Debug, Clone)]
pub struct Document {
    /// Адрес, по которому документ открыт. После редиректов — итоговый.
    pub address: Address,
    pub title: String,
    /// Тело в markdown: и переваренная веб-страница, и родной `.md`.
    pub markdown: String,
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
        // Режим репозитория — M2. До тех пор открываем ту же страницу вебом:
        // честная деградация лучше заглушки «пока не умеем».
        Address::Repo(repo) => open_web(&repo.web_url(), ua),
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
            address,
        }),
        ContentKind::Html => {
            let article = extract::extract(&page.body, &page.url)?;
            let title = article.title.clone();
            Ok(Document {
                markdown: markdown::from_article(&article)?,
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
