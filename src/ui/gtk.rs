//! Окно Brevier на GTK4.
//!
//! Статья рисуется одним `GtkTextView`, а не набором виджетов на абзац:
//! выделение должно идти через весь документ, а не обрываться на границе
//! абзаца. Тем же решением бесплатно приходят копирование, контекстное меню,
//! точное оглавление по меткам в тексте и доступность через AT-SPI.
//!
//! Тулкит живёт только здесь. Разбор адреса, история, оглавление и тексты
//! ошибок лежат в ядре и про GTK не знают ничего.

use std::cell::RefCell;
use std::rc::Rc;

use gtk::gio;
use gtk::glib;
use gtk::pango;
use gtk::prelude::*;
use gtk::{Application, ApplicationWindow};

use brevier::address::{self, Address};
use brevier::failure::describe;
use brevier::outline::{
    HEADINGS, LINE_HEIGHT, MAX_WAYPOINTS, MEASURE, MIN_HEADINGS, TEXT_SIZE, clip, lead,
};
use brevier::{Document, History, UserAgent};

const APP_ID: &str = "dev.brevier.Brevier";
const BODY_FAMILY: &str = "PT Serif";
const MONO_FAMILY: &str = "PT Mono";
/// Жирность в единицах Pango: свойство тега — целое, а не перечисление.
const BOLD: i32 = 700;
const TOC_WIDTH: i32 = 260;
/// Короче этого оглавление не нужно: страница и так вся под рукой.
const MIN_DOC_CHARS: i32 = 4000;
/// Сколько знаков влезает на корешок вкладки.
const TAB_LABEL: usize = 24;

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
    /// Номер загрузки. Ответ брошенной страницы отличаем по нему: отменить
    /// синхронный `ureq` нечем, но и слушать его уже незачем.
    generation: u64,
}

struct State {
    tabs: Vec<Tab>,
    next_id: u64,
    /// Тема общая для всех вкладок: это настройка читателя, а не страницы.
    dark: bool,
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
            .placeholder_text("адрес, gh:owner/repo или путь к .md")
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
            .tooltip_text("Оглавление")
            .active(true)
            .sensitive(false)
            .build(),
        dark_mode: gtk::ToggleButton::builder()
            .icon_name("weather-clear-night-symbolic")
            .tooltip_text("Тёмная тема")
            .active(true)
            .build(),
    };
    ui.contents_pane.set_child(Some(&ui.contents));
    ui.notebook.set_hexpand(true);
    ui.notebook.set_vexpand(true);
    ui.back.set_sensitive(false);
    ui.forward.set_sensitive(false);

    let new_tab_button = gtk::Button::from_icon_name("tab-new-symbolic");
    new_tab_button.set_tooltip_text(Some("Новая вкладка (Ctrl+T)"));

    let header = gtk::HeaderBar::builder().build();
    header.pack_start(&ui.back);
    header.pack_start(&ui.forward);
    header.pack_start(&new_tab_button);
    header.pack_end(&ui.dark_mode);
    header.pack_end(&ui.show_contents);
    header.set_title_widget(Some(&ui.entry));
    ui.window.set_titlebar(Some(&header));

    let body = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    body.append(&ui.notebook);
    body.append(&ui.contents_pane);
    ui.window.set_child(Some(&body));

    let state = Rc::new(RefCell::new(State {
        tabs: Vec::new(),
        next_id: 0,
        dark: true,
    }));
    apply_theme(&state);

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
                        show_message(&tab, problem.headline, &problem.detail);
                    }
                }
            }
        });
    }
    {
        let ui = ui.clone();
        let state = state.clone();
        ui.back.clone().connect_clicked(move |_| step(&ui, &state, true));
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
        let state = state.clone();
        ui.dark_mode.clone().connect_toggled(move |button| {
            state.borrow_mut().dark = button.is_active();
            apply_theme(&state);
        });
    }
    {
        let pane = ui.contents_pane.clone();
        ui.show_contents.clone().connect_toggled(move |button| {
            pane.set_visible(button.is_active() && button.is_sensitive());
        });
    }
    {
        let ui = ui.clone();
        let state = state.clone();
        ui.notebook
            .clone()
            .connect_switch_page(move |_, _, index| {
                sync(&ui, &state, Some(index as usize));
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
}

/// Открыть новую вкладку и, если дали адрес, сразу читать.
fn new_tab(ui: &Ui, state: &Rc<RefCell<State>>, address: Option<Address>) {
    let view = gtk::TextView::builder()
        .editable(false)
        .cursor_visible(false)
        .wrap_mode(gtk::WrapMode::Word)
        .halign(gtk::Align::Center)
        .top_margin(28)
        .bottom_margin(80)
        .build();
    tags(&view.buffer(), state.borrow().dark);
    view.set_width_request(measure_px());

    let scroller = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .hexpand(true)
        .vexpand(true)
        .child(&view)
        .build();

    let label = gtk::Label::builder()
        .label("Новая вкладка")
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
            let index = state
                .borrow()
                .tabs
                .iter()
                .position(|tab| tab.id == id);
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
            let target = {
                let mut state = state.borrow_mut();
                let Some(tab) = state.find(id) else { return };
                link_at(&view, &tab.links, x, y)
            };
            let Some(target) = target else { return };
            let Ok(address) = address::parse(&target) else {
                return;
            };
            // Как в браузерах: Ctrl и средняя кнопка открывают вкладкой,
            // обычный клик уводит на страницу.
            let ctrl = gesture
                .current_event_state()
                .contains(gtk::gdk::ModifierType::CONTROL_MASK);
            let middle = gesture.current_button() == gtk::gdk::BUTTON_MIDDLE;
            if ctrl || middle {
                new_tab(&ui, &state, Some(address));
            } else if gesture.current_button() == gtk::gdk::BUTTON_PRIMARY {
                open(&ui, &state, id, address, true);
            }
        });
    }
    view.add_controller(click);

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
                gtk::gdk::Key::space | gtk::gdk::Key::Page_Down => {
                    adjustment.value() + page * 0.9
                }
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
        sync(ui, state, None);
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
    let generation = {
        let mut state = state.borrow_mut();
        let Some(tab) = state.find(id) else { return };
        if remember {
            tab.history.visit(address.clone());
        }
        tab.generation += 1;
        tab.label.set_text(&clip("Загружаю…", TAB_LABEL));
        tab.generation
    };
    sync(ui, state, None);

    if let Some(view) = view_of(state, id) {
        show_message(&view, "Загружаю…", "");
    }

    let ui = ui.clone();
    let state = state.clone();
    glib::spawn_future_local(async move {
        let loaded =
            gio::spawn_blocking(move || brevier::open(&address, UserAgent::Honest)).await;

        // Читатель уже ушёл на другую страницу — ответ никому не нужен.
        if state.borrow_mut().find(id).map(|tab| tab.generation) != Some(generation) {
            return;
        }
        let Some(view) = view_of(&state, id) else { return };

        match loaded {
            Ok(Ok(document)) => {
                let page = render(&view, &document);
                view.grab_focus();
                let mut borrowed = state.borrow_mut();
                if let Some(tab) = borrowed.find(id) {
                    tab.label.set_text(&clip(&document.title, TAB_LABEL));
                    tab.label.set_tooltip_text(Some(&document.title));
                    tab.links = page.links;
                    tab.marks = page.marks;
                }
                drop(borrowed);
                sync(&ui, &state, None);
            }
            Ok(Err(error)) => {
                let problem = describe(&error);
                show_message(&view, problem.headline, &problem.detail);
                let mut borrowed = state.borrow_mut();
                if let Some(tab) = borrowed.find(id) {
                    tab.label.set_text(&clip(problem.headline, TAB_LABEL));
                    tab.links.clear();
                    tab.marks.clear();
                }
                drop(borrowed);
                sync(&ui, &state, None);
            }
            Err(_) => show_message(&view, "Загрузка сорвалась", ""),
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
            tab.history.current().map(Address::display).unwrap_or_default(),
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
}

/// Тема. Кроме настройки GTK перекрашиваем свои теги: цвет ссылки
/// и приглушённого текста — часть типографики, а не оформления окна,
/// и в теге он задан явно.
fn apply_theme(state: &Rc<RefCell<State>>) {
    let dark = state.borrow().dark;
    if let Some(settings) = gtk::Settings::default() {
        settings.set_gtk_application_prefer_dark_theme(dark);
    }
    for tab in &state.borrow().tabs {
        recolor(&tab.view.buffer(), dark);
    }
}

fn recolor(buffer: &gtk::TextBuffer, dark: bool) {
    let table = buffer.tag_table();
    let (link, dim) = palette(dark);
    if let Some(tag) = table.lookup("link") {
        tag.set_property("foreground", link);
    }
    if let Some(tag) = table.lookup("dim") {
        tag.set_property("foreground", dim);
    }
}

/// Цвет ссылки и приглушённого текста под тему.
fn palette(dark: bool) -> (&'static str, &'static str) {
    if dark {
        ("#88c0d0", "#8b98a5")
    } else {
        ("#0b6ea8", "#6b7480")
    }
}

/// Мера в пикселях. В GTK кегль задаётся пунктами, а ширина виджета
/// пикселями; без пересчёта по разрешению в строке оказывалось бы разное
/// число знаков на разных экранах.
fn measure_px() -> i32 {
    let dpi = gtk::Settings::for_display(&gtk::gdk::Display::default().unwrap()).gtk_xft_dpi();
    // Настройка хранится в 1024-х долях точки; 0 или -1 значит «не задано».
    let dpi = if dpi > 0 { f64::from(dpi) / 1024.0 } else { 96.0 };
    (f64::from(MEASURE) * dpi / 72.0).round() as i32
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
    const FONTS: [(&str, &[u8]); 5] = [
        ("PTSerif-Regular.ttf", include_bytes!("../../assets/fonts/PTSerif-Regular.ttf")),
        ("PTSerif-Italic.ttf", include_bytes!("../../assets/fonts/PTSerif-Italic.ttf")),
        ("PTSerif-Bold.ttf", include_bytes!("../../assets/fonts/PTSerif-Bold.ttf")),
        ("PTSerif-BoldItalic.ttf", include_bytes!("../../assets/fonts/PTSerif-BoldItalic.ttf")),
        ("PTMono-Regular.ttf", include_bytes!("../../assets/fonts/PTMono-Regular.ttf")),
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
fn link_at(view: &gtk::TextView, links: &[Link], x: f64, y: f64) -> Option<String> {
    let (bx, by) = view.window_to_buffer_coords(gtk::TextWindowType::Widget, x as i32, y as i32);
    let (iter, _) = view.iter_at_position(bx, by)?;
    let offset = iter.offset();
    links
        .iter()
        .find(|link| offset >= link.start && offset < link.end)
        .map(|link| link.target.clone())
}

fn show_message(view: &gtk::TextView, headline: &str, detail: &str) {
    let buffer = view.buffer();
    buffer.set_text("");
    let mut end = buffer.end_iter();
    buffer.insert_with_tags_by_name(&mut end, headline, &["h2"]);
    if !detail.is_empty() {
        buffer.insert(&mut end, "\n\n");
        buffer.insert(&mut end, detail);
    }
}/// Теги — вся типографика статьи. Кегли и интерлиньяж те же, что были
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
                ("weight", &BOLD),
                // Воздух сверху, а не снизу: заголовок принадлежит тому,
                // что под ним.
                ("pixels-above-lines", &(extra * 3)),
                ("pixels-below-lines", &extra),
            ],
        );
    }

    buffer.create_tag(Some("em"), &[("style", &pango::Style::Italic)]);
    buffer.create_tag(Some("strong"), &[("weight", &BOLD)]);
    buffer.create_tag(
        Some("code"),
        &[("family", &MONO_FAMILY), ("size-points", &(body * 0.88))],
    );
    buffer.create_tag(
        Some("codeblock"),
        &[
            ("family", &MONO_FAMILY),
            ("size-points", &(body * 0.88)),
            ("left-margin", &24),
            ("pixels-below-lines", &extra),
        ],
    );
    buffer.create_tag(
        Some("quote"),
        &[("style", &pango::Style::Italic), ("left-margin", &24)],
    );
    let (link, dim) = palette(dark);
    buffer.create_tag(
        Some("link"),
        &[("underline", &pango::Underline::Single), ("foreground", &link)],
    );
    buffer.create_tag(Some("dim"), &[("foreground", &dim)]);
}

/// Разложить статью по буферу. Возвращает ссылки с их местами в тексте —
/// по ним потом опознаётся клик.
struct Page {
    links: Vec<Link>,
    marks: Vec<Mark>,
}

fn render(view: &gtk::TextView, document: &Document) -> Page {
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
    let mut writer = Writer {
        buffer: &buffer,
        links: &mut links,
        marks: &mut marks,
    };

    for node in root.children() {
        writer.block(node, &[]);
    }
    let marks = contents_of(marks, buffer.char_count());

    // Возврат наверх: новая статья начинается сначала. Через `idle`,
    // потому что в момент вставки текста у виджета ещё нет раскладки
    // и прокручивать ему некуда.
    buffer.place_cursor(&buffer.start_iter());
    let view = view.clone();
    glib::idle_add_local_once(move || {
        let mut start = view.buffer().start_iter();
        view.scroll_to_iter(&mut start, 0.0, true, 0.0, 0.0);
    });
    Page { links, marks }
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
        let mut iter = view.buffer().iter_at_offset(offset);
        // Точное попадание: заголовок встаёт под верх окна.
        view.scroll_to_iter(&mut iter, 0.0, true, 0.0, 0.05);
    });
}

struct Writer<'a> {
    buffer: &'a gtk::TextBuffer,
    links: &'a mut Vec<Link>,
    marks: &'a mut Vec<Mark>,
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

    fn block<'n>(&mut self, node: &'n comrak::nodes::AstNode<'n>, outer: &[&str]) {
        use comrak::nodes::NodeValue;

        match &node.data.borrow().value {
            NodeValue::Heading(heading) => {
                let level = usize::from(heading.level).clamp(1, 6);
                let name = format!("h{level}");
                let mut tags = outer.to_vec();
                tags.push(&name);
                let start = self.offset();
                self.inlines(node, &tags);
                let title = self.text_since(start);
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
                self.put(code.literal.trim_end_matches('\n'), &["codeblock"]);
                self.put("\n", &["codeblock"]);
            }
            NodeValue::BlockQuote => {
                let mut tags = outer.to_vec();
                tags.push("quote");
                for child in node.children() {
                    self.block(child, &tags);
                }
            }
            NodeValue::List(_) => {
                for item in node.children() {
                    let mut tags = outer.to_vec();
                    tags.push("body");
                    self.put("  •  ", &tags);
                    for child in item.children() {
                        self.inlines(child, &tags);
                    }
                    self.put("\n", &tags);
                }
                self.put("\n", &["body"]);
            }
            NodeValue::ThematicBreak => self.put("* * *\n\n", &["dim"]),
            NodeValue::Table(_) => {
                // Таблицы в буфере честно вырождаются в столбцы моноширинным:
                // настоящая таблица потребует виджетов на якорях, и это
                // отдельная работа.
                for row in node.descendants() {
                    if let NodeValue::TableRow(_) = row.data.borrow().value {
                        let cells: Vec<String> = row
                            .children()
                            .map(|cell| plain_text(cell).trim().to_owned())
                            .collect();
                        self.put(&cells.join("  │  "), &["codeblock"]);
                        self.put("\n", &["codeblock"]);
                    }
                }
                self.put("\n", &["body"]);
            }
            _ => {
                for child in node.children() {
                    self.block(child, outer);
                }
            }
        }
    }

    fn inlines<'n>(&mut self, node: &'n comrak::nodes::AstNode<'n>, tags: &[&str]) {
        use comrak::nodes::NodeValue;

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
                    // Картинки не грузим до M4: показываем, что здесь было
                    // изображение, и подпись.
                    let mut with = tags.to_vec();
                    with.push("dim");
                    let alt = plain_text(child);
                    let label = if alt.trim().is_empty() {
                        "[изображение]".to_owned()
                    } else {
                        format!("[изображение: {}]", alt.trim())
                    };
                    let start = self.offset();
                    self.put(&label, &with);
                    let end = self.offset();
                    self.links.push(Link {
                        start,
                        end,
                        target: image.url.clone(),
                    });
                }
                NodeValue::SoftBreak => self.put(" ", tags),
                NodeValue::LineBreak => self.put("\n", tags),
                _ => self.inlines(child, tags),
            }
        }
    }
}

/// Текст узла без разметки — для подписей картинок и ячеек таблицы.
fn plain_text<'n>(node: &'n comrak::nodes::AstNode<'n>) -> String {
    use comrak::nodes::NodeValue;

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
