//! Окно Brevier на GTK4.
//!
//! Статья рисуется одним `GtkTextView`, а не набором виджетов на абзац:
//! выделение должно идти через весь документ, а не обрываться на границе
//! абзаца. Тем же решением бесплатно приходят копирование, контекстное меню,
//! точное оглавление по меткам в тексте и доступность через AT-SPI.
//!
//! Тулкит живёт только здесь. Разбор адреса, история, оглавление и тексты
//! ошибок лежат в ядре и про GTK не знают ничего.

// На Windows окно — оконная программа, а не консольная: иначе запуск из меню
// «Пуск» поднимал бы рядом пустую консоль. `--help` и `--version` при этом
// видны, только если вывод перенаправлен; в консоли отвечает `brevier.exe`.
#![cfg_attr(windows, windows_subsystem = "windows")]

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, VecDeque};
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

mod article;
mod formula;
mod sharp;

use article::Article;
use formula::Formula;
use sharp::Sharp;

use gtk::gio;
use gtk::glib;
use gtk::pango;
use gtk::prelude::*;
use gtk::{Application, ApplicationWindow};

use brevier::address::{self, Address, ArchivePage, Internal, Repo};
use brevier::failure::{Failure, describe, describe_page};
use brevier::media::{self, Raster, Source};
use brevier::outline::{
    ALERT_SIZE, ALERT_TRACKING, CODE_GAP, CODE_SIZE, HANG, HEADING_WEIGHTS, HEADINGS, INDENT,
    LINE_HEIGHT, MEASURE, NOTE_HANG, NOTE_INDENT, NOTE_SIZE, NOTEREF_RISE, NOTEREF_SIZE, PAD_SIZE,
    TEXT_SIZE, ZOOM_NORMAL, ZOOM_STEPS, clip,
};
use brevier::page::{self, Block};
use brevier::palette::{
    FOUND, FOUND_HERE, FOUND_INK, INK_DARK, INK_LIGHT, PAPER_DARK, PAPER_LIGHT, SHELF_DARK,
    SHELF_LIGHT, colors, rgb,
};
use brevier::reading::{self, Progress, Readings};
use brevier::save;
use brevier::store::{self, HINTS, Hint, Marks, Settings, Store};
use brevier::{Document, History, UserAgent};

const APP_ID: &str = "io.github.gurov.brevier";
/// Высота полосы прогресса внизу статьи (#19), в точках.
const BAR_HEIGHT: i32 = 7;
/// Сколько читатель «здесь» после последнего движения: экран текста читают
/// около минуты, три — с запасом на длинный абзац.
const PRESENT: Duration = Duration::from_secs(180);
/// Сколько висит предложение «Continue from N%» (#19).
const OFFER_FOR: Duration = Duration::from_secs(60);
/// Начало адреса поиска по архиву (#8): его ставит `Ctrl+Shift+F`.
const ARCHIVE_SEARCH: &str = "brevier:archive?q=";
const BODY_FAMILY: &str = "Noto Sans";
const MONO_FAMILY: &str = "Noto Sans Mono";
/// Жирность в единицах Pango: свойство тега — целое, а не перечисление.
const BOLD: i32 = 700;
/// Ширина полки: с чего она начинает и в каких пределах её тянут
/// за делитель. Верхний предел ещё и упирается в меру статьи: ужать
/// колонку текста делитель не даёт.
const TOC_WIDTH: i32 = 260;
const TOC_MIN: i32 = 150;
const TOC_MAX: i32 = 520;
/// Сколько строк отводим пункту полки. Что не влезло — многоточие.
/// Заголовки бывают длинными, и обрезанный заголовок хуже длинного:
/// по нему не опознать раздел, ради которого в полку и смотрят.
const TOC_LINES: i32 = 5;
/// Длиннее этого заголовок в полку не отдаём вовсе: раскладывать абзац,
/// от которого видно пять строк, незачем.
const TOC_CHARS: usize = 300;
/// Сколько знаков влезает на корешок вкладки.
const TAB_LABEL: usize = 24;
/// Естественная ширина подписи в рамке картинки, в знаках. Подпись всё равно
/// переносится по всей ширине рамки; число только не даёт ей просить больше.
/// Без него переносимая метка GTK просит ширину всего текста в одну строку,
/// и длинный `alt` раздвигал колонку за меру — у nngroup до края окна.
const CAPTION_CHARS: i32 = 40;

/// Докуда растёт список подсказок. Восемь строк в две строчки каждая —
/// и ни пикселем больше: подсказка помогает выбрать, а не читать.
const HINT_HEIGHT: i32 = 330;

/// Уже этого список не показываем, даже если адресная строка уже: в узком
/// окне она короче адреса, и подсказка из одних многоточий не подсказка.
const HINT_WIDTH: i32 = 420;

/// Как называется дверь снаружи: действие окна, которым второй запуск
/// передаёт адрес уже открытому.
const OPEN_ACTION: &str = "open-address";

/// Докуда растёт «Loading…», прежде чем начать сначала.
const LOADING_DOTS: usize = 10;
/// Что говорим на странице, оказавшейся списком ссылок, а не статьёй.
const LISTING: &str = "A list of links, not an article — pick one to read.";
const SERVED_MARKDOWN: &str = "Served as Markdown by the site — the author's exact text.";
/// Чем и как отмечена страница, приехавшая по `http://`.
const INSECURE_ICON: &str = "channel-insecure-symbolic";
const INSECURE: &str =
    "Not secure: this page came over plain http, so anyone on the way can read and change it.";
/// Метка, которой прокручивают буфер: одна на все прыжки.
const JUMP: &str = "brevier-jump";
/// Сколько кадров ждём, пока картинки и таблицы займут своё место.
const SETTLE_FRAMES: u8 = 45;
/// Куда по высоте окна ставить заголовок, к которому прыгнули: вплотную
/// к кромке он выглядит обрезанным.
const ANCHOR_ALIGN: f64 = 0.1;

/// Что окно отвечает в терминале. Оно запускается строкой, значит обязано
/// уметь объяснить себя там же: `--help` у окна — такая же часть продукта,
/// как и у cli.
const HELP: &str = "\
brevier-ui — the Brevier window: reading without JavaScript, in the typography
you chose rather than the one the site shipped.

Usage: brevier-ui [options] [<url|feed://…|gh:owner/repo|path>…]

Every address opens in its own tab; without one the window starts on its intro
page. Brevier is a single application: a second launch adds a window to the one
already running.

Options:
  -h, --help     this text
  -V, --version  version

Keys: Ctrl+L the address bar, Ctrl+T new tab, Ctrl+W close it, Ctrl+H what you
      have read, Ctrl+F find on page, Ctrl+S save the article, Ctrl+O hand the
      page to your system browser, Ctrl+plus/minus/0 zoom the page.

Pages you read are remembered: the address bar suggests them as you type, and
`brevier:history` lists them by day. The list is a plain text file under
$XDG_DATA_HOME/brevier (BREVIER_DATA_DIR moves it); deleting a line forgets
a page.
";

fn main() -> glib::ExitCode {
    // Про саму программу отвечаем до того, как поднято приложение: у GTK
    // второй запуск отдаёт строку уже работающему экземпляру, и ответ вышел бы
    // в чужой терминал — в тот, из которого запустили первое окно.
    match answer(&std::env::args().skip(1).collect::<Vec<String>>()) {
        Some(Ok(text)) => {
            print!("{text}");
            return glib::ExitCode::SUCCESS;
        }
        Some(Err(text)) => {
            eprint!("{text}");
            return glib::ExitCode::FAILURE;
        }
        None => {}
    }

    brevier::init_crypto();
    // fontconfig читает свой конфиг при первой отрисовке, поэтому до GTK;
    // на Windows шрифты отдаются Pango, а её карта шрифтов есть только после.
    #[cfg(not(windows))]
    use_bundled_fonts();

    let app = Application::builder()
        .application_id(APP_ID)
        // Адреса разбираем сами, поэтому GTK их трогать не должен.
        .flags(gio::ApplicationFlags::HANDLES_COMMAND_LINE)
        .build();

    // Иконка ставится на запуске приложения, а не в `main`: темы значков
    // до открытого дисплея ещё нет. Там же, раз на запуск и в фоне, — уборка
    // недельного кэша страниц: второй экземпляр сюда не доходит.
    app.connect_startup(|_| {
        use_bundled_icon();
        #[cfg(windows)]
        use_bundled_fonts();
        std::thread::spawn(|| {
            brevier::cache::Cache::open().prune();
            // Архив (#8): копия живёт, пока на неё указывает журнал, а у
            // закладки — последняя копия навсегда.
            let kept = brevier::store::Store::open().copies();
            let marked = brevier::store::Marks::open()
                .marks()
                .iter()
                .map(|mark| mark.address.clone())
                .collect();
            brevier::archive::Archive::open().prune(&kept, &marked);
        });
    });

    app.connect_command_line(|app, command_line| {
        let start: Vec<String> = command_line
            .arguments()
            .into_iter()
            .skip(1)
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect();

        // Адрес, пришедший снаружи — из чата, почты, «открыть с помощью», —
        // ложится вкладкой в уже открытое окно. Это продуктовый выбор,
        // и он такой же, как у браузеров: ссылка означает «покажи мне ещё
        // одну страницу», а не «дай мне ещё одно окно». Второе окно
        // по-прежнему заводится запуском без адреса.
        // Действие висит на `ApplicationWindow`: только он и умеет
        // быть группой действий.
        let open = app
            .active_window()
            .and_then(|window| window.downcast::<ApplicationWindow>().ok())
            .filter(|_| !start.is_empty());
        match open {
            Some(window) => {
                for text in &start {
                    ActionGroupExt::activate_action(&window, OPEN_ACTION, Some(&text.to_variant()));
                }
                window.present();
            }
            None => build(app, start),
        }
        glib::ExitCode::SUCCESS
    });

    app.run()
}

/// Ответ терминалу вместо окна — или `None`, если в строке одни адреса.
///
/// Читается до GTK, поэтому отвечает всегда тот процесс, которого спросили.
///
/// Всё, начинающееся с дефиса, считается ключом: адреса с него не начинаются,
/// и открывать вкладку на опечатке в ключе — худший из ответов, потому что
/// выглядит он как сломанная страница.
fn answer(args: &[String]) -> Option<Result<String, String>> {
    for arg in args {
        match arg.as_str() {
            "-h" | "--help" => return Some(Ok(HELP.to_owned())),
            "-V" | "--version" => {
                return Some(Ok(format!("brevier-ui {}\n", env!("CARGO_PKG_VERSION"))));
            }
            _ if arg.starts_with('-') && arg != "-" => {
                return Some(Err(format!("brevier-ui: unknown option `{arg}`\n\n{HELP}")));
            }
            _ => {}
        }
    }
    None
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
    /// История посещённого. Место ей рядом с «назад» и «вперёд»: это тоже
    /// навигация, только не по ссылкам, а по времени.
    history: gtk::Button,
    /// Подсказки адресной строки: список под ней, как в любом браузере.
    /// Не `autohide`: всплывающее окно, забирающее себе клавиатуру, отняло
    /// бы её у строки, в которой в этот момент печатают.
    hints: gtk::Popover,
    hint_list: gtk::ListBox,
    /// Строку адреса окно правит и само — при каждом переходе. Пока правит,
    /// подсказки молчат: список, выскакивающий после каждой загруженной
    /// страницы, — это не помощь.
    quiet: Rc<Cell<bool>>,
    contents: gtk::ListBox,
    /// Полка целиком: подпись, черта и прокрутка под ними. Прячется
    /// и показывается она, а не список, — подпись обязана уходить вместе
    /// с оглавлением.
    shelf: gtk::Box,
    /// Окно прокрутки полки. Нужно отдельно: по нему подсвеченную строку
    /// доводят до глаз, когда оглавление длиннее полки.
    shelf_view: gtk::ScrolledWindow,
    /// Делитель окна: ширину полки читатель задаёт перетаскиванием.
    split: gtk::Paned,
    /// Ширина полки, которую выбрал читатель. Храним ширину, а не позицию
    /// делителя: позиция считается от левого края, полка стоит у правого,
    /// и при смене размера окна одна и та же позиция означала бы разную
    /// ширину.
    shelf_width: Rc<Cell<i32>>,
    show_contents: gtk::ToggleButton,
    /// Меню в шапке: настройки, история, закладки. Раньше здесь была
    /// шестерёнка, открывавшая отдельное окно настроек; теперь это выпадающее
    /// меню, а сами настройки живут вкладкой, а не отдельным окном.
    menu: gtk::MenuButton,
    /// Ступень масштаба. Появляется в шапке, только когда она не сто
    /// процентов, и одним нажатием возвращает к ним: панель не свалка,
    /// а кнопка, которая всегда показывает «100%», не говорит ничего.
    zoom_level: gtk::Button,
    save: gtk::Button,
    /// Закладка на открытую страницу. Кнопка, а не строка настроек:
    /// решение это не «раз и надолго», а про ту страницу, что сейчас
    /// на экране, — и состояние своё она показывает сама, значком.
    star: gtk::Button,
    /// Строка состояния внизу: что сохранилось, что не загрузилось.
    notice: gtk::Label,
    /// «Continue from 43%» (#19): предложение, а не прыжок — кнопкой рядом
    /// со строкой состояния.
    offer: gtk::Button,
    /// Тонкая полоса внизу статьи: докуда дочитано и точки разделов —
    /// как в читалках книг (CoolReader). Рисует `draw_bar`.
    bar: gtk::DrawingArea,
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
    links: Vec<page::Link>,
    /// Ссылка под отметкой клавиатуры — номер в `links` (#34). Новая
    /// отрисовка её снимает: тег уходит вместе с текстом.
    focus: Option<usize>,
    /// Куда прыгать по оглавлению — смещения в буфере, а не доли высоты.
    marks: Vec<page::Mark>,
    /// Якоря заголовков: по ним находится место для ссылки вида `#anchor`.
    anchors: Vec<(String, usize)>,
    /// Места картинок в тексте.
    shots: Vec<Shot>,
    /// Навигация сайта: его меню и подвал. Свойство страницы, а не вкладки,
    /// — у каждого сайта своё.
    site: Vec<Entry>,
    /// Ленты, объявленные страницей. Тоже свойство страницы.
    feeds: Vec<Entry>,
    /// Точки входа в документацию проекта. Свойство репозитория, а не файла:
    /// при переходе между файлами одного проекта заново не ищутся.
    entries: Vec<Entry>,
    /// Для какого проекта они найдены. Ставится до того, как проба вернулась,
    /// иначе каждый открытый файл запускал бы её снова.
    entries_for: Option<Repo>,
    /// Что показано. Нужно для сохранения: на экране текст уже разложен
    /// по буферу, а на диск ложится markdown.
    document: Option<Document>,
    /// Номер загрузки. Ответ брошенной страницы отличаем по нему: отменить
    /// синхронный `ureq` нечем, но и слушать его уже незачем.
    generation: u64,
    /// Страница едет прямо сейчас. По нему живёт счётчик точек в слове
    /// «Loading»: номера загрузки мало — он остаётся тем же и после того,
    /// как страница приехала.
    loading: bool,
    /// Куда вернуть читателя, когда страница приедет: смещение в буфере
    /// из сохранённой сессии. Живёт во вкладке, а не в аргументах `open`,
    /// потому что нужно ровно один раз и ровно после загрузки.
    resume: Option<i32>,
    /// Копия только что загруженной страницы в архиве (#8): путь для строки
    /// журнала. Живёт до показа страницы — ровно как `resume`.
    copy: Option<String>,
    /// Прогресс чтения открытой страницы (#19): что прочитано, где место.
    progress: Option<Progress>,
    /// Страницу открыли заново — ссылкой, адресом, из истории, — а не шагом
    /// «назад/вперёд»: только тогда предлагаем продолжить с прошлого места.
    fresh_visit: bool,
    /// Вкладка есть, страницы ещё нет: так возвращается из сессии всё,
    /// кроме той вкладки, что была впереди. Грузится она в тот миг,
    /// когда на неё переключились, — десять восстановленных вкладок
    /// не должны означать десять запросов на старте.
    pending: bool,
    /// На какой ступени масштаба вкладка сейчас нарисована. Масштаб общий
    /// на окно; если читатель сменил его, пока эта вкладка была в фоне,
    /// при показе её надо перерисовать под новую ступень.
    zoom_seen: usize,
    /// При какой плотности экрана (`scale_factor`) разобраны её картинки.
    /// Окно переехало на другой экран — перерисовать, как при смене ступени:
    /// картинки разбираются в пикселях экрана (#15).
    density_seen: i32,
    /// Кэш «назад/вперёд»: уже показанные документы этой вкладки, по адресу.
    /// «Назад» рисует страницу из него сразу, без сети, — как bfcache
    /// у браузеров. Живёт ровно на путь истории: страницы, обрубленные
    /// новым переходом, из кэша уходят (`prune_pages`). Внутренние страницы
    /// (сама история) не кэшируются — они обязаны показывать то, что на диске.
    pages: HashMap<String, Document>,
    /// Уже скачанные картинки этой вкладки — сырые байты по адресу источника.
    /// Чтобы «назад» показывал их из памяти, а не тянул из сети заново. Общий
    /// на вкладку: одна и та же картинка на двух страницах качается один раз.
    blobs: Blobs,
    /// Это вкладка настроек, а не статья: `view` у неё заглушка, истории нет,
    /// в сессию она не едет и на масштаб не отзывается. Одна на окно.
    settings: bool,
}

/// Сколько байтов картинок держим на вкладке, прежде чем вытеснять старые.
/// Читательская сессия иначе набрала бы сотни мегабайт за день; вытесняем
/// по возрасту — самое старое первым.
const BLOB_BUDGET: usize = 48 * 1024 * 1024;

/// Кэш сырых байтов картинок вкладки с потолком по объёму.
#[derive(Default)]
struct Blobs {
    map: HashMap<String, Arc<Vec<u8>>>,
    /// Порядок прихода — по нему вытесняем старое, когда упёрлись в потолок.
    order: VecDeque<String>,
    bytes: usize,
}

impl Blobs {
    fn get(&self, url: &str) -> Option<Arc<Vec<u8>>> {
        self.map.get(url).cloned()
    }

    fn put(&mut self, url: String, data: Arc<Vec<u8>>) {
        if self.map.contains_key(&url) {
            return;
        }
        self.bytes += data.len();
        self.order.push_back(url.clone());
        self.map.insert(url, data);
        while self.bytes > BLOB_BUDGET {
            let Some(old) = self.order.pop_front() else {
                break;
            };
            if let Some(gone) = self.map.remove(&old) {
                self.bytes = self.bytes.saturating_sub(gone.len());
            }
        }
    }
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
    /// Класть ли копию каждой прочитанной страницы в архив (#8).
    archive: bool,
    /// Ступень масштаба — одна на окно, общая для всех страниц. Читатель
    /// выставляет её раз и ждёт её везде: разная по хостам ступень
    /// оборачивалась тем, что соседняя страница открывалась «странно».
    /// На диск по-прежнему не едет — только на этот запуск: иначе это уже
    /// хранилище настроек со своим форматом и починкой при обновлении.
    zoom: usize,
    /// Куда читатель уже ходил. Журнал на диске, свод в памяти: по нему
    /// строятся подсказки адресной строки, а страница истории читает файл
    /// заново — программа может быть открыта и дважды.
    store: Store,
    /// Страницы, отмеченные читателем. В памяти — чтобы звёздочка знала,
    /// зажигаться ли ей, не заглядывая на диск при каждом переходе.
    marks: Marks,
    /// Запомненные долгие чтения (#19): `reading.tsv`.
    readings: Readings,
    /// Когда читатель последний раз что-то делал со страницей: листал,
    /// нажимал, водил мышью. Время чтения идёт, только пока это было
    /// недавно (`PRESENT`) и окно активно.
    last_input: Instant,
    /// Полоски долей прочитанного у строк оглавления: где начинается раздел,
    /// его доля и сама полоска.
    bars: Vec<(usize, Rc<Cell<f32>>, gtk::DrawingArea)>,
    /// Предложение продолжить, которое сейчас висит у строки состояния:
    /// чья вкладка и куда.
    offered: Option<(u64, usize)>,
    /// Это окно отвечает за сессию: оно её подняло, оно её и пишет.
    /// Сессия одна на программу, а окон бывает несколько — иначе второе
    /// окно затирало бы вкладки первого своими.
    keeps_session: bool,
    /// Что сейчас в списке подсказок. Как и у полки: обработчик подключён
    /// один раз, а строка ищет себя по месту в списке.
    hints: Vec<String>,
    /// Поиск: строка одна на окно, поэтому и состояние одно.
    search: Search,
    /// Что делает строка полки. Полка одна на окно, значит и список один,
    /// а обработчик подключён раз при сборке: иначе на каждой открытой
    /// странице копился бы ещё один, со ссылками на прошлую.
    shelf: Vec<Row>,
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
/// (см. шапку `formula.rs`). У холста помним его место в буфере — по нему
/// формулу опускают под базовую линию (`sink`).
#[derive(Clone)]
enum Slot {
    Frame(gtk::Box),
    Canvas(Formula, i32),
}

impl Shot {
    fn new(source: &Source, alt: &str, inline: bool, slot: Slot) -> Self {
        Shot {
            source: source.clone(),
            alt: alt.to_owned(),
            inline,
            slot,
            busy: Rc::new(Cell::new(false)),
        }
    }
}

impl Slot {
    fn frame(&self) -> Option<&gtk::Box> {
        match self {
            Slot::Frame(frame) => Some(frame),
            Slot::Canvas(..) => None,
        }
    }
}

impl State {
    fn find(&mut self, id: u64) -> Option<&mut Tab> {
        self.tabs.iter_mut().find(|tab| tab.id == id)
    }
}

/// Точка входа в документацию проекта: строка полки, ведущая в другой файл.
#[derive(Clone)]
struct Entry {
    title: String,
    address: Address,
}

/// Что делает строка полки. Две группы рядом означают две разные работы:
/// строка оглавления прокручивает открытый документ, строка проекта уводит
/// в другой файл.
#[derive(Clone)]
enum Row {
    Jump(i32),
    Open(Address),
    /// Подпись над группой, а не строка: нажать её нельзя.
    Header,
}

fn build(app: &Application, start: Vec<String>) {
    // Настройки поднимаем до виджетов: выставить их потом значило бы
    // дёрнуть все обработчики разом и на глазах у читателя перекрасить
    // окно из светлого в тёмное.
    let settings = Settings::load();
    let ui = Ui {
        window: ApplicationWindow::builder()
            .application(app)
            .title("Brevier")
            .default_width(1100)
            .default_height(800)
            .build(),
        notebook: gtk::Notebook::builder().scrollable(true).build(),
        entry: gtk::Entry::builder()
            .placeholder_text("address or search")
            .hexpand(true)
            .build(),
        back: gtk::Button::from_icon_name("go-previous-symbolic"),
        forward: gtk::Button::from_icon_name("go-next-symbolic"),
        history: gtk::Button::from_icon_name("document-open-recent-symbolic"),
        hints: gtk::Popover::builder()
            .autohide(false)
            .has_arrow(false)
            .position(gtk::PositionType::Bottom)
            .build(),
        hint_list: gtk::ListBox::builder()
            .selection_mode(gtk::SelectionMode::Single)
            // Строка подсказки фокуса не берёт: щелчок по ней не должен
            // уводить курсор из адресной строки — иначе строка теряет фокус
            // раньше, чем щелчок доходит до списка.
            .can_focus(false)
            .build(),
        quiet: Rc::new(Cell::new(false)),
        contents: gtk::ListBox::builder()
            // Выделение здесь не выбор, а «вы сейчас здесь»: строку под
            // глазами полка отмечает сама, по ходу чтения.
            .selection_mode(gtk::SelectionMode::Single)
            .build(),
        shelf: gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .width_request(TOC_MIN)
            .visible(false)
            .build(),
        shelf_view: gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vexpand(true)
            .build(),
        split: gtk::Paned::builder()
            .orientation(gtk::Orientation::Horizontal)
            // Лишнее место при смене размера окна достаётся статье,
            // полка держит свою ширину.
            .resize_start_child(true)
            .resize_end_child(false)
            // Ужать статью уже меры делитель не даст: мера — обещание
            // продукта, а не предпочтение читателя.
            .shrink_start_child(false)
            .shrink_end_child(false)
            .vexpand(true)
            .build(),
        shelf_width: Rc::new(Cell::new(settings.shelf_width.unwrap_or(TOC_WIDTH))),
        show_contents: gtk::ToggleButton::builder()
            .icon_name("view-list-symbolic")
            .tooltip_text("Contents")
            .active(settings.shelf)
            .sensitive(false)
            .build(),
        menu: gtk::MenuButton::builder()
            .icon_name("open-menu-symbolic")
            .tooltip_text("Menu")
            .build(),
        zoom_level: gtk::Button::builder()
            .tooltip_text("Reset zoom (Ctrl+0)")
            .visible(false)
            .build(),
        save: gtk::Button::from_icon_name("document-save-symbolic"),
        star: gtk::Button::from_icon_name("non-starred-symbolic"),
        notice: gtk::Label::builder()
            .xalign(0.0)
            .wrap(true)
            .hexpand(true)
            .margin_start(10)
            .margin_end(10)
            .margin_top(4)
            .margin_bottom(4)
            .visible(false)
            .build(),
        offer: gtk::Button::builder()
            .visible(false)
            .margin_end(10)
            .valign(gtk::Align::Center)
            // Справа и тогда, когда строка состояния уже погасла.
            .halign(gtk::Align::End)
            .hexpand(true)
            .build(),
        bar: gtk::DrawingArea::builder()
            .content_height(BAR_HEIGHT)
            .hexpand(true)
            .vexpand(false)
            .build(),
        search: gtk::SearchBar::builder().build(),
        needle: gtk::SearchEntry::builder()
            .placeholder_text("find on page")
            .hexpand(true)
            .build(),
        tally: gtk::Label::builder().width_chars(10).xalign(1.0).build(),
        paint: gtk::CssProvider::new(),
    };
    // Подпись над полкой. Стоит над прокруткой, а не первой строкой
    // списка: иначе она уезжает вверх вместе с оглавлением ровно тогда,
    // когда по ней сверяются.
    let shelf_title = gtk::Label::builder()
        .label("Contents")
        .xalign(0.0)
        .margin_start(10)
        .margin_end(10)
        .margin_top(8)
        .margin_bottom(8)
        .build();
    shelf_title.add_css_class("shelf-title");
    ui.shelf_view.set_child(Some(&ui.contents));
    ui.shelf.append(&shelf_title);
    ui.shelf
        .append(&gtk::Separator::new(gtk::Orientation::Horizontal));
    ui.shelf.append(&ui.shelf_view);
    ui.shelf.add_css_class("shelf");
    ui.shelf_view.add_css_class("shelf");
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
    // Полоса — часть страницы, и фон у неё бумажный, а не окна.
    ui.bar.add_css_class("page");
    ui.back.set_sensitive(false);
    ui.forward.set_sensitive(false);

    let new_tab_button = gtk::Button::from_icon_name("tab-new-symbolic");
    new_tab_button.set_tooltip_text(Some("New tab (Ctrl+T)"));

    // Подсказки висят на самой строке, а не на окне: тогда GTK сам держит
    // их под ней при смене размера окна.
    let hint_pane = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .propagate_natural_height(true)
        .max_content_height(HINT_HEIGHT)
        .child(&ui.hint_list)
        .build();
    ui.hints.set_child(Some(&hint_pane));
    ui.hints.set_parent(&ui.entry);
    ui.hints.add_css_class("hints");
    ui.hint_list.add_css_class("hints");
    ui.history.set_tooltip_text(Some("History (Ctrl+H)"));

    let header = gtk::HeaderBar::builder().build();
    header.pack_start(&ui.back);
    header.pack_start(&ui.forward);
    header.pack_start(&ui.history);
    header.pack_start(&new_tab_button);
    header.pack_end(&ui.menu);
    header.pack_end(&ui.zoom_level);
    header.pack_end(&ui.show_contents);
    header.pack_end(&ui.save);
    header.pack_end(&ui.star);
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

    // Полоса прогресса (#19) — под статьёй, а не под полкой: она про текст.
    let reading_side = gtk::Box::new(gtk::Orientation::Vertical, 0);
    reading_side.append(&ui.notebook);
    reading_side.append(&ui.bar);
    ui.notebook.set_vexpand(true);
    ui.split.set_start_child(Some(&reading_side));
    ui.split.set_end_child(Some(&ui.shelf));

    // Поиск внизу, как в браузерах: строка приходит и уходит, и двигать
    // ради неё текст незачем.
    let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
    root.append(&ui.split);
    root.append(&ui.search);
    // Строка состояния и рядом — кнопка предложения, когда оно есть.
    let status = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    status.append(&ui.notice);
    status.append(&ui.offer);
    root.append(&status);
    ui.offer.add_css_class("flat");
    ui.window.set_child(Some(&root));
    ui.notice.add_css_class("caption");
    ui.save.set_tooltip_text(Some("Save the article (Ctrl+S)"));
    ui.star.set_tooltip_text(Some("Keep this page (Ctrl+D)"));

    let state = Rc::new(RefCell::new(State {
        tabs: Vec::new(),
        next_id: 0,
        dark: settings.dark,
        images: settings.images,
        archive: settings.archive,
        zoom: ZOOM_NORMAL,
        store: Store::open(),
        marks: Marks::open(),
        readings: Readings::open(),
        last_input: Instant::now(),
        bars: Vec::new(),
        offered: None,
        // Сессию поднимает и пишет первое окно процесса. Второе окно —
        // это «открой мне ещё одну ссылку», а не «вот мои вкладки».
        keeps_session: OWNS_SESSION.with(|first| first.replace(false)),
        hints: Vec::new(),
        search: Search::default(),
        shelf: Vec::new(),
    }));
    apply_theme(&ui, &state);
    reading_hooks(&ui, &state);
    {
        // Дверь снаружи: по ней приезжают адреса из второго запуска.
        // Действие принадлежит окну, а не приложению, — окон бывает
        // несколько, и адрес должен попасть в то, которое открыто сейчас.
        let window = ui.window.clone();
        let ui = ui.clone();
        let state = state.clone();
        let open = gio::SimpleAction::new(OPEN_ACTION, Some(glib::VariantTy::STRING));
        open.connect_activate(move |_, text| {
            let Some(address) = text
                .and_then(glib::Variant::str)
                .and_then(|text| address::parse(text).ok())
            else {
                return;
            };
            outside_tab(&ui, &state, address);
        });
        window.add_action(&open);
    }

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
                        show_message(&tab, problem.headline, &problem.detail, &[]);
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
    // Боковые кнопки мыши — «назад» и «вперёд», как в любом браузере (#34).
    // Восьмая и девятая: так их называют и X11, и Wayland. Каждый жест
    // слушает свою кнопку, поэтому левую и среднюю он не трогает.
    for (button, backwards) in [(8, true), (9, false)] {
        let window = ui.window.clone();
        let ui = ui.clone();
        let state = state.clone();
        let side = gtk::GestureClick::builder()
            .button(button)
            .propagation_phase(gtk::PropagationPhase::Capture)
            .build();
        side.connect_pressed(move |gesture, _, _, _| {
            gesture.set_state(gtk::EventSequenceState::Claimed);
            step(&ui, &state, backwards);
        });
        window.add_controller(side);
    }
    {
        // История открывается вкладкой, как `Ctrl+H` в хроме: читатель
        // пришёл за ней, не бросив того, что читает.
        let ui = ui.clone();
        let state = state.clone();
        ui.history.clone().connect_clicked(move |_| {
            new_tab(&ui, &state, Some(Address::Internal(Internal::History)));
        });
    }
    {
        // Печатают — показываем, куда он уже ходил.
        let ui = ui.clone();
        let state = state.clone();
        ui.entry
            .clone()
            .connect_changed(move |_| offer_hints(&ui, &state));
    }
    {
        // Ушли из строки — список убираем: он висит над текстом статьи.
        let ui = ui.clone();
        let focus = gtk::EventControllerFocus::new();
        focus.connect_leave(move |_| ui.hints.popdown());
        ui.entry.add_controller(focus);
    }
    {
        // Выбрали строку мышью.
        let ui = ui.clone();
        let state = state.clone();
        ui.hint_list.clone().connect_row_activated(move |_, row| {
            let chosen = state.borrow().hints.get(row.index() as usize).cloned();
            if let Some(address) = chosen {
                take_hint(&ui, &state, &address);
            }
        });
    }
    {
        // Клавиши в адресной строке. Перехват до самой строки (`Capture`):
        // иначе Enter уходит в неё и открывает напечатанное, а не выбранное.
        let entry = ui.entry.clone();
        let ui = ui.clone();
        let state = state.clone();
        let keys = gtk::EventControllerKey::new();
        keys.set_propagation_phase(gtk::PropagationPhase::Capture);
        keys.connect_key_pressed(move |_, key, _, _| {
            let shown = ui.hints.is_visible();
            match key {
                gtk::gdk::Key::Down if shown => walk_hints(&ui, 1),
                gtk::gdk::Key::Up if shown => walk_hints(&ui, -1),
                gtk::gdk::Key::Escape if shown => ui.hints.popdown(),
                gtk::gdk::Key::Return | gtk::gdk::Key::KP_Enter if shown => {
                    let chosen = ui
                        .hint_list
                        .selected_row()
                        .and_then(|row| state.borrow().hints.get(row.index() as usize).cloned());
                    match chosen {
                        Some(address) => take_hint(&ui, &state, &address),
                        // Ничего не выбрано — открывается напечатанное,
                        // и список просто уходит с дороги.
                        None => {
                            ui.hints.popdown();
                            return glib::Propagation::Proceed;
                        }
                    }
                }
                _ => return glib::Propagation::Proceed,
            }
            glib::Propagation::Stop
        });
        entry.add_controller(keys);
    }
    {
        // Всплывающее окно обязано отцепиться от строки раньше, чем строку
        // разберут: иначе GTK жалуется на виджет с ребёнком при разборке.
        let hints = ui.hints.clone();
        ui.entry.clone().connect_destroy(move |_| hints.unparent());
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
        ui.save
            .clone()
            .connect_clicked(move |_| ask_where_to_save(&ui, &state));
    }
    {
        let ui = ui.clone();
        let state = state.clone();
        ui.star
            .clone()
            .connect_clicked(move |_| keep_page(&ui, &state));
    }
    {
        let ui = ui.clone();
        let state = state.clone();
        ui.zoom_level
            .clone()
            .connect_clicked(move |_| zoom_by(&ui, &state, 0));
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
        let ui = ui.clone();
        let state = state.clone();
        ui.show_contents.clone().connect_toggled(move |button| {
            ui.shelf
                .set_visible(button.is_active() && button.is_sensitive());
            fit_shelf(&ui);
            remember_settings(&ui, &state);
        });
    }
    {
        // Делитель подвинули — запоминаем ширину полки, а не позицию.
        //
        // Пока мы позицию не поставили, ею распоряжается GTK, и полка
        // у него схлопнута до `width_request`: принять это за выбор
        // читателя значит выбор потерять.
        let ui = ui.clone();
        ui.split.clone().connect_position_notify(move |split| {
            if !ui.shelf.is_visible() || !split.is_position_set() {
                return;
            }
            let width = split.width();
            if width > 0 {
                ui.shelf_width
                    .set((width - split.position()).clamp(TOC_MIN, TOC_MAX));
            }
        });
    }
    {
        // Строка полки делает одно из двух: прокручивает открытый документ
        // или уводит в другой файл проекта. Что именно — знает состояние,
        // а обработчик один на всю жизнь окна.
        let ui = ui.clone();
        let state = state.clone();
        ui.contents.clone().connect_row_activated(move |_, row| {
            let act = state
                .borrow()
                .shelf
                .get(row.index().max(0) as usize)
                .cloned();
            match act {
                Some(Row::Jump(offset)) => {
                    if let Some(view) = current(&ui, &state) {
                        // Доводим прокрутку до конца, а не прыгаем один раз:
                        // пока картинки и таблицы добирают высоту, одиночный
                        // прыжок промахивается мимо заголовка — и на полке
                        // загорается соседний пункт.
                        settle(&view, offset, ANCHOR_ALIGN);
                    }
                }
                Some(Row::Open(address)) => open_current(&ui, &state, address, true),
                Some(Row::Header) | None => {}
            }
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
            glib::idle_add_local_once(move || {
                sync(&ui, &state, None);
                wake_tab(&ui, &state);
                resume_place(&ui, &state);
                rezoom_current(&ui, &state);
                remember_session(&ui, &state);
            });
        });
    }
    {
        // Окно переехало на экран другой плотности: картинки открытой вкладки
        // разобраны под прежнюю — перерисовать (#15). Фоновые догонят при показе.
        let ui = ui.clone();
        let state = state.clone();
        ui.window.clone().connect_scale_factor_notify(move |_| {
            rezoom_current(&ui, &state);
        });
    }

    keyboard(&ui, &state, app);

    // ── стартовые вкладки: по адресу на каждый аргумент
    let addresses: Vec<Address> = start
        .iter()
        .filter_map(|text| address::parse(text).ok())
        .collect();
    if addresses.is_empty() {
        // Названного адреса нет — значит окно открывают «просто так»,
        // и вернуть надо то, что в нём было. Названный адрес сессию
        // не поднимает: попросили страницу, а не вчерашний день.
        let restored = state.borrow().keeps_session && restore_session(&ui, &state);
        if !restored {
            new_tab(&ui, &state, None);
        }
    } else {
        for address in addresses {
            outside_tab(&ui, &state, address);
        }
        // Открываем первую: читатель просил их в этом порядке, а не наоборот.
        ui.notebook.set_current_page(Some(0));
    }

    {
        // Место в тексте меняется молча, без событий, — значит последний
        // снимок надо взять ровно перед тем, как окно закроется.
        let ui = ui.clone();
        let state = state.clone();
        ui.window.clone().connect_close_request(move |_| {
            remember_session(&ui, &state);
            // Ширина полки — тоже решение читателя, но извещение о ней
            // приходит на каждый пиксель перетаскивания: писать файл там
            // значит писать его сотню раз на один жест.
            remember_settings(&ui, &state);
            glib::Propagation::Proceed
        });
    }

    ui.window.present();
}

/// Открыть настройки вкладкой. Раньше это было отдельное окно; теперь —
/// страница внутри блокнота, чтобы всё жило в одном окне.
///
/// Вкладка одна: если она уже открыта, просто переключаемся на неё. Иначе
/// у окна оказались бы два экземпляра одних и тех же переключателей, а виджет
/// GTK живёт ровно в одном месте.
fn open_settings_tab(ui: &Ui, state: &Rc<RefCell<State>>) {
    if let Some(index) = state.borrow().tabs.iter().position(|tab| tab.settings) {
        ui.notebook.set_current_page(Some(index as u32));
        return;
    }

    let page = settings_page(ui, state);
    let scroller = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .hexpand(true)
        .vexpand(true)
        .child(&page)
        .build();
    // Тот же тон, что у страницы: холодная панель GTK рядом со слоновой
    // костью выдавала бы склейку из двух окон.
    scroller.add_css_class("page");
    page.add_css_class("page");

    let label = gtk::Label::builder()
        .label("Settings")
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

    // Вкладке настроек нужен `view` как всякой вкладке, но статьи в ней нет —
    // это заглушка, которую никто не показывает. Функции, которым она без
    // разницы (сессия, масштаб, догрузка картинок), отсекают её по `settings`.
    let id = {
        let mut borrowed = state.borrow_mut();
        let id = borrowed.next_id;
        borrowed.next_id += 1;
        borrowed.tabs.push(Tab {
            id,
            view: gtk::TextView::new(),
            label: label.clone(),
            history: History::new(),
            links: Vec::new(),
            focus: None,
            marks: Vec::new(),
            anchors: Vec::new(),
            shots: Vec::new(),
            site: Vec::new(),
            feeds: Vec::new(),
            entries: Vec::new(),
            entries_for: None,
            document: None,
            generation: 0,
            loading: false,
            resume: None,
            copy: None,
            progress: None,
            fresh_visit: false,
            pending: false,
            zoom_seen: ZOOM_NORMAL,
            density_seen: 1,
            pages: HashMap::new(),
            blobs: Blobs::default(),
            settings: true,
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
    {
        let ui = ui.clone();
        let state = state.clone();
        let middle = gtk::GestureClick::builder()
            .button(gtk::gdk::BUTTON_MIDDLE)
            .build();
        middle.connect_pressed(move |gesture, _, _, _| {
            gesture.set_state(gtk::EventSequenceState::Claimed);
            let index = state.borrow().tabs.iter().position(|tab| tab.id == id);
            if let Some(index) = index {
                close_tab(&ui, &state, index);
            }
        });
        corner.add_controller(middle);
    }

    sync(ui, state, None);
}

/// Содержимое страницы настроек: переключатели того, что читатель решает раз
/// и надолго. Строится заново на каждое открытие — переключатели тут свои,
/// поэтому закрыть вкладку можно как любую другую, не разбирая виджеты руками.
///
/// У каждой настройки есть причина, которую надо объяснить строкой, — в кнопку
/// с иконкой такое не помещается, потому это страница, а не значок в панели.
fn settings_page(ui: &Ui, state: &Rc<RefCell<State>>) -> gtk::Box {
    let (dark_on, images_on, archive_on) = {
        let borrowed = state.borrow();
        (borrowed.dark, borrowed.images, borrowed.archive)
    };
    let keep_copies = gtk::Switch::builder()
        .valign(gtk::Align::Center)
        .active(archive_on)
        .build();
    {
        let ui = ui.clone();
        let state = state.clone();
        keep_copies.connect_active_notify(move |switch| {
            state.borrow_mut().archive = switch.is_active();
            remember_settings(&ui, &state);
        });
    }
    let dark_mode = gtk::Switch::builder()
        .valign(gtk::Align::Center)
        .active(dark_on)
        .build();
    let show_images = gtk::Switch::builder()
        .valign(gtk::Align::Center)
        .active(images_on)
        .build();
    {
        let ui = ui.clone();
        let state = state.clone();
        dark_mode.connect_active_notify(move |switch| {
            state.borrow_mut().dark = switch.is_active();
            apply_theme(&ui, &state);
            remember_settings(&ui, &state);
        });
    }
    {
        let ui = ui.clone();
        let state = state.clone();
        show_images.connect_active_notify(move |switch| {
            state.borrow_mut().images = switch.is_active();
            if switch.is_active() {
                show_all_shots(&ui, &state);
            }
            remember_settings(&ui, &state);
        });
    }

    let page = gtk::Box::new(gtk::Orientation::Vertical, 12);
    page.set_margin_top(18);
    page.set_margin_bottom(18);
    page.set_margin_start(18);
    page.set_margin_end(18);

    let title = gtk::Label::builder().label("Reading").xalign(0.0).build();
    title.add_css_class("shelf-title");
    page.append(&title);

    let rows = gtk::ListBox::builder()
        .selection_mode(gtk::SelectionMode::None)
        .build();
    rows.add_css_class("rich-list");
    rows.append(&setting_row(
        "Dark theme",
        "Warm dark, in the same row as the ivory paper.",
        &dark_mode,
    ));
    rows.append(&setting_row(
        "Images",
        "Off means no decoding at all: after JavaScript is gone, the image \
         decoder is the one serious attack surface left.",
        &show_images,
    ));
    page.append(&rows);

    let kept = gtk::Label::builder().label("History").xalign(0.0).build();
    kept.add_css_class("shelf-title");
    page.append(&kept);

    // Действие, а не настройка, и место ему всё же здесь: вычистить историю
    // хотят раз в полгода, а панель шапки — для того, что нужно на каждой
    // странице. Сама страница истории на эту кнопку и показывает.
    let forget = gtk::Button::builder()
        .label("Forget")
        .valign(gtk::Align::Center)
        .build();
    forget.add_css_class("destructive-action");
    let history_rows = gtk::ListBox::builder()
        .selection_mode(gtk::SelectionMode::None)
        .build();
    history_rows.add_css_class("rich-list");
    history_rows.append(&setting_row(
        "Keep a copy of every page",
        "Each article you read is kept as a compressed Markdown file in \
         brevier:archive, so you can read and search it even when the site is \
         gone. Off: new pages are not kept; what is there stays.",
        &keep_copies,
    ));
    history_rows.append(&action_row(
        "Forget everything you have read",
        "The list at brevier:history goes away, the address bar stops \
         suggesting those pages, and the saved copies of pages and the \
         archive are deleted. Bookmarks and open tabs stay.",
        &forget,
    ));
    page.append(&history_rows);
    {
        let ui = ui.clone();
        let state = state.clone();
        forget.connect_clicked(move |_| forget_everything(&ui, &state));
    }

    page
}

/// Строка настройки: что делает, почему так и сам переключатель.
/// Строка с кнопкой вместо переключателя: то же место и тот же вид,
/// но действие, а не состояние.
fn action_row(title: &str, why: &str, button: &gtk::Button) -> gtk::ListBoxRow {
    setting_row(title, why, button)
}

fn setting_row(title: &str, why: &str, switch: &impl IsA<gtk::Widget>) -> gtk::ListBoxRow {
    let text = gtk::Box::new(gtk::Orientation::Vertical, 2);
    text.set_hexpand(true);
    let name = gtk::Label::builder().label(title).xalign(0.0).build();
    let note = gtk::Label::builder()
        .label(why)
        .xalign(0.0)
        .wrap(true)
        .max_width_chars(44)
        .build();
    note.add_css_class("caption");
    text.append(&name);
    text.append(&note);

    let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    row.set_margin_top(10);
    row.set_margin_bottom(10);
    row.set_margin_start(12);
    row.set_margin_end(12);
    row.append(&text);
    row.append(switch);

    gtk::ListBoxRow::builder()
        .child(&row)
        .activatable(false)
        .build()
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

    // Масштаб страницы. Лестница и сочетания знакомые — читатель приходит
    // с ними из браузера и не разбирается заново.
    for (name, step, keys) in [
        (
            "zoom-in",
            1,
            &["<Control>plus", "<Control>equal", "<Control>KP_Add"][..],
        ),
        (
            "zoom-out",
            -1,
            &["<Control>minus", "<Control>KP_Subtract"][..],
        ),
        ("zoom-reset", 0, &["<Control>0", "<Control>KP_0"][..]),
    ] {
        let zoom = gio::SimpleAction::new(name, None);
        let ui = ui.clone();
        let state = state.clone();
        zoom.connect_activate(move |_, _| zoom_by(&ui, &state, step));
        add(name, keys, zoom);
    }

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

    let history = gio::SimpleAction::new("history", None);
    {
        let ui = ui.clone();
        let state = state.clone();
        history.connect_activate(move |_, _| {
            new_tab(&ui, &state, Some(Address::Internal(Internal::History)));
        });
    }
    add("history", &["<Control>h"], history);

    // Закладки и настройки — соседи истории в выпадающем меню шапки.
    // У закладок клавиши нет намеренно: их ставят звёздочкой, а список
    // открывают из меню; у настроек — тоже, их трогают редко.
    let bookmarks = gio::SimpleAction::new("bookmarks", None);
    {
        let ui = ui.clone();
        let state = state.clone();
        bookmarks.connect_activate(move |_, _| {
            new_tab(&ui, &state, Some(Address::Internal(Internal::Bookmarks)));
        });
    }
    add("bookmarks", &[], bookmarks);

    // Архив (#8) — сосед истории: тот же журнал, только с текстом страниц.
    let archive = gio::SimpleAction::new("archive", None);
    {
        let ui = ui.clone();
        let state = state.clone();
        archive.connect_activate(move |_, _| {
            new_tab(
                &ui,
                &state,
                Some(Address::Internal(Internal::Archive(ArchivePage::List))),
            );
        });
    }
    add("archive", &[], archive);

    // Поиск по архиву — адресом `brevier:archive?q=`: форм у Brevier нет,
    // а строка адреса уже умеет и набор, и подсказки, и Enter. Клавиша
    // ставит туда начало адреса и курсор за ним — остаётся набрать слова.
    let search_archive = gio::SimpleAction::new("search-archive", None);
    {
        let ui = ui.clone();
        search_archive.connect_activate(move |_, _| {
            ui.entry.set_text(ARCHIVE_SEARCH);
            ui.entry.grab_focus_without_selecting();
            ui.entry.set_position(-1);
        });
    }
    add("search-archive", &["<Control><Shift>f"], search_archive);

    let settings = gio::SimpleAction::new("settings", None);
    {
        let ui = ui.clone();
        let state = state.clone();
        settings.connect_activate(move |_, _| open_settings_tab(&ui, &state));
    }
    add("settings", &[], settings);

    // Проверка открытой страницы (`brevier:check/…`) — новой вкладкой:
    // отчёт читают рядом со страницей, а не вместо неё. Проверяют веб;
    // на проверке это перепроверка.
    let check = gio::SimpleAction::new("check", None);
    {
        let ui = ui.clone();
        let state = state.clone();
        check.connect_activate(move |_, _| {
            let target = ui.notebook.current_page().and_then(|index| {
                state
                    .borrow()
                    .tabs
                    .get(index as usize)?
                    .history
                    .current()
                    .and_then(Address::check)
            });
            match target {
                Some(address) => new_tab(&ui, &state, Some(address)),
                None => notice(&ui, "Only a web page can be checked"),
            }
        });
    }
    add("check", &[], check);

    // Перезагрузка — мимо памяти вкладки и мимо недельной копии: читатель
    // просит страницу такой, какая она на сайте сейчас. Место чтения
    // остаётся, как у браузеров.
    let reload = gio::SimpleAction::new("reload", None);
    {
        let ui = ui.clone();
        let state = state.clone();
        reload.connect_activate(move |_, _| reload_current(&ui, &state));
    }
    add("reload", &["<Control>r", "F5"], reload);

    // Само меню под кнопкой в шапке. Сверху — про открытую страницу, под
    // чертой — места программы в том порядке, как просили: настройки,
    // история, закладки.
    let page = gio::Menu::new();
    page.append(Some("Reload"), Some("app.reload"));
    page.append(Some("Check this page"), Some("app.check"));
    let places = gio::Menu::new();
    places.append(Some("Settings"), Some("app.settings"));
    places.append(Some("History"), Some("app.history"));
    places.append(Some("Bookmarks"), Some("app.bookmarks"));
    places.append(Some("Archive"), Some("app.archive"));
    let menu = gio::Menu::new();
    menu.append_section(None, &page);
    menu.append_section(None, &places);
    ui.menu.set_menu_model(Some(&menu));

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
            // Своей страницы снаружи нет: у неё и адреса-то наружу нет
            // (`Address::external` пуст), и отдавать чужому браузеру нечего.
            let target = current_target(&ui, &state);
            match target {
                Some(target) if !open_in_system_browser(&target) => {
                    notice(&ui, "No other browser is registered for links");
                }
                _ => {}
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

    let keep = gio::SimpleAction::new("keep", None);
    {
        let ui = ui.clone();
        let state = state.clone();
        keep.connect_activate(move |_, _| keep_page(&ui, &state));
    }
    add("keep", &["<Control>d"], keep);

    let save = gio::SimpleAction::new("save", None);
    {
        let ui = ui.clone();
        let state = state.clone();
        save.connect_activate(move |_, _| ask_where_to_save(&ui, &state));
    }
    add("save", &["<Control>s"], save);
}

/// Загрузить открытую страницу заново, из сети, на том же месте.
fn reload_current(ui: &Ui, state: &Rc<RefCell<State>>) {
    let Some(index) = ui.notebook.current_page() else {
        return;
    };
    let target = {
        let mut borrowed = state.borrow_mut();
        let Some(tab) = borrowed.tabs.get_mut(index as usize) else {
            return;
        };
        if tab.settings {
            return;
        }
        let here = top_of(&tab.view);
        tab.resume = (here > 0).then_some(here);
        tab.history
            .current()
            .cloned()
            .map(|address| (tab.id, address))
    };
    if let Some((id, address)) = target {
        open_with(ui, state, id, address, false, true);
    }
}

/// Открыть новую вкладку и, если дали адрес, сразу читать.
fn new_tab(ui: &Ui, state: &Rc<RefCell<State>>, address: Option<Address>) {
    add_tab(ui, state, address, History::new());
}

/// Вкладка под адрес из другой программы — из чата, почты, второго
/// запуска: «назад» с этой страницы гаснет, начальной за ней нет.
fn outside_tab(ui: &Ui, state: &Rc<RefCell<State>>, address: Address) {
    add_tab(ui, state, Some(address), History::from_outside());
}

fn add_tab(ui: &Ui, state: &Rc<RefCell<State>>, address: Option<Address>, history: History) {
    // Виджет статьи — свой: `GtkTextView`, который дорисовывает линейку
    // слева от цитаты. Настраиваем его уже как `TextView`, чтобы не спорить
    // с одноимёнными методами других интерфейсов GTK.
    let article = Article::new();
    article.set_rule_color(rule_color(state.borrow().dark));
    let view: gtk::TextView = article.upcast();
    view.set_editable(false);
    view.set_cursor_visible(false);
    // По словам, а слово длиннее строки — внутри него. Одних слов мало:
    // адрес, отпечаток ключа, строка JSON в блоке кода не рвутся нигде,
    // вылезают за меру, и вид становится шире своей колонки — сначала
    // раздвигал её, а после начальной прокрутки сдвигал текст вбок, срезая
    // начала строк. Мягкие переносы по языку остаются первыми кандидатами.
    view.set_wrap_mode(gtk::WrapMode::WordChar);
    view.set_halign(gtk::Align::Center);
    view.set_top_margin(28);
    view.set_bottom_margin(80);
    tags(&view.buffer(), state.borrow().dark, ZOOM_STEPS[ZOOM_NORMAL]);
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
            history,
            links: Vec::new(),
            focus: None,
            marks: Vec::new(),
            anchors: Vec::new(),
            shots: Vec::new(),
            site: Vec::new(),
            feeds: Vec::new(),
            entries: Vec::new(),
            entries_for: None,
            document: None,
            generation: 0,
            loading: false,
            resume: None,
            copy: None,
            progress: None,
            fresh_visit: false,
            pending: false,
            zoom_seen: ZOOM_NORMAL,
            density_seen: 1,
            pages: HashMap::new(),
            blobs: Blobs::default(),
            settings: false,
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

    {
        // Средняя кнопка по корешку закрывает вкладку — привычка из браузеров.
        // Отдельным жестом ровно на средней кнопке: левый клик должен
        // по-прежнему доставаться блокноту и переключать вкладку.
        let ui = ui.clone();
        let state = state.clone();
        let middle = gtk::GestureClick::builder()
            .button(gtk::gdk::BUTTON_MIDDLE)
            .build();
        middle.connect_pressed(move |gesture, _, _, _| {
            gesture.set_state(gtk::EventSequenceState::Claimed);
            let index = state.borrow().tabs.iter().position(|tab| tab.id == id);
            if let Some(index) = index {
                close_tab(&ui, &state, index);
            }
        });
        corner.add_controller(middle);
    }

    {
        // Читатель прокрутил страницу — полка отмечает, куда он доехал.
        let ui = ui.clone();
        let state = state.clone();
        scroller.vadjustment().connect_value_changed(move |_| {
            if current_id(&ui, &state) == Some(id) {
                touched(&state);
                follow(&ui, &state);
                ui.bar.queue_draw();
            }
        });
    }
    {
        // Время чтения идёт, пока читатель здесь (#19): листает, жмёт
        // клавиши, водит мышью по тексту. Сами события не трогаем.
        let keys = gtk::EventControllerKey::new();
        let state2 = state.clone();
        keys.connect_key_pressed(move |_, _, _, _| {
            touched(&state2);
            glib::Propagation::Proceed
        });
        view.add_controller(keys);
        let motion = gtk::EventControllerMotion::new();
        let state2 = state.clone();
        motion.connect_motion(move |_, _, _| touched(&state2));
        view.add_controller(motion);
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
            let primary = gesture.current_button() == gtk::gdk::BUTTON_PRIMARY;
            if !middle && !primary {
                return;
            }
            let target = {
                let mut borrowed = state.borrow_mut();
                let Some(tab) = borrowed.find(id) else { return };
                let Some(target) = link_at(&view, &tab.links, x, y).map(|link| link.target.clone())
                else {
                    return;
                };
                target
            };
            follow_link(&ui, &state, id, &view, &target, ctrl || middle);
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

    // Наведение на ссылку — нативная подсказка с её адресом: видно, куда ведёт,
    // ещё до нажатия. То же, что строка состояния браузера, только у курсора.
    view.set_has_tooltip(true);
    {
        let state = state.clone();
        let view = view.clone();
        view.clone()
            .connect_query_tooltip(move |_, x, y, _keyboard, tooltip| {
                // GTK дёргает этот обработчик в произвольный момент — в том
                // числе изнутри `set_tooltip_text` на соседнем виджете, пока
                // мы держим `borrow_mut` в `show_document`. Поэтому `try_borrow`,
                // а не `borrow`: занят — молча пропускаем, подсказка не срочная
                // и покажется на следующем наведении.
                let target = state.try_borrow().ok().and_then(|state| {
                    state.tabs.iter().find(|tab| tab.id == id).and_then(|tab| {
                        link_at(&view, &tab.links, x as f64, y as f64)
                            .map(|link| link.target.clone())
                    })
                });
                match target {
                    Some(target) => {
                        tooltip.set_text(Some(&target));
                        true
                    }
                    None => false,
                }
            });
    }

    // Клавиши прокрутки висят на тексте, а не на окне: иначе пробел
    // и стрелки ломали бы набор адреса в строке.
    let keys = gtk::EventControllerKey::new();
    {
        let scroller = scroller.clone();
        let ui = ui.clone();
        let state = state.clone();
        let view = view.clone();
        keys.connect_key_pressed(move |_, key, _, modifiers| {
            if modifiers.contains(gtk::gdk::ModifierType::CONTROL_MASK) {
                return glib::Propagation::Proceed;
            }
            let adjustment = scroller.vadjustment();
            let page = adjustment.page_size();
            let step = page / 10.0;
            // Пробел в самом конце листал бы в пустоту — он открывает
            // следующую страницу, если страница её назвала (#35): та же
            // клавиша, что листала до сих пор, листает и дальше.
            let at_end = adjustment.value() + page >= adjustment.upper() - 1.0;
            if key == gtk::gdk::Key::space && at_end {
                let next = state.borrow_mut().find(id).and_then(|tab| {
                    tab.document
                        .as_ref()
                        .and_then(|document| document.next.as_ref())
                        .map(|next| next.address.clone())
                });
                if let Some(next) = next {
                    follow_link(&ui, &state, id, &view, &next, false);
                    return glib::Propagation::Stop;
                }
            }
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

    // Ссылки с клавиатуры (#34): Tab и Shift+Tab ходят по ссылкам статьи,
    // начиная с видимых, Enter открывает, Ctrl+Enter — вкладкой, Escape
    // снимает отметку. За последней ссылкой Tab уходит дальше по окну —
    // в шапку и на полку, как в браузере.
    let link_keys = gtk::EventControllerKey::new();
    {
        let ui = ui.clone();
        let state = state.clone();
        let view = view.clone();
        link_keys.connect_key_pressed(move |_, key, _, modifiers| {
            let ctrl = modifiers.contains(gtk::gdk::ModifierType::CONTROL_MASK);
            let shift = modifiers.contains(gtk::gdk::ModifierType::SHIFT_MASK);
            let handled = match key {
                gtk::gdk::Key::Tab | gtk::gdk::Key::KP_Tab if !ctrl => {
                    move_link_focus(&state, id, &view, !shift)
                }
                gtk::gdk::Key::ISO_Left_Tab if !ctrl => move_link_focus(&state, id, &view, false),
                gtk::gdk::Key::Return | gtk::gdk::Key::KP_Enter => {
                    let target = state.borrow_mut().find(id).and_then(|tab| {
                        tab.focus
                            .and_then(|at| tab.links.get(at))
                            .map(|link| link.target.clone())
                    });
                    match target {
                        Some(target) => {
                            follow_link(&ui, &state, id, &view, &target, ctrl);
                            true
                        }
                        None => false,
                    }
                }
                gtk::gdk::Key::Escape => drop_link_focus(&state, id, &view),
                _ => false,
            };
            if handled {
                glib::Propagation::Stop
            } else {
                glib::Propagation::Proceed
            }
        });
    }
    view.add_controller(link_keys);

    // Масштаб колесом с Ctrl, как в браузере. Шаги копим: у мыши одно
    // движение колеса это ровно единица, а тачпад сыплет долями, и без
    // накопления страница улетала бы на край лестницы от одного жеста.
    let wheel = gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::VERTICAL);
    {
        let ui = ui.clone();
        let state = state.clone();
        let spun = Cell::new(0.0f64);
        wheel.connect_scroll(move |controller, _, dy| {
            if !controller
                .current_event_state()
                .contains(gtk::gdk::ModifierType::CONTROL_MASK)
            {
                return glib::Propagation::Proceed;
            }
            let total = spun.get() + dy;
            if total.abs() >= 1.0 {
                spun.set(0.0);
                // Колесо от себя — ближе, как и везде.
                zoom_by(&ui, &state, if total < 0.0 { 1 } else { -1 });
            } else {
                spun.set(total);
            }
            glib::Propagation::Stop
        });
    }
    view.add_controller(wheel);

    // Копирование снимает типографские знаки: в буфере лежат мягкие переносы
    // и неразрывные пробелы, а читателю в буфер обмена нужен чистый текст.
    // После штатного копирования переписываем буфер обмена очищенным текстом
    // (`copy-clipboard`, обработчик — после умолчания). Ctrl+C и «Копировать»
    // из контекстного меню идут через тот же сигнал.
    view.connect_closure(
        "copy-clipboard",
        true,
        glib::closure_local!(move |view: gtk::TextView| {
            let buffer = view.buffer();
            if let Some((start, end)) = buffer.selection_bounds() {
                let selected = buffer.text(&start, &end, false).to_string();
                if brevier::typeset::marked(&selected) {
                    view.clipboard()
                        .set_text(&brevier::typeset::plain(&selected));
                }
            }
        }),
    );

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
    // Уходя, записываем прочитанное (#19) и бросаем загрузку: слушать её
    // больше некому.
    let id = state.borrow().tabs[index].id;
    leave_page(state, id);
    state.borrow_mut().tabs.remove(index).generation = u64::MAX;
    ui.notebook.remove_page(Some(index as u32));

    // Окно без вкладок показывать нечем — заводим чистую.
    if state.borrow().tabs.is_empty() {
        new_tab(ui, state, None);
        // Закрыли всё до одной — это тоже решение читателя, и сессия
        // обязана стать пустой, а не помнить закрытое.
        remember_session(ui, state);
        return;
    }
    ui.notebook.set_show_tabs(ui.notebook.n_pages() > 1);
    sync(ui, state, None);
    remember_session(ui, state);
}

/// Какая вкладка открыта. Прокрутка фоновой вкладки полку не трогает.
fn current_id(ui: &Ui, state: &Rc<RefCell<State>>) -> Option<u64> {
    let index = ui.notebook.current_page()? as usize;
    state.try_borrow().ok()?.tabs.get(index).map(|tab| tab.id)
}

fn current(ui: &Ui, state: &Rc<RefCell<State>>) -> Option<gtk::TextView> {
    let index = ui.notebook.current_page()? as usize;
    state.borrow().tabs.get(index).map(|tab| tab.view.clone())
}

/// «Loading» с точками. Точка прибавляется раз в секунду до десяти
/// и начинается заново: это не индикатор доли — долю мы не знаем,
/// `ureq` синхронный и о ходе загрузки не рассказывает, — а признак жизни.
/// Большая страница едет секунды, и неподвижная надпись всё это время
/// выглядит как зависшая программа.
fn show_loading(view: &gtk::TextView, dots: usize) {
    show_message(view, &format!("Loading{}", ".".repeat(dots)), "", &[]);
}

/// Заводить часы на время загрузки. Останавливаются сами: вкладку закрыли,
/// страница приехала или читатель ушёл на другую — во всех трёх случаях
/// показывать точки больше некому.
fn tick_loading(state: &Rc<RefCell<State>>, id: u64, generation: u64, view: gtk::TextView) {
    let state = state.clone();
    let dots = Cell::new(1usize);
    glib::timeout_add_local(Duration::from_secs(1), move || {
        let alive = state
            .borrow_mut()
            .find(id)
            .is_some_and(|tab| tab.loading && tab.generation == generation);
        if !alive {
            return glib::ControlFlow::Break;
        }
        dots.set(dots.get() % LOADING_DOTS + 1);
        show_loading(&view, dots.get());
        glib::ControlFlow::Continue
    });
}

/// Вернуть вкладку туда, где её застали в прошлый раз.
///
/// Делается, только когда вкладка на экране и уже загрузилась:
/// `GtkTextView` невидимой страницы блокнота раскладки не считает,
/// и прокрутка в ней уходит в никуда. Поэтому место ждёт своего часа
/// во вкладке (`resume`) — восстановленная вкладка, куда за весь сеанс
/// так и не заглянули, унесёт своё место в следующую сессию нетронутым.
///
/// `settle`, а не один прыжок: картинки и таблицы добирают высоту
/// не сразу, и ранний прыжок промахивается — та же причина, что и у прыжка
/// по якорю.
fn resume_place(ui: &Ui, state: &Rc<RefCell<State>>) {
    let Some(index) = ui.notebook.current_page() else {
        return;
    };
    let ready = {
        let mut borrowed = state.borrow_mut();
        let Some(tab) = borrowed.tabs.get_mut(index as usize) else {
            return;
        };
        if tab.loading || tab.document.is_none() {
            return;
        }
        match tab.resume.take() {
            Some(place) if place > 0 => Some((tab.view.clone(), place)),
            _ => None,
        }
    };
    if let Some((view, place)) = ready {
        settle(&view, place, 0.0);
    }
}

/// Отметить открытую страницу — или снять отметку.
///
/// Про внутренние страницы (сама история, сами закладки) отметки не бывает:
/// класть список в список незачем, и звёздочка на них гаснет.
fn keep_page(ui: &Ui, state: &Rc<RefCell<State>>) {
    let Some(index) = ui.notebook.current_page() else {
        return;
    };
    let page = {
        let borrowed = state.borrow();
        let Some(tab) = borrowed.tabs.get(index as usize) else {
            return;
        };
        let title = tab
            .document
            .as_ref()
            .map(|document| document.title.clone())
            .unwrap_or_default();
        tab.history
            .current()
            .filter(|address| !address.is_internal())
            .cloned()
            .map(|address| (address, title))
    };
    let Some((address, title)) = page else { return };

    let kept = {
        let mut borrowed = state.borrow_mut();
        let offset = local_offset();
        borrowed.marks.toggle(&address, &title, offset)
    };
    show_star(ui, kept);
    notice(
        ui,
        if kept {
            "Kept — the list is at brevier:bookmarks"
        } else {
            "Taken off the bookmarks"
        },
    );
}

/// Зажечь или погасить звёздочку.
fn show_star(ui: &Ui, kept: bool) {
    ui.star.set_icon_name(if kept {
        "starred-symbolic"
    } else {
        "non-starred-symbolic"
    });
}

/// Забыть всё прочитанное — с вопросом, потому что назад это не отыграть.
fn forget_everything(ui: &Ui, state: &Rc<RefCell<State>>) {
    let dialog = gtk::AlertDialog::builder()
        .message("Forget everything you have read?")
        .detail(
            "The list of pages goes away, the address bar stops suggesting them, \
             and the saved copies of pages and the archive are deleted. Bookmarks \
             and open tabs stay.",
        )
        .buttons(["Cancel", "Forget"])
        .cancel_button(0)
        .default_button(0)
        .modal(true)
        .build();

    let window = ui.window.clone();
    let ui = ui.clone();
    let state = state.clone();
    dialog.choose(Some(&window), gio::Cancellable::NONE, move |answer| {
        if answer != Ok(1) {
            return;
        }
        state.borrow_mut().store.forget();
        // Копии страниц — такой же след прочитанного, как журнал. Память
        // вкладок не трогаем: открытые вкладки остаются, как и обещано.
        brevier::cache::Cache::open().forget();
        // Архив — тоже след прочитанного (#8): «забыть всё» значит всё.
        brevier::archive::Archive::open().forget();
        // И что прочитано на страницах (#19): файл и счёт в открытых вкладках.
        {
            let mut borrowed = state.borrow_mut();
            borrowed.readings.forget();
            for tab in &mut borrowed.tabs {
                if let Some(progress) = tab.progress.as_mut() {
                    progress.reset();
                }
            }
        }
        update_bars(&ui, &state);
        // А вот недавнее на начальных страницах — тот же журнал, и оставить
        // его на экране значило бы не забыть.
        let starts: Vec<(u64, gtk::TextView)> = state
            .borrow()
            .tabs
            .iter()
            .filter(|tab| !tab.settings && !tab.pending && tab.history.current().is_none())
            .map(|tab| (tab.id, tab.view.clone()))
            .collect();
        for (id, view) in starts {
            show_intro(&ui, &state, id, &view);
        }
        notice(&ui, "The list of pages you have read is empty now");
    });
}

/// Запомнить решения читателя: тему, картинки, полку и её ширину.
///
/// Пишут их все окна, а не одно: в отличие от сессии, это одни и те же
/// значения, и затирать друг другу тут нечего.
fn remember_settings(ui: &Ui, state: &Rc<RefCell<State>>) {
    let borrowed = state.borrow();
    Settings {
        dark: borrowed.dark,
        images: borrowed.images,
        shelf: ui.show_contents.is_active(),
        shelf_width: Some(ui.shelf_width.get()),
        archive: borrowed.archive,
    }
    .save();
}

/// Запомнить открытое: вкладки, их путь и место в тексте.
///
/// Зовётся на каждое событие, которое меняет состав окна, — открыли,
/// закрыли, перешли, переключились. Файл маленький, запись целиком, так что
/// дешевле ловить момент «читатель закрыл окно» и надёжнее его же.
fn remember_session(ui: &Ui, state: &Rc<RefCell<State>>) {
    let borrowed = state.borrow();
    if !borrowed.keeps_session {
        return;
    }
    // Вкладку настроек в сессию не пишем: это временный экран, а не место,
    // куда читатель вернётся при следующем запуске. Текущей она поэтому и
    // помечается по идентификатору, а не по индексу блокнота — иначе, стоя
    // на настройках, читатель сбил бы отметку «текущей» соседней статье.
    let current_id = ui
        .notebook
        .current_page()
        .and_then(|index| borrowed.tabs.get(index as usize))
        .map(|tab| tab.id);
    let tabs: Vec<store::Opened> = borrowed
        .tabs
        .iter()
        .filter(|tab| !tab.settings)
        .map(|tab| store::Opened {
            addresses: tab.history.entries().iter().map(Address::display).collect(),
            // Вкладка, оставленная на начальной странице, возвращается
            // на первую страницу своего пути: пустых вкладок сессия
            // не поднимает, и путь пропал бы вместе с ней.
            at: tab.history.at().unwrap_or(0),
            // Место, которое ещё не применили, старше того, что показывает
            // виджет: у вкладки, которая пока не грузилась или не была
            // на экране, он честно отвечает «ноль», и этим нулём мы бы
            // затёрли настоящее место.
            place: tab.resume.unwrap_or_else(|| top_of(&tab.view)),
            current: Some(tab.id) == current_id,
        })
        .collect();
    drop(borrowed);
    store::remember(&tabs);
}

/// Открыть заново то, что было открыто. Возвращает `false`, если сессии нет:
/// тогда заводится обычная пустая вкладка.
fn restore_session(ui: &Ui, state: &Rc<RefCell<State>>) -> bool {
    let mut opened = 0;
    let mut front = 0;
    for tab in store::session() {
        let entries: Vec<Address> = tab
            .addresses
            .iter()
            .filter_map(|text| address::parse(text).ok())
            .collect();
        let at = tab.at.min(entries.len().saturating_sub(1));
        let Some(address) = entries.get(at).cloned() else {
            continue;
        };

        // Вкладка заводится пустой, а история ставится готовой: иначе
        // «назад» после восстановления упирался бы в начальную страницу.
        new_tab(ui, state, None);
        {
            let mut borrowed = state.borrow_mut();
            // Корешок называет страницу до того, как она поедет из сети:
            // заголовок берём из журнала посещённого, а если её там нет —
            // остаётся адрес, он тоже говорящий.
            let shown = borrowed
                .store
                .title_of(&address.display())
                .map(str::to_owned)
                .unwrap_or_else(|| address.display());
            let Some(fresh) = borrowed.tabs.last_mut() else {
                continue;
            };
            fresh.history = History::restored(entries, at);
            fresh.resume = Some(tab.place);
            fresh.pending = true;
            fresh.label.set_text(&clip(&shown, TAB_LABEL));
            fresh.label.set_tooltip_text(Some(&shown));
            // Начальную страницу, которую нарисовала пустая вкладка, убираем:
            // «что это за программа» читателю, вернувшемуся к своим вкладкам,
            // не адресовано, а показывать её вместо статьи — врать. Вместе
            // с текстом уходят и её ссылки с оглавлением: смещения в пустом
            // буфере ведут в никуда.
            fresh.view.buffer().set_text("");
            fresh.links.clear();
            fresh.marks.clear();
            fresh.anchors.clear();
        }
        if tab.current {
            front = opened;
        }
        opened += 1;
    }
    if opened == 0 {
        return false;
    }
    ui.notebook.set_current_page(Some(front));
    // Грузим ровно одну — ту, которую читатель сейчас видит. Остальные
    // подождут своей очереди, и большинство её не дождётся: вкладок,
    // до которых так и не дошли руки, в сессии всегда больше половины.
    wake_tab(ui, state);
    true
}

/// Открыть вкладку, которая до сих пор была только вкладкой.
///
/// Зовётся там, где вкладка выходит на экран. Ничего не делает, если
/// страница уже есть, — значит её можно звать не разбираясь.
fn wake_tab(ui: &Ui, state: &Rc<RefCell<State>>) {
    let Some(index) = ui.notebook.current_page() else {
        return;
    };
    let waking = {
        let mut borrowed = state.borrow_mut();
        let Some(tab) = borrowed.tabs.get_mut(index as usize) else {
            return;
        };
        if !tab.pending {
            return;
        }
        tab.pending = false;
        tab.history
            .current()
            .cloned()
            .map(|address| (tab.id, address))
    };
    // В историю вкладки этот адрес уже записан — он из неё и взят.
    if let Some((id, address)) = waking {
        open(ui, state, id, address, false);
    }
}

/// Показать подсказки к напечатанному.
///
/// Список строится по журналу посещённого, порядок считает ядро
/// (`store::suggest`): совпавшее с начала хоста, потом с середины адреса,
/// потом заголовком. Здесь только показ.
fn offer_hints(ui: &Ui, state: &Rc<RefCell<State>>) {
    if ui.quiet.get() {
        return;
    }
    let typed = ui.entry.text().to_string();
    let found = state.borrow().store.suggest(&typed, HINTS);

    while let Some(child) = ui.hint_list.first_child() {
        ui.hint_list.remove(&child);
    }
    if found.is_empty() {
        state.borrow_mut().hints.clear();
        ui.hints.popdown();
        return;
    }

    // Открытое всплывающее окно держит ту высоту, с которой его показали:
    // строк стало меньше — под списком осталась бы пустая плита. Показываем
    // заново, но только когда число строк и правда изменилось, иначе окно
    // мигало бы на каждом нажатии.
    if ui.hints.is_visible() && found.len() != state.borrow().hints.len() {
        ui.hints.popdown();
    }
    for hint in &found {
        ui.hint_list.append(&hint_row(hint));
    }
    // Ничего не выбрано: первое нажатие Enter обязано открыть напечатанное,
    // а не то, что программа угадала за читателя. Выбирают стрелкой.
    ui.hint_list.unselect_all();
    state.borrow_mut().hints = found.into_iter().map(|hint| hint.address).collect();

    // Ширину берём у строки: список — её продолжение вниз, и уже неё
    // он выглядел бы чужим.
    ui.hints
        .set_size_request(ui.entry.width().max(HINT_WIDTH), -1);
    ui.hints.popup();
}

/// Строка подсказки: заголовок сверху, адрес под ним приглушённым.
/// Ровно так устроена подсказка в браузерах, и по делу: заголовок
/// вспоминается, а адрес опознаётся.
fn hint_row(hint: &Hint) -> gtk::ListBoxRow {
    let column = gtk::Box::new(gtk::Orientation::Vertical, 0);
    column.set_margin_start(10);
    column.set_margin_end(10);
    column.set_margin_top(4);
    column.set_margin_bottom(4);

    if !hint.title.is_empty() {
        let title = gtk::Label::builder()
            .label(&hint.title)
            .xalign(0.0)
            .ellipsize(pango::EllipsizeMode::End)
            .build();
        column.append(&title);
    }
    let address = gtk::Label::builder()
        .label(&hint.address)
        .xalign(0.0)
        .ellipsize(pango::EllipsizeMode::Middle)
        .build();
    address.add_css_class("hint-address");
    column.append(&address);

    let row = gtk::ListBoxRow::builder().can_focus(false).build();
    row.set_child(Some(&column));
    row
}

/// Шаг по списку подсказок. По кругу, как в браузере: список короткий,
/// и упираться в его край читателю незачем.
fn walk_hints(ui: &Ui, step: i32) {
    let mut rows = Vec::new();
    let mut child = ui.hint_list.first_child();
    while let Some(row) = child {
        child = row.next_sibling();
        if let Ok(row) = row.downcast::<gtk::ListBoxRow>() {
            rows.push(row);
        }
    }
    if rows.is_empty() {
        return;
    }
    let at = match ui.hint_list.selected_row() {
        Some(row) => row.index() + step,
        // Сверху вниз — с первой строки, снизу вверх — с последней.
        None if step > 0 => 0,
        None => rows.len() as i32 - 1,
    };
    let at = at.rem_euclid(rows.len() as i32) as usize;
    ui.hint_list.select_row(Some(&rows[at]));
}

/// Открыть выбранную подсказку.
fn take_hint(ui: &Ui, state: &Rc<RefCell<State>>, address: &str) {
    ui.hints.popdown();
    set_address(ui, address);
    match address::parse(address) {
        Ok(address) => open_current(ui, state, address, true),
        Err(error) => {
            let problem = describe(&error);
            if let Some(view) = current(ui, state) {
                show_message(&view, problem.headline, &problem.detail, &[]);
            }
        }
    }
}

/// Написать в адресной строке — от имени программы, а не читателя.
/// На время правки подсказки молчат: иначе каждый переход по ссылке
/// выбрасывал бы список поверх статьи.
fn set_address(ui: &Ui, text: &str) {
    ui.quiet.set(true);
    ui.entry.set_text(text);
    ui.quiet.set(false);
    mark_insecure(ui, text);
}

/// «Not secure» — только там, где это правда что-то значит.
///
/// Замочка на защищённой странице нет намеренно, и это не лень: браузеры
/// от него отказались, потому что читатель понимал его как «сайт надёжный»,
/// а означает он всего лишь «канал зашифрован». Отмечать надо обратное —
/// страницу, приехавшую по `http://`: её видит и правит любой посредник
/// по дороге.
///
/// Сайта с непроверенным сертификатом здесь быть не может вовсе: такую
/// страницу мы не открываем, а объясняем отказ целой страницей
/// (`failure.rs`). Значок ей не нужен — она сама и есть предупреждение.
fn mark_insecure(ui: &Ui, address: &str) {
    let insecure = address.starts_with("http://");
    ui.entry
        .set_primary_icon_name(insecure.then_some(INSECURE_ICON));
    ui.entry
        .set_primary_icon_tooltip_text(insecure.then_some(INSECURE));
    ui.entry.set_primary_icon_activatable(false);
    ui.entry.set_primary_icon_sensitive(false);
}

/// Смещение местных часов от UTC, в секундах. Часового пояса ядро не знает
/// и знать не должно; у окна он есть — от GLib.
fn local_offset() -> i32 {
    glib::DateTime::now_local()
        .map(|now| (now.utc_offset().0 / 1_000_000) as i32)
        .unwrap_or(0)
}

/// Чем открыть текущую страницу снаружи. Не то же, что показано в строке:
/// короткую форму `gh:owner/repo` чужой браузер не понимает, а у своей
/// страницы внешнего адреса нет вовсе.
fn current_target(ui: &Ui, state: &Rc<RefCell<State>>) -> Option<String> {
    let index = ui.notebook.current_page()? as usize;
    state
        .borrow()
        .tabs
        .get(index)?
        .history
        .current()
        .map(Address::external)
        .filter(|target| !target.is_empty())
}

fn open_current(ui: &Ui, state: &Rc<RefCell<State>>, address: Address, remember: bool) {
    let Some(index) = ui.notebook.current_page() else {
        return;
    };
    let id = match state.borrow().tabs.get(index as usize) {
        // На вкладке настроек статье места нет — открываем её новой вкладкой,
        // а не поверх переключателей.
        Some(tab) if tab.settings => {
            new_tab(ui, state, Some(address));
            return;
        }
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
        let mut borrowed = state.borrow_mut();
        let Some(tab) = borrowed.tabs.get_mut(index as usize) else {
            return;
        };
        // Где читатель стоит на этой странице — запоминаем, пока `at` ещё
        // указывает на неё, чтобы вернуть его сюда шагом обратно.
        let here = top_of(&tab.view);
        tab.history.set_place(here);
        let moved = if backwards {
            tab.history.back()
        } else {
            tab.history.forward()
        };
        // Куда прокрутить страницу назначения, когда она приедет.
        let place = tab.history.place();
        moved.then(|| (tab.id, tab.history.current().cloned(), place))
    };
    let Some((id, address, place)) = step else {
        return;
    };
    // Назад с первой страницы — на начальную: она корень истории вкладки.
    let Some(address) = address else {
        show_start(ui, state, id);
        return;
    };
    if let Some(tab) = state.borrow_mut().find(id) {
        tab.resume = (place > 0).then_some(place);
    }
    open(ui, state, id, address, false);
}

/// Вернуть вкладку на начальную страницу (#26). Загрузку, если она шла,
/// бросаем — показывать её ответ уже некуда; полку очищаем от навигации
/// и лент прежней страницы. Точки входа проекта не трогаем: они вкладки,
/// а не страницы, и шаг «вперёд» в тот же репозиторий их не повторит.
fn show_start(ui: &Ui, state: &Rc<RefCell<State>>, id: u64) {
    let view = {
        let mut borrowed = state.borrow_mut();
        let Some(tab) = borrowed.find(id) else { return };
        tab.generation += 1;
        tab.loading = false;
        tab.resume = None;
        tab.document = None;
        tab.shots.clear();
        tab.site.clear();
        tab.feeds.clear();
        tab.label.set_text("New tab");
        tab.label.set_tooltip_text(None);
        tab.view.clone()
    };
    show_intro(ui, state, id, &view);
    remember_session(ui, state);
}

/// Открыть адрес в названной вкладке.
fn open(ui: &Ui, state: &Rc<RefCell<State>>, id: u64, address: Address, remember: bool) {
    open_with(ui, state, id, address, remember, false);
}

/// Открыть адрес; `fresh` — мимо обоих кэшей, прямо из сети (перезагрузка).
///
/// Откуда берётся страница, по порядку: память вкладки («назад/вперёд»),
/// недельная копия на диске (`brevier::cache`), сеть. Скачанное ложится
/// и в память, и на диск.
fn open_with(
    ui: &Ui,
    state: &Rc<RefCell<State>>,
    id: u64,
    address: Address,
    remember: bool,
    fresh: bool,
) {
    // Решётку в адресе запоминаем здесь: серверу её не отправляют, и в адресе
    // загруженного документа её уже не будет.
    let anchor = page::anchor_in(&address);
    // Ключ кэша — тот адрес, которым по вкладке и ходят «назад/вперёд».
    // Внутренние страницы (сама история) не кэшируем: они обязаны показывать
    // то, что на диске, а не слепок момента. Снимок Wayback (#9) — страница
    // из сети, как любая: два запроса к архиву на шаг назад ни к чему.
    let key = address.display();
    let cacheable = !matches!(address, Address::Internal(_))
        || matches!(address, Address::Internal(Internal::Wayback(_)));
    leave_page(state, id);
    let (generation, cached) = {
        let mut state = state.borrow_mut();
        let Some(tab) = state.find(id) else { return };
        tab.fresh_visit = remember && anchor.is_none();
        if remember {
            // Место на покидаемой странице — чтобы «назад» вернул сюда,
            // а не в её начало. У новой вкладки истории ещё нет, и запись
            // молча ни к чему не привяжется.
            let here = top_of(&tab.view);
            tab.history.set_place(here);
            tab.history.visit(address.clone());
            // Переход в сторону обрубает «вперёд» — вместе с ним из кэша
            // уходят страницы, до которых больше не дойти.
            prune_pages(tab);
        }
        tab.generation += 1;
        // «Назад» и «вперёд» по уже показанной странице — из памяти, без сети:
        // страница у читателя уже была, тянуть её заново незачем. Свежий заход
        // (набор адреса, клик по ссылке) кэш обходит — там читатель просит
        // именно новую загрузку.
        let cached = (!remember && cacheable && !fresh)
            .then(|| tab.pages.get(&key).cloned())
            .flatten();
        if cached.is_none() {
            tab.loading = true;
            // Корешок вкладки не мигает точками намеренно: ширина строки в нём
            // меняла бы ширину самой вкладки, и полоса корешков дёргалась бы
            // раз в секунду.
            tab.label.set_text(&clip("Loading…", TAB_LABEL));
        }
        (tab.generation, cached)
    };

    // Есть в кэше — показываем сразу, тем же трактом, что и свежую загрузку.
    if let Some(document) = cached {
        show_document(ui, state, id, &document, anchor.as_deref());
        return;
    }

    // Недельная копия на диске. Читается сразу, без потока: это один файл
    // в десятки килобайт, а «Loading…» на кадр мигал бы зря. Что страница
    // из копии, говорим строкой — это не та страница, что сейчас на сайте.
    if !fresh && let Some(copy) = brevier::cache::Cache::open().page(&address) {
        show_document(ui, state, id, &copy.document, anchor.as_deref());
        if current_id(ui, state) == Some(id) {
            notice(
                ui,
                &format!(
                    "{} Ctrl+R loads the page afresh.",
                    brevier::cache::copy_note(copy.saved)
                ),
            );
        }
        if cacheable && let Some(tab) = state.borrow_mut().find(id) {
            tab.pages.insert(key, copy.document);
        }
        return;
    }

    sync(ui, state, None);

    if let Some(view) = view_of(state, id) {
        show_loading(&view, 1);
        tick_loading(state, id, generation, view);
    }

    let ui = ui.clone();
    let state = state.clone();
    // Чем открыть это в чужом браузере — считаем до того, как адрес уедет
    // в поток загрузки: на отказе он понадобится, а его уже не будет.
    let external = address.external();
    let archive_on = state.borrow().archive;
    let offset = local_offset();
    glib::spawn_future_local(async move {
        let loaded = gio::spawn_blocking(move || {
            // Отказ объясняем здесь же, в потоке: объяснение заглядывает
            // в архив за копией страницы (#9).
            let document = brevier::open(&address, UserAgent::Honest).map_err(|error| {
                describe_page(&error, &address, &brevier::archive::Archive::open())
            })?;
            // На диск — здесь же, в потоке: кэш сам решает, годится ли
            // документ (статья из сети — да, лента и свои страницы — нет).
            brevier::cache::Cache::open().keep(&address, &document);
            // И в архив (#8), по тому же правилу: что кладётся — решает он.
            let copy = archive_on
                .then(|| {
                    brevier::archive::Archive::open()
                        .keep(&document, &brevier::store::Stamp::now(offset))
                })
                .flatten();
            Ok::<_, Failure>((document, copy))
        })
        .await;

        // Читатель уже ушёл на другую страницу — ответ никому не нужен.
        if state.borrow_mut().find(id).map(|tab| tab.generation) != Some(generation) {
            return;
        }
        let Some(view) = view_of(&state, id) else {
            return;
        };

        match loaded {
            Ok(Ok((document, copy))) => {
                if let Some(tab) = state.borrow_mut().find(id) {
                    tab.copy = copy;
                }
                show_document(&ui, &state, id, &document, anchor.as_deref());
                // Кладём в кэш вкладки: теперь «назад» покажет её без сети.
                if cacheable && let Some(tab) = state.borrow_mut().find(id) {
                    tab.pages.insert(key.clone(), document.clone());
                }
            }
            Ok(Err(problem)) => {
                // Копии (#9) открываются здесь же, во вкладке, — как ссылка.
                // Холостым ходом, а не из обработчика: открытие стирает буфер
                // с самой кнопкой, и кнопка, уничтоженная посреди своего же
                // сигнала, роняла окно (SIGSEGV).
                let mut buttons: Vec<gtk::Button> = problem
                    .ways
                    .iter()
                    .map(|way| {
                        let button = message_button(way.label);
                        let (ui, state, target) = (ui.clone(), state.clone(), way.address.clone());
                        button.connect_clicked(move |_| {
                            let Ok(address) = address::parse(&target) else {
                                return;
                            };
                            let (ui, state) = (ui.clone(), state.clone());
                            glib::idle_add_local_once(move || {
                                open(&ui, &state, id, address, true);
                            });
                        });
                        button
                    })
                    .collect();
                buttons.extend(problem.offer_browser.then(|| browser_button(&external)));
                show_message(&view, problem.headline, &problem.detail, &buttons);
                let mut borrowed = state.borrow_mut();
                if let Some(tab) = borrowed.find(id) {
                    tab.loading = false;
                    tab.resume = None;
                    tab.label.set_text(&clip(problem.headline, TAB_LABEL));
                    tab.links.clear();
                    tab.marks.clear();
                    tab.anchors.clear();
                    tab.shots.clear();
                    tab.document = None;
                }
                drop(borrowed);
                sync(&ui, &state, None);
                // Вкладка с отказом — тоже открытая вкладка, и в сессии
                // ей место: читатель закрыл окно с ней и ждёт её обратно.
                remember_session(&ui, &state);
            }
            Err(_) => {
                if let Some(tab) = state.borrow_mut().find(id) {
                    tab.loading = false;
                }
                show_message(&view, "The load fell through", "", &[]);
            }
        }
    });
}

/// Разложить готовый документ по вкладке и привести окно в порядок.
///
/// Общий путь для двух заходов: свежей загрузки из сети и показа из кэша
/// «назад/вперёд». Отсюда и синхронность — вся отрисовка, отметки, полка
/// и оживление картинок в одном месте, чтобы кэш и сеть вели себя одинаково.
fn show_document(
    ui: &Ui,
    state: &Rc<RefCell<State>>,
    id: u64,
    document: &Document,
    anchor: Option<&str>,
) {
    let Some(view) = view_of(state, id) else {
        return;
    };
    dress(state, &view);
    let page = render(&view, document, anchor);
    // Клавиатуру отдаём только той вкладке, которую читатель видит. Фоновая,
    // догрузившись, забирала её себе, и стрелки с пробелом переставали
    // прокручивать открытую страницу — заметно на восстановлении сессии.
    if current_id(ui, state) == Some(id) {
        view.grab_focus();
    }
    // Список ссылок показываем как есть, но говорим, что это он: читатель
    // пришёл на главную блога не читать, а выбирать.
    if document.kind == brevier::Kind::Listing {
        notice(ui, LISTING);
    } else if document.served {
        // Сайт отдал markdown сам — извлечения не было, текст точный.
        notice(ui, SERVED_MARKDOWN);
    }
    let mut borrowed = state.borrow_mut();
    // В историю идёт то, что открылось, и адрес итоговый — после редиректов.
    let copy = borrowed.find(id).and_then(|tab| tab.copy.take());
    borrowed.store.record_with(
        &document.address,
        &document.title,
        local_offset(),
        copy.as_deref(),
    );
    let seen = current_zoom(&borrowed);
    // Прогресс чтения (#19) — у статьи, не у списка ссылок и не у своих
    // страниц. Записанное о ней поднимаем сразу: доли на полке видны с порога.
    let readable = document.kind == brevier::Kind::Article
        && !matches!(document.address, Address::Internal(_));
    let saved = readable
        .then(|| borrowed.readings.find(&document.address.display()).cloned())
        .flatten();
    let progress = readable.then(|| {
        let mut progress = Progress::new(&document.address.display(), &page.text, &page.anchors);
        if let Some(saved) = &saved {
            progress.restore(saved);
        }
        progress
    });
    let mut offered = None;
    if let Some(tab) = borrowed.find(id) {
        // Предложить продолжить — только странице, открытой заново, и без
        // якоря: «назад», сессия и ссылка на раздел ставят место сами.
        let fresh = std::mem::take(&mut tab.fresh_visit);
        if fresh
            && anchor.is_none()
            && tab.resume.is_none()
            && let (Some(progress), Some(saved)) = (&progress, &saved)
        {
            offered = reading::offer(progress, saved);
        }
        tab.progress = progress;
    }
    if let Some(tab) = borrowed.find(id) {
        tab.loading = false;
        tab.label.set_text(&clip(&document.title, TAB_LABEL));
        tab.label.set_tooltip_text(Some(&document.title));
        tab.links = page.links;
        tab.focus = None;
        tab.marks = page.marks;
        tab.anchors = page.anchors;
        tab.shots = page.shots.clone();
        tab.site = site_rows(&document.site);
        tab.feeds = site_rows(&document.feeds);
        tab.document = Some(document.clone());
        tab.zoom_seen = seen;
        tab.density_seen = density(ui);
    }
    drop(borrowed);
    sync(ui, state, None);
    resume_place(ui, state);
    remember_session(ui, state);
    seek_entries(ui, state, id, &document.address);
    match offered {
        Some((at, share)) if current_id(ui, state) == Some(id) => {
            offer_continue(ui, state, id, at, share);
        }
        _ => withdraw_offer(ui, state),
    }
    // Заглушки оживляем после того, как вкладка узнала про них: клик
    // по заглушке ищет вкладку по номеру.
    let eager = state.borrow().images;
    for shot in &page.shots {
        place_shot(ui, state, id, shot, None);
        // Именно в эту вкладку, а не в открытую: пока страница грузилась,
        // читатель мог уйти смотреть другую.
        if eager {
            load_shot(ui, state, id, shot);
        }
    }
    for cell in &page.cells {
        follow_cell_links(ui, state, id, cell);
    }
}

/// Выбросить из кэша страницы, до которых по истории больше не дойти.
/// Зовётся после перехода в сторону: `visit` обрубает «вперёд», и держать
/// обрубленное в памяти незачем.
fn prune_pages(tab: &mut Tab) {
    let live: Vec<String> = tab.history.entries().iter().map(Address::display).collect();
    tab.pages.retain(|key, _| live.contains(key));
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

    let (
        address,
        here,
        (can_back, back_home),
        can_forward,
        marks,
        entries,
        site,
        feeds,
        title,
        kept,
    ) = {
        let borrowed = state.borrow();
        let Some(tab) = borrowed.tabs.get(index) else {
            return;
        };
        // Звёздочка — про открытую страницу, а не про вкладку: гаснет
        // на внутренних (список в списке ни к чему) и на пустой.
        let open_now = tab.history.current();
        let kept = open_now
            .filter(|address| !address.is_internal())
            .map(|address| borrowed.marks.has(&address.display()));
        (
            tab.history
                .current()
                .map(Address::display)
                .unwrap_or_default(),
            tab.history.current().and_then(directory_row),
            (tab.history.can_go_back(), tab.history.at_first()),
            tab.history.can_go_forward(),
            tab.marks.clone(),
            // Точки входа — свойство вкладки, но уводят они с открытой
            // страницы проекта; на начальной их показывать не к чему.
            if open_now.is_some() {
                tab.entries.clone()
            } else {
                Vec::new()
            },
            tab.site.clone(),
            tab.feeds.clone(),
            tab.label.text().to_string(),
            kept,
        )
    };
    ui.star.set_sensitive(kept.is_some());
    show_star(ui, kept.unwrap_or(false));

    set_address(ui, &address);
    ui.back.set_sensitive(can_back);
    // Стрелка остаётся стрелкой и на первой странице вкладки; куда она
    // теперь ведёт, говорит подсказка. Погасшая (вкладка открыта снаружи)
    // на начальную не ведёт и так не подписывается.
    ui.back.set_tooltip_text(Some(if can_back && back_home {
        "Back to the start page"
    } else {
        "Back"
    }));
    ui.forward.set_sensitive(can_forward);

    // Ступень видна, только когда она не «как задумано»: кнопка, всегда
    // показывающая «100%», не говорит ничего и занимает место в панели.
    let step = current_zoom(&state.borrow());
    ui.zoom_level
        .set_label(&format!("{}%", (ZOOM_STEPS[step] * 100.0).round() as i32));
    ui.zoom_level.set_visible(step != ZOOM_NORMAL);
    // Общая ступень — в ней же считается ширина колонки ниже по этой функции.
    set_zoom(ZOOM_STEPS[step]);
    ui.window.set_title(Some(&if address.is_empty() {
        "Brevier".to_owned()
    } else {
        format!("{title} — Brevier")
    }));

    let (total, dark) = {
        let borrowed = state.borrow();
        let total = ui
            .notebook
            .current_page()
            .and_then(|index| borrowed.tabs.get(index as usize))
            .and_then(|tab| tab.progress.as_ref())
            .map(Progress::total)
            .unwrap_or(0);
        (total, borrowed.dark)
    };
    let (shelf, bars) = fill_contents(
        &ui.contents,
        &marks,
        total,
        dark,
        &entries,
        here.as_ref(),
        &feeds,
        &site,
    );
    let empty = shelf.is_empty();
    {
        let mut borrowed = state.borrow_mut();
        borrowed.shelf = shelf;
        borrowed.bars = bars;
    }
    update_bars(ui, state);
    ui.show_contents.set_sensitive(!empty);
    ui.shelf.set_visible(ui.show_contents.is_active() && !empty);
    fit_shelf(ui);
    follow(ui, state);

    // Поиск открыт — ищем в том, что теперь на экране: подсветка и счётчик
    // принадлежат странице, а не строке ввода.
    if ui.search.is_search_mode() {
        find(ui, state, &ui.needle.text(), true);
    }
}

/// Поставить делитель так, чтобы полка вышла той ширины, какую выбрали.
///
/// Позицию GTK считает от левого края, а полка стоит у правого: держать
/// позицию значило бы отдавать полке весь прирост окна. Поэтому хранится
/// ширина, а позиция каждый раз считается заново.
fn fit_shelf(ui: &Ui) {
    let width = ui.split.width();
    if width <= 0 {
        // Окно ещё не разложено, ширины нет — считать не по чему. Первая
        // страница открывается до показа окна, и без этого возврата полка
        // так и осталась бы самой узкой, какую позволяет `width_request`.
        let ui = ui.clone();
        ui.split.clone().add_tick_callback(move |split, _| {
            if split.width() <= 0 {
                return glib::ControlFlow::Continue;
            }
            fit_shelf(&ui);
            glib::ControlFlow::Break
        });
        return;
    }
    // Полке достаётся то, чего не заняла колонка. На большой ступени
    // колонка растёт, и место кончается: тогда полка уходит целиком,
    // а не мельчает до переносов по слогам. Мера — обещание продукта,
    // полка — удобство; уступает удобство. Вернётся само, как только
    // масштаб или окно позволят.
    let free = width - measure_px();
    if free < TOC_MIN {
        ui.shelf.set_visible(false);
        return;
    }
    // Шире половины окна полки не бывает: она рядом со статьёй, а не вместо
    // неё. Снизу её держит `width_request`, и делитель туда не пустит.
    let room = TOC_MAX.min((width / 2).max(TOC_MIN)).min(free);
    ui.split
        .set_position(width - ui.shelf_width.get().clamp(TOC_MIN, room));
}

/// Отметить на полке то место страницы, где читатель сейчас.
///
/// Полка без этого отвечает только на вопрос «что на странице есть»,
/// а читателю по ходу чтения нужен и второй — «где я в ней».
fn follow(ui: &Ui, state: &Rc<RefCell<State>>) {
    if !ui.shelf.is_visible() {
        return;
    }
    let Some(page) = ui.notebook.current_page() else {
        return;
    };
    // Прокрутка приходит и посреди перестройки состояния: заём тогда занят,
    // а подсветке довольно дождаться следующего движения.
    let Ok(borrowed) = state.try_borrow() else {
        return;
    };
    let Some(tab) = borrowed.tabs.get(page as usize) else {
        return;
    };
    let view = tab.view.clone();

    // Мерим не по верхней кромке, а по той строке, куда ставит заголовок
    // прыжок по оглавлению: иначе заголовок, к которому только что перешли,
    // оказывается выше пробы и текущим не считается.
    //
    // Плюс полстроки запаса. Прыжок ставит верх заголовка ровно на линию
    // пробы, и промаха в пиксель хватает, чтобы проба попала в строку выше,
    // а на полке отметился предыдущий раздел — то самое «кликнул, а горит
    // не то».
    let seen = view.visible_rect();
    let slack = (text_px() * LINE_HEIGHT / 2.0) as i32;
    let probe = seen.y() + (f64::from(seen.height()) * ANCHOR_ALIGN) as i32 + slack;
    // Проба мимо текста — не ответ «конец документа», а «сейчас не знаю».
    // Посреди прокрутки `GtkTextView` перекладывает строки, и высота
    // документа на кадр расходится с прокруткой; считать такую пробу
    // концом значило бы подсвечивать последний пункт — он и мигал.
    let Some(place) = view.iter_at_location(0, probe) else {
        return;
    };
    let offset = place.offset();

    let mut here = None;
    for (index, row) in borrowed.shelf.iter().enumerate() {
        match row {
            Row::Jump(at) if *at <= offset => here = Some(index),
            // Дальше только заголовки ниже пробы: они по порядку.
            Row::Jump(_) => break,
            _ => {}
        }
    }
    drop(borrowed);

    match here.and_then(|index| ui.contents.row_at_index(index as i32)) {
        // Выше первого заголовка отмечать нечего: читатель ещё во врезке.
        None => ui.contents.unselect_all(),
        Some(row) if row.is_selected() => {}
        Some(row) => {
            ui.contents.select_row(Some(&row));
            reveal(&ui.shelf_view, &row);
        }
    }
}

/// Довести отмеченную строку до глаз — и не дальше того. Полка, которая
/// прыгает на каждом повороте колеса, мешает больше, чем помогает.
fn reveal(pane: &gtk::ScrolledWindow, row: &gtk::ListBoxRow) {
    let bar = pane.vadjustment();
    let top = f64::from(row.allocation().y());
    let bottom = top + f64::from(row.height());
    if top < bar.value() {
        bar.set_value(top);
    } else if bottom > bar.value() + bar.page_size() {
        bar.set_value(bottom - bar.page_size());
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
    let (chosen, touched) = (colors.chosen, colors.touched);

    format!(
        // Шапка и окно — в тот же тёплый ряд, что и бумага. Иначе слоновая
        // кость соседствует с холодно-белой панелью GTK, и окно выглядит
        // склеенным из двух разных.
        //
        // Значок окна в шапке (его просит раскладка кнопок `icon:…`, так
        // у flatpak под GNOME) тема ставила вплотную к краю окна: кнопки
        // окна она сдвигает к краю на −6 px, а отступ возвращает только
        // первой кнопке — значок же не кнопка. Возвращаем и ему.
        "window, headerbar {{ background-color: {shelf}; }}\n\
         headerbar windowcontrols.start > image.icon:first-child {{ margin-left: 6px; }}\n\
         headerbar windowcontrols.end > image.icon:last-child {{ margin-right: 6px; }}\n\
         .page, .page text {{ background-color: {paper}; color: {ink}; }}\n\
         .shelf, .shelf > viewport, .shelf list, .shelf row {{ background-color: {shelf}; }}\n\
         .shelf separator {{ background-color: {rule}; }}\n\
         .shelf-title {{ color: {dim}; font-weight: 500; }}\n\
         .shelf row:hover {{ background-color: {touched}; }}\n\
         .shelf row:selected {{ background-color: {chosen}; }}\n\
         .shelf row:selected, .shelf row:selected label {{ color: {ink}; }}\n\
         .shot {{ border: 1px dashed {dim}; border-radius: 6px; padding: 20px 14px; \
                  color: {dim}; margin: 6px 0; }}\n\
         popover.hints > contents {{ background-color: {shelf}; padding: 4px 0; }}\n\
         .hints, .hints row {{ background-color: {shelf}; }}\n\
         .hints row:hover {{ background-color: {touched}; }}\n\
         .hints row:selected {{ background-color: {chosen}; }}\n\
         .hints row:selected label {{ color: {ink}; }}\n\
         .hint-address {{ color: {dim}; font-size: 0.85em; }}\n\
         .caption {{ color: {dim}; font-size: 0.85em; margin-bottom: 6px; }}\n\
         .formula {{ padding: 0 2px; min-height: 0; min-width: 0; color: {dim}; }}\n\
         .table {{ margin: 10px 0 14px 0; }}\n\
         .table separator {{ background-color: {rule}; min-height: 1px; }}\n\
         .th {{ font-weight: 500; }}\n"
    )
}

/// Подложка ссылки под отметкой клавиатуры: цвет ссылки на пятую часть.
fn focus_color(dark: bool) -> gtk::gdk::RGBA {
    let mut color = gtk::gdk::RGBA::parse(colors(dark).link).unwrap_or(gtk::gdk::RGBA::BLUE);
    color.set_alpha(0.22);
    color
}

fn recolor(buffer: &gtk::TextBuffer, dark: bool) {
    let table = buffer.tag_table();
    if let Some(tag) = table.lookup("focus") {
        tag.set_property("background-rgba", focus_color(dark));
    }
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

thread_local! {
    static ZOOM: Cell<f32> = const { Cell::new(1.0) };
    /// Первое окно процесса — то, которое отвечает за сессию.
    static OWNS_SESSION: Cell<bool> = const { Cell::new(true) };
}

/// Общая ступень масштаба окна, ограниченная лестницей.
fn current_zoom(state: &State) -> usize {
    state.zoom.min(ZOOM_STEPS.len() - 1)
}

/// Приготовить вкладку к отрисовке: общая ступень, теги под неё, колонка
/// под меру. Всё, что зависит от масштаба, ставится здесь — иначе кегли,
/// картинки и сетка таблицы разъедутся между собой.
fn dress(state: &Rc<RefCell<State>>, view: &gtk::TextView) {
    let (scale, dark) = {
        let borrowed = state.borrow();
        (ZOOM_STEPS[current_zoom(&borrowed)], borrowed.dark)
    };
    set_zoom(scale);
    tags(&view.buffer(), dark, scale);
    view.set_width_request(measure_px());
}

/// Сменить общую ступень масштаба: `step` — насколько сдвинуться по лестнице,
/// ноль возвращает к «как задумано».
fn zoom_by(ui: &Ui, state: &Rc<RefCell<State>>, step: i32) {
    let Some(index) = ui.notebook.current_page().map(|page| page as usize) else {
        return;
    };
    let was = current_zoom(&state.borrow());
    let now = if step == 0 {
        ZOOM_NORMAL
    } else {
        (was as i32 + step).clamp(0, ZOOM_STEPS.len() as i32 - 1) as usize
    };
    if now == was {
        return;
    }
    // Выше того, что помещается на экран, не поднимаемся. Колонка держит
    // меру жёстко, окно растёт вслед за ней, и на узком экране следующая
    // ступень уехала бы за край — вместе с текстом. Обещать меру и не дать
    // её увидеть хуже, чем не увеличить.
    if now > was && !fits_on_screen(ui, ZOOM_STEPS[now]) {
        notice(ui, "That step would not fit on this screen");
        return;
    }
    state.borrow_mut().zoom = now;
    redraw(ui, state, index);
}

/// Поместится ли колонка такой ступени на экран, где стоит окно.
fn fits_on_screen(ui: &Ui, scale: f32) -> bool {
    let monitor = ui.window.surface().and_then(|surface| {
        gtk::gdk::Display::default().and_then(|display| display.monitor_at_surface(&surface))
    });
    let Some(monitor) = monitor else {
        // Про экран ничего не известно — не мешаем читателю.
        return true;
    };
    let column = (f64::from(MEASURE) * f64::from(scale) * dpi() / 72.0).round() as i32;
    column <= monitor.geometry().width()
}

/// Перерисовать открытую страницу на новой ступени.
///
/// Текст в буфере от масштаба не зависит — от него зависят кегли, поля,
/// ширина картинок и сетка таблицы, — поэтому после перерисовки смещения
/// те же самые. На этом и держится возврат: место чтения и выделение
/// запоминаются смещениями, а не пикселями, которые как раз уехали.
fn redraw(ui: &Ui, state: &Rc<RefCell<State>>, index: usize) {
    let (id, view, document) = {
        let borrowed = state.borrow();
        let Some(tab) = borrowed.tabs.get(index) else {
            return;
        };
        (tab.id, tab.view.clone(), tab.document.clone())
    };
    // Вкладка настроек и ещё не грузившаяся: рисовать нечего. У настроек
    // своё содержимое, не статья, и масштаб к нему не применяется; неоткрытой
    // подсовывать начальную страницу нельзя — она не пустая, а неоткрытая.
    if state
        .borrow()
        .tabs
        .get(index)
        .is_some_and(|tab| tab.settings || tab.pending)
    {
        return;
    }
    // Пустая вкладка: на ней начальная страница, и у неё свой тракт.
    let Some(document) = document else {
        show_intro(ui, state, id, &view);
        return;
    };

    let buffer = view.buffer();
    let selection = buffer
        .selection_bounds()
        .map(|(from, to)| (from.offset(), to.offset()));
    let place = top_of(&view);

    dress(state, &view);
    let page = render(&view, &document, None);
    {
        let mut borrowed = state.borrow_mut();
        let seen = current_zoom(&borrowed);
        if let Some(tab) = borrowed.find(id) {
            tab.links = page.links;
            tab.focus = None;
            tab.marks = page.marks;
            tab.anchors = page.anchors;
            tab.shots = page.shots.clone();
            tab.zoom_seen = seen;
            tab.density_seen = density(ui);
        }
    }
    sync(ui, state, None);

    // Картинки декодируются под меру, а мера уехала: заглушки ставим заново
    // и заново же оживляем — иначе на увеличении осталось бы мыло.
    let eager = state.borrow().images;
    for shot in &page.shots {
        place_shot(ui, state, id, shot, None);
        if eager {
            load_shot(ui, state, id, shot);
        }
    }
    for cell in &page.cells {
        follow_cell_links(ui, state, id, cell);
    }

    if let Some((from, to)) = selection {
        let (from, to) = (buffer.iter_at_offset(from), buffer.iter_at_offset(to));
        buffer.select_range(&from, &to);
    }
    settle(&view, place, 0.0);
}

/// Догнать общий масштаб на вкладке, которую только что показали. Если
/// читатель сменил ступень, пока эта вкладка была в фоне, перерисовать её
/// под новую. Место чтения держится смещением в буфере — от масштаба оно
/// не зависит, поэтому `redraw` вернёт читателя туда же. Так же догоняется
/// и плотность экрана, когда окно переехало на другой.
fn rezoom_current(ui: &Ui, state: &Rc<RefCell<State>>) {
    let Some(index) = ui.notebook.current_page().map(|page| page as usize) else {
        return;
    };
    let stale = {
        let borrowed = state.borrow();
        borrowed.tabs.get(index).is_some_and(|tab| {
            !tab.pending
                && tab.document.is_some()
                && (tab.zoom_seen != current_zoom(&borrowed) || tab.density_seen != density(ui))
        })
    };
    if stale {
        redraw(ui, state, index);
    }
}

/// Смещение строки у верхнего края окна: ею читатель и мерит, где он
/// в тексте.
fn top_of(view: &gtk::TextView) -> i32 {
    view.iter_at_location(0, view.visible_rect().y())
        .map(|iter| iter.offset())
        .unwrap_or_default()
}

/// Масштаб страницы, в котором рисуем прямо сейчас.
///
/// Окружающий, как и `dpi()`, и по той же причине: это свойство рисования,
/// а не аргумент. Протаскивать его через два десятка функций до разбора svg
/// значило бы переписать их все ради одного числа. Ставится перед
/// отрисовкой страницы и перед загрузкой её картинок — то есть там, где
/// известно, какой вкладке рисуем.
fn zoom() -> f64 {
    f64::from(ZOOM.with(Cell::get))
}

fn set_zoom(scale: f32) {
    ZOOM.with(|cell| cell.set(scale));
}

/// Мера в пикселях. В GTK кегль задаётся пунктами, а ширина виджета
/// пикселями; без пересчёта по разрешению в строке оказывалось бы разное
/// число знаков на разных экранах.
///
/// Масштаб входит и сюда: мера задана в кеглях, поэтому колонка растёт
/// вместе с кеглем и строка остаётся той же длины в знаках.
fn measure_px() -> i32 {
    (f64::from(MEASURE) * zoom() * dpi() / 72.0).round() as i32
}

/// Кегль текста в пикселях. Нужен не окну, а разбору svg: MathJax печатает
/// формулы в `ex`, и они обязаны быть ростом с текст — на любой ступени.
fn text_px() -> f32 {
    (f64::from(TEXT_SIZE) * zoom() * dpi() / 72.0) as f32
}

/// Сколько пикселей экрана на точку окна. Целое: дробный масштаб GTK
/// округляет вверх, и картинка, разобранная с запасом, на экран ложится
/// уменьшенной, а не растянутой.
fn density(ui: &Ui) -> i32 {
    ui.window.scale_factor().max(1)
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
/// Отдать адрес чужому браузеру. Отвечает, нашлось ли кому.
///
/// Не `xdg-open` первым делом, и это поймано на живой системе: когда
/// Brevier назначен браузером по умолчанию, `xdg-open` показывает на нас,
/// и «открыть в браузере» превращается в ещё одну вкладку Brevier — ровно
/// там, где читателю нужен настоящий браузер. Поэтому спрашиваем у системы
/// список обработчиков `https` и берём первый, который не мы; порядок
/// задаёт сама система, и первым в нём идёт её выбор по умолчанию.
///
/// `xdg-open` остаётся запасным путём. На macOS списка приложений GIO
/// не ведёт, и `open` там единственный путь; то же кольцо там возможно,
/// и это известный предел, а не недосмотр.
///
/// На Windows запасной путь — оболочка (`ShellExecuteW`), а не `cmd /C
/// start`: `cmd` читает `&` в адресе как конец команды, и ссылка со страницы
/// стала бы командой.
fn open_in_system_browser(target: &str) -> bool {
    // В песочнице наружу ведёт только портал, и спрашивать там некого:
    // список приложений — это список приложений самой песочницы, а его
    // хостовых браузеров в нём нет вовсе. Выбор делает хост, а сходить
    // к нему умеет сам GTK.
    if sandboxed() {
        gtk::UriLauncher::new(target).launch(None::<&gtk::Window>, gio::Cancellable::NONE, |_| {});
        return true;
    }

    if let Some(browser) = other_browser()
        && browser
            .launch_uris(&[target], None::<&gio::AppLaunchContext>)
            .is_ok()
    {
        return true;
    }

    #[cfg(windows)]
    {
        shell_open(target)
    }
    #[cfg(not(windows))]
    {
        let program = if cfg!(target_os = "macos") {
            "open"
        } else {
            "xdg-open"
        };
        std::process::Command::new(program)
            .arg(target)
            .spawn()
            .is_ok()
    }
}

/// Отдать адрес оболочке Windows — так, как это делает там любая программа:
/// откроется браузер по умолчанию. GLib на этом месте
/// (`launch_default_for_uri`) на живой Windows отвечала отказом.
///
/// Оболочка не только открывает, но и запускает: `.exe`, `.bat`, `.lnk`
/// или чужой протокол из ссылки стали бы программой. Поэтому ей уходят
/// только адреса сайтов и файлы тех видов, что Brevier читает и показывает
/// сам.
#[cfg(windows)]
fn shell_open(target: &str) -> bool {
    use std::ffi::c_void;

    #[link(name = "shell32")]
    unsafe extern "system" {
        fn ShellExecuteW(
            window: *mut c_void,
            verb: *const u16,
            file: *const u16,
            parameters: *const u16,
            directory: *const u16,
            show: i32,
        ) -> isize;
    }

    const KINDS: &[&str] = &[
        ".md",
        ".markdown",
        ".rss",
        ".atom",
        ".xml",
        ".json",
        ".png",
        ".jpg",
        ".jpeg",
        ".gif",
        ".webp",
        ".svg",
        ".avif",
    ];
    let lower = target.to_ascii_lowercase();
    let web = lower.starts_with("http://") || lower.starts_with("https://");
    if !web && !KINDS.iter().any(|kind| lower.ends_with(kind)) {
        return false;
    }

    let wide = |text: &str| text.encode_utf16().chain([0]).collect::<Vec<u16>>();
    let (verb, file) = (wide("open"), wide(target));
    // Безопасно: обе строки живут до конца вызова и кончаются нулём, прочие
    // указатели пустые, как разрешает документация. 1 — SW_SHOWNORMAL;
    // успех — ответ больше 32, так у ShellExecute заведено.
    let code = unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            verb.as_ptr(),
            file.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            1,
        )
    };
    code > 32
}

/// Живём ли мы в песочнице flatpak.
///
/// Признак её собственный и надёжный: внутри `/.flatpak-info` есть всегда,
/// снаружи не бывает. Обычную сборку это не задевает ни одной проверкой
/// сверх одной.
fn sandboxed() -> bool {
    std::path::Path::new("/.flatpak-info").exists()
}

/// Первый зарегистрированный обработчик ссылок, который не мы. Себя узнаём
/// и по ярлыку, и по бинарнику: на Windows ярлыков нет, там приложение —
/// это его exe.
fn other_browser() -> Option<gio::AppInfo> {
    let ours = format!("{APP_ID}.desktop");
    let us = |app: &gio::AppInfo| {
        app.id().is_some_and(|id| id.as_str() == ours)
            || app
                .executable()
                .file_stem()
                .is_some_and(|stem| stem.eq_ignore_ascii_case("brevier-ui"))
    };
    gio::AppInfo::all_for_type("x-scheme-handler/https")
        .into_iter()
        .find(|app| app.supports_uris() && !us(app))
}

/// Куда окно выкладывает то, что везёт в себе: гарнитуры и иконку.
///
/// Кэш, потому что это производное от бинарника: удалили — разложится
/// заново при следующем запуске.
fn unpacked() -> std::path::PathBuf {
    glib::user_cache_dir().join("brevier")
}

/// Иконка программы — в комплекте, как и гарнитуры.
const LOGO: &[u8] = include_bytes!("../../assets/brevier.svg");

/// Показать окну его иконку, ничего не устанавливая в систему.
///
/// Иконку окно берёт не из файла, а из темы значков — по имени, и имя это
/// идентификатор программы. Поэтому свою кладём в тему: выкладываем в кэш
/// (`icons/hicolor/scalable/apps/io.github.gurov.brevier.svg`) и добавляем этот
/// каталог в поиск темы. Приём тот же, что и с гарнитурами, и причина та же:
/// своё добро приложение раскладывает у себя, а не в системных каталогах.
///
/// Цена записана честно: на Wayland иконку окна выбирает не программа,
/// а композитор — по ярлыку `.desktop` и идентификатору приложения, — и без
/// установки ярлыка там останется заглушка. На X11 работает и без установки.
fn use_bundled_icon() {
    let theme = unpacked().join("icons");
    let apps = theme.join("hicolor").join("scalable").join("apps");
    let file = apps.join(format!("{APP_ID}.svg"));

    // Имя ставим всегда, даже если разложить не удалось: на машине,
    // где иконка установлена по-человечески (ярлык плюс тема значков),
    // она найдётся и без нашего кэша.
    gtk::Window::set_default_icon_name(APP_ID);

    // Перезаписываем только при расхождении: иконку меняют раз в год,
    // а запусков много.
    let same = std::fs::read(&file).is_ok_and(|bytes| bytes == LOGO);
    if !same && (std::fs::create_dir_all(&apps).is_err() || std::fs::write(&file, LOGO).is_err()) {
        return;
    }
    if let Some(display) = gtk::gdk::Display::default() {
        gtk::IconTheme::for_display(&display).add_search_path(&theme);
    }
}

/// Гарнитуры из комплекта — в обход системной установки.
///
/// GTK берёт шрифты у fontconfig, а тот знает только про установленные.
/// Класть свои в системные каталоги приложение не вправе, поэтому
/// выкладываем их в свой кэш и подсовываем fontconfig собственный конфиг,
/// который включает системный и добавляет нашу папку. Всё остаётся
/// внутри кэша приложения.
///
/// На macOS механизм другой (`CTFontManagerRegisterFontsForURL`) — это
/// отдельная работа при упаковке.
#[cfg(not(windows))]
fn use_bundled_fonts() {
    let Some(fonts) = unpack_fonts() else {
        return;
    };
    let config = unpacked().join("fonts.conf");
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

/// То же на Windows. GTK там рисует текст через DirectWrite, и fontconfig
/// в этом не участвует; файлы отдаём самой карте шрифтов Pango — той,
/// что достаётся каждому виджету. Поэтому и зовётся после подъёма GTK.
#[cfg(windows)]
fn use_bundled_fonts() {
    use pango::prelude::FontMapExt;

    let Some(fonts) = unpack_fonts() else {
        return;
    };
    let Some(map) = gtk::Label::new(None).pango_context().font_map() else {
        return;
    };
    for (name, _) in FONTS {
        if let Err(error) = map.add_font_file(fonts.join(name)) {
            eprintln!("brevier-ui: the bundled font {name}: {error}");
        }
    }
}

/// Выложить гарнитуры в кэш: шрифт подключается файлом, а не байтами.
/// Отвечает папкой, когда все файлы на месте.
fn unpack_fonts() -> Option<std::path::PathBuf> {
    let fonts = unpacked().join("fonts");
    std::fs::create_dir_all(&fonts).ok()?;
    for (name, bytes) in FONTS {
        let path = fonts.join(name);
        let stale = std::fs::metadata(&path).map(|meta| meta.len() as usize != bytes.len());
        if stale.unwrap_or(true) {
            std::fs::write(&path, bytes).ok()?;
        }
    }
    Some(fonts)
}

/// Гарнитуры, которые окно везёт в себе: Noto Sans и Noto Sans Mono (OFL).
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

/// Ссылка под точкой окна, если она там есть.
/// Перейти по ссылке статьи — по клику или с клавиатуры. Ссылка внутрь
/// этой же страницы — не загрузка, а прыжок по буферу: место заголовка
/// мы знаем точно. `aside` — новой вкладкой (Ctrl, средняя кнопка).
fn follow_link(
    ui: &Ui,
    state: &Rc<RefCell<State>>,
    id: u64,
    view: &gtk::TextView,
    target: &str,
    aside: bool,
) {
    let from = {
        let mut borrowed = state.borrow_mut();
        let Some(tab) = borrowed.find(id) else { return };
        let from = tab.history.current().cloned();
        let here = from.as_ref().map(Address::display);
        if !aside
            && let Some(fragment) = page::fragment_of(target, here.as_deref())
            && jump(view, &tab.anchors, &fragment)
        {
            return;
        }
        from
    };
    let Ok(address) = address::parse(target) else {
        return;
    };
    if !address::may_follow(from.as_ref(), &address) {
        notice(ui, "A page from the web can't open files on your computer");
        return;
    }
    if aside {
        new_tab(ui, state, Some(address));
    } else {
        open(ui, state, id, address, true);
    }
}

/// Отметить следующую ссылку статьи (`forward`) или предыдущую (#34).
/// Отметка на экране — шаг от неё; ушла с экрана — с того, что видно
/// сейчас. Отвечает, нашлась ли ссылка: за последней Tab уходит дальше
/// по окну.
fn move_link_focus(
    state: &Rc<RefCell<State>>,
    id: u64,
    view: &gtk::TextView,
    forward: bool,
) -> bool {
    // Под заимствованием только копия: прокрутка ниже зовёт обработчики,
    // которые сами берут состояние.
    let (links, focus) = {
        let mut borrowed = state.borrow_mut();
        let Some(tab) = borrowed.find(id) else {
            return false;
        };
        let links: Vec<(usize, usize)> = tab
            .links
            .iter()
            .map(|link| (link.start, link.end))
            .collect();
        (links, tab.focus.filter(|at| *at < tab.links.len()))
    };

    let buffer = view.buffer();
    let iter = |offset: usize| buffer.iter_at_offset(offset as i32);
    // Видно ли — по месту строки на экране, а не по смещениям: смещение
    // у края окна GTK отдаёт, только когда точка над текстом.
    let seen = view.visible_rect();
    let (top, bottom) = (seen.y(), seen.y() + seen.height());
    let line = |offset: usize| {
        let place = view.iter_location(&iter(offset));
        (place.y(), place.y() + place.height())
    };
    let on_screen = |(start, _): (usize, usize)| {
        let (above, below) = line(start);
        below > top && above < bottom
    };
    let next = match (focus.filter(|at| on_screen(links[*at])), forward) {
        (Some(at), true) => Some(at + 1).filter(|next| *next < links.len()),
        (Some(at), false) => at.checked_sub(1),
        (None, true) => links.iter().position(|&(start, _)| line(start).1 > top),
        (None, false) => links.iter().rposition(|&(start, _)| line(start).0 < bottom),
    };
    if let Some(tab) = state.borrow_mut().find(id) {
        tab.focus = next;
    }

    if let Some((start, end)) = focus.map(|at| links[at]) {
        buffer.remove_tag_by_name("focus", &iter(start), &iter(end));
    }
    let Some((start, end)) = next.map(|at| links[at]) else {
        return false;
    };
    buffer.apply_tag_by_name("focus", &iter(start), &iter(end));
    // Каретку — на ссылку: курсор не виден, но экранный чтец идёт за ней.
    buffer.place_cursor(&iter(start));
    // Докручиваем, только если ссылка видна не целиком.
    let (above, _) = line(start);
    let (_, below) = line(end.saturating_sub(1).max(start));
    if above < top || below > bottom {
        scroll_to(view, start as i32, 0.3);
    }
    true
}

/// Снять отметку со ссылки. Отвечает, была ли она.
fn drop_link_focus(state: &Rc<RefCell<State>>, id: u64, view: &gtk::TextView) -> bool {
    let range = {
        let mut borrowed = state.borrow_mut();
        let Some(tab) = borrowed.find(id) else {
            return false;
        };
        let range = tab
            .focus
            .and_then(|at| tab.links.get(at))
            .map(|link| (link.start, link.end));
        tab.focus = None;
        range
    };
    let Some((start, end)) = range else {
        return false;
    };
    let buffer = view.buffer();
    buffer.remove_tag_by_name(
        "focus",
        &buffer.iter_at_offset(start as i32),
        &buffer.iter_at_offset(end as i32),
    );
    true
}

fn link_at<'a>(
    view: &gtk::TextView,
    links: &'a [page::Link],
    x: f64,
    y: f64,
) -> Option<&'a page::Link> {
    let (bx, by) = view.window_to_buffer_coords(gtk::TextWindowType::Widget, x as i32, y as i32);
    let (iter, _) = view.iter_at_position(bx, by)?;
    let offset = iter.offset() as usize;
    links
        .iter()
        .find(|link| offset >= link.start && offset < link.end)
}

/// Стереть страницу вкладки.
///
/// Виджеты страницы (кнопки отказа, рамки картинок, таблицы) снимаем
/// до того, как стирать текст, пока буфер цел. Стёртый вместе с текстом
/// виджет под указателем меняет состояние наведения у самого `GtkTextView`,
/// а тот на `state-flags-changed` читает выделение полуудалённого буфера —
/// SIGSEGV в `gtk_text_buffer_get_selection_bounds` (GTK 4.20; поймано
/// 9 октября 2026 щелчком по кнопке «Open your copy», #9). Фокус с такого
/// виджета — на сам текст: клавиши прокрутки остаются у страницы.
fn wipe(view: &gtk::TextView) {
    if view.focus_child().is_some() {
        view.grab_focus();
    }
    let buffer = view.buffer();
    let mut at = buffer.start_iter();
    while let Some((found, after)) =
        at.forward_search("\u{FFFC}", gtk::TextSearchFlags::empty(), None)
    {
        if let Some(anchor) = found.child_anchor() {
            for widget in anchor.widgets() {
                view.remove(&widget);
            }
        }
        at = after;
    }
    buffer.set_text("");
}

/// Страница-сообщение: заголовок, объяснение и кнопки под ним — одной строкой.
fn show_message(view: &gtk::TextView, headline: &str, detail: &str, buttons: &[gtk::Button]) {
    let buffer = view.buffer();
    wipe(view);
    let mut end = buffer.end_iter();
    buffer.insert_with_tags_by_name(&mut end, headline, &["h2"]);
    if !detail.is_empty() {
        buffer.insert(&mut end, "\n\n");
        buffer.insert(&mut end, detail);
    }
    if buttons.is_empty() {
        return;
    }
    buffer.insert(&mut end, "\n\n");
    let anchor = buffer.create_child_anchor(&mut end);
    let row = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(8)
        .halign(gtk::Align::Start)
        .build();
    for button in buttons {
        row.append(button);
    }
    view.add_child_at_anchor(&row, &anchor);
}

fn message_button(label: &str) -> gtk::Button {
    gtk::Button::builder().label(label).build()
}

/// Кнопка, а не только Ctrl+O: на странице, где ничего не показалось,
/// читателю нужен выход, а не память о сочетании клавиш. Что предлагать
/// её, решает ядро (`Failure::offer_browser`): при любом отказе, кроме
/// адреса, который не разобрался, — там отдавать браузеру нечего.
fn browser_button(target: &str) -> gtk::Button {
    let button = message_button("Open in your browser");
    let target = target.to_owned();
    button.connect_clicked(move |_| {
        open_in_system_browser(&target);
    });
    button
}

/// Начальная страница. Рисуется тем же трактом, что и статья: текст в ядре,
/// разметка markdown, рендерер общий. Историю и адресную строку не трогает —
/// это не открытая страница, а пустая вкладка, которой есть что сказать.
/// Недавнее в ней берётся из журнала на каждый показ.
fn show_intro(ui: &Ui, state: &Rc<RefCell<State>>, id: u64, view: &gtk::TextView) {
    let markdown = brevier::intro::page(&state.borrow().store, false);
    let document = Document {
        address: Address::Web(String::new()),
        title: brevier::intro::TITLE.to_owned(),
        markdown,
        kind: brevier::Kind::Article,
        served: false,
        site: Vec::new(),
        feeds: Vec::new(),
        lang: None,
        next: None,
        archived: None,
    };
    dress(state, view);
    let page = render(view, &document, None);
    let mut borrowed = state.borrow_mut();
    let seen = current_zoom(&borrowed);
    if let Some(tab) = borrowed.find(id) {
        tab.links = page.links;
        tab.focus = None;
        tab.marks = page.marks;
        tab.anchors = page.anchors;
        tab.zoom_seen = seen;
        tab.density_seen = density(ui);
    }
    drop(borrowed);
    sync(ui, state, None);
}
/// Завести тег или переписать ему свойства.
///
/// Дважды тег в таблицу буфера не заводят, а масштаб страницы меняет ровно
/// те же свойства, что ставит первая отрисовка. Значит список должен быть
/// один: `tags` вызывается и на сборке вкладки, и на смене ступени.
/// Приоритет при этом остаётся от первого захода — на нём держатся
/// подпись оповещения поверх курсива цитаты и подсветка поиска поверх
/// цвета ссылки.
fn style(buffer: &gtk::TextBuffer, name: &str, properties: &[(&str, &dyn glib::value::ToValue)]) {
    if let Some(tag) = buffer.tag_table().lookup(name) {
        for (property, value) in properties {
            tag.set_property_from_value(property, &value.to_value());
        }
    } else {
        buffer.create_tag(Some(name), properties);
    }
}

/// Теги — вся типографика статьи. Кегли и интерлиньяж те же, что были
/// в прошлом интерфейсе: они живут в ядре и от тулкита не зависят.
///
/// Масштаб умножает модель целиком — кегль, воздух между строками, шкалу
/// заголовков, отступы списка и цитаты, поля блока кода. Умножить один
/// кегль значило бы развалить их между собой.
fn tags(buffer: &gtk::TextBuffer, dark: bool, scale: f32) {
    let scale = f64::from(scale);
    let px = |value: f64| (value * scale).round() as i32;
    let extra = px(f64::from((LINE_HEIGHT - 1.0) * TEXT_SIZE));
    let body = f64::from(TEXT_SIZE) * scale;

    style(
        buffer,
        "body",
        &[
            ("family", &BODY_FAMILY),
            ("size-points", &body),
            ("pixels-inside-wrap", &extra),
            ("pixels-below-lines", &(extra * 2)),
        ],
    );
    // Строфа (#21): каждая строка стиха — свой абзац буфера, и воздух между
    // ними тот же, что между строками прозы внутри абзаца. Длинная строка,
    // не влезшая в меру, переносится с висячим отступом — так перенос
    // не спутать с новой строкой стиха. Строфы разводит пустая строка,
    // её ставит модель страницы.
    style(
        buffer,
        "verse",
        &[
            ("family", &BODY_FAMILY),
            ("size-points", &body),
            ("pixels-inside-wrap", &extra),
            ("pixels-below-lines", &extra),
            ("indent", &px(-f64::from(HANG))),
        ],
    );

    for (index, ratio) in HEADINGS.iter().enumerate() {
        let name = format!("h{}", index + 1);
        style(
            buffer,
            &name,
            &[
                ("family", &BODY_FAMILY),
                ("size-points", &(body * f64::from(*ratio))),
                ("weight", &HEADING_WEIGHTS[index]),
                // Воздух сверху, а не снизу: заголовок принадлежит тому,
                // что под ним.
                ("pixels-above-lines", &(extra * 3)),
                ("pixels-below-lines", &extra),
            ],
        );
    }

    style(buffer, "em", &[("style", &pango::Style::Italic)]);
    style(buffer, "strong", &[("weight", &BOLD)]);
    let colors = colors(dark);

    // Код в строке отличается не только гарнитурой: подложка отделяет его
    // от текста там, где моноширинного мало — в одном-двух знаках.
    style(
        buffer,
        "code",
        &[
            ("family", &MONO_FAMILY),
            ("size-points", &(body * f64::from(CODE_SIZE))),
            ("background", &colors.panel),
        ],
    );
    // Блок кода — панель: подложка во всю меру, поля по краям, строки плотнее,
    // чем в тексте. `paragraph-background` красит строку целиком, поэтому
    // панель получается без единого виджета.
    style(
        buffer,
        "codeblock",
        &[
            ("family", &MONO_FAMILY),
            ("size-points", &(body * f64::from(CODE_SIZE))),
            // Блок стоит в той же мере, что и текст: левый край кода
            // ровно под первой буквой абзаца. Поле слева было бы видно
            // дважды — подложка красится от него же, и панель отъезжала
            // вправо от колонки.
            ("left-margin", &px(0.0)),
            // Отрицательный отступ у Pango — это отступ продолжению:
            // первая строка стоит у края, перенос длинной строки кода
            // уходит правее, и его видно. Строку кода не перенести нельзя —
            // колонок в буфере нет.
            ("indent", &px(-f64::from(HANG))),
            ("pixels-below-lines", &px(f64::from(CODE_GAP))),
            ("paragraph-background", &colors.panel),
        ],
    );
    // Пустая строка с той же подложкой — это поля панели сверху и снизу.
    // Пустую строку GTK сам не красит: фон ей и пустым строкам кода
    // дорисовывает виджет статьи по этому же тегу (`article.rs`, #28).
    style(
        buffer,
        "pad",
        &[
            ("size-points", &(body * f64::from(PAD_SIZE))),
            ("paragraph-background", &colors.panel),
            ("pixels-above-lines", &extra),
            ("pixels-below-lines", &extra),
        ],
    );

    style(buffer, "kw", &[("foreground", &colors.keyword)]);
    style(buffer, "lit", &[("foreground", &colors.literal)]);
    style(buffer, "num", &[("foreground", &colors.number)]);
    style(
        buffer,
        "com",
        &[
            ("foreground", &colors.comment),
            ("style", &pango::Style::Italic),
        ],
    );
    // Поле слева пошире обычного: в нём стоит линейка, которую рисует
    // виджет статьи. Уровень вложенности — свой отступ и своя линейка:
    // на треде обсуждения ответ на ответ иначе неотличим от новой реплики.
    for level in 1..=page::QUOTE_LEVELS {
        style(
            buffer,
            &format!("quote{level}"),
            &[
                ("style", &pango::Style::Italic),
                ("left-margin", &px(f64::from(INDENT) * f64::from(level))),
            ],
        );
    }

    // Список: маркер выступает влево, перенос строки встаёт под текст,
    // а не под маркер. Уровни вложенности — свой отступ каждому.
    for level in 1..=page::LIST_LEVELS {
        style(
            buffer,
            &format!("list{level}"),
            &[
                ("left-margin", &px(f64::from(INDENT) * f64::from(level))),
                ("indent", &px(-f64::from(HANG))),
                // Пункты стоят плотнее абзацев: список — одна мысль, разбитая
                // на части, а не несколько абзацев подряд.
                ("pixels-below-lines", &(extra / 2)),
            ],
        );
    }
    // Блок кода внутри цитаты или пункта (#13): только поле — то, под которым
    // стоит их текст (у пункта — правее маркера). Теги самих цитаты и пункта
    // принесли бы курсив и воздух. Заводятся после `codeblock`, чтобы их
    // поле перебило его нулевое; линейку цитаты на этих строках рисует виджет
    // статьи по тегу `rule`, у которого свойств нет вовсе.
    for level in 1..=page::QUOTE_LEVELS {
        style(
            buffer,
            &format!("inset{level}"),
            &[("left-margin", &px(f64::from(INDENT) * f64::from(level)))],
        );
        style(buffer, &format!("rule{level}"), &[]);
    }
    for level in 1..=page::LIST_LEVELS {
        style(
            buffer,
            &format!("iteminset{level}"),
            &[(
                "left-margin",
                &px(f64::from(INDENT) * f64::from(level) + f64::from(HANG)),
            )],
        );
    }
    let (link, dim) = (colors.link, colors.dim);
    style(
        buffer,
        "link",
        &[
            ("underline", &pango::Underline::Single),
            ("foreground", &link),
        ],
    );
    style(buffer, "dim", &[("foreground", &dim)]);
    // Подпись оповещения: заводится после цитаты, чтобы её курсив перебить —
    // у наложенного позже тега приоритет выше.
    style(
        buffer,
        "alert",
        &[
            ("weight", &700),
            ("style", &pango::Style::Normal),
            ("size-points", &(body * f64::from(ALERT_SIZE))),
            (
                "letter-spacing",
                &px(f64::from(pango::SCALE) * f64::from(ALERT_TRACKING)),
            ),
        ],
    );

    // Метка сноски в тексте — верхним индексом: мельче и выше строки.
    // Не юникодными «¹²³», потому что номер бывает трёхзначным, а набор
    // таких знаков в гарнитурах кончается на девятке.
    style(
        buffer,
        "noteref",
        &[
            ("size-points", &(body * f64::from(NOTEREF_SIZE))),
            (
                "rise",
                &px(f64::from(pango::SCALE) * f64::from(NOTEREF_RISE)),
            ),
        ],
    );
    // Сама сноска под статьёй: мельче текста, с висячим отступом, как пункт
    // списка, — она и есть пункт списка.
    style(
        buffer,
        "note",
        &[
            ("size-points", &(body * f64::from(NOTE_SIZE))),
            ("left-margin", &px(f64::from(NOTE_INDENT))),
            ("indent", &px(-f64::from(NOTE_HANG))),
            ("pixels-below-lines", &(extra / 2)),
        ],
    );

    // Ссылка под отметкой клавиатуры (#34): бледная подложка цвета ссылки —
    // видна, но текст не перекрикивает. До подсветки поиска: найденное
    // важнее, его и показываем поверх.
    style(buffer, "focus", &[("background-rgba", &focus_color(dark))]);

    // Подсветка поиска. Заводится последней: у тегов, наложенных позже,
    // приоритет выше, и жёлтое ложится поверх цвета ссылки.
    style(
        buffer,
        "found",
        &[("background", &FOUND), ("foreground", &FOUND_INK)],
    );
    style(
        buffer,
        "here",
        &[("background", &FOUND_HERE), ("foreground", &FOUND_INK)],
    );
}

/// Что остаётся у окна после отрисовки статьи. Ссылки, оглавление и якоря
/// приходят из модели ядра как есть; картинки и ячейки таблиц — виджеты,
/// которые окно поставило само.
struct Drawn {
    /// Текст страницы — для прогресса чтения: отпечаток и длина (#19).
    text: String,
    links: Vec<page::Link>,
    marks: Vec<page::Mark>,
    anchors: Vec<(String, usize)>,
    shots: Vec<Shot>,
    cells: Vec<gtk::Label>,
}

/// Разложить статью по буферу.
///
/// Раскладывает ядро (`page::Page`), окно только переводит её в свои
/// средства: имя стиля — это имя тега, знак объекта — якорь виджета или
/// холст формулы. Оба занимают в буфере ровно один символ, как знак объекта
/// в модели, поэтому смещения модели остаются смещениями буфера. `target` —
/// якорь, уже приведённый (`page::anchor_in`).
fn render(view: &gtk::TextView, document: &Document, target: Option<&str>) -> Drawn {
    let page = page::Page::of(document);

    let buffer = view.buffer();
    wipe(view);

    let mut shots = Vec::new();
    let mut cells = Vec::new();
    let mut end = buffer.end_iter();
    let mut rest = page.text.as_str();
    let mut at = 0;
    for placed in &page.blocks {
        let (before, after) = split_chars(rest, placed.at - at);
        buffer.insert(&mut end, before);
        match &placed.block {
            // Холст, а не виджет: сотня виджетов в строках текста рвала
            // прокрутку (см. шапку `formula.rs`).
            Block::Image {
                source,
                alt,
                inline: true,
            } => {
                let canvas = Formula::new();
                let at = end.offset();
                buffer.insert_paintable(&mut end, &canvas);
                shots.push(Shot::new(source, alt, true, Slot::Canvas(canvas, at)));
            }
            Block::Image {
                source,
                alt,
                inline: false,
            } => {
                let frame = frame_at(view, &mut end);
                shots.push(Shot::new(source, alt, false, Slot::Frame(frame)));
            }
            Block::Table(table) => {
                let frame = frame_at(view, &mut end);
                frame.add_css_class("table");
                frame.append(&grid(table, &mut cells));
            }
        }
        rest = after.strip_prefix(page::OBJECT).unwrap_or(after);
        at = placed.at + 1;
    }
    buffer.insert(&mut end, rest);

    // Теги — по участкам модели. Приоритет тегов GTK задан порядком их
    // заведения, а не наложения, поэтому накладывать можно в любом порядке.
    let table = buffer.tag_table();
    let mut known: HashMap<page::Style, Option<gtk::TextTag>> = HashMap::new();
    for run in &page.runs {
        let from = buffer.iter_at_offset(run.start as i32);
        let to = buffer.iter_at_offset(run.end as i32);
        for style in &run.styles {
            let tag = known
                .entry(*style)
                .or_insert_with(|| table.lookup(&style.name()));
            if let Some(tag) = tag {
                buffer.apply_tag(tag, &from, &to);
            }
        }
    }

    // Новая статья начинается сначала — или с того места, на которое указывала
    // решётка в адресе. Через `idle`, потому что в момент вставки текста
    // у виджета ещё нет раскладки и прокручивать ему некуда.
    buffer.place_cursor(&buffer.start_iter());
    let offset = target
        .and_then(|want| page.anchors.iter().find(|(name, _)| name == want))
        .map(|(_, offset)| *offset as i32);
    let view = view.clone();
    glib::idle_add_local_once(move || match offset {
        Some(offset) => settle(&view, offset, ANCHOR_ALIGN),
        None => scroll_to(&view, 0, 0.0),
    });
    Drawn {
        text: page.text,
        links: page.links,
        marks: page.contents,
        anchors: page.anchors,
        shots,
        cells,
    }
}

/// Разрезать строку после `count` символов: смещения модели в символах,
/// а срез у `str` — в байтах.
fn split_chars(text: &str, count: usize) -> (&str, &str) {
    let byte = text
        .char_indices()
        .nth(count)
        .map_or(text.len(), |(byte, _)| byte);
    text.split_at(byte)
}

/// Место под виджет в тексте — якорь с рамкой в меру.
///
/// Своя строка у картинки и таблицы уже есть: переводы строк вокруг знака
/// объекта ставит модель.
fn frame_at(view: &gtk::TextView, end: &mut gtk::TextIter) -> gtk::Box {
    let place = view.buffer().create_child_anchor(end);
    let frame = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(4)
        .width_request(measure_px())
        .build();
    view.add_child_at_anchor(&frame, &place);
    frame
}

/// Таблица — сетка виджетов на якоре.
///
/// Текстом её не набрать: в буфере нет колонок, и раньше строки
/// склеивались палками в моноширинном — читать это нельзя. Цена решения
/// записана честно: текст таблицы лежит в виджетах, а не в буфере,
/// поэтому поиск по странице и «скопировать всё» её не видят. В markdown
/// при сохранении таблица цела.
fn grid(table: &page::Table, cells: &mut Vec<gtk::Label>) -> gtk::Grid {
    let columns = table
        .rows
        .iter()
        .map(|row| row.cells.len())
        .max()
        .unwrap_or(1);

    let grid = gtk::Grid::builder()
        .column_spacing(20)
        .row_spacing(7)
        .hexpand(true)
        .build();
    let mut line = 0;
    for row in &table.rows {
        for (column, content) in row.cells.iter().enumerate() {
            let cell = gtk::Label::builder()
                .wrap(true)
                .wrap_mode(pango::WrapMode::WordChar)
                .max_width_chars(30)
                .valign(gtk::Align::Start)
                .selectable(true)
                .can_focus(false)
                .build();
            cell.set_markup(&markup_of(content));
            // Выравнивание берём из самой таблицы: колонка чисел, объявленная
            // правой, должна стоять справа.
            let align = match table.alignments.get(column) {
                Some(page::Align::Right) => 1.0,
                Some(page::Align::Center) => 0.5,
                _ => 0.0,
            };
            cell.set_xalign(align);
            // Остаток меры отдаём последнему столбцу: обычно там текст,
            // а не число, и ему перенос дороже.
            cell.set_hexpand(column + 1 == columns);
            cell.add_css_class(if row.header { "th" } else { "td" });
            grid.attach(&cell, column as i32, line, 1, 1);
            cells.push(cell);
        }
        line += 1;
        if row.header {
            let rule = gtk::Separator::new(gtk::Orientation::Horizontal);
            grid.attach(&rule, 0, line, columns as i32, 1);
            line += 1;
        }
    }
    grid
}

/// Ячейка таблицы разметкой Pango: курсив, полужирный, код и ссылки.
///
/// `GtkLabel` понимает подмножество разметки и сам делает ссылки живыми —
/// иначе пришлось бы городить виджет на каждую ячейку. Участки модели плоские,
/// а разметка вложенная: стили участка идут от внешнего к внутреннему,
/// поэтому держим стопку открытых и закрываем только то, что кончилось, —
/// курсив вокруг ссылки остаётся одним `<i>`, а не рвётся на три.
fn markup_of(cell: &page::Cell) -> String {
    // Ссылка опознаётся номером, а не адресом: две соседние ссылки
    // на одно место — всё равно две ссылки.
    let link_at = |offset: usize| {
        cell.links
            .iter()
            .position(|link| link.start <= offset && offset < link.end)
    };
    let mut out = String::new();
    let mut open: Vec<(page::Style, Option<usize>)> = Vec::new();
    let close = |out: &mut String, (style, _): &(page::Style, Option<usize>)| {
        out.push_str(match style {
            page::Style::Em => "</i>",
            page::Style::Strong => "</b>",
            page::Style::Code => "</tt>",
            page::Style::Link => "</a>",
            _ => "",
        });
    };
    for run in &cell.runs {
        let wanted: Vec<(page::Style, Option<usize>)> = run
            .styles
            .iter()
            .map(|style| {
                let link = (*style == page::Style::Link)
                    .then(|| link_at(run.start))
                    .flatten();
                (*style, link)
            })
            .collect();
        let common = open.iter().zip(&wanted).take_while(|(a, b)| a == b).count();
        while open.len() > common {
            let last = open.pop().unwrap();
            close(&mut out, &last);
        }
        for (style, link) in &wanted[common..] {
            match style {
                page::Style::Em => out.push_str("<i>"),
                page::Style::Strong => out.push_str("<b>"),
                page::Style::Code => out.push_str("<tt>"),
                page::Style::Link => {
                    let href = link.map_or("", |index| cell.links[index].target.as_str());
                    out.push_str(&format!("<a href=\"{}\">", glib::markup_escape_text(href)));
                }
                _ => {}
            }
            open.push((*style, *link));
        }
        let text: String = cell
            .text
            .chars()
            .skip(run.start)
            .take(run.end - run.start)
            .collect();
        out.push_str(&glib::markup_escape_text(&text));
    }
    while let Some(last) = open.pop() {
        close(&mut out, &last);
    }
    out
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
    // Метку выравнивают и по горизонтали, а сбоку колонку не листают вовсе:
    // строка списка с отступом, поставленная к левому краю, срезала бы
    // начала строк всей колонки.
    if let Some(sideways) = view.hadjustment() {
        sideways.set_value(sideways.lower());
    }
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

/// Прыгнуть к якорю — уже приведённому (`page::fragment_of`). `false`
/// значит «такого заголовка на странице нет» — тогда ссылка отрабатывает
/// как обычная.
fn jump(view: &gtk::TextView, anchors: &[(String, usize)], want: &str) -> bool {
    let Some((_, offset)) = anchors.iter().find(|(name, _)| name == want) else {
        return false;
    };
    settle(view, *offset as i32, ANCHOR_ALIGN);
    true
}

/// Показать оглавление и связать строки с местами в тексте.
/// Заполнить полку и сказать, что делает каждая её строка.
///
/// Групп четыре: точки входа в документацию проекта, оглавление открытой
/// страницы, ленты сайта и его навигация. Проект стоит выше — ради него
/// режим репозитория и затевался, а оглавление длинное и увело бы эти
/// две-три строки под сгиб.
///
/// Группа проекта подписана всегда: её строки уводят со страницы, и знать
/// об этом читатель должен до нажатия. Оглавление подписывается только
/// под ней — в одиночку полка и так оглавление, и лишняя строка над ним
/// ничего не объясняет.
/// Полоска доли прочитанного у строки оглавления: где начинается раздел,
/// его доля и сама полоска (#19).
type ShareBar = (usize, Rc<Cell<f32>>, gtk::DrawingArea);

#[allow(clippy::too_many_arguments)]
fn fill_contents(
    list: &gtk::ListBox,
    marks: &[page::Mark],
    total: usize,
    dark: bool,
    entries: &[Entry],
    here: Option<&Entry>,
    feeds: &[Entry],
    site: &[Entry],
) -> (Vec<Row>, Vec<ShareBar>) {
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }

    let mut shelf: Vec<Row> = Vec::new();
    let project: Vec<&Entry> = entries.iter().chain(here).collect();

    if !project.is_empty() {
        list.append(&group("In this repository"));
        shelf.push(Row::Header);
    }
    for entry in &project {
        let (row, label) = shelf_row(&entry.title, 0);
        // Куда уводит строка, читатель вправе знать до нажатия.
        label.set_tooltip_text(Some(&entry.address.display()));
        list.append(&row);
        shelf.push(Row::Open(entry.address.clone()));
    }
    if !project.is_empty() && !marks.is_empty() {
        list.append(&group("On this page"));
        shelf.push(Row::Header);
    }

    let mut bars: Vec<ShareBar> = Vec::new();
    for mark in marks {
        // Веха несёт своё место в подписи (#19): «40% · Начало абзаца…» —
        // заголовка у неё нет, и где она, иначе не понять.
        let title = if mark.heading || total == 0 {
            mark.title.clone()
        } else {
            reading::waypoint_label(mark, total)
        };
        let (row, label, area, share) =
            shelf_row_with_share(&title, i32::from(mark.level.saturating_sub(1)) * 12, dark);
        if !mark.heading {
            // Веха — не структура автора, а наша выжимка. Пусть это видно.
            label.add_css_class("dim-label");
        }
        list.append(&row);
        // Точное попадание: смещение в буфере, а не доля высоты.
        shelf.push(Row::Jump(mark.offset as i32));
        bars.push((mark.offset, share, area));
    }

    // Ленты сайта — между оглавлением и меню. Подписаны всегда: строка уводит
    // со страницы. Выше меню, потому что меню бывает в полсотни строк,
    // а лента — одна-две, и под ним её бы не нашли.
    if !feeds.is_empty() {
        list.append(&group(feed_group(feeds.len())));
        shelf.push(Row::Header);
    }
    for entry in feeds {
        let (row, label) = shelf_row(&entry.title, 0);
        label.set_tooltip_text(Some(&entry.address.display()));
        list.append(&row);
        shelf.push(Row::Open(entry.address.clone()));
    }

    // Навигация сайта идёт последней и всегда подписана: её строки уводят
    // со страницы, и знать об этом читатель должен до нажатия. Ниже
    // оглавления потому, что оглавление — про то, что читают сейчас,
    // а меню — про то, куда пойти потом.
    if !site.is_empty() {
        list.append(&group("On this site"));
        shelf.push(Row::Header);
    }
    for entry in site {
        let (row, label) = shelf_row(&entry.title, 0);
        label.add_css_class("dim-label");
        label.set_tooltip_text(Some(&entry.address.display()));
        list.append(&row);
        shelf.push(Row::Open(entry.address.clone()));
    }

    (shelf, bars)
}

/// Подпись группы лент: так она и сказана в роадмапе — «This site has a feed».
fn feed_group(count: usize) -> &'static str {
    if count == 1 {
        "This site has a feed"
    } else {
        "This site has feeds"
    }
}

/// Навигация сайта строками полки.
///
/// Адрес разбираем нашим же разбором: ссылка на github из меню должна
/// открыться режимом репозитория, а не сырой страницей хостинга, — ровно
/// как ссылка из текста.
fn site_rows(site: &[brevier::Link]) -> Vec<Entry> {
    site.iter()
        .filter_map(|link| {
            address::parse(&link.address).ok().map(|address| Entry {
                title: link.title.clone(),
                address,
            })
        })
        .collect()
}

/// Строка «а что ещё лежит рядом» — дверь к файлам каталога.
///
/// Без неё листинг доступен только тому, кто сам напечатает адрес каталога,
/// а знать раскладку чужого репозитория читатель не обязан: именно этим
/// хостинг и помогает — списком файлов над README. Стоит строка в группе
/// проекта, потому что уводит со страницы, и ничего не стоит, пока
/// на неё не нажали: запрос к API уходит по нажатию.
///
/// На самом листинге строки нет: шаг наверх у него в тексте, а показывать
/// ссылку на себя же незачем.
fn directory_row(address: &Address) -> Option<Entry> {
    brevier::repo::directory_of(address).map(|address| Entry {
        title: "Files in this directory".to_owned(),
        address,
    })
}

/// Строка полки: подпись, за которой стоит работа.
///
/// Многоточие ставим только тому, что не влезло в три строки. Считать
/// знаки, как раньше, значит рубить заголовок там, где место ещё было:
/// полка теперь шире или уже по воле читателя, и сколько знаков в неё
/// войдёт, знает Pango, а не мы. Ограничение по знакам остаётся крайним
/// (`TOC_CHARS`) — на случай «заголовка» в целый абзац.
fn shelf_row(title: &str, indent: i32) -> (gtk::ListBoxRow, gtk::Label) {
    let label = gtk::Label::builder()
        .label(clip(title, TOC_CHARS))
        .xalign(0.0)
        .wrap(true)
        .wrap_mode(pango::WrapMode::WordChar)
        .ellipsize(pango::EllipsizeMode::End)
        .lines(TOC_LINES)
        .margin_top(4)
        .margin_bottom(4)
        .margin_start(10 + indent)
        .margin_end(10)
        .build();
    let row = gtk::ListBoxRow::builder().child(&label).build();
    // Строка куда-то ведёт, и курсор обязан это показать — как на ссылке.
    row.set_cursor_from_name(Some("pointer"));
    (row, label)
}

/// Строка оглавления с полоской справа: доля прочитанного в её разделе
/// (#19). Тонкая и молчит, пока раздел не начат: полка — для того, что на
/// странице, а не отчёт о чтении.
fn shelf_row_with_share(
    title: &str,
    indent: i32,
    dark: bool,
) -> (gtk::ListBoxRow, gtk::Label, gtk::DrawingArea, Rc<Cell<f32>>) {
    let (row, label) = shelf_row(title, indent);
    label.set_hexpand(true);
    let share = Rc::new(Cell::new(0.0_f32));
    let area = gtk::DrawingArea::builder()
        .content_width(3)
        .margin_end(4)
        .margin_top(6)
        .margin_bottom(6)
        .build();
    {
        let share = share.clone();
        area.set_draw_func(move |_, cairo, width, height| {
            let part = f64::from(share.get().clamp(0.0, 1.0));
            if part <= 0.0 {
                return;
            }
            let colors = colors(dark);
            let paint = |hex: &str, alpha: f64| {
                let [r, g, b] = rgb(hex);
                cairo.set_source_rgba(
                    f64::from(r) / 255.0,
                    f64::from(g) / 255.0,
                    f64::from(b) / 255.0,
                    alpha,
                );
            };
            let (w, h) = (f64::from(width), f64::from(height));
            paint(colors.rule, 1.0);
            cairo.rectangle(0.0, 0.0, w, h);
            let _ = cairo.fill();
            paint(colors.dim, 0.85);
            cairo.rectangle(0.0, 0.0, w, h * part);
            let _ = cairo.fill();
        });
    }
    // Строка — ряд: подпись и полоска; сама строка уже собрана `shelf_row`.
    let line = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    row.set_child(None::<&gtk::Widget>);
    line.append(&label);
    line.append(&area);
    row.set_child(Some(&line));
    (row, label, area, share)
}

/// Подпись над группой полки. Не строка: нажимать её не на что.
fn group(title: &str) -> gtk::ListBoxRow {
    let label = gtk::Label::builder()
        .label(title)
        .xalign(0.0)
        .margin_top(10)
        .margin_bottom(2)
        .margin_start(10)
        .margin_end(10)
        .build();
    label.add_css_class("caption");
    label.add_css_class("dim-label");
    gtk::ListBoxRow::builder()
        .child(&label)
        .activatable(false)
        .selectable(false)
        .build()
}

/// Найти точки входа в документацию проекта — фоном.
///
/// Дюжина проб на CDN стоит около полусекунды, и платить их открытием
/// страницы незачем: статья уже на экране, полка дополнится, когда ответ
/// придёт. Ищем на проект, а не на файл: переход между файлами одного
/// репозитория пробы не повторяет.
fn seek_entries(ui: &Ui, state: &Rc<RefCell<State>>, id: u64, address: &Address) {
    let Address::Repo(repo) = address else {
        // Ушли из репозитория — проекту на полке делать нечего.
        let mut borrowed = state.borrow_mut();
        if let Some(tab) = borrowed.find(id) {
            tab.entries.clear();
            tab.entries_for = None;
        }
        return;
    };

    {
        let mut borrowed = state.borrow_mut();
        let Some(tab) = borrowed.find(id) else { return };
        if tab
            .entries_for
            .as_ref()
            .is_some_and(|known| same_project(known, repo))
        {
            return;
        }
        tab.entries.clear();
        tab.entries_for = Some(repo.clone());
    }

    let ui = ui.clone();
    let state = state.clone();
    let asked = repo.clone();
    glib::spawn_future_local(async move {
        let repo = asked.clone();
        let Ok(found) =
            gio::spawn_blocking(move || brevier::repo::documentation(&repo, UserAgent::Honest))
                .await
        else {
            return;
        };

        let mut borrowed = state.borrow_mut();
        let Some(tab) = borrowed.find(id) else { return };
        // Пока искали, читатель мог уйти в другой проект или в веб.
        if !tab
            .entries_for
            .as_ref()
            .is_some_and(|now| same_project(now, &asked))
        {
            return;
        }
        tab.entries = found
            .into_iter()
            .map(|entry| Entry {
                title: entry.title,
                address: brevier::repo::entry_address(&asked, entry.path),
            })
            .collect();
        drop(borrowed);
        sync(&ui, &state, None);
    });
}

/// Тот же проект? Путь внутри репозитория различать не должен: точки входа
/// принадлежат проекту целиком.
fn same_project(a: &Repo, b: &Repo) -> bool {
    a.host == b.host && a.owner == b.owner && a.name == b.name
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
    if let Slot::Canvas(canvas, _) = &shot.slot {
        let waiting = trouble.is_some() || !state.borrow().images;
        let layout = waiting.then(|| {
            let view = view_of(state, id);
            let layout = match &view {
                Some(view) => view.create_pango_layout(Some(&page::formula(&shot.alt))),
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

    let Some(frame) = shot.slot.frame() else {
        return;
    };

    let name = if shot.alt.is_empty() {
        "image".to_owned()
    } else {
        format!("image: {}", clip(&shot.alt, 160))
    };
    let label = match trouble {
        Some(trouble) => format!("{trouble}\n{name}"),
        None => name,
    };

    let button = gtk::Button::builder()
        .label(&label)
        .has_frame(false)
        .build();
    button.add_css_class("shot");
    button.set_cursor_from_name(Some("pointer"));
    button.set_tooltip_text(Some(&shot.source.display()));
    if let Some(text) = button.child().and_downcast::<gtk::Label>() {
        text.set_wrap(true);
        text.set_max_width_chars(CAPTION_CHARS);
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

/// Если картинка, которая только что приехала, стоит выше верхнего края окна,
/// её рост из заглушки в полный размер сдвинул бы текст под глазами читателя.
/// Тогда снимаем место чтения — вернём его `settle` после того, как картинка
/// встанет. Для картинки в окне или ниже него делать нечего: верхняя строка
/// не двигается, и трогать прокрутку значило бы драться с самим читателем.
/// Возвращаем `None` и для фоновой вкладки, и для формулы (у неё нет рамки):
/// формула растёт с кегль, а не с колонку, и рывка не даёт.
fn hold_reading_place(
    ui: &Ui,
    state: &Rc<RefCell<State>>,
    id: u64,
    shot: &Shot,
) -> Option<(gtk::TextView, i32)> {
    if current_id(ui, state) != Some(id) {
        return None;
    }
    let frame = shot.slot.frame()?;
    let view = view_of(state, id)?;
    // Верх рамки в координатах окна: отрицательный — картинка уехала выше
    // видимой области.
    let (_, y) = frame.translate_coordinates(&view, 0.0, 0.0)?;
    (y < 0.0).then(|| (view.clone(), top_of(&view)))
}

/// Скачать и показать одну картинку.
///
/// Скачивание и декодирование уходят в отдельный поток: `ureq` синхронный,
/// а схема на пол-мегабайта разбирается заметное время — окно не должно
/// вставать на ней колом. Уже скачанную картинку берём из кэша вкладки:
/// «назад» декодирует её из памяти, а не тянет из сети заново.
fn load_shot(ui: &Ui, state: &Rc<RefCell<State>>, id: u64, shot: &Shot) {
    if shot.busy.replace(true) {
        return;
    }
    // Под общую ступень: картинка ужимается до меры, а мера зависит от неё.
    set_zoom(ZOOM_STEPS[current_zoom(&state.borrow())]);
    let (generation, cached) = {
        let mut borrowed = state.borrow_mut();
        let Some(tab) = borrowed.find(id) else {
            shot.busy.set(false);
            return;
        };
        // Файловые картинки не кэшируем: диск и так рядом, тянуть нечего.
        let cached = match &shot.source {
            Source::Web(url) => tab.blobs.get(url),
            Source::File(_) => None,
        };
        (tab.generation, cached)
    };

    // Заглушку «loading…» показываем только когда правда идём в сеть: из кэша
    // картинка встаёт почти в тот же кадр, и мигание заглушкой лишнее.
    if cached.is_none()
        && let Some(frame) = shot.slot.frame()
    {
        let waiting = gtk::Label::builder()
            .label("loading the image…")
            .wrap(true)
            .max_width_chars(CAPTION_CHARS)
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
        density: density(ui) as f32,
    };
    let ui = ui.clone();
    let state = state.clone();
    let shot = shot.clone();

    glib::spawn_future_local(async move {
        // Из кэша — только декодируем; из сети — сначала берём байты, чтобы
        // положить их в кэш, а потом декодируем. Второе значение — сырые байты
        // к сохранению (только у сетевой картинки), иначе `None`.
        let loaded = gio::spawn_blocking(move || match cached {
            Some(bytes) => {
                let raster = media::decode(&bytes, None, look)?;
                Ok((raster, None::<Vec<u8>>))
            }
            None => {
                // Недельная копия на диске, потом сеть. На диск — только
                // то, что разобралось: битые байты хранить незачем.
                let disk = brevier::cache::Cache::open();
                let copy = match &source {
                    Source::Web(url) => disk.image(url),
                    Source::File(_) => None,
                };
                let from_disk = copy.is_some();
                let (bytes, mime) = match copy {
                    Some(bytes) => (bytes, None),
                    None => media::grab(&source, UserAgent::Honest)?,
                };
                let raster = media::decode(&bytes, mime.as_deref(), look)?;
                if !from_disk && let Source::Web(url) = &source {
                    disk.keep_image(url, &bytes);
                }
                let keep = matches!(source, Source::Web(_)).then_some(bytes);
                Ok((raster, keep))
            }
        })
        .await;

        // Вкладку успели увести на другую страницу — рамки уже нет.
        if state.borrow_mut().find(id).map(|tab| tab.generation) != Some(generation) {
            return;
        }
        match loaded {
            Ok(Ok((raster, keep))) => {
                // Свежескачанное кладём в кэш вкладки под адресом источника.
                if let (Source::Web(url), Some(bytes)) = (&shot.source, keep)
                    && let Some(tab) = state.borrow_mut().find(id)
                {
                    tab.blobs.put(url.clone(), Arc::new(bytes));
                }
                // Картинка выше окна вырастет из заглушки в полный рост
                // и толкнёт текст под глазами — держим место чтения.
                let hold = hold_reading_place(&ui, &state, id, &shot);
                show_shot(&shot, raster, view_of(&state, id).as_ref());
                if let Some((view, place)) = hold {
                    settle(&view, place, 0.0);
                }
            }
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

/// Опустить формулу на её глубину под базовой линией (#14).
///
/// `GtkTextView` ставит холст нижним краем на базовую линию, а у формулы
/// с индексом снизу или дробью часть рисунка — под ней; MathJax это
/// объявляет, ядро переводит в пиксели (`Raster::depth`). Опускаем тегом
/// `rise` на единственном знаке холста — строка расступается под вынос
/// сама, как под букву с хвостом. Тег на глубину, а не на формулу: разных
/// глубин на странице единицы, формул — сотни.
fn sink(buffer: &gtk::TextBuffer, canvas: &Formula, at: i32, depth: i32) {
    if depth <= 0 {
        return;
    }
    let from = buffer.iter_at_offset(at);
    // Страницу успели перерисовать (масштаб): на этом месте уже другой холст.
    if from.paintable().as_ref() != Some(canvas.upcast_ref::<gtk::gdk::Paintable>()) {
        return;
    }
    let mut to = from;
    to.forward_char();
    let name = format!("sink{depth}");
    let table = buffer.tag_table();
    let tag = table.lookup(&name).unwrap_or_else(|| {
        let tag = gtk::TextTag::builder()
            .name(name.as_str())
            .rise(-depth * pango::SCALE)
            .build();
        table.add(&tag);
        tag
    });
    buffer.apply_tag(&tag, &from, &to);
}

/// Показать разобранную картинку с подписью.
fn show_shot(shot: &Shot, raster: Raster, view: Option<&gtk::TextView>) {
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

    // Растр — в пикселях экрана, место — в точках окна (#15).
    let image = Sharp::new(texture.upcast_ref(), raster.density);

    // Формула — холст в буфере: меняем в нём картинку, виджета тут нет вовсе.
    if let Slot::Canvas(canvas, at) = &shot.slot {
        canvas.set_image(&image);
        let depth = (raster.depth as f32 / raster.density).round() as i32;
        if let Some(view) = view {
            sink(&view.buffer(), canvas, *at, depth);
        }
        return;
    }
    let Some(frame) = shot.slot.frame() else {
        return;
    };

    // Если картинка уже стоит в рамке — меняем холст, а не ребёнка:
    // перестройка дерева виджетов посреди кадра и есть та самая жалоба
    // GTK на снимок без раскладки.
    let picture = match frame.first_child().and_downcast::<gtk::Picture>() {
        Some(picture) => {
            picture.set_paintable(Some(&image));
            picture
        }
        None => {
            let picture = gtk::Picture::for_paintable(&image);
            fill(frame, &picture);
            picture
        }
    };
    picture.set_can_shrink(true);
    picture.set_size_request(image.width(), image.height());
    picture.set_cursor_from_name(Some("pointer"));
    picture.set_halign(gtk::Align::Center);
    picture.set_tooltip_text(Some(&shot.source.display()));

    // Полный размер — работа системного браузера: у нас картинка ужата
    // до меры текста.
    let click = gtk::GestureClick::new();
    let target = shot.source.display();
    click.connect_released(move |_, _, _, _| {
        open_in_system_browser(&target);
    });
    picture.add_controller(click);

    if !shot.alt.is_empty() {
        // Подпись стоит под картинкой, а не под колонкой: узкая картинка
        // висит по центру, и подпись у левого поля выглядела бы чужой.
        let narrow = image.width() < measure_px();
        let caption = gtk::Label::builder()
            .label(&shot.alt)
            .wrap(true)
            .max_width_chars(CAPTION_CHARS)
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

    let chooser = gtk::FileDialog::builder()
        .title("Save article")
        .initial_name(save::suggested_name(&document))
        .modal(true)
        .build();

    let window = ui.window.clone();
    let ui = ui.clone();
    // `FileDialog` сам держит себя в живых до ответа и сам зовёт обратно —
    // прежний `FileChooserNative` требовал ссылки на самого себя и разбора
    // кода ответа.
    chooser.save(Some(&window), gio::Cancellable::NONE, move |answer| {
        let Some(path) = answer.ok().and_then(|file| file.path()) else {
            return;
        };
        save_to(&ui, path, document.clone());
    });
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

// ── прогресс чтения (#19) ───────────────────────────────────────────────────

/// Прогресс чтения в окне (#19): тик раз в секунду, кнопка «Continue»,
/// полоса внизу и «читатель здесь», когда окно стало активным.
fn reading_hooks(ui: &Ui, state: &Rc<RefCell<State>>) {
    {
        let ui = ui.clone();
        let state = state.clone();
        glib::timeout_add_seconds_local(1, move || {
            // Окно закрыто — и тикать некому.
            if !ui.window.is_visible() {
                return glib::ControlFlow::Break;
            }
            tick_reading(&ui, &state);
            glib::ControlFlow::Continue
        });
    }
    {
        let ui2 = ui.clone();
        let state = state.clone();
        ui.offer.connect_clicked(move |_| take_offer(&ui2, &state));
    }
    {
        let ui2 = ui.clone();
        let state = state.clone();
        ui.bar.set_draw_func(move |_, cairo, width, height| {
            draw_bar(&ui2, &state, cairo, width, height)
        });
    }
    {
        let state = state.clone();
        ui.window.connect_is_active_notify(move |window| {
            if window.is_active() {
                touched(&state);
            }
        });
    }
}

/// Уйти со страницы: записать прочитанное, если страница запомнена. Место —
/// то, что сейчас у верха окна, а не на последнем тике.
fn leave_page(state: &Rc<RefCell<State>>, id: u64) {
    let saved = {
        let mut borrowed = state.borrow_mut();
        let Some(tab) = borrowed.find(id) else { return };
        let Some(progress) = tab.progress.as_mut() else {
            return;
        };
        progress.set_place(top_of(&tab.view).max(0) as usize);
        progress
            .remembered()
            .then(|| progress.saved(store::Stamp::now(local_offset())))
    };
    if let Some(saved) = saved {
        state.borrow_mut().readings.put(saved);
    }
}

/// Предложить продолжить с прошлого места — кнопкой у строки состояния.
/// Предложение, а не прыжок: открыть страницу заново бывает и намеренно.
fn offer_continue(ui: &Ui, state: &Rc<RefCell<State>>, id: u64, at: usize, share: u32) {
    state.borrow_mut().offered = Some((id, at));
    ui.offer.set_label(&format!("Continue from {share}%"));
    ui.offer.set_visible(true);
    notice(ui, "You have read part of this page before.");
    let button = ui.offer.clone();
    let label = format!("Continue from {share}%");
    // Минута: решить, продолжать или перечитать, читатель вправе не сразу.
    // Раньше предложение уходит само — с уходом со страницы или по нажатию.
    glib::timeout_add_local_once(OFFER_FOR, move || {
        if button.label().as_deref() == Some(label.as_str()) {
            button.set_visible(false);
        }
    });
}

fn withdraw_offer(ui: &Ui, state: &Rc<RefCell<State>>) {
    if let Ok(mut borrowed) = state.try_borrow_mut() {
        borrowed.offered = None;
    }
    ui.offer.set_visible(false);
}

/// Принять предложение: туда, где читатель остановился.
fn take_offer(ui: &Ui, state: &Rc<RefCell<State>>) {
    let offered = state.borrow_mut().offered.take();
    ui.offer.set_visible(false);
    let Some((id, at)) = offered else { return };
    if let Some(view) = view_of(state, id) {
        settle(&view, at as i32, ANCHOR_ALIGN);
    }
}

/// Видимый кусок текста: смещения у верхнего и нижнего края окна.
///
/// По строкам, а не по точке: `iter_at_location` у края окна попадает в поле
/// колонки, а не в текст, и отвечает «не знаю» — подставленный тогда конец
/// буфера засчитывал прочитанным весь документ. Нижняя строка, видимая
/// не целиком, в видимое не идёт.
fn visible_range(view: &gtk::TextView) -> (usize, usize) {
    let seen = view.visible_rect();
    let (top, _) = view.line_at_y(seen.y());
    let (bottom, _) = view.line_at_y((seen.y() + seen.height() - 1).max(seen.y()));
    let (top, bottom) = (
        top.offset().max(0) as usize,
        bottom.offset().max(0) as usize,
    );
    (top, bottom.max(top))
}

/// Секунда чтения: если читатель здесь — окно активно, страницу недавно
/// трогали, — открытая страница получает тик, а полка и полоса — свежие доли.
fn tick_reading(ui: &Ui, state: &Rc<RefCell<State>>) {
    let Some(index) = ui.notebook.current_page() else {
        return;
    };
    let saved = {
        let Ok(mut borrowed) = state.try_borrow_mut() else {
            return;
        };
        let present = ui.window.is_active() && borrowed.last_input.elapsed() < PRESENT;
        let Some(tab) = borrowed.tabs.get_mut(index as usize) else {
            return;
        };
        if !present || tab.loading || tab.pending || tab.settings {
            None
        } else {
            let (from, to) = visible_range(&tab.view);
            tab.progress.as_mut().and_then(|progress| {
                progress
                    .tick(from, to, 1)
                    .then(|| progress.saved(store::Stamp::now(local_offset())))
            })
        }
    };
    if let Some(saved) = saved
        && let Ok(mut borrowed) = state.try_borrow_mut()
    {
        borrowed.readings.put(saved);
    }
    update_bars(ui, state);
}

/// Доли прочитанного — полоскам полки, и перерисовать полосу внизу.
fn update_bars(ui: &Ui, state: &Rc<RefCell<State>>) {
    ui.bar.queue_draw();
    let Some(index) = ui.notebook.current_page() else {
        return;
    };
    let Ok(borrowed) = state.try_borrow() else {
        return;
    };
    let Some(progress) = borrowed
        .tabs
        .get(index as usize)
        .and_then(|tab| tab.progress.as_ref())
    else {
        return;
    };
    let starts: Vec<usize> = borrowed.bars.iter().map(|(at, _, _)| *at).collect();
    let shares = progress.shares(&starts);
    for ((_, cell, area), share) in borrowed.bars.iter().zip(shares) {
        if (cell.get() - share).abs() > 0.001 {
            cell.set(share);
            area.queue_draw();
        }
    }
}

/// Читатель что-то сделал со страницей: время чтения снова идёт.
fn touched(state: &Rc<RefCell<State>>) {
    if let Ok(mut borrowed) = state.try_borrow_mut() {
        borrowed.last_input = Instant::now();
    }
}

/// Полоса внизу статьи: тонкая линия, докуда дошли — чуть темнее, точки —
/// начала разделов оглавления. Ненавязчиво, как в читалках книг: её видно,
/// когда ищут, и не видно, когда читают.
fn draw_bar(
    ui: &Ui,
    state: &Rc<RefCell<State>>,
    cairo: &gtk::cairo::Context,
    width: i32,
    height: i32,
) {
    let Some(index) = ui.notebook.current_page() else {
        return;
    };
    let Ok(borrowed) = state.try_borrow() else {
        return;
    };
    let Some(tab) = borrowed.tabs.get(index as usize) else {
        return;
    };
    let Some(progress) = tab.progress.as_ref() else {
        return;
    };
    let total = progress.total();
    // Страница в экран — ни пути, ни полосы.
    let Some(bar) = tab.view.vadjustment() else {
        return;
    };
    if total == 0 || bar.upper() <= bar.page_size() + 1.0 {
        return;
    }
    let (_, reached) = visible_range(&tab.view);
    let colors = colors(borrowed.dark);
    let paint = |hex: &str, alpha: f64| {
        let [r, g, b] = rgb(hex);
        cairo.set_source_rgba(
            f64::from(r) / 255.0,
            f64::from(g) / 255.0,
            f64::from(b) / 255.0,
            alpha,
        );
    };
    let margin = 12.0;
    let span = (f64::from(width) - margin * 2.0).max(1.0);
    let y = f64::from(height) / 2.0;
    let x_of = |at: usize| margin + span * (at.min(total) as f64 / total as f64);

    // Дорожка.
    paint(colors.rule, 1.0);
    cairo.set_line_width(1.0);
    cairo.move_to(margin, y.floor() + 0.5);
    cairo.line_to(margin + span, y.floor() + 0.5);
    let _ = cairo.stroke();
    // Пройденное.
    paint(colors.dim, 0.9);
    cairo.set_line_width(2.0);
    cairo.move_to(margin, y);
    cairo.line_to(x_of(reached), y);
    let _ = cairo.stroke();
    // Разделы — точками: пройденные краской пути, впереди — бледнее.
    for mark in &tab.marks {
        let passed = mark.offset <= reached;
        paint(colors.dim, if passed { 0.9 } else { 0.45 });
        cairo.arc(x_of(mark.offset), y, 1.8, 0.0, std::f64::consts::TAU);
        let _ = cairo.fill();
    }
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
    let (start, end) = buffer.bounds();
    let full = buffer.text(&start, &end, true);
    page::hits(&full, needle)
        .into_iter()
        .map(|(from, to)| (from as i32, to as i32))
        .collect()
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
                    page::fragment_of(target, here.as_deref())
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

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn the_window_answers_about_itself_in_the_terminal() {
        assert!(matches!(answer(&args(&["--help"])), Some(Ok(_))));
        assert!(matches!(answer(&args(&["-V"])), Some(Ok(_))));
        // Адреса — не вопрос к программе, на них открывают вкладки.
        assert!(answer(&args(&["https://e.com/a", "gh:o/n"])).is_none());
        assert!(answer(&args(&[])).is_none());
        // А опечатка в ключе — не адрес: вкладка с «not a URL» врала бы
        // читателю про причину.
        assert!(matches!(answer(&args(&["--dark"])), Some(Err(_))));
    }
}
