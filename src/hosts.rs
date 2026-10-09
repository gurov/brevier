//! Правила под хост — одной небольшой таблицей.
//!
//! Решено роадмапом («Per-host rules exist, in one small table»): для
//! открытого веба правила пишутся по форме, а треды и хосты документации
//! получают именованные правила, у каждого — страницы в проверке. Системы
//! плагинов вокруг нет и не будет: строка таблицы — функция здесь.
//!
//! Сейчас в таблице reddit, поисковик, читалка книг royallib и Wayback Machine. Страницы reddit без JavaScript пусты
//! (8 КБ заглушки), `old.reddit.com` уводит на вход, `.json` отвечает 403 —
//! а лента `.rss` у сабреддита и у треда открыта любому клиенту, с автором
//! и полным текстом каждой реплики. Цена — вложенности ответов в ленте нет,
//! и жёсткий лимит: второй запрос за несколько секунд получает 429.
//!
//! Поисковик — DuckDuckGo Lite (#25): выдача простым HTML по GET, без
//! скриптов и без ключа. Из проверенных 28 сентября 2026 он один отдаёт
//! хорошую выдачу, в том числе по-русски: Google требует JavaScript,
//! обычный DuckDuckGo — POST, у Bing ссылки завёрнуты в свой счётчик,
//! а у JSON-поисковиков либо ключ с картой, либо маленький индекс.
//! Выдача — таблица, и обычным трактом десять результатов слипаются в одну
//! строку, поэтому её разбирает правило ниже.
//!
//! Читалка royallib (#20): в страницу сервер кладёт только первую порцию
//! книги, остальные листает скрипт, забирая `/br.php?i=<книга>&pg=<n>`
//! простым GET, без кук. Порции те же, что у сайта, и каждая — своя
//! страница Brevier: `…/книга.html?part=3`. Сервер параметра не замечает
//! и отдаёт читалку как есть, а правило кладёт в неё третью порцию и строку
//! «Part 3 of 5» со ссылками на соседние. Книгу целиком не тянем: у романа
//! это сотни запросов, а прочтут, может быть, одну главу. Зато у каждой
//! порции своё место в истории, своё место чтения и своя недельная копия.

use dom_query::Document;
use url::Url;

/// Хосты reddit, чьи страницы читаются через ленту.
const REDDIT: &[&str] = &[
    "reddit.com",
    "www.reddit.com",
    "old.reddit.com",
    "new.reddit.com",
    "np.reddit.com",
];

/// Адрес, которым страницу читать вместо неё самой, — если хост в таблице.
/// Адрес во вкладке и в истории остаётся тем, что открывали.
pub fn feed_for(url: &str) -> Option<String> {
    let parsed = Url::parse(url).ok()?;
    let host = parsed.host_str()?.to_ascii_lowercase();
    if !REDDIT.contains(&host.as_str()) {
        return None;
    }
    let path = parsed.path().trim_end_matches('/');
    let parts: Vec<&str> = path.split('/').filter(|part| !part.is_empty()).collect();
    // Сабреддит (`/r/rust`) и тред в нём (`/r/rust/comments/<id>/…`).
    // Лента уже лентой — `/r/rust/.rss` — идёт своим путём.
    let wanted = matches!(parts.as_slice(), ["r", _] | ["r", _, "comments", _, ..]);
    (wanted && !path.ends_with(".rss")).then(|| format!("https://www.reddit.com{path}/.rss"))
}

/// Тело реплики без обвязки хоста. reddit кладёт текст автора в `div.md`,
/// а за ним — «submitted by /u/… [link] [comments]»: это подпись, которую
/// тред и так показывает строкой над репликой. У поста-ссылки текста нет
/// вовсе, есть миниатюра и та же подпись; тогда остаётся сама ссылка.
pub fn reply_html(html: &str) -> String {
    if !html.contains("class=\"md\"") && !html.contains("submitted by") {
        return html.to_owned();
    }
    let doc = dom_query::Document::fragment(html);
    let text = doc.select("div.md");
    if text.exists() {
        return text.first().inner_html().to_string();
    }
    for node in doc.select("a[href]").nodes() {
        if node.text().trim() == "[link]"
            && let Some(href) = node.attr("href")
        {
            return format!("<p><a href=\"{href}\">{href}</a></p>");
        }
    }
    html.to_owned()
}

/// Хост читалки книг, чьи порции правило собирает само.
const ROYALLIB: &str = "royallib.com";

/// Номер порции книги (с нуля), если адрес — читалка royallib. В адресе
/// номер с единицы, как его видит читатель: `?part=2` — вторая порция,
/// без параметра — первая.
pub fn book_part(url: &str) -> Option<usize> {
    let parsed = Url::parse(url).ok()?;
    let host = parsed.host_str()?.to_ascii_lowercase();
    if host.trim_start_matches("www.") != ROYALLIB || !parsed.path().starts_with("/read/") {
        return None;
    }
    let part = parsed
        .query_pairs()
        .find(|(key, _)| key == "part")
        .and_then(|(_, value)| value.parse::<usize>().ok())
        .unwrap_or(1);
    Some(part.saturating_sub(1))
}

/// Книга в читалке: её номер у сайта и границы порций.
#[derive(Debug, PartialEq, Eq)]
pub struct Book {
    id: String,
    /// Границы порций, как их считает сайт: порция `n` — от `bounds[n]`
    /// до `bounds[n + 1]`. Пусто — карты в странице не было.
    bounds: Vec<u64>,
    /// Текст первой порции сервер положил в страницу сам (абзацы
    /// в `#contentDiv`); до конца сентября 2026 не клал.
    pub served: bool,
}

impl Book {
    /// Номер книги (`#bid`) и карта порций (`rlServerMap`) из страницы
    /// читалки. Без номера это не читалка, и правило не нужно.
    pub fn of(html: &str) -> Option<Book> {
        let doc = Document::from(html);
        let id = squeeze(&doc.select("#bid").text());
        if id.is_empty() || !id.chars().all(|c| c.is_ascii_digit()) {
            return None;
        }
        let bounds = html
            .find("rlServerMap")
            .map(|at| bounds_of(&html[at..]))
            .unwrap_or_default();
        Some(Book {
            id,
            bounds,
            served: doc.select("#contentDiv p").exists(),
        })
    }

    /// Сколько порций; без карты — ноль.
    pub fn parts(&self) -> usize {
        self.bounds.len().saturating_sub(1)
    }

    /// Карта порций, если её не было в странице: так её спрашивает и скрипт сайта.
    pub fn map_url(&self, url: &str) -> Option<String> {
        let mut map = Url::parse(url).ok()?.join("/br.php").ok()?;
        map.query_pairs_mut()
            .append_pair("i", &self.id)
            .append_pair("map", "1");
        Some(map.into())
    }

    /// Принять карту из ответа `map_url` — JSON-массив чисел.
    pub fn set_map(&mut self, json: &str) {
        self.bounds = bounds_of(json);
    }

    /// Адрес порции `part` (с нуля) у сайта.
    pub fn part_url(&self, url: &str, part: usize) -> Option<String> {
        let mut text = Url::parse(url).ok()?.join("/br.php").ok()?;
        text.query_pairs_mut()
            .append_pair("i", &self.id)
            .append_pair("pg", &part.to_string());
        Some(text.into())
    }
}

/// Числа первого `[…]` в тексте: `rlServerMap = [0,20277,40749];`.
fn bounds_of(text: &str) -> Vec<u64> {
    let Some(open) = text.find('[') else {
        return Vec::new();
    };
    let Some(close) = text[open..].find(']') else {
        return Vec::new();
    };
    let numbers: Option<Vec<u64>> = text[open + 1..open + close]
        .split(',')
        .map(|number| number.trim().parse().ok())
        .collect();
    numbers.unwrap_or_default()
}

/// Страница читалки с порцией `part` (с нуля): текст порции — внутрь
/// `#contentDiv`, на место того, что положил туда сервер, а под ним строка
/// «Part 2 of 5» со ссылками на соседние порции. `text` — `None`, когда
/// порция в странице уже есть (первая, сервер кладёт её сам).
pub fn book_page(html: &str, url: &str, book: &Book, part: usize, text: Option<&str>) -> String {
    let doc = Document::from(html);
    let content = doc.select("#contentDiv");
    if !content.exists() {
        return html.to_owned();
    }
    if let Some(text) = text {
        content.set_html(text);
    }
    let parts = book.parts();
    if parts > 1 {
        let mut line = format!("Part {} of {parts}", part + 1);
        if part > 0 {
            line += &format!(
                " · <a href=\"{}\">Previous part</a>",
                part_address(url, part - 1)
            );
        }
        if part + 1 < parts {
            line += &format!(
                " · <a href=\"{}\">Next part</a>",
                part_address(url, part + 1)
            );
        }
        content.append_html(format!("<p>{line}</p>"));
    }
    doc.html().to_string()
}

/// Адрес порции у Brevier: первая — сама страница читалки, остальные —
/// с `?part=`, номер с единицы.
fn part_address(url: &str, part: usize) -> String {
    let Ok(mut address) = Url::parse(url) else {
        return url.to_owned();
    };
    address.set_fragment(None);
    address.set_query(None);
    if part > 0 {
        address
            .query_pairs_mut()
            .append_pair("part", &(part + 1).to_string());
    }
    address.into()
}

/// Где искать: DuckDuckGo Lite. Страница выдачи — по GET, запрос в `q`.
const SEARCH: &str = "https://lite.duckduckgo.com/lite/";

/// Адрес выдачи по запросу.
pub fn search_url(query: &str) -> String {
    let mut url = Url::parse(SEARCH).expect("адрес поисковика разбирается");
    url.query_pairs_mut().append_pair("q", query.trim());
    url.into()
}

/// Wayback Machine (#9): снимки страниц.
const WAYBACK: &str = "https://web.archive.org/web/";

/// Есть ли у Wayback страница (Availability API): ближайший снимок, живой.
const WAYBACK_AVAILABLE: &str = "https://archive.org/wayback/available";

/// Ответ Availability API — один снимок, а не список: килобайта хватает.
const MAX_AVAILABLE: u64 = 64 * 1024;

/// Список снимков Wayback (CDX): кто сохранён, когда и с каким ответом.
const WAYBACK_CDX: &str = "https://web.archive.org/cdx/search/cdx";

/// Ответ CDX — одна строка (`limit=-1`).
const MAX_CDX: u64 = 64 * 1024;

/// Сколько ждать CDX. У страницы с тысячами снимков он отвечал 16–50 секунд
/// (Google Reader, 9 октября 2026) — дольше обычных 30 на запрос; ждёт
/// читатель, который сам нажал кнопку, и ложное «копии нет» хуже ожидания.
const CDX_PATIENCE: std::time::Duration = std::time::Duration::from_secs(90);

/// Есть ли смысл искать страницу у Wayback; ответ — её адрес без решётки.
///
/// Не для всего: выдача поиска, сам архив и машина без имени в сети
/// (`localhost`, адрес IP, имя без точки) снимков у Wayback не имеют.
pub fn for_wayback(url: &str) -> Option<String> {
    let mut parsed = Url::parse(url).ok()?;
    if !matches!(parsed.scheme(), "http" | "https") || search_query(url).is_some() {
        return None;
    }
    let url::Host::Domain(host) = parsed.host()? else {
        return None;
    };
    let host = host.to_lowercase();
    if !host.contains('.') || host == "archive.org" || host.ends_with(".archive.org") {
        return None;
    }
    parsed.set_fragment(None);
    Some(parsed.into())
}

/// Последний снимок страницы, который сохранил её саму, а не отказ.
///
/// `web.archive.org/web/<адрес>` ведёт на последний снимок любой, а у мёртвой
/// страницы это обычно снимок её 404: обходчик архива заходит по старым
/// ссылкам и прилежно сохраняет отказ (у Google Reader на 9 октября 2026 —
/// снимок 404 того же утра). Поэтому спрашиваем живой снимок, и по нажатию
/// читателя, не раньше.
///
/// Сначала Availability API: отвечает за секунду и снимки отказов
/// пропускает, но ненадёжен — тот же вопрос через полчаса получил пустой
/// ответ, трижды подряд (9 октября 2026). Пусто или отказ — список снимков
/// (CDX) с фильтром по ответу 200: точно, но медленно (`CDX_PATIENCE`).
///
/// Снимок открывается видом без `id_`: ссылки и картинки в нём Wayback
/// переписал на свои снимки, и сайт, которого нет, читается внутри архива,
/// а не по мёртвым адресам. Текст статьи в обоих видах один и тот же —
/// сверено 9 октября 2026; тулбар, который Wayback вставляет в страницу,
/// вырезает извлечение (`without_wayback_toolbar`).
pub fn wayback_latest(url: &str, ua: crate::fetch::UserAgent) -> Result<String, crate::Error> {
    let available =
        crate::fetch::binary(&available_query(url), ua, "application/json", MAX_AVAILABLE);
    if let Ok(blob) = available
        && let Some(snapshot) = available_snapshot(&String::from_utf8_lossy(&blob.bytes))
    {
        return Ok(snapshot);
    }
    let cdx =
        crate::fetch::binary_within(&cdx_query(url), ua, "text/plain", MAX_CDX, CDX_PATIENCE)?;
    latest_snapshot(&String::from_utf8_lossy(&cdx.bytes)).ok_or(crate::Error::NotInWayback)
}

fn available_query(url: &str) -> String {
    let mut query = Url::parse(WAYBACK_AVAILABLE).expect("адрес Availability API разбирается");
    query.query_pairs_mut().append_pair("url", url);
    query.into()
}

/// Запрос к CDX: последний снимок с ответом 200, время и адрес как сохранён.
fn cdx_query(url: &str) -> String {
    let mut query = Url::parse(WAYBACK_CDX).expect("адрес CDX разбирается");
    query
        .query_pairs_mut()
        .append_pair("url", url)
        .append_pair("filter", "statuscode:200")
        .append_pair("fl", "timestamp,original")
        .append_pair("limit", "-1");
    query.into()
}

/// Адрес снимка из ответа CDX: строки «время адрес», последняя — свежая.
fn latest_snapshot(cdx: &str) -> Option<String> {
    cdx.lines().rev().find_map(|line| {
        let (stamp, original) = line.trim().split_once(' ')?;
        (stamp.len() >= 8 && stamp.bytes().all(|byte| byte.is_ascii_digit()))
            .then(|| format!("{WAYBACK}{stamp}/{original}"))
    })
}

/// Адрес снимка из ответа: только живой снимок, с ответом 2xx.
fn available_snapshot(json: &str) -> Option<String> {
    let root = crate::json::parse(json)?;
    let closest = root.get("archived_snapshots")?.get("closest")?;
    if closest.get("available") != Some(&crate::json::Value::Bool(true))
        || !closest.text("status")?.starts_with('2')
    {
        return None;
    }
    let address = closest.text("url")?;
    Some(match address.strip_prefix("http://web.archive.org/") {
        Some(rest) => format!("https://web.archive.org/{rest}"),
        None => address.to_owned(),
    })
}

/// Где у Wayback все снимки страницы: его календарь — то, что увидит
/// браузер читателя.
pub fn wayback_calendar(url: &str) -> String {
    format!("{WAYBACK}*/{url}")
}

/// Снимок Wayback без тулбара, который архив вставляет в страницу между
/// своими комментариями. Его строка «The Wayback Machine - <адрес>» видна
/// только при печати, а Readability у короткой страницы брал её в текст.
pub fn without_wayback_toolbar(html: &str) -> std::borrow::Cow<'_, str> {
    const BEGIN: &str = "<!-- BEGIN WAYBACK TOOLBAR INSERT -->";
    const END: &str = "<!-- END WAYBACK TOOLBAR INSERT -->";
    let Some(start) = html.find(BEGIN) else {
        return std::borrow::Cow::Borrowed(html);
    };
    let Some(length) = html[start..].find(END) else {
        return std::borrow::Cow::Borrowed(html);
    };
    let end = start + length + END.len();
    std::borrow::Cow::Owned(format!("{}{}", &html[..start], &html[end..]))
}

/// Снимок ли это Wayback, и если да — чей и от какого дня. Строка над
/// снимком говорит об этом так же, как над своей копией из архива: снимок —
/// не страница. Узнаётся по адресу, поэтому строка есть и у снимка, открытого
/// ссылкой внутри Wayback, из истории или из недельной копии.
pub fn wayback_snapshot(url: &str) -> Option<crate::Archived> {
    let rest = url
        .strip_prefix(WAYBACK)
        .or_else(|| url.strip_prefix("http://web.archive.org/web/"))?;
    let (stamp, original) = rest.split_once('/')?;
    let digits: String = stamp.chars().take_while(char::is_ascii_digit).collect();
    if digits.len() < 8 || !original.starts_with("http") {
        return None;
    }
    let year = &digits[..4];
    let month: usize = digits[4..6].parse().ok()?;
    let day: u32 = digits[6..8].parse().ok()?;
    let month = crate::store::MONTHS.get(month.checked_sub(1)?)?;
    Some(crate::Archived {
        source: original.split('#').next().unwrap_or_default().to_owned(),
        read: format!("{day} {month} {year}"),
        wayback: true,
    })
}

/// Запрос, если адрес — выдача нашего поисковика. По нему же страница
/// получает заголовок, и выдача узнаётся, откуда бы адрес ни пришёл:
/// из адресной строки, истории или сессии.
pub fn search_query(url: &str) -> Option<String> {
    let parsed = Url::parse(url).ok()?;
    let ours = Url::parse(SEARCH).ok()?;
    if parsed.host_str()? != ours.host_str()? || parsed.path().trim_end_matches('/') != "/lite" {
        return None;
    }
    parsed
        .query_pairs()
        .find(|(key, _)| key == "q")
        .map(|(_, value)| value.trim().to_owned())
        .filter(|query| !query.is_empty())
}

/// Один результат выдачи.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hit {
    pub title: String,
    /// Адрес самой страницы, уже без счётчика поисковика.
    pub url: String,
    /// Подводка — как её дал поисковик, без выделения слов запроса.
    pub snippet: String,
    /// Адрес, как его показывает выдача: хост и путь, без схемы.
    pub shown: String,
    /// Дата, если выдача её знает (`2026-08-04`).
    pub date: Option<String>,
}

/// Что пришло в ответ на запрос.
#[derive(Debug, PartialEq, Eq)]
pub enum Results {
    Hits(Vec<Hit>),
    /// Поисковик ничего не нашёл.
    Nothing,
    /// Поисковик принял нас за бота и показал загадку. Решать её мы
    /// не станем: это работа для браузера читателя.
    Challenge,
}

/// Разобрать страницу выдачи DuckDuckGo Lite.
///
/// Результат — строка таблицы со ссылкой `a.result-link`, за ней строки
/// с подводкой (`td.result-snippet`), показанным адресом (`span.link-text`)
/// и иногда датой (`span.timestamp`) — до следующей ссылки. Реклама ведёт
/// не через `uddg`, а через `y.js`, и отсеивается на развороте ссылки.
pub fn results(html: &str) -> Results {
    let doc = Document::from(html);
    let mut hits = Vec::new();
    for link in doc.select("a.result-link").nodes() {
        let Some(url) = link.attr("href").and_then(|href| unwrap_redirect(&href)) else {
            continue;
        };
        let mut hit = Hit {
            title: squeeze(&link.text()),
            url,
            snippet: String::new(),
            shown: String::new(),
            date: None,
        };
        let row = std::iter::successors(link.parent(), |node| node.parent())
            .find(|node| node.node_name().as_deref() == Some("tr"));
        let mut next = row.and_then(|row| row.next_element_sibling());
        while let Some(tr) = next {
            let has = |selector: &str| {
                tr.descendants()
                    .into_iter()
                    .find(|node| node.is(selector))
                    .map(|node| squeeze(&node.text()))
            };
            if has("a.result-link").is_some() {
                break;
            }
            if let Some(snippet) = has("td.result-snippet") {
                hit.snippet = snippet;
            }
            if let Some(shown) = has("span.link-text") {
                hit.shown = shown;
            }
            if let Some(date) = has("span.timestamp") {
                hit.date = date.get(..10).map(str::to_owned);
            }
            next = tr.next_element_sibling();
        }
        if !hit.title.is_empty() {
            hits.push(hit);
        }
    }
    if !hits.is_empty() {
        Results::Hits(hits)
    } else if doc
        .select(".anomaly-modal__modal, .anomaly-modal__puzzle")
        .exists()
    {
        Results::Challenge
    } else {
        Results::Nothing
    }
}

/// Адрес страницы из ссылки выдачи: `//duckduckgo.com/l/?uddg=<адрес>` —
/// счётчик переходов, настоящий адрес в `uddg`. Разворачиваем сами, чтобы
/// подсказка у ссылки и история показывали сайт, а не счётчик. Ссылка
/// рекламы (`y.js`) адреса в `uddg` не несёт — её отбрасываем.
fn unwrap_redirect(href: &str) -> Option<String> {
    let href = href.trim();
    let absolute = match href.strip_prefix("//") {
        Some(rest) => format!("https://{rest}"),
        None => href.to_owned(),
    };
    let parsed = Url::parse(&absolute).ok()?;
    let target = if parsed
        .host_str()
        .is_some_and(|host| host.ends_with("duckduckgo.com"))
    {
        parsed
            .query_pairs()
            .find(|(key, _)| key == "uddg")
            .map(|(_, value)| value.into_owned())?
    } else {
        absolute
    };
    let target_url = Url::parse(&target).ok()?;
    matches!(target_url.scheme(), "http" | "https").then_some(target)
}

fn squeeze(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Выдача markdown-ом — тот же вид, что у ленты: заголовок — запрос,
/// результат — заголовок второго уровня ссылкой (полка показывает их
/// оглавлением), под ним хост и дата курсивом, дальше подводка. Хост,
/// а не весь адрес: адрес видно в подсказке у ссылки, а строка под
/// заголовком нужна, чтобы узнать сайт с одного взгляда.
pub fn search_markdown(query: &str, hits: &[Hit]) -> String {
    use crate::feed::{block, destination, heading, inline};

    let mut out = format!("# {}\n\n", heading(query));
    if hits.is_empty() {
        out.push_str("Nothing found. Try other words.\n");
        return out;
    }
    out.push_str("*Results from DuckDuckGo*\n\n");
    for hit in hits {
        out.push_str(&format!(
            "## [{}]({})\n\n",
            heading(&hit.title),
            destination(&hit.url)
        ));
        let host = Url::parse(&hit.url)
            .ok()
            .and_then(|url| {
                url.host_str()
                    .map(|host| host.trim_start_matches("www.").to_owned())
            })
            .unwrap_or_else(|| hit.shown.clone());
        let meta: Vec<String> = [Some(host), hit.date.clone()]
            .into_iter()
            .flatten()
            .filter(|text| !text.is_empty())
            .map(|text| inline(&text))
            .collect();
        if !meta.is_empty() {
            out.push_str(&format!("*{}*\n\n", meta.join(" · ")));
        }
        if !hit.snippet.is_empty() {
            out.push_str(&block(&hit.snippet));
            out.push_str("\n\n");
        }
    }
    out.truncate(out.trim_end().len());
    out.push('\n');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Читалка royallib, сжатая до того, на что смотрит правило.
    const READER: &str = "<html><head><title>Гаррисон Гарри :: Режим чтения</title>\
        <script>var rlServerMap = [0,20277,40749,60937];</script></head><body>\
        <div id=\"bid\" style=\"display:none\">28323</div>\
        <div id=\"contentDiv\"><p>Глава 1. Начало.</p></div></body></html>";
    const BOOK: &str = "https://royallib.com/read/garrison_garri/neukrotimaya_planeta.html";

    #[test]
    fn a_royallib_reader_page_is_a_part_of_a_book() {
        assert_eq!(book_part(BOOK), Some(0));
        assert_eq!(book_part(&format!("{BOOK}?part=3")), Some(2));
        assert_eq!(book_part(&format!("{BOOK}?part=0")), Some(0));
        assert_eq!(book_part("https://www.royallib.com/read/a/b.html"), Some(0));
        // Карточка книги и чужой хост — не читалка.
        assert_eq!(
            book_part("https://royallib.com/book/garrison_garri/neukrotimaya_planeta.html"),
            None
        );
        assert_eq!(book_part("https://example.com/read/a/b.html"), None);
    }

    #[test]
    fn a_reader_page_knows_its_book_and_parts() {
        let book = Book::of(READER).unwrap();
        assert_eq!(book.parts(), 3);
        assert!(book.served);
        assert_eq!(
            book.part_url(BOOK, 2).as_deref(),
            Some("https://royallib.com/br.php?i=28323&pg=2")
        );
        assert_eq!(
            book.map_url(BOOK).as_deref(),
            Some("https://royallib.com/br.php?i=28323&map=1")
        );
        // Без карты в странице её дают отдельным ответом.
        let mut bare = Book::of(&READER.replace("rlServerMap", "other")).unwrap();
        assert_eq!(bare.parts(), 0);
        bare.set_map("[0,20277,40749,60937,81104,83848]");
        assert_eq!(bare.parts(), 5);
        // Без номера книги это не читалка.
        assert_eq!(Book::of("<html><body><p>Текст</p></body></html>"), None);
    }

    #[test]
    fn a_part_goes_into_the_reader_with_a_way_on() {
        let book = Book::of(READER).unwrap();
        let second = book_page(
            READER,
            &format!("{BOOK}?part=2#top"),
            &book,
            1,
            Some("<p>Глава 2. Дальше.</p>"),
        );
        assert!(second.contains("Глава 2. Дальше."));
        assert!(!second.contains("Глава 1. Начало."));
        assert!(second.contains("Part 2 of 3"));
        assert!(second.contains(&format!("href=\"{BOOK}\">Previous part")));
        assert!(second.contains(&format!("href=\"{BOOK}?part=3\">Next part")));
        // Первая порция уже в странице: только строка, и назад из неё некуда.
        let first = book_page(READER, BOOK, &book, 0, None);
        assert!(first.contains("Глава 1. Начало."));
        assert!(first.contains("Part 1 of 3"));
        assert!(!first.contains("Previous part"));
    }

    #[test]
    fn reddit_pages_are_read_through_their_feeds() {
        assert_eq!(
            feed_for("https://www.reddit.com/r/rust/comments/1wkmzun/no_more_code_dumps/"),
            Some(
                "https://www.reddit.com/r/rust/comments/1wkmzun/no_more_code_dumps/.rss".to_owned()
            )
        );
        assert_eq!(
            feed_for("https://old.reddit.com/r/rust/?sort=new"),
            Some("https://www.reddit.com/r/rust/.rss".to_owned())
        );
        // Уже лента, чужой хост, страница не треда — мимо.
        assert_eq!(feed_for("https://www.reddit.com/r/rust/.rss"), None);
        assert_eq!(feed_for("https://example.org/r/rust/"), None);
        assert_eq!(feed_for("https://www.reddit.com/user/someone/"), None);
    }

    #[test]
    fn a_reddit_reply_loses_its_signature() {
        let post = "<!-- SC_OFF --><div class=\"md\"><p>Text <a href=\"/r/rust\">r/rust</a>.</p></div>\
            <!-- SC_ON --> &#32; submitted by &#32; <a href=\"https://www.reddit.com/user/x\"> /u/x </a> \
            <br/> <span><a href=\"https://t.co/\">[link]</a></span>";
        assert_eq!(
            reply_html(post),
            "<p>Text <a href=\"/r/rust\">r/rust</a>.</p>"
        );

        let link = "<table><tr><td><a href=\"https://r.it/t\"><img src=\"thumb.jpg\"></a></td>\
            <td> submitted by <a href=\"/u/x\">/u/x</a> <span><a href=\"https://blog.example/post\">[link]</a></span>\
            <span><a href=\"https://r.it/t\">[comments]</a></span></td></tr></table>";
        assert_eq!(
            reply_html(link),
            "<p><a href=\"https://blog.example/post\">https://blog.example/post</a></p>"
        );
        assert_eq!(reply_html("<p>plain</p>"), "<p>plain</p>");
    }

    #[test]
    fn a_query_becomes_the_address_of_the_results_and_back() {
        let url = search_url(" как работает borrow checker ");
        assert_eq!(
            url,
            "https://lite.duckduckgo.com/lite/?q=%D0%BA%D0%B0%D0%BA+%D1%80%D0%B0%D0%B1%D0%BE%D1%82%D0%B0%D0%B5%D1%82+borrow+checker"
        );
        assert_eq!(
            search_query(&url).as_deref(),
            Some("как работает borrow checker")
        );
        assert_eq!(search_query("https://lite.duckduckgo.com/lite/"), None);
        assert_eq!(search_query("https://duckduckgo.com/?q=rust"), None);
        assert_eq!(search_query("https://example.org/lite/?q=rust"), None);
    }

    /// Фикстура — настоящая выдача 28 сентября 2026: поменяет DuckDuckGo
    /// разметку — упадёт этот тест, а не чтение.
    #[test]
    fn duckduckgo_lite_results_become_hits() {
        let Results::Hits(hits) = results(include_str!("../tests/fixtures/ddg-lite-results.html"))
        else {
            panic!("выдача не разобралась");
        };
        assert_eq!(hits.len(), 10);
        assert_eq!(
            hits[0],
            Hit {
                title: "Borrowing - Rust By Example".to_owned(),
                url: "https://doc.rust-lang.org/beta/rust-by-example/scope/borrow.html".to_owned(),
                snippet: "Borrowing Most of the time, we'd like to access data without taking \
                    ownership over it. To accomplish this, Rust uses a borrowing mechanism. \
                    Instead of passing objects by value (T), objects can be passed by reference \
                    (&T). The compiler statically guarantees (via its borrow checker) that \
                    references always point to valid objects."
                    .to_owned(),
                shown: "doc.rust-lang.org/beta/rust-by-example/scope/borrow.html".to_owned(),
                date: None,
            }
        );
        // Ни одного адреса счётчика: все ссылки ведут на сами сайты.
        assert!(hits.iter().all(|hit| !hit.url.contains("duckduckgo.com")));
        let last = hits.last().unwrap();
        assert_eq!(last.date.as_deref(), Some("2026-08-04"));
    }

    #[test]
    fn a_duckduckgo_puzzle_is_a_challenge_not_an_empty_list() {
        assert_eq!(
            results(include_str!("../tests/fixtures/ddg-lite-challenge.html")),
            Results::Challenge
        );
        assert_eq!(
            results("<html><body><table></table></body></html>"),
            Results::Nothing
        );
    }

    #[test]
    fn results_read_as_a_list_of_links() {
        let hits = vec![Hit {
            title: "Rust [book]".to_owned(),
            url: "https://www.example.org/a".to_owned(),
            snippet: "1. Borrowing *is* explained".to_owned(),
            shown: "example.org/a".to_owned(),
            date: Some("2026-08-04".to_owned()),
        }];
        assert_eq!(
            search_markdown("borrow checker", &hits),
            "# borrow checker\n\n*Results from DuckDuckGo*\n\n\
             ## [Rust \\[book\\]](https://www.example.org/a)\n\n\
             *example.org · 2026-08-04*\n\n\
             1\\. Borrowing \\*is\\* explained\n"
        );
        assert_eq!(
            search_markdown("qzx", &[]),
            "# qzx\n\nNothing found. Try other words.\n"
        );
    }

    #[test]
    fn an_ad_is_not_a_result() {
        assert_eq!(
            unwrap_redirect(
                "//duckduckgo.com/y.js?ad_domain=example.com&u3=https%3A%2F%2Fad.example"
            ),
            None
        );
        assert_eq!(
            unwrap_redirect("//duckduckgo.com/l/?uddg=https%3A%2F%2Fexample.org%2Fa&rut=1"),
            Some("https://example.org/a".to_owned())
        );
        assert_eq!(
            unwrap_redirect("https://example.org/direct"),
            Some("https://example.org/direct".to_owned())
        );
        assert_eq!(unwrap_redirect("javascript:void(0)"), None);
    }

    /// Wayback ищут для страницы в сети, без решётки; не для выдачи
    /// поиска, самого архива и машин без имени.
    #[test]
    fn the_wayback_is_asked_only_about_pages_on_the_web() {
        assert_eq!(
            for_wayback("https://danluu.com/keyboard-latency/?a=1&b=2#computers").as_deref(),
            Some("https://danluu.com/keyboard-latency/?a=1&b=2")
        );
        assert_eq!(
            for_wayback("http://example.org/").as_deref(),
            Some("http://example.org/")
        );
        for none in [
            &search_url("borrow checker") as &str,
            "https://web.archive.org/web/https://example.org/",
            "https://archive.org/details/x",
            "http://localhost:8000/page",
            "http://192.168.1.10/page",
            "http://nas/page",
        ] {
            assert_eq!(for_wayback(none), None, "{none}");
        }
    }

    /// Wayback спрашивают о странице и берут только живой снимок; пустой
    /// ответ, снимок отказа и не JSON — снимка нет.
    #[test]
    fn the_snapshot_is_the_latest_one_that_kept_the_page() {
        assert_eq!(
            available_query("https://www.google.com/reader/about/?a=1&b=2"),
            "https://archive.org/wayback/available?url=https%3A%2F%2Fwww.google.com%2Freader%2Fabout%2F%3Fa%3D1%26b%3D2"
        );
        let answer = |status: &str| {
            format!(
                r#"{{"url": "www.google.com/reader/about/", "archived_snapshots": {{"closest": {{"status": "{status}", "available": true, "url": "http://web.archive.org/web/20210503140021/https://www.google.com/reader/about/", "timestamp": "20210503140021"}}}}}}"#
            )
        };
        assert_eq!(
            available_snapshot(&answer("200")).as_deref(),
            Some("https://web.archive.org/web/20210503140021/https://www.google.com/reader/about/")
        );
        assert_eq!(available_snapshot(&answer("404")), None);
        assert_eq!(
            available_snapshot(r#"{"url": "danluu.com", "archived_snapshots": {}}"#),
            None
        );
        assert_eq!(available_snapshot("<html>Temporarily Offline</html>"), None);
    }

    /// Запасной путь — CDX: только снимки с ответом 200, последний; из ответа
    /// берётся последняя строка, пустой ответ — снимка нет.
    #[test]
    fn the_cdx_gives_the_latest_good_snapshot_when_asked() {
        assert_eq!(
            cdx_query("https://www.google.com/reader/about/?a=1&b=2"),
            "https://web.archive.org/cdx/search/cdx?url=https%3A%2F%2Fwww.google.com%2Freader%2Fabout%2F%3Fa%3D1%26b%3D2\
             &filter=statuscode%3A200&fl=timestamp%2Coriginal&limit=-1"
        );
        assert_eq!(
            latest_snapshot(
                "20130101000000 http://www.google.com/reader/about/\n\
                 20210503140021 https://www.google.com/reader/about/\n"
            )
            .as_deref(),
            Some("https://web.archive.org/web/20210503140021/https://www.google.com/reader/about/")
        );
        assert_eq!(latest_snapshot(""), None);
        assert_eq!(latest_snapshot("<html>Temporarily Offline</html>"), None);
    }

    /// Тулбар Wayback вырезается целиком, страница вокруг остаётся; без
    /// его комментариев страница не трогается.
    #[test]
    fn the_wayback_toolbar_is_not_the_page() {
        let snapshot = "<body><!-- BEGIN WAYBACK TOOLBAR INSERT --><div id=\"wm-ipp-print\">\
            The Wayback Machine - https://web.archive.org/web/2021/https://a.org/</div>\
            <!-- END WAYBACK TOOLBAR INSERT --><h1>Reader</h1></body>";
        assert_eq!(
            without_wayback_toolbar(snapshot),
            "<body><h1>Reader</h1></body>"
        );
        let plain = "<body><h1>Reader</h1></body>";
        assert!(matches!(
            without_wayback_toolbar(plain),
            std::borrow::Cow::Borrowed(_)
        ));
    }

    /// Снимок узнаётся по адресу: чей он и от какого дня.
    #[test]
    fn a_snapshot_says_what_page_it_is_and_from_when() {
        let snapshot = wayback_snapshot(
            "https://web.archive.org/web/20130701123456/https://www.google.com/reader/about/?x=1#top",
        )
        .unwrap();
        assert_eq!(snapshot.source, "https://www.google.com/reader/about/?x=1");
        assert_eq!(snapshot.read, "1 July 2013");
        assert!(snapshot.wayback);
        assert!(
            wayback_snapshot("https://web.archive.org/web/20130701im_/https://a.org/x.png")
                .is_some()
        );
        assert_eq!(
            wayback_snapshot("https://web.archive.org/web/*/https://a.org/"),
            None
        );
        assert_eq!(
            wayback_snapshot("https://example.org/web/20130701/https://a.org/"),
            None
        );
    }
}
