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

pub const PAPER_DARK: &str = "#1d1b19";
pub const INK_DARK: &str = "#ded8cf";
pub const PAPER_LIGHT: &str = "#faf5ea";
pub const INK_LIGHT: &str = "#23201c";
/// Оглавлению отличаться можно: это не страница, а полка рядом с ней.
pub const SHELF_DARK: &str = "#171514";
pub const SHELF_LIGHT: &str = "#f3ecdd";
/// Найденное поиском. Цвета одни на обе темы: подсветка обязана читаться
/// и там и там, а жёлтый маркер узнаётся без объяснений.
pub const FOUND: &str = "#f2d47e";
pub const FOUND_HERE: &str = "#f6a13c";
pub const FOUND_INK: &str = "#1c1a17";

/// Краски, зависящие от темы. Всё, что не бумага и не краска текста:
/// ссылка, приглушённое, подложка кода, четыре цвета подсветки и линейка
/// таблицы. Собраны в одном месте, потому что меняются вместе.
pub struct Colors {
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
    /// Строка полки под глазами и строка под курсором. В тёплом ряду
    /// бумаги, а не в синем ряду темы: полка стоит вплотную к странице.
    pub chosen: &'static str,
    pub touched: &'static str,
}

pub fn colors(dark: bool) -> Colors {
    if dark {
        Colors {
            link: "#8ec4d4",
            dim: "#958c80",
            panel: "#26231f",
            keyword: "#c79bd4",
            literal: "#8fbf8f",
            number: "#dda15e",
            comment: "#8a8175",
            rule: "#3d3833",
            chosen: "#332e27",
            touched: "#252220",
        }
    } else {
        Colors {
            link: "#0d6a9e",
            dim: "#7a7266",
            panel: "#f2ead9",
            keyword: "#7b3fa0",
            literal: "#1f7a3d",
            number: "#9a5518",
            comment: "#857c6e",
            rule: "#e2d9c6",
            chosen: "#e7dabc",
            touched: "#efe7d6",
        }
    }
}

/// Бумага и краска текста по теме.
pub fn paper_and_ink(dark: bool) -> (&'static str, &'static str) {
    if dark {
        (PAPER_DARK, INK_DARK)
    } else {
        (PAPER_LIGHT, INK_LIGHT)
    }
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
