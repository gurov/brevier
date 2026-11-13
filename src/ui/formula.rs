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
//!
//! И третье — показать, что формулу нашёл поиск (#16): находку холст красит
//! сам, поверх себя, с рамкой — фон тега лёг бы под краску, а она тонкая.
//!
//! Своя формула приходит от ядра краской на бумаге, а показывается одной
//! краской (`set_image` с цветами): под ней видно выделение и отметку
//! заметки, как под буквами рядом, а не карточку бумаги посреди них.

use std::cell::{Cell, RefCell};

use gtk::gdk;
use gtk::glib;
use gtk::graphene;
use gtk::gsk;
use gtk::pango;
use gtk::prelude::*;
use gtk::subclass::prelude::*;

use super::sharp::Sharp;

mod imp {
    use super::*;

    pub struct Formula {
        /// Картинка, когда приехала. С плотностью: на экране 2× она разобрана
        /// в его пикселях, а место занимает в точках (#15).
        pub image: RefCell<Option<Sharp>>,
        /// Чем заменить её, пока не приехала: исходник формулы, уже
        /// разложенный. Раскладку делает окно — у холста нет своего
        /// контекста Pango.
        pub fallback: RefCell<Option<pango::Layout>>,
        pub ink: Cell<gdk::RGBA>,
        /// Бумага и краска картинки, если от неё показывать одну краску.
        pub lift: Cell<Option<([u8; 3], [u8; 3])>>,
        /// Цвет находки поиска, если формула найдена.
        pub mark: Cell<Option<gdk::RGBA>>,
    }

    impl Default for Formula {
        fn default() -> Self {
            Self {
                image: RefCell::new(None),
                fallback: RefCell::new(None),
                // Настоящую краску холст получает вместе с исходником:
                // она зависит от темы, а у `RGBA` умолчания нет.
                ink: Cell::new(gdk::RGBA::new(0.1, 0.1, 0.1, 1.0)),
                lift: Cell::new(None),
                mark: Cell::new(None),
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
            if let Some(image) = self.image.borrow().as_ref() {
                return image.width();
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
            if let Some(image) = self.image.borrow().as_ref() {
                return image.height();
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
            if let Some(image) = self.image.borrow().as_ref() {
                match (self.lift.get(), snapshot.downcast_ref::<gtk::Snapshot>()) {
                    (Some((paper, ink)), Some(own)) => {
                        let (matrix, offset) = ink_only(paper, ink);
                        own.push_color_matrix(&matrix, &offset);
                        image.snapshot(snapshot, width, height);
                        own.pop();
                    }
                    _ => image.snapshot(snapshot, width, height),
                }
            } else if let Some(layout) = self.fallback.borrow().clone()
                && let Some(snapshot) = snapshot.downcast_ref::<gtk::Snapshot>()
            {
                snapshot.append_layout(&layout, &self.ink.get());
            }
            // Находка — как маркером: цвет поверх, краска сквозь него видна,
            // и рамка тем же цветом, чтобы найти глазом на тёмной бумаге.
            let (Some(mark), Some(snapshot)) =
                (self.mark.get(), snapshot.downcast_ref::<gtk::Snapshot>())
            else {
                return;
            };
            let bounds = graphene::Rect::new(0.0, 0.0, width as f32, height as f32);
            let mut wash = mark;
            wash.set_alpha(WASH);
            snapshot.append_color(&wash, &bounds);
            // Рамка — снаружи рисунка: формула обрезана впритык, и внутри
            // рамка легла бы на краску.
            let outside = graphene::Rect::new(
                -RULE,
                -RULE,
                width as f32 + 2.0 * RULE,
                height as f32 + 2.0 * RULE,
            );
            snapshot.append_border(
                &gsk::RoundedRect::from_rect(outside, 0.0),
                &[RULE; 4],
                &[mark; 4],
            );
        }
    }
}

/// Матрица, оставляющая от картинки «краска на бумаге» одну краску: доля
/// краски в точке — насколько точка ушла от бумаги к краске (по сумме
/// каналов), она и становится непрозрачностью; цвет — сама краска.
/// GSK считает `transpose(matrix) * pixel + offset`, каналы 0…1.
fn ink_only(paper: [u8; 3], ink: [u8; 3]) -> (graphene::Matrix, graphene::Vec4) {
    let sum = |color: [u8; 3]| color.iter().map(|&c| f32::from(c)).sum::<f32>() / 255.0;
    let (paper, ink_sum) = (sum(paper), sum(ink));
    let span = if paper == ink_sum {
        1.0
    } else {
        paper - ink_sum
    };
    let k = -1.0 / span;
    #[rustfmt::skip]
    let matrix = graphene::Matrix::from_float([
        0.0, 0.0, 0.0, k,
        0.0, 0.0, 0.0, k,
        0.0, 0.0, 0.0, k,
        0.0, 0.0, 0.0, 0.0,
    ]);
    let channel = |c: u8| f32::from(c) / 255.0;
    let offset = graphene::Vec4::new(
        channel(ink[0]),
        channel(ink[1]),
        channel(ink[2]),
        paper / span,
    );
    (matrix, offset)
}

/// Насколько сквозь цвет находки видна формула.
const WASH: f32 = 0.4;
/// Толщина рамки находки, в точках.
const RULE: f32 = 1.5;

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
    /// был размером с исходник формулы или с пиксель. `lift` — бумага
    /// и краска своей формулы: показать от неё одну краску.
    pub fn set_image(&self, image: &Sharp, lift: Option<([u8; 3], [u8; 3])>) {
        self.imp().lift.set(lift);
        self.imp().image.replace(Some(image.clone()));
        self.invalidate_size();
        self.invalidate_contents();
    }

    /// Нашёл ли формулу поиск: цвет находки или `None`.
    pub fn set_mark(&self, mark: Option<gdk::RGBA>) {
        if self.imp().mark.get() != mark {
            self.imp().mark.set(mark);
            self.invalidate_contents();
        }
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
