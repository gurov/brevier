//! Холст формулы: `GdkPaintable`, который живёт прямо в буфере текста.
//!
//! Заведён по замеру, а не из красоты. Формула, поставленная виджетом
//! на якорь, стоит `GtkTextView` дорого: на статье с сотней формул
//! (теорема Эрроу в википедии) прокрутка рвётся — p99 кадра 100 мс против
//! 16.7 мс на том же тексте без виджетов. Виджетов там два на формулу,
//! под две сотни на статью, и GTK обязан разместить и снять каждый
//! на каждом кадре.
//!
//! Холст в буфере занимает **тот же один символ**, что и якорь, — значит
//! смещения ссылок, заголовков и совпадений поиска не едут, а это и была
//! причина, по которой в проекте выбрали якорь, а не подмену текста.
//! Виджета при этом не создаётся ни одного.
//!
//! Холст умеет две вещи: показать картинку, когда она приехала, и показать
//! исходник формулы, пока её нет или пока картинки выключены. Второе — тоже
//! не украшение: `{\displaystyle b}` читается плохо, но лучше пустого места
//! посреди фразы.

use std::cell::{Cell, RefCell};

use gtk::gdk;
use gtk::glib;
use gtk::pango;
use gtk::prelude::*;
use gtk::subclass::prelude::*;

mod imp {
    use super::*;

    pub struct Formula {
        /// Картинка, когда приехала.
        pub texture: RefCell<Option<gdk::Texture>>,
        /// Чем заменить её, пока не приехала: исходник формулы, уже
        /// разложенный. Раскладку делает окно — у холста нет своего
        /// контекста Pango.
        pub fallback: RefCell<Option<pango::Layout>>,
        pub ink: Cell<gdk::RGBA>,
    }

    impl Default for Formula {
        fn default() -> Self {
            Self {
                texture: RefCell::new(None),
                fallback: RefCell::new(None),
                // Настоящую краску холст получает вместе с исходником:
                // она зависит от темы, а у `RGBA` умолчания нет.
                ink: Cell::new(gdk::RGBA::new(0.1, 0.1, 0.1, 1.0)),
            }
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Formula {
        const NAME: &'static str = "BrevierFormula";
        type Type = super::Formula;
        type Interfaces = (gdk::Paintable,);
    }

    impl ObjectImpl for Formula {}

    impl PaintableImpl for Formula {
        fn intrinsic_width(&self) -> i32 {
            if let Some(texture) = self.texture.borrow().as_ref() {
                return texture.width();
            }
            match self.fallback.borrow().as_ref() {
                Some(layout) => layout.pixel_size().0.max(1),
                // Не ноль: пустой холст нулевой ширины GTK разложить не может,
                // и это ровно та жалоба про снимок без раскладки, которую
                // раньше давала пустая рамка.
                None => 1,
            }
        }

        fn intrinsic_height(&self) -> i32 {
            if let Some(texture) = self.texture.borrow().as_ref() {
                return texture.height();
            }
            match self.fallback.borrow().as_ref() {
                // Не вся высота раскладки, а до базовой линии: `GtkTextView`
                // ставит холст нижним краем ровно на базовую линию строки,
                // и пустой запас на выносные снизу поднимал бы текст над ней.
                // Та же причина, по которой в ядре срезается пустой запас
                // у svg с формулой.
                Some(layout) => (layout.baseline() / pango::SCALE).max(1),
                None => 1,
            }
        }

        fn snapshot(&self, snapshot: &gdk::Snapshot, width: f64, height: f64) {
            if let Some(texture) = self.texture.borrow().as_ref() {
                texture.snapshot(snapshot, width, height);
                return;
            }
            let Some(layout) = self.fallback.borrow().clone() else {
                return;
            };
            let Some(snapshot) = snapshot.downcast_ref::<gtk::Snapshot>() else {
                return;
            };
            snapshot.append_layout(&layout, &self.ink.get());
        }
    }
}

glib::wrapper! {
    pub struct Formula(ObjectSubclass<imp::Formula>) @implements gdk::Paintable;
}

impl Default for Formula {
    fn default() -> Self {
        Self::new()
    }
}

impl Formula {
    pub fn new() -> Self {
        glib::Object::builder().build()
    }

    /// Картинка приехала. Меняются и содержимое, и размер: до этого холст
    /// был размером с исходник формулы или с пиксель.
    pub fn set_texture(&self, texture: &gdk::Texture) {
        self.imp().texture.replace(Some(texture.clone()));
        self.invalidate_size();
        self.invalidate_contents();
    }

    /// Чем показывать формулу, пока картинки нет. Раскладка приходит готовой:
    /// её делает окно, которое знает и гарнитуру, и кегль страницы.
    pub fn set_fallback(&self, layout: Option<pango::Layout>, ink: gdk::RGBA) {
        self.imp().ink.set(ink);
        self.imp().fallback.replace(layout);
        self.invalidate_size();
        self.invalidate_contents();
    }
}
