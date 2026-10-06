//! Картинка с плотностью: текстура в пикселях экрана, место — в точках окна.
//!
//! `GdkTexture` своей плотности не знает: её размер — число пикселей,
//! и ровно столько места она просит у раскладки. На экране 2× картинка,
//! разобранная под него, встала бы вдвое крупнее, а разобранная в точках
//! окна выходила мылом (#15). Здесь размер делится на плотность, а рисуется
//! текстура целиком — на экран она ложится пиксель в пиксель.

use std::cell::{Cell, RefCell};

use gtk::gdk;
use gtk::glib;
use gtk::prelude::*;
use gtk::subclass::prelude::*;

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct Sharp {
        pub texture: RefCell<Option<gdk::Texture>>,
        /// Сколько пикселей текстуры на точку окна.
        pub density: Cell<f64>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Sharp {
        const NAME: &'static str = "BrevierSharp";
        type Type = super::Sharp;
        type Interfaces = (gdk::Paintable,);
    }

    impl ObjectImpl for Sharp {}

    impl Sharp {
        fn points(&self, pixels: impl Fn(&gdk::Texture) -> i32) -> i32 {
            self.texture.borrow().as_ref().map_or(0, |texture| {
                (f64::from(pixels(texture)) / self.density.get().max(1e-3))
                    .round()
                    .max(1.0) as i32
            })
        }
    }

    impl PaintableImpl for Sharp {
        fn intrinsic_width(&self) -> i32 {
            self.points(|texture| texture.width())
        }

        fn intrinsic_height(&self) -> i32 {
            self.points(|texture| texture.height())
        }

        fn intrinsic_aspect_ratio(&self) -> f64 {
            self.texture.borrow().as_ref().map_or(0.0, |texture| {
                f64::from(texture.width()) / f64::from(texture.height().max(1))
            })
        }

        fn snapshot(&self, snapshot: &gdk::Snapshot, width: f64, height: f64) {
            if let Some(texture) = self.texture.borrow().as_ref() {
                texture.snapshot(snapshot, width, height);
            }
        }
    }
}

glib::wrapper! {
    pub struct Sharp(ObjectSubclass<imp::Sharp>) @implements gdk::Paintable;
}

impl Sharp {
    pub fn new(texture: &gdk::Texture, density: f32) -> Self {
        let sharp: Self = glib::Object::builder().build();
        sharp.imp().texture.replace(Some(texture.clone()));
        sharp.imp().density.set(f64::from(density));
        sharp
    }

    /// Ширина в точках окна — столько картинка и займёт.
    pub fn width(&self) -> i32 {
        self.intrinsic_width()
    }

    /// Высота в точках окна.
    pub fn height(&self) -> i32 {
        self.intrinsic_height()
    }
}
