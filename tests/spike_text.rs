//! Спайк: хватит ли `cosmic-text`, чтобы сделать своими руками то,
//! чего не даёт готовый виджет iced.
//!
//! Вопроса два, и оба решают, менять ли тулкит:
//!
//! 1. можно ли узнать, какая **ссылка под курсором** — от этого зависят
//!    контекстное меню по ссылке и средняя кнопка;
//! 2. можно ли превратить пару точек в **выделенный текст** — это пункт M3
//!    «выделение и копирование», сейчас заблокированный.
//!
//! Спайк намеренно без окна: раскладка и попадание курсора считаются
//! без графики, поэтому ответ получается проверяемым, а не «на глаз».

use cosmic_text::{Attrs, Buffer, FontSystem, Metrics, Shaping};

/// Кегль и интерлиньяж — те же, что в окне.
const SIZE: f32 = 17.0;
const LINE: f32 = 17.0 * 1.55;
const MEASURE: f32 = 560.0;

/// Куски абзаца: текст и номер ссылки (0 — не ссылка).
const PARTS: [(&str, usize); 5] = [
    ("Начало абзаца без всякой ссылки, ", 0),
    ("первая ссылка", 1),
    (", между ними обычный текст, ", 0),
    ("вторая ссылка", 2),
    (", и хвост абзаца после неё.", 0),
];

fn paragraph() -> String {
    PARTS.iter().map(|(text, _)| *text).collect()
}

fn laid_out(fonts: &mut FontSystem) -> Buffer {
    let mut buffer = Buffer::new(fonts, Metrics::new(SIZE, LINE));
    buffer.set_size(fonts, Some(MEASURE), Some(400.0));

    let spans: Vec<(&str, Attrs)> = PARTS
        .iter()
        .map(|(text, link)| {
            let mut attrs = Attrs::new();
            attrs.metadata = *link;
            (*text, attrs)
        })
        .collect();

    buffer.set_rich_text(fonts, spans, &Attrs::new(), Shaping::Advanced, None);
    buffer.shape_until_scroll(fonts, false);
    buffer
}

/// Своя гарнитура из комплекта, а не системная: заодно проверяем, что
/// шрифт можно скормить раскладке напрямую, без fontconfig.
fn font_system() -> FontSystem {
    let mut fonts = FontSystem::new();
    fonts.db_mut().load_font_data(
        include_bytes!("../assets/fonts/PTSerif-Regular.ttf").to_vec(),
    );
    fonts
}

/// Номер ссылки под точкой — по `metadata` глифа, который её накрывает.
fn link_at(buffer: &Buffer, x: f32, y: f32) -> Option<usize> {
    for run in buffer.layout_runs() {
        let top = run.line_top;
        if y < top || y >= top + run.line_height {
            continue;
        }
        for glyph in run.glyphs {
            if x >= glyph.x && x < glyph.x + glyph.w {
                return Some(glyph.metadata);
            }
        }
    }
    None
}

/// Середина глифа, попадающего в заданный кусок абзаца.
fn point_inside(buffer: &Buffer, part: usize) -> (f32, f32) {
    let start: usize = PARTS[..part].iter().map(|(text, _)| text.len()).sum();
    let end = start + PARTS[part].0.len();
    let middle = (start + end) / 2;

    for run in buffer.layout_runs() {
        for glyph in run.glyphs {
            if glyph.start <= middle && middle < glyph.end {
                return (glyph.x + glyph.w / 2.0, run.line_top + run.line_height / 2.0);
            }
        }
    }
    panic!("кусок {part} не нашёлся в раскладке");
}

#[test]
fn a_point_knows_which_link_it_is_over() {
    let mut fonts = font_system();
    let buffer = laid_out(&mut fonts);

    let (x, y) = point_inside(&buffer, 1);
    assert_eq!(link_at(&buffer, x, y), Some(1), "первая ссылка");

    let (x, y) = point_inside(&buffer, 3);
    assert_eq!(link_at(&buffer, x, y), Some(2), "вторая ссылка");

    let (x, y) = point_inside(&buffer, 2);
    assert_eq!(link_at(&buffer, x, y), Some(0), "между ссылками ссылки нет");
}

#[test]
fn two_points_make_a_selection() {
    let mut fonts = font_system();
    let buffer = laid_out(&mut fonts);
    let text = paragraph();

    let (x1, y1) = point_inside(&buffer, 1);
    let (x2, y2) = point_inside(&buffer, 3);

    let from = buffer.hit(x1, y1).expect("начало выделения");
    let to = buffer.hit(x2, y2).expect("конец выделения");
    assert_eq!(from.line, to.line, "абзац лежит одной строкой буфера");

    let (a, b) = if from.index <= to.index {
        (from.index, to.index)
    } else {
        (to.index, from.index)
    };
    let selected = &text[a..b];

    assert!(selected.contains("между ними обычный текст"), "выделено: {selected:?}");
    assert!(!selected.contains("хвост абзаца"), "лишнего не захватили: {selected:?}");
    eprintln!("спайк: выделено {} знаков — {selected:?}", selected.chars().count());
}
