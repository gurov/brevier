//! HTML → Markdown.
//!
//! Markdown здесь — внутреннее представление, а не формат файлов, и диалект
//! объявлен: CommonMark + GFM.
//!
//! Обратной печати через comrak в тракте вывода нет, хотя соблазн был:
//! канонизировать результат конвертации выглядело правильным. Но writer comrak
//! экранирует `!`, `_`, `[`, `<` безусловно (`cm.rs`), и русский текст
//! превращается в «публикация\!». На M1 это съест рендерер и никто не заметит,
//! а на M0 вывод читают глазами в `less` — там это ровно тот мусор вёрстки,
//! который рубрика велит считать дефектом. comrak остаётся парсером: на нём
//! собираются ссылки, на нём же будет рендер.

use comrak::nodes::NodeValue;
use comrak::{Arena, Options};
use htmd::HtmlToMarkdown;
use htmd::options::{
    BulletListMarker, CodeBlockStyle, HeadingStyle, LinkStyle, Options as HtmdOptions,
};

use crate::error::Error;
use crate::extract::Article;

/// Теги, из которых нечего читать. Readability большую часть уже вырезал,
/// но `--raw` идёт мимо него.
const SKIP: &[&str] = &[
    "script", "style", "noscript", "iframe", "svg", "form", "button", "object", "embed", "canvas",
];

/// Статья: заголовок, автор, текст.
pub fn from_article(article: &Article) -> Result<String, Error> {
    let body = to_markdown(&article.content_html)?;

    let mut doc = String::with_capacity(body.len() + 128);
    let title = article.title.trim();
    if !title.is_empty() {
        doc.push_str("# ");
        doc.push_str(title);
        doc.push_str("\n\n");
    }
    if let Some(byline) = &article.byline {
        doc.push_str(byline.trim());
        doc.push_str("\n\n");
    }
    doc.push_str(body.trim());
    doc.push('\n');

    Ok(tidy(&doc))
}

/// Страница целиком, без Readability (`--raw`). Нужен, чтобы отличать
/// «извлечение промахнулось» от «конвертация промахнулась».
pub fn from_html(html: &str) -> Result<String, Error> {
    Ok(tidy(&to_markdown(html)?))
}

/// Убрать то, что htmd оставляет после вырезанных элементов: хвостовые пробелы
/// и дыры в три и больше пустых строк.
fn tidy(md: &str) -> String {
    let mut out = String::with_capacity(md.len());
    let mut blanks = 0;

    for line in md.lines() {
        let line = line.trim_end();
        if line.is_empty() {
            blanks += 1;
            if blanks > 1 {
                continue;
            }
        } else {
            blanks = 0;
        }
        out.push_str(line);
        out.push('\n');
    }

    let trimmed = out.trim_end().to_owned();
    if trimmed.is_empty() {
        trimmed
    } else {
        trimmed + "\n"
    }
}

/// Исходящие ссылки статьи, в порядке появления, без повторов.
///
/// Нужны для навигационного замера M0: страница может конвертироваться
/// идеально, а все ссылки с неё вести в SPA, пейволлы и PDF.
pub fn links(md: &str) -> Vec<String> {
    let arena = Arena::new();
    let root = comrak::parse_document(&arena, md, &options());

    let mut seen = Vec::new();
    for node in root.descendants() {
        let NodeValue::Link(link) = &node.data.borrow().value else {
            continue;
        };
        let url = &link.url;
        if (url.starts_with("http://") || url.starts_with("https://")) && !seen.contains(url) {
            seen.push(url.clone());
        }
    }
    seen
}

fn to_markdown(html: &str) -> Result<String, Error> {
    let converter = HtmlToMarkdown::builder()
        .skip_tags(SKIP.to_vec())
        .options(HtmdOptions {
            heading_style: HeadingStyle::Atx,
            bullet_list_marker: BulletListMarker::Dash,
            link_style: LinkStyle::Inlined,
            code_block_style: CodeBlockStyle::Fenced,
            ..Default::default()
        })
        .build();

    converter.convert(html).map_err(Error::Convert)
}

/// CommonMark + GFM. Таблицы — обязательная часть, из-за них GFM и выбран.
fn options() -> Options<'static> {
    let mut options = Options::default();
    options.extension.table = true;
    options.extension.strikethrough = true;
    options.extension.autolink = true;
    options.extension.tasklist = true;
    // Ширина колонки — дело рендерера и читателя, не файла: строка = абзац,
    // так диффы корпуса показывают правку, а не переливание переносов.
    options.render.width = 0;
    options
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tables_survive_the_round_trip() {
        let md =
            from_html("<table><tr><th>a</th><th>b</th></tr><tr><td>1</td><td>2</td></tr></table>")
                .unwrap();
        assert!(md.contains("| a | b |"), "таблица потерялась:\n{md}");
    }

    #[test]
    fn headings_and_links() {
        let md = from_html(r#"<h2>Titl</h2><p>text <a href="https://e.com">link</a></p>"#).unwrap();
        assert!(md.contains("## Titl"));
        assert!(md.contains("[link](https://e.com)"));
    }

    #[test]
    fn text_is_not_over_escaped() {
        // comrak на этом месте выдавал «Ура\!» и «a\_b».
        let md = from_html("<p>Ура! Вот так: a_b, 3 &lt; 5.</p>").unwrap();
        assert_eq!(md, "Ура! Вот так: a\\_b, 3 < 5.\n");
    }

    #[test]
    fn blank_line_runs_collapse() {
        let md = from_html("<p>раз</p><div></div><div></div><p>два</p>").unwrap();
        assert_eq!(md, "раз\n\nдва\n");
    }

    #[test]
    fn links_are_collected_once_and_absolute_only() {
        let md = from_html(
            r#"<p><a href="https://e.com/a">1</a> <a href="https://e.com/a">1</a>
               <a href="/relative">2</a> <a href="mailto:x@e.com">3</a></p>"#,
        )
        .unwrap();
        assert_eq!(links(&md), vec!["https://e.com/a".to_owned()]);
    }

    #[test]
    fn scripts_are_dropped() {
        let md = from_html("<p>text</p><script>alert('x')</script>").unwrap();
        assert!(!md.contains("alert"), "скрипт просочился:\n{md}");
    }
}
