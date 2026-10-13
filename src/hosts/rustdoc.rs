//! Документация Rust (#11): docs.rs, doc.rust-lang.org и любая своя сборка
//! rustdoc — своим правилом, мимо Readability.
//!
//! rustdoc стоит не на одном хосте, поэтому строка таблицы узнаёт программу:
//! `<meta name="generator" content="rustdoc">`. Статья у него в одном
//! известном месте — `#main-content`, — и угадывать её незачем: Readability
//! на странице типажа переставляла заголовок раздела в середину описания
//! метода, а кнопка «Expand description» доезжала до читателя текстом.
//!
//! Из статьи снимаем то, что rustdoc рисует для мыши: подпись свёрнутого
//! описания, «Copy item path», ссылки «Source», версию и значки под
//! заголовком и справа от метода, значок ⓘ у примера. Блокам кода rustdoc пишет язык классом `rust` без
//! префикса — кладём его в `data-lang`, чтобы код подсветился.

use dom_query::Document;

use super::thread::{first, squeeze};

/// Статья страницы rustdoc: заголовок и HTML, или `None`, если это не rustdoc.
pub struct Page {
    pub title: String,
    pub html: String,
}

/// rustdoc ли это — по генератору в шапке.
pub fn is_rustdoc(doc: &Document) -> bool {
    first(&doc.root(), "meta[name=generator]")
        .and_then(|meta| meta.attr("content"))
        .is_some_and(|content| content.trim() == "rustdoc")
}

pub fn read(doc: &Document) -> Option<Page> {
    if !is_rustdoc(doc) {
        return None;
    }
    let root = doc.root();
    let main = first(&root, "#main-content")?;
    let mouse = [
        "summary.hideme",
        "a.src",
        ".rightside",
        "rustdoc-toolbar",
        "#copy-path",
        "a.tooltip",
        ".rustdoc-breadcrumbs",
        // Под заголовком: значки «notable traits», версия, «Source».
        ".main-heading .sub-heading",
        ".notable-traits",
    ];
    for node in main_selection(doc, &mouse) {
        node.remove_from_parent();
    }
    // `where` в сигнатуре — блок внутри кода: в браузере он с новой строки,
    // а htmd окружил бы его пустыми строками.
    for clause in main_selection(doc, &["pre div.where"]) {
        let inner = clause.inner_html();
        clause.replace_with_html(format!("\n{inner}\n"));
    }
    for code in main_selection(doc, &["pre.rust > code"]) {
        if code.attr("data-lang").is_none() {
            code.set_attr("data-lang", "rust");
        }
    }
    // Заголовок — `h1` страницы («Trait Serialize»), без пробелов вёрстки.
    let title = first(&main, "h1")
        .map(|h1| squeeze(&h1.text()))
        .filter(|title| !title.is_empty())
        .or_else(|| {
            first(&root, "title")
                .map(|title| squeeze(&title.text()))
                .filter(|title| !title.is_empty())
        })?;
    Some(Page {
        title,
        html: main.html().to_string(),
    })
}

fn main_selection<'a>(doc: &'a Document, selectors: &[&str]) -> Vec<dom_query::NodeRef<'a>> {
    let within: Vec<String> = selectors
        .iter()
        .map(|selector| format!("#main-content {selector}"))
        .collect();
    doc.select(&within.join(", ")).nodes().to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAGE: &str = "<html><head><meta name=\"generator\" content=\"rustdoc\">\
        <title>Serialize in serde - Rust</title></head><body><nav class=\"sidebar\">\
        <a href=\"#required-methods\">Required Methods</a></nav><main><section id=\"main-content\">\
        <div class=\"main-heading\"><div class=\"rustdoc-breadcrumbs\"><a href=\"index.html\">serde</a></div>\
        <h1>Trait <span class=\"trait\">Serialize</span>&nbsp;<button id=\"copy-path\">Copy item path</button></h1>\
        <rustdoc-toolbar></rustdoc-toolbar><span class=\"sub-heading\"><span class=\"since\">1.0.0</span> · \
        <a class=\"src\" href=\"../src/x.rs.html\">Source</a></span></div>\
        <pre class=\"rust item-decl\"><code>pub trait Serialize {\n    fn <a href=\"#tymethod.serialize\">serialize</a>&lt;S&gt;()<div class=\"where\">where\n    S: Serializer,</div>}</code></pre>\
        <details class=\"toggle top-doc\" open><summary class=\"hideme\"><span>Expand description</span></summary>\
        <div class=\"docblock\"><p>A <strong>data structure</strong>, see <a href=\"ser/index.html\">ser</a>.</p>\
        <div class=\"example-wrap\"><a href=\"#\" class=\"tooltip\" title=\"This example runs\">ⓘ</a>\
        <pre class=\"rust rust-example-rendered\"><code>let x = 1;\nlet y = 2;</code></pre></div></div></details>\
        <h2 id=\"required-methods\">Required Methods</h2><section class=\"method\">\
        <a class=\"src rightside\" href=\"../src/x.rs.html#2\">Source</a><h4 class=\"code-header\">fn serialize</h4></section>\
        </section></main></body></html>";

    #[test]
    fn the_article_is_main_content_without_the_mouse_parts() {
        let article = crate::extract::extract(
            PAGE,
            "https://docs.rs/serde/latest/serde/trait.Serialize.html",
        )
        .unwrap();
        assert_eq!(article.title, "Trait Serialize");
        let markdown = crate::markdown::from_article(&article).unwrap().markdown;
        assert!(markdown.starts_with("# Trait Serialize\n\n```rust\npub trait Serialize {\n    fn serialize<S>()\nwhere\n    S: Serializer,\n}\n```\n"), "{markdown}");
        assert!(
            markdown.contains("[ser](https://docs.rs/serde/latest/serde/ser/index.html)"),
            "{markdown}"
        );
        assert!(markdown.contains("```rust\nlet x = 1;"), "{markdown}");
        assert!(markdown.contains("## Required Methods"), "{markdown}");
        for noise in [
            "Expand description",
            "Copy item path",
            "Source",
            "1.0.0",
            "ⓘ",
            "Required Methods](#",
        ] {
            assert!(!markdown.contains(noise), "{noise}: {markdown}");
        }
    }

    #[test]
    fn only_rustdoc() {
        let other = PAGE.replace("content=\"rustdoc\"", "content=\"Hugo\"");
        assert!(read(&Document::from(other)).is_none());
    }
}
