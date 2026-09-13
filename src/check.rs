//! `brevier --check <url>` — во что странице обходится чистое чтение.
//!
//! Проверка гоняет обычный тракт (`fetch → extract → markdown`) и смотрит,
//! что случилось на каждой ступени, а потом печатает отчёт: счёт от 0 до 100,
//! находки по стадиям и первый экран того, что увидит читатель. В markdown —
//! через тот же рендерер, что и статья: в окне это `brevier:check`, в pull
//! request это дифф.
//!
//! Три правила держат проверку честной, и все три — против соблазна:
//!
//! - **один скачанный ответ, и никакого второго.** Ни headless-браузера,
//!   ни внешнего сервиса: проверка меряет ровно то, что сервер отдал
//!   читателю без скриптов. Иначе она мерила бы не страницу, а свою машину.
//! - **веса лежат в одной таблице** ([`RULES`]) и печатаются в каждом отчёте.
//!   Счёт — не скрытая формула: с любым числом можно спорить построчно.
//! - **счёт детерминирован**, как и извлечение: та же страница даёт тот же
//!   отчёт. Поэтому отчёты и ложатся в корпус эталонами (`corpus/check/`).
//!
//! Гейта у этого числа нет и быть не должно: подгонять правила под свой же
//! балл — то же самое, что подгонять извлечение под чужой бенчмарк. Число
//! говорит автору страницы, что стоит между ней и чтением, — не больше.

use dom_query::{Document, NodeRef};

use crate::error::Error;
use crate::extract;
use crate::fetch::{self, ContentKind, UserAgent};
use crate::markdown;

/// К какой ступени тракта относится находка. Порядок — порядок отчёта:
/// сперва доступ (без него нет и остального), потом текст, разметка, мелочи.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    /// Пустил ли сервер честного читателя и отдал ли текст.
    Access,
    /// Есть ли в HTML сам текст и много ли вокруг него шума.
    Text,
    /// Размечен ли текст так, чтобы его можно было набрать: заголовки,
    /// абзацы, язык, автор.
    Structure,
    /// Что необязательно, но помогает: `alt`, фид, копия в markdown.
    Extras,
}

impl Stage {
    fn title(self) -> &'static str {
        match self {
            Stage::Access => "Access",
            Stage::Text => "Text",
            Stage::Structure => "Structure",
            Stage::Extras => "Extras",
        }
    }

    /// Порядок стадий в отчёте и в таблице весов.
    const ORDER: [Stage; 4] = [Stage::Access, Stage::Text, Stage::Structure, Stage::Extras];
}

/// Во что находка обходится счёту.
#[derive(Debug, Clone, Copy)]
enum Cost {
    /// Вычесть столько очков из ста.
    Deduct(u32),
    /// Ограничить счёт этим числом, сколько бы ни осталось после вычетов.
    /// Страница, которую нельзя прочесть, не «читается на 70» из-за того,
    /// что заголовки в порядке.
    Cap(u32),
}

/// Одна проверка. Совет — одной фразой и с именем элемента: читатель отчёта
/// должен знать, что менять, не выходя за строку.
#[derive(Debug)]
struct Rule {
    id: &'static str,
    stage: Stage,
    cost: Cost,
    advice: &'static str,
}

/// Та самая одна таблица весов. Источник истины и для счёта, и для отчёта.
/// Числа здесь спорные намеренно — на то они и напечатаны.
const RULES: &[Rule] = &[
    // --- Access. Всё это — стены до первой буквы, поэтому caps, а не вычет.
    Rule {
        id: "access-forbidden",
        stage: Stage::Access,
        cost: Cost::Cap(0),
        advice: "Let in a client that identifies itself and runs no scripts; a 403 to an honest reader is a wall.",
    },
    Rule {
        id: "access-http",
        stage: Stage::Access,
        cost: Cost::Cap(0),
        advice: "Return the page with a 2xx status; there is nothing to read behind an error code.",
    },
    Rule {
        id: "access-cert",
        stage: Stage::Access,
        cost: Cost::Cap(0),
        advice: "Serve a certificate the operating system trusts; an unknown CA stops the reader before the first byte.",
    },
    Rule {
        id: "access-unreachable",
        stage: Stage::Access,
        cost: Cost::Cap(0),
        advice: "The host did not answer; a reader cannot read a page it cannot reach.",
    },
    Rule {
        id: "access-content-type",
        stage: Stage::Access,
        cost: Cost::Cap(0),
        advice: "Serve the text as HTML or Markdown; this content type is not text a reader can set.",
    },
    Rule {
        id: "access-too-large",
        stage: Stage::Access,
        cost: Cost::Cap(0),
        advice: "The response is larger than a document should be; a reader stops at a size limit.",
    },
    // --- Text.
    Rule {
        id: "text-empty",
        stage: Stage::Text,
        cost: Cost::Cap(10),
        advice: "The extractor found no article; the text a reader keeps is the text that stands in the HTML.",
    },
    Rule {
        id: "text-script-only",
        stage: Stage::Text,
        cost: Cost::Cap(10),
        advice: "Put the words in the HTML: they arrive here only after a script runs, and this reader runs none.",
    },
    Rule {
        id: "text-noise",
        stage: Stage::Text,
        cost: Cost::Deduct(6),
        advice: "Most of the page is furniture around a little text; a reader keeps only the article, so the ratio shows.",
    },
    Rule {
        id: "text-lazy-images",
        stage: Stage::Text,
        cost: Cost::Deduct(4),
        advice: "Give images a real `src`: an address parked in a `data-` attribute for a script to move never loads here.",
    },
    // --- Structure.
    Rule {
        id: "structure-h1",
        stage: Stage::Structure,
        cost: Cost::Deduct(8),
        advice: "Give the page exactly one `<h1>` — its title, once.",
    },
    Rule {
        id: "structure-heading-order",
        stage: Stage::Structure,
        cost: Cost::Deduct(5),
        advice: "Let heading levels descend without skipping, so the outline is the author's and not the type size's.",
    },
    Rule {
        id: "structure-landmark",
        stage: Stage::Structure,
        cost: Cost::Deduct(8),
        advice: "Wrap the article in `<article>` or `<main>` so its body is unambiguous.",
    },
    Rule {
        id: "structure-paragraphs",
        stage: Stage::Structure,
        cost: Cost::Deduct(5),
        advice: "Make paragraphs `<p>`, not `<br><br>`: a reader splits text by the element, not by a blank line.",
    },
    Rule {
        id: "structure-code-lang",
        stage: Stage::Structure,
        cost: Cost::Deduct(4),
        advice: "Name the language on `<pre><code class=\"language-…\">`, so code is set and coloured as code.",
    },
    Rule {
        id: "structure-lang",
        stage: Stage::Structure,
        cost: Cost::Deduct(6),
        advice: "Declare the page's language with `lang` on `<html>`, so it can be hyphenated the right way.",
    },
    Rule {
        id: "structure-title",
        stage: Stage::Structure,
        cost: Cost::Deduct(4),
        advice: "Give the page a `<title>`: it names the tab, the saved file and the history line.",
    },
    Rule {
        id: "structure-title-h1",
        stage: Stage::Structure,
        cost: Cost::Deduct(3),
        advice: "Let the `<title>` agree with the `<h1>`; they name the same page.",
    },
    Rule {
        id: "structure-byline",
        stage: Stage::Structure,
        cost: Cost::Deduct(4),
        advice: "Mark the author where the extractor can find it (`rel=author`, or an `<address>` in the article).",
    },
    // --- Extras.
    Rule {
        id: "extras-alt",
        stage: Stage::Extras,
        cost: Cost::Deduct(4),
        advice: "Describe images in `alt`; with images off, a reader sees the description in the frame.",
    },
    Rule {
        id: "extras-feed",
        stage: Stage::Extras,
        cost: Cost::Deduct(2),
        advice: "Advertise a feed in `<head>` (`<link rel=alternate type=application/rss+xml>`) so it can be followed.",
    },
    Rule {
        id: "extras-alt-markdown",
        stage: Stage::Extras,
        cost: Cost::Deduct(3),
        advice: "Offer `<link rel=alternate type=text/markdown>`: a reader then takes the exact text, with no extraction in the way.",
    },
];

fn rule_of(id: &str) -> &'static Rule {
    RULES
        .iter()
        .find(|rule| rule.id == id)
        .expect("check rule id must exist in RULES")
}

/// Одна замеченная неисправность: правило плюс то, что именно увидели.
#[derive(Debug)]
pub struct Finding {
    rule: &'static Rule,
    /// Что именно на этой странице, конкретными числами: «3 h1 elements»,
    /// «kept 12% of the page's text».
    seen: String,
}

/// Отчёт проверки. Публичный: окно откроет его как `brevier:check`,
/// напечатав `to_markdown` тем же рендерером, что и статью.
#[derive(Debug)]
pub struct Report {
    pub address: String,
    /// Мерили ли доступ. При `--stdin` его нет — HTML пришёл не из сети.
    pub access_measured: bool,
    /// Сервер отдал текст сразу (markdown или plain) — извлекать нечего,
    /// и это лучший исход, а не находка.
    served: Option<&'static str>,
    findings: Vec<Finding>,
    /// Первый экран того, что достаётся читателю. `None`, если извлекать
    /// было нечего.
    preview: Option<String>,
    /// Счёт 0..100, уже с вычетами и ограничениями.
    pub score: u32,
}

/// Скачать и проверить. Отказ доступа — не ошибка процесса, а находка
/// со счётом 0: в этом и смысл проверки. Ошибкой остаётся только то,
/// что не даёт даже начать, — неразобранный адрес и чужая схема.
pub fn check(url: &str, ua: UserAgent) -> Result<Report, Error> {
    match fetch::fetch(url, ua) {
        Ok(page) => Ok(match page.kind {
            ContentKind::Html => from_html(&page.body, &page.url, true),
            ContentKind::Markdown => served(&page.url, &page.body, "Markdown"),
            ContentKind::Text => served(&page.url, &page.body, "plain text"),
        }),
        Err(error) => match access_finding(&error) {
            Some(finding) => Ok(from_findings(url.to_owned(), vec![finding], true, None)),
            None => Err(error),
        },
    }
}

/// Проверить HTML, который уже на руках, — `--check --stdin`, для страницы,
/// которую ещё не выложили. Доступ здесь не меряется: скачивания не было.
pub fn check_html(html: &str, url: &str) -> Report {
    from_html(html, url, false)
}

/// Ошибку fetch — в находку стадии Access. `None` означает «это не про
/// страницу»: адрес не разобрался или схема чужая, отчёту неоткуда взяться.
fn access_finding(error: &Error) -> Option<Finding> {
    let (id, seen) = match error {
        Error::HttpStatus(403 | 401) => (
            "access-forbidden",
            format!("the server answered {}", status(error)),
        ),
        Error::HostingLimit => (
            "access-http",
            "the hosting API is rate limited (a 403 by another name)".to_owned(),
        ),
        Error::HttpStatus(code) => ("access-http", format!("the server answered {code}")),
        Error::UnsupportedContentType(mime) => {
            ("access-content-type", format!("served `{mime}`, not text"))
        }
        Error::TooLarge(limit) => (
            "access-too-large",
            format!("the response is over the {limit} byte limit"),
        ),
        Error::Network(inner) => {
            let text = inner.to_string();
            if text.to_ascii_lowercase().contains("certificate") {
                (
                    "access-cert",
                    "the certificate is not trusted by the operating system".to_owned(),
                )
            } else {
                ("access-unreachable", squeeze(&text))
            }
        }
        // Не про страницу: без адреса и схемы проверять нечего.
        Error::BadUrl(_)
        | Error::UnsupportedScheme(_)
        | Error::EmptyExtraction
        | Error::Convert(_)
        | Error::Media(_) => return None,
    };
    Some(note(id, seen))
}

fn status(error: &Error) -> u16 {
    match error {
        Error::HttpStatus(code) => *code,
        _ => 0,
    }
}

/// Страница, которую сервер отдал текстом сразу. Извлекать нечего,
/// ставить в вину — тоже: это ровно то, о чём просит манифест.
fn served(url: &str, body: &str, kind: &'static str) -> Report {
    Report {
        address: url.to_owned(),
        access_measured: true,
        served: Some(kind),
        findings: Vec::new(),
        preview: Some(first_screen(body)),
        score: 100,
    }
}

fn from_html(html: &str, url: &str, access_measured: bool) -> Report {
    let doc = Document::from(html);
    let mut findings = Vec::new();

    structure_findings(&doc, &mut findings);
    extras_findings(&doc, &mut findings);
    let preview = text_findings(html, url, &doc, &mut findings);

    from_findings_with_preview(url.to_owned(), findings, access_measured, preview)
}

fn from_findings(
    address: String,
    findings: Vec<Finding>,
    access_measured: bool,
    preview: Option<String>,
) -> Report {
    from_findings_with_preview(address, findings, access_measured, preview)
}

fn from_findings_with_preview(
    address: String,
    findings: Vec<Finding>,
    access_measured: bool,
    preview: Option<String>,
) -> Report {
    let score = score(&findings);
    Report {
        address,
        access_measured,
        served: None,
        findings,
        preview,
        score,
    }
}

/// Счёт: старт сто, вычитаем `Deduct`, режем по наименьшему `Cap`. Порядок
/// не важен — сложение коммутативно, а cap берёт минимум.
fn score(findings: &[Finding]) -> u32 {
    let mut points: i32 = 100;
    let mut ceiling: u32 = 100;
    for finding in findings {
        match finding.rule.cost {
            Cost::Deduct(p) => points -= p as i32,
            Cost::Cap(c) => ceiling = ceiling.min(c),
        }
    }
    (points.clamp(0, 100) as u32).min(ceiling)
}

fn note(id: &str, seen: String) -> Finding {
    Finding {
        rule: rule_of(id),
        seen,
    }
}

// --- Стадия Text: есть ли текст и много ли вокруг него шума.

/// Возвращает первый экран извлечённого — или `None`, если извлекать
/// было нечего.
fn text_findings(
    html: &str,
    url: &str,
    doc: &Document,
    findings: &mut Vec<Finding>,
) -> Option<String> {
    // Ленивые картинки — про страницу, а не про наш обходной путь: адрес,
    // спрятанный в `data-`, у любого клиента без скрипта не загрузится.
    if let Some(count) = lazy_images(doc) {
        let seen = if count == 1 {
            "one image carries its address in a `data-` attribute, not `src`".to_owned()
        } else {
            format!("{count} images carry their address in a `data-` attribute, not `src`")
        };
        findings.push(note("text-lazy-images", seen));
    }

    match extract::extract(html, url) {
        Ok(article) => {
            // Доля шума: сколько текста страницы читателю пришлось выбросить.
            let page = page_text_len(html);
            let kept = text_len(&article.content_html);
            if page > 2000 && (kept as f32) < 0.20 * page as f32 {
                let percent = (kept * 100).checked_div(page).unwrap_or(0);
                findings.push(note(
                    "text-noise",
                    format!("the extractor kept {percent}% of the page's text"),
                ));
            }

            if article
                .byline
                .as_deref()
                .map(str::trim)
                .unwrap_or("")
                .is_empty()
            {
                findings.push(note("structure-byline", "no author was found".to_owned()));
            }

            markdown::from_article(&article)
                .ok()
                .map(|reading| first_screen(&reading.markdown))
        }
        Err(_) => {
            // Извлечь нечего. Отчего — от скрипта или страница правда пуста.
            if script_only(doc) {
                findings.push(note(
                    "text-script-only",
                    "the text is absent until a script runs".to_owned(),
                ));
            } else {
                findings.push(note(
                    "text-empty",
                    "the extractor found no article on the page".to_owned(),
                ));
            }
            None
        }
    }
}

/// Признаки того, что текст приезжает только со скриптом: большой `<noscript>`
/// с извинением, текст, запертый в `<template>`, или горсть картинок с адресом
/// в `data-`, но без `src`.
fn script_only(doc: &Document) -> bool {
    const APOLOGY: usize = 200;
    let noscript = squeeze(&doc.select("noscript").text()).chars().count();
    let template = squeeze(&doc.select("template").text()).chars().count();
    noscript > APOLOGY || template > APOLOGY || lazy_images(doc).is_some_and(|n| n >= 3)
}

/// Картинки, чей адрес спрятан в `data-`-атрибуте, а `src` пуст или заглушка.
/// `None` — таких нет.
fn lazy_images(doc: &Document) -> Option<usize> {
    const DATA_SRC: [&str; 4] = ["data-src", "data-original", "data-lazy-src", "data-srcset"];
    let count = doc
        .select("img")
        .nodes()
        .iter()
        .filter(|img| {
            let real = img
                .attr("src")
                .map(|src| src.trim().to_owned())
                .unwrap_or_default();
            let missing = real.is_empty() || real.starts_with("data:");
            missing && DATA_SRC.iter().any(|name| img.has_attr(name))
        })
        .count();
    (count > 0).then_some(count)
}

// --- Стадия Structure: набрана ли страница так, чтобы её можно было прочесть.

fn structure_findings(doc: &Document, findings: &mut Vec<Finding>) {
    // Ровно один h1.
    let h1 = doc.select("h1").length();
    if h1 != 1 {
        let seen = if h1 == 0 {
            "no `<h1>` on the page".to_owned()
        } else {
            format!("{h1} `<h1>` elements")
        };
        findings.push(note("structure-h1", seen));
    }

    // Уровни заголовков не прыгают вниз через ступень.
    if let Some((from, to)) = heading_skip(doc) {
        findings.push(note(
            "structure-heading-order",
            format!("a heading jumps from h{from} to h{to}"),
        ));
    }

    // Тело статьи размечено ориентиром.
    if !doc.select("article").exists() && !doc.select("main").exists() {
        findings.push(note(
            "structure-landmark",
            "neither `<article>` nor `<main>` is present".to_owned(),
        ));
    }

    // Абзацы не через `<br><br>`.
    let breaks = doc.select("br + br").length();
    if breaks >= 3 {
        findings.push(note(
            "structure-paragraphs",
            format!("paragraphs are split by `<br><br>` in {breaks} places"),
        ));
    }

    // Блоки кода называют язык.
    if let Some(total) = code_without_language(doc) {
        let seen = if total == 1 {
            "a code block names no language".to_owned()
        } else {
            format!("{total} code blocks name no language")
        };
        findings.push(note("structure-code-lang", seen));
    }

    // Язык страницы объявлен.
    if doc
        .select("html")
        .attr("lang")
        .map(|lang| lang.trim().to_owned())
        .unwrap_or_default()
        .is_empty()
    {
        findings.push(note("structure-lang", "no `lang` on `<html>`".to_owned()));
    }

    // Заголовок вкладки есть и совпадает с h1.
    let title = squeeze(&doc.select("title").text());
    if title.is_empty() {
        findings.push(note("structure-title", "no `<title>`".to_owned()));
    } else if let Some(h1_text) = first_h1(doc)
        && !title_matches_h1(&title, &h1_text)
    {
        findings.push(note(
            "structure-title-h1",
            format!(
                "`<title>` {} does not match the `<h1>` {}",
                clip(&title),
                clip(&h1_text)
            ),
        ));
    }
}

/// Первый прыжок уровня вниз через ступень (h2→h4). Вверх скакать можно:
/// это конец подраздела, а не пропуск.
fn heading_skip(doc: &Document) -> Option<(u8, u8)> {
    let mut previous: Option<u8> = None;
    for node in doc.select("h1, h2, h3, h4, h5, h6").nodes() {
        let Some(level) = heading_level(node) else {
            continue;
        };
        if let Some(prev) = previous
            && level > prev + 1
        {
            return Some((prev, level));
        }
        previous = Some(level);
    }
    None
}

fn heading_level(node: &NodeRef) -> Option<u8> {
    match node.node_name().as_deref() {
        Some("h1") => Some(1),
        Some("h2") => Some(2),
        Some("h3") => Some(3),
        Some("h4") => Some(4),
        Some("h5") => Some(5),
        Some("h6") => Some(6),
        _ => None,
    }
}

fn first_h1(doc: &Document) -> Option<String> {
    doc.select("h1")
        .nodes()
        .first()
        .map(|node| squeeze(&node.text()))
        .filter(|text| !text.is_empty())
}

/// Совпадает ли заголовок вкладки с h1. Хвост «… | Сайт» отсекаем: он и есть
/// то, чем `<title>` честно отличается от `<h1>`. Сравнение по вхождению —
/// один обычно длиннее другого ровно на имя сайта.
fn title_matches_h1(title: &str, h1: &str) -> bool {
    let title = trim_site_suffix(&title.to_lowercase());
    let h1 = h1.to_lowercase();
    title.contains(&h1) || h1.contains(&title)
}

fn trim_site_suffix(title: &str) -> String {
    for sep in [" | ", " – ", " — ", " · ", " :: "] {
        if let Some((head, _)) = title.split_once(sep) {
            return head.trim().to_owned();
        }
    }
    title.trim().to_owned()
}

/// Сколько блоков кода не называют язык. `None` — либо кода нет, либо
/// язык назван хотя бы у одного (генератор един на страницу, так что
/// хватает одного признака).
fn code_without_language(doc: &Document) -> Option<usize> {
    let selection = doc.select("pre code");
    let blocks = selection.nodes();
    if blocks.is_empty() {
        return None;
    }
    let named = blocks.iter().any(has_language);
    (!named).then_some(blocks.len())
}

/// Язык бывает на самом `<code>`, на `<pre>` над ним или на обёртке
/// (`<figure data-lang>`) — те же три места, что снимает `extract::keep_lang`.
fn has_language(code: &NodeRef) -> bool {
    fn marks(node: &NodeRef) -> bool {
        if node.has_attr("data-lang") {
            return true;
        }
        node.attr("class").is_some_and(|class| {
            class
                .split_whitespace()
                .any(|token| token.starts_with("language-") || token.starts_with("lang-"))
        })
    }

    if marks(code) {
        return true;
    }
    let mut ancestor = code.parent();
    for _ in 0..2 {
        let Some(node) = ancestor else { break };
        if marks(&node) {
            return true;
        }
        ancestor = node.parent();
    }
    false
}

// --- Стадия Extras: чего можно и не делать, но с чем читателю лучше.

fn extras_findings(doc: &Document, findings: &mut Vec<Finding>) {
    // Картинки описаны в alt.
    let selection = doc.select("img");
    let images = selection.nodes();
    let total = images.len();
    if total > 0 {
        let missing = images
            .iter()
            .filter(|img| {
                img.attr("alt")
                    .map(|alt| alt.trim().is_empty())
                    .unwrap_or(true)
            })
            .count();
        if missing > 0 {
            let seen = if total == 1 {
                "the only image has no alt text".to_owned()
            } else {
                format!("{missing} of {total} images have no alt text")
            };
            findings.push(note("extras-alt", seen));
        }
    }

    // Фид объявлен в шапке.
    if !link_with_type(doc, &["application/rss+xml", "application/atom+xml"]) {
        findings.push(note(
            "extras-feed",
            "no feed advertised in `<head>`".to_owned(),
        ));
    }

    // Копия в markdown предложена ссылкой.
    if !link_with_type(doc, &["text/markdown"]) {
        findings.push(note(
            "extras-alt-markdown",
            "no `<link rel=alternate type=text/markdown>`".to_owned(),
        ));
    }
}

/// Есть ли `<link>` с одним из типов. Перебором по узлам, а не селектором
/// по значению: значение атрибута приходит с регистром и пробелами как есть.
fn link_with_type(doc: &Document, types: &[&str]) -> bool {
    doc.select("link").nodes().iter().any(|link| {
        link.attr("type")
            .map(|value| value.trim().to_ascii_lowercase())
            .is_some_and(|value| types.contains(&value.as_str()))
    })
}

// --- Текст страницы: числа для доли шума.

/// Видимый текст страницы, без скриптов, стилей и заготовок. Знаменатель
/// доли шума.
fn page_text_len(html: &str) -> usize {
    let doc = Document::from(html);
    doc.select("script, style, noscript, template").remove();
    squeeze(&doc.select("body").text()).chars().count()
}

fn text_len(html: &str) -> usize {
    squeeze(&Document::from(html).text()).chars().count()
}

// --- Отчёт.

impl Report {
    /// Отчёт markdown-ом. Тем же, что рендерит статью: в окне это `brevier:check`,
    /// в less читается глазами, в pull request диффается.
    pub fn to_markdown(&self) -> String {
        let mut out = String::new();
        out.push_str("# Check\n\n");
        out.push_str(&format!("`{}`\n\n", self.address));
        out.push_str(&self.headline());
        out.push('\n');

        for stage in Stage::ORDER {
            out.push_str(&format!("## {}\n\n", stage.title()));
            out.push_str(&self.stage_body(stage));
            out.push('\n');
        }

        out.push_str("## What the reader will see\n\n");
        match &self.preview {
            Some(preview) => {
                out.push_str(&fenced(preview));
                out.push('\n');
            }
            None => out.push_str("Nothing was extracted.\n"),
        }
        out.push('\n');

        out.push_str(&self.weights_table());
        out
    }

    /// Счёт и арифметика к нему.
    fn headline(&self) -> String {
        if let Some(kind) = self.served {
            return format!(
                "**Score {} / 100.** Served as {kind} by the site — a reader takes it exactly, \
                 with no extraction in the way.\n",
                self.score
            );
        }

        let (deducted, cap, cap_seen) = self.arithmetic();

        // Ограничение бьёт сильнее вычетов: сперва о нём.
        if let Some(seen) = cap_seen
            && cap < deducted
        {
            let mut line = format!(
                "**Score {} / 100.** The page cannot be read as it stands: {seen}.\n",
                self.score
            );
            if deducted < 100 {
                line.push_str(&format!(
                    "\nEven served, the markup would bring it to {deducted}.\n"
                ));
            }
            return line;
        }

        // Обычный случай: вычеты по порядку.
        let deducts = self.deduct_terms();
        if deducts.is_empty() {
            return "**Score 100 / 100.** Nothing stands between this page and a clean read.\n"
                .to_owned();
        }
        let terms = deducts
            .iter()
            .map(|(points, id)| format!(" − {points} ({id})"))
            .collect::<String>();
        format!(
            "**Score {} / 100.**\n\n100{terms} = {}\n",
            self.score, self.score
        )
    }

    /// Итог вычетов, наименьшее ограничение и то, чем оно вызвано.
    fn arithmetic(&self) -> (u32, u32, Option<&str>) {
        let mut points: i32 = 100;
        let mut cap: u32 = 100;
        let mut cap_seen = None;
        for finding in &self.findings {
            match finding.rule.cost {
                Cost::Deduct(p) => points -= p as i32,
                Cost::Cap(c) => {
                    if c < cap {
                        cap = c;
                        cap_seen = Some(finding.seen.as_str());
                    }
                }
            }
        }
        (points.clamp(0, 100) as u32, cap, cap_seen)
    }

    /// Слагаемые арифметики — по порядку стадий, чтобы строка «100 − …»
    /// читалась в том же порядке, что и разделы отчёта.
    fn deduct_terms(&self) -> Vec<(u32, &str)> {
        let mut terms = Vec::new();
        for stage in Stage::ORDER {
            for finding in self.findings.iter().filter(|f| f.rule.stage == stage) {
                if let Cost::Deduct(points) = finding.rule.cost {
                    terms.push((points, finding.rule.id));
                }
            }
        }
        terms
    }

    fn stage_body(&self, stage: Stage) -> String {
        if stage == Stage::Access && !self.access_measured {
            return "Not fetched — the HTML came from stdin, so access, redirects and content \
                    type were not measured.\n"
                .to_owned();
        }

        let mut lines = String::new();
        for finding in self.findings.iter().filter(|f| f.rule.stage == stage) {
            let cost = match finding.rule.cost {
                Cost::Deduct(points) => format!("−{points}"),
                Cost::Cap(ceiling) => format!("caps at {ceiling}"),
            };
            lines.push_str(&format!(
                "- **{cost} · {}** — {}. {}\n",
                finding.rule.id, finding.seen, finding.rule.advice
            ));
        }
        if lines.is_empty() {
            "Nothing to fix.\n".to_owned()
        } else {
            lines
        }
    }

    fn weights_table(&self) -> String {
        let mut out = String::from("## The weights\n\n");
        out.push_str(
            "Every check and what it costs. Argue with a number here, not with a hidden formula.\n\n",
        );
        out.push_str("| check | stage | cost |\n|---|---|---:|\n");
        for stage in Stage::ORDER {
            for rule in RULES.iter().filter(|rule| rule.stage == stage) {
                let cost = match rule.cost {
                    Cost::Deduct(points) => format!("−{points}"),
                    Cost::Cap(ceiling) => format!("caps at {ceiling}"),
                };
                out.push_str(&format!("| {} | {} | {cost} |\n", rule.id, stage.title()));
            }
        }
        out
    }
}

// --- Мелочи вывода.

/// Первый экран: до [`LINES`] строк или [`CHARS`] знаков, что раньше. Дальше —
/// многоточие: отчёт показывает, что читатель получит, а не пересказывает
/// всю статью.
fn first_screen(markdown: &str) -> String {
    const LINES: usize = 24;
    const CHARS: usize = 1600;

    let mut out = String::new();
    let mut truncated = false;
    for (count, line) in markdown.lines().enumerate() {
        if count >= LINES || out.len() + line.len() > CHARS {
            truncated = true;
            break;
        }
        out.push_str(line);
        out.push('\n');
    }
    let out = out.trim_end().to_owned();
    if truncated && !out.is_empty() {
        format!("{out}\n…")
    } else {
        out
    }
}

/// Загородка достаточной длины: длиннее самой длинной череды бэктиков внутри,
/// иначе блок кода в статье разорвёт ограждение отчёта.
fn fenced(body: &str) -> String {
    let longest = body
        .lines()
        .map(|line| line.bytes().take_while(|&b| b == b'`').count())
        .max()
        .unwrap_or(0);
    let fence = "`".repeat(longest.max(2) + 1);
    format!("{fence}\n{body}\n{fence}\n")
}

/// Заголовок в кавычках, обрезанный до вменяемой длины.
fn clip(text: &str) -> String {
    const MAX: usize = 50;
    let text: String = text.chars().take(MAX).collect();
    format!("\"{}\"", text.trim())
}

fn squeeze(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_rule_id_is_unique() {
        let mut ids: Vec<&str> = RULES.iter().map(|rule| rule.id).collect();
        ids.sort_unstable();
        let before = ids.len();
        ids.dedup();
        assert_eq!(before, ids.len(), "duplicate rule id in RULES");
    }

    #[test]
    fn score_subtracts_then_caps() {
        // Пусто — сто.
        assert_eq!(score(&[]), 100);
        // Один вычет.
        assert_eq!(score(&[note("structure-h1", String::new())]), 92);
        // Ограничение бьёт сильнее вычетов и не уводит ниже нуля.
        let capped = score(&[
            note("structure-h1", String::new()),
            note("access-forbidden", String::new()),
        ]);
        assert_eq!(capped, 0);
        // Наименьший из потолков.
        assert_eq!(
            score(&[note("text-empty", String::new())]),
            10,
            "text-empty caps at 10"
        );
    }

    #[test]
    fn a_bare_page_collects_structure_findings() {
        // Ни h1, ни article, ни lang, ни title.
        let html = "<html><body><div>текст без разметки</div></body></html>";
        let report = check_html(html, "https://example.com/bare");
        let ids: Vec<&str> = report.findings.iter().map(|f| f.rule.id).collect();
        assert!(ids.contains(&"structure-h1"), "{ids:?}");
        assert!(ids.contains(&"structure-landmark"), "{ids:?}");
        assert!(ids.contains(&"structure-lang"), "{ids:?}");
        assert!(ids.contains(&"structure-title"), "{ids:?}");
        assert!(report.score < 80);
    }

    #[test]
    fn a_well_formed_article_passes() {
        let html = "\
<html lang=\"en\">
<head>
  <title>A clean page</title>
  <link rel=\"alternate\" type=\"application/rss+xml\" href=\"/feed.xml\">
  <link rel=\"alternate\" type=\"text/markdown\" href=\"/page.md\">
</head>
<body>
  <main>
    <article>
      <h1>A clean page</h1>
      <address rel=\"author\">A. Writer</address>
      <p>A paragraph long enough to count as prose and not a caption or a stray line of text.</p>
      <h2>A section</h2>
      <p>Another paragraph of real body text, several words wide, so the extractor keeps it.</p>
    </article>
  </main>
</body></html>";
        let report = check_html(html, "https://example.com/clean");
        assert_eq!(report.findings.len(), 0, "{:?}", report.findings);
        assert_eq!(report.score, 100);
        assert!(report.preview.is_some());
    }

    #[test]
    fn a_heading_skip_is_caught() {
        let html = "<html lang=en><body><article><h1>T</h1><h3>skips h2</h3>\
                    <p>body body body body body body body</p></article></body></html>";
        let report = check_html(html, "https://example.com/skip");
        assert!(
            report
                .findings
                .iter()
                .any(|f| f.rule.id == "structure-heading-order")
        );
    }

    #[test]
    fn code_without_a_language_is_caught() {
        let html = "<html lang=en><body><article><h1>T</h1>\
                    <p>enough words here to keep the article alive and well</p>\
                    <pre><code>fn main() {}</code></pre></article></body></html>";
        let report = check_html(html, "https://example.com/code");
        assert!(
            report
                .findings
                .iter()
                .any(|f| f.rule.id == "structure-code-lang")
        );

        let named = html.replace("<code>", "<code class=\"language-rust\">");
        let report = check_html(&named, "https://example.com/code");
        assert!(
            !report
                .findings
                .iter()
                .any(|f| f.rule.id == "structure-code-lang")
        );
    }

    #[test]
    fn report_prints_the_weight_table_and_arithmetic() {
        let html = "<html><body><div>текст без разметки</div></body></html>";
        let markdown = check_html(html, "https://example.com/bare").to_markdown();
        assert!(markdown.contains("## The weights"));
        assert!(markdown.contains("structure-h1"));
        assert!(markdown.contains("100"));
        // Стадия Access при stdin помечена как неизмеренная.
        assert!(markdown.contains("Not fetched"));
    }
}
