//! Виджет статьи: `GtkTextView`, который умеет рисовать под текстом.
//!
//! Заведён ради одной вещи — вертикальной линейки слева от цитаты. Тегом
//! буфера её не выразить: теги умеют фон абзаца и отступы, но не линию
//! в поле. Альтернативой было вынести цитату в отдельный виджет на якоре,
//! как таблицу, но тогда из неё пропали бы выделение и поиск по странице,
//! а цитату как раз копируют чаще всего.
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
/// Толщина линейки и её место в левом поле цитаты. Шаг уровня тот же,
/// что у отступа самого тега.
const RULE_WIDTH: f32 = 3.0;
const RULE_X: i32 = 8;
const RULE_STEP: i32 = 26;
/// Насколько линейка короче строки сверху и снизу: вплотную к соседям
/// она выглядит сплошной колонкой, а не отметкой цитаты.
const RULE_INSET: f32 = 2.0;

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
                draw_quote_rules(view.upcast_ref(), &snapshot, self.rule.get());
            }
            self.parent_snapshot_layer(layer, snapshot);
        }
    }
}

glib::wrapper! {
    pub struct Article(ObjectSubclass<imp::Article>)
        @extends gtk::TextView, gtk::Widget,
        @implements gtk::Buildable, gtk::ConstraintTarget, gtk::Scrollable;
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
    if quotes.iter().all(Option::is_none) {
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
        for (level, quote) in quotes.iter().enumerate() {
            let Some(quote) = quote else { continue };
            if !line.has_tag(quote) {
                continue;
            }
            // Слой рисуется в координатах буфера: GTK сдвигает снимок
            // на прокрутку сам, и переводить ничего не надо.
            snapshot.append_color(
                &color,
                &gtk::graphene::Rect::new(
                    (RULE_X + RULE_STEP * level as i32) as f32,
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
