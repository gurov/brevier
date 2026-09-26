//! Лента: RSS или Atom, открытая как страница.
//!
//! Лента — это и есть «список ссылок», тот же вид, что у главной блога:
//! заголовок записи ссылкой, дата, пара строк подводки. Читатель открывает
//! ленту не читать её, а выбрать, куда пойти, — поэтому подводка короткая,
//! даже когда лента несёт статьи целиком, и порядок записей тот, что задал
//! автор.
//!
//! Разбор — настоящим XML-разборщиком (`roxmltree`), а не тем, что читает
//! HTML: у HTML `<link>` пустой элемент, и адрес записи RSS уехал бы из него
//! в соседний текст. Живые ленты при этом нередко не XML: `&nbsp;` без
//! объявления, голый `&` в заголовке. Такую ленту чиним и разбираем второй
//! раз — читатели лент прощают это все, и отказ тут был бы придиркой.
//!
//! Здесь же — ленты, которые страница объявила в шапке: их показывает полка
//! («This site has a feed»).

use std::collections::HashSet;

use roxmltree::{Document, Node, ParsingOptions};
use url::Url;

use crate::error::Error;
use crate::extract::Link;
use crate::outline::{clip, lead};
use crate::store::MONTHS;

/// Сколько знаков подводки оставляем записи. Две-три строки колонки: хватает
/// понять, о чём запись, и не хватает, чтобы лента стала книгой.
const SUMMARY_CHARS: usize = 280;
/// Описание самой ленты под её заголовком — того же порядка.
const ABOUT_CHARS: usize = 200;
/// Сколько объявленных лент показывать на полке. Больше трёх-четырёх
/// (сайт, комментарии, рубрика) не бывает, а дальше это уже свалка.
const MAX_ADVERTISED: usize = 4;
/// Потолок узлов разбора: тело и так не больше `fetch::MAX_BODY`, но разборщик
/// не должен строить дерево на миллионы узлов из восьми мегабайт мусора.
const NODES_LIMIT: u32 = 1_000_000;

const RSS1: &str = "http://purl.org/rss/1.0/";
const RSS09: &str = "http://my.netscape.com/rdf/simple/0.9/";
const DUBLIN_CORE: &str = "http://purl.org/dc/elements/1.1/";
const CONTENT: &str = "http://purl.org/rss/1.0/modules/content/";
const XML: &str = "http://www.w3.org/XML/1998/namespace";

/// Разобранная лента.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Feed {
    pub title: String,
    /// Сайт, чья это лента, — если лента его назвала.
    pub site: Option<String>,
    /// Описание ленты, простым текстом.
    pub about: Option<String>,
    pub entries: Vec<Entry>,
}

/// Запись ленты. Любое поле может отсутствовать: RSS разрешает запись
/// из одного описания, без заголовка и без ссылки.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Entry {
    pub title: Option<String>,
    pub link: Option<String>,
    /// Дата для показа: «9 September 2026», а если лента написала её
    /// не по стандарту — как написала.
    pub date: Option<String>,
    pub author: Option<String>,
    /// Подводка простым текстом, уже укороченная.
    pub summary: Option<String>,
}

/// Лента ли это — по первому элементу документа.
///
/// Нужен, потому что типу содержимого верить нельзя: ленту отдают
/// и `text/xml`, и `text/plain` (сырые файлы хостингов), и `text/html`,
/// а `application/xml` бывает чем угодно. Смотрим только начало: пролог,
/// комментарии и DOCTYPE пропускаем, первый элемент — `rss`, `feed`
/// или `rdf:RDF`. HTML-страница начинается с `html`, и сюда не попадёт.
pub fn is_feed(body: &str) -> bool {
    root_name(body).is_some_and(|name| {
        let local = name.rsplit(':').next().unwrap_or(name);
        matches!(local, "rss" | "feed" | "RDF")
    })
}

fn root_name(body: &str) -> Option<&str> {
    let mut rest = body.trim_start_matches('\u{feff}').trim_start();
    loop {
        if let Some(after) = rest.strip_prefix("<?") {
            rest = after.split_once("?>")?.1.trim_start();
        } else if let Some(after) = rest.strip_prefix("<!--") {
            rest = after.split_once("-->")?.1.trim_start();
        } else if let Some(after) = rest.strip_prefix("<!") {
            // DOCTYPE, возможно с внутренним подмножеством в квадратных скобках.
            let close = after.find('>')?;
            rest = match after.find('[') {
                Some(open) if open < close => after.split_once("]>")?.1,
                _ => &after[close + 1..],
            }
            .trim_start();
        } else {
            break;
        }
    }
    let name = rest.strip_prefix('<')?;
    let end = name
        .find(|ch: char| ch.is_whitespace() || ch == '>' || ch == '/')
        .unwrap_or(name.len());
    Some(&name[..end])
}

/// Текст ленты из байтов. Кодировку называет заголовок ответа, а если он
/// промолчал — объявление XML (`<?xml … encoding="koi8-r"?>`): у русских
/// лент старой школы это обычное дело, и `ureq` его не читает. Метка
/// порядка байтов главнее обоих.
pub fn decode(bytes: &[u8], charset: Option<&str>) -> String {
    let label = charset.map(str::to_owned).or_else(|| declared(bytes));
    let encoding = label
        .and_then(|label| encoding_rs::Encoding::for_label(label.trim().as_bytes()))
        .unwrap_or(encoding_rs::UTF_8);
    let (text, _, _) = encoding.decode(bytes);
    text.into_owned()
}

/// Кодировка из объявления XML — оно всегда в начале и всегда ASCII.
fn declared(bytes: &[u8]) -> Option<String> {
    let head = String::from_utf8_lossy(&bytes[..bytes.len().min(256)]);
    let declaration = head.split_once("<?xml")?.1.split_once("?>")?.0;
    let value = declaration.split_once("encoding")?.1.trim_start();
    let value = value.strip_prefix('=')?.trim_start();
    let quote = value
        .chars()
        .next()
        .filter(|ch| *ch == '"' || *ch == '\'')?;
    let value = &value[1..];
    Some(value[..value.find(quote)?].to_owned())
}

/// Разобрать ленту. `url` — откуда она пришла: от него разворачиваются
/// относительные адреса записей.
pub fn parse(xml: &str, url: &str) -> Result<Feed, Error> {
    let repaired: String;
    let doc = match Document::parse_with_options(xml, options()) {
        Ok(doc) => doc,
        Err(first) => {
            repaired = repair(xml);
            Document::parse_with_options(&repaired, options())
                .map_err(|_| Error::Feed(first.to_string()))?
        }
    };

    let root = doc.root_element();
    let base = base_of(root, Url::parse(url).ok());
    let channel = own(root, "channel").unwrap_or(root);
    let feed = match root.tag_name().name() {
        "rss" => rss(channel, channel, base.as_ref()),
        // RSS 1.0: записи — соседи канала, а не его дети.
        "RDF" => rss(channel, root, base.as_ref()),
        "feed" => atom(root, base.as_ref()),
        other => return Err(Error::Feed(format!("`<{other}>` is not RSS or Atom"))),
    };
    Ok(Feed {
        title: if feed.title.is_empty() {
            base.as_ref()
                .and_then(|url| url.host_str().map(str::to_owned))
                .unwrap_or_else(|| url.to_owned())
        } else {
            feed.title
        },
        ..feed
    })
}

fn options<'input>() -> ParsingOptions<'input> {
    ParsingOptions {
        // DOCTYPE носят ленты RSS 0.91. Против «миллиарда смешков» у разборщика
        // своя защита, запрет DTD — лишь запас, и ленту он бы отверг зря.
        allow_dtd: true,
        nodes_limit: NODES_LIMIT,
        ..ParsingOptions::default()
    }
}

/// Починить амперсанды: известные XML сущности и числовые ссылки оставить,
/// сущности HTML перевести в числа, остальное — буквальный `&`. Внутри
/// CDATA ничего не трогаем: там `&` законен.
fn repair(xml: &str) -> String {
    let mut out = String::with_capacity(xml.len() + 64);
    let mut rest = xml;
    while let Some(at) = rest.find(['&', '<']) {
        out.push_str(&rest[..at]);
        rest = &rest[at..];
        if let Some(after) = rest.strip_prefix("<![CDATA[") {
            let end = after.find("]]>").map_or(after.len(), |end| end + 3);
            out.push_str("<![CDATA[");
            out.push_str(&after[..end]);
            rest = &after[end..];
            continue;
        }
        if let Some(after) = rest.strip_prefix('<') {
            out.push('<');
            rest = after;
            continue;
        }
        let after = &rest[1..];
        let name = after
            .split_once(';')
            .map(|(name, _)| name)
            .filter(|name| name.len() <= 10 && !name.is_empty());
        match name {
            Some(name)
                if matches!(name, "amp" | "lt" | "gt" | "quot" | "apos")
                    || (name.starts_with('#') && name.len() > 1) =>
            {
                out.push('&');
            }
            Some(name) if html_entity(name).is_some() => {
                out.push_str(&format!("&#{};", html_entity(name).unwrap_or(' ') as u32));
                rest = &after[name.len() + 1..];
                continue;
            }
            _ => out.push_str("&amp;"),
        }
        rest = after;
    }
    out.push_str(rest);
    out
}

/// Сущности HTML, которые встречаются в лентах без объявления.
fn html_entity(name: &str) -> Option<char> {
    Some(match name {
        "nbsp" => '\u{a0}',
        "shy" => '\u{ad}',
        "mdash" => '—',
        "ndash" => '–',
        "hellip" => '…',
        "laquo" => '«',
        "raquo" => '»',
        "lsquo" => '‘',
        "rsquo" => '’',
        "ldquo" => '“',
        "rdquo" => '”',
        "bdquo" => '„',
        "copy" => '©',
        "reg" => '®',
        "trade" => '™',
        "deg" => '°',
        "times" => '×',
        "middot" => '·',
        "bull" => '•',
        "euro" => '€',
        _ => return None,
    })
}

/// RSS 2.0 и RSS 1.0. Разница в том, где лежат записи: у 2.0 — в канале,
/// у 1.0 — рядом с ним, поэтому их держатель назван отдельно.
fn rss(holder: Node, items: Node, base: Option<&Url>) -> Feed {
    let entries = items
        .children()
        .filter(|node| is_own(*node) && node.tag_name().name() == "item")
        .map(|item| rss_entry(item, base))
        .collect();

    Feed {
        title: own(holder, "title").map(text_of).unwrap_or_default(),
        site: own(holder, "link")
            .map(text_of)
            .and_then(|link| resolve(base, &link)),
        about: own(holder, "description")
            .map(|node| shorten(&html_text(&raw_text(node)), ABOUT_CHARS))
            .filter(|about| !about.is_empty()),
        entries,
    }
}

fn rss_entry(item: Node, base: Option<&Url>) -> Entry {
    let base = base_of(item, base.cloned());
    let base = base.as_ref();
    let link = own(item, "link")
        .map(text_of)
        .filter(|link| !link.is_empty())
        .or_else(|| {
            // `guid` — постоянный адрес, если лента не сказала обратного.
            own(item, "guid")
                .filter(|guid| guid.attribute("isPermaLink") != Some("false"))
                .map(text_of)
                .filter(|guid| guid.starts_with("http://") || guid.starts_with("https://"))
        })
        .and_then(|link| resolve(base, &link));
    let date = own(item, "pubDate")
        .or_else(|| module(item, DUBLIN_CORE, "date"))
        .map(|node| date_text(&text_of(node)));
    let author = module(item, DUBLIN_CORE, "creator")
        .or_else(|| own(item, "author"))
        .map(|node| author_text(&text_of(node)));
    let summary = own(item, "description")
        .or_else(|| module(item, CONTENT, "encoded"))
        .map(|node| shorten(&html_text(&raw_text(node)), SUMMARY_CHARS));

    Entry {
        title: own(item, "title").map(|node| plain_title(&raw_text(node))),
        link,
        date,
        author,
        summary,
    }
    .tidy()
}

/// Atom — 1.0 и старый 0.3: у него свои имена дат, а устройство то же.
fn atom(root: Node, base: Option<&Url>) -> Feed {
    let ns = root.tag_name().namespace();
    let child = |node, name| named(node, ns, name);
    let entries = root
        .children()
        .filter(|node| is_named(*node, ns, "entry"))
        .map(|entry| {
            let base = base_of(entry, base.cloned());
            let date = ["published", "updated", "issued", "modified"]
                .iter()
                .find_map(|name| child(entry, name))
                .map(|node| date_text(&text_of(node)));
            let summary = child(entry, "summary")
                .or_else(|| child(entry, "content"))
                .map(|node| shorten(&atom_text(node), SUMMARY_CHARS));
            Entry {
                title: child(entry, "title").map(|node| clean(&atom_text(node))),
                link: atom_link(entry, ns).and_then(|href| resolve(base.as_ref(), href)),
                date,
                author: child(entry, "author")
                    .and_then(|author| child(author, "name"))
                    .map(text_of),
                summary,
            }
            .tidy()
        })
        .collect();

    Feed {
        title: child(root, "title")
            .map(|node| clean(&atom_text(node)))
            .unwrap_or_default(),
        site: atom_link(root, ns).and_then(|href| resolve(base, href)),
        about: child(root, "subtitle")
            .or_else(|| child(root, "tagline"))
            .map(|node| shorten(&atom_text(node), ABOUT_CHARS))
            .filter(|about| !about.is_empty()),
        entries,
    }
}

/// Адрес записи Atom: ссылка `alternate` (или без `rel`, что то же самое);
/// из нескольких — та, что HTML.
fn atom_link<'a>(node: Node<'a, '_>, ns: Option<&str>) -> Option<&'a str> {
    let links: Vec<Node> = node
        .children()
        .filter(|child| is_named(*child, ns, "link"))
        .filter(|link| matches!(link.attribute("rel"), None | Some("alternate")))
        .collect();
    links
        .iter()
        .find(|link| {
            link.attribute("type")
                .is_some_and(|kind| kind.contains("html"))
        })
        .or(links.first())
        .and_then(|link| link.attribute("href"))
}

/// Текстовая конструкция Atom: `text` как есть, `html` — разметкой внутри
/// текста, `xhtml` — разметкой прямо в дереве.
fn atom_text(node: Node) -> String {
    match node.attribute("type") {
        Some("html") | Some("text/html") => html_text(&raw_text(node)),
        _ => clean(&raw_text(node)),
    }
}

/// Элемент самой ленты: без пространства имён (RSS 2.0) или в пространстве
/// RSS 1.0 и 0.9. `atom:link` внутри канала RSS — чужой, и его тут нет.
fn is_own(node: Node) -> bool {
    node.is_element() && matches!(node.tag_name().namespace(), None | Some(RSS1) | Some(RSS09))
}

fn own<'a, 'i>(node: Node<'a, 'i>, name: &str) -> Option<Node<'a, 'i>> {
    node.children()
        .find(|child| is_own(*child) && child.tag_name().name() == name)
}

/// Элемент модуля: `dc:creator`, `content:encoded`.
fn module<'a, 'i>(node: Node<'a, 'i>, ns: &str, name: &str) -> Option<Node<'a, 'i>> {
    named(node, Some(ns), name)
}

fn named<'a, 'i>(node: Node<'a, 'i>, ns: Option<&str>, name: &str) -> Option<Node<'a, 'i>> {
    node.children().find(|child| is_named(*child, ns, name))
}

fn is_named(node: Node, ns: Option<&str>, name: &str) -> bool {
    node.is_element() && node.tag_name().namespace() == ns && node.tag_name().name() == name
}

/// Весь текст узла, со всеми потомками, как есть.
fn raw_text(node: Node) -> String {
    node.descendants()
        .filter(|node| node.is_text())
        .filter_map(|node| node.text())
        .collect()
}

/// Текст узла одной строкой.
fn text_of(node: Node) -> String {
    clean(&raw_text(node))
}

/// Пробелы в одну строку, по одному.
fn clean(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Заголовок RSS — текст, но бывает и с разметкой внутри: её снимаем.
fn plain_title(text: &str) -> String {
    if text.contains('<') {
        html_text(text)
    } else {
        clean(text)
    }
}

/// Текст из HTML: разметку снять, между блоками поставить пробел — иначе
/// соседние абзацы слиплись бы в одно слово.
fn html_text(html: &str) -> String {
    if !html.contains('<') && !html.contains('&') {
        return clean(html);
    }
    let doc = dom_query::Document::fragment(html);
    let mut out = String::with_capacity(html.len());
    walk(&doc.root(), &mut out);
    clean(&out)
}

fn walk(node: &dom_query::NodeRef, out: &mut String) {
    for child in node.children() {
        if child.is_text() {
            out.push_str(&child.text());
            continue;
        }
        if !child.is_element() {
            continue;
        }
        let name = child.node_name().unwrap_or_default();
        match &*name {
            // Не текст вовсе.
            "script" | "style" | "noscript" | "template" | "head" => {}
            "p" | "div" | "br" | "li" | "ul" | "ol" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6"
            | "blockquote" | "pre" | "tr" | "td" | "th" | "table" | "section" | "article"
            | "figure" | "figcaption" | "hr" | "dd" | "dt" => {
                out.push(' ');
                walk(&child, out);
                out.push(' ');
            }
            _ => walk(&child, out),
        }
    }
}

/// Укоротить до `limit` знаков по границе слова.
fn shorten(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        return text.to_owned();
    }
    let cut: String = text.chars().take(limit).collect();
    let cut = match cut.rfind(char::is_whitespace) {
        Some(space) => &cut[..space],
        None => &cut,
    };
    format!(
        "{}…",
        cut.trim_end_matches(|ch: char| ch.is_whitespace() || ",;:—–-".contains(ch))
    )
}

/// Автор RSS пишется адресом почты: `jane@example.org (Jane Doe)` — имя
/// в скобках, если оно есть.
fn author_text(text: &str) -> String {
    match text.split_once('(') {
        Some((mail, name)) if mail.contains('@') => clean(name.trim_end().trim_end_matches(')')),
        _ => text.to_owned(),
    }
}

/// Дата для показа. RSS пишет её по RFC 822 (`Tue, 10 Jun 2003 04:00:00 GMT`),
/// Atom и Dublin Core — по ISO 8601 (`2003-12-13T18:30:02Z`). Показываем
/// день так же, как страница истории: «10 June 2003»; час в ленте ничего
/// не решает. Дату не по стандарту отдаём как написана — лучше, чем ничего.
pub fn date_text(raw: &str) -> String {
    match day_of(raw) {
        Some((year, month, day)) => format!("{day} {} {year}", MONTHS[month as usize - 1]),
        None => clip(raw.trim(), 40),
    }
}

fn day_of(raw: &str) -> Option<(i64, u32, u32)> {
    let raw = raw.trim();
    let (year, month, day) = if raw.len() >= 10 && raw.as_bytes()[4] == b'-' {
        (
            raw.get(..4)?.parse().ok()?,
            raw.get(5..7)?.parse().ok()?,
            raw.get(8..10)?.parse().ok()?,
        )
    } else {
        // RFC 822: день недели с запятой необязателен.
        let rest = raw.split_once(',').map_or(raw, |(_, rest)| rest);
        let mut parts = rest.split_whitespace();
        let day: u32 = parts.next()?.parse().ok()?;
        let month = parts.next()?.to_ascii_lowercase();
        let prefix = month.get(..3)?;
        let month = MONTHS
            .iter()
            .position(|name| name.to_ascii_lowercase().starts_with(prefix))?
            as u32
            + 1;
        let year: i64 = parts.next()?.parse().ok()?;
        // Двузначный год — из старого RFC 822.
        let year = match year {
            0..=49 => 2000 + year,
            50..=99 => 1900 + year,
            _ => year,
        };
        (year, month, day)
    };
    ((1..=12).contains(&month) && (1..=31).contains(&day)).then_some((year, month, day))
}

/// Адрес относительно ленты. Только http и https: `javascript:` в ленте
/// никуда не ведёт.
fn resolve(base: Option<&Url>, link: &str) -> Option<String> {
    let link = link.trim();
    let url = match base {
        Some(base) => base.join(link).ok()?,
        None => Url::parse(link).ok()?,
    };
    matches!(url.scheme(), "http" | "https").then(|| url.to_string())
}

/// `xml:base` на узле, если есть, — иначе то, что было.
fn base_of(node: Node, base: Option<Url>) -> Option<Url> {
    match node.attribute((XML, "base")) {
        Some(relative) => match &base {
            Some(base) => base.join(relative.trim()).ok(),
            None => Url::parse(relative.trim()).ok(),
        }
        .or(base),
        None => base,
    }
}

impl Entry {
    /// Пустые поля — в `None`: пустая строка на странице не нужна никому.
    fn tidy(self) -> Entry {
        let keep = |field: Option<String>| field.filter(|text| !text.trim().is_empty());
        Entry {
            title: keep(self.title),
            link: keep(self.link),
            date: keep(self.date),
            author: keep(self.author),
            summary: keep(self.summary),
        }
    }
}

/// Лента markdown-ом: заголовок, строка о ленте, дальше записи — заголовок
/// ссылкой, дата и автор курсивом, подводка абзацем. Записи — заголовки
/// второго уровня, поэтому полка показывает их оглавлением.
pub fn to_markdown(feed: &Feed) -> String {
    let mut out = format!("# {}\n\n", heading(&feed.title));

    let mut about = Vec::new();
    if let Some(text) = &feed.about {
        about.push(inline(text));
    }
    if let Some(site) = &feed.site {
        let name = Url::parse(site)
            .ok()
            .and_then(|url| url.host_str().map(str::to_owned))
            .unwrap_or_else(|| site.clone());
        about.push(format!("[{}]({})", inline(&name), destination(site)));
    }
    if !about.is_empty() {
        out.push_str(&format!("*{}*\n\n", about.join(" · ")));
    }
    if feed.entries.is_empty() {
        out.push_str("No entries in this feed yet.\n");
    }

    for entry in &feed.entries {
        let title = entry
            .title
            .clone()
            .or_else(|| entry.summary.as_deref().map(lead))
            .or_else(|| entry.date.clone())
            .unwrap_or_else(|| "Untitled".to_owned());
        match &entry.link {
            Some(link) => out.push_str(&format!(
                "## [{}]({})\n\n",
                heading(&title),
                destination(link)
            )),
            None => out.push_str(&format!("## {}\n\n", heading(&title))),
        }

        let meta: Vec<String> = [&entry.date, &entry.author]
            .into_iter()
            .flatten()
            .map(|text| inline(text))
            .collect();
        if !meta.is_empty() {
            out.push_str(&format!("*{}*\n\n", meta.join(" · ")));
        }
        // Подводка без заголовка уже стала заголовком — дважды её не пишем.
        if let Some(summary) = &entry.summary
            && entry.title.is_some()
        {
            out.push_str(&block(summary));
            out.push_str("\n\n");
        }
    }
    out.truncate(out.trim_end().len());
    out.push('\n');
    out
}

/// Экранировать то, что markdown принял бы за разметку внутри строки.
/// Текст ленты пишет не наш код: звёздочка в заголовке — звёздочка.
fn inline(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 8);
    for ch in text.chars() {
        if matches!(
            ch,
            '\\' | '`' | '*' | '_' | '[' | ']' | '<' | '>' | '&' | '~' | '|'
        ) {
            out.push('\\');
        }
        out.push(ch);
    }
    out
}

/// То же для абзаца: вдобавок начало строки не должно стать списком,
/// цитатой или заголовком.
fn block(text: &str) -> String {
    let text = inline(text);
    let digits = text.chars().take_while(char::is_ascii_digit).count();
    if digits > 0 && matches!(text[digits..].chars().next(), Some('.' | ')')) {
        return format!("{}\\{}", &text[..digits], &text[digits..]);
    }
    match text.chars().next() {
        Some('#' | '+' | '-' | '=') => format!("\\{text}"),
        _ => text,
    }
}

/// Текст заголовка: `#` в конце ATX-заголовок съел бы как закрывающий.
fn heading(text: &str) -> String {
    let text = block(text);
    match text.strip_suffix('#') {
        Some(rest) => format!("{rest}\\#"),
        None => text,
    }
}

/// Адрес ссылки: в угловых скобках, если в нём то, что оборвало бы ссылку.
fn destination(url: &str) -> String {
    if url.contains(['(', ')', ' ', '<', '>']) {
        format!("<{}>", url.replace('<', "%3C").replace('>', "%3E"))
    } else {
        url.to_owned()
    }
}

/// Ленты, объявленные в шапке страницы: `<link rel="alternate"
/// type="application/rss+xml" href title>`. Подпись — `title`, а без него
/// родом ленты. Собственные ленты сайта, ленты комментариев, рубрик —
/// всё, что страница сама назвала, в том порядке, в каком назвала.
pub fn advertised(doc: &dom_query::Document, url: &str) -> Vec<Link> {
    let base = Url::parse(url).ok();
    let mut seen: HashSet<String> = HashSet::new();
    let mut feeds = Vec::new();

    for node in doc.select("link[rel][type][href]").nodes() {
        if feeds.len() >= MAX_ADVERTISED {
            break;
        }
        let rel = node.attr("rel").unwrap_or_default();
        if !rel
            .split_whitespace()
            .any(|token| token.eq_ignore_ascii_case("alternate"))
        {
            continue;
        }
        let kind = node.attr("type").unwrap_or_default().to_ascii_lowercase();
        let fallback = match kind.split(';').next().unwrap_or_default().trim() {
            "application/rss+xml" => "RSS feed",
            "application/atom+xml" => "Atom feed",
            _ => continue,
        };
        let Some(address) = node
            .attr("href")
            .and_then(|href| resolve(base.as_ref(), &href))
        else {
            continue;
        };
        if !seen.insert(address.clone()) {
            continue;
        }
        let title = node
            .attr("title")
            .map(|title| clean(&title))
            .filter(|title| !title.is_empty())
            .unwrap_or_else(|| fallback.to_owned());
        feeds.push(Link { title, address });
    }
    feeds
}

#[cfg(test)]
mod tests {
    use super::*;

    const RSS: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<rss version="2.0" xmlns:atom="http://www.w3.org/2005/Atom"
     xmlns:dc="http://purl.org/dc/elements/1.1/">
<channel>
  <title>Example Blog</title>
  <link>https://example.org/</link>
  <atom:link href="https://example.org/feed.xml" rel="self" type="application/rss+xml"/>
  <description>Notes on &lt;b&gt;things&lt;/b&gt;</description>
  <item>
    <title>First *post*</title>
    <link>/2026/first</link>
    <pubDate>Tue, 10 Jun 2003 04:00:00 GMT</pubDate>
    <dc:creator>Jane Doe</dc:creator>
    <description><![CDATA[<p>One paragraph.</p><p>Two &amp; three.</p>]]></description>
  </item>
  <item>
    <description>Only a description here, and it is long enough to become a title of its own.</description>
    <guid>https://example.org/2026/second</guid>
    <author>jane@example.org (Jane Doe)</author>
  </item>
</channel>
</rss>"#;

    const ATOM: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<feed xmlns="http://www.w3.org/2005/Atom" xml:base="https://example.com/blog/">
  <title type="html">A &lt;i&gt;title&lt;/i&gt;</title>
  <subtitle>Words</subtitle>
  <link href="https://example.com/blog/feed.atom" rel="self"/>
  <link href="https://example.com/blog/"/>
  <entry>
    <title>Entry one</title>
    <link rel="alternate" type="text/html" href="one.html"/>
    <link rel="replies" href="one/comments.atom"/>
    <updated>2026-09-25T10:00:00Z</updated>
    <published>2026-09-24T09:00:00+03:00</published>
    <author><name>Ann</name></author>
    <content type="xhtml"><div xmlns="http://www.w3.org/1999/xhtml"><p>Hello <b>there</b>.</p></div></content>
  </entry>
</feed>"#;

    #[test]
    fn a_feed_is_known_by_its_first_element() {
        assert!(is_feed(RSS));
        assert!(is_feed(ATOM));
        assert!(is_feed(
            "\u{feff}<?xml version=\"1.0\"?>\n<!-- hi -->\n<rdf:RDF xmlns:rdf=\"x\"></rdf:RDF>"
        ));
        assert!(is_feed(
            "<!DOCTYPE rss PUBLIC \"-//Netscape Communications//DTD RSS 0.91//EN\" \
             \"http://my.netscape.com/publish/formats/rss-0.91.dtd\">\n<rss version=\"0.91\">"
        ));
        assert!(!is_feed("<!DOCTYPE html>\n<html><head><title>x</title>"));
        assert!(!is_feed("<?xml version=\"1.0\"?><svg xmlns=\"x\"/>"));
        assert!(!is_feed("# Markdown\n\ntext"));
    }

    #[test]
    fn rss_entries_carry_links_dates_and_summaries() {
        let feed = parse(RSS, "https://example.org/feed.xml").unwrap();
        assert_eq!(feed.title, "Example Blog");
        assert_eq!(feed.site.as_deref(), Some("https://example.org/"));
        assert_eq!(feed.about.as_deref(), Some("Notes on things"));
        assert_eq!(feed.entries.len(), 2);

        let first = &feed.entries[0];
        assert_eq!(first.title.as_deref(), Some("First *post*"));
        // Относительный адрес — от адреса ленты.
        assert_eq!(
            first.link.as_deref(),
            Some("https://example.org/2026/first")
        );
        assert_eq!(first.date.as_deref(), Some("10 June 2003"));
        assert_eq!(first.author.as_deref(), Some("Jane Doe"));
        assert_eq!(
            first.summary.as_deref(),
            Some("One paragraph. Two & three.")
        );

        // Без заголовка и ссылки: адрес из `guid`, автор из почты.
        let second = &feed.entries[1];
        assert_eq!(second.title, None);
        assert_eq!(
            second.link.as_deref(),
            Some("https://example.org/2026/second")
        );
        assert_eq!(second.author.as_deref(), Some("Jane Doe"));
    }

    #[test]
    fn atom_takes_the_html_link_and_the_published_date() {
        let feed = parse(ATOM, "https://example.com/feed").unwrap();
        assert_eq!(feed.title, "A title");
        assert_eq!(feed.site.as_deref(), Some("https://example.com/blog/"));
        assert_eq!(feed.about.as_deref(), Some("Words"));
        let entry = &feed.entries[0];
        // Относительный адрес — от `xml:base`, а не от адреса ленты.
        assert_eq!(
            entry.link.as_deref(),
            Some("https://example.com/blog/one.html")
        );
        assert_eq!(entry.date.as_deref(), Some("24 September 2026"));
        assert_eq!(entry.author.as_deref(), Some("Ann"));
        assert_eq!(entry.summary.as_deref(), Some("Hello there."));
    }

    #[test]
    fn rss_1_keeps_its_items_beside_the_channel() {
        let rdf = r#"<?xml version="1.0"?>
<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"
         xmlns="http://purl.org/rss/1.0/" xmlns:dc="http://purl.org/dc/elements/1.1/">
  <channel rdf:about="https://e.org/"><title>Old school</title><link>https://e.org/</link></channel>
  <item rdf:about="https://e.org/1"><title>One</title><link>https://e.org/1</link>
    <dc:date>2004-02-29T10:00:00Z</dc:date></item>
</rdf:RDF>"#;
        let feed = parse(rdf, "https://e.org/index.rdf").unwrap();
        assert_eq!(feed.title, "Old school");
        assert_eq!(feed.entries.len(), 1);
        assert_eq!(feed.entries[0].date.as_deref(), Some("29 February 2004"));
    }

    #[test]
    fn a_feed_that_is_not_quite_xml_is_repaired() {
        let broken = "<rss><channel><title>AT&T&nbsp;news &mdash; daily</title>\
            <item><title>Q&A</title><link>https://e.org/a?x=1&y=2</link></item>\
            <item><title><![CDATA[Fish & chips]]></title></item></channel></rss>";
        let feed = parse(broken, "https://e.org/rss").unwrap();
        // Неразрывный пробел в заголовке стал обычным: строка одна, и держать
        // её вместе незачем.
        assert_eq!(feed.title, "AT&T news — daily");
        assert_eq!(feed.entries[0].title.as_deref(), Some("Q&A"));
        assert_eq!(
            feed.entries[0].link.as_deref(),
            Some("https://e.org/a?x=1&y=2")
        );
        assert_eq!(feed.entries[1].title.as_deref(), Some("Fish & chips"));

        assert!(matches!(
            parse("<rss><channel>", "https://e.org/"),
            Err(Error::Feed(_))
        ));
        assert!(matches!(
            parse("<html></html>", "https://e.org/"),
            Err(Error::Feed(_))
        ));
    }

    #[test]
    fn the_encoding_comes_from_the_header_or_the_declaration() {
        let koi8 = b"<?xml version=\"1.0\" encoding=\"koi8-r\"?><rss><channel><title>\xf0\xd2\xc9\xd7\xc5\xd4</title></channel></rss>";
        let text = decode(koi8, None);
        assert!(text.contains("Привет"), "{text}");
        // Заголовок ответа главнее объявления.
        let text = decode(
            "<?xml version=\"1.0\" encoding=\"koi8-r\"?><r>й</r>".as_bytes(),
            Some("utf-8"),
        );
        assert!(text.contains('й'));
    }

    #[test]
    fn dates_read_both_standards_and_keep_the_rest_as_written() {
        assert_eq!(date_text("Wed, 02 Oct 2002 13:00:00 GMT"), "2 October 2002");
        assert_eq!(date_text("2 Oct 02 13:00 +0000"), "2 October 2002");
        assert_eq!(date_text("2026-09-26"), "26 September 2026");
        assert_eq!(date_text("вчера в 13:42"), "вчера в 13:42");
    }

    #[test]
    fn markdown_escapes_what_the_feed_wrote() {
        let feed = parse(RSS, "https://example.org/feed.xml").unwrap();
        let markdown = to_markdown(&feed);
        assert!(markdown.starts_with(
            "# Example Blog\n\n*Notes on things · [example.org](https://example.org/)*\n\n"
        ));
        assert!(markdown.contains("## [First \\*post\\*](https://example.org/2026/first)\n\n*10 June 2003 · Jane Doe*\n\nOne paragraph. Two \\& three."));
        // Запись без заголовка получает начало подводки — и подводка второй раз
        // не печатается.
        assert!(markdown.contains("## [Only a description here, and it is long…](https://example.org/2026/second)\n\n*Jane Doe*"));
        assert!(!markdown.contains("become a title"));
        assert!(markdown.ends_with("*Jane Doe*\n"));

        assert_eq!(block("1. not a list"), "1\\. not a list");
        assert_eq!(block("- not a list"), "\\- not a list");
        assert_eq!(heading("C#"), "C\\#");
        assert_eq!(destination("https://e.org/a (b)"), "<https://e.org/a (b)>");
    }

    #[test]
    fn a_page_advertises_its_feeds_in_the_head() {
        let doc = dom_query::Document::from(
            r#"<html><head>
            <link rel="alternate" type="application/rss+xml" title="Blog » Feed" href="/feed/">
            <link rel="alternate" type="application/atom+xml" href="https://e.org/atom.xml">
            <link rel="alternate" type="application/rss+xml" href="/feed/">
            <link rel="alternate" type="text/markdown" href="/post.md">
            <link rel="stylesheet" type="text/css" href="/s.css">
            </head><body></body></html>"#,
        );
        let feeds = advertised(&doc, "https://e.org/post");
        assert_eq!(
            feeds,
            vec![
                Link {
                    title: "Blog » Feed".to_owned(),
                    address: "https://e.org/feed/".to_owned()
                },
                Link {
                    title: "Atom feed".to_owned(),
                    address: "https://e.org/atom.xml".to_owned()
                },
            ]
        );
    }
}
