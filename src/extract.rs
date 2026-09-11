//! Извлечение основного содержимого — порт Mozilla Readability.
//!
//! Это самое хрупкое место продукта: сайты меняют разметку, эвристики протухают.
//! Поэтому вывод отсюда фиксируется в корпусе эталонов и служит базой
//! регрессионных тестов на всю жизнь проекта.

use std::collections::HashMap;

use dom_query::{Document, NodeRef};
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
    /// Лента страницы — если страница оказалась лентой, а извлечение
    /// принесло из неё одну карточку. HTML с абсолютными адресами,
    /// см. [`listing`].
    pub listing_html: Option<String>,
}

/// Сколько карточек делают страницу лентой.
///
/// Восемь, а не три: три карточки бывают и в хвосте статьи («читайте ещё»),
/// и шесть бывают — у spectrum.ieee.org ровно шесть. Лента же начинается
/// от десятка: у habr их девятнадцать, у блога на главной — десять.
const MIN_CARDS: usize = 8;

/// Какую долю ленты может занимать извлечённое, чтобы считаться записью
/// из неё, а не статьёй, к которой ленту приложили сбоку.
///
/// Замер 11 сентября 2026, шесть страниц: у ленты habr извлечённое — 17%
/// от суммы карточек, у статьи spectrum с шестью анонсами — 73%,
/// у stratechery — 75%, у alistapart — 400%. Порог посередине пустоты.
const CARD_SHARE: f32 = 0.4;

/// До этого размера картинка внутри ссылки — значок, а не изображение.
const ICON_PX: u32 = 48;

/// Лента, снятая с исходного дерева: её html и её же текст — по тексту
/// потом решают, лента это или статья с витриной в хвосте.
struct Listing {
    html: String,
    /// Текст ленты целиком: извлечённое должно лежать внутри неё, иначе
    /// это статья, а лента — виджет сбоку от неё.
    text: String,
    /// Сколько текста в записях. Складывается по карточкам, а не берётся
    /// у ленты: между карточками лежит и обвязка, которая в счёт не идёт.
    weight: usize,
    cards: usize,
}

pub fn extract(html: &str, url: &str) -> Result<Article, Error> {
    let cfg = Config::default();

    let doc = Document::from(html);
    unlazy(&doc);
    deicon(&doc);
    // Снять до извлечения: `Readability` документ перебирает и чистит,
    // и половины картинок после него в дереве уже нет.
    let thumbs = thumbs(&doc, url);
    let listing = listing(&doc, url);

    let mut readability =
        Readability::with_document(doc, Some(url), Some(cfg)).map_err(|e| match e {
            ReadabilityError::BadDocumentURL => Error::BadUrl(url.to_owned()),
            _ => Error::EmptyExtraction,
        })?;

    let article = readability.parse().map_err(|_| Error::EmptyExtraction)?;

    if article.content.trim().is_empty() {
        return Err(Error::EmptyExtraction);
    }

    let content_html = article.content.to_string();
    let listing_html = listing
        .filter(|listing| listing.holds(&content_html))
        .map(|listing| listing.html);

    Ok(Article {
        title: article.title.to_string(),
        byline: article.byline.filter(|b| !b.trim().is_empty()),
        content_html,
        thumbs,
        listing_html,
    })
}

impl Listing {
    /// Лежит ли извлечённое внутри одной из карточек.
    ///
    /// Вопрос ровно один: что нашла Readability — статью или одну запись
    /// из двадцати. Ответ даёт место, а не размер. Текст статьи, у которой
    /// в хвосте витрина «читайте ещё», не лежит ни в одной карточке
    /// витрины; текст выбранной записи лежит в своей карточке целиком.
    ///
    /// Сверять с лентой целиком нельзя: общим предком карточек бывает
    /// и весь `body` — когда заголовок-ссылка нашёлся и в шапке, и в подвале,
    /// — а в нём лежит вообще всё, включая статью. Поэтому сверяем
    /// с отдельной карточкой, и куском из середины: начало занято
    /// заголовком и подписью автора, которые извлечение переписывает.
    fn holds(&self, content_html: &str) -> bool {
        const SAMPLE: usize = 120;

        let inside = squeeze(&Document::from(content_html).text());
        let length = inside.chars().count();
        if self.cards < MIN_CARDS || length < SAMPLE * 2 {
            return false;
        }

        // Статья, к которой ленту приложили сбоку, занимает больше, чем сама
        // лента; запись из ленты — малую её долю.
        if length as f32 > self.weight as f32 * CARD_SHARE {
            return false;
        }

        // И лежать эта запись должна внутри ленты: у статьи с виджетом
        // «читайте ещё» текст лежит мимо него. Берём кусок из середины —
        // начало занято заголовком и подписью автора, которые извлечение
        // переписывает по-своему.
        let from: String = inside.chars().skip(length / 3).collect();
        let sample: String = from.chars().take(SAMPLE).collect();
        self.text.contains(&sample)
    }
}

/// Выбросить значки: картинку, которая одна заполняет собой ссылку.
///
/// Аватар автора, флажок языка, иконка «поделиться» — в ленте таких
/// по одной на карточку, и каждая занимает в тексте отдельную строку.
/// Признак по форме: объявлена мелкой с обеих сторон, без подписи,
/// и кроме неё в ссылке ничего нет. Картинку в прозе это не трогает
/// (tonsky ставит логотип apple прямо в строку — он не в ссылке),
/// формулу тоже: у неё в `alt` исходник.
fn deicon(doc: &Document) {
    for link in doc.select("a").nodes() {
        if !link.text().trim().is_empty() {
            continue;
        }
        let images: Vec<NodeRef> = link
            .descendants()
            .into_iter()
            .filter(|node| node.node_name().as_deref() == Some("img"))
            .collect();
        let [image] = images.as_slice() else {
            continue;
        };
        if is_icon(image) {
            image.remove_from_parent();
        }
    }
}

fn is_icon(image: &NodeRef) -> bool {
    let small = |name: &str| {
        image
            .attr(name)
            .and_then(|value| value.trim().parse::<u32>().ok())
            .is_some_and(|size| size <= ICON_PX)
    };
    let captioned = image.attr("alt").is_some_and(|alt| !alt.trim().is_empty());

    small("width") && small("height") && !captioned
}

/// Лента страницы: узел, в котором лежат все карточки.
///
/// Карточка — заголовок, который целиком ссылка на другую страницу; форма
/// та же, по которой конвертер отличает анонс от раздела. Readability
/// на ленте выбирает самую текстовую карточку и выбрасывает остальные
/// девятнадцать: получается «статья» под именем сайта, которой на странице
/// нет. Поэтому ленту снимаем сами — общим предком карточек, а не всей
/// страницей: так в текст не попадают ни меню, ни подвал.
///
/// Снимать надо до извлечения: `Readability` забирает документ себе.
fn listing(doc: &Document, base: &str) -> Option<Listing> {
    let cards: Vec<NodeRef> = doc
        .select("h1, h2, h3")
        .nodes()
        .iter()
        .filter(|node| is_card(node))
        .cloned()
        .collect();

    if cards.len() < MIN_CARDS {
        return None;
    }

    let region = common_ancestor(&cards)?;
    // Лента — часть страницы, а не страница целиком. Если в вырезанное
    // попали меню или подвал, значит общий предок карточек — весь документ:
    // так бывает, когда заголовок-ссылка нашёлся и в шапке, и сбоку.
    // Тогда вырезать нечего, и страница остаётся статьёй.
    if region
        .descendants()
        .iter()
        .any(|node| matches!(node.node_name().as_deref(), Some("nav" | "footer")))
    {
        return None;
    }

    let weight = cards
        .iter()
        .map(|card| squeeze(&block_of(card, &cards).text()).chars().count())
        .sum();

    Some(Listing {
        html: absolute_html(&region.html(), base),
        text: squeeze(&region.text()),
        weight,
        cards: cards.len(),
    })
}

/// Карточка целиком: самый верхний предок заголовка, в котором нет других
/// заголовков-карточек. Это и есть запись ленты — с миниатюрой, датой
/// и подводкой.
fn block_of<'a>(card: &NodeRef<'a>, cards: &[NodeRef<'a>]) -> NodeRef<'a> {
    let mut block = *card;

    for parent in card.ancestors_it(None) {
        let shared = cards
            .iter()
            .any(|other| other.id != card.id && inside(other, &parent));
        if shared {
            break;
        }
        block = parent;
    }

    block
}

fn inside(node: &NodeRef, ancestor: &NodeRef) -> bool {
    node.ancestors_it(None).any(|up| up.id == ancestor.id)
}

/// Развернуть адреса в вырезанном куске.
///
/// Правим копию, а не само дерево: правка на месте досталась бы и статье —
/// `Readability` разбирает тот же документ, — и якоря заголовков
/// (`[#section2](#section2)`) превратились бы в адреса страницы.
fn absolute_html(html: &str, base: &str) -> String {
    let doc = Document::from(html.to_string());
    let base = Url::parse(base).ok();

    for node in doc.select("a[href], img[src]").nodes() {
        let attr = match node.node_name().as_deref() {
            Some("a") => "href",
            _ => "src",
        };
        let Some(value) = node.attr(attr) else {
            continue;
        };
        if let Some(absolute) = absolute(base.as_ref(), &value) {
            node.set_attr(attr, &absolute);
        }
    }

    doc.html().to_string()
}

/// Заголовок, который целиком является ссылкой на другую страницу.
fn is_card(node: &NodeRef) -> bool {
    let text = node.text();
    let text = text.trim();
    if text.is_empty() {
        return false;
    }

    let links: Vec<NodeRef> = node
        .descendants()
        .into_iter()
        .filter(|node| node.node_name().as_deref() == Some("a") && node.has_attr("href"))
        .collect();
    let [link] = links.as_slice() else {
        return false;
    };

    let href = link.attr("href").unwrap_or_default();
    let href = href.trim();
    // Ссылка на якорь внутри страницы — это раздел статьи, а не карточка.
    !href.is_empty() && !href.starts_with('#') && link.text().trim() == text
}

/// Ближайший общий предок карточек.
fn common_ancestor<'a>(cards: &[NodeRef<'a>]) -> Option<NodeRef<'a>> {
    let chains: Vec<Vec<dom_query::NodeId>> = cards
        .iter()
        .map(|card| card.ancestors_it(None).map(|node| node.id).collect())
        .collect();

    cards
        .first()?
        .ancestors_it(None)
        .find(|node| chains.iter().all(|chain| chain.contains(&node.id)))
}

/// Текст без лишних пробелов: разметка ставит их как придётся, а сравнивать
/// приходится куски из разных мест дерева.
fn squeeze(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
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

    /// Абзац настоящей длины: извлечению нужен текст, иначе оно ничего
    /// не выбирает.
    const TEXT: &str = "Кеннет Эрроу доказал теорему о невозможности коллективного \
        выбора, и это перевернуло теорию общественного благосостояния. \
        Ниже разбирается, что именно утверждает теорема и почему её \
        следствия так неудобны для любой процедуры голосования.";

    /// Статья с картинкой, у которой рядом со `src` лежит посторонний
    /// атрибут с похожим на картинку значением.
    fn page(img: &str) -> String {
        let text = TEXT;
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

    /// Лента: десяток карточек, и Readability приносит из них одну.
    /// Правило должно узнать в этом ленту — проверяем само правило,
    /// а не то, какую из карточек выберет извлечение.
    #[test]
    fn a_feed_is_recognised_when_extraction_keeps_one_card() {
        let teaser = TEXT.repeat(2);
        let mut cards = String::new();
        for i in 1..=10 {
            cards.push_str(&format!(
                "<article><h2><a href=\"/post/{i}\">Запись номер {i}</a></h2><p>{teaser}</p></article>"
            ));
        }
        let html =
            format!("<html><body><main><div class=\"feed\">{cards}</div></main></body></html>");

        let listing =
            listing(&Document::from(html), "https://example.org/feed").expect("лента не опознана");
        // Извлечение принесло одну запись из десяти.
        assert!(listing.holds(&format!("<div><p>{teaser}</p></div>")));
        assert!(listing.html.contains("Запись номер 10"));
        // Адреса в ленте разворачиваем сами: мимо Readability это некому.
        assert!(listing.html.contains("https://example.org/post/3"));
    }

    /// Статья, к которой приложили витрину «читайте ещё», остаётся статьёй:
    /// её текст больше всей витрины.
    #[test]
    fn an_article_with_a_teaser_grid_is_not_a_feed() {
        let mut cards = String::new();
        for i in 1..=10 {
            cards.push_str(&format!(
                "<div><h3><a href=\"/post/{i}\">Другая статья {i}</a></h3><p>{TEXT}</p></div>"
            ));
        }
        let body = TEXT.repeat(12);
        let html = format!(
            "<html><body><main><article><h1>Статья</h1><p>{body}</p></article>\
             <div class=\"more\">{cards}</div></main></body></html>"
        );

        let listing =
            listing(&Document::from(html), "https://example.org/post").expect("витрина есть");
        assert!(
            !listing.holds(&format!("<article><p>{body}</p></article>")),
            "статью приняли за запись ленты"
        );
    }

    /// Меню и подвал внутри вырезанного означают, что общим предком карточек
    /// оказалась вся страница. Вырезать нечего.
    #[test]
    fn a_page_wide_region_is_not_a_feed() {
        let mut cards = String::new();
        for i in 1..=10 {
            cards.push_str(&format!(
                "<div><h3><a href=\"/post/{i}\">Запись {i}</a></h3><p>{TEXT}</p></div>"
            ));
        }
        let html = format!(
            "<html><body><div><h3><a href=\"/top\">Самое читаемое</a></h3>{cards}\
             <footer>Все права</footer></div></body></html>"
        );

        assert!(listing(&Document::from(html), "https://example.org/").is_none());
    }

    /// Аватар в ленте — значок, а не картинка: он один заполняет ссылку,
    /// объявлен мелким и без подписи.
    #[test]
    fn an_icon_filling_a_link_is_dropped() {
        let img = r#"<p><a href="https://example.org/users/kot"><img src="https://example.org/avatar.jpg" width="24" height="24" alt=""></a></p>"#;
        let article = extract(&page(img), "https://example.org/feed").unwrap();
        assert!(!article.content_html.contains("avatar.jpg"));
    }

    /// А та же мелкая картинка в строке текста — картинка: так tonsky
    /// ставит логотип apple посреди абзаца.
    #[test]
    fn a_small_picture_in_the_text_survives() {
        let img = r#"<p>и вы увидите <img src="https://example.org/logo.png" width="16" height="16" alt=""> вот это</p>"#;
        let article = extract(&page(img), "https://example.org/post").unwrap();
        assert!(article.content_html.contains("logo.png"));
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
