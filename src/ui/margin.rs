//! Поле заметок (#42): заметки читателя рядом с текстом, как на полях книги.
//!
//! Решено с мейнтейнером 29 октября 2026: справа налево — полка, поле
//! заметок, колонка. Карточка стоит вровень со строкой, где начинается
//! её цитата, и от цитаты к ней ведёт тонкая линия — как примечания в Word
//! и LibreOffice. Места не хватает (узкое окно, рядом полка) — карточки
//! встают под свой абзац: колонку уже меры не ужимаем.
//!
//! **Карточки — дети `GtkTextView` в координатах буфера** (`add_overlay`):
//! прокручиваются вместе с текстом сами. Своего места в тексте у них нет —
//! ни знака объекта, ни строки: смещения буфера, на которых держатся
//! заметки, прогресс чтения, поиск и «назад» на то же место, не сдвигаются.
//! Поле справа — правое поле самого текста (`right_margin`, ставит окно),
//! место под абзацем — его воздух снизу (`pixels-below-lines` своим тегом).
//!
//! У оверлеев GTK (4.20, `gtktextviewchild.c`) три свойства, и все три
//! здесь учтены. **Снять оверлей нельзя**: `gtk_text_view_remove` знает
//! только детей на якорях и на чужом пишет «is not a child». Поэтому
//! оверлеями стоят «гнёзда» — пустые коробки, их заводят раз и прячут,
//! а карточка — ребёнок гнезда, её меняют свободно. **Оверлей получает
//! свой наименьший размер**, а не естественный, и **наименьшая высота
//! любого оверлея — наименьшая высота всего текста**: метка с переносом
//! при наименьшей ширине (одно слово) вытягивалась в столбик, и текст
//! статьи требовал высоты больше окна. Поэтому у карточки наименьшая
//! ширина — её ширина, а высота ограничена: слова — строками,
//! картинки — общим бюджетом, поле правки — прокруткой.
//!
//! **Раскладка — как у Google Docs**: карточки не налезают друг на друга —
//! нижняя уступает вниз; выбранная (щелчком или фокусом) встаёт ровно
//! у своей строки, а соседи расступаются. От наведения порядок не меняется:
//! карточки, прыгающие под мышью, читать нельзя. Линии бледные, у той
//! заметки, на которую смотрят, — в полный цвет.
//!
//! **Слова — меткой, правка — полем**: метка меряет высоту сразу и точно,
//! а у текстового поля высота известна только после раскладки. Щелчок
//! по словам открывает поле; Ctrl+Enter, Escape или уход фокуса — обратно.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::{Rc, Weak};

use gtk::prelude::*;
use gtk::{gdk, gio, glib, graphene, pango};

use brevier::media;

use crate::sharp::Sharp;

/// Заметка, как её видит поле.
#[derive(Debug, Clone, PartialEq)]
pub struct Card {
    pub id: u64,
    /// Цитата в буфере: `[from, to)` в знаках.
    pub place: (i32, i32),
    pub text: String,
    /// Картинки: имя в заметке и файл на диске.
    pub pictures: Vec<(String, PathBuf)>,
}

/// Где показывать заметки.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Form {
    /// Только подложка в тексте: читатель поле выключил.
    Hidden,
    /// Справа от колонки.
    Side,
    /// Под своим абзацем: справа места нет.
    Under,
}

/// Чего карточка просит у окна. Просьба уходит на idle: обработчик фокуса
/// GTK зовёт и посреди перестройки поля, а окно в этот миг держит своё
/// состояние занятым.
pub enum Ask {
    Edit(u64, String),
    Remove(u64),
    /// Картинка из буфера обмена или перетащенная — байтами.
    Attach(u64, Vec<u8>),
    /// Перетащенный файл: читать его — дело окна, не главного потока.
    AttachFile(u64, PathBuf),
    /// Выбрать картинку диалогом.
    Pick(u64),
    Detach(u64, String),
    /// Показать картинку крупно.
    Show(PathBuf),
}

/// Размеры и вид поля: всё это знает окно — от меры, ступени и темы.
#[derive(Debug, Clone, PartialEq)]
pub struct Room {
    /// Ширина текста — мера в пикселях окна.
    pub text: i32,
    /// Промежуток между текстом и карточкой.
    pub gap: i32,
    /// Ширина карточки сбоку.
    pub card: i32,
    /// Кегль слов заметки в точках и гарнитура.
    pub points: f64,
    pub family: String,
    /// Для картинок: бумага под прозрачным, плотность экрана, кегль в px.
    pub paper: [u8; 3],
    pub density: f32,
    pub font_px: f32,
}

impl Room {
    /// Сколько места под картинку внутри карточки этой формы.
    fn inner(&self, form: Form) -> i32 {
        let outer = if form == Form::Under {
            self.text
        } else {
            self.card
        };
        // Поля карточки и черта слева (`page_css`, `.margin-note`).
        (outer - CARD_PADDING).max(16)
    }
}

/// Отступы карточки по горизонтали вместе с чертой: см. `.margin-note`
/// в `page_css`.
pub const CARD_PADDING: i32 = 3 + 10 + 10;

/// Между карточками и между абзацем и карточкой под ним.
const SPACING: i32 = 8;

/// Место кнопки меню справа от слов — не меньше; сколько на деле, решает тема.
const MENU_WIDTH: i32 = 28;
/// Между словами и кнопкой меню.
const HEAD_SPACING: i32 = 2;

/// Сколько строк слов видно на карточке; остальное — в правке. Карточка
/// выше окна сломала бы весь текст (см. шапку).
const SAID_LINES: i32 = 12;

/// Высота поля правки: строк на шесть, дальше — прокрутка.
const EDITOR_HEIGHT: i32 = 130;

/// Сколько высоты на все картинки карточки, в кеглях текста, и не меньше
/// скольки на одну.
const PICTURES_BUDGET: f32 = 18.0;
const PICTURE_LEAST: f32 = 4.0;

/// Линия заметки, на которую не смотрят, — такой доли цвета.
const QUIET: f64 = 0.4;

struct Placed {
    card: Card,
    /// Гнездо-оверлей, в котором стоит карточка (см. шапку).
    slot: gtk::Box,
    root: gtk::Box,
    /// Меню карточки: его действия — у него самого.
    menu: gtk::MenuButton,
    /// Слова: метка, а на время правки — поле.
    said: gtk::Label,
    field: gtk::TextView,
    /// Прокрутка поля правки: её и прячут, и показывают.
    editor: gtk::ScrolledWindow,
    /// Подсказка в пустой новой заметке.
    hint: gtk::Label,
    pictures: gtk::Box,
    /// Где карточка сейчас, в координатах буфера, и её высота.
    x: Cell<i32>,
    y: Cell<i32>,
    height: Cell<i32>,
    /// По какому месту текста карточка встала (`anchor_of`): сдвинулось
    /// оно — пора перекладывать.
    anchor: Cell<i32>,
}

/// Кто отвечает карточкам — окно.
type Answer = Rc<dyn Fn(Ask)>;

/// Кому сказать новую ширину.
type Heard = Rc<dyn Fn(i32)>;

/// Поле одной вкладки. Живёт в `Article` и рисуется им же.
pub struct Margin {
    view: glib::WeakRef<gtk::TextView>,
    me: RefCell<Weak<Margin>>,
    placed: RefCell<Vec<Placed>>,
    form: Cell<Form>,
    room: RefCell<Option<Room>>,
    /// Выбранная заметка: встаёт у своей строки (щелчок, фокус).
    chosen: Cell<Option<u64>>,
    /// Заметка под глазами: её линия — в полный цвет (наведение).
    hovered: Cell<Option<u64>>,
    ask: RefCell<Option<Answer>>,
    /// Теги воздуха под абзацами с заметками: строка буфера → тег и сколько
    /// в нём своего воздуха сверх абзацного.
    gaps: RefCell<HashMap<i32, (gtk::TextTag, i32)>>,
    /// Свободные гнёзда: спрятаны и ждут следующей карточки.
    free: RefCell<Vec<gtk::Box>>,
    queued: Cell<bool>,
    color: Cell<gdk::RGBA>,
}

impl Margin {
    pub fn new(view: &gtk::TextView) -> Rc<Self> {
        let margin = Rc::new(Margin {
            view: view.downgrade(),
            me: RefCell::new(Weak::new()),
            placed: RefCell::new(Vec::new()),
            form: Cell::new(Form::Hidden),
            room: RefCell::new(None),
            chosen: Cell::new(None),
            hovered: Cell::new(None),
            ask: RefCell::new(None),
            gaps: RefCell::new(HashMap::new()),
            free: RefCell::new(Vec::new()),
            queued: Cell::new(false),
            color: Cell::new(gdk::RGBA::new(0.44, 0.56, 0.27, 1.0)),
        });
        margin.me.replace(Rc::downgrade(&margin));
        margin
    }

    /// Куда уходят просьбы карточек.
    pub fn set_ask(&self, ask: impl Fn(Ask) + 'static) {
        self.ask.replace(Some(Rc::new(ask)));
    }

    /// Цвет линий и черты карточки — цвет заметки в теме.
    pub fn set_color(&self, color: gdk::RGBA) {
        self.color.set(color);
        if let Some(view) = self.view.upgrade() {
            view.queue_draw();
        }
    }

    pub fn form(&self) -> Form {
        self.form.get()
    }

    /// Заметка, которую сейчас правят в карточке.
    pub fn editing(&self) -> Option<u64> {
        self.placed
            .borrow()
            .iter()
            .find(|placed| placed.editor.is_visible())
            .map(|placed| placed.card.id)
    }

    /// Показать заметки страницы. Карточки тех же заметок остаются теми же
    /// виджетами — правят только то, что поменялось: перестройка всего
    /// поля на каждую правку сбрасывала бы фокус соседней карточки, в которую
    /// читатель как раз щёлкнул. `fresh` — новая заметка: её карточка есть
    /// и пустой, и в ней сразу курсор.
    pub fn show(&self, cards: Vec<Card>, form: Form, room: Room, fresh: Option<u64>) {
        let Some(view) = self.view.upgrade() else {
            return;
        };
        let changed = self.room.borrow().as_ref() != Some(&room) || self.form.get() != form;
        if changed || form == Form::Hidden {
            // Другая ступень, тема или форма — карточки собираются заново:
            // кегль метки и ширина картинок задаются при сборке.
            self.clear();
        }
        self.form.set(form);
        self.room.replace(Some(room.clone()));
        if form == Form::Hidden {
            return;
        }

        let mut gone = Vec::new();
        {
            let mut placed = self.placed.borrow_mut();
            let mut kept: Vec<Placed> = Vec::new();
            for old in placed.drain(..) {
                if cards.iter().any(|card| card.id == old.card.id) {
                    kept.push(old);
                } else {
                    gone.push(old);
                }
            }
            for card in cards {
                match kept.iter().position(|old| old.card.id == card.id) {
                    Some(index) => {
                        let old = kept.remove(index);
                        self.refresh(&old, &card, &room, form);
                        placed.push(Placed { card, ..old });
                    }
                    None => placed.push(self.build(&view, card, &room, form)),
                }
            }
        }
        // Снимаем вне заёма: уход фокуса с убранной карточки зовёт наш же
        // обработчик.
        for old in gone {
            self.release(old);
        }
        if let Some(id) = fresh {
            self.chosen.set(Some(id));
        }
        self.lay_out();
        if let Some(id) = fresh {
            self.edit(id);
        }
    }

    /// Убрать поле целиком: карточки, воздух под абзацами, линии.
    /// Начатая правка не пропадает: написанное уходит в заметку.
    pub fn clear(&self) {
        let Some(view) = self.view.upgrade() else {
            return;
        };
        let editing: Vec<u64> = self
            .placed
            .borrow()
            .iter()
            .filter(|placed| placed.editor.is_visible())
            .map(|placed| placed.card.id)
            .collect();
        for id in editing {
            self.finish(id, true, false);
        }
        let all: Vec<Placed> = self.placed.borrow_mut().drain(..).collect();
        for placed in all {
            self.release(placed);
        }
        let buffer = view.buffer();
        let (start, end) = buffer.bounds();
        for (tag, _) in self.gaps.borrow_mut().drain().map(|(_, gap)| gap) {
            buffer.remove_tag(&tag, &start, &end);
            buffer.tag_table().remove(&tag);
        }
        self.chosen.set(None);
        self.hovered.set(None);
        view.queue_draw();
    }

    /// Гнездо под карточку: свободное или новое. Новое встаёт оверлеем
    /// раз и навсегда — снять его GTK не даёт.
    fn slot(&self, view: &gtk::TextView) -> gtk::Box {
        if let Some(slot) = self.free.borrow_mut().pop() {
            slot.set_visible(true);
            return slot;
        }
        let slot = gtk::Box::new(gtk::Orientation::Vertical, 0);
        view.add_overlay(&slot, 0, 0);
        slot
    }

    /// Карточку — из гнезда, гнездо — спрятать до следующей.
    fn release(&self, placed: Placed) {
        placed.slot.remove(&placed.root);
        placed.slot.set_visible(false);
        self.free.borrow_mut().push(placed.slot);
    }

    /// Разложить на следующем кадре: высота документа поменялась (картинка
    /// догрузилась, GTK досчитал строки), карточка выросла.
    pub fn queue(&self) {
        // Сначала форма, потом флаг: скрытое поле флаг поднять не должно,
        // иначе раскладку не позвать уже никогда.
        if self.form.get() == Form::Hidden || self.queued.replace(true) {
            return;
        }
        let Some(view) = self.view.upgrade() else {
            return;
        };
        let me = self.me.borrow().clone();
        view.add_tick_callback(move |_, _| {
            if let Some(margin) = me.upgrade() {
                margin.queued.set(false);
                margin.lay_out();
            }
            glib::ControlFlow::Break
        });
    }

    /// Заметка под глазами — наведением на карточку или на цитату.
    pub fn hover(&self, id: Option<u64>) {
        if self.hovered.replace(id) == id {
            return;
        }
        self.mark_active();
    }

    /// Выбрать заметку: щелчок по цитате. Её карточка встаёт у строки.
    pub fn choose(&self, id: u64) -> bool {
        if !self
            .placed
            .borrow()
            .iter()
            .any(|placed| placed.card.id == id)
        {
            return false;
        }
        self.chosen.set(Some(id));
        self.mark_active();
        self.lay_out();
        true
    }

    /// Начать правку слов заметки.
    pub fn edit(&self, id: u64) {
        let list = self.placed.borrow();
        let Some(placed) = list.iter().find(|placed| placed.card.id == id) else {
            return;
        };
        let buffer = placed.field.buffer();
        buffer.set_text(&placed.card.text);
        placed.said.set_visible(false);
        placed.editor.set_visible(true);
        placed
            .hint
            .set_visible(placed.card.text.is_empty() && placed.card.pictures.is_empty());
        placed.root.add_css_class("editing");
        let field = placed.field.clone();
        // Фокус — когда карточка уже стоит и показана: иначе GTK его
        // не отдаёт.
        glib::idle_add_local_once(move || {
            field.grab_focus();
            let buffer = field.buffer();
            buffer.place_cursor(&buffer.end_iter());
        });
        drop(list);
        self.queue();
    }

    fn mark_active(&self) {
        let active = self.hovered.get().or(self.chosen.get());
        for placed in self.placed.borrow().iter() {
            if Some(placed.card.id) == active {
                placed.root.add_css_class("active");
            } else {
                placed.root.remove_css_class("active");
            }
        }
        if let Some(view) = self.view.upgrade() {
            view.queue_draw();
        }
    }

    fn send(&self, ask: Ask) {
        let Some(ask_fn) = self.ask.borrow().clone() else {
            return;
        };
        glib::idle_add_local_once(move || ask_fn(ask));
    }

    // ── карточка ────────────────────────────────────────────────────────────

    fn build(&self, view: &gtk::TextView, card: Card, room: &Room, form: Form) -> Placed {
        let root = gtk::Box::new(gtk::Orientation::Vertical, 6);
        root.add_css_class("margin-note");
        let width = if form == Form::Under {
            room.text
        } else {
            room.card
        };
        root.set_size_request(width, -1);
        let said = gtk::Label::builder()
            .xalign(0.0)
            .wrap(true)
            .wrap_mode(pango::WrapMode::WordChar)
            .max_width_chars(1)
            .lines(SAID_LINES)
            .ellipsize(pango::EllipsizeMode::End)
            .build();
        said.set_attributes(Some(&font(room)));
        said.add_css_class("margin-note-said");
        let field = gtk::TextView::builder()
            .wrap_mode(gtk::WrapMode::WordChar)
            .accepts_tab(false)
            .build();
        field.add_css_class("margin-note-field");
        // Поле правки — окном с прокруткой постоянной высоты: у оверлея
        // высота — наименьшая, и расти вместе с текстом поле не стало бы.
        let editor = gtk::ScrolledWindow::builder()
            .child(&field)
            .hscrollbar_policy(gtk::PolicyType::Never)
            .min_content_height(EDITOR_HEIGHT)
            .visible(false)
            .build();
        let tag = gtk::TextTag::builder()
            .name("say")
            .family(room.family.as_str())
            .size_points(room.points)
            .build();
        field.buffer().tag_table().add(&tag);
        let hint = gtk::Label::builder()
            .label("Write a few words, or paste a picture")
            .xalign(0.0)
            .wrap(true)
            .max_width_chars(1)
            .visible(false)
            .build();
        hint.add_css_class("dim-label");
        hint.add_css_class("caption");

        let menu = gtk::MenuButton::builder()
            .icon_name("view-more-symbolic")
            .has_frame(false)
            .valign(gtk::Align::Start)
            .halign(gtk::Align::End)
            .tooltip_text("Note")
            .build();
        menu.add_css_class("margin-note-menu");
        let model = gio::Menu::new();
        model.append(Some("Edit"), Some("card.edit"));
        model.append(Some("Add a picture…"), Some("card.pick"));
        model.append(Some("Paste a picture"), Some("card.paste"));
        let last = gio::Menu::new();
        last.append(Some("Remove the note"), Some("card.remove"));
        model.append_section(None, &last);
        menu.set_menu_model(Some(&model));

        let head = gtk::Box::new(gtk::Orientation::Horizontal, HEAD_SPACING);
        let words = gtk::Box::new(gtk::Orientation::Vertical, 2);
        words.append(&said);
        words.append(&editor);
        words.append(&hint);
        head.append(&words);
        head.append(&menu);
        root.append(&head);

        let pictures = gtk::Box::new(gtk::Orientation::Vertical, 6);
        root.append(&pictures);

        let slot = self.slot(view);
        slot.append(&root);
        // Наименьшая ширина слов — вся их ширина в карточке (см. шапку),
        // естественная — не больше: длинная заметка одной строкой иначе
        // раздвигала бы весь текст. Кнопку меряем уже в окне, с темой:
        // шире расчётной, она раздвинула бы карточку, а под абзацем — и всю
        // колонку, и окно не сужалось бы до меры.
        let button = menu
            .measure(gtk::Orientation::Horizontal, -1)
            .0
            .max(MENU_WIDTH);
        let words_width = (room.inner(form) - button - HEAD_SPACING).max(16);
        for words in [
            said.upcast_ref::<gtk::Widget>(),
            editor.upcast_ref(),
            hint.upcast_ref(),
        ] {
            words.set_size_request(words_width, -1);
        }
        let placed = Placed {
            card,
            slot,
            root,
            menu,
            said,
            field,
            editor,
            hint,
            pictures,
            x: Cell::new(-1),
            y: Cell::new(-1),
            height: Cell::new(0),
            anchor: Cell::new(i32::MIN),
        };
        self.fill(&placed, &placed.card, room, form);
        self.wire(&placed);
        placed
    }

    /// Слова и картинки заметки — в собранную карточку.
    fn fill(&self, placed: &Placed, card: &Card, room: &Room, form: Form) {
        placed.said.set_text(&card.text);
        let editing = placed.editor.is_visible();
        placed.said.set_visible(!editing && !card.text.is_empty());
        self.fill_pictures(placed, card, room, form);
    }

    /// Картинки карточки — заново. Высоты у них общий бюджет: карточка
    /// не должна выйти выше окна (см. шапку), а крупно — по щелчку.
    fn fill_pictures(&self, placed: &Placed, card: &Card, room: &Room, form: Form) {
        while let Some(child) = placed.pictures.first_child() {
            placed.pictures.remove(&child);
        }
        let count = card.pictures.len().max(1) as f32;
        let tall = (room.font_px * (PICTURES_BUDGET / count).max(PICTURE_LEAST)).round() as i32;
        for (name, path) in &card.pictures {
            placed
                .pictures
                .append(&self.picture(card.id, name, path, room, form, tall));
        }
        placed.pictures.set_visible(!card.pictures.is_empty());
    }

    /// Заметку поправили: обновить карточку, не трогая той, что в правке.
    fn refresh(&self, placed: &Placed, card: &Card, room: &Room, form: Form) {
        if placed.card.text != card.text && !placed.field.has_focus() {
            placed.said.set_text(&card.text);
            placed
                .said
                .set_visible(!placed.editor.is_visible() && !card.text.is_empty());
        }
        if placed.card.pictures != card.pictures {
            self.fill_pictures(placed, card, room, form);
        }
        if !card.pictures.is_empty() {
            placed.hint.set_visible(false);
        }
    }

    /// Картинка заметки: место — сразу, по заголовку файла, пиксели —
    /// из фона. Соседние карточки не прыгают, пока она разбирается.
    fn picture(
        &self,
        id: u64,
        name: &str,
        path: &PathBuf,
        room: &Room,
        form: Form,
        tall: i32,
    ) -> gtk::Widget {
        let inner = room.inner(form);
        let picture = gtk::Picture::builder()
            .can_shrink(true)
            .content_fit(gtk::ContentFit::Contain)
            .halign(gtk::Align::Start)
            .build();
        picture.add_css_class("margin-note-picture");
        picture.set_cursor_from_name(Some("zoom-in"));
        picture.set_tooltip_text(Some("Your picture · click to see it larger"));
        let bytes = std::fs::read(path).unwrap_or_default();
        // Не шире карточки, не выше своей доли бюджета, не крупнее натуры.
        let (shown_w, shown_h) = match media::size(&bytes) {
            Some((w, h)) => {
                let (w, h) = (i64::from(w.max(1)), i64::from(h.max(1)));
                let mut shown_w = w.min(i64::from(inner)).max(1);
                let mut shown_h = (h * shown_w / w).max(1);
                if shown_h > i64::from(tall) {
                    shown_w = (shown_w * i64::from(tall) / shown_h).max(1);
                    shown_h = i64::from(tall);
                }
                (shown_w as i32, shown_h as i32)
            }
            None => (inner, (room.font_px * PICTURE_LEAST) as i32),
        };
        picture.set_size_request(shown_w, shown_h);
        let look = media::Look {
            width: shown_w.max(1) as u32,
            paper: room.paper,
            font_size: room.font_px,
            fit: media::Fit::Column,
            density: room.density,
        };
        {
            let picture = picture.clone();
            glib::spawn_future_local(async move {
                let raster = gio::spawn_blocking(move || media::decode(&bytes, None, look)).await;
                let Ok(Ok(raster)) = raster else {
                    picture.set_alternative_text(Some("The picture could not be read"));
                    return;
                };
                let texture = gdk::MemoryTexture::new(
                    raster.width as i32,
                    raster.height as i32,
                    gdk::MemoryFormat::R8g8b8a8,
                    &glib::Bytes::from_owned(raster.rgba),
                    raster.width as usize * 4,
                );
                picture.set_paintable(Some(&Sharp::new(texture.upcast_ref(), raster.density)));
            });
        }

        let me = self.me.borrow().clone();
        let click = gtk::GestureClick::builder().button(0).build();
        // Своё нажатие — сразу своё: иначе его забрала бы карточка.
        click.connect_pressed(|gesture, _, _, _| {
            gesture.set_state(gtk::EventSequenceState::Claimed);
        });
        {
            let path = path.clone();
            let name = name.to_owned();
            let picture2 = picture.clone();
            click.connect_released(move |gesture, _, x, y| {
                let Some(margin) = me.upgrade() else { return };
                if gesture.current_button() == gdk::BUTTON_SECONDARY {
                    margin.picture_menu(&picture2, id, &name, x, y);
                } else if gesture.current_button() == gdk::BUTTON_PRIMARY {
                    margin.send(Ask::Show(path.clone()));
                }
            });
        }
        picture.add_controller(click);
        picture.upcast()
    }

    fn picture_menu(&self, picture: &gtk::Picture, id: u64, name: &str, x: f64, y: f64) {
        let popover = gtk::Popover::new();
        popover.set_parent(picture);
        popover.set_pointing_to(Some(&gdk::Rectangle::new(x as i32, y as i32, 1, 1)));
        let remove = gtk::Button::builder()
            .label("Remove the picture")
            .has_frame(false)
            .build();
        popover.set_child(Some(&remove));
        {
            let me = self.me.borrow().clone();
            let popover = popover.clone();
            let name = name.to_owned();
            remove.connect_clicked(move |_| {
                popover.popdown();
                if let Some(margin) = me.upgrade() {
                    margin.send(Ask::Detach(id, name.clone()));
                }
            });
        }
        popover.connect_closed(|popover| {
            let popover = popover.clone();
            glib::idle_add_local_once(move || popover.unparent());
        });
        popover.popup();
    }

    /// Обработчики карточки: правка, меню, наведение, перетаскивание.
    fn wire(&self, placed: &Placed) {
        let id = placed.card.id;
        let me = self.me.borrow().clone();

        // Меню карточки.
        let actions = gio::SimpleActionGroup::new();
        let add = |name: &str, run: Box<dyn Fn(&Margin)>| {
            let action = gio::SimpleAction::new(name, None);
            let me = me.clone();
            action.connect_activate(move |_, _| {
                if let Some(margin) = me.upgrade() {
                    run(&margin);
                }
            });
            actions.add_action(&action);
        };
        add("edit", Box::new(move |margin| margin.edit(id)));
        add("pick", Box::new(move |margin| margin.send(Ask::Pick(id))));
        {
            let root = placed.root.clone();
            add(
                "paste",
                Box::new(move |margin| {
                    margin.paste(&root.clipboard(), id, false);
                }),
            );
        }
        add(
            "remove",
            Box::new(move |margin| margin.send(Ask::Remove(id))),
        );
        // Группа — у самой кнопки: вставленную в карточку меню внутри
        // оверлея текста не видело, и пункты стояли серыми (GTK 4.22,
        // проверено 29 октября 2026).
        placed.menu.insert_action_group("card", Some(&actions));

        // Щелчок по словам — правка; любой щелчок по карточке — её выбор,
        // и тексту под ней он не достаётся: справа от меры строка ведь
        // тоже «есть», и ссылка в её конце открылась бы.
        let click = gtk::GestureClick::new();
        {
            let me = me.clone();
            let said = placed.said.clone();
            click.connect_pressed(move |gesture, _, x, y| {
                // Кнопка меню забирает нажатие позже нас: заберём его первыми —
                // меню не откроется.
                let Some(root) = gesture.widget() else { return };
                let on_button = root.pick(x, y, gtk::PickFlags::DEFAULT).is_some_and(|hit| {
                    hit.is::<gtk::Button>() || hit.ancestor(gtk::Button::static_type()).is_some()
                });
                if on_button {
                    return;
                }
                gesture.set_state(gtk::EventSequenceState::Claimed);
                let Some(margin) = me.upgrade() else { return };
                let point = graphene::Point::new(x as f32, y as f32);
                let on_words = said.is_visible()
                    && said
                        .compute_bounds(&root)
                        .is_some_and(|bounds| bounds.contains_point(&point));
                margin.chosen.set(Some(id));
                margin.mark_active();
                if on_words {
                    margin.edit(id);
                } else {
                    margin.queue();
                }
            });
        }
        placed.root.add_controller(click);

        let motion = gtk::EventControllerMotion::new();
        {
            let me = me.clone();
            motion.connect_enter(move |_, _, _| {
                if let Some(margin) = me.upgrade() {
                    margin.hover(Some(id));
                }
            });
        }
        {
            let me = me.clone();
            motion.connect_leave(move |_| {
                if let Some(margin) = me.upgrade()
                    && margin.hovered.get() == Some(id)
                {
                    margin.hover(None);
                }
            });
        }
        placed.root.add_controller(motion);

        // Правка: Ctrl+Enter — оставить, Escape — отменить, уход фокуса —
        // оставить. Ctrl+V с картинкой в буфере кладёт картинку. Ловим
        // в захвате: свой обработчик поля стоит раньше нашего и Ctrl+Enter
        // забирает себе переводом строки.
        let keys = gtk::EventControllerKey::new();
        keys.set_propagation_phase(gtk::PropagationPhase::Capture);
        {
            let me = me.clone();
            let field = placed.field.clone();
            keys.connect_key_pressed(move |_, key, _, modifiers| {
                let Some(margin) = me.upgrade() else {
                    return glib::Propagation::Proceed;
                };
                let ctrl = modifiers.contains(gdk::ModifierType::CONTROL_MASK);
                let shift = modifiers.contains(gdk::ModifierType::SHIFT_MASK);
                match key {
                    gdk::Key::Escape => {
                        margin.finish(id, false, true);
                        glib::Propagation::Stop
                    }
                    gdk::Key::Return | gdk::Key::KP_Enter if ctrl => {
                        margin.finish(id, true, true);
                        glib::Propagation::Stop
                    }
                    gdk::Key::v | gdk::Key::V if ctrl => {
                        if margin.paste(&field.clipboard(), id, true) {
                            glib::Propagation::Stop
                        } else {
                            glib::Propagation::Proceed
                        }
                    }
                    gdk::Key::Insert if shift => {
                        if margin.paste(&field.clipboard(), id, true) {
                            glib::Propagation::Stop
                        } else {
                            glib::Propagation::Proceed
                        }
                    }
                    _ => glib::Propagation::Proceed,
                }
            });
        }
        placed.field.add_controller(keys);
        let focus = gtk::EventControllerFocus::new();
        {
            let me = me.clone();
            focus.connect_enter(move |_| {
                if let Some(margin) = me.upgrade() {
                    margin.chosen.set(Some(id));
                    margin.mark_active();
                }
            });
        }
        {
            let me = me.clone();
            focus.connect_leave(move |_| {
                if let Some(margin) = me.upgrade() {
                    margin.finish(id, true, false);
                }
            });
        }
        placed.field.add_controller(focus);
        {
            let me = me.clone();
            let hint = placed.hint.clone();
            placed.field.buffer().connect_changed(move |buffer| {
                if buffer.char_count() > 0 {
                    hint.set_visible(false);
                }
                let (start, end) = buffer.bounds();
                buffer.apply_tag_by_name("say", &start, &end);
                if let Some(margin) = me.upgrade() {
                    margin.queue();
                }
            });
        }

        // Картинку можно бросить на карточку — файлом или самой картинкой.
        let drop = gtk::DropTarget::new(glib::Type::INVALID, gdk::DragAction::COPY);
        drop.set_types(&[gdk::Texture::static_type(), gdk::FileList::static_type()]);
        {
            let me = me.clone();
            drop.connect_drop(move |_, value, _, _| {
                let Some(margin) = me.upgrade() else {
                    return false;
                };
                if let Ok(texture) = value.get::<gdk::Texture>() {
                    margin.send(Ask::Attach(id, texture.save_to_png_bytes().to_vec()));
                    return true;
                }
                if let Ok(files) = value.get::<gdk::FileList>()
                    && let Some(path) = files.files().first().and_then(|file| file.path())
                {
                    margin.send(Ask::AttachFile(id, path));
                    return true;
                }
                false
            });
        }
        placed.root.add_controller(drop);
    }

    /// Кончить правку: `keep` — оставить написанное, `refocus` — вернуть
    /// клавиатуру статье (Escape, Ctrl+Enter; ушедший сам фокус не трогаем).
    fn finish(&self, id: u64, keep: bool, refocus: bool) {
        let ended = {
            let Ok(placed) = self.placed.try_borrow() else {
                return;
            };
            let Some(placed) = placed.iter().find(|placed| placed.card.id == id) else {
                return;
            };
            if !placed.editor.is_visible() {
                return;
            }
            let buffer = placed.field.buffer();
            let text = buffer
                .text(&buffer.start_iter(), &buffer.end_iter(), false)
                .trim()
                .to_owned();
            placed.editor.set_visible(false);
            placed.hint.set_visible(false);
            placed.root.remove_css_class("editing");
            let shown = if keep { &text } else { &placed.card.text };
            placed.said.set_text(shown);
            placed.said.set_visible(!shown.is_empty());
            // Пустая без картинок — уже не карточка, а подложка: окно
            // уберёт её, перерисовав заметки. Поэтому спрашиваем и тогда,
            // когда слова не поменялись.
            let empty = placed.card.text.is_empty() && placed.card.pictures.is_empty();
            let ask = (keep && text != placed.card.text) || (empty && (text.is_empty() || !keep));
            ask.then(|| if keep { text } else { placed.card.text.clone() })
        };
        // Поле уже спрятано: его уход фокуса сюда вернётся и ничего не тронет.
        if refocus && let Some(view) = self.view.upgrade() {
            view.grab_focus();
        }
        if let Some(text) = ended {
            self.send(Ask::Edit(id, text));
        }
        self.queue();
    }

    /// Картинка из буфера обмена — в заметку. `false` — в буфере её нет
    /// (тогда Ctrl+V вставит текст как обычно).
    fn paste(&self, clipboard: &gdk::Clipboard, id: u64, quiet: bool) -> bool {
        if !clipboard
            .formats()
            .contains_type(gdk::Texture::static_type())
        {
            if !quiet {
                self.send(Ask::Attach(id, Vec::new()));
            }
            return false;
        }
        let me = self.me.borrow().clone();
        clipboard.read_texture_async(gio::Cancellable::NONE, move |texture| {
            let Some(margin) = me.upgrade() else { return };
            let bytes = match texture {
                Ok(Some(texture)) => texture.save_to_png_bytes().to_vec(),
                _ => Vec::new(),
            };
            margin.send(Ask::Attach(id, bytes));
        });
        true
    }

    // ── раскладка ───────────────────────────────────────────────────────────

    /// Расставить карточки по местам.
    pub fn lay_out(&self) {
        let Some(view) = self.view.upgrade() else {
            return;
        };
        let Some(room) = self.room.borrow().clone() else {
            return;
        };
        match self.form.get() {
            Form::Hidden => {}
            Form::Side => self.lay_side(&view, &room),
            Form::Under => self.lay_under(&view),
        }
        view.queue_draw();
    }

    fn lay_side(&self, view: &gtk::TextView, room: &Room) {
        let placed = self.placed.borrow();
        let x = room.text + room.gap;
        // (номер карточки, где хочет встать, высота)
        let mut items: Vec<(usize, i32, i32)> = placed
            .iter()
            .enumerate()
            .map(|(index, placed)| {
                let want = anchor_of(view, &placed.card, Form::Side);
                placed.anchor.set(want);
                (index, want, height_of(placed))
            })
            .collect();
        items.sort_by_key(|&(index, want, _)| (want, index));
        let wants: Vec<i32> = items.iter().map(|item| item.1).collect();
        let heights: Vec<i32> = items.iter().map(|item| item.2).collect();
        let chosen = self.chosen.get().and_then(|id| {
            items
                .iter()
                .position(|&(index, _, _)| placed[index].card.id == id)
        });
        for ((index, _, height), y) in items.iter().zip(stack(&wants, &heights, chosen)) {
            let placed = &placed[*index];
            place(view, placed, x, y);
            placed.height.set(*height);
        }
    }

    fn lay_under(&self, view: &gtk::TextView) {
        let buffer = view.buffer();
        let placed = self.placed.borrow();
        // Карточки по абзацам: строка буфера, где кончается цитата.
        let mut lines: Vec<(i32, Vec<usize>)> = Vec::new();
        for (index, placed) in placed.iter().enumerate() {
            let end = buffer.iter_at_offset((placed.card.place.1 - 1).max(placed.card.place.0));
            let line = end.line();
            match lines.iter_mut().find(|(known, _)| *known == line) {
                Some((_, list)) => list.push(index),
                None => lines.push((line, vec![index])),
            }
        }
        for (_, list) in &mut lines {
            list.sort_by_key(|&index| placed[index].card.place.0);
        }
        // Воздух под абзацем — под его карточки. Свой тег на абзац:
        // высоты карточек у абзацев разные.
        let mut relaid = false;
        let mut gaps = self.gaps.borrow_mut();
        let table = buffer.tag_table();
        let live: Vec<i32> = lines.iter().map(|(line, _)| *line).collect();
        let (start, end) = buffer.bounds();
        gaps.retain(|line, (tag, _)| {
            let keep = live.contains(line);
            if !keep {
                buffer.remove_tag(tag, &start, &end);
                table.remove(tag);
            }
            keep
        });
        for (line, list) in &lines {
            let extra: i32 = list
                .iter()
                .map(|&index| {
                    let height = height_of(&placed[index]);
                    placed[index].height.set(height);
                    height + SPACING
                })
                .sum::<i32>()
                + SPACING;
            let Some(from) = buffer.iter_at_line(*line) else {
                continue;
            };
            let mut to = from;
            if !to.ends_line() {
                to.forward_to_line_end();
            }
            let base = below(view, &from, gaps.get(line).map(|(tag, _)| tag));
            match gaps.get_mut(line) {
                Some((tag, had)) => {
                    if *had != extra || tag.pixels_below_lines() != base + extra {
                        tag.set_pixels_below_lines(base + extra);
                        *had = extra;
                        relaid = true;
                    }
                }
                None => {
                    let tag = gtk::TextTag::builder()
                        .pixels_below_lines(base + extra)
                        .build();
                    table.add(&tag);
                    buffer.apply_tag(&tag, &from, &to);
                    gaps.insert(*line, (tag, extra));
                    relaid = true;
                }
            }
        }
        // Места — от высоты абзаца вместе с нашим воздухом: низ текста
        // и есть верх первой карточки.
        for (line, list) in &lines {
            let Some(from) = buffer.iter_at_line(*line) else {
                continue;
            };
            let Some((tag, _)) = gaps.get(line) else {
                continue;
            };
            // Низ текста абзаца — его высота без всего воздуха снизу.
            let (top, height) = view.line_yrange(&from);
            let mut y = top + height - tag.pixels_below_lines() + SPACING;
            for &index in list {
                place(view, &placed[index], 0, y);
                placed[index]
                    .anchor
                    .set(anchor_of(view, &placed[index].card, Form::Under));
                y += placed[index].height.get() + SPACING;
            }
        }
        drop(gaps);
        drop(placed);
        if relaid {
            // Абзацы раздвинулись — GTK пересчитает их высоту к следующему
            // кадру, тогда и встанем точно.
            self.queue();
        }
    }

    // ── линии ───────────────────────────────────────────────────────────────

    /// Линии от цитат к карточкам. Рисуются слоем под текстом, в координатах
    /// буфера, и только видимые.
    ///
    /// Заодно сверяет места: GTK досчитывает высоты строк понемногу,
    /// картинки статьи догружаются, и строка цитаты уезжает. Сигнала об этом
    /// нет (у документа короче окна не меняется даже прокрутка), а отрисовка
    /// после такого есть всегда — карточки встают заново на следующем кадре.
    pub fn draw(&self, view: &gtk::TextView, snapshot: &gtk::Snapshot) {
        let form = self.form.get();
        if form == Form::Hidden {
            return;
        }
        let Ok(placed) = self.placed.try_borrow() else {
            return;
        };
        if placed
            .iter()
            .any(|placed| anchor_of(view, &placed.card, form) != placed.anchor.get())
        {
            self.queue();
        }
        if form != Form::Side || placed.is_empty() {
            return;
        }
        let Some(room) = self.room.borrow().clone() else {
            return;
        };
        let seen = view.visible_rect();
        let (top, bottom) = (seen.y(), seen.y() + seen.height());
        let buffer = view.buffer();
        let active = self.hovered.get().or(self.chosen.get());
        let cairo = snapshot.append_cairo(&graphene::Rect::new(
            seen.x() as f32,
            seen.y() as f32,
            seen.width() as f32,
            seen.height() as f32,
        ));
        let color = self.color.get();
        for placed in placed.iter() {
            let (from, to) = placed.card.place;
            let start = buffer.iter_at_offset(from);
            let line = view.iter_location(&start);
            let (card_y, card_h) = (placed.y.get(), placed.height.get());
            let y = f64::from(line.y() + line.height()) - 1.5;
            if (y < f64::from(top) && card_y + card_h < top)
                || (y > f64::from(bottom) && card_y > bottom)
            {
                continue;
            }
            // Конец цитаты в её первой строке экрана: дальше линия идёт
            // под строкой к краю колонки, затем — к карточке.
            let mut stop = start;
            view.forward_display_line_end(&mut stop);
            if stop.offset() > to {
                stop = buffer.iter_at_offset(to);
            }
            let x0 = f64::from(view.iter_location(&stop).x());
            let edge = f64::from(room.text) + 4.0;
            let card_x = f64::from(placed.x.get());
            let target = y.clamp(f64::from(card_y) + 10.0, f64::from(card_y + card_h) - 10.0);
            let lit = Some(placed.card.id) == active;
            cairo.set_source_rgba(
                f64::from(color.red()),
                f64::from(color.green()),
                f64::from(color.blue()),
                f64::from(color.alpha()) * if lit { 1.0 } else { QUIET },
            );
            cairo.set_line_width(if lit { 1.5 } else { 1.0 });
            cairo.move_to(x0, y);
            cairo.line_to(edge, y);
            cairo.line_to(card_x - 1.0, target);
            let _ = cairo.stroke();
        }
    }
}

/// Подвинуть гнездо карточки на место.
fn place(view: &gtk::TextView, placed: &Placed, x: i32, y: i32) {
    if placed.x.get() != x || placed.y.get() != y {
        view.move_overlay(&placed.slot, x, y);
    }
    placed.x.set(x);
    placed.y.set(y);
}

/// Место текста, от которого стоит карточка: сбоку — верх строки начала
/// цитаты, под абзацем — низ абзаца вместе с воздухом.
fn anchor_of(view: &gtk::TextView, card: &Card, form: Form) -> i32 {
    let buffer = view.buffer();
    match form {
        Form::Under => {
            // Строка буфера — абзац целиком, со всеми строками экрана.
            let end = buffer.iter_at_offset((card.place.1 - 1).max(card.place.0));
            let (top, height) = view.line_yrange(&end);
            top + height
        }
        _ => view.iter_location(&buffer.iter_at_offset(card.place.0)).y(),
    }
}

/// Высота карточки, какую ей даст GTK: оверлей получает наименьший
/// размер — наименьшую ширину и высоту при ней.
fn height_of(placed: &Placed) -> i32 {
    placed.slot.preferred_size().0.height()
}

/// Воздух под абзацем без нашего тега: от тега с наибольшим приоритетом,
/// у которого он задан, как считает и GTK, иначе — у самого текста.
fn below(view: &gtk::TextView, line: &gtk::TextIter, ours: Option<&gtk::TextTag>) -> i32 {
    line.tags()
        .iter()
        .rev()
        .filter(|tag| Some(*tag) != ours)
        .find(|tag| tag.is_pixels_below_lines_set())
        .map_or(view.pixels_below_lines(), |tag| tag.pixels_below_lines())
}

/// Кегль и гарнитура слов заметки — атрибутами метки: CSS кегля в точках
/// под ступень не знает.
fn font(room: &Room) -> pango::AttrList {
    let list = pango::AttrList::new();
    let mut description = pango::FontDescription::new();
    description.set_family(&room.family);
    description.set_size((room.points * f64::from(pango::SCALE)).round() as i32);
    list.insert(pango::AttrFontDesc::new(&description));
    list
}

/// Где встать карточкам сбоку, по порядку их строк: каждая — у своей строки,
/// но не выше нижней соседки. `chosen` встаёт ровно у своей, а те, что выше,
/// уступают ей вверх — как карточки примечаний в Google Docs.
fn stack(wants: &[i32], heights: &[i32], chosen: Option<usize>) -> Vec<i32> {
    let n = wants.len();
    let mut ys = vec![0; n];
    let mut floor = 0;
    for i in 0..n {
        ys[i] = wants[i].max(floor);
        floor = ys[i] + heights[i] + SPACING;
    }
    let Some(chosen) = chosen.filter(|&chosen| chosen < n) else {
        return ys;
    };
    ys[chosen] = wants[chosen];
    let mut floor = ys[chosen] + heights[chosen] + SPACING;
    for i in chosen + 1..n {
        ys[i] = wants[i].max(floor);
        floor = ys[i] + heights[i] + SPACING;
    }
    let mut ceiling = ys[chosen] - SPACING;
    for i in (0..chosen).rev() {
        ys[i] = ys[i].min(ceiling - heights[i]);
        ceiling = ys[i] - SPACING;
    }
    // Вверх уступать некуда — у начала страницы снова сверху вниз.
    if ys.first().is_some_and(|&y| y < 0) {
        let mut floor = 0;
        for i in 0..n {
            ys[i] = ys[i].max(floor);
            floor = ys[i] + heights[i] + SPACING;
        }
    }
    ys
}

/// Ширина места под статью. У виджетов GTK 4 нет сигнала о своём
/// размере, а поле решает по ширине, встать ли справа: нулевой виджет
/// рядом со статьёй узнаёт её из собственной раскладки и говорит, когда
/// она сменилась, — на idle, не посреди раскладки.
mod watch {
    use super::*;
    use gtk::subclass::prelude::*;

    #[derive(Default)]
    pub struct Watch {
        pub width: Cell<i32>,
        pub on: RefCell<Option<Heard>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Watch {
        const NAME: &'static str = "BrevierWidthWatch";
        type Type = super::Watch;
        type ParentType = gtk::Widget;
    }

    impl ObjectImpl for Watch {}

    impl WidgetImpl for Watch {
        fn measure(&self, _: gtk::Orientation, _: i32) -> (i32, i32, i32, i32) {
            (0, 0, -1, -1)
        }

        fn size_allocate(&self, width: i32, height: i32, baseline: i32) {
            self.parent_size_allocate(width, height, baseline);
            if self.width.replace(width) != width
                && let Some(on) = self.on.borrow().clone()
            {
                glib::idle_add_local_once(move || on(width));
            }
        }
    }
}

glib::wrapper! {
    pub struct Watch(ObjectSubclass<watch::Watch>)
        @extends gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl Default for Watch {
    fn default() -> Self {
        Self::new()
    }
}

impl Watch {
    pub fn new() -> Self {
        let watch: Self = glib::Object::builder().build();
        watch.set_hexpand(true);
        watch
    }

    pub fn set_on(&self, on: impl Fn(i32) + 'static) {
        use gtk::subclass::prelude::ObjectSubclassIsExt;
        self.imp().on.replace(Some(Rc::new(on)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cards_do_not_overlap_and_stay_by_their_lines() {
        // Две заметки на соседних строках: вторая уступает вниз.
        assert_eq!(
            stack(&[100, 110, 400], &[50, 30, 20], None),
            vec![100, 158, 400]
        );
    }

    #[test]
    fn the_chosen_card_stands_by_its_line_and_the_others_make_way() {
        // Выбрали вторую: она у своей строки, первая уходит вверх.
        assert_eq!(
            stack(&[100, 110, 400], &[50, 30, 20], Some(1)),
            vec![52, 110, 400]
        );
        // Наверху уступать некуда — снова сверху вниз.
        assert_eq!(stack(&[0, 10], &[50, 30], Some(1)), vec![0, 58]);
    }
}
