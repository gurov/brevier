//! Минимальная подсветка кода.
//!
//! Не парсер и не претендует: комментарий, строка, число, ключевое слово —
//! четыре вещи, которые видно в любом языке и которых хватает, чтобы блок
//! кода читался как код, а не как серая стена. Настоящая подсветка требует
//! грамматик на каждый язык (`syntect` тащит ониговую C-регэкспку — ровно
//! то, чего в проекте про memory safety быть не должно), а выигрыш для
//! читателя, который код не правит, а читает, невелик.
//!
//! Ядро, а не интерфейс: разметка кода от тулкита не зависит и проверяется
//! обычными тестами.

/// Что нашли. Всё остальное — обычный текст кода.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Comment,
    /// Строковый литерал вместе с кавычками.
    Literal,
    Number,
    Keyword,
}

/// Кусок кода: границы в байтах и что это.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
    pub kind: Kind,
}

/// Слова, которые красим. Список общий на все языки — так он остаётся
/// коротким и не превращается в грамматику. Слово, которое в одном языке
/// ключевое, а в другом имя, покрасится лишний раз: цена этого — один
/// цветной идентификатор, а не сломанный разбор.
const KEYWORDS: &[&str] = &[
    "abstract", "and", "as", "assert", "async", "await", "begin", "bool", "break", "case",
    "catch", "char", "class", "const", "constructor", "continue", "def", "default", "defer",
    "del", "delete", "do", "double", "dyn", "elif", "else", "elseif", "end", "enum", "except",
    "export", "extends", "extern", "false", "final", "finally", "float", "fn", "for", "foreach",
    "from", "func", "function", "go", "goto", "if", "impl", "implements", "import", "in", "int",
    "interface", "is", "lambda", "let", "loop", "macro", "match", "mod", "module", "move", "mut",
    "namespace", "new", "nil", "none", "not", "null", "object", "or", "package", "pass", "private",
    "protected", "pub", "public", "raise", "readonly", "ref", "return", "select", "self", "static",
    "str", "string", "struct", "super", "switch", "template", "then", "this", "throw", "trait",
    "true", "try", "type", "typedef", "union", "unless", "unsafe", "until", "use", "using", "val",
    "var", "virtual", "void", "when", "where", "while", "with", "yield",
];

/// Чем в этом языке пишут комментарии и строки.
struct Rules {
    /// Начала однострочных комментариев.
    line: &'static [&'static str],
    /// Пара для блочного комментария.
    block: Option<(&'static str, &'static str)>,
    /// Кавычки, которые открывают строку.
    quotes: &'static [char],
    /// Одинарная кавычка тут — символ, а не строка (`'x'`, `'\n'`).
    ///
    /// Различать обязательно: в расте `&'a str` — время жизни, и если считать
    /// одинарную кавычку началом строки, покрасится пол-строки кода. Языку,
    /// которого мы не знаем, приписываем осторожное «символ»: потерянная
    /// подсветка строки безобиднее покрашенного мусора.
    chars_only: bool,
}

const SLASHES: &[&str] = &["//"];
const HASH: &[&str] = &["#"];
const DASHES: &[&str] = &["--"];
const PERCENT: &[&str] = &["%"];
const C_BLOCK: Option<(&str, &str)> = Some(("/*", "*/"));

/// Правила по имени языка из ограждения. Незнакомый язык получает общий
/// набор: `//` и `/* */` встречаются чаще всего, а `#` в незнакомом языке
/// слишком часто оказывается не комментарием (препроцессор, цвет в CSS,
/// заголовок в markdown), поэтому его туда не берём.
fn rules(language: &str) -> Rules {
    let language = language
        .split([' ', ',', ';'])
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();

    match language.as_str() {
        "python" | "py" | "ruby" | "rb" | "sh" | "bash" | "zsh" | "shell" | "console" | "yaml"
        | "yml" | "toml" | "ini" | "conf" | "perl" | "r" | "make" | "makefile" | "dockerfile"
        | "nix" | "elixir" | "ex" | "julia" | "jl" | "awk" | "cmake" | "tcl" | "fish" => Rules {
            line: HASH,
            block: None,
            quotes: &['"', '\'', '`'],
            chars_only: false,
        },
        "sql" | "lua" | "haskell" | "hs" | "elm" | "ada" => Rules {
            line: DASHES,
            block: None,
            quotes: &['"', '\''],
            chars_only: false,
        },
        "erlang" | "erl" | "tex" | "latex" | "matlab" | "prolog" => Rules {
            line: PERCENT,
            block: None,
            quotes: &['"', '\''],
            chars_only: false,
        },
        "html" | "xml" | "svg" | "vue" => Rules {
            line: &[],
            block: Some(("<!--", "-->")),
            quotes: &['"', '\''],
            chars_only: false,
        },
        "css" | "scss" | "less" => Rules {
            line: &[],
            block: C_BLOCK,
            quotes: &['"', '\''],
            chars_only: false,
        },
        "js" | "javascript" | "jsx" | "ts" | "typescript" | "tsx" | "php" | "groovy" | "json" => {
            Rules {
                line: SLASHES,
                block: C_BLOCK,
                quotes: &['"', '\'', '`'],
                chars_only: false,
            }
        }
        // Раст, си и родня: одинарная кавычка — символ. Сюда же попадает
        // незнакомый язык.
        _ => Rules {
            line: SLASHES,
            block: C_BLOCK,
            quotes: &['"', '\''],
            chars_only: true,
        },
    }
}

/// Разметить код. Куски не пересекаются и идут по порядку; всё, что между
/// ними, — обычный текст.
pub fn spans(code: &str, language: &str) -> Vec<Span> {
    let rules = rules(language);
    let bytes = code.as_bytes();
    let mut spans = Vec::new();
    let mut i = 0;

    while i < bytes.len() {
        // ── комментарий до конца строки
        if let Some(open) = rules
            .line
            .iter()
            .find(|open| code[i..].starts_with(**open))
        {
            let _ = open;
            let end = code[i..].find('\n').map(|at| i + at).unwrap_or(code.len());
            spans.push(Span {
                start: i,
                end,
                kind: Kind::Comment,
            });
            i = end;
            continue;
        }

        // ── блочный комментарий
        if let Some((open, close)) = rules.block
            && code[i..].starts_with(open)
        {
            let end = code[i + open.len()..]
                .find(close)
                .map(|at| i + open.len() + at + close.len())
                .unwrap_or(code.len());
            spans.push(Span {
                start: i,
                end,
                kind: Kind::Comment,
            });
            i = end;
            continue;
        }

        let ch = code[i..].chars().next().unwrap_or('\0');

        // ── строка. Незакрытую до конца строки не считаем строкой вовсе:
        // так `'a` в расте остаётся временем жизни, а не красит полфайла.
        if rules.quotes.contains(&ch)
            && let Some(end) = closing(code, i, ch, rules.chars_only)
        {
            spans.push(Span {
                start: i,
                end,
                kind: Kind::Literal,
            });
            i = end;
            continue;
        }

        // ── число или слово
        if ch.is_ascii_digit() && !starts_inside_word(code, i) {
            let end = word_end(code, i);
            spans.push(Span {
                start: i,
                end,
                kind: Kind::Number,
            });
            i = end;
            continue;
        }
        if (ch.is_alphabetic() || ch == '_') && !starts_inside_word(code, i) {
            let end = word_end(code, i);
            if KEYWORDS.contains(&&code[i..end]) {
                spans.push(Span {
                    start: i,
                    end,
                    kind: Kind::Keyword,
                });
            }
            i = end;
            continue;
        }

        i += ch.len_utf8();
    }
    spans
}

/// Где закрывается строка, начатая в `open`. `None`, если до конца строки
/// она так и не закрылась — и если одинарная кавычка в этом языке означает
/// символ, а внутри оказался не символ.
fn closing(code: &str, open: usize, quote: char, chars_only: bool) -> Option<usize> {
    let end = closes_at(code, open, quote)?;
    if chars_only && quote == '\'' {
        let inside = &code[open + 1..end - 1];
        let single = inside.chars().count() == 1;
        let escape = inside.starts_with('\\') && inside.len() <= 8;
        if !single && !escape {
            return None;
        }
    }
    Some(end)
}

fn closes_at(code: &str, open: usize, quote: char) -> Option<usize> {
    let mut escaped = false;
    for (offset, ch) in code[open + quote.len_utf8()..].char_indices() {
        let at = open + quote.len_utf8() + offset;
        match ch {
            _ if escaped => escaped = false,
            '\\' => escaped = true,
            '\n' => return None,
            _ if ch == quote => return Some(at + ch.len_utf8()),
            _ => {}
        }
    }
    None
}

/// Стоим ли посреди слова: `utf8` — одно слово, а не «utf» и «8».
fn starts_inside_word(code: &str, at: usize) -> bool {
    code[..at]
        .chars()
        .next_back()
        .is_some_and(|ch| ch.is_alphanumeric() || ch == '_')
}

fn word_end(code: &str, from: usize) -> usize {
    code[from..]
        .char_indices()
        .find(|(_, ch)| !(ch.is_alphanumeric() || *ch == '_'))
        .map(|(at, _)| from + at)
        .unwrap_or(code.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn painted<'a>(code: &'a str, language: &str) -> Vec<(Kind, &'a str)> {
        spans(code, language)
            .into_iter()
            .map(|span| (span.kind, &code[span.start..span.end]))
            .collect()
    }

    #[test]
    fn four_things_and_no_more() {
        let code = "let x = 42; // тут ответ\nlet s = \"строка\";";
        assert_eq!(
            painted(code, "rust"),
            vec![
                (Kind::Keyword, "let"),
                (Kind::Number, "42"),
                (Kind::Comment, "// тут ответ"),
                (Kind::Keyword, "let"),
                (Kind::Literal, "\"строка\""),
            ]
        );
    }

    #[test]
    fn a_lifetime_is_not_a_string() {
        // Кавычка, которая не закрылась до конца строки, — не строка.
        // Иначе `'a` красил бы весь остаток файла.
        assert_eq!(
            painted("fn f<'a>(x: &'a str) -> char { 'y' }", "rust"),
            vec![
                (Kind::Keyword, "fn"),
                (Kind::Keyword, "str"),
                (Kind::Keyword, "char"),
                (Kind::Literal, "'y'"),
            ]
        );
    }

    #[test]
    fn the_hash_is_a_comment_only_where_it_is_one() {
        assert_eq!(painted("x = 1  # готово", "python").last(), Some(&(Kind::Comment, "# готово")));
        // В незнакомом языке решётка комментарием не считается: слишком часто
        // это препроцессор или цвет.
        assert!(!painted("#include <stdio.h>", "").iter().any(|(kind, _)| *kind == Kind::Comment));
    }

    #[test]
    fn a_block_comment_can_span_lines() {
        assert_eq!(
            painted("a /* два\nряда */ b", "c"),
            vec![(Kind::Comment, "/* два\nряда */")]
        );
        // Незакрытый — до конца текста, а не до первой попавшейся строки.
        assert_eq!(painted("/* хвост", "c"), vec![(Kind::Comment, "/* хвост")]);
    }

    #[test]
    fn a_word_is_taken_whole() {
        // «utf8» — одно слово, а не «utf» и число.
        assert!(painted("let utf8 = to_utf8(x);", "rust").iter().all(|(kind, _)| *kind != Kind::Number));
        // «format» не ключевое, «for» внутри него — тем более.
        assert_eq!(painted("format(x)", "rust"), vec![]);
    }

    #[test]
    fn html_gets_its_own_comment() {
        assert_eq!(
            painted("<p>a</p> <!-- прячем --> <b>b</b>", "html"),
            vec![(Kind::Comment, "<!-- прячем -->")]
        );
    }
}
