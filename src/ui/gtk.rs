//! Окно Brevier на GTK4.
//!
//! Статья рисуется одним `GtkTextView`, а не набором виджетов на абзац:
//! выделение должно идти через весь документ, а не обрываться на границе
//! абзаца. Тем же решением бесплатно приходят копирование, метки для точного
//! оглавления и доступность через AT-SPI.
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
use brevier::outline::{HEADINGS, LINE_HEIGHT, MEASURE, TEXT_SIZE};
use brevier::{Document, History, UserAgent};

const APP_ID: &str = "dev.brevier.Brevier";
const BODY_FAMILY: &str = "PT Serif";
const MONO_FAMILY: &str = "PT Mono";
/// Жирность в единицах Pango: свойство тега — целое, а не перечисление.
const BOLD: i32 = 700;

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
        build(app, start.first().cloned());
        glib::ExitCode::SUCCESS
    });

    app.run()
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

/// Что читатель сейчас видит.
struct Reader {
    history: History,
    /// Ссылки в тексте: где начинается, где кончается, куда ведёт.
    links: Vec<Link>,
    /// Номер загрузки. Ответ брошенной страницы отличаем по нему:
    /// отменить синхронный `ureq` нечем, но и слушать его уже незачем.
    generation: u64,
}

struct Link {
    start: i32,
    end: i32,
    target: String,
}

fn build(app: &Application, start: Option<String>) {
    let reader = Rc::new(RefCell::new(Reader {
        history: History::new(),
        links: Vec::new(),
        generation: 0,
    }));

    let view = gtk::TextView::builder()
        .editable(false)
        .cursor_visible(false)
        .wrap_mode(gtk::WrapMode::Word)
        .halign(gtk::Align::Center)
        .top_margin(28)
        .bottom_margin(80)
        .build();
    tags(&view.buffer());
    // Мера задана в кеглях, а кегль в GTK — пункты, не пиксели. Переводим
    // по разрешению, которым рисует Pango, иначе на разных экранах
    // в строке окажется разное число знаков.
    let dpi = gtk::Settings::for_display(&gtk::gdk::Display::default().unwrap())
        .gtk_xft_dpi();
    // Настройка хранится в 1024-х долях точки; 0 или -1 значит «не задано».
    let dpi = if dpi > 0 { f64::from(dpi) / 1024.0 } else { 96.0 };
    let measure = f64::from(MEASURE) * dpi / 72.0;
    view.set_width_request(measure.round() as i32);

    let scroller = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vexpand(true)
        .child(&view)
        .build();

    let address_entry = gtk::Entry::builder()
        .placeholder_text("адрес, gh:owner/repo или путь к .md")
        .hexpand(true)
        .build();

    let back = gtk::Button::from_icon_name("go-previous-symbolic");
    let forward = gtk::Button::from_icon_name("go-next-symbolic");
    back.set_sensitive(false);
    forward.set_sensitive(false);

    let header = gtk::HeaderBar::builder().build();
    header.pack_start(&back);
    header.pack_start(&forward);
    header.set_title_widget(Some(&address_entry));

    let window = ApplicationWindow::builder()
        .application(app)
        .title("Brevier")
        .default_width(980)
        .default_height(760)
        .child(&scroller)
        .build();
    window.set_titlebar(Some(&header));

    // ── переходы
    let open = {
        let reader = reader.clone();
        let view = view.clone();
        let entry = address_entry.clone();
        let window = window.clone();
        let back = back.clone();
        let forward = forward.clone();
        Rc::new(move |address: Address, remember: bool| {
            if remember {
                reader.borrow_mut().history.visit(address.clone());
            }
            entry.set_text(&address.display());
            back.set_sensitive(reader.borrow().history.can_go_back());
            forward.set_sensitive(reader.borrow().history.can_go_forward());

            let generation = {
                let mut state = reader.borrow_mut();
                state.generation += 1;
                state.generation
            };
            show_message(&view, "Загружаю…", "");

            let reader = reader.clone();
            let view = view.clone();
            let window = window.clone();
            glib::spawn_future_local(async move {
                let loaded = gio::spawn_blocking(move || {
                    brevier::open(&address, UserAgent::Honest)
                })
                .await;

                // Читатель уже ушёл на другую страницу — ответ никому не нужен.
                if reader.borrow().generation != generation {
                    return;
                }
                match loaded {
                    Ok(Ok(document)) => {
                        window.set_title(Some(&format!("{} — Brevier", document.title)));
                        let links = render(&view, &document);
                        reader.borrow_mut().links = links;
                    }
                    Ok(Err(error)) => {
                        let problem = describe(&error);
                        window.set_title(Some("Brevier"));
                        show_message(&view, problem.headline, &problem.detail);
                        reader.borrow_mut().links.clear();
                    }
                    Err(_) => show_message(&view, "Загрузка сорвалась", ""),
                }
            });
        })
    };

    {
        let open = open.clone();
        let view = view.clone();
        address_entry.connect_activate(move |entry| {
            match address::parse(&entry.text()) {
                Ok(address) => open(address, true),
                Err(error) => {
                    let problem = describe(&error);
                    show_message(&view, problem.headline, &problem.detail);
                }
            }
        });
    }
    {
        let open = open.clone();
        let reader = reader.clone();
        back.connect_clicked(move |_| {
            let previous = reader.borrow_mut().history.back().cloned();
            if let Some(address) = previous {
                open(address, false);
            }
        });
    }
    {
        let open = open.clone();
        let reader = reader.clone();
        forward.connect_clicked(move |_| {
            let next = reader.borrow_mut().history.forward().cloned();
            if let Some(address) = next {
                open(address, false);
            }
        });
    }

    // ── клик по ссылке
    let click = gtk::GestureClick::new();
    {
        let reader = reader.clone();
        let open = open.clone();
        let view = view.clone();
        click.connect_released(move |_, _, x, y| {
            let Some(target) = link_at(&view, &reader.borrow().links, x, y) else {
                return;
            };
            if let Ok(address) = address::parse(&target) {
                open(address, true);
            }
        });
    }
    view.add_controller(click);

    window.present();

    if let Some(start) = start {
        if let Ok(address) = address::parse(&start) {
            open(address, true);
        }
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
}

/// Теги — вся типографика статьи. Кегли и интерлиньяж те же, что были
/// в прошлом интерфейсе: они живут в ядре и от тулкита не зависят.
fn tags(buffer: &gtk::TextBuffer) {
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
    buffer.create_tag(
        Some("link"),
        &[("underline", &pango::Underline::Single), ("foreground", &"#88c0d0")],
    );
    buffer.create_tag(Some("dim"), &[("foreground", &"#8b98a5")]);
}

/// Разложить статью по буферу. Возвращает ссылки с их местами в тексте —
/// по ним потом опознаётся клик.
fn render(view: &gtk::TextView, document: &Document) -> Vec<Link> {
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
    let mut writer = Writer {
        buffer: &buffer,
        links: &mut links,
    };

    for node in root.children() {
        writer.block(node, &[]);
    }

    // Возврат наверх: новая статья начинается сначала. Через `idle`,
    // потому что в момент вставки текста у виджета ещё нет раскладки
    // и прокручивать ему некуда.
    buffer.place_cursor(&buffer.start_iter());
    let view = view.clone();
    glib::idle_add_local_once(move || {
        let mut start = view.buffer().start_iter();
        view.scroll_to_iter(&mut start, 0.0, true, 0.0, 0.0);
    });
    links
}

struct Writer<'a> {
    buffer: &'a gtk::TextBuffer,
    links: &'a mut Vec<Link>,
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

    fn block<'n>(&mut self, node: &'n comrak::nodes::AstNode<'n>, outer: &[&str]) {
        use comrak::nodes::NodeValue;

        match &node.data.borrow().value {
            NodeValue::Heading(heading) => {
                let level = usize::from(heading.level).clamp(1, 6);
                let name = format!("h{level}");
                let mut tags = outer.to_vec();
                tags.push(&name);
                self.inlines(node, &tags);
                self.put("\n", &[]);
            }
            NodeValue::Paragraph => {
                let mut tags = outer.to_vec();
                tags.push("body");
                self.inlines(node, &tags);
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
