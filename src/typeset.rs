//! Типографика по языку страницы: переносы и неразрывные пробелы.
//!
//! За фичей `typeset` (её тянет `ui`): cli печатает markdown как есть,
//! типографику задаёт окно, и паттерны переносов в бинарнике `brevier`
//! ни к чему.
//!
//! Всё здесь — вёрстка, а не правка: сохранённый markdown остаётся
//! байт-в-байт тем же. Знаки, которые тут расставляются, живут только
//! в буфере окна:
//!
//! - **мягкий перенос** (U+00AD) — Pango рвёт строку в этом месте, если надо,
//!   и сам рисует дефис; расставляем по словарю переносов, выбранному
//!   языком страницы;
//! - **неразрывный пробел** (U+00A0) — приклеивает однобуквенный предлог
//!   или союз к следующему слову, чтобы он не висел в конце строки; русская
//!   и чешская традиция, которой не делает ни одна читалка.
//!
//! Цена записана в роадмапе и здесь: буфер видят копирование, поиск и место
//! чтения. Поэтому наружу вместе с [`typeset`] идёт [`plain`] — она снимает
//! оба знака обратно, и ею пользуются копирование (в буфер знаки, в буфер
//! обмена — чистый текст) и поиск (ищем по тексту без переносов). Длину
//! правило про предлоги не меняет (пробел на пробел), а переносы меняют —
//! поэтому смещения места чтения считаются по тому же буферу, в который знаки
//! уже вставлены: пока перенос детерминирован, буфер один и тот же и смещения
//! не съезжают.

use hyphenation::{Hyphenator, Language, Load, Standard};

/// Мягкий перенос: разрешённое место разрыва, дефис Pango дорисует сам.
const SOFT_HYPHEN: char = '\u{00AD}';
/// Неразрывный пробел: разрыва строки здесь не будет.
const NBSP: char = '\u{00A0}';

/// Готовый к работе набор правил для одного языка. Словарь переносов
/// загружается один раз — при создании, — а не на каждое слово: в окне
/// `shape` зовётся на каждый прозаический кусок абзаца, и десериализовать
/// словарь столько же раз было бы расточительством.
pub struct Typesetter {
    dict: Option<Standard>,
    glue: bool,
}

impl Typesetter {
    /// Набор для языка страницы. `None`, если делать нечего вовсе — нет ни
    /// словаря, ни правила про предлоги: тогда окно и не зовёт `shape`.
    pub fn for_language(lang: &str) -> Option<Self> {
        let dict = dictionary(lang);
        let glue = glues(lang);
        (dict.is_some() || glue).then_some(Typesetter { dict, glue })
    }

    /// Расставить типографику в куске прозы. Сперва клеим однобуквенные
    /// предлоги (по обычному пробелу), потом переносим слова — перенос
    /// неразрывный пробел не трогает, он не буква.
    pub fn shape(&self, text: &str) -> String {
        let glued = if self.glue {
            keep_together(text)
        } else {
            text.to_owned()
        };
        match &self.dict {
            Some(dict) => hyphenate(&glued, dict),
            None => glued,
        }
    }
}

/// Расставить типографику по языку страницы разом. Словарь при этом грузится
/// заново — для окна есть [`Typesetter`], который держит его между вызовами.
pub fn typeset(text: &str, lang: &str) -> String {
    Typesetter::for_language(lang).map_or_else(|| text.to_owned(), |ts| ts.shape(text))
}

/// Снять типографские знаки обратно: мягкие переносы — вон, неразрывный
/// пробел — обычным. Этим текстом пользуются копирование и поиск: в буфере
/// знаки стоят, а читателю в буфер обмена и в строку поиска идёт чистый текст.
pub fn plain(text: &str) -> String {
    text.chars()
        .filter(|&c| c != SOFT_HYPHEN)
        .map(|c| if c == NBSP { ' ' } else { c })
        .collect()
}

/// Есть ли в буфере наши знаки — чтобы копирование и поиск не чистили строку
/// впустую, когда переносов не было (язык неизвестен).
pub fn marked(text: &str) -> bool {
    text.contains(SOFT_HYPHEN) || text.contains(NBSP)
}

/// Русский словарь лежит ассетом, а не вшит фичей: крейт умеет вшить либо
/// один английский (`embed_en-us`), либо все ~70 языков (`embed_all`, +2.2 МБ).
/// Нам нужны два, поэтому английский берём фичей, а русский — этим файлом
/// (42 КБ), как и шрифты: своё добро возим сами. Провенанс — assets/hyph/NOTICE.md.
const RUSSIAN: &[u8] = include_bytes!("../assets/hyph/ru.standard.bincode");

/// Язык страницы → словарь переносов. Поддержаны английский и русский;
/// остальным переносов нет — и `--check` про отсутствующий `lang` уже
/// говорит. Тег берём до первого дефиса: `en-GB` и `ru-RU` — те же правила.
fn dictionary(lang: &str) -> Option<Standard> {
    match primary(lang).as_str() {
        "en" => Standard::from_embedded(Language::EnglishUS).ok(),
        "ru" => {
            // `from_reader` читает тот же формат, что вшивает `from_embedded`.
            let mut bytes = RUSSIAN;
            Standard::from_reader(Language::Russian, &mut bytes).ok()
        }
        _ => None,
    }
}

/// Языки, где однобуквенное слово в конце строки — дефект набора.
fn glues(lang: &str) -> bool {
    matches!(primary(lang).as_str(), "ru" | "cs")
}

/// Тег до региона, в нижнем регистре: `en-GB` и `EN` — тот же английский.
fn primary(lang: &str) -> String {
    lang.trim()
        .split(['-', '_'])
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase()
}

/// Расставить мягкие переносы по словам. Слово — подряд идущие буквы; всё
/// прочее (пробелы, пунктуация, цифры, наш неразрывный пробел) проходит
/// как есть.
fn hyphenate(text: &str, dict: &Standard) -> String {
    let mut out = String::with_capacity(text.len() + text.len() / 8);
    let mut word = String::new();
    for ch in text.chars() {
        if ch.is_alphabetic() {
            word.push(ch);
        } else {
            if !word.is_empty() {
                push_hyphenated(&mut out, &word, dict);
                word.clear();
            }
            out.push(ch);
        }
    }
    if !word.is_empty() {
        push_hyphenated(&mut out, &word, dict);
    }
    out
}

/// Одно слово с мягкими переносами в местах, которые указал словарь.
/// Точки разрыва приходят байтовыми смещениями внутри слова и всегда
/// стоят на границе символов — вставка по ним безопасна.
fn push_hyphenated(out: &mut String, word: &str, dict: &Standard) {
    let breaks = dict.hyphenate(word).breaks;
    let mut last = 0;
    for point in breaks {
        out.push_str(&word[last..point]);
        out.push(SOFT_HYPHEN);
        last = point;
    }
    out.push_str(&word[last..]);
}

/// Приклеить однобуквенный предлог/союз к следующему слову: пробел после
/// него становится неразрывным. Длина строки при этом не меняется — знак
/// на знак, — поэтому смещения в буфере остаются на месте.
///
/// Однобуквенным слово считается, если перед буквой — начало строки или
/// не-буквенно-цифровой знак (пробел, скобка, кавычка), а после буквы —
/// пробел, за которым есть ещё слово.
fn keep_together(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());

    for i in 0..chars.len() {
        let c = chars[i];
        let single_letter_before = c == ' '
            // перед пробелом — буква
            && chars.get(i.wrapping_sub(1)).is_some_and(|p| p.is_alphabetic())
            // и это слово из одной буквы: до буквы начало строки или граница
            && (i < 2 || !chars[i - 2].is_alphanumeric())
            // и дальше есть к чему приклеивать
            && chars.get(i + 1).is_some_and(|n| !n.is_whitespace());

        out.push(if single_letter_before { NBSP } else { c });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_languages_get_a_dictionary() {
        assert!(dictionary("ru").is_some());
        assert!(dictionary("en-US").is_some());
        assert!(dictionary("EN").is_some());
        assert!(dictionary("de").is_none());
        assert!(dictionary("").is_none());
    }

    #[test]
    fn a_long_word_gets_soft_hyphens() {
        let out = typeset("hyphenation", "en");
        assert!(out.contains(SOFT_HYPHEN), "{out:?}");
        // Снятые обратно — то же слово, что было.
        assert_eq!(plain(&out), "hyphenation");
    }

    #[test]
    fn russian_words_are_hyphenated_too() {
        let out = typeset("переносится", "ru");
        assert!(out.contains(SOFT_HYPHEN), "{out:?}");
        assert_eq!(plain(&out), "переносится");
    }

    #[test]
    fn one_letter_prepositions_are_glued_in_russian() {
        let out = typeset("я в лесу", "ru");
        // Копирование и поиск увидят обычный текст, знаки — только в буфере.
        assert_eq!(plain(&out), "я в лесу");
        // Одиночные «я» и «в» приклеены неразрывным пробелом; после «лесу»
        // клеить не к чему. Ровно два неразрывных — проверяем счётом, потому
        // что «лесу» переносы могут разбить, и подстроки уже не найти.
        assert_eq!(out.matches(NBSP).count(), 2, "{out:?}");
        assert!(out.starts_with("я\u{00A0}в\u{00A0}"), "{out:?}");
    }

    #[test]
    fn gluing_is_only_for_single_letter_words() {
        // «по» — два знака, не клеим.
        let out = keep_together("по лесу");
        assert_eq!(out, "по лесу");
        // В конце строки клеить не к чему.
        assert_eq!(keep_together("иду и"), "иду и");
    }

    #[test]
    fn english_does_not_get_the_preposition_rule() {
        // Правило русское и чешское; в английском одиночные «a»/«I» не клеим.
        let out = typeset("a house", "en");
        assert!(!out.contains(NBSP), "{out:?}");
    }

    #[test]
    fn an_unknown_language_is_left_untouched() {
        let out = typeset("das Wort", "de");
        assert_eq!(out, "das Wort");
        assert!(!marked(&out));
    }

    #[test]
    fn plain_strips_both_marks() {
        assert_eq!(plain("pro\u{00AD}gram\u{00A0}me"), "program me");
        assert!(!marked(&plain("a\u{00AD}b\u{00A0}c")));
    }
}
