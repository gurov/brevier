//! Извлечение основного содержимого — порт Mozilla Readability.
//!
//! Это самое хрупкое место продукта: сайты меняют разметку, эвристики протухают.
//! Поэтому вывод отсюда фиксируется в корпусе эталонов и служит базой
//! регрессионных тестов на всю жизнь проекта.

use dom_smoothie::{Config, Readability, ReadabilityError};

use crate::error::Error;

pub struct Article {
    pub title: String,
    pub byline: Option<String>,
    /// Очищенный HTML статьи. Относительные ссылки уже развёрнуты в абсолютные.
    pub content_html: String,
}

pub fn extract(html: &str, url: &str) -> Result<Article, Error> {
    let cfg = Config::default();

    let mut readability = Readability::new(html, Some(url), Some(cfg)).map_err(|e| match e {
        ReadabilityError::BadDocumentURL => Error::BadUrl(url.to_owned()),
        _ => Error::EmptyExtraction,
    })?;

    let article = readability.parse().map_err(|_| Error::EmptyExtraction)?;

    if article.content.trim().is_empty() {
        return Err(Error::EmptyExtraction);
    }

    Ok(Article {
        title: article.title.to_string(),
        byline: article.byline.filter(|b| !b.trim().is_empty()),
        content_html: article.content.to_string(),
    })
}
