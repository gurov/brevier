//! Извлечение основного содержимого — порт Mozilla Readability.
//!
//! Это самое хрупкое место продукта: сайты меняют разметку, эвристики протухают.
//! Поэтому вывод отсюда фиксируется в корпусе эталонов и служит базой
//! регрессионных тестов на всю жизнь проекта.

use std::collections::HashMap;

use dom_query::Document;
use dom_smoothie::{Config, Readability, ReadabilityError};
use url::Url;

use crate::error::Error;

pub struct Article {
    pub title: String,
    pub byline: Option<String>,
    /// Очищенный HTML статьи. Относительные ссылки уже развёрнуты в абсолютные.
    pub content_html: String,
    /// Миниатюры записей: адрес ссылки → адрес картинки внутри неё.
    /// Собраны со всей страницы, до извлечения; подставляются обратно
    /// только в ленте — см. [`thumbs`].
    pub thumbs: HashMap<String, String>,
}

pub fn extract(html: &str, url: &str) -> Result<Article, Error> {
    let cfg = Config::default();

    let doc = Document::from(html);
    unlazy(&doc);
    // Снять до извлечения: `Readability` документ перебирает и чистит,
    // и половины картинок после него в дереве уже нет.
    let thumbs = thumbs(&doc, url);

    let mut readability =
        Readability::with_document(doc, Some(url), Some(cfg)).map_err(|e| match e {
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
        thumbs,
    })
}

/// Картинки, спрятанные за ссылками: адрес ссылки → адрес картинки.
///
/// Лента состоит из карточек «миниатюра плюс заголовок», и миниатюру сайт
/// помечает `aria-hidden="true"` — для скринридера она дубль соседнего
/// заголовка, и размечено это правильно. Readability понимает подсказку
/// буквально и выбрасывает картинку вместе со ссылкой; читателю, который
/// смотрит глазами, карточка без картинки уже не карточка.
///
/// Поэтому миниатюры снимаются с исходного дерева, до извлечения, и в текст
/// возвращаются только там, где страница оказалась лентой: в статье такая
/// подстановка была бы отсебятиной.
///
/// Адреса разворачиваем сами: `fix_relative_uris` из dom_smoothie правит
/// только извлечённое содержимое, а мы берём картинки мимо него.
fn thumbs(doc: &Document, base: &str) -> HashMap<String, String> {
    /// Насколько глубоко картинка сидит внутри ссылки. Обычно `<a><img>`,
    /// но между ними бывает `<figure>` или `<span>` с рамкой.
    const DEPTH: usize = 4;

    let base = Url::parse(base).ok();
    let mut out = HashMap::new();

    for img in doc.select("img[src]").nodes() {
        let Some(src) = img.attr("src") else {
            continue;
        };
        let Some(link) = img
            .ancestors_it(Some(DEPTH))
            .find(|node| node.node_name().as_deref() == Some("a"))
        else {
            continue;
        };
        let Some(href) = link.attr("href") else {
            continue;
        };

        let (Some(href), Some(src)) = (
            absolute(base.as_ref(), &href),
            absolute(base.as_ref(), &src),
        ) else {
            continue;
        };
        // Первая картинка ссылки и есть её миниатюра: дальше идут значки
        // вроде «комментарии» и «поделиться».
        out.entry(href).or_insert(src);
    }

    out
}

fn absolute(base: Option<&Url>, link: &str) -> Option<String> {
    let link = link.trim();
    if link.is_empty() || link.starts_with("data:") {
        return None;
    }
    match base {
        Some(base) => base.join(link).ok().map(String::from),
        None => Url::parse(link).ok().map(String::from),
    }
}

/// Снять подсказку `loading="lazy"` с картинок, у которых адрес и так на месте.
///
/// `loading` — подсказка браузеру, когда качать, а не признак подменённого
/// адреса. dom_smoothie считает иначе: у него в «ленивые» попадает любая
/// `<img loading="lazy">`, и тогда он ищет настоящий адрес по остальным
/// атрибутам — берёт первый, в значении которого мерещится имя файла
/// картинки. У википедии рядом со `src` стоит `resource` с адресом
/// *страницы описания* файла (`…/wiki/Файл:Портрет.jpg`), и он затирает
/// настоящий адрес на upload.wikimedia.org — вместо фотографии читателю
/// приезжает html. Иконки на той же странице уцелели случайно: их `resource`
/// оканчивается на `.svg`, а этого расширения в списке у dom_smoothie нет.
///
/// В самом readability.js ленивой считается картинка без `src` либо
/// с классом `lazy`; возвращаем это правило, снимая подсказку с тех,
/// у кого адрес уже есть. Заглушку в `src` (`data:`-пиксель) не трогаем:
/// вот там подстановка по атрибутам и есть единственный способ найти
/// картинку.
fn unlazy(doc: &Document) {
    for node in doc.select("img[loading]").nodes() {
        let Some(src) = node.attr("src") else {
            continue;
        };
        let src = src.trim();
        if src.is_empty() || src.starts_with("data:") {
            continue;
        }
        node.remove_attr("loading");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Статья с картинкой, у которой рядом со `src` лежит посторонний
    /// атрибут с похожим на картинку значением.
    fn page(img: &str) -> String {
        let text = "Кеннет Эрроу доказал теорему о невозможности коллективного \
            выбора, и это перевернуло теорию общественного благосостояния. \
            Ниже разбирается, что именно утверждает теорема и почему её \
            следствия так неудобны для любой процедуры голосования.";
        format!(
            "<html><body><article><h1>Эрроу</h1><p>{img}</p><p>{text}</p><p>{text}</p><p>{text}</p></article></body></html>"
        )
    }

    #[test]
    fn a_lazy_hint_does_not_replace_a_working_address() {
        let img = r#"<img loading="lazy" src="https://upload.example.org/commons/portrait.jpg" resource="https://example.org/wiki/File:Portrait.jpg">"#;
        let article = extract(&page(img), "https://example.org/wiki/Arrow").unwrap();
        assert!(
            article
                .content_html
                .contains(r#"src="https://upload.example.org/commons/portrait.jpg""#)
        );
        assert!(
            !article
                .content_html
                .contains(r#"src="https://example.org/wiki/File:Portrait.jpg""#)
        );
    }

    #[test]
    fn a_placeholder_is_still_replaced() {
        let img = r#"<img loading="lazy" src="data:image/gif;base64,R0lGODlhAQABAAAAACH5BAEKAAEALAAAAAABAAEAAAICTAEAOw==" data-src="https://upload.example.org/commons/portrait.jpg">"#;
        let article = extract(&page(img), "https://example.org/wiki/Arrow").unwrap();
        assert!(
            article
                .content_html
                .contains("upload.example.org/commons/portrait.jpg")
        );
    }
}
