//! Цвета страницы.
//!
//! Заданы здесь, а не взяты у темы системы, по той же причине, по которой
//! в комплекте едут гарнитуры: вид задаёт читатель, а не система. И живут
//! в ядре, а не в окне: цвет бумаги, ссылки и приглушённого текста — часть
//! типографики, и на телефоне статья обязана выглядеть той же, что на
//! десктопе.
//!
//! Бумага цвета слоновой кости, а не белая: чистый белый на экране светится,
//! а тёплый тон это свечение снимает, не трогая контраст — краска остаётся
//! почти чёрной. Тёмная тема подобрана в тот же тёплый ряд, иначе переключение
//! выглядит сменой продукта, а не света.
//!
//! Высокий контраст (#44) — второй набор рядом с обычным, а не правка
//! обычного: белая бумага и чёрная краска (в тёмной теме — наоборот), и всё
//! прочее затемнено под них так, что любой текст держит 7:1 (WCAG AAA).
//! Обычный набор держит свою планку — 4.5:1 (AA): приглушённое на слоновой
//! кости было 4.4, комментарии кода — 3.4. Обе планки сверяют тесты.

/// Найденное поиском. Цвета одни на обе темы и оба контраста: подсветка
/// обязана читаться везде, а жёлтый маркер узнаётся без объяснений.
pub const FOUND: &str = "#f2d47e";
pub const FOUND_HERE: &str = "#f6a13c";
pub const FOUND_INK: &str = "#1c1a17";

/// Чем печатать: тема и контраст. Решает читатель — контраст, пока он сам
/// не выбрал, решает система, — и одно на все вкладки: это настройка
/// читателя, а не страницы.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Theme {
    pub dark: bool,
    /// Высокий контраст: белая бумага и чёрная краска, в тёмной теме —
    /// чёрная бумага и белая краска.
    pub high: bool,
}

/// Краски одной темы. Собраны в одном месте, потому что меняются вместе.
pub struct Colors {
    pub paper: &'static str,
    pub ink: &'static str,
    /// Оглавлению отличаться можно: это не страница, а полка рядом с ней.
    pub shelf: &'static str,
    pub link: &'static str,
    pub dim: &'static str,
    /// Подложка блока кода и кода в строке.
    pub panel: &'static str,
    pub keyword: &'static str,
    pub literal: &'static str,
    pub number: &'static str,
    pub comment: &'static str,
    /// Линейки таблицы.
    pub rule: &'static str,
    /// Строка полки под глазами и строка под курсором. В ряду бумаги,
    /// а не в синем ряду темы: полка стоит вплотную к странице.
    pub chosen: &'static str,
    pub touched: &'static str,
    /// Заметка читателя (#39): подложка отмеченного и черта под тем, к чему
    /// приписаны слова. Зелёный маркер, а не жёлтый: жёлтый — найденное
    /// поиском, и одно не должно путаться с другим.
    pub note: &'static str,
    pub note_rule: &'static str,
}

pub fn colors(theme: Theme) -> Colors {
    match (theme.dark, theme.high) {
        (false, false) => Colors {
            paper: "#faf5ea",
            ink: "#23201c",
            shelf: "#f3ecdd",
            link: "#0d6a9e",
            dim: "#70685c",
            panel: "#f2ead9",
            keyword: "#7b3fa0",
            literal: "#1c7539",
            number: "#9a5518",
            comment: "#70675a",
            rule: "#e2d9c6",
            chosen: "#e7dabc",
            touched: "#efe7d6",
            note: "#dfeccb",
            note_rule: "#6f8f45",
        },
        (true, false) => Colors {
            paper: "#1d1b19",
            ink: "#ded8cf",
            shelf: "#171514",
            link: "#8ec4d4",
            dim: "#958c80",
            panel: "#26231f",
            keyword: "#c79bd4",
            literal: "#8fbf8f",
            number: "#dda15e",
            comment: "#9a9185",
            rule: "#3d3833",
            chosen: "#332e27",
            touched: "#252220",
            note: "#33402a",
            note_rule: "#8fae6a",
        },
        // Белая бумага светится — это и есть цена, которую за контраст
        // платит читатель, попросивший его. Тёплого ряда здесь нет: серое
        // на белом — самое тёмное, что не спорит с краской.
        (false, true) => Colors {
            paper: "#ffffff",
            ink: "#000000",
            shelf: "#f2f2f2",
            link: "#0a4a78",
            dim: "#404040",
            panel: "#f0f0f0",
            keyword: "#5f1f8a",
            literal: "#0f5c2a",
            number: "#7a3a00",
            comment: "#404040",
            rule: "#858585",
            chosen: "#dedede",
            touched: "#e9e9e9",
            note: "#d3efb5",
            note_rule: "#4a7a1a",
        },
        (true, true) => Colors {
            paper: "#000000",
            ink: "#ffffff",
            shelf: "#141414",
            link: "#9fd2ff",
            dim: "#c0c0c0",
            panel: "#1a1a1a",
            keyword: "#e2b6ff",
            literal: "#a6e3a6",
            number: "#ffc27a",
            comment: "#c0c0c0",
            rule: "#707070",
            chosen: "#303030",
            touched: "#242424",
            note: "#23401a",
            note_rule: "#9ad16f",
        },
    }
}

/// Бумага, на которую ложится прозрачное в картинке: светлая бумага того же
/// контраста, и в тёмной теме тоже. Не белое: белая карточка посреди
/// слоновой кости заметна, а схеме нужен только светлый фон — на тёмной
/// теме тем более. Высокому контрасту — белая.
pub fn picture_paper(theme: Theme) -> [u8; 3] {
    rgb(colors(Theme {
        dark: false,
        ..theme
    })
    .paper)
}

/// `#rrggbb` в три байта. Цвета записаны так, как их читают глазами,
/// а декодеру картинок нужны числа.
pub fn rgb(hex: &str) -> [u8; 3] {
    let hex = hex.trim_start_matches('#');
    if hex.len() < 6 {
        return [255, 255, 255];
    }
    let byte = |at: usize| u8::from_str_radix(&hex[at..at + 2], 16).unwrap_or(255);
    [byte(0), byte(2), byte(4)]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Относительная яркость по WCAG 2.
    fn luminance(hex: &str) -> f64 {
        let channel = |byte: u8| {
            let c = f64::from(byte) / 255.0;
            if c <= 0.040_45 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        };
        let [r, g, b] = rgb(hex);
        0.2126 * channel(r) + 0.7152 * channel(g) + 0.0722 * channel(b)
    }

    fn contrast(a: &str, b: &str) -> f64 {
        let (a, b) = (luminance(a), luminance(b));
        (a.max(b) + 0.05) / (a.min(b) + 0.05)
    }

    const THEMES: [Theme; 4] = [
        Theme {
            dark: false,
            high: false,
        },
        Theme {
            dark: true,
            high: false,
        },
        Theme {
            dark: false,
            high: true,
        },
        Theme {
            dark: true,
            high: true,
        },
    ];

    #[test]
    fn every_text_reads_at_its_level() {
        for theme in THEMES {
            let c = colors(theme);
            // Любой текст: AA у обычного набора, AAA у высокого.
            let least = if theme.high { 7.0 } else { 4.5 };
            let pairs = [
                ("ink", c.ink, c.paper),
                ("link", c.link, c.paper),
                ("dim", c.dim, c.paper),
                ("ink on the shelf", c.ink, c.shelf),
                ("dim on the shelf", c.dim, c.shelf),
                ("ink on code", c.ink, c.panel),
                ("dim on code", c.dim, c.panel),
                ("keyword", c.keyword, c.panel),
                ("literal", c.literal, c.panel),
                ("number", c.number, c.panel),
                ("comment", c.comment, c.panel),
                ("ink on a note", c.ink, c.note),
                ("ink on a chosen row", c.ink, c.chosen),
            ];
            for (what, fore, back) in pairs {
                let ratio = contrast(fore, back);
                assert!(ratio >= least, "{theme:?}: {what} {ratio:.2} < {least}");
            }
            assert!(contrast(FOUND_INK, FOUND) >= 7.0);
        }
    }

    #[test]
    fn high_contrast_rules_are_seen_too() {
        // Линейки и черта заметки — не текст: им WCAG просит 3:1.
        for theme in THEMES.into_iter().filter(|theme| theme.high) {
            let c = colors(theme);
            assert!(contrast(c.rule, c.paper) >= 3.0, "{theme:?}");
            assert!(contrast(c.rule, c.shelf) >= 3.0, "{theme:?}");
            assert!(contrast(c.note_rule, c.paper) >= 3.0, "{theme:?}");
        }
    }

    #[test]
    fn pictures_lie_on_light_paper_of_their_contrast() {
        let dark = Theme {
            dark: true,
            high: false,
        };
        assert_eq!(picture_paper(dark), rgb("#faf5ea"));
        let high = Theme {
            dark: true,
            high: true,
        };
        assert_eq!(picture_paper(high), [255, 255, 255]);
    }
}
