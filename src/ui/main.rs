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
use iced::widget::{
    button, column, container, markdown, operation, rich_text, row, scrollable, text, text_input,
};
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
    Failed(Failure),
}

/// Ошибка, переведённая с языка тракта на язык читателя.
///
/// `brevier::Error` писан для stderr и прогона корпуса: там нужен код возврата
/// и короткая английская строка. В окне нужно другое — что случилось, почему
/// и что теперь делать. Разные причины требуют разных ответов: сертификат
/// лечится установкой корня в систему, 403 не лечится ничем, а пустое
/// извлечение — повод сразу предложить системный браузер.
#[derive(Debug, Clone)]
struct Failure {
    headline: &'static str,
    detail: String,
    /// Стоит ли предлагать открыть страницу в системном браузере.
    offer_browser: bool,
}

fn describe(error: &brevier::Error) -> Failure {
    use brevier::Error;

    let failure = |headline, detail: String, offer_browser| Failure {
        headline,
        detail,
        offer_browser,
    };

    match error {
        Error::BadUrl(what) if what.trim().is_empty() => failure(
            "Пустой адрес",
            "Напечатайте адрес страницы, `gh:owner/repo` или путь к файлу `.md`."
                .to_owned(),
            false,
        ),
        Error::BadUrl(what) => failure(
            "Это не похоже на адрес",
            format!("Brevier не понял, что открывать: «{what}». Полный адрес выглядит так: https://example.com/статья."),
            false,
        ),
        Error::UnsupportedScheme(scheme) => failure(
            "Такие адреса Brevier не открывает",
            format!("Схема «{scheme}:» не поддерживается — Brevier ходит только по http и https."),
            true,
        ),
        // Проверку сертификата не обходим никогда, поэтому объясняем причину:
        // это не «сайт сломался», а нехватка корня в хранилище самой системы,
        // и лечится она установкой корня, а не флагом в читалке.
        Error::Network(e) if is_certificate_problem(&e.to_string()) => failure(
            "Сертификату сайта нет доверия",
            "Он подписан центром, которого нет в хранилище вашей операционной системы. Brevier доверяет тем же корням, что и вся система, и проверку не обходит. Если этот центр вам известен — поставьте его корень в систему."
                .to_owned(),
            true,
        ),
        Error::Network(e) => failure(
            "Не удалось соединиться",
            format!("{e}. Возможно, хост недоступен или нет сети."),
            true,
        ),
        Error::HttpStatus(401 | 403) => failure(
            "Сайт не пустил",
            "Страница закрыта для незалогиненных или отсечена защитой от ботов. Brevier не умеет логиниться — это осознанно."
                .to_owned(),
            true,
        ),
        Error::HttpStatus(404 | 410) => failure(
            "Страницы нет",
            "Сервер отвечает, что по этому адресу ничего не лежит.".to_owned(),
            true,
        ),
        Error::HttpStatus(429) => failure(
            "Слишком часто",
            "Сайт просит подождать: запросов с вашего адреса пришло больше, чем он готов принять."
                .to_owned(),
            true,
        ),
        Error::HttpStatus(code) if *code >= 500 => failure(
            "Сервер сайта отвечает ошибкой",
            format!("Код {code}. Это не у вас — попробуйте позже."),
            true,
        ),
        Error::HttpStatus(code) => failure(
            "Сервер ответил не тем",
            format!("Код {code}."),
            true,
        ),
        Error::UnsupportedContentType(kind) => failure(
            "Это не страница",
            format!("Сервер отдал «{kind}». Brevier читает html, markdown и простой текст; PDF, видео и картинки — работа для системного браузера."),
            true,
        ),
        Error::TooLarge(limit) => failure(
            "Страница слишком большая",
            format!("Тело ответа не влезло в предел {limit} байт."),
            true,
        ),
        Error::EmptyExtraction => failure(
            "Статьи на странице нет",
            "Так выглядят ленты, каталоги и сайты, которые собираются джаваскриптом. Brevier показывает статью или честно говорит, что её нет."
                .to_owned(),
            true,
        ),
        Error::Convert(e) => failure(
            "Не удалось разобрать страницу",
            format!("{e}"),
            true,
        ),
    }
}

/// У `ureq` причина отказа TLS не вынесена в тип — она приходит текстом
/// от `rustls`. Смотрим на текст: другого способа отличить недоверенный
/// сертификат от оборванного соединения сейчас нет.
fn is_certificate_problem(message: &str) -> bool {
    let message = message.to_lowercase();
    message.contains("certificate") || message.contains("unknownissuer")
}

#[derive(Debug, Clone)]
enum Message {
    InputChanged(String),
    Go,
    Loaded(Box<Result<Document, Failure>>),
    LinkClicked(markdown::Uri),
    Back,
    Forward,
    /// Клавиша прокрутки нажата — но сначала выясним, не набирают ли адрес.
    Scrolling(Scroll),
    /// Ответ на этот вопрос: прокручиваем, если курсор не в адресной строке.
    Scrolled(Scroll, bool),
    FocusAddress,
    Unfocus,
    OpenInBrowser,
    OpenExternal(String),
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
            // После Enter фокус уходит со строки: дальше человек читает,
            // а не правит адрес, и клавиши должны листать страницу.
            Message::Go => Task::batch([unfocus(), self.go()]),
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
            Message::OpenExternal(target) => {
                open_in_system_browser(&target);
                Task::none()
            }
            Message::OpenInBrowser => {
                if let Some(address) = self.history.current() {
                    open_in_system_browser(&address.display());
                }
                Task::none()
            }
            // Подписка на клавиатуру приходит мимо виджетов и не знает, кто
            // в фокусе. Поэтому не прокручиваем сразу, а спрашиваем: если
            // курсор в адресной строке, пробел и стрелки принадлежат набору.
            Message::Scrolling(scroll) => operation::is_focused(address_id())
                .map(move |typing| Message::Scrolled(scroll, typing)),
            Message::Scrolled(scroll, typing) => {
                if typing {
                    Task::none()
                } else {
                    scroll.task()
                }
            }
            Message::FocusAddress => Task::batch([
                operation::focus(address_id()),
                operation::select_all(address_id()),
            ]),
            Message::Unfocus => unfocus(),
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
                    Err(failure) => {
                        self.content = markdown::Content::new();
                        self.page = Page::Failed(failure);
                    }
                }
                // Новая страница начинается сверху. Без этого переход по ссылке
                // открывает статью с той же высоты, на которой бросили прошлую.
                operation::scroll_to(page_id(), AbsoluteOffset { x: 0.0, y: 0.0 })
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
                self.page = Page::Failed(describe(&e));
                Task::none()
            }
        }
    }

    fn load(&mut self, address: Address) -> Task<Message> {
        self.page = Page::Loading;
        self.input = address.display();
        Task::perform(
            async move {
                brevier::open(&address, UserAgent::Honest).map_err(|e| describe(&e))
            },
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
            go(
                if self.dark { "☀" } else { "☾" },
                Some(Message::ToggleTheme)
            ),
        ]
        .spacing(6)
        .padding(8)
        .align_y(Center);

        let body: Element<'_, Message> = match &self.page {
            Page::Blank => hint("Введите адрес и нажмите Enter."),
            Page::Loading => hint("Загружаю…"),
            Page::Failed(problem) => failed(problem),
            Page::Shown(_) => markdown::view_with(
                self.content.items(),
                markdown::Settings::with_text_size(TEXT_SIZE, self.theme()),
                &Reading,
            ),
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
        Named::ArrowDown => Some(Message::Scrolling(Scroll::By(STEP))),
        Named::ArrowUp => Some(Message::Scrolling(Scroll::By(-STEP))),
        Named::PageDown | Named::Space => Some(Message::Scrolling(Scroll::By(PAGE))),
        Named::PageUp => Some(Message::Scrolling(Scroll::By(-PAGE))),
        Named::Home => Some(Message::Scrolling(Scroll::To(0.0))),
        Named::End => Some(Message::Scrolling(Scroll::To(f32::MAX))),
        Named::Escape => Some(Message::Unfocus),
        _ => None,
    }
}

fn hint(message: &str) -> Element<'_, Message> {
    column![text(message).size(15)].padding([40, 0]).into()
}

/// Сообщение об ошибке плюс выход: на открытом вебе передача страницы
/// системному браузеру — частый путь, а не крайний случай.
fn failed(problem: &Failure) -> Element<'_, Message> {
    let mut block = column![
        text(problem.headline).size(26),
        text(&problem.detail).size(TEXT_SIZE),
    ]
    .spacing(12);

    if problem.offer_browser {
        block = block.push(
            button(text("Открыть в системном браузере").size(TEXT_SIZE))
                .on_press(Message::OpenInBrowser)
                .padding([7, 14]),
        );
    }

    block.padding([40, 0]).into()
}

/// Отдать адрес системному браузеру. Без внешних крейтов: это три команды,
/// а каждая зависимость в проекте про безопасность стоит дороже трёх строк.
/// Свой вид для markdown. От умолчания отличается одним: картинками.
struct Reading;

impl<'a> markdown::Viewer<'a, Message> for Reading {
    fn on_link_click(url: markdown::Uri) -> Message {
        Message::LinkClicked(url)
    }

    /// Картинки не грузим — они единственная серьёзная поверхность атаки
    /// после отказа от JS, и по плану включаются на M4: по клику
    /// и только same-origin. Но молча пропадать текст не должен: показываем,
    /// что здесь была картинка, с её подписью, и даём открыть её в системном
    /// браузере — тем же выходом, что и для страниц, которые мы не тянем.
    fn image(
        &self,
        settings: markdown::Settings,
        url: &'a markdown::Uri,
        _title: &'a str,
        alt: &markdown::Text,
    ) -> Element<'a, Message> {
        let caption = rich_text(alt.spans(settings.style)).on_link_click(Self::on_link_click);
        let target = url.to_string();

        container(
            column![
                button(text("изображение — открыть в браузере").size(TEXT_SIZE * 0.8))
                    .on_press(Message::OpenExternal(target))
                    .padding([3, 8]),
                caption,
            ]
            .spacing(6),
        )
        .padding(10)
        .width(Fill)
        .into()
    }
}

/// Куда прокрутить страницу.
#[derive(Debug, Clone, Copy)]
enum Scroll {
    By(f32),
    To(f32),
}

impl Scroll {
    fn task(self) -> Task<Message> {
        let offset = |y| AbsoluteOffset { x: 0.0, y };
        match self {
            Scroll::By(delta) => operation::scroll_by(page_id(), offset(delta)),
            Scroll::To(y) => operation::scroll_to(page_id(), offset(y)),
        }
    }
}

/// Снять фокус со всего. Отдельной операции для этого нет, но `focus`
/// снимает фокус со всех виджетов, кроме указанного, — наводим его
/// на заведомо несуществующий.
fn unfocus() -> Task<Message> {
    operation::focus(iced::widget::Id::new("brevier-nowhere"))
}

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

#[cfg(test)]
mod tests {
    use super::*;

    fn named(key: key::Named) -> key::Key {
        key::Key::Named(key)
    }

    #[test]
    fn scroll_keys_ask_before_scrolling() {
        // Прокрутка не решается на месте: сначала вопрос про фокус.
        for key in [key::Named::Space, key::Named::PageDown, key::Named::ArrowDown] {
            assert!(
                matches!(
                    keys(&named(key), Modifiers::default()),
                    Some(Message::Scrolling(_))
                ),
                "{key:?} должна проситься на прокрутку"
            );
        }
    }

    #[test]
    fn typing_in_the_address_bar_wins_over_scrolling() {
        let scroll = Scroll::By(PAGE);
        // Тот же ответ, но с курсором в адресной строке — прокрутки нет.
        let mut reader = Reader::new(None).0;
        let _ = reader.update(Message::Scrolled(scroll, true));
        assert!(matches!(reader.page, Page::Blank), "состояние не должно меняться");
    }

    #[test]
    fn horizontal_arrows_belong_to_the_text_field() {
        // Без Alt они двигают курсор по адресу, а не листают страницу.
        assert!(keys(&named(key::Named::ArrowLeft), Modifiers::default()).is_none());
        assert!(keys(&named(key::Named::ArrowRight), Modifiers::default()).is_none());
    }

    #[test]
    fn alt_arrows_walk_the_history() {
        assert!(matches!(
            keys(&named(key::Named::ArrowLeft), Modifiers::ALT),
            Some(Message::Back)
        ));
        assert!(matches!(
            keys(&named(key::Named::ArrowRight), Modifiers::ALT),
            Some(Message::Forward)
        ));
    }

    #[test]
    fn escape_lets_go_of_the_address_bar() {
        assert!(matches!(
            keys(&named(key::Named::Escape), Modifiers::default()),
            Some(Message::Unfocus)
        ));
    }

    #[test]
    fn plain_letters_are_not_shortcuts() {
        assert!(keys(&key::Key::Character("l".into()), Modifiers::default()).is_none());
        assert!(matches!(
            keys(&key::Key::Character("l".into()), Modifiers::COMMAND),
            Some(Message::FocusAddress)
        ));
    }
}
