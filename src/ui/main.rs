//! Окно Brevier.
//!
//! Тулкит здесь и только здесь. Ядро (`brevier`) про iced не знает ничего:
//! выбор рустового GUI ещё не закрыт спайком доступности, и смена тулкита
//! обязана стоить «переписать вид», а не «переписать всё».
//!
//! Рендерер — программный (`tiny-skia`), без wgpu: читалке нечего считать
//! на видеокарте, а меньше зависимостей — меньше поверхность для CVE.

use iced::keyboard::{self, Modifiers, key};
use iced::widget::scrollable::AbsoluteOffset;
use iced::widget::{button, column, container, markdown, operation, row, scrollable, text, text_input};
use iced::{Center, Element, Fill, Subscription, Task, Theme};

use brevier::address::{self, Address};
use brevier::{Document, History, UserAgent};

/// Ширина колонки в логических пикселях. Мера — около 65 знаков при кегле 17:
/// та ширина, на которой глаз находит начало следующей строки без усилия.
/// Считается по средней ширине знака примерно в половину кегля.
const MEASURE: f32 = 560.0;
const TEXT_SIZE: f32 = 17.0;
/// На сколько прокручивает стрелка и на сколько — страница.
const STEP: f32 = 60.0;
const PAGE: f32 = 520.0;

fn main() -> iced::Result {
    brevier::init_crypto();
    let start = std::env::args().nth(1);

    iced::application(
        move || Reader::new(start.clone()),
        Reader::update,
        Reader::view,
    )
    .title(Reader::title)
    .theme(Reader::theme)
    .subscription(Reader::subscription)
    .window_size((980.0, 760.0))
    .run()
}

struct Reader {
    /// Что напечатано в адресной строке прямо сейчас.
    input: String,
    history: History,
    page: Page,
    content: markdown::Content,
    dark: bool,
}

enum Page {
    /// Ещё ничего не открывали.
    Blank,
    Loading,
    Shown(String),
    /// Ошибка. Текст уже человеческий — он растёт из `brevier::Error`.
    Failed(String),
}

#[derive(Debug, Clone)]
enum Message {
    InputChanged(String),
    Go,
    Loaded(Box<Result<Document, String>>),
    LinkClicked(markdown::Uri),
    Back,
    Forward,
    Scroll(f32),
    ScrollTo(f32),
    FocusAddress,
    OpenInBrowser,
    ToggleTheme,
}

impl Reader {
    fn new(start: Option<String>) -> (Self, Task<Message>) {
        let reader = Self {
            input: start.clone().unwrap_or_default(),
            history: History::new(),
            page: Page::Blank,
            content: markdown::Content::new(),
            dark: true,
        };
        match start {
            Some(_) => {
                let mut reader = reader;
                let task = reader.go();
                (reader, task)
            }
            None => (reader, Task::none()),
        }
    }

    fn title(&self) -> String {
        match &self.page {
            Page::Shown(title) if !title.is_empty() => format!("{title} — Brevier"),
            _ => "Brevier".to_owned(),
        }
    }

    fn subscription(&self) -> Subscription<Message> {
        keyboard::listen().filter_map(on_key)
    }

    fn theme(&self) -> Theme {
        if self.dark { Theme::Nord } else { Theme::Light }
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::InputChanged(value) => {
                self.input = value;
                Task::none()
            }
            Message::Go => self.go(),
            Message::LinkClicked(uri) => {
                self.input = uri.to_string();
                self.go()
            }
            Message::Back => match self.history.back().cloned() {
                Some(address) => self.load(address),
                None => Task::none(),
            },
            Message::Forward => match self.history.forward().cloned() {
                Some(address) => self.load(address),
                None => Task::none(),
            },
            Message::OpenInBrowser => {
                if let Some(address) = self.history.current() {
                    open_in_system_browser(&address.display());
                }
                Task::none()
            }
            Message::Scroll(delta) => {
                operation::scroll_by(page_id(), AbsoluteOffset { x: 0.0, y: delta })
            }
            Message::ScrollTo(y) => {
                operation::scroll_to(page_id(), AbsoluteOffset { x: 0.0, y })
            }
            Message::FocusAddress => operation::focus(address_id()),
            Message::ToggleTheme => {
                self.dark = !self.dark;
                Task::none()
            }
            Message::Loaded(result) => {
                match *result {
                    Ok(document) => {
                        self.input = document.address.display();
                        self.content = markdown::Content::parse(&document.markdown);
                        self.page = Page::Shown(document.title);
                    }
                    Err(message) => {
                        self.content = markdown::Content::new();
                        self.page = Page::Failed(message);
                    }
                }
                Task::none()
            }
        }
    }

    /// Открыть то, что напечатано в адресной строке.
    fn go(&mut self) -> Task<Message> {
        match address::parse(&self.input) {
            Ok(address) => {
                self.history.visit(address.clone());
                self.load(address)
            }
            Err(e) => {
                self.page = Page::Failed(e.to_string());
                Task::none()
            }
        }
    }

    fn load(&mut self, address: Address) -> Task<Message> {
        self.page = Page::Loading;
        self.input = address.display();
        Task::perform(
            async move { brevier::open(&address, UserAgent::Honest).map_err(|e| e.to_string()) },
            |result| Message::Loaded(Box::new(result)),
        )
    }

    fn view(&self) -> Element<'_, Message> {
        let go = |label, message: Option<Message>| {
            let mut b = button(text(label).size(15)).padding([4, 10]);
            if let Some(message) = message {
                b = b.on_press(message);
            }
            b
        };

        let bar = row![
            go("←", self.history.can_go_back().then_some(Message::Back)),
            go("→", self.history.can_go_forward().then_some(Message::Forward)),
            text_input("адрес, gh:owner/repo или путь к .md", &self.input)
                .id(address_id())
                .on_input(Message::InputChanged)
                .on_submit(Message::Go)
                .padding([6, 10])
                .size(15),
            go(
                "в браузере",
                (!self.history.is_empty()).then_some(Message::OpenInBrowser)
            ),
            go(if self.dark { "светлая" } else { "тёмная" }, Some(Message::ToggleTheme)),
        ]
        .spacing(6)
        .padding(8)
        .align_y(Center);

        let body: Element<'_, Message> = match &self.page {
            Page::Blank => hint("Введите адрес и нажмите Enter."),
            Page::Loading => hint("Загружаю…"),
            Page::Failed(message) => failure(message),
            Page::Shown(_) => markdown::view(
                self.content.items(),
                markdown::Settings::with_text_size(TEXT_SIZE, self.theme()),
            )
            .map(Message::LinkClicked),
        };

        // Мера — около 65 знаков: ширина, на которой глаз находит начало
        // следующей строки без усилия. Колонка держится по центру окна.
        let page = container(container(body).width(Fill).max_width(MEASURE))
            .center_x(Fill)
            .padding([24, 16]);

        column![bar, scrollable(page).id(page_id()).height(Fill)].into()
    }
}

fn on_key(event: keyboard::Event) -> Option<Message> {
    match event {
        keyboard::Event::KeyPressed { key, modifiers, .. } => keys(&key, modifiers),
        _ => None,
    }
}

/// Клавиатура. Прокрутка живёт здесь, а не в самом `scrollable`: у iced
/// он на клавиши не отзывается вовсе.
fn keys(key: &key::Key, modifiers: Modifiers) -> Option<Message> {
    use key::Named;

    let named = match key.as_ref() {
        key::Key::Named(named) => named,
        key::Key::Character("l") if modifiers.command() => return Some(Message::FocusAddress),
        _ => return None,
    };

    match named {
        Named::ArrowLeft if modifiers.alt() => Some(Message::Back),
        Named::ArrowRight if modifiers.alt() => Some(Message::Forward),
        Named::ArrowDown => Some(Message::Scroll(STEP)),
        Named::ArrowUp => Some(Message::Scroll(-STEP)),
        Named::PageDown | Named::Space => Some(Message::Scroll(PAGE)),
        Named::PageUp => Some(Message::Scroll(-PAGE)),
        Named::Home => Some(Message::ScrollTo(0.0)),
        Named::End => Some(Message::ScrollTo(f32::MAX)),
        _ => None,
    }
}

fn hint(message: &str) -> Element<'_, Message> {
    column![text(message).size(15)].padding([40, 0]).into()
}

/// Сообщение об ошибке плюс выход: на открытом вебе передача страницы
/// системному браузеру — частый путь, а не крайний случай.
fn failure(message: &str) -> Element<'_, Message> {
    column![
        text("Не открылось").size(24),
        text(message.to_owned()).size(15),
        button(text("Открыть в системном браузере").size(15))
            .on_press(Message::OpenInBrowser)
            .padding([6, 12]),
    ]
    .spacing(14)
    .into()
}

/// Отдать адрес системному браузеру. Без внешних крейтов: это три команды,
/// а каждая зависимость в проекте про безопасность стоит дороже трёх строк.
fn page_id() -> iced::widget::Id {
    iced::widget::Id::new("page")
}

fn address_id() -> iced::widget::Id {
    iced::widget::Id::new("address")
}

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
