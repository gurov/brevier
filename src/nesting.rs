//! Глубина вложенности разметки — до того, как разметку начнут разбирать.
//!
//! Почти всё, что строит из HTML и XML дерево и обходит его, рекурсивно:
//! htmd (HTML → markdown), roxmltree (ленты), usvg (svg), Readability
//! (`dom_smoothie`). На тысяче вложенных элементов у потока кончается стек,
//! и процесс падает целиком, со всеми вкладками, а глубже — и на Linux, где
//! стек больше. Readability на глубине к тому же работает за куб: две тысячи
//! вложенных `<div>` — четверть минуты. Всё это проверено злонамеренными
//! страницами (аудит 10 октября 2026), а чужие крейты в этом месте мы
//! не перепишем.
//!
//! Настоящие страницы глубже сотни-другой уровней не бывают, поэтому порог —
//! с запасом против них и с запасом против стека. Проверка — по байтам,
//! за один проход: строить дерево ради того, чтобы узнать, что его нельзя
//! строить, значило бы попасть в ту же ловушку.

/// Сколько уровней вложенности читаем. Больше — отказ, а не попытка.
pub const MAX_DEPTH: usize = 256;

/// HTML глубже [`MAX_DEPTH`].
///
/// Считаются только элементы, которые закрывают концом: пустые (`<br>`,
/// `<img>`) ничего не открывают, а у `<p>`, `<li>`, `<td>` и им подобных
/// конец необязателен — их закрывает следующий такой же, и считать их значило
/// бы принять старую страницу без `</p>` за глубокую. Конец закрывает всё,
/// что открыто внутри, как и у парсера: `<div><span></div>` не копит глубину.
/// `/>` закрывает сразу — так пишут встроенный svg, а его значки на любой
/// странице. Содержимое `<script>`, `<style>` и комментарии — не разметка.
pub fn html_too_deep(html: &str) -> bool {
    depth(html, true) > MAX_DEPTH
}

/// XML глубже [`MAX_DEPTH`]: лента, svg. Здесь закрыт каждый элемент.
pub fn xml_too_deep(xml: &str) -> bool {
    depth(xml, false) > MAX_DEPTH
}

/// Не открывают ничего.
const VOID: &[&str] = &[
    "area", "base", "basefont", "bgsound", "br", "col", "embed", "frame", "hr", "image", "img",
    "input", "keygen", "link", "meta", "param", "source", "track", "wbr",
];

/// Закрываются сами, следующим таким же или концом родителя.
const SELF_ENDING: &[&str] = &[
    "a", "body", "button", "caption", "colgroup", "dd", "dt", "form", "head", "html", "li", "nobr",
    "optgroup", "option", "p", "rb", "rp", "rt", "rtc", "tbody", "td", "tfoot", "th", "thead",
    "tr",
];

/// Внутри — текст, а не разметка, до своего конца.
const RAW: &[&str] = &[
    "iframe",
    "noembed",
    "noframes",
    "plaintext",
    "script",
    "style",
    "textarea",
    "title",
    "xmp",
];

/// Наибольшая глубина, но не дальше порога: дальше считать незачем.
fn depth(text: &str, html: bool) -> usize {
    let bytes = text.as_bytes();
    let mut open: Vec<&[u8]> = Vec::new();
    let mut deepest = 0;
    let mut at = 0;

    while let Some(found) = find(bytes, at, b"<") {
        let rest = &bytes[found + 1..];
        // Комментарий, CDATA, объявление, инструкция — не элементы.
        if rest.starts_with(b"!--") {
            at = find(bytes, found + 4, b"-->").map_or(bytes.len(), |end| end + 3);
            continue;
        }
        if rest.starts_with(b"![CDATA[") {
            at = find(bytes, found + 9, b"]]>").map_or(bytes.len(), |end| end + 3);
            continue;
        }
        if rest.starts_with(b"!") || rest.starts_with(b"?") {
            at = find(bytes, found + 1, b">").map_or(bytes.len(), |end| end + 1);
            continue;
        }

        let closing = rest.first() == Some(&b'/');
        let start = found + 1 + usize::from(closing);
        let mut end = start;
        while end < bytes.len()
            && (bytes[end].is_ascii_alphanumeric()
                || matches!(bytes[end], b'-' | b'_' | b':' | b'.'))
        {
            end += 1;
        }
        // `a < b`, `<3` — не тег.
        if end == start || !bytes[start].is_ascii_alphabetic() {
            at = found + 1;
            continue;
        }
        let name = &bytes[start..end];
        let (after, empty) = tag_end(bytes, end);
        at = after;

        if html {
            if !closing
                && RAW
                    .iter()
                    .any(|raw| name.eq_ignore_ascii_case(raw.as_bytes()))
            {
                let mut close = b"</".to_vec();
                close.extend_from_slice(name);
                at = find_ignore_case(bytes, at, &close).unwrap_or(bytes.len());
                continue;
            }
            let skipped =
                |list: &[&str]| list.iter().any(|n| name.eq_ignore_ascii_case(n.as_bytes()));
            if skipped(VOID) || skipped(SELF_ENDING) || skipped(RAW) {
                continue;
            }
        }

        if closing {
            let matching = open
                .iter()
                .rposition(|open| open.eq_ignore_ascii_case(name));
            if let Some(index) = matching {
                open.truncate(index);
            }
        } else if !empty {
            open.push(name);
            deepest = deepest.max(open.len());
            if deepest > MAX_DEPTH {
                break;
            }
        }
    }
    deepest
}

/// Конец тега от места после имени: `>` вне кавычек. И пустой ли тег (`/>`).
fn tag_end(bytes: &[u8], from: usize) -> (usize, bool) {
    let mut quote = None;
    let mut index = from;
    while index < bytes.len() {
        let byte = bytes[index];
        match quote {
            Some(open) if byte == open => quote = None,
            Some(_) => {}
            None if byte == b'"' || byte == b'\'' => quote = Some(byte),
            None if byte == b'>' => return (index + 1, index > from && bytes[index - 1] == b'/'),
            None => {}
        }
        index += 1;
    }
    (bytes.len(), false)
}

fn find(bytes: &[u8], from: usize, needle: &[u8]) -> Option<usize> {
    bytes
        .get(from..)?
        .windows(needle.len())
        .position(|window| window == needle)
        .map(|offset| from + offset)
}

fn find_ignore_case(bytes: &[u8], from: usize, needle: &[u8]) -> Option<usize> {
    bytes
        .get(from..)?
        .windows(needle.len())
        .position(|window| window.eq_ignore_ascii_case(needle))
        .map(|offset| from + offset)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nest(open: &str, close: &str, n: usize) -> String {
        format!("{}x{}", open.repeat(n), close.repeat(n))
    }

    #[test]
    fn deep_markup_is_caught() {
        assert!(html_too_deep(&nest("<div>", "</div>", MAX_DEPTH + 1)));
        assert!(html_too_deep(&nest("<b><i>", "</i></b>", MAX_DEPTH)));
        assert!(html_too_deep(&nest(
            "<ul><li>",
            "</li></ul>",
            MAX_DEPTH + 1
        )));
        assert!(xml_too_deep(&nest("<x>", "</x>", MAX_DEPTH + 1)));
        // Не закрыты вовсе — глубина та же, парсер их тоже держит открытыми.
        assert!(html_too_deep(&"<font>".repeat(MAX_DEPTH + 1)));
    }

    #[test]
    fn a_page_at_the_limit_is_read() {
        assert!(!html_too_deep(&nest("<div>", "</div>", MAX_DEPTH)));
        assert!(!xml_too_deep(&nest("<x>", "</x>", MAX_DEPTH)));
    }

    /// Обычные вольности HTML глубины не копят.
    #[test]
    fn loose_html_stays_shallow() {
        // Абзацы и пункты без конца.
        let old = "<ul>".to_owned() + &"<li><p>item".repeat(2000) + "</ul>";
        assert!(!html_too_deep(&old));
        // Конец родителя закрывает забытое внутри.
        assert!(!html_too_deep(&"<div><span>text</div>".repeat(2000)));
        // Перепутанный порядок концов.
        assert!(!html_too_deep(&"<b><i>x</b></i>".repeat(2000)));
        // Значки встроенным svg — сотни `<path/>`.
        let icons = "<svg><path d=\"M0 0\"/><circle r=\"1\"/></svg>".repeat(2000);
        assert!(!html_too_deep(&icons));
        // Пустые элементы, комментарии, скрипт с «тегами» внутри.
        let noise = "<br><img src=\"a>b\"><hr><!-- <div> --><script>if (a<b) document.write('<div>')</script>";
        assert!(!html_too_deep(&noise.repeat(2000)));
        // Ссылки без конца закрывает следующая.
        assert!(!html_too_deep(&"<a href=\"/x\">link ".repeat(2000)));
    }

    #[test]
    fn a_feed_with_a_doctype_is_measured_past_it() {
        let feed = "<?xml version=\"1.0\"?><!DOCTYPE rss [<!ENTITY a \"b\">]><rss><channel><item><title>&a;</title></item></channel></rss>";
        assert!(!xml_too_deep(feed));
        assert_eq!(depth(feed, false), 4);
    }
}
