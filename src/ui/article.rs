//! Виджет статьи: `GtkTextView`, который умеет рисовать под текстом.
//!
//! Заведён ради вертикальной линейки слева от цитаты. Тегом буфера её
//! не выразить: теги умеют фон абзаца и отступы, но не линию в поле.
//! Альтернативой было вынести цитату в отдельный виджет на якоре, как
//! таблицу, но тогда из неё пропали бы выделение и поиск по странице,
//! а цитату как раз копируют чаще всего. Вторая забота пришла позже:
//! фон абзаца у пустых строк, который GTK не рисует сам (#28).
//!
//! Рисуем слоем ниже текста (`snapshot_layer`) — это штатный способ GTK
//! дописать что-то к отрисовке `GtkTextView`, не переписывая её.

use std::cell::Cell;

use gtk::glib;
use gtk::prelude::*;
use gtk::subclass::prelude::*;

/// Теги, у строк которых рисуется линейка: по одному на уровень цитаты.
/// Вложенная цитата получает свою, правее, — иначе ответ на ответ в треде
/// обсуждения не отличить от новой реплики.
const QUOTES: [&str; 3] = ["quote1", "quote2", "quote3"];
/// Линейка без цитаты: строки блока кода внутри неё (#13).
const RULES: [&str; 3] = ["rule1", "rule2", "rule3"];
// Толщина линейки, её место в левом поле цитаты и шаг уровня — в ядре,
// вместе с остальной типографской моделью: телефон рисует ту же линейку.
use brevier::outline::{INDENT as RULE_STEP, RULE_INSET, RULE_WIDTH, RULE_X};
/// Сколько GTK оставляет справа под курсор: фон абзаца кончается на столько
/// левее полной ширины текста (`SPACE_FOR_CURSOR` в gtktextview.c). Фон
/// пустой строки обязан кончаться там же, иначе у панели кода справа зубец.
const CURSOR_SPACE: i32 = 1;

mod imp {
    use super::*;

    pub struct Article {
        /// Цвет линейки. Меняется вместе с темой, поэтому не константа.
        pub rule: Cell<gtk::gdk::RGBA>,
    }

    impl Default for Article {
        fn default() -> Self {
            Self {
                rule: Cell::new(gtk::gdk::RGBA::new(0.6, 0.6, 0.6, 1.0)),
            }
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Article {
        const NAME: &'static str = "BrevierArticle";
        type Type = super::Article;
        type ParentType = gtk::TextView;
    }

    impl ObjectImpl for Article {}
    impl WidgetImpl for Article {}

    impl TextViewImpl for Article {
        fn snapshot_layer(&self, layer: gtk::TextViewLayer, snapshot: gtk::Snapshot) {
            if layer == gtk::TextViewLayer::BelowText {
                let view = self.obj();
                draw_empty_paragraphs(view.upcast_ref(), &snapshot);
                draw_quote_rules(view.upcast_ref(), &snapshot, self.rule.get());
            }
            self.parent_snapshot_layer(layer, snapshot);
        }
    }
}

glib::wrapper! {
    pub struct Article(ObjectSubclass<imp::Article>)
        @extends gtk::TextView, gtk::Widget,
        // `Accessible` в списке обязателен с GTK 4.10 и по делу: любой виджет
        // его реализует, а продукт, обещающий доступность, объявить это
        // должен явно.
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget, gtk::Scrollable;
}

impl Default for Article {
    fn default() -> Self {
        Self::new()
    }
}

impl Article {
    pub fn new() -> Self {
        glib::Object::builder().build()
    }

    /// Цвет линейки цитаты. Задаётся из темы окна.
    pub fn set_rule_color(&self, color: gtk::gdk::RGBA) {
        self.imp().rule.set(color);
        self.queue_draw();
    }
}

/// Линейка слева от каждой строки, помеченной тегом цитаты.
///
/// Идём по видимым строкам, а не по всему документу: длинная статья — это
/// тысячи строк, а на экране их сорок.
fn draw_quote_rules(view: &gtk::TextView, snapshot: &gtk::Snapshot, color: gtk::gdk::RGBA) {
    let buffer = view.buffer();
    let table = buffer.tag_table();
    let quotes: Vec<Option<gtk::TextTag>> = QUOTES.iter().map(|name| table.lookup(name)).collect();
    let rules: Vec<Option<gtk::TextTag>> = RULES.iter().map(|name| table.lookup(name)).collect();
    if quotes.iter().chain(&rules).all(Option::is_none) {
        return;
    }

    let seen = view.visible_rect();
    let bottom = seen.y() + seen.height();
    let (mut line, _) = view.line_at_y(seen.y());

    loop {
        let (top, height) = view.line_yrange(&line);
        if top > bottom {
            break;
        }
        for (level, (quote, rule)) in quotes.iter().zip(&rules).enumerate() {
            let tagged =
                |tag: &Option<gtk::TextTag>| tag.as_ref().is_some_and(|tag| line.has_tag(tag));
            if !tagged(quote) && !tagged(rule) {
                continue;
            }
            // Слой рисуется в координатах буфера: GTK сдвигает снимок
            // на прокрутку сам, и переводить ничего не надо.
            snapshot.append_color(
                &color,
                &gtk::graphene::Rect::new(
                    RULE_X + RULE_STEP * level as f32,
                    top as f32 + RULE_INSET,
                    RULE_WIDTH,
                    (height as f32 - RULE_INSET * 2.0).max(1.0),
                ),
            );
        }
        if !line.forward_line() {
            break;
        }
    }
}

/// Фон абзаца у пустых строк (#28).
///
/// Строку без единого знака GTK не рисует вовсе: `gtk_text_layout_snapshot`
/// пропускает её, пока в ней нет выделения или курсора, — а с ней пропадает
/// и `paragraph-background`. Пустые строки с фоном у нас — поля панели кода
/// сверху и снизу (`pad`) и пустые строки внутри самого кода: панель шла
/// полосами. Красим их сами по тем же правилам, что GTK: цвет и поля — от
/// тега с наибольшим приоритетом, у которого они заданы; прямоугольник —
/// от левого поля до правого края текста, на всю высоту строки вместе
/// с воздухом над и под ней.
fn draw_empty_paragraphs(view: &gtk::TextView, snapshot: &gtk::Snapshot) {
    let seen = view.visible_rect();
    let bottom = seen.y() + seen.height();
    let (mut line, _) = view.line_at_y(seen.y());
    // Полная ширина текста — не ширина окна, а самой широкой строки, если
    // та шире: рамка картинки или таблицы стоит во всю меру и раздвигает
    // раскладку как раз на пиксель под курсор. GTK уже свёл обе величины
    // в верхнюю границу горизонтальной прокрутки.
    let full = view
        .hadjustment()
        .map_or(seen.width(), |scroll| scroll.upper() as i32);

    loop {
        let (top, height) = view.line_yrange(&line);
        if top > bottom {
            break;
        }
        if line.ends_line() {
            // Теги идут по возрастанию приоритета: действует последний.
            let tags = line.tags();
            let last = |set: fn(&gtk::TextTag) -> bool| tags.iter().rev().find(|tag| set(tag));
            if let Some(color) = last(|tag| tag.is_paragraph_background_set())
                .and_then(|tag| tag.paragraph_background_rgba())
            {
                let left = last(|tag| tag.is_left_margin_set())
                    .map_or(view.left_margin(), |tag| tag.left_margin());
                let right = last(|tag| tag.is_right_margin_set())
                    .map_or(view.right_margin(), |tag| tag.right_margin());
                let width = full - CURSOR_SPACE - right - left;
                snapshot.append_color(
                    &color,
                    &gtk::graphene::Rect::new(
                        left as f32,
                        top as f32,
                        width.max(0) as f32,
                        height as f32,
                    ),
                );
            }
        }
        if !line.forward_line() {
            break;
        }
    }
}
