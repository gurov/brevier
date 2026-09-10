//! Окно Brevier на GTK4.
//!
//! Статья рисуется одним `GtkTextView`, а не набором виджетов на абзац:
//! выделение должно идти через весь документ, а не обрываться на границе
//! абзаца. Тем же решением бесплатно приходят копирование, контекстное меню,
//! точное оглавление по меткам в тексте и доступность через AT-SPI.
//!
//! Тулкит живёт только здесь. Разбор адреса, история, оглавление и тексты
//! ошибок лежат в ядре и про GTK не знают ничего.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

mod article;
mod formula;

use article::Article;
use formula::Formula;

use gtk::gio;
use gtk::glib;
use gtk::pango;
use gtk::prelude::*;
use gtk::{Application, ApplicationWindow};

use comrak::nodes::{ListType, NodeValue, TableAlignment};

use brevier::address::{self, Address};
use brevier::code;
use brevier::failure::describe;
use brevier::media::{self, Raster, Source};
use brevier::outline::{
    HEADING_WEIGHTS, HEADINGS, LINE_HEIGHT, MAX_WAYPOINTS, MEASURE, MIN_HEADINGS, TEXT_SIZE,
    anchor, clip, lead,
};
use brevier::save;
use brevier::{Document, History, UserAgent};

const APP_ID: &str = "dev.brevier.Brevier";
const BODY_FAMILY: &str = "Noto Sans";
const MONO_FAMILY: &str = "Noto Sans Mono";
/// Жирность в единицах Pango: свойство тега — целое, а не перечисление.
const BOLD: i32 = 700;
const TOC_WIDTH: i32 = 260;
/// Короче этого оглавление не нужно: страница и так вся под рукой.
const MIN_DOC_CHARS: i32 = 4000;
/// Сколько знаков влезает на корешок вкладки.
const TAB_LABEL: usize = 24;
/// Сколько совпадений подсвечиваем. Дальше это уже не поиск, а заливка.
const MAX_HITS: usize = 2000;
/// Метка, которой прокручивают буфер: одна на все прыжки.
const JUMP: &str = "brevier-jump";
/// Сколько кадров ждём, пока картинки и таблицы займут своё место.
const SETTLE_FRAMES: u8 = 45;
/// Глубже этого вложенные списки не отступают: место кончается.
const LIST_LEVELS: i32 = 3;
/// Куда по высоте окна ставить заголовок, к которому прыгнули: вплотную
/// к кромке он выглядит обрезанным.
const ANCHOR_ALIGN: f64 = 0.1;

/// Цвета страницы. Заданы здесь, а не взяты у темы GTK, по той же причине,
/// по которой в комплекте едут гарнитуры: вид задаёт читатель, а не система.
/// Заодно уходит разнобой, из-за которого поля вокруг колонки текста красила
/// тема, а саму колонку — виджет текста.
///
/// Бумага цвета слоновой кости, а не белая: чистый белый на экране светится,
/// а тёплый тон это свечение снимает, не трогая контраст — краска остаётся
/// почти чёрной. Тёмная тема подобрана в тот же тёплый ряд, иначе переключение
/// выглядит сменой продукта, а не света.
const PAPER_DARK: &str = "#1d1b19";
const INK_DARK: &str = "#ded8cf";
const PAPER_LIGHT: &str = "#faf5ea";
const INK_LIGHT: &str = "#23201c";
/// Оглавлению отличаться можно: это не страница, а полка рядом с ней.
const SHELF_DARK: &str = "#171514";
const SHELF_LIGHT: &str = "#f3ecdd";
/// Найденное поиском. Цвета одни на обе темы: подсветка обязана читаться
/// и там и там, а жёлтый маркер узнаётся без объяснений.
const FOUND: &str = "#f2d47e";
const FOUND_HERE: &str = "#f6a13c";
const FOUND_INK: &str = "#1c1a17";

fn main() -> glib::ExitCode {
    brevier::init_crypto();
    use_bundled_fonts();

    let app = Application::builder()
        .application_id(APP_ID)
        // Адреса разбираем сами, поэтому GTK их трогать не должен.
        .flags(gio::ApplicationFlags::HANDLES_COMMAND_LINE)
        .build();

    app.connect_command_line(|app, command_line| {
        let start: Vec<String> = command_line
            .arguments()
            .into_iter()
            .skip(1)
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect();
        build(app, start);
        glib::ExitCode::SUCCESS
    });

    app.run()
}

/// Виджеты окна. Живут отдельно от данных: GTK-виджеты сами по себе
/// разделяемые и изменяемые, а вот состояние вкладок требует `RefCell`,
/// и держать их вместе значило бы занимать заём на каждый чих.
#[derive(Clone)]
struct Ui {
    window: ApplicationWindow,
    notebook: gtk::Notebook,
    entry: gtk::Entry,
    back: gtk::Button,
    forward: gtk::Button,
    contents: gtk::ListBox,
    contents_pane: gtk::ScrolledWindow,
    show_contents: gtk::ToggleButton,
    dark_mode: gtk::ToggleButton,
    show_images: gtk::ToggleButton,
    save: gtk::Button,
    /// Строка состояния внизу: что сохранилось, что не загрузилось.
    notice: gtk::Label,
    /// Поиск по странице: строка внизу окна, как в браузерах.
    search: gtk::SearchBar,
    needle: gtk::SearchEntry,
    tally: gtk::Label,
    /// Свои цвета страницы. Тема GTK красит окно, эта таблица — текст.
    paint: gtk::CssProvider,
}

/// Вкладка: своя статья, своя история, своё место в тексте.
///
/// Место хранить не нужно: у каждой вкладки собственный `ScrolledWindow`,
/// и он помнит прокрутку сам.
struct Tab {
    id: u64,
    view: gtk::TextView,
    label: gtk::Label,
    history: History,
    /// Ссылки в тексте: где начинается, где кончается, куда ведёт.
    links: Vec<Link>,
    /// Куда прыгать по оглавлению — смещения в буфере, а не доли высоты.
    marks: Vec<Mark>,
    /// Якоря заголовков: по ним находится место для ссылки вида `#anchor`.
    anchors: Vec<(String, i32)>,
    /// Места картинок в тексте.
    shots: Vec<Shot>,
    /// Что показано. Нужно для сохранения: на экране текст уже разложен
    /// по буферу, а на диск ложится markdown.
    document: Option<Document>,
    /// Номер загрузки. Ответ брошенной страницы отличаем по нему: отменить
    /// синхронный `ureq` нечем, но и слушать его уже незачем.
    generation: u64,
}

struct State {
    tabs: Vec<Tab>,
    next_id: u64,
    /// Тема общая для всех вкладок: это настройка читателя, а не страницы.
    dark: bool,
    /// Грузить ли картинки. Читатель волен выключить их кнопкой в панели:
    /// после отказа от JS декодер картинок — единственная серьёзная
    /// поверхность атаки, и закрыть её должно быть чем.
    images: bool,
    /// Поиск: строка одна на окно, поэтому и состояние одно.
    search: Search,
}

/// Что нашёл поиск и на котором совпадении стоим.
#[derive(Default)]
struct Search {
    hits: Vec<(i32, i32)>,
    at: usize,
}

/// Место картинки в тексте.
///
/// В буфере на её месте стоит якорь с контейнером: сначала в нём заглушка,
/// после загрузки — сама картинка с подписью. Контейнер, а не текст, потому
/// что подменять текст пришлось бы вместе со всеми смещениями ссылок,
/// заголовков и совпадений поиска.
#[derive(Clone)]
struct Shot {
    source: Source,
    alt: String,
    /// Картинка внутри строки — обычно формула: у неё нет ни своей строки,
    /// ни подписи, и роста она с текст, а не с колонку.
    inline: bool,
    slot: Slot,
    /// Чтобы второй клик не начинал вторую загрузку той же картинки.
    busy: Rc<Cell<bool>>,
}

/// Куда приедет картинка.
///
/// Иллюстрация — в рамку-виджет на якоре: у неё своя строка, подпись
/// и клик «открыть в полном размере». Формула — в холст прямо в буфере:
/// виджет посреди строки текста стоит прокрутки, и это замерено
/// (см. шапку `formula.rs`).
#[derive(Clone)]
enum Slot {
    Frame(gtk::Box),
    Canvas(Formula),
}

impl Slot {
    fn frame(&self) -> Option<&gtk::Box> {
        match self {
            Slot::Frame(frame) => Some(frame),
            Slot::Canvas(_) => None,
        }
    }
}

impl State {
    fn find(&mut self, id: u64) -> Option<&mut Tab> {
        self.tabs.iter_mut().find(|tab| tab.id == id)
    }
}

struct Link {
    start: i32,
    end: i32,
    target: String,
}

/// Строка оглавления: что показать и куда это в буфере.
#[derive(Clone)]
struct Mark {
    level: u8,
    title: String,
    offset: i32,
    /// Настоящий заголовок автора или веха, которую поставили мы.
    heading: bool,
}

fn build(app: &Application, start: Vec<String>) {
    let ui = Ui {
        window: ApplicationWindow::builder()
            .application(app)
            .title("Brevier")
            .default_width(1100)
            .default_height(800)
            .build(),
        notebook: gtk::Notebook::builder().scrollable(true).build(),
        entry: gtk::Entry::builder()
            .placeholder_text("address or path to a file")
            .hexpand(true)
            .build(),
        back: gtk::Button::from_icon_name("go-previous-symbolic"),
        forward: gtk::Button::from_icon_name("go-next-symbolic"),
        contents: gtk::ListBox::builder()
            .selection_mode(gtk::SelectionMode::None)
            .build(),
        contents_pane: gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .width_request(TOC_WIDTH)
            .visible(false)
            .build(),
        show_contents: gtk::ToggleButton::builder()
            .icon_name("view-list-symbolic")
            .tooltip_text("Contents")
            .active(true)
            .sensitive(false)
            .build(),
        dark_mode: gtk::ToggleButton::builder()
            .icon_name("weather-clear-night-symbolic")
            .tooltip_text("Dark theme")
            .build(),
        show_images: gtk::ToggleButton::builder()
            .icon_name("image-x-generic-symbolic")
            .tooltip_text("Images")
            .active(true)
            .build(),
        save: gtk::Button::from_icon_name("document-save-symbolic"),
        notice: gtk::Label::builder()
            .xalign(0.0)
            .wrap(true)
            .margin_start(10)
            .margin_end(10)
            .margin_top(4)
            .margin_bottom(4)
            .visible(false)
            .build(),
        search: gtk::SearchBar::builder().build(),
        needle: gtk::SearchEntry::builder()
            .placeholder_text("find on page")
            .hexpand(true)
            .build(),
        tally: gtk::Label::builder().width_chars(10).xalign(1.0).build(),
        paint: gtk::CssProvider::new(),
    };
    ui.contents_pane.set_child(Some(&ui.contents));
    ui.contents_pane.add_css_class("shelf");
    ui.contents.add_css_class("shelf");
    if let Some(display) = gtk::gdk::Display::default() {
        gtk::style_context_add_provider_for_display(
            &display,
            &ui.paint,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }
    ui.notebook.set_hexpand(true);
    ui.notebook.set_vexpand(true);
    ui.back.set_sensitive(false);
    ui.forward.set_sensitive(false);

    let new_tab_button = gtk::Button::from_icon_name("tab-new-symbolic");
    new_tab_button.set_tooltip_text(Some("New tab (Ctrl+T)"));

    let header = gtk::HeaderBar::builder().build();
    header.pack_start(&ui.back);
    header.pack_start(&ui.forward);
    header.pack_start(&new_tab_button);
    header.pack_end(&ui.dark_mode);
    header.pack_end(&ui.show_contents);
    header.pack_end(&ui.show_images);
    header.pack_end(&ui.save);
    header.set_title_widget(Some(&ui.entry));
    ui.window.set_titlebar(Some(&header));

    let find_previous = gtk::Button::from_icon_name("go-up-symbolic");
    let find_next = gtk::Button::from_icon_name("go-down-symbolic");
    find_previous.set_tooltip_text(Some("Previous (Shift+Enter)"));
    find_next.set_tooltip_text(Some("Next (Enter)"));
    let find_row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    find_row.set_margin_start(6);
    find_row.set_margin_end(6);
    find_row.append(&ui.needle);
    find_row.append(&ui.tally);
    find_row.append(&find_previous);
    find_row.append(&find_next);
    ui.search.set_child(Some(&find_row));
    ui.search.set_show_close_button(true);
    ui.search.connect_entry(&ui.needle);

    let body = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    body.append(&ui.notebook);
    body.append(&ui.contents_pane);
    body.set_vexpand(true);

    // Поиск внизу, как в браузерах: строка приходит и уходит, и двигать
    // ради неё текст незачем.
    let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
    root.append(&body);
    root.append(&ui.search);
    root.append(&ui.notice);
    ui.window.set_child(Some(&root));
    ui.notice.add_css_class("caption");
    ui.save.set_tooltip_text(Some("Save the article (Ctrl+S)"));

    let state = Rc::new(RefCell::new(State {
        tabs: Vec::new(),
        next_id: 0,
        // Светлая по умолчанию: бумага белая, и читатель, которому нужно иначе,
        // жмёт кнопку.
        dark: false,
        images: true,
        search: Search::default(),
    }));
    apply_theme(&ui, &state);

    // ── сцепка виджетов с действиями
    {
        let ui = ui.clone();
        let state = state.clone();
        ui.entry.clone().connect_activate(move |entry| {
            let text = entry.text().to_string();
            match address::parse(&text) {
                Ok(address) => open_current(&ui, &state, address, true),
                Err(error) => {
                    let problem = describe(&error);
                    if let Some(tab) = current(&ui, &state) {
                        show_message(&tab, problem.headline, &problem.detail, None);
                    }
                }
            }
        });
    }
    {
        let ui = ui.clone();
        let state = state.clone();
        ui.back
            .clone()
            .connect_clicked(move |_| step(&ui, &state, true));
    }
    {
        let ui = ui.clone();
        let state = state.clone();
        ui.forward
            .clone()
            .connect_clicked(move |_| step(&ui, &state, false));
    }
    {
        let ui = ui.clone();
        let state = state.clone();
        new_tab_button.connect_clicked(move |_| {
            new_tab(&ui, &state, None);
        });
    }
    {
        let ui = ui.clone();
        let state = state.clone();
        ui.dark_mode.clone().connect_toggled(move |button| {
            state.borrow_mut().dark = button.is_active();
            apply_theme(&ui, &state);
        });
    }
    {
        let ui = ui.clone();
        let state = state.clone();
        ui.save
            .clone()
            .connect_clicked(move |_| ask_where_to_save(&ui, &state));
    }
    {
        let ui = ui.clone();
        let state = state.clone();
        ui.show_images.clone().connect_toggled(move |button| {
            state.borrow_mut().images = button.is_active();
            if button.is_active() {
                show_all_shots(&ui, &state);
            }
        });
    }

    // ── поиск по странице
    {
        let ui = ui.clone();
        let state = state.clone();
        ui.needle.clone().connect_search_changed(move |entry| {
            find(&ui, &state, &entry.text(), true);
        });
    }
    {
        let ui = ui.clone();
        let state = state.clone();
        ui.needle
            .clone()
            .connect_activate(move |_| step_hit(&ui, &state, true));
    }
    {
        let ui = ui.clone();
        let state = state.clone();
        find_next.connect_clicked(move |_| step_hit(&ui, &state, true));
    }
    {
        let ui = ui.clone();
        let state = state.clone();
        find_previous.connect_clicked(move |_| step_hit(&ui, &state, false));
    }
    {
        // Shift+Enter — назад. Отдельным контроллером: у `SearchEntry`
        // сигнал `activate` про модификаторы ничего не знает.
        let needle = ui.needle.clone();
        let ui = ui.clone();
        let state = state.clone();
        let keys = gtk::EventControllerKey::new();
        keys.connect_key_pressed(move |_, key, _, modifiers| {
            let shift = modifiers.contains(gtk::gdk::ModifierType::SHIFT_MASK);
            if shift && matches!(key, gtk::gdk::Key::Return | gtk::gdk::Key::KP_Enter) {
                step_hit(&ui, &state, false);
                return glib::Propagation::Stop;
            }
            glib::Propagation::Proceed
        });
        needle.add_controller(keys);
    }
    {
        // Строку закрыли — подсветку убираем: жёлтые пятна в тексте
        // после поиска читать мешают.
        let ui = ui.clone();
        let state = state.clone();
        ui.search
            .clone()
            .connect_search_mode_enabled_notify(move |bar| {
                if bar.is_search_mode() {
                    find(&ui, &state, &ui.needle.text(), true);
                } else {
                    find(&ui, &state, "", true);
                    if let Some(view) = current(&ui, &state) {
                        view.grab_focus();
                    }
                }
            });
    }
    {
        let pane = ui.contents_pane.clone();
        ui.show_contents.clone().connect_toggled(move |button| {
            pane.set_visible(button.is_active() && button.is_sensitive());
        });
    }
    {
        // Сигнал приходит посреди работы самого `Notebook`: страница ещё
        // добавляется или удаляется, у нового ребёнка раскладки нет. Трогать
        // виджеты в этот момент — способ получить от GTK жалобу на снимок
        // виджета без раскладки, поэтому приводим окно в порядок следующим
        // холостым ходом, когда перестройка закончится.
        let ui = ui.clone();
        let state = state.clone();
        ui.notebook.clone().connect_switch_page(move |_, _, _| {
            let ui = ui.clone();
            let state = state.clone();
            glib::idle_add_local_once(move || sync(&ui, &state, None));
        });
    }

    keyboard(&ui, &state, app);

    // ── стартовые вкладки: по адресу на каждый аргумент
    let addresses: Vec<Address> = start
        .iter()
        .filter_map(|text| address::parse(text).ok())
        .collect();
    if addresses.is_empty() {
        new_tab(&ui, &state, None);
    } else {
        for address in addresses {
            new_tab(&ui, &state, Some(address));
        }
        // Открываем первую: читатель просил их в этом порядке, а не наоборот.
        ui.notebook.set_current_page(Some(0));
    }

    ui.window.present();
}

/// Клавиши, которые GTK сам не разбирает.
fn keyboard(ui: &Ui, state: &Rc<RefCell<State>>, app: &Application) {
    let add = |name: &str, keys: &[&str], action: gio::SimpleAction| {
        app.add_action(&action);
        app.set_accels_for_action(&format!("app.{name}"), keys);
    };

    let new = gio::SimpleAction::new("new-tab", None);
    {
        let ui = ui.clone();
        let state = state.clone();
        new.connect_activate(move |_, _| {
            new_tab(&ui, &state, None);
        });
    }
    add("new-tab", &["<Control>t"], new);

    let close = gio::SimpleAction::new("close-tab", None);
    {
        let ui = ui.clone();
        let state = state.clone();
        close.connect_activate(move |_, _| {
            if let Some(index) = ui.notebook.current_page() {
                close_tab(&ui, &state, index as usize);
            }
        });
    }
    add("close-tab", &["<Control>w"], close);

    let focus = gio::SimpleAction::new("focus-address", None);
    {
        let ui = ui.clone();
        focus.connect_activate(move |_, _| {
            ui.entry.grab_focus();
        });
    }
    add("focus-address", &["<Control>l"], focus);

    let browser = gio::SimpleAction::new("open-in-browser", None);
    {
        let ui = ui.clone();
        let state = state.clone();
        browser.connect_activate(move |_, _| {
            let target = current_address(&ui, &state);
            if let Some(target) = target {
                open_in_system_browser(&target);
            }
        });
    }
    add("open-in-browser", &["<Control>o"], browser);

    let find_action = gio::SimpleAction::new("find", None);
    {
        let ui = ui.clone();
        find_action.connect_activate(move |_, _| {
            ui.search.set_search_mode(true);
            ui.needle.grab_focus();
            ui.needle.select_region(0, -1);
        });
    }
    add("find", &["<Control>f"], find_action);

    let save = gio::SimpleAction::new("save", None);
    {
        let ui = ui.clone();
        let state = state.clone();
        save.connect_activate(move |_, _| ask_where_to_save(&ui, &state));
    }
    add("save", &["<Control>s"], save);
}

/// Открыть новую вкладку и, если дали адрес, сразу читать.
fn new_tab(ui: &Ui, state: &Rc<RefCell<State>>, address: Option<Address>) {
    // Виджет статьи — свой: `GtkTextView`, который дорисовывает линейку
    // слева от цитаты. Настраиваем его уже как `TextView`, чтобы не спорить
    // с одноимёнными методами других интерфейсов GTK.
    let article = Article::new();
    article.set_rule_color(rule_color(state.borrow().dark));
    let view: gtk::TextView = article.upcast();
    view.set_editable(false);
    view.set_cursor_visible(false);
    view.set_wrap_mode(gtk::WrapMode::Word);
    view.set_halign(gtk::Align::Center);
    view.set_top_margin(28);
    view.set_bottom_margin(80);
    tags(&view.buffer(), state.borrow().dark);
    view.set_width_request(measure_px());

    let scroller = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .hexpand(true)
        .vexpand(true)
        .child(&view)
        .build();

    // Страница — одного цвета целиком: и колонка текста, и поля вокруг неё.
    // Без этого поля красит тема окна, и получаются три полосы разного тона.
    view.add_css_class("page");
    scroller.add_css_class("page");

    let label = gtk::Label::builder()
        .label("New tab")
        .ellipsize(pango::EllipsizeMode::End)
        .width_chars(TAB_LABEL as i32)
        .max_width_chars(TAB_LABEL as i32)
        .build();
    let close = gtk::Button::builder()
        .icon_name("window-close-symbolic")
        .has_frame(false)
        .build();
    let corner = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    corner.append(&label);
    corner.append(&close);

    let id = {
        let mut state = state.borrow_mut();
        let id = state.next_id;
        state.next_id += 1;
        state.tabs.push(Tab {
            id,
            view: view.clone(),
            label: label.clone(),
            history: History::new(),
            links: Vec::new(),
            marks: Vec::new(),
            anchors: Vec::new(),
            shots: Vec::new(),
            document: None,
            generation: 0,
        });
        id
    };

    let index = ui.notebook.append_page(&scroller, Some(&corner));
    ui.notebook.set_tab_reorderable(&scroller, true);
    ui.notebook.set_show_tabs(ui.notebook.n_pages() > 1);
    ui.notebook.set_current_page(Some(index));

    {
        let ui = ui.clone();
        let state = state.clone();
        close.connect_clicked(move |_| {
            let index = state.borrow().tabs.iter().position(|tab| tab.id == id);
            if let Some(index) = index {
                close_tab(&ui, &state, index);
            }
        });
    }

    // ── клик по ссылке
    // Ноль значит «все кнопки»: по умолчанию жест слушает только левую,
    // и средняя до ссылки не доходила.
    let click = gtk::GestureClick::builder().button(0).build();
    {
        let ui = ui.clone();
        let state = state.clone();
        let view = view.clone();
        click.connect_released(move |gesture, _, x, y| {
            // Как в браузерах: Ctrl и средняя кнопка открывают вкладкой,
            // обычный клик уводит на страницу.
            let ctrl = gesture
                .current_event_state()
                .contains(gtk::gdk::ModifierType::CONTROL_MASK);
            let middle = gesture.current_button() == gtk::gdk::BUTTON_MIDDLE;
            let plain = !ctrl && !middle;

            let target = {
                let mut borrowed = state.borrow_mut();
                let Some(tab) = borrowed.find(id) else { return };
                let Some(target) = link_at(&view, &tab.links, x, y).map(|link| link.target.clone())
                else {
                    return;
                };
                // Ссылка внутрь этой же страницы — не загрузка, а прыжок
                // по буферу: место заголовка мы знаем точно.
                let here = tab.history.current().map(Address::display);
                if plain
                    && let Some(fragment) = fragment_of(&target, here.as_deref())
                    && jump(&view, &tab.anchors, &fragment)
                {
                    return;
                }
                target
            };
            let Ok(address) = address::parse(&target) else {
                return;
            };
            if ctrl || middle {
                new_tab(&ui, &state, Some(address));
            } else if gesture.current_button() == gtk::gdk::BUTTON_PRIMARY {
                open(&ui, &state, id, address, true);
            }
        });
    }
    view.add_controller(click);

    // Курсор над ссылкой — палец, как в любом браузере. `GtkTextView` ставит
    // себе курсор-текст сам, поэтому возвращаем его руками, когда ссылка
    // кончилась.
    let hover = gtk::EventControllerMotion::new();
    {
        let state = state.clone();
        let view = view.clone();
        // Движение мыши приходит на каждый пиксель, а смена курсора — работа
        // для сервера: трогаем, только когда ссылка появилась или кончилась.
        let was_over = Cell::new(false);
        hover.connect_motion(move |_, x, y| {
            let over = {
                let mut borrowed = state.borrow_mut();
                let Some(tab) = borrowed.find(id) else { return };
                link_at(&view, &tab.links, x, y).is_some()
            };
            if over != was_over.replace(over) {
                view.set_cursor_from_name(Some(if over { "pointer" } else { "text" }));
            }
        });
    }
    view.add_controller(hover);

    // Клавиши прокрутки висят на тексте, а не на окне: иначе пробел
    // и стрелки ломали бы набор адреса в строке.
    let keys = gtk::EventControllerKey::new();
    {
        let scroller = scroller.clone();
        keys.connect_key_pressed(move |_, key, _, modifiers| {
            if modifiers.contains(gtk::gdk::ModifierType::CONTROL_MASK) {
                return glib::Propagation::Proceed;
            }
            let adjustment = scroller.vadjustment();
            let page = adjustment.page_size();
            let step = page / 10.0;
            let to = match key {
                gtk::gdk::Key::space | gtk::gdk::Key::Page_Down => adjustment.value() + page * 0.9,
                gtk::gdk::Key::BackSpace | gtk::gdk::Key::Page_Up => {
                    adjustment.value() - page * 0.9
                }
                gtk::gdk::Key::Down => adjustment.value() + step,
                gtk::gdk::Key::Up => adjustment.value() - step,
                gtk::gdk::Key::Home => adjustment.lower(),
                gtk::gdk::Key::End => adjustment.upper(),
                _ => return glib::Propagation::Proceed,
            };
            let highest = (adjustment.upper() - page).max(adjustment.lower());
            adjustment.set_value(to.clamp(adjustment.lower(), highest));
            glib::Propagation::Stop
        });
    }
    view.add_controller(keys);

    if let Some(address) = address {
        open(ui, state, id, address, true);
    } else {
        // Пустая вкладка — не пустой экран: читатель должен узнать,
        // куда попал. `show_intro` сам зовёт `sync`.
        show_intro(ui, state, id, &view);
        ui.entry.grab_focus();
    }
}

fn close_tab(ui: &Ui, state: &Rc<RefCell<State>>, index: usize) {
    if index >= state.borrow().tabs.len() {
        return;
    }
    // Уходя, бросаем загрузку: слушать её больше некому.
    state.borrow_mut().tabs.remove(index).generation = u64::MAX;
    ui.notebook.remove_page(Some(index as u32));

    // Окно без вкладок показывать нечем — заводим чистую.
    if state.borrow().tabs.is_empty() {
        new_tab(ui, state, None);
        return;
    }
    ui.notebook.set_show_tabs(ui.notebook.n_pages() > 1);
    sync(ui, state, None);
}

fn current(ui: &Ui, state: &Rc<RefCell<State>>) -> Option<gtk::TextView> {
    let index = ui.notebook.current_page()? as usize;
    state.borrow().tabs.get(index).map(|tab| tab.view.clone())
}

fn current_address(ui: &Ui, state: &Rc<RefCell<State>>) -> Option<String> {
    let index = ui.notebook.current_page()? as usize;
    state
        .borrow()
        .tabs
        .get(index)?
        .history
        .current()
        .map(Address::display)
}

fn open_current(ui: &Ui, state: &Rc<RefCell<State>>, address: Address, remember: bool) {
    let Some(index) = ui.notebook.current_page() else {
        return;
    };
    let id = match state.borrow().tabs.get(index as usize) {
        Some(tab) => tab.id,
        None => return,
    };
    open(ui, state, id, address, remember);
}

/// Шаг по истории текущей вкладки.
fn step(ui: &Ui, state: &Rc<RefCell<State>>, backwards: bool) {
    let Some(index) = ui.notebook.current_page() else {
        return;
    };
    let step = {
        let mut state = state.borrow_mut();
        let Some(tab) = state.tabs.get_mut(index as usize) else {
            return;
        };
        let address = if backwards {
            tab.history.back().cloned()
        } else {
            tab.history.forward().cloned()
        };
        address.map(|address| (tab.id, address))
    };
    if let Some((id, address)) = step {
        open(ui, state, id, address, false);
    }
}

/// Открыть адрес в названной вкладке.
fn open(ui: &Ui, state: &Rc<RefCell<State>>, id: u64, address: Address, remember: bool) {
    // Решётку в адресе запоминаем здесь: серверу её не отправляют, и в адресе
    // загруженного документа её уже не будет.
    let anchor = anchor_of(&address);
    let generation = {
        let mut state = state.borrow_mut();
        let Some(tab) = state.find(id) else { return };
        if remember {
            tab.history.visit(address.clone());
        }
        tab.generation += 1;
        tab.label.set_text(&clip("Loading…", TAB_LABEL));
        tab.generation
    };
    sync(ui, state, None);

    if let Some(view) = view_of(state, id) {
        show_message(&view, "Loading…", "", None);
    }

    let ui = ui.clone();
    let state = state.clone();
    // Чем открыть это в чужом браузере — считаем до того, как адрес уедет
    // в поток загрузки: на отказе он понадобится, а его уже не будет.
    let external = address.external();
    glib::spawn_future_local(async move {
        let loaded = gio::spawn_blocking(move || brevier::open(&address, UserAgent::Honest)).await;

        // Читатель уже ушёл на другую страницу — ответ никому не нужен.
        if state.borrow_mut().find(id).map(|tab| tab.generation) != Some(generation) {
            return;
        }
        let Some(view) = view_of(&state, id) else {
            return;
        };

        match loaded {
            Ok(Ok(document)) => {
                let page = render(&view, &document, anchor.as_deref());
                view.grab_focus();
                let mut borrowed = state.borrow_mut();
                if let Some(tab) = borrowed.find(id) {
                    tab.label.set_text(&clip(&document.title, TAB_LABEL));
                    tab.label.set_tooltip_text(Some(&document.title));
                    tab.links = page.links;
                    tab.marks = page.marks;
                    tab.anchors = page.anchors;
                    tab.shots = page.shots.clone();
                    tab.document = Some(document.clone());
                }
                drop(borrowed);
                sync(&ui, &state, None);
                // Заглушки оживляем после того, как вкладка узнала про них:
                // клик по заглушке ищет вкладку по номеру.
                let eager = state.borrow().images;
                for shot in &page.shots {
                    place_shot(&ui, &state, id, shot, None);
                    // Именно в эту вкладку, а не в открытую: пока страница
                    // грузилась, читатель мог уйти смотреть другую.
                    if eager {
                        load_shot(&ui, &state, id, shot);
                    }
                }
                for cell in &page.cells {
                    follow_cell_links(&ui, &state, id, cell);
                }
            }
            Ok(Err(error)) => {
                let problem = describe(&error);
                show_message(
                    &view,
                    problem.headline,
                    &problem.detail,
                    problem.offer_browser.then(|| external.clone()),
                );
                let mut borrowed = state.borrow_mut();
                if let Some(tab) = borrowed.find(id) {
                    tab.label.set_text(&clip(problem.headline, TAB_LABEL));
                    tab.links.clear();
                    tab.marks.clear();
                    tab.anchors.clear();
                    tab.shots.clear();
                    tab.document = None;
                }
                drop(borrowed);
                sync(&ui, &state, None);
            }
            Err(_) => show_message(&view, "The load fell through", "", None),
        }
    });
}

fn view_of(state: &Rc<RefCell<State>>, id: u64) -> Option<gtk::TextView> {
    state
        .borrow()
        .tabs
        .iter()
        .find(|tab| tab.id == id)
        .map(|tab| tab.view.clone())
}

/// Привести окно в соответствие с открытой вкладкой.
fn sync(ui: &Ui, state: &Rc<RefCell<State>>, index: Option<usize>) {
    let index = index.or_else(|| ui.notebook.current_page().map(|page| page as usize));
    let Some(index) = index else { return };

    let (address, can_back, can_forward, marks, title, view) = {
        let borrowed = state.borrow();
        let Some(tab) = borrowed.tabs.get(index) else {
            return;
        };
        (
            tab.history
                .current()
                .map(Address::display)
                .unwrap_or_default(),
            tab.history.can_go_back(),
            tab.history.can_go_forward(),
            tab.marks.clone(),
            tab.label.text().to_string(),
            tab.view.clone(),
        )
    };

    ui.entry.set_text(&address);
    ui.back.set_sensitive(can_back);
    ui.forward.set_sensitive(can_forward);
    ui.window.set_title(Some(&if address.is_empty() {
        "Brevier".to_owned()
    } else {
        format!("{title} — Brevier")
    }));

    fill_contents(&ui.contents, &marks, &view);
    ui.show_contents.set_sensitive(!marks.is_empty());
    ui.contents_pane
        .set_visible(ui.show_contents.is_active() && !marks.is_empty());

    // Поиск открыт — ищем в том, что теперь на экране: подсветка и счётчик
    // принадлежат странице, а не строке ввода.
    if ui.search.is_search_mode() {
        find(ui, state, &ui.needle.text(), true);
    }
}

/// Тема. Кроме настройки GTK перекрашиваем страницу и свои теги: цвет бумаги,
/// ссылки и приглушённого текста — часть типографики, а не оформления окна.
fn apply_theme(ui: &Ui, state: &Rc<RefCell<State>>) {
    let dark = state.borrow().dark;
    if let Some(settings) = gtk::Settings::default() {
        settings.set_gtk_application_prefer_dark_theme(dark);
    }
    ui.paint.load_from_data(&page_css(dark));
    for tab in &state.borrow().tabs {
        recolor(&tab.view.buffer(), dark);
        if let Some(article) = tab.view.downcast_ref::<Article>() {
            article.set_rule_color(rule_color(dark));
        }
    }
}

/// Цвет линейки цитаты: приглушённая краска вполсилы. Линейка отмечает
/// чужую речь, а не спорит с ней, поэтому берёт не цвет текста и не цвет
/// линеек таблицы, а середину между ними.
/// Краска страницы. Нужна холсту формулы: он рисует исходник сам,
/// в обход тегов буфера, и цвет ему надо дать явно.
fn ink_color(dark: bool) -> gtk::gdk::RGBA {
    if dark { INK_DARK } else { INK_LIGHT }
        .parse::<gtk::gdk::RGBA>()
        .unwrap_or_else(|_| gtk::gdk::RGBA::new(0.1, 0.1, 0.1, 1.0))
}

fn rule_color(dark: bool) -> gtk::gdk::RGBA {
    let mut color = colors(dark)
        .dim
        .parse::<gtk::gdk::RGBA>()
        .unwrap_or_else(|_| gtk::gdk::RGBA::new(0.6, 0.6, 0.6, 1.0));
    color.set_alpha(0.5);
    color
}

/// Цвета страницы одной таблицей.
///
/// Красим и текст, и то, что вокруг него: колонка узкая, поля по бокам широкие,
/// и если их красит тема окна, страница получается из трёх полос разного тона.
/// Оглавлению отличаться можно — оно рядом со страницей, а не на ней.
fn page_css(dark: bool) -> String {
    let (paper, ink, shelf) = if dark {
        (PAPER_DARK, INK_DARK, SHELF_DARK)
    } else {
        (PAPER_LIGHT, INK_LIGHT, SHELF_LIGHT)
    };
    let colors = colors(dark);
    let (dim, rule) = (colors.dim, colors.rule);

    format!(
        // Шапка и окно — в тот же тёплый ряд, что и бумага. Иначе слоновая
        // кость соседствует с холодно-белой панелью GTK, и окно выглядит
        // склеенным из двух разных.
        "window, headerbar {{ background-color: {shelf}; }}\n\
         .page, .page text {{ background-color: {paper}; color: {ink}; }}\n\
         .shelf, .shelf > viewport, .shelf list, .shelf row {{ background-color: {shelf}; }}\n\
         .shot {{ border: 1px dashed {dim}; border-radius: 6px; padding: 20px 14px; \
                  color: {dim}; margin: 6px 0; }}\n\
         .caption {{ color: {dim}; font-size: 0.85em; margin-bottom: 6px; }}\n\
         .formula {{ padding: 0 2px; min-height: 0; min-width: 0; color: {dim}; }}\n\
         .table {{ margin: 10px 0 14px 0; }}\n\
         .table separator {{ background-color: {rule}; min-height: 1px; }}\n\
         .th {{ font-weight: 500; }}\n"
    )
}

fn recolor(buffer: &gtk::TextBuffer, dark: bool) {
    let table = buffer.tag_table();
    let colors = colors(dark);
    let paint = |name: &str, property: &str, value: &str| {
        if let Some(tag) = table.lookup(name) {
            tag.set_property(property, value);
        }
    };

    paint("link", "foreground", colors.link);
    paint("dim", "foreground", colors.dim);
    paint("code", "background", colors.panel);
    paint("codeblock", "paragraph-background", colors.panel);
    paint("pad", "paragraph-background", colors.panel);
    paint("kw", "foreground", colors.keyword);
    paint("lit", "foreground", colors.literal);
    paint("num", "foreground", colors.number);
    paint("com", "foreground", colors.comment);
}

/// Краски, зависящие от темы. Всё, что не бумага и не краска текста:
/// ссылка, приглушённое, подложка кода, четыре цвета подсветки и линейка
/// таблицы. Собраны в одном месте, потому что меняются вместе.
struct Colors {
    link: &'static str,
    dim: &'static str,
    /// Подложка блока кода и кода в строке.
    panel: &'static str,
    keyword: &'static str,
    literal: &'static str,
    number: &'static str,
    comment: &'static str,
    /// Линейки таблицы.
    rule: &'static str,
}

fn colors(dark: bool) -> Colors {
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
        }
    }
}

/// `#rrggbb` в три байта. Цвета записаны так, как их читают глазами,
/// а декодеру картинок нужны числа.
fn rgb(hex: &str) -> [u8; 3] {
    let hex = hex.trim_start_matches('#');
    if hex.len() < 6 {
        return [255, 255, 255];
    }
    let byte = |at: usize| u8::from_str_radix(&hex[at..at + 2], 16).unwrap_or(255);
    [byte(0), byte(2), byte(4)]
}

/// Мера в пикселях. В GTK кегль задаётся пунктами, а ширина виджета
/// пикселями; без пересчёта по разрешению в строке оказывалось бы разное
/// число знаков на разных экранах.
fn measure_px() -> i32 {
    (f64::from(MEASURE) * dpi() / 72.0).round() as i32
}

/// Кегль текста в пикселях. Нужен не окну, а разбору svg: MathJax печатает
/// формулы в `ex`, и они обязаны быть ростом с текст.
fn text_px() -> f32 {
    (f64::from(TEXT_SIZE) * dpi() / 72.0) as f32
}

fn dpi() -> f64 {
    let dpi = gtk::Settings::for_display(&gtk::gdk::Display::default().unwrap()).gtk_xft_dpi();
    // Настройка хранится в 1024-х долях точки; 0 или -1 значит «не задано».
    if dpi > 0 {
        f64::from(dpi) / 1024.0
    } else {
        96.0
    }
}

/// Отдать адрес системному браузеру. Без внешних крейтов: это три команды,
/// а каждая зависимость в проекте про безопасность стоит дороже трёх строк.
fn open_in_system_browser(target: &str) {
    let (program, args): (&str, &[&str]) = if cfg!(target_os = "macos") {
        ("open", &[])
    } else if cfg!(target_os = "windows") {
        ("cmd", &["/C", "start", ""])
    } else {
        ("xdg-open", &[])
    };
    let _ = std::process::Command::new(program)
        .args(args)
        .arg(target)
        .spawn();
}

/// Гарнитуры из комплекта — в обход системной установки.
///
/// GTK берёт шрифты у fontconfig, а тот знает только про установленные.
/// Класть свои в системные каталоги приложение не вправе, поэтому
/// выкладываем их в свой кэш и подсовываем fontconfig собственный конфиг,
/// который включает системный и добавляет нашу папку. Всё остаётся
/// внутри кэша приложения.
///
/// На Windows и macOS механизм другой (`AddFontResourceEx`,
/// `CTFontManagerRegisterFontsForURL`) — это отдельная работа при упаковке.
fn use_bundled_fonts() {
    const FONTS: [(&str, &[u8]); 7] = [
        (
            "NotoSans-Light.ttf",
            include_bytes!("../../assets/fonts/NotoSans-Light.ttf"),
        ),
        (
            "NotoSans-Regular.ttf",
            include_bytes!("../../assets/fonts/NotoSans-Regular.ttf"),
        ),
        (
            "NotoSans-Italic.ttf",
            include_bytes!("../../assets/fonts/NotoSans-Italic.ttf"),
        ),
        (
            "NotoSans-Medium.ttf",
            include_bytes!("../../assets/fonts/NotoSans-Medium.ttf"),
        ),
        (
            "NotoSans-Bold.ttf",
            include_bytes!("../../assets/fonts/NotoSans-Bold.ttf"),
        ),
        (
            "NotoSans-BoldItalic.ttf",
            include_bytes!("../../assets/fonts/NotoSans-BoldItalic.ttf"),
        ),
        (
            "NotoSansMono-Regular.ttf",
            include_bytes!("../../assets/fonts/NotoSansMono-Regular.ttf"),
        ),
    ];

    let home = glib::user_cache_dir().join("brevier");
    let fonts = home.join("fonts");
    if std::fs::create_dir_all(&fonts).is_err() {
        return;
    }
    for (name, bytes) in FONTS {
        let path = fonts.join(name);
        let stale = std::fs::metadata(&path).map(|meta| meta.len() as usize != bytes.len());
        if stale.unwrap_or(true) && std::fs::write(&path, bytes).is_err() {
            return;
        }
    }

    let config = home.join("fonts.conf");
    let text = format!(
        "<?xml version=\"1.0\"?>\n\
         <!DOCTYPE fontconfig SYSTEM \"fonts.dtd\">\n\
         <fontconfig>\n\
         \x20 <include ignore_missing=\"yes\">/etc/fonts/fonts.conf</include>\n\
         \x20 <dir>{}</dir>\n\
         </fontconfig>\n",
        fonts.display()
    );
    if std::fs::write(&config, text).is_err() {
        return;
    }

    // Безопасно: это самое начало `main`, потоков ещё нет, и fontconfig
    // читает переменную позже — при первой отрисовке текста.
    unsafe {
        std::env::set_var("FONTCONFIG_FILE", &config);
    }
}

/// Ссылка под точкой окна, если она там есть.
fn link_at<'a>(view: &gtk::TextView, links: &'a [Link], x: f64, y: f64) -> Option<&'a Link> {
    let (bx, by) = view.window_to_buffer_coords(gtk::TextWindowType::Widget, x as i32, y as i32);
    let (iter, _) = view.iter_at_position(bx, by)?;
    let offset = iter.offset();
    links
        .iter()
        .find(|link| offset >= link.start && offset < link.end)
}

fn show_message(view: &gtk::TextView, headline: &str, detail: &str, offer: Option<String>) {
    let buffer = view.buffer();
    buffer.set_text("");
    let mut end = buffer.end_iter();
    buffer.insert_with_tags_by_name(&mut end, headline, &["h2"]);
    if !detail.is_empty() {
        buffer.insert(&mut end, "\n\n");
        buffer.insert(&mut end, detail);
    }

    // Кнопка, а не только Ctrl+O: на странице, где ничего не показалось,
    // читателю нужен выход, а не память о сочетании клавиш. Что предлагать
    // её, решает ядро (`Failure::offer_browser`) — 403 не лечится ничем,
    // а пустое извлечение лечится именно этим.
    let Some(target) = offer else { return };
    buffer.insert(&mut end, "\n\n");
    let anchor = buffer.create_child_anchor(&mut end);
    let button = gtk::Button::builder()
        .label("Open in your browser")
        .halign(gtk::Align::Start)
        .build();
    button.connect_clicked(move |_| open_in_system_browser(&target));
    view.add_child_at_anchor(&button, &anchor);
}

/// Начальная страница. Рисуется тем же трактом, что и статья: текст в ядре,
/// разметка markdown, рендерер общий. Историю и адресную строку не трогает —
/// это не открытая страница, а пустая вкладка, которой есть что сказать.
fn show_intro(ui: &Ui, state: &Rc<RefCell<State>>, id: u64, view: &gtk::TextView) {
    let document = Document {
        address: Address::Web(String::new()),
        title: brevier::intro::TITLE.to_owned(),
        markdown: brevier::intro::MARKDOWN.to_owned(),
    };
    let page = render(view, &document, None);
    let mut borrowed = state.borrow_mut();
    if let Some(tab) = borrowed.find(id) {
        tab.links = page.links;
        tab.marks = page.marks;
        tab.anchors = page.anchors;
    }
    drop(borrowed);
    sync(ui, state, None);
}
/// Теги — вся типографика статьи. Кегли и интерлиньяж те же, что были
/// в прошлом интерфейсе: они живут в ядре и от тулкита не зависят.
fn tags(buffer: &gtk::TextBuffer, dark: bool) {
    let extra = ((LINE_HEIGHT - 1.0) * TEXT_SIZE).round() as i32;
    let body = f64::from(TEXT_SIZE);

    buffer.create_tag(
        Some("body"),
        &[
            ("family", &BODY_FAMILY),
            ("size-points", &body),
            ("pixels-inside-wrap", &extra),
            ("pixels-below-lines", &(extra * 2)),
        ],
    );

    for (index, scale) in HEADINGS.iter().enumerate() {
        let name = format!("h{}", index + 1);
        buffer.create_tag(
            Some(&name),
            &[
                ("family", &BODY_FAMILY),
                ("size-points", &(body * f64::from(*scale))),
                ("weight", &HEADING_WEIGHTS[index]),
                // Воздух сверху, а не снизу: заголовок принадлежит тому,
                // что под ним.
                ("pixels-above-lines", &(extra * 3)),
                ("pixels-below-lines", &extra),
            ],
        );
    }

    buffer.create_tag(Some("em"), &[("style", &pango::Style::Italic)]);
    buffer.create_tag(Some("strong"), &[("weight", &BOLD)]);
    let colors = colors(dark);

    // Код в строке отличается не только гарнитурой: подложка отделяет его
    // от текста там, где моноширинного мало — в одном-двух знаках.
    buffer.create_tag(
        Some("code"),
        &[
            ("family", &MONO_FAMILY),
            ("size-points", &(body * 0.9)),
            ("background", &colors.panel),
        ],
    );
    // Блок кода — панель: подложка во всю меру, поля по краям, строки плотнее,
    // чем в тексте. `paragraph-background` красит строку целиком, поэтому
    // панель получается без единого виджета.
    buffer.create_tag(
        Some("codeblock"),
        &[
            ("family", &MONO_FAMILY),
            ("size-points", &(body * 0.9)),
            // Висячий отступ наоборот: продолжение длинной строки уходит
            // правее её начала, и перенос видно. Строку кода не перенести
            // нельзя — колонок в буфере нет.
            ("left-margin", &40),
            ("indent", &-18),
            ("right-margin", &22),
            ("pixels-below-lines", &2),
            ("paragraph-background", &colors.panel),
        ],
    );
    // Пустая строка с той же подложкой — это поля панели сверху и снизу.
    buffer.create_tag(
        Some("pad"),
        &[
            ("size-points", &(body * 0.4)),
            ("paragraph-background", &colors.panel),
            ("pixels-above-lines", &extra),
            ("pixels-below-lines", &extra),
        ],
    );

    buffer.create_tag(Some("kw"), &[("foreground", &colors.keyword)]);
    buffer.create_tag(Some("lit"), &[("foreground", &colors.literal)]);
    buffer.create_tag(Some("num"), &[("foreground", &colors.number)]);
    buffer.create_tag(
        Some("com"),
        &[
            ("foreground", &colors.comment),
            ("style", &pango::Style::Italic),
        ],
    );
    // Поле слева пошире обычного: в нём стоит линейка, которую рисует
    // виджет статьи.
    buffer.create_tag(
        Some("quote"),
        &[("style", &pango::Style::Italic), ("left-margin", &26)],
    );

    // Список: маркер выступает влево, перенос строки встаёт под текст,
    // а не под маркер. Уровни вложенности — свой отступ каждому.
    for level in 1..=LIST_LEVELS {
        buffer.create_tag(
            Some(&format!("list{level}")),
            &[
                ("left-margin", &(26 * level)),
                ("indent", &-18),
                // Пункты стоят плотнее абзацев: список — одна мысль, разбитая
                // на части, а не несколько абзацев подряд.
                ("pixels-below-lines", &(extra / 2)),
            ],
        );
    }
    let (link, dim) = (colors.link, colors.dim);
    buffer.create_tag(
        Some("link"),
        &[
            ("underline", &pango::Underline::Single),
            ("foreground", &link),
        ],
    );
    buffer.create_tag(Some("dim"), &[("foreground", &dim)]);

    // Подсветка поиска. Заводится последней: у тегов, наложенных позже,
    // приоритет выше, и жёлтое ложится поверх цвета ссылки.
    buffer.create_tag(
        Some("found"),
        &[("background", &FOUND), ("foreground", &FOUND_INK)],
    );
    buffer.create_tag(
        Some("here"),
        &[("background", &FOUND_HERE), ("foreground", &FOUND_INK)],
    );
}

/// Разложить статью по буферу. Возвращает ссылки с их местами в тексте —
/// по ним потом опознаётся клик.
struct Page {
    links: Vec<Link>,
    marks: Vec<Mark>,
    anchors: Vec<(String, i32)>,
    shots: Vec<Shot>,
    cells: Vec<gtk::Label>,
}

fn render(view: &gtk::TextView, document: &Document, target: Option<&str>) -> Page {
    use comrak::{Arena, Options, parse_document};

    let buffer = view.buffer();
    buffer.set_text("");

    let mut options = Options::default();
    options.extension.table = true;
    options.extension.strikethrough = true;
    options.extension.autolink = true;

    let arena = Arena::new();
    let root = parse_document(&arena, &document.markdown, &options);

    let mut links = Vec::new();
    let mut marks = Vec::new();
    let mut anchors = Vec::new();
    let mut shots = Vec::new();
    let mut cells = Vec::new();
    let mut writer = Writer {
        buffer: &buffer,
        view,
        base: &document.address,
        links: &mut links,
        marks: &mut marks,
        anchors: &mut anchors,
        shots: &mut shots,
        cells: &mut cells,
        depth: 0,
    };

    for node in root.children() {
        writer.block(node, &[]);
    }
    let marks = contents_of(marks, buffer.char_count());

    // Новая статья начинается сначала — или с того места, на которое указывала
    // решётка в адресе. Через `idle`, потому что в момент вставки текста
    // у виджета ещё нет раскладки и прокручивать ему некуда.
    buffer.place_cursor(&buffer.start_iter());
    let target = target.map(anchor);
    let places = anchors.clone();
    let view = view.clone();
    glib::idle_add_local_once(move || {
        let offset = target
            .and_then(|want| places.iter().find(|(name, _)| *name == want))
            .map(|(_, offset)| *offset);
        match offset {
            Some(offset) => settle(&view, offset, ANCHOR_ALIGN),
            None => scroll_to(&view, 0, 0.0),
        }
    });
    Page {
        links,
        marks,
        anchors,
        shots,
        cells,
    }
}

/// Прокрутить к месту в буфере.
///
/// Через метку, а не через итератор: сразу после отрисовки раскладки ещё нет,
/// и `scroll_to_iter` промахивается — он меряет по тому, что успело
/// разложиться. Прокрутка к метке умеет дождаться раскладки. Метка одна
/// на буфер и переезжает с места на место: плодить их незачем.
fn scroll_to(view: &gtk::TextView, offset: i32, align: f64) {
    let buffer = view.buffer();
    let place = buffer.iter_at_offset(offset);
    let mark = match buffer.mark(JUMP) {
        Some(mark) => {
            buffer.move_mark(&mark, &place);
            mark
        }
        None => buffer.create_mark(Some(JUMP), &place, true),
    };
    view.scroll_to_mark(&mark, 0.0, true, 0.0, align);
}

/// Прокрутка, которая доводит дело до конца.
///
/// В тексте живут виджеты — картинки и таблицы, — и свой размер они получают
/// не сразу: раскладка буфера готова, а высота документа ещё растёт. Одного
/// прыжка поэтому мало, он оказывается выше цели. Повторяем несколько кадров,
/// пока высота не перестанет меняться.
fn settle(view: &gtk::TextView, offset: i32, align: f64) {
    scroll_to(view, offset, align);

    let left = Cell::new(SETTLE_FRAMES);
    let was = Cell::new(-1.0);
    let stable = Cell::new(0u8);
    view.add_tick_callback(move |view, _| {
        // Прыгаем каждый кадр, а не только когда высота изменилась: пока
        // раскладка неполная, `GtkTextView` честно уезжает к нижнему краю
        // документа, и одного удачного прыжка мало — нужен последний,
        // когда высота уже настоящая.
        scroll_to(view, offset, align);

        let height = view
            .vadjustment()
            .map(|bar| bar.upper())
            .unwrap_or_default();
        if (height - was.get()).abs() > 0.5 {
            was.set(height);
            stable.set(0);
        } else {
            stable.set(stable.get() + 1);
        }

        let left_now = left.get().saturating_sub(1);
        left.set(left_now);
        // Кончаем, когда высота устоялась несколько кадров подряд, — или
        // по исчерпании терпения: держать окно на поводке дольше нельзя,
        // читатель уже мог прокрутить страницу сам.
        if left_now == 0 || (height > 0.0 && stable.get() >= 5) {
            glib::ControlFlow::Break
        } else {
            glib::ControlFlow::Continue
        }
    });
}

/// Ссылка внутрь открытой страницы: `#anchor` или полный адрес с решёткой,
/// совпадающий с тем, что уже открыто.
fn fragment_of(target: &str, here: Option<&str>) -> Option<String> {
    if let Some(fragment) = target.strip_prefix('#') {
        return (!fragment.is_empty()).then(|| fragment.to_owned());
    }
    let (page, fragment) = target.split_once('#')?;
    let here = here?;
    let here = here.split('#').next().unwrap_or(here);
    (!fragment.is_empty() && page.trim_end_matches('/') == here.trim_end_matches('/'))
        .then(|| fragment.to_owned())
}

/// Прыгнуть к якорю. `false` значит «такого заголовка на странице нет» —
/// тогда ссылка отрабатывает как обычная.
fn jump(view: &gtk::TextView, anchors: &[(String, i32)], fragment: &str) -> bool {
    let want = anchor(fragment);
    let Some((_, offset)) = anchors.iter().find(|(name, _)| *name == want) else {
        return false;
    };
    settle(view, *offset, ANCHOR_ALIGN);
    true
}

/// Решётка в адресе, если она там есть.
fn anchor_of(address: &Address) -> Option<String> {
    match address {
        Address::Web(url) => url
            .split_once('#')
            .map(|(_, fragment)| fragment.to_owned())
            .filter(|fragment| !fragment.is_empty()),
        _ => None,
    }
}

/// Оглавление из того, что встретилось при отрисовке.
///
/// Заголовки берём как есть — их место в буфере известно точно, поэтому
/// прыжок попадает в заголовок, а не примерно туда. Если заголовков мало,
/// вехами служат начала абзацев, расставленные по документу примерно
/// поровну. Короткая страница не получает оглавления вовсе.
fn contents_of(marks: Vec<Mark>, total: i32) -> Vec<Mark> {
    if total < MIN_DOC_CHARS {
        return Vec::new();
    }

    let mut headings: Vec<Mark> = marks.iter().filter(|mark| mark.heading).cloned().collect();
    // Название статьи — не раздел: оно и так наверху.
    if matches!(headings.first(), Some(first) if first.level == 1 && first.offset == 0) {
        headings.remove(0);
    }
    if headings.len() >= MIN_HEADINGS {
        return headings;
    }

    let leads: Vec<&Mark> = marks.iter().filter(|mark| !mark.heading).collect();
    if leads.is_empty() {
        return Vec::new();
    }
    let wanted = ((total / MIN_DOC_CHARS.max(1)) as usize + 1).clamp(2, MAX_WAYPOINTS);

    let mut chosen: Vec<Mark> = Vec::with_capacity(wanted);
    let mut taken = 0usize;
    for step in 0..wanted {
        let target = total * (step as i32 * 2 + 1) / (wanted as i32 * 2);
        let Some((index, mark)) = leads
            .iter()
            .enumerate()
            .skip(taken)
            .min_by_key(|(_, mark)| (mark.offset - target).abs())
        else {
            break;
        };
        taken = index + 1;
        chosen.push((*mark).clone());
    }
    chosen
}

/// Показать оглавление и связать строки с местами в тексте.
fn fill_contents(list: &gtk::ListBox, marks: &[Mark], view: &gtk::TextView) {
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }

    for mark in marks {
        let label = gtk::Label::builder()
            .label(clip(&mark.title, 42))
            .xalign(0.0)
            .wrap(true)
            .margin_top(4)
            .margin_bottom(4)
            .margin_start(10 + i32::from(mark.level.saturating_sub(1)) * 12)
            .margin_end(10)
            .build();
        if !mark.heading {
            // Веха — не структура автора, а наша выжимка. Пусть это видно.
            label.add_css_class("dim-label");
        }

        let row = gtk::ListBoxRow::builder().child(&label).build();
        list.append(&row);
    }

    let offsets: Vec<i32> = marks.iter().map(|mark| mark.offset).collect();
    let view = view.clone();
    list.connect_row_activated(move |_, row| {
        let Some(&offset) = offsets.get(row.index().max(0) as usize) else {
            return;
        };
        // Точное попадание: заголовок встаёт под верх окна.
        scroll_to(&view, offset, ANCHOR_ALIGN);
    });
}

struct Writer<'a> {
    buffer: &'a gtk::TextBuffer,
    /// Нужен только ради картинок: виджет на якоре живёт в нём, а не в буфере.
    view: &'a gtk::TextView,
    /// Адрес документа: от него разворачиваются относительные ссылки картинок.
    base: &'a Address,
    links: &'a mut Vec<Link>,
    marks: &'a mut Vec<Mark>,
    anchors: &'a mut Vec<(String, i32)>,
    shots: &'a mut Vec<Shot>,
    /// Ячейки таблиц: ссылки внутри них живут в разметке `GtkLabel`,
    /// и вешать на них переход приходится снаружи.
    cells: &'a mut Vec<gtk::Label>,
    /// Глубина вложенности списка: от неё отступ пункта.
    depth: i32,
}

impl Writer<'_> {
    fn put(&mut self, text: &str, tags: &[&str]) {
        let mut end = self.buffer.end_iter();
        if tags.is_empty() {
            self.buffer.insert(&mut end, text);
        } else {
            self.buffer.insert_with_tags_by_name(&mut end, text, tags);
        }
    }

    fn offset(&self) -> i32 {
        self.buffer.end_iter().offset()
    }

    /// Что вставили с этого места — заголовок или начало абзаца.
    fn text_since(&self, start: i32) -> String {
        let from = self.buffer.iter_at_offset(start);
        let to = self.buffer.end_iter();
        self.buffer.text(&from, &to, false).to_string()
    }

    /// Таблица — сетка виджетов на якоре.
    ///
    /// Текстом её не набрать: в буфере нет колонок, и раньше строки
    /// склеивались палками в моноширинном — читать это нельзя. Цена решения
    /// записана честно: текст таблицы лежит в виджетах, а не в буфере,
    /// поэтому поиск по странице и «скопировать всё» её не видят. В markdown
    /// при сохранении таблица цела.
    fn table<'n>(&mut self, node: &'n comrak::nodes::AstNode<'n>, alignments: &[TableAlignment]) {
        let mut rows: Vec<(bool, Vec<String>)> = Vec::new();
        for row in node.children() {
            let NodeValue::TableRow(header) = row.data.borrow().value else {
                continue;
            };
            let cells: Vec<String> = row.children().map(markup_of).collect();
            if !cells.is_empty() {
                rows.push((header, cells));
            }
        }
        if rows.is_empty() {
            return;
        }
        let columns = rows.iter().map(|(_, cells)| cells.len()).max().unwrap_or(1);

        let grid = gtk::Grid::builder()
            .column_spacing(20)
            .row_spacing(7)
            .hexpand(true)
            .build();
        let mut line = 0;
        for (header, cells) in &rows {
            for (column, markup) in cells.iter().enumerate() {
                let cell = gtk::Label::builder()
                    .wrap(true)
                    .wrap_mode(pango::WrapMode::WordChar)
                    .max_width_chars(30)
                    .valign(gtk::Align::Start)
                    .selectable(true)
                    .can_focus(false)
                    .build();
                cell.set_markup(markup);
                // Выравнивание берём из самой таблицы: колонка чисел, объявленная
                // правой, должна стоять справа.
                let align = match alignments.get(column) {
                    Some(TableAlignment::Right) => 1.0,
                    Some(TableAlignment::Center) => 0.5,
                    _ => 0.0,
                };
                cell.set_xalign(align);
                // Остаток меры отдаём последнему столбцу: обычно там текст,
                // а не число, и ему перенос дороже.
                cell.set_hexpand(column + 1 == columns);
                cell.add_css_class(if *header { "th" } else { "td" });
                grid.attach(&cell, column as i32, line, 1, 1);
                self.cells.push(cell);
            }
            line += 1;
            if *header {
                let rule = gtk::Separator::new(gtk::Orientation::Horizontal);
                grid.attach(&rule, 0, line, columns as i32, 1);
                line += 1;
            }
        }

        let frame = self.anchor();
        frame.add_css_class("table");
        frame.append(&grid);
    }

    /// Поставить в текст место под картинку.
    ///
    /// Картинка — блок: своя строка сверху и снизу. Внутри абзаца её ставят
    /// редко, а разорванная надвое строка читается плохо.
    fn shot(&mut self, source: Source, alt: String, inline: bool) {
        let slot = if inline {
            // Холст, а не виджет: он занимает тот же один символ, поэтому
            // смещения ссылок, заголовков и поиска не едут, — а сотня
            // виджетов в строках текста рвала прокрутку.
            let canvas = Formula::new();
            let mut end = self.buffer.end_iter();
            self.buffer.insert_paintable(&mut end, &canvas);
            Slot::Canvas(canvas)
        } else {
            Slot::Frame(self.anchor())
        };
        self.shots.push(Shot {
            source,
            alt,
            inline,
            slot,
            busy: Rc::new(Cell::new(false)),
        });
    }

    /// Место под виджет в тексте: своя строка, рамка в меру.
    ///
    /// Картинка и таблица — блоки: строка с ними своя. Внутри абзаца их ставят
    /// редко, а разорванная надвое строка читается плохо.
    fn anchor(&mut self) -> gtk::Box {
        if !self.buffer.end_iter().starts_line() {
            self.put("\n", &[]);
        }
        let mut end = self.buffer.end_iter();
        let place = self.buffer.create_child_anchor(&mut end);

        let frame = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(4)
            .width_request(measure_px())
            .build();
        self.view.add_child_at_anchor(&frame, &place);
        self.put("\n", &["body"]);
        frame
    }

    fn block<'n>(&mut self, node: &'n comrak::nodes::AstNode<'n>, outer: &[&str]) {
        match &node.data.borrow().value {
            NodeValue::Heading(heading) => {
                let level = usize::from(heading.level).clamp(1, 6);
                let name = format!("h{level}");
                let mut tags = outer.to_vec();
                tags.push(&name);
                let start = self.offset();
                self.inlines(node, &tags);
                let title = self.text_since(start);
                self.anchors.push((anchor(&title), start));
                self.marks.push(Mark {
                    level: level as u8,
                    title,
                    offset: start,
                    heading: true,
                });
                self.put("\n", &[]);
            }
            NodeValue::Paragraph => {
                let mut tags = outer.to_vec();
                tags.push("body");
                let start = self.offset();
                self.inlines(node, &tags);
                let text = self.text_since(start);
                // Вехой может быть только настоящий абзац: у короткой
                // строки начало ничего не говорит.
                if text.chars().count() >= 120 {
                    self.marks.push(Mark {
                        level: 1,
                        title: lead(&text),
                        offset: start,
                        heading: false,
                    });
                }
                self.put("\n", &["body"]);
            }
            NodeValue::CodeBlock(code) => {
                let text = code.literal.trim_end_matches('\n');
                self.put("\n", &["pad"]);

                let mut at = 0;
                for span in code::spans(text, &code.info) {
                    if span.start > at {
                        self.put(&text[at..span.start], &["codeblock"]);
                    }
                    let paint = match span.kind {
                        code::Kind::Comment => "com",
                        code::Kind::Literal => "lit",
                        code::Kind::Number => "num",
                        code::Kind::Keyword => "kw",
                    };
                    self.put(&text[span.start..span.end], &["codeblock", paint]);
                    at = span.end;
                }
                if at < text.len() {
                    self.put(&text[at..], &["codeblock"]);
                }

                self.put("\n", &["codeblock"]);
                self.put("\n", &["pad"]);
            }
            NodeValue::BlockQuote => {
                let mut tags = outer.to_vec();
                tags.push("quote");
                for child in node.children() {
                    self.block(child, &tags);
                }
            }
            NodeValue::List(list) => {
                let ordered = matches!(list.list_type, ListType::Ordered);
                let mut number = list.start;

                self.depth += 1;
                let level = format!("list{}", self.depth.min(LIST_LEVELS));
                for item in node.children() {
                    let mut tags = outer.to_vec();
                    tags.push("body");
                    tags.push(&level);

                    let marker = match &item.data.borrow().value {
                        // Пункт списка задач: галочка вместо маркера — так его
                        // и рисуют везде, где markdown вообще про них знает.
                        NodeValue::TaskItem(done) => {
                            if done.symbol.is_some() {
                                "☑  ".to_owned()
                            } else {
                                "☐  ".to_owned()
                            }
                        }
                        _ if ordered => {
                            let marker = format!("{number}.  ");
                            number += 1;
                            marker
                        }
                        _ => "•  ".to_owned(),
                    };
                    self.put(&marker, &tags);

                    let start = self.offset();
                    for child in item.children() {
                        match &child.data.borrow().value {
                            NodeValue::Paragraph => {
                                self.inlines(child, &tags);
                                self.put("\n", &tags);
                            }
                            // Вложенный список, блок кода или цитата внутри
                            // пункта — обычный блок, только глубже.
                            _ => self.block(child, outer),
                        }
                    }
                    if self.offset() == start {
                        self.put("\n", &tags);
                    }
                }
                self.depth -= 1;

                if self.depth == 0 {
                    self.put("\n", &["body"]);
                }
            }
            NodeValue::ThematicBreak => self.put("* * *\n\n", &["dim"]),
            NodeValue::Table(table) => {
                let alignments = table.alignments.clone();
                self.table(node, &alignments);
            }
            _ => {
                for child in node.children() {
                    self.block(child, outer);
                }
            }
        }
    }

    fn inlines<'n>(&mut self, node: &'n comrak::nodes::AstNode<'n>, tags: &[&str]) {
        for child in node.children() {
            match &child.data.borrow().value {
                NodeValue::Text(text) => self.put(text, tags),
                NodeValue::Code(code) => {
                    let mut with = tags.to_vec();
                    with.push("code");
                    self.put(&code.literal, &with);
                }
                NodeValue::Emph => {
                    let mut with = tags.to_vec();
                    with.push("em");
                    self.inlines(child, &with);
                }
                NodeValue::Strong => {
                    let mut with = tags.to_vec();
                    with.push("strong");
                    self.inlines(child, &with);
                }
                NodeValue::Link(link) => {
                    let start = self.offset();
                    let mut with = tags.to_vec();
                    with.push("link");
                    self.inlines(child, &with);
                    let end = self.offset();
                    self.links.push(Link {
                        start,
                        end,
                        target: link.url.clone(),
                    });
                }
                NodeValue::Image(image) => {
                    let alt = plain_text(child).trim().to_owned();
                    // Картинка одна в абзаце — иллюстрация; окружённая
                    // текстом — часть строки. Вторым способом в вебе набирают
                    // формулы: википедия печатает их картинками MathJax.
                    let inline = !stands_alone(child);
                    match media::resolve(self.base, &image.url) {
                        Some(source) => self.shot(source, alt, inline),
                        // Чего сами не достанем (`data:`, `blob:`) — оставляем
                        // строкой: честнее пустой рамки.
                        None => {
                            let mut with = tags.to_vec();
                            with.push("dim");
                            let label = if alt.is_empty() {
                                "[image]".to_owned()
                            } else {
                                format!("[image: {alt}]")
                            };
                            self.put(&label, &with);
                        }
                    }
                }
                NodeValue::SoftBreak => self.put(" ", tags),
                NodeValue::LineBreak => self.put("\n", tags),
                _ => self.inlines(child, tags),
            }
        }
    }
}

// ── картинки ────────────────────────────────────────────────────────────────

/// Заглушка на месте картинки: нажмёшь — загрузится.
///
/// По умолчанию картинки не грузятся вовсе. Это не осторожность ради
/// осторожности: после отказа от JS декодер картинок остаётся единственной
/// серьёзной поверхностью атаки, и решение открыть её принимает читатель —
/// кликом или переключателем в шапке.
fn place_shot(ui: &Ui, state: &Rc<RefCell<State>>, id: u64, shot: &Shot, trouble: Option<&str>) {
    // Формула живёт холстом в буфере: ставить в неё нечего, надо только
    // решить, чем её показывать до картинки. Исходник `{\displaystyle b}`
    // читается плохо, но лучше пустого места посреди фразы.
    if let Slot::Canvas(canvas) = &shot.slot {
        let waiting = trouble.is_some() || !state.borrow().images;
        let layout = waiting.then(|| {
            let view = view_of(state, id);
            let layout = match &view {
                Some(view) => view.create_pango_layout(Some(&formula(&shot.alt))),
                None => return None,
            };
            layout.set_font_description(Some(&pango::FontDescription::from_string(&format!(
                "{BODY_FAMILY} {}",
                TEXT_SIZE
            ))));
            Some(layout)
        });
        canvas.set_fallback(layout.flatten(), ink_color(state.borrow().dark));
        return;
    }

    let Some(frame) = shot.slot.frame() else { return };

    let name = if shot.alt.is_empty() {
        "image".to_owned()
    } else {
        format!("image: {}", clip(&shot.alt, 160))
    };
    let label = match trouble {
        Some(trouble) => format!("{trouble}\n{name}"),
        None => name,
    };

    let button = gtk::Button::builder().label(&label).has_frame(false).build();
    button.add_css_class("shot");
    button.set_cursor_from_name(Some("pointer"));
    button.set_tooltip_text(Some(&shot.source.display()));
    if let Some(text) = button.child().and_downcast::<gtk::Label>() {
        text.set_wrap(true);
        text.set_justify(gtk::Justification::Center);
    }
    {
        let ui = ui.clone();
        let state = state.clone();
        let shot = shot.clone();
        button.connect_clicked(move |_| load_shot(&ui, &state, id, &shot));
    }
    fill(frame, &button);
}

/// Загрузить все картинки открытой вкладки.
fn show_all_shots(ui: &Ui, state: &Rc<RefCell<State>>) {
    let Some(index) = ui.notebook.current_page() else {
        return;
    };
    let Some((id, shots)) = ({
        let borrowed = state.borrow();
        borrowed
            .tabs
            .get(index as usize)
            .map(|tab| (tab.id, tab.shots.clone()))
    }) else {
        return;
    };
    for shot in &shots {
        load_shot(ui, state, id, shot);
    }
}

/// Скачать и показать одну картинку.
///
/// Скачивание и декодирование уходят в отдельный поток: `ureq` синхронный,
/// а схема на пол-мегабайта разбирается заметное время — окно не должно
/// вставать на ней колом.
fn load_shot(ui: &Ui, state: &Rc<RefCell<State>>, id: u64, shot: &Shot) {
    if shot.busy.replace(true) {
        return;
    }
    let Some(generation) = state.borrow_mut().find(id).map(|tab| tab.generation) else {
        return;
    };

    if let Some(frame) = shot.slot.frame() {
        let waiting = gtk::Label::builder()
            .label("loading the image…")
            .wrap(true)
            .build();
        waiting.add_css_class("caption");
        fill(frame, &waiting);
    }

    let source = shot.source.clone();
    let look = media::Look {
        width: measure_px().max(1) as u32,
        // Прозрачное кладём на светлую бумагу, а не на белое: белая карточка
        // посреди слоновой кости заметна, а схеме нужен только светлый фон —
        // на тёмной теме тем более.
        paper: rgb(PAPER_LIGHT),
        font_size: text_px(),
        fit: if shot.inline {
            media::Fit::Natural
        } else {
            media::Fit::Column
        },
    };
    let ui = ui.clone();
    let state = state.clone();
    let shot = shot.clone();

    glib::spawn_future_local(async move {
        let loaded =
            gio::spawn_blocking(move || media::load(&source, UserAgent::Honest, look)).await;

        // Вкладку успели увести на другую страницу — рамки уже нет.
        if state.borrow_mut().find(id).map(|tab| tab.generation) != Some(generation) {
            return;
        }
        match loaded {
            Ok(Ok(raster)) => show_shot(&shot, raster),
            Ok(Err(error)) => {
                shot.busy.set(false);
                place_shot(&ui, &state, id, &shot, Some(describe(&error).headline));
            }
            Err(_) => {
                shot.busy.set(false);
                place_shot(&ui, &state, id, &shot, Some("The load fell through"));
            }
        }
    });
}

/// Исходник формулы из `alt`: MathJax заворачивает его в `{\displaystyle …}`,
/// и читателю эта обёртка не нужна.
fn formula(alt: &str) -> String {
    let text = alt.trim();
    let inner = text
        .strip_prefix('{')
        .and_then(|text| text.strip_suffix('}'))
        .map(|text| text.trim())
        .and_then(|text| text.strip_prefix("\\displaystyle").or(Some(text)))
        .unwrap_or(text);
    let inner = inner.trim();
    if inner.is_empty() {
        "formula".to_owned()
    } else {
        inner.to_owned()
    }
}

/// Показать разобранную картинку с подписью.
fn show_shot(shot: &Shot, raster: Raster) {
    let bytes = glib::Bytes::from_owned(raster.rgba);
    let texture = gtk::gdk::MemoryTexture::new(
        raster.width as i32,
        raster.height as i32,
        // Ядро отдаёт непрозрачный RGBA: прозрачное оно кладёт на белое ещё
        // при декодировании, иначе схема с чёрными линиями пропадала бы
        // на тёмной теме.
        gtk::gdk::MemoryFormat::R8g8b8a8,
        &bytes,
        raster.width as usize * 4,
    );

    // Формула — холст в буфере: меняем в нём картинку, виджета тут нет вовсе.
    if let Slot::Canvas(canvas) = &shot.slot {
        canvas.set_texture(texture.upcast_ref());
        return;
    }
    let Some(frame) = shot.slot.frame() else { return };

    // Если картинка уже стоит в рамке — меняем холст, а не ребёнка:
    // перестройка дерева виджетов посреди кадра и есть та самая жалоба
    // GTK на снимок без раскладки.
    let picture = match frame.first_child().and_downcast::<gtk::Picture>() {
        Some(picture) => {
            picture.set_paintable(Some(&texture));
            picture
        }
        None => {
            let picture = gtk::Picture::for_paintable(&texture);
            fill(frame, &picture);
            picture
        }
    };
    picture.set_can_shrink(true);
    picture.set_size_request(raster.width as i32, raster.height as i32);
    picture.set_cursor_from_name(Some("pointer"));
    picture.set_halign(gtk::Align::Center);
    picture.set_tooltip_text(Some(&shot.source.display()));

    // Полный размер — работа системного браузера: у нас картинка ужата
    // до меры текста.
    let click = gtk::GestureClick::new();
    let target = shot.source.display();
    click.connect_released(move |_, _, _, _| open_in_system_browser(&target));
    picture.add_controller(click);

    if !shot.alt.is_empty() {
        // Подпись стоит под картинкой, а не под колонкой: узкая картинка
        // висит по центру, и подпись у левого поля выглядела бы чужой.
        let narrow = raster.width < measure_px() as u32;
        let caption = gtk::Label::builder()
            .label(&shot.alt)
            .wrap(true)
            .xalign(if narrow { 0.5 } else { 0.0 })
            .justify(if narrow {
                gtk::Justification::Center
            } else {
                gtk::Justification::Left
            })
            .build();
        caption.add_css_class("caption");
        frame.append(&caption);
    }
}

/// Единственный ребёнок рамки — тот, что дали.
fn fill(frame: &gtk::Box, child: &impl IsA<gtk::Widget>) {
    while let Some(old) = frame.first_child() {
        frame.remove(&old);
    }
    frame.append(child);
}

// ── сохранение ──────────────────────────────────────────────────────────────

/// Спросить, куда класть статью, и сохранить.
///
/// Имя предлагаем по документу: есть картинки — архив, нет — просто текст.
/// Читатель волен переименовать, и расширение из диалога старше нашего.
fn ask_where_to_save(ui: &Ui, state: &Rc<RefCell<State>>) {
    let Some(document) = current_document(ui, state) else {
        notice(ui, "Nothing to save yet");
        return;
    };

    let chooser = gtk::FileChooserNative::new(
        Some("Save article"),
        Some(&ui.window),
        gtk::FileChooserAction::Save,
        Some("Save"),
        Some("Cancel"),
    );
    chooser.set_current_name(&save::suggested_name(&document));

    let chooser = Rc::new(chooser);
    let alive = chooser.clone();
    let ui = ui.clone();
    chooser.connect_response(move |chooser, answer| {
        // Ссылка на самого себя держит диалог в живых до ответа: местных
        // переменных к этому моменту уже нет.
        let _ = &alive;
        chooser.hide();
        if answer != gtk::ResponseType::Accept {
            return;
        }
        let Some(path) = chooser.file().and_then(|file| file.path()) else {
            return;
        };
        save_to(&ui, path, document.clone());
    });
    chooser.show();
}

/// Записать статью на диск.
///
/// Картинки для архива качаются здесь же, поэтому работа уходит в отдельный
/// поток: десяток иллюстраций — это десяток сетевых запросов.
fn save_to(ui: &Ui, path: std::path::PathBuf, document: Document) {
    notice(ui, "Saving…");
    let ui = ui.clone();

    glib::spawn_future_local(async move {
        let done =
            gio::spawn_blocking(move || save::write(&path, &document, UserAgent::Honest)).await;

        match done {
            Ok(Ok(saved)) => {
                let name = saved
                    .path
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_else(|| saved.path.display().to_string());
                let mut said = format!("Saved as {name}");
                if saved.images > 0 {
                    said.push_str(&format!(" · {}", count(saved.images, "image", "images")));
                }
                if saved.missed > 0 {
                    said.push_str(&format!(
                        " · {} could not be fetched",
                        count(saved.missed, "image", "images")
                    ));
                }
                notice(&ui, &said);
            }
            Ok(Err(error)) => notice(&ui, &format!("Not saved: {}", describe(&error).headline)),
            Err(_) => notice(&ui, "Not saved: the write was interrupted"),
        }
    });
}

fn count(how_many: usize, one: &str, many: &str) -> String {
    if how_many == 1 {
        format!("{how_many} {one}")
    } else {
        format!("{how_many} {many}")
    }
}

fn current_document(ui: &Ui, state: &Rc<RefCell<State>>) -> Option<Document> {
    let index = ui.notebook.current_page()? as usize;
    state.borrow().tabs.get(index)?.document.clone()
}

/// Строка состояния внизу окна. Сама и убирается: сообщение о сохранении
/// живёт ровно столько, сколько на него смотрят.
fn notice(ui: &Ui, said: &str) {
    ui.notice.set_text(said);
    ui.notice.set_visible(!said.is_empty());

    let label = ui.notice.clone();
    let said = said.to_owned();
    glib::timeout_add_local_once(std::time::Duration::from_secs(8), move || {
        if label.text() == said {
            label.set_text("");
            label.set_visible(false);
        }
    });
}

// ── поиск по странице ───────────────────────────────────────────────────────

/// Найти всё и встать на ближайшее совпадение.
///
/// `restart` значит «читатель поменял запрос»: тогда ищем от того места,
/// которое он видит, а не от начала документа.
fn find(ui: &Ui, state: &Rc<RefCell<State>>, needle: &str, restart: bool) {
    let Some(view) = current(ui, state) else {
        return;
    };
    let buffer = view.buffer();
    let (start, end) = buffer.bounds();
    buffer.remove_tag_by_name("found", &start, &end);
    buffer.remove_tag_by_name("here", &start, &end);

    let hits = if needle.is_empty() {
        Vec::new()
    } else {
        hits_of(&buffer, needle)
    };
    for (from, to) in &hits {
        buffer.apply_tag_by_name(
            "found",
            &buffer.iter_at_offset(*from),
            &buffer.iter_at_offset(*to),
        );
    }

    let at = if restart {
        first_visible(&view, &hits)
    } else {
        state.borrow().search.at.min(hits.len().saturating_sub(1))
    };
    {
        let mut borrowed = state.borrow_mut();
        borrowed.search.hits = hits;
        borrowed.search.at = at;
    }

    let empty = state.borrow().search.hits.is_empty();
    if empty && !needle.is_empty() {
        ui.needle.add_css_class("error");
        ui.tally.set_text("no matches");
    } else {
        ui.needle.remove_css_class("error");
        ui.tally.set_text("");
    }
    show_hit(ui, state, &view);
}

/// Шаг по совпадениям, по кругу: дойдя до низа, поиск начинает сверху.
fn step_hit(ui: &Ui, state: &Rc<RefCell<State>>, forward: bool) {
    let Some(view) = current(ui, state) else {
        return;
    };
    {
        let mut borrowed = state.borrow_mut();
        let total = borrowed.search.hits.len();
        if total == 0 {
            return;
        }
        let at = borrowed.search.at;
        borrowed.search.at = if forward {
            (at + 1) % total
        } else {
            (at + total - 1) % total
        };
    }
    show_hit(ui, state, &view);
}

/// Подсветить то совпадение, на котором стоим, и подвести к нему страницу.
fn show_hit(ui: &Ui, state: &Rc<RefCell<State>>, view: &gtk::TextView) {
    let buffer = view.buffer();
    let (start, end) = buffer.bounds();
    buffer.remove_tag_by_name("here", &start, &end);

    let (total, at, hit) = {
        let borrowed = state.borrow();
        (
            borrowed.search.hits.len(),
            borrowed.search.at,
            borrowed.search.hits.get(borrowed.search.at).copied(),
        )
    };
    let Some((from, to)) = hit else { return };

    ui.tally.set_text(&format!("{} of {total}", at + 1));
    buffer.apply_tag_by_name(
        "here",
        &buffer.iter_at_offset(from),
        &buffer.iter_at_offset(to),
    );
    // Треть экрана сверху: совпадение нужно видеть в контексте, а не в самом
    // верху окна.
    scroll_to(view, from, 0.3);
}

fn hits_of(buffer: &gtk::TextBuffer, needle: &str) -> Vec<(i32, i32)> {
    // Регистр не важен: читатель ищет слово, а не написание. `TEXT_ONLY`
    // велит не спотыкаться о картинки — на их месте в буфере стоит якорь.
    let flags = gtk::TextSearchFlags::CASE_INSENSITIVE | gtk::TextSearchFlags::TEXT_ONLY;
    let mut hits = Vec::new();
    let mut from = buffer.start_iter();

    while let Some((start, end)) = from.forward_search(needle, flags, None) {
        hits.push((start.offset(), end.offset()));
        if hits.len() >= MAX_HITS {
            break;
        }
        from = end;
    }
    hits
}

/// Первое совпадение, которое читатель уже видит или увидит ниже.
fn first_visible(view: &gtk::TextView, hits: &[(i32, i32)]) -> usize {
    let top = view.visible_rect();
    let offset = view
        .iter_at_location(top.x(), top.y())
        .map(|iter| iter.offset())
        .unwrap_or(0);
    hits.iter()
        .position(|(from, _)| *from >= offset)
        .unwrap_or(0)
}

/// Стоит ли картинка в абзаце одна.
///
/// Соседи-пробелы не в счёт: `![схема](url)` на своей строке приходит
/// с переводами строк по краям, и это всё равно иллюстрация.
fn stands_alone<'n>(image: &'n comrak::nodes::AstNode<'n>) -> bool {
    let empty = |node: Option<&'n comrak::nodes::AstNode<'n>>| match node {
        None => true,
        Some(node) => match &node.data.borrow().value {
            NodeValue::Text(text) => text.trim().is_empty(),
            NodeValue::SoftBreak | NodeValue::LineBreak => true,
            _ => false,
        },
    };
    empty(image.previous_sibling()) && empty(image.next_sibling())
}

/// Ячейка таблицы разметкой Pango: курсив, полужирный, код и ссылки.
///
/// `GtkLabel` понимает подмножество разметки и сам делает ссылки живыми —
/// иначе пришлось бы городить виджет на каждую ячейку.
fn markup_of<'n>(node: &'n comrak::nodes::AstNode<'n>) -> String {
    let mut out = String::new();
    for child in node.children() {
        match &child.data.borrow().value {
            NodeValue::Text(text) => out.push_str(&glib::markup_escape_text(text)),
            NodeValue::Code(code) => {
                out.push_str("<tt>");
                out.push_str(&glib::markup_escape_text(&code.literal));
                out.push_str("</tt>");
            }
            NodeValue::Emph => out.push_str(&format!("<i>{}</i>", markup_of(child))),
            NodeValue::Strong => out.push_str(&format!("<b>{}</b>", markup_of(child))),
            NodeValue::Link(link) => out.push_str(&format!(
                "<a href=\"{}\">{}</a>",
                glib::markup_escape_text(&link.url),
                markup_of(child)
            )),
            NodeValue::Image(_) => out.push_str(&glib::markup_escape_text(&plain_text(child))),
            NodeValue::SoftBreak | NodeValue::LineBreak => out.push(' '),
            _ => out.push_str(&markup_of(child)),
        }
    }
    out
}

/// Ссылка в ячейке ведёт туда же, куда вела бы в тексте.
fn follow_cell_links(ui: &Ui, state: &Rc<RefCell<State>>, id: u64, cell: &gtk::Label) {
    let ui = ui.clone();
    let state = state.clone();
    cell.connect_activate_link(move |_, target| {
        let jumped = {
            let mut borrowed = state.borrow_mut();
            match borrowed.find(id) {
                Some(tab) => {
                    let here = tab.history.current().map(Address::display);
                    let view = tab.view.clone();
                    fragment_of(target, here.as_deref())
                        .is_some_and(|fragment| jump(&view, &tab.anchors, &fragment))
                }
                None => false,
            }
        };
        if !jumped && let Ok(address) = address::parse(target) {
            open(&ui, &state, id, address, true);
        }
        glib::Propagation::Stop
    });
}

/// Текст узла без разметки — для подписей картинок и ячеек таблицы.
fn plain_text<'n>(node: &'n comrak::nodes::AstNode<'n>) -> String {
    let mut out = String::new();
    for child in node.descendants() {
        match &child.data.borrow().value {
            NodeValue::Text(text) => out.push_str(text),
            NodeValue::Code(code) => out.push_str(&code.literal),
            NodeValue::SoftBreak | NodeValue::LineBreak => out.push(' '),
            _ => {}
        }
    }
    out
}
