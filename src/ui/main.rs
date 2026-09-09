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
    button, column, container, markdown, mouse_area, operation, rich_text, row, scrollable, stack,
    text, text_input,
};
use iced::{Center, Element, Fill, Point, Subscription, Task, Theme};

use brevier::address::{self, Address};
use brevier::{Document, History, UserAgent};

/// Типографика. Числа связаны между собой, поэтому и живут вместе: мера
/// задана в кеглях, а не в пикселях, чтобы при смене размера шрифта строка
/// оставалась той же длины в знаках. Тридцать три кегля — это около
/// 65 знаков при средней ширине знака примерно в половину кегля.
const TEXT_SIZE: f32 = 17.0;
const MEASURE_IN_EMS: f32 = 33.0;
const MEASURE: f32 = TEXT_SIZE * MEASURE_IN_EMS;

/// Межстрочный интервал. Умолчание iced (1.3) собрано для интерфейса,
/// где строки короткие; на мере в 65 знаков глаз на обратном ходе
/// соскакивает на соседнюю строку. Полтора с небольшим — книжная норма.
const LINE_HEIGHT: f32 = 1.55;
/// В заголовке строки короткие, и полуторный интервал разваливает его
/// на отдельные строки. Плотнее.
const HEADING_LINE_HEIGHT: f32 = 1.15;

/// Шкала заголовков в долях кегля. Умолчание iced — вдвое на первом уровне
/// и минус четверть на каждом следующем; на экране это даёт заголовок
/// в 34 пункта, который спорит с текстом, а не ведёт к нему.
const HEADINGS: [f32; 6] = [1.75, 1.45, 1.28, 1.14, 1.05, 1.0];

/// Гарнитуры едут в комплекте, а не берутся из системы. Продукт обещает,
/// что типографику задаёт читатель, а не сайт; если шрифт выбирает
/// операционная система, обещание не выполняется ни на одной из трёх.
/// PT Serif сделан ParaType под кириллицу с латиницей и предназначен
/// для чтения, PT Mono — парный к нему. Обе под OFL, лицензии рядом
/// с файлами. Смена гарнитуры — это две константы и файлы в `assets/fonts`.
const BODY_FAMILY: &str = "PT Serif";
const MONO_FAMILY: &str = "PT Mono";

/// Оглавление показываем, только если оно что-то даёт.
const MIN_HEADINGS: usize = 3;
const TOC_WIDTH: f32 = 240.0;
/// Ниже этой высоты оглавление не нужно: страница и так вся под рукой.
const MIN_DOC_HEIGHT: f32 = 1800.0;
/// Через сколько высоты ставить веху, когда заголовков в статье нет.
const WAYPOINT_EVERY: f32 = 900.0;
const MAX_WAYPOINTS: usize = 14;
/// На сколько прокручивает стрелка и на сколько — страница.
const STEP: f32 = 60.0;
const PAGE: f32 = 520.0;

fn main() -> iced::Result {
    brevier::init_crypto();
    // Каждый адрес из командной строки — своя вкладка.
    let start: Vec<String> = std::env::args().skip(1).collect();

    iced::application(
        move || Reader::new(start.clone()),
        Reader::update,
        Reader::view,
    )
    .font(include_bytes!("../../assets/fonts/PTSerif-Regular.ttf").as_slice())
    .font(include_bytes!("../../assets/fonts/PTSerif-Italic.ttf").as_slice())
    .font(include_bytes!("../../assets/fonts/PTSerif-Bold.ttf").as_slice())
    .font(include_bytes!("../../assets/fonts/PTSerif-BoldItalic.ttf").as_slice())
    .font(include_bytes!("../../assets/fonts/PTMono-Regular.ttf").as_slice())
    .default_font(iced::Font::with_name(BODY_FAMILY))
    .title(Reader::title)
    .theme(Reader::theme)
    .subscription(Reader::subscription)
    .window_size((980.0, 760.0))
    .run()
}

struct Reader {
    tabs: Vec<Tab>,
    active: usize,
    /// Счётчик имён вкладок. Ответ загрузки адресуется по нему, а не
    /// по месту в списке: пока страница едет, соседнюю вкладку могли закрыть.
    next_id: u64,
    /// Показывать ли оглавление. Решение читателя, общее для всех вкладок.
    show_toc: bool,
    dark: bool,
    /// Зажатые клавиши. Клик по ссылке приходит без них — виджет
    /// сообщает только адрес, — поэтому состояние держим сами.
    modifiers: Modifiers,
    /// Где стоит курсор и где открыто контекстное меню.
    cursor: Point,
    menu: Option<Point>,
}

/// Вкладка: своя статья, своя история и своё место в тексте.
///
/// Место хранится здесь, а не в самом `scrollable`: виджет один на все
/// вкладки, и без сохранения переключение бросало бы читателя туда, где
/// он остановился в прошлой.
struct Tab {
    id: u64,
    /// Что напечатано в адресной строке этой вкладки.
    input: String,
    history: History,
    page: Page,
    content: markdown::Content,
    outline: Vec<Entry>,
    /// Текст статьи как есть — для «скопировать статью».
    source: String,
    /// Ручка загрузки. У каждой вкладки своя: фоновая догружается сама
    /// и переключение на соседнюю её не отменяет.
    loading: Option<iced::task::Handle>,
    scroll: f32,
}

impl Tab {
    fn new(id: u64, input: String) -> Self {
        Self {
            id,
            input,
            history: History::new(),
            page: Page::Blank,
            content: markdown::Content::new(),
            outline: Vec::new(),
            source: String::new(),
            loading: None,
            scroll: 0.0,
        }
    }

    /// Подпись на корешке вкладки.
    fn label(&self) -> String {
        match &self.page {
            Page::Shown(title) if !title.trim().is_empty() => title.clone(),
            Page::Loading => "Загружаю…".to_owned(),
            Page::Failed(problem) => problem.headline.to_owned(),
            _ => match self.history.current() {
                Some(address) => address.display(),
                None => "Новая вкладка".to_owned(),
            },
        }
    }
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
    /// Страница доехала. Имя вкладки — потому что доехать она могла
    /// и в фоновую, пока читатель смотрит соседнюю.
    Loaded(u64, Box<Result<Document, Failure>>),
    NewTab,
    SelectTab(usize),
    CloseTab(usize),
    /// То же, но по клавише: раскладка не знает, какая вкладка сейчас открыта.
    CloseActiveTab,
    /// Следующая вкладка (`true`) или предыдущая.
    CycleTab(bool),
    /// Читатель прокрутил страницу — запоминаем место для этой вкладки.
    ScrollChanged(f32),
    ModifiersChanged(Modifiers),
    CursorMoved(Point),
    OpenMenu,
    CloseMenu,
    Copy(String),
    LinkClicked(markdown::Uri),
    Back,
    Forward,
    /// Клавиша прокрутки нажата — но сначала выясним, не набирают ли адрес.
    Scrolling(Scroll),
    /// Ответ на этот вопрос: прокручиваем, если курсор не в адресной строке.
    Scrolled(Scroll, bool),
    FocusAddress,
    Unfocus,
    JumpTo(f32),
    OpenInBrowser,
    ToggleContents,
    OpenExternal(String),
    ToggleTheme,
}

impl Reader {
    fn new(start: Vec<String>) -> (Self, Task<Message>) {
        let addresses = if start.is_empty() {
            vec![String::new()]
        } else {
            start
        };
        let tabs: Vec<Tab> = addresses
            .iter()
            .enumerate()
            .map(|(index, address)| Tab::new(index as u64, address.clone()))
            .collect();
        let next_id = tabs.len() as u64;

        let mut reader = Self {
            tabs,
            active: 0,
            next_id,
            show_toc: true,
            dark: true,
            modifiers: Modifiers::default(),
            cursor: Point::ORIGIN,
            menu: None,
        };

        // Грузим все сразу: фоновая вкладка приезжает сама, для этого
        // у каждой своя ручка загрузки.
        let mut tasks = Vec::new();
        for index in 0..reader.tabs.len() {
            if reader.tabs[index].input.is_empty() {
                continue;
            }
            reader.active = index;
            tasks.push(reader.go());
        }
        reader.active = 0;

        (reader, Task::batch(tasks))
    }

    fn tab(&self) -> &Tab {
        &self.tabs[self.active]
    }

    fn tab_mut(&mut self) -> &mut Tab {
        &mut self.tabs[self.active]
    }

    fn title(&self) -> String {
        match &self.tab().page {
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
                self.tab_mut().input = value;
                Task::none()
            }
            // После Enter фокус уходит со строки: дальше человек читает,
            // а не правит адрес, и клавиши должны листать страницу.
            Message::Go => Task::batch([unfocus(), self.go()]),
            Message::LinkClicked(uri) => {
                let target = uri.to_string();
                // Как в браузерах: Ctrl открывает в новой вкладке, Shift
                // кладёт адрес в буфер, обычный клик уводит на страницу.
                if self.modifiers.command() {
                    self.open_in_new_tab(target)
                } else if self.modifiers.shift() {
                    iced::clipboard::write(target)
                } else {
                    self.tab_mut().input = target;
                    self.go()
                }
            }
            Message::ModifiersChanged(modifiers) => {
                self.modifiers = modifiers;
                Task::none()
            }
            Message::CursorMoved(point) => {
                self.cursor = point;
                Task::none()
            }
            Message::OpenMenu => {
                self.menu = Some(self.cursor);
                Task::none()
            }
            Message::CloseMenu => {
                self.menu = None;
                Task::none()
            }
            Message::Copy(text) => {
                self.menu = None;
                iced::clipboard::write(text)
            }
            Message::Back => match self.tab_mut().history.back().cloned() {
                Some(address) => self.load(address),
                None => Task::none(),
            },
            Message::Forward => match self.tab_mut().history.forward().cloned() {
                Some(address) => self.load(address),
                None => Task::none(),
            },
            Message::OpenExternal(target) => {
                open_in_system_browser(&target);
                Task::none()
            }
            Message::OpenInBrowser => {
                if let Some(address) = self.tab().history.current() {
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
            Message::ScrollChanged(y) => {
                self.tab_mut().scroll = y;
                Task::none()
            }
            Message::FocusAddress => Task::batch([
                operation::focus(address_id()),
                operation::select_all(address_id()),
            ]),
            Message::Unfocus => unfocus(),
            Message::JumpTo(at) => {
                operation::snap_to(page_id(), scrollable::RelativeOffset { x: 0.0, y: at })
            }
            Message::ToggleContents => {
                self.show_toc = !self.show_toc;
                Task::none()
            }
            Message::ToggleTheme => {
                self.dark = !self.dark;
                Task::none()
            }
            Message::NewTab => {
                let id = self.next_id;
                self.next_id += 1;
                self.tabs.push(Tab::new(id, String::new()));
                self.active = self.tabs.len() - 1;
                // Новая вкладка пуста, читать в ней нечего — курсор в адрес.
                Task::batch([
                    operation::focus(address_id()),
                    operation::select_all(address_id()),
                ])
            }
            Message::SelectTab(index) => self.switch_to(index),
            Message::CycleTab(forward) => {
                let count = self.tabs.len();
                let next = if forward {
                    (self.active + 1) % count
                } else {
                    (self.active + count - 1) % count
                };
                self.switch_to(next)
            }
            Message::CloseActiveTab => {
                let index = self.active;
                self.update(Message::CloseTab(index))
            }
            Message::CloseTab(index) => {
                if index >= self.tabs.len() {
                    return Task::none();
                }
                let mut closed = self.tabs.remove(index);
                // Закрыли — значит и грузить незачем.
                if let Some(handle) = closed.loading.take() {
                    handle.abort();
                }
                // Последняя вкладка не закрывается в пустоту: окно без
                // единой вкладки показывать нечем, поэтому заводим чистую.
                if self.tabs.is_empty() {
                    let id = self.next_id;
                    self.next_id += 1;
                    self.tabs.push(Tab::new(id, String::new()));
                }
                let landing = if index <= self.active {
                    self.active.saturating_sub(1)
                } else {
                    self.active
                };
                self.switch_to(landing)
            }
            Message::Loaded(id, result) => {
                let active = self.tab().id;
                let Some(tab) = self.tabs.iter_mut().find(|tab| tab.id == id) else {
                    // Вкладку закрыли, пока страница ехала.
                    return Task::none();
                };
                tab.loading = None;
                match *result {
                    Ok(document) => {
                        tab.input = document.address.display();
                        tab.outline = outline(&document.markdown);
                        tab.content = markdown::Content::parse(&document.markdown);
                        tab.source = document.markdown;
                        tab.page = Page::Shown(document.title);
                    }
                    Err(failure) => {
                        tab.content = markdown::Content::new();
                        tab.outline = Vec::new();
                        tab.source = String::new();
                        tab.page = Page::Failed(failure);
                    }
                }
                tab.scroll = 0.0;

                // Новая страница начинается сверху — но только если читатель
                // на неё смотрит. Догрузка фоновой вкладки чужую прокрутку
                // трогать не смеет.
                if active == id {
                    operation::scroll_to(page_id(), AbsoluteOffset { x: 0.0, y: 0.0 })
                } else {
                    Task::none()
                }
            }
        }
    }

    /// Открыть адрес в новой вкладке, не уводя читателя с текущей.
    fn open_in_new_tab(&mut self, target: String) -> Task<Message> {
        let id = self.next_id;
        self.next_id += 1;
        self.tabs.push(Tab::new(id, target));

        let opened = self.tabs.len() - 1;
        let here = self.active;
        self.active = opened;
        let task = self.go();
        // Ctrl+клик открывает вкладку в фоне: читатель остался в статье,
        // а не улетел в ссылку, которую только присмотрел.
        self.active = here;
        task
    }

    /// Переключиться на вкладку и вернуть читателя туда, где он её бросил.
    fn switch_to(&mut self, index: usize) -> Task<Message> {
        if index >= self.tabs.len() {
            return Task::none();
        }
        self.active = index;
        let y = self.tab().scroll;
        operation::scroll_to(page_id(), AbsoluteOffset { x: 0.0, y })
    }

    /// Открыть то, что напечатано в адресной строке.
    fn go(&mut self) -> Task<Message> {
        match address::parse(&self.tab().input) {
            Ok(address) => {
                self.tab_mut().history.visit(address.clone());
                self.load(address)
            }
            Err(e) => {
                self.tab_mut().page = Page::Failed(describe(&e));
                Task::none()
            }
        }
    }

    fn load(&mut self, address: Address) -> Task<Message> {
        // Уходя со страницы, бросаем её загрузку: иначе медленный сайт
        // догоняет читателя и подменяет уже открытую статью своей.
        self.abort_loading();

        let id = self.tab().id;
        let tab = self.tab_mut();
        tab.page = Page::Loading;
        tab.input = address.display();

        let (task, handle) = Task::perform(
            async move {
                brevier::open(&address, UserAgent::Honest).map_err(|e| describe(&e))
            },
            move |result| Message::Loaded(id, Box::new(result)),
        )
        .abortable();

        self.tab_mut().loading = Some(handle);
        task
    }

    fn abort_loading(&mut self) {
        if let Some(handle) = self.tab_mut().loading.take() {
            handle.abort();
        }
    }

    fn view(&self) -> Element<'_, Message> {
        let go = |label, message: Option<Message>| {
            let mut b = button(text(label).size(15)).padding([4, 10]);
            if let Some(message) = message {
                b = b.on_press(message);
            }
            b
        };
        let tab = self.tab();

        let bar = row![
            go("←", tab.history.can_go_back().then_some(Message::Back)),
            go("→", tab.history.can_go_forward().then_some(Message::Forward)),
            text_input("адрес, gh:owner/repo или путь к .md", &tab.input)
                .id(address_id())
                .on_input(Message::InputChanged)
                .on_submit(Message::Go)
                .padding([6, 10])
                .size(15),
            go("+", Some(Message::NewTab)),
            go(
                "☰",
                (!tab.outline.is_empty()).then_some(Message::ToggleContents)
            ),
            go(
                if self.dark { "☀" } else { "☾" },
                Some(Message::ToggleTheme)
            ),
        ]
        .spacing(6)
        .padding(8)
        .align_y(Center);

        let body: Element<'_, Message> = match &tab.page {
            Page::Blank => hint("Введите адрес и нажмите Enter."),
            Page::Loading => hint("Загружаю…"),
            Page::Failed(problem) => failed(problem),
            Page::Shown(_) => markdown::view_with(
                tab.content.items(),
                markdown_settings(&self.theme()),
                &Reading,
            ),
        };

        // Мера — около 65 знаков: ширина, на которой глаз находит начало
        // следующей строки без усилия. Колонка держится по центру окна.
        let page = container(container(body).width(Fill).max_width(MEASURE))
            .center_x(Fill)
            .padding([24, 16]);

        let reading = scrollable(page)
            .id(page_id())
            .height(Fill)
            .on_scroll(|viewport| Message::ScrollChanged(viewport.absolute_offset().y));

        let reading: Element<'_, Message> = if self.show_toc && !tab.outline.is_empty() {
            row![reading, contents(&tab.outline)].into()
        } else {
            reading.into()
        };

        // Правая кнопка открывает меню, левая закрывает. `mouse_area`
        // только смотрит: клики по ссылкам под ним работают как работали.
        let reading = mouse_area(reading)
            .on_move(Message::CursorMoved)
            .on_right_press(Message::OpenMenu)
            .on_press(Message::CloseMenu);

        let reading: Element<'_, Message> = match self.menu {
            Some(at) => stack![reading, menu(at, tab)].into(),
            None => reading.into(),
        };

        // Корешки показываем только со второй вкладки: на одной статье
        // полоса — пустой шум, а «+» и Ctrl+T есть всегда.
        if self.tabs.len() > 1 {
            column![strip(&self.tabs, self.active), bar, reading].into()
        } else {
            column![bar, reading].into()
        }
    }
}

/// Контекстное меню. Своего виджета для него в iced нет, поэтому кладём
/// обычный столбец кнопок поверх страницы через `stack` и сдвигаем отступом
/// к месту клика.
///
/// Пункты только страничные, и это не лень: «скопировать ссылку»
/// и «скопировать выделенное» сделать нечем. `rich_text` не отдаёт наружу,
/// какая ссылка под курсором (`hovered_link` — приватное состояние виджета),
/// а выделения текста в iced 0.14 нет вовсе. Ссылка копируется Shift+кликом,
/// это работает.
fn menu(at: Point, tab: &Tab) -> Element<'_, Message> {
    let address = tab
        .history
        .current()
        .map(brevier::Address::display)
        .unwrap_or_default();

    let item = |label: &'static str, message: Option<Message>| {
        let mut b = button(text(label).size(TEXT_SIZE * 0.85))
            .width(Fill)
            .padding([6, 12])
            .style(button::text);
        if let Some(message) = message {
            b = b.on_press(message);
        }
        b
    };

    let items = column![
        item(
            "Копировать адрес",
            (!address.is_empty()).then(|| Message::Copy(address.clone()))
        ),
        item(
            "Копировать статью",
            (!tab.source.is_empty()).then(|| Message::Copy(tab.source.clone()))
        ),
        item(
            "Открыть в системном браузере",
            (!address.is_empty()).then_some(Message::OpenInBrowser)
        ),
    ]
    .width(MENU_WIDTH);

    container(container(items).style(container::rounded_box).padding(4))
        .padding(iced::Padding {
            top: at.y,
            left: at.x,
            ..iced::Padding::ZERO
        })
        .into()
}

const MENU_WIDTH: f32 = 260.0;

/// Сколько знаков влезает на корешок. Обрезаем сами: запрет переноса
/// не мешает длинной подписи вылезти за кнопку и налезть на крестик.
const TAB_LABEL: usize = 24;

/// Обрезать подпись по границе знака, с многоточием.
fn clip(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_owned();
    }
    let mut out: String = text.chars().take(max.saturating_sub(1)).collect();
    out = out.trim_end().to_owned();
    out.push('…');
    out
}

/// Полоса корешков.
fn strip(tabs: &[Tab], active: usize) -> Element<'_, Message> {
    let mut list = row![].spacing(4);

    for (index, tab) in tabs.iter().enumerate() {
        let style = if index == active {
            button::primary
        } else {
            button::secondary
        };
        list = list.push(
            row![
                button(
                    text(clip(&tab.label(), TAB_LABEL))
                        .size(TEXT_SIZE * 0.78)
                        .wrapping(text::Wrapping::None)
                )
                .on_press(Message::SelectTab(index))
                .style(style)
                .width(190)
                .padding([4, 8]),
                button(text("×").size(TEXT_SIZE * 0.85))
                    .on_press(Message::CloseTab(index))
                    .style(button::text)
                    .padding([4, 6]),
            ]
            .align_y(Center),
        );
    }

    scrollable(list.padding([6, 8]))
        .direction(scrollable::Direction::Horizontal(
            scrollable::Scrollbar::default().width(4).scroller_width(4),
        ))
        .into()
}

fn on_key(event: keyboard::Event) -> Option<Message> {
    match event {
        keyboard::Event::KeyPressed { key, modifiers, .. } => keys(&key, modifiers),
        // Клик по ссылке приходит без модификаторов, поэтому их состояние
        // мы отслеживаем отдельно и храним у себя.
        keyboard::Event::ModifiersChanged(modifiers) => Some(Message::ModifiersChanged(modifiers)),
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
        // Кнопки в панели больше нет, но выход в системный браузер обязан
        // оставаться под рукой: на открытом вебе это частый путь, а не
        // крайний случай.
        key::Key::Character("o") if modifiers.command() => return Some(Message::OpenInBrowser),
        key::Key::Character("t") if modifiers.command() => return Some(Message::NewTab),
        key::Key::Character("w") if modifiers.command() => return Some(Message::CloseActiveTab),
        _ => return None,
    };

    match named {
        Named::ArrowLeft if modifiers.alt() => Some(Message::Back),
        Named::ArrowRight if modifiers.alt() => Some(Message::Forward),
        // Вкладки идут раньше прокрутки: Ctrl+PageDown иначе просто
        // пролистал бы страницу.
        Named::Tab if modifiers.command() && modifiers.shift() => Some(Message::CycleTab(false)),
        Named::Tab if modifiers.command() => Some(Message::CycleTab(true)),
        Named::PageDown if modifiers.command() => Some(Message::CycleTab(true)),
        Named::PageUp if modifiers.command() => Some(Message::CycleTab(false)),
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

/// Заголовок в оглавлении и его место в документе — долей от полной высоты.
#[derive(Debug, Clone, PartialEq)]
struct Entry {
    level: u8,
    title: String,
    at: f32,
}

/// Сколько знаков влезает в строку на нашей мере.
const CHARS_PER_LINE: f32 = MEASURE_IN_EMS * 2.0;
/// Высота блока-картинки: кнопка-заглушка плюс подпись.
const IMAGE_BLOCK: f32 = 90.0;

/// Блок разметки с его местом по высоте.
struct Block {
    at: f32,
    kind: Kind,
}

enum Kind {
    Heading { level: u8, title: String },
    Paragraph { lead: String },
}

/// Оглавление статьи.
///
/// Заголовки есть не везде: половина статей в вебе — сплошной текст без
/// единого `##`. Оглавление там всё равно нужно, иначе длинную статью
/// не с чего листать, — только вехами служат не заголовки, а начала
/// абзацев, расставленные примерно через экран. На короткой странице
/// не нужно ни то, ни другое: она и так вся под рукой.
///
/// Прыгать по документу iced умеет только долями от полной высоты
/// (`snap_to`), а спросить, где лежит виджет, нечем: `visible_bounds`
/// в 0.14 нет. Поэтому высоту считаем сами по разметке — так же, как её
/// потом разложит рендерер. Это оценка, а не измерение; систематическая
/// ошибка масштаба безвредна, она сокращается в доле.
fn outline(source: &str) -> Vec<Entry> {
    let (blocks, height) = scan(source);

    if height < MIN_DOC_HEIGHT {
        return Vec::new();
    }

    let mut headings: Vec<Entry> = blocks
        .iter()
        .filter_map(|block| match &block.kind {
            Kind::Heading { level, title } => Some(Entry {
                level: *level,
                title: title.clone(),
                at: block.at,
            }),
            Kind::Paragraph { .. } => None,
        })
        .collect();

    // Название статьи — не раздел: оно и так наверху, и в счёт разделов
    // не идёт. Иначе статья с двумя разделами считалась бы за три.
    if matches!(headings.first(), Some(first) if first.level == 1 && first.at == 0.0) {
        headings.remove(0);
    }

    let entries = if headings.len() >= MIN_HEADINGS {
        headings
    } else {
        waypoints(&blocks, height)
    };

    into_fractions(entries, height)
}

/// Перевести высоты в доли от полной.
fn into_fractions(mut entries: Vec<Entry>, height: f32) -> Vec<Entry> {
    let total = height.max(1.0);
    for entry in &mut entries {
        entry.at = (entry.at / total).clamp(0.0, 1.0);
    }
    entries
}

/// Вехи по началам абзацев — примерно через экран.
fn waypoints(blocks: &[Block], height: f32) -> Vec<Entry> {
    let leads: Vec<&Block> = blocks
        .iter()
        .filter(|block| matches!(block.kind, Kind::Paragraph { .. }))
        .collect();
    if leads.is_empty() {
        return Vec::new();
    }

    let wanted = ((height / WAYPOINT_EVERY).round() as usize).clamp(2, MAX_WAYPOINTS);
    let mut entries: Vec<Entry> = Vec::with_capacity(wanted);
    let mut taken = 0usize;

    for step in 0..wanted {
        let target = height * (step as f32 + 0.5) / wanted as f32;
        // Ближайший абзац к цели, но не тот, что уже взяли.
        let Some((index, block)) = leads
            .iter()
            .enumerate()
            .skip(taken)
            .min_by(|(_, a), (_, b)| {
                (a.at - target)
                    .abs()
                    .total_cmp(&(b.at - target).abs())
            })
        else {
            break;
        };
        taken = index + 1;

        if let Kind::Paragraph { lead } = &block.kind {
            entries.push(Entry {
                level: 1,
                title: lead.clone(),
                at: block.at,
            });
        }
    }
    entries
}

/// Разложить разметку на блоки и посчитать высоту так, как её разложит
/// рендерер: абзац занимает столько строк, сколько знаков не влезло
/// в меру, у кода строка своя, у картинки — фиксированный блок.
fn scan(source: &str) -> (Vec<Block>, f32) {
    let line_px = TEXT_SIZE * LINE_HEIGHT;
    let gap = TEXT_SIZE * 0.95;
    let code_line = TEXT_SIZE * 0.88 * 1.35;

    let mut blocks = Vec::new();
    let mut height = 0.0f32;
    let mut in_code = false;

    for line in source.lines() {
        let text = line.trim();

        if text.starts_with("```") {
            in_code = !in_code;
            height += code_line;
            continue;
        }
        if in_code {
            height += code_line;
            continue;
        }
        if text.is_empty() {
            height += gap;
            continue;
        }
        if let Some((level, title)) = heading(text) {
            blocks.push(Block {
                at: height,
                kind: Kind::Heading { level, title },
            });
            height += TEXT_SIZE * HEADINGS[usize::from(level - 1)] * HEADING_LINE_HEIGHT
                + TEXT_SIZE * 1.4;
            continue;
        }
        if text.starts_with("![") {
            height += IMAGE_BLOCK;
            continue;
        }

        let rows = (text.chars().count() as f32 / CHARS_PER_LINE).ceil().max(1.0);
        if rows >= 2.0 {
            // Вехой может быть только настоящий абзац, а не строка списка
            // или подпись: у коротких строк начало ничего не говорит.
            blocks.push(Block {
                at: height,
                kind: Kind::Paragraph { lead: lead(text) },
            });
        }
        height += rows * line_px;
    }

    (blocks, height)
}

/// Начало абзаца как подпись к вехе: до первой границы слова после сорока
/// знаков. Смысл в том, чтобы читатель узнал место, а не прочитал абзац.
fn lead(text: &str) -> String {
    let plain = plain(text);
    let mut out = String::new();

    for word in plain.split_whitespace() {
        if out.chars().count() + word.chars().count() > 40 {
            out.push('…');
            return out;
        }
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(word);
    }
    out
}

/// `## Заголовок` → уровень и текст без разметки.
fn heading(line: &str) -> Option<(u8, String)> {
    let level = line.chars().take_while(|c| *c == '#').count();
    if level == 0 || level > 6 {
        return None;
    }
    let rest = line[level..].strip_prefix(' ')?.trim();
    (!rest.is_empty()).then(|| (level as u8, plain(rest)))
}

/// Снять разметку с текста заголовка: в оглавлении нужен только он сам.
fn plain(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    let mut depth = 0usize;

    while let Some(c) = chars.next() {
        match c {
            // `[текст](ссылка)` — оставляем текст, адрес выбрасываем.
            '[' => {}
            ']' if chars.peek() == Some(&'(') => {
                depth = 1;
                chars.next();
            }
            '(' if depth > 0 => depth += 1,
            ')' if depth > 0 => depth -= 1,
            _ if depth > 0 => {}
            '*' | '_' | '`' => {}
            '\\' => {
                if let Some(escaped) = chars.next() {
                    out.push(escaped);
                }
            }
            _ => out.push(c),
        }
    }
    out.trim().to_owned()
}

/// Оглавление сбоку.
fn contents(entries: &[Entry]) -> Element<'_, Message> {
    let mut list = column![].spacing(2).width(Fill);

    for entry in entries {
        let indent = f32::from(entry.level.saturating_sub(1)) * 12.0;
        list = list.push(
            button(text(&entry.title).size(TEXT_SIZE * 0.8).width(Fill))
                .on_press(Message::JumpTo(entry.at))
                .width(Fill)
                .style(button::text)
                .padding(iced::Padding {
                    left: 6.0 + indent,
                    right: 6.0,
                    top: 3.0,
                    bottom: 3.0,
                }),
        );
    }

    container(scrollable(list).height(Fill))
        .width(TOC_WIDTH)
        .padding([24, 8])
        .into()
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

    /// Абзац с книжным межстрочным интервалом: у iced по умолчанию
    /// интерфейсный, для сплошного текста тесный.
    fn paragraph(
        &self,
        settings: markdown::Settings,
        text: &markdown::Text,
    ) -> Element<'a, Message> {
        rich_text(text.spans(settings.style))
            .size(settings.text_size)
            .line_height(LINE_HEIGHT)
            .on_link_click(Self::on_link_click)
            .into()
    }

    /// Заголовок: своя шкала кеглей, плотный интервал и воздух сверху,
    /// а не снизу — заголовок принадлежит тому, что под ним.
    fn heading(
        &self,
        settings: markdown::Settings,
        level: &'a markdown::HeadingLevel,
        text: &'a markdown::Text,
        index: usize,
    ) -> Element<'a, Message> {
        use markdown::HeadingLevel;

        let size = match level {
            HeadingLevel::H1 => settings.h1_size,
            HeadingLevel::H2 => settings.h2_size,
            HeadingLevel::H3 => settings.h3_size,
            HeadingLevel::H4 => settings.h4_size,
            HeadingLevel::H5 => settings.h5_size,
            HeadingLevel::H6 => settings.h6_size,
        };
        let air = if index > 0 { TEXT_SIZE * 1.4 } else { 0.0 };

        container(
            rich_text(text.spans(settings.style))
                .size(size)
                .line_height(HEADING_LINE_HEIGHT)
                .on_link_click(Self::on_link_click),
        )
        .padding(iced::Padding {
            top: air,
            ..iced::Padding::ZERO
        })
        .into()
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

/// Настройки рендера markdown. Собираем сами, а не через `with_text_size`:
/// у него своя шкала заголовков.
fn markdown_settings(theme: &Theme) -> markdown::Settings {
    let heading = |level: usize| iced::Pixels(TEXT_SIZE * HEADINGS[level]);

    markdown::Settings {
        text_size: TEXT_SIZE.into(),
        h1_size: heading(0),
        h2_size: heading(1),
        h3_size: heading(2),
        h4_size: heading(3),
        h5_size: heading(4),
        h6_size: heading(5),
        code_size: (TEXT_SIZE * 0.88).into(),
        spacing: (TEXT_SIZE * 0.95).into(),
        style: markdown::Style {
            font: iced::Font::with_name(BODY_FAMILY),
            inline_code_font: iced::Font::with_name(MONO_FAMILY),
            code_block_font: iced::Font::with_name(MONO_FAMILY),
            ..markdown::Style::from_palette(theme.palette())
        },
    }
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
        let mut reader = Reader::new(Vec::new()).0;
        let _ = reader.update(Message::Scrolled(scroll, true));
        assert!(matches!(reader.tab().page, Page::Blank), "состояние не должно меняться");
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

    /// Достаточно длинный кусок текста, чтобы страница не считалась короткой.
    fn filler() -> String {
        let para = "Длинный абзац статьи, в котором знаков хватает на несколько \
строк нашей меры, иначе страница выйдет короткой и оглавления не получит.";
        vec![para; 12].join("\n\n")
    }

    #[test]
    fn outline_skips_the_article_title() {
        let text = filler();
        let doc = format!(
            "# Название статьи\n\n{text}\n\n## Первый\n\n{text}\n\n## Второй\n\n{text}\n\n## Третий\n"
        );
        let entries = outline(&doc);
        assert_eq!(entries.len(), 3, "название в оглавление не идёт: {entries:?}");
        assert_eq!(entries[0].title, "Первый");
        assert_eq!(entries[0].level, 2);
    }

    #[test]
    fn a_short_page_gets_no_contents() {
        let doc = "# Заметка\n\nОдин абзац, и на этом всё.\n";
        assert!(outline(doc).is_empty(), "короткой странице оглавление не нужно");
    }

    #[test]
    fn a_long_page_without_headings_gets_waypoints() {
        let para = "Длинный абзац статьи, в котором достаточно знаков, чтобы \
он занял несколько строк на нашей мере и попал в разметку вехой.";
        let doc = format!("# Название\n\n{}\n", vec![para; 40].join("\n\n"));
        let entries = outline(&doc);
        assert!(entries.len() >= 2, "вехи должны появиться: {}", entries.len());
        assert!(entries.len() <= MAX_WAYPOINTS);
        assert!(entries.windows(2).all(|w| w[0].at <= w[1].at), "порядок вех нарушен");
        assert!(entries.iter().all(|e| !e.title.is_empty()));
    }

    #[test]
    fn a_waypoint_label_is_short() {
        let long = "Это очень длинное начало абзаца, которое ни в какое оглавление целиком не влезет";
        let label = lead(long);
        assert!(label.chars().count() <= 42, "подпись слишком длинная: {label:?}");
        assert!(label.ends_with('…'));
    }

    #[test]
    fn outline_places_headings_in_order() {
        let text = filler();
        let doc = format!(
            "# Название\n\n{text}\n\n## Начало\n\n{text}\n\n## Середина\n\n{text}\n\n## Конец\n\n{text}\n"
        );
        let entries = outline(&doc);
        assert_eq!(entries.len(), 3, "{entries:?}");
        assert!(entries.windows(2).all(|w| w[0].at < w[1].at), "порядок нарушен: {entries:?}");
        let middle = entries[1].at;
        assert!((0.35..0.7).contains(&middle), "середина не в середине: {middle}");
        assert!(entries.iter().all(|e| (0.0..=1.0).contains(&e.at)));
    }

    #[test]
    fn heading_text_loses_its_markup() {
        assert_eq!(
            heading("## [Ссылка](https://example.com) и **жирное**"),
            Some((2, "Ссылка и жирное".to_owned()))
        );
        assert_eq!(heading("#Не заголовок"), None);
        assert_eq!(heading("Обычный текст"), None);
    }

    #[test]
    fn code_fences_do_not_become_headings() {
        let text = filler();
        let doc = format!(
            "# Название\n\n```\n# это комментарий, а не заголовок\n```\n\n\
## Первый\n\n{text}\n\n## Второй\n\n{text}\n\n## Третий\n\n{text}\n"
        );
        let entries = outline(&doc);
        assert_eq!(entries.len(), 3, "комментарий в коде — не заголовок: {entries:?}");
        assert!(entries.iter().all(|e| e.title != "это комментарий, а не заголовок"));
    }

    #[test]
    fn the_way_out_to_the_browser_survives_as_a_shortcut() {
        // Кнопку из панели убрали, клавиша осталась — требование M1 в силе.
        assert!(matches!(
            keys(&key::Key::Character("o".into()), Modifiers::COMMAND),
            Some(Message::OpenInBrowser)
        ));
        assert!(keys(&key::Key::Character("o".into()), Modifiers::default()).is_none());
    }

    #[test]
    fn a_new_tab_opens_and_becomes_the_one_you_read() {
        let mut reader = Reader::new(Vec::new()).0;
        assert_eq!(reader.tabs.len(), 1);

        let _ = reader.update(Message::NewTab);
        assert_eq!(reader.tabs.len(), 2);
        assert_eq!(reader.active, 1, "новая вкладка должна стать открытой");
        assert!(matches!(reader.tab().page, Page::Blank));
    }

    #[test]
    fn tabs_keep_their_own_address() {
        let mut reader = Reader::new(Vec::new()).0;
        let _ = reader.update(Message::InputChanged("первая".to_owned()));
        let _ = reader.update(Message::NewTab);
        let _ = reader.update(Message::InputChanged("вторая".to_owned()));

        assert_eq!(reader.tabs[0].input, "первая");
        assert_eq!(reader.tabs[1].input, "вторая");
    }

    #[test]
    fn a_tab_remembers_where_you_left_it() {
        let mut reader = Reader::new(Vec::new()).0;
        let _ = reader.update(Message::ScrollChanged(1200.0));
        let _ = reader.update(Message::NewTab);
        let _ = reader.update(Message::ScrollChanged(40.0));

        assert_eq!(reader.tabs[0].scroll, 1200.0);
        assert_eq!(reader.tabs[1].scroll, 40.0);

        let _ = reader.update(Message::SelectTab(0));
        assert_eq!(reader.tab().scroll, 1200.0, "вернуться надо туда же");
    }

    #[test]
    fn the_last_tab_does_not_close_into_nothing() {
        let mut reader = Reader::new(Vec::new()).0;
        let _ = reader.update(Message::InputChanged("что-то".to_owned()));
        let _ = reader.update(Message::CloseActiveTab);

        assert_eq!(reader.tabs.len(), 1, "окно без вкладок показывать нечем");
        assert!(reader.tab().input.is_empty(), "и она должна быть чистой");
    }

    #[test]
    fn closing_a_tab_lands_on_a_neighbour() {
        let mut reader = Reader::new(Vec::new()).0;
        let _ = reader.update(Message::NewTab);
        let _ = reader.update(Message::NewTab);
        assert_eq!((reader.tabs.len(), reader.active), (3, 2));

        let _ = reader.update(Message::CloseActiveTab);
        assert_eq!((reader.tabs.len(), reader.active), (2, 1));

        let _ = reader.update(Message::CloseTab(0));
        assert_eq!((reader.tabs.len(), reader.active), (1, 0));
    }

    #[test]
    fn cycling_wraps_around() {
        let mut reader = Reader::new(Vec::new()).0;
        let _ = reader.update(Message::NewTab);
        let _ = reader.update(Message::CycleTab(true));
        assert_eq!(reader.active, 0, "с последней вперёд — на первую");
        let _ = reader.update(Message::CycleTab(false));
        assert_eq!(reader.active, 1, "с первой назад — на последнюю");
    }

    #[test]
    fn a_long_title_fits_the_tab() {
        let clipped = clip("Announcing Rust 1.81.0 | Rust Blog", TAB_LABEL);
        assert!(clipped.chars().count() <= TAB_LABEL, "{clipped:?}");
        assert!(clipped.ends_with('…'));
        assert_eq!(clip("Коротко", TAB_LABEL), "Коротко");
    }

    fn link(target: &str) -> Message {
        Message::LinkClicked(target.to_owned())
    }

    #[test]
    fn ctrl_click_opens_a_link_in_a_background_tab() {
        let mut reader = Reader::new(Vec::new()).0;
        let _ = reader.update(Message::ModifiersChanged(Modifiers::COMMAND));
        let _ = reader.update(link("https://example.test/статья"));

        assert_eq!(reader.tabs.len(), 2, "ссылка должна открыться вкладкой");
        assert_eq!(reader.active, 0, "и открыться в фоне, не уводя читателя");
        assert_eq!(reader.tabs[1].input, "https://example.test/статья");
        assert!(matches!(reader.tabs[1].page, Page::Loading));
        // Читатель остался там, где был.
        assert!(matches!(reader.tabs[0].page, Page::Blank));
    }

    #[test]
    fn shift_click_only_copies_the_link() {
        let mut reader = Reader::new(Vec::new()).0;
        let _ = reader.update(Message::ModifiersChanged(Modifiers::SHIFT));
        let _ = reader.update(link("https://example.test/"));

        assert_eq!(reader.tabs.len(), 1, "новой вкладки быть не должно");
        assert!(matches!(reader.tab().page, Page::Blank), "и перехода тоже");
    }

    #[test]
    fn a_plain_click_follows_the_link_where_you_are() {
        let mut reader = Reader::new(Vec::new()).0;
        let _ = reader.update(link("https://example.test/"));

        assert_eq!(reader.tabs.len(), 1);
        assert!(matches!(reader.tab().page, Page::Loading));
    }

    #[test]
    fn the_menu_opens_where_you_clicked_and_closes_on_the_next_click() {
        let mut reader = Reader::new(Vec::new()).0;
        let _ = reader.update(Message::CursorMoved(Point::new(120.0, 340.0)));
        let _ = reader.update(Message::OpenMenu);
        assert_eq!(reader.menu, Some(Point::new(120.0, 340.0)));

        let _ = reader.update(Message::CloseMenu);
        assert!(reader.menu.is_none());
    }

    #[test]
    fn copying_from_the_menu_closes_it() {
        let mut reader = Reader::new(Vec::new()).0;
        let _ = reader.update(Message::OpenMenu);
        let _ = reader.update(Message::Copy("что-то".to_owned()));
        assert!(reader.menu.is_none());
    }

    #[test]
    fn tab_shortcuts_are_wired() {
        assert!(matches!(
            keys(&key::Key::Character("t".into()), Modifiers::COMMAND),
            Some(Message::NewTab)
        ));
        assert!(matches!(
            keys(&key::Key::Character("w".into()), Modifiers::COMMAND),
            Some(Message::CloseActiveTab)
        ));
        assert!(matches!(
            keys(&named(key::Named::Tab), Modifiers::COMMAND),
            Some(Message::CycleTab(true))
        ));
        assert!(matches!(
            keys(&named(key::Named::Tab), Modifiers::COMMAND | Modifiers::SHIFT),
            Some(Message::CycleTab(false))
        ));
        // Без Ctrl PageDown остаётся прокруткой.
        assert!(matches!(
            keys(&named(key::Named::PageDown), Modifiers::default()),
            Some(Message::Scrolling(_))
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
