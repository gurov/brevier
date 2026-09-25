//! Вход для Android: приложение получает ядро общей библиотекой, и снаружи
//! у ядра четыре функции. Модуль собирается только под Android
//! (`cfg(target_os = "android")`), поэтому ни cli, ни окно о нём не знают —
//! тулкит и платформа по-прежнему сидят на краю.
//!
//! Контракт с Kotlin — класс `io.github.gurov.brevier.Core`:
//!
//! - `init(Context, String)` — обязателен и не про чтение: системный
//!   проверяющий сертификаты живёт в JVM, и `rustls-platform-verifier` должен
//!   получить контекст приложения **до** первого запроса. Без этого решение
//!   «доверие делегируем ОС» на Android не работает вовсе — а обходить
//!   проверку мы не станем и здесь. Вторым аргументом приходит папка,
//!   которую система выдала приложению: в ней живут история, закладки,
//!   настройки и сессия.
//! - `call(String, String): String` — всё остальное: имя действия и аргумент,
//!   ответ в JSON. Один вход вместо двадцати: каждая функция JNI — это
//!   обвязка, имя символа и шанс разойтись с Kotlin, а текстовый контракт
//!   виден целиком в одном `match`. Поля аргумента разделены знаком `U+001F`
//!   (разделитель единиц ASCII): его не бывает ни в адресах, ни в заголовках.
//! - `image(...)` — картинка: байты, а не текст, поэтому свой вход.
//!   Декодирует ядро, а не `BitmapFactory`: после отказа от JS декодер
//!   картинок — главная поверхность атаки, и отдавать её системному коду
//!   на C++ значило бы отменить решение «свои декодеры и потолки».
//! - `find(String, String): IntArray` — поиск по странице. Ищет ядро: правила
//!   «мягкий перенос не мешает, неразрывный пробел — пробел» одни на всех.
//!
//! Сеть синхронная (`ureq`), поэтому `call` с сетевыми действиями Kotlin
//! зовёт не из главного потока. Общее состояние — за мьютексом, и сеть
//! под ним не ходит никогда: иначе одна медленная страница держала бы
//! и подсказки адресной строки, и соседнюю вкладку.

use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};

use jni::EnvUnowned;
use jni::errors::{Error as JniError, ThrowRuntimeExAndDefault};
use jni::objects::{JByteArray, JClass, JIntArray, JObject, JString};

use crate::address::{self, Address};
use crate::cache::{self, Cache};
use crate::failure::describe;
use crate::fetch::UserAgent;
use crate::media::{self, Fit, Look, Source};
use crate::outline;
use crate::page::{self, Page, json_string};
use crate::palette;
use crate::repo;
use crate::save;
use crate::store::{self, HINTS, Marks, Settings, Store};
use crate::{Document, Kind};

/// Разделитель полей аргумента `call`.
const FIELD: char = '\u{1f}';
/// Сколько прочитанных документов помнить ради сохранения. Страница
/// на экране у Kotlin есть готовая, но на диск ложится markdown, а он здесь.
const DOCUMENTS: usize = 64;
/// Сколько байтов картинок держим, прежде чем вытеснять старые. Столько же,
/// сколько у вкладки окна (`BLOB_BUDGET`).
const BLOB_BUDGET: usize = 48 * 1024 * 1024;

/// Что ядро держит между вызовами.
struct Core {
    store: Store,
    marks: Marks,
    /// Прочитанные документы по адресу — для сохранения.
    documents: VecDeque<(String, Document)>,
    /// Сырые байты уже скачанных картинок по адресу: «назад» декодирует
    /// их из памяти, а не тянет из сети заново.
    blobs: Blobs,
}

#[derive(Default)]
struct Blobs {
    map: HashMap<String, Arc<Vec<u8>>>,
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

static CORE: OnceLock<Mutex<Core>> = OnceLock::new();

fn core() -> std::sync::MutexGuard<'static, Core> {
    let core = CORE.get_or_init(|| {
        Mutex::new(Core {
            store: Store::open(),
            marks: Marks::open(),
            documents: VecDeque::new(),
            blobs: Blobs::default(),
        })
    });
    // Отравленный мьютекс значит, что какой-то вызов упал посреди правки.
    // Ядро от этого не ломается: данные в нём — кэш и журнал, который и так
    // дописывается строкой. Работаем дальше, а не роняем приложение.
    core.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Отдать платформенному проверяющему контекст приложения, назвать папку
/// хранилища и поставить провайдер шифров. Зовётся один раз, из
/// `Application.onCreate`.
#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_gurov_brevier_Core_init<'caller>(
    mut unowned: EnvUnowned<'caller>,
    _class: JClass<'caller>,
    context: JObject<'caller>,
    home: JString<'caller>,
) {
    let outcome = unowned.with_env(|env| -> Result<(), JniError> {
        let home = home.try_to_string(env)?;
        store::set_home(PathBuf::from(home));
        rustls_platform_verifier::android::init_with_env(env, context)?;
        crate::init_crypto();
        // Уборка недельного кэша страниц — раз на запуск и в фоне.
        std::thread::spawn(|| Cache::open().prune());
        Ok(())
    });
    outcome.resolve::<ThrowRuntimeExAndDefault>()
}

/// Действие по имени; ответ — JSON.
#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_gurov_brevier_Core_call<'caller>(
    mut unowned: EnvUnowned<'caller>,
    _class: JClass<'caller>,
    method: JString<'caller>,
    arg: JString<'caller>,
) -> JString<'caller> {
    let outcome = unowned.with_env(|env| -> Result<JString<'caller>, JniError> {
        let method = method.try_to_string(env)?;
        let arg = arg.try_to_string(env)?;
        JString::from_str(env, call(&method, &arg))
    });
    outcome.resolve::<ThrowRuntimeExAndDefault>()
}

/// Картинка: скачать (или взять из кэша) и разобрать под колонку.
///
/// Ответ — байты: первый — ответ, дальше данные. `0`: ширина и высота
/// (по четыре байта, big-endian), затем RGBA построчно. `1`: причина отказа
/// в UTF-8 — её показывают на месте картинки.
#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_gurov_brevier_Core_image<'caller>(
    mut unowned: EnvUnowned<'caller>,
    _class: JClass<'caller>,
    source: JString<'caller>,
    width: i32,
    paper: i32,
    font_size: f32,
    natural: bool,
) -> JByteArray<'caller> {
    let outcome = unowned.with_env(|env| -> Result<JByteArray<'caller>, JniError> {
        let source = source.try_to_string(env)?;
        let look = Look {
            width: width.max(1) as u32,
            paper: [(paper >> 16) as u8, (paper >> 8) as u8, paper as u8],
            font_size,
            fit: if natural { Fit::Natural } else { Fit::Column },
        };
        env.byte_array_from_slice(&image(&source, look))
    });
    outcome.resolve::<ThrowRuntimeExAndDefault>()
}

/// Совпадения поиска: пары «начало, конец» в единицах UTF-16 подряд.
#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_gurov_brevier_Core_find<'caller>(
    mut unowned: EnvUnowned<'caller>,
    _class: JClass<'caller>,
    text: JString<'caller>,
    needle: JString<'caller>,
) -> JIntArray<'caller> {
    let outcome = unowned.with_env(|env| -> Result<JIntArray<'caller>, JniError> {
        let text = text.try_to_string(env)?;
        let needle = needle.try_to_string(env)?;
        let found = page::utf16_offsets(&text, &page::hits(&text, &needle));
        let flat: Vec<i32> = found
            .iter()
            .flat_map(|(from, to)| [*from as i32, *to as i32])
            .collect();
        let array = JIntArray::new(env, flat.len())?;
        array.set_region(env, 0, &flat)?;
        Ok(array)
    });
    outcome.resolve::<ThrowRuntimeExAndDefault>()
}

/// Разобрать вызов. Незнакомое действие — ошибка в JSON, а не паника:
/// Kotlin и ядро собираются вместе, но расхождение должно быть видно.
fn call(method: &str, arg: &str) -> String {
    let fields: Vec<&str> = arg.split(FIELD).collect();
    let field = |at: usize| fields.get(at).copied().unwrap_or("");
    match method {
        "typography" => typography(),
        "intro" => intro(),
        "parse" => parse(arg),
        "open" => open(field(0).parse().unwrap_or(0), field(1), field(2) == "1"),
        "follow" => follow(field(0), field(1)),
        "suggest" => suggest(arg),
        "title" => title(arg),
        "kept" => format!("{{\"kept\":{}}}", core().marks.has(arg)),
        "bookmark" => bookmark(field(0).parse().unwrap_or(0), field(1), field(2)),
        "settings" => settings(),
        "settings.save" => {
            let mut settings = Settings::load();
            settings.dark = field(0) == "1";
            settings.images = field(1) == "1";
            settings.save();
            "{}".to_owned()
        }
        "session" => session(),
        "session.save" => {
            remember(arg);
            "{}".to_owned()
        }
        "forget" => {
            core().store.forget();
            // Копии страниц — такой же след прочитанного, как журнал.
            Cache::open().forget();
            "{}".to_owned()
        }
        "docs" => docs(arg),
        "savename" => savename(arg),
        "save" => save(field(0), field(1)),
        other => {
            let mut out = String::from("{\"error\":");
            json_string(&mut out, &format!("unknown call: {other}"));
            out.push('}');
            out
        }
    }
}

/// Типографская модель и цвета — одним ответом. Kotlin чисел не повторяет:
/// мера, кегли и отступы у статьи одни на обеих платформах.
fn typography() -> String {
    let list = |values: &[f32]| {
        values
            .iter()
            .map(|value| value.to_string())
            .collect::<Vec<_>>()
            .join(",")
    };
    let weights = outline::HEADING_WEIGHTS
        .iter()
        .map(|weight| weight.to_string())
        .collect::<Vec<_>>()
        .join(",");
    let theme = |dark: bool| {
        let colors = palette::colors(dark);
        let (paper, ink) = palette::paper_and_ink(dark);
        let shelf = if dark {
            palette::SHELF_DARK
        } else {
            palette::SHELF_LIGHT
        };
        format!(
            "{{\"paper\":\"{paper}\",\"ink\":\"{ink}\",\"shelf\":\"{shelf}\",\
             \"link\":\"{}\",\"dim\":\"{}\",\"panel\":\"{}\",\"keyword\":\"{}\",\
             \"literal\":\"{}\",\"number\":\"{}\",\"comment\":\"{}\",\"rule\":\"{}\",\
             \"chosen\":\"{}\",\"touched\":\"{}\"}}",
            colors.link,
            colors.dim,
            colors.panel,
            colors.keyword,
            colors.literal,
            colors.number,
            colors.comment,
            colors.rule,
            colors.chosen,
            colors.touched,
        )
    };
    format!(
        "{{\"textSize\":{},\"textPx\":{},\"lineHeight\":{},\"headingLineHeight\":{},\
         \"headings\":[{}],\"headingWeights\":[{}],\"measureInEms\":{},\
         \"zoomSteps\":[{}],\"zoomNormal\":{},\
         \"indent\":{},\"hang\":{},\"noteIndent\":{},\"noteHang\":{},\"codeGap\":{},\
         \"codeSize\":{},\"padSize\":{},\"noteSize\":{},\"noterefSize\":{},\
         \"noterefRise\":{},\"alertSize\":{},\"alertTracking\":{},\
         \"ruleWidth\":{},\"ruleX\":{},\"ruleInset\":{},\
         \"listLevels\":{},\"quoteLevels\":{},\"hints\":{},\
         \"found\":\"{}\",\"foundHere\":\"{}\",\"foundInk\":\"{}\",\
         \"light\":{},\"dark\":{}}}",
        outline::TEXT_SIZE,
        outline::TEXT_PX,
        outline::LINE_HEIGHT,
        outline::HEADING_LINE_HEIGHT,
        list(&outline::HEADINGS),
        weights,
        outline::MEASURE_IN_EMS,
        list(&outline::ZOOM_STEPS),
        outline::ZOOM_NORMAL,
        outline::INDENT,
        outline::HANG,
        outline::NOTE_INDENT,
        outline::NOTE_HANG,
        outline::CODE_GAP,
        outline::CODE_SIZE,
        outline::PAD_SIZE,
        outline::NOTE_SIZE,
        outline::NOTEREF_SIZE,
        outline::NOTEREF_RISE,
        outline::ALERT_SIZE,
        outline::ALERT_TRACKING,
        outline::RULE_WIDTH,
        outline::RULE_X,
        outline::RULE_INSET,
        page::LIST_LEVELS,
        page::QUOTE_LEVELS,
        HINTS,
        palette::FOUND,
        palette::FOUND_HERE,
        palette::FOUND_INK,
        theme(false),
        theme(true),
    )
}

/// Начальная страница: тот же тракт, что у статьи.
fn intro() -> String {
    let document = Document {
        address: Address::Web(String::new()),
        title: crate::intro::TITLE.to_owned(),
        markdown: crate::intro::TOUCH.to_owned(),
        kind: Kind::Article,
        served: false,
        site: Vec::new(),
        lang: None,
    };
    let mut out = String::from("{\"title\":");
    json_string(&mut out, &document.title);
    out.push_str(",\"page\":");
    out.push_str(&Page::of(&document).to_json());
    out.push('}');
    out
}

/// Разобрать напечатанное. Окну это нужно до загрузки: в историю вкладки
/// идёт адрес, а не то, что набрали.
fn parse(typed: &str) -> String {
    match address::parse(typed) {
        Ok(address) => {
            let mut out = String::from("{\"ok\":true,");
            address_fields(&mut out, &address);
            out.push('}');
            out
        }
        Err(error) => failure_json(&describe(&error), None),
    }
}

/// Поля адреса в ответе: как показать, чем открыть снаружи, свой ли он.
fn address_fields(out: &mut String, address: &Address) {
    out.push_str("\"address\":");
    json_string(out, &address.display());
    out.push_str(",\"external\":");
    json_string(out, &address.external());
    out.push_str(&format!(
        ",\"internal\":{},\"insecure\":{}",
        address.is_internal(),
        address.display().starts_with("http://")
    ));
    // Проверка этой страницы — адресом, который решает ядро: веб да,
    // репозиторий и свои страницы нет.
    if let Some(check) = address.check() {
        out.push_str(",\"check\":");
        json_string(out, &check.display());
    }
    // Проект — для полки: точки входа в документацию принадлежат ему,
    // а не файлу, и переход между файлами одного проекта их не ищет заново.
    if let Address::Repo(repo) = address {
        out.push_str(",\"project\":");
        json_string(
            out,
            &format!("{:?}/{}/{}", repo.host, repo.owner, repo.name),
        );
    }
}

fn failure_json(failure: &crate::failure::Failure, external: Option<String>) -> String {
    let mut out = String::from("{\"ok\":false,\"headline\":");
    json_string(&mut out, failure.headline);
    out.push_str(",\"detail\":");
    json_string(&mut out, &failure.detail);
    out.push_str(",\"page\":");
    out.push_str(&Page::message(failure.headline, &failure.detail).to_json());
    if failure.offer_browser
        && let Some(external) = external.filter(|external| !external.is_empty())
    {
        out.push_str(",\"offer\":");
        json_string(&mut out, &external);
    }
    out.push('}');
    out
}

/// Открыть адрес: недельная копия или сеть, извлечение, раскладка — и запись
/// в журнал. `fresh` — мимо копии, прямо из сети (перезагрузка).
fn open(offset: i32, typed: &str, fresh: bool) -> String {
    let address = match address::parse(typed) {
        Ok(address) => address,
        Err(error) => return failure_json(&describe(&error), None),
    };
    let external = address.external();
    let anchor = page::anchor_in(&address);
    let disk = Cache::open();
    let copy = (!fresh).then(|| disk.page(&address)).flatten();
    let saved = copy.as_ref().map(|copy| copy.saved);
    let document = match copy {
        Some(copy) => copy.document,
        None => match crate::open(&address, UserAgent::Honest) {
            Ok(document) => {
                disk.keep(&address, &document);
                document
            }
            Err(error) => return failure_json(&describe(&error), Some(external)),
        },
    };
    let page = Page::of(&document);

    let kept = {
        let mut core = core();
        // В историю идёт то, что открылось, и адрес итоговый — после
        // редиректов.
        core.store
            .record(&document.address, &document.title, offset);
        let key = document.address.display();
        core.documents.retain(|(known, _)| *known != key);
        core.documents.push_back((key.clone(), document.clone()));
        while core.documents.len() > DOCUMENTS {
            core.documents.pop_front();
        }
        core.marks.has(&key)
    };

    let mut out = String::from("{\"ok\":true,");
    address_fields(&mut out, &document.address);
    out.push_str(",\"title\":");
    json_string(&mut out, &document.title);
    out.push_str(&format!(
        ",\"listing\":{},\"served\":{},\"kept\":{kept}",
        document.kind == Kind::Listing,
        document.served,
    ));
    // Страница из копии говорит об этом: это не то, что на сайте сейчас.
    if let Some(saved) = saved {
        out.push_str(",\"copy\":");
        json_string(&mut out, &cache::copy_note(saved));
    }
    if let Some(anchor) = anchor {
        out.push_str(",\"anchor\":");
        json_string(&mut out, &anchor);
    }
    // Навигация сайта. Адрес разбираем нашим же разбором: ссылка на github
    // из меню должна открыться режимом репозитория, как ссылка из текста.
    out.push_str(",\"site\":[");
    let mut first = true;
    for link in &document.site {
        let Ok(target) = address::parse(&link.address) else {
            continue;
        };
        if !first {
            out.push(',');
        }
        first = false;
        entry_json(&mut out, &link.title, &target);
    }
    out.push(']');
    if let Some(directory) = repo::directory_of(&document.address) {
        out.push_str(",\"directory\":");
        entry_json(&mut out, "Files in this directory", &directory);
    }
    out.push_str(",\"page\":");
    out.push_str(&page.to_json());
    out.push('}');
    out
}

fn entry_json(out: &mut String, title: &str, address: &Address) {
    out.push_str("{\"title\":");
    json_string(out, title);
    out.push_str(",\"address\":");
    json_string(out, &address.display());
    out.push('}');
}

/// Что делать с нажатой ссылкой: прыгнуть внутри страницы или открыть адрес.
fn follow(here: &str, target: &str) -> String {
    let here = (!here.is_empty()).then_some(here);
    if let Some(anchor) = page::fragment_of(target, here) {
        let mut out = String::from("{\"jump\":");
        json_string(&mut out, &anchor);
        out.push('}');
        return out;
    }
    match address::parse(target) {
        Ok(address) => {
            let mut out = String::from("{\"open\":");
            json_string(&mut out, &address.display());
            out.push('}');
            out
        }
        Err(error) => {
            let mut out = String::from("{\"error\":");
            json_string(&mut out, describe(&error).headline);
            out.push('}');
            out
        }
    }
}

fn suggest(typed: &str) -> String {
    let hints = core().store.suggest(typed, HINTS);
    let mut out = String::from("[");
    for (index, hint) in hints.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        out.push_str("{\"address\":");
        json_string(&mut out, &hint.address);
        out.push_str(",\"title\":");
        json_string(&mut out, &hint.title);
        out.push('}');
    }
    out.push(']');
    out
}

/// Под каким заголовком адрес читали в прошлый раз — корешку вкладки,
/// которую вернули из сессии, но ещё не открыли.
fn title(address: &str) -> String {
    let mut out = String::from("{\"title\":");
    match core().store.title_of(address) {
        Some(title) => json_string(&mut out, title),
        None => out.push_str("null"),
    }
    out.push('}');
    out
}

fn bookmark(offset: i32, typed: &str, title: &str) -> String {
    let Ok(address) = address::parse(typed) else {
        return "{\"kept\":false}".to_owned();
    };
    let kept = core().marks.toggle(&address, title, offset);
    format!("{{\"kept\":{kept}}}")
}

fn settings() -> String {
    let settings = Settings::load();
    format!(
        "{{\"dark\":{},\"images\":{}}}",
        settings.dark, settings.images
    )
}

fn session() -> String {
    let mut out = String::from("[");
    for (index, tab) in store::session().iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        out.push_str("{\"addresses\":[");
        for (index, address) in tab.addresses.iter().enumerate() {
            if index > 0 {
                out.push(',');
            }
            json_string(&mut out, address);
        }
        out.push_str(&format!(
            "],\"at\":{},\"place\":{},\"current\":{}}}",
            tab.at, tab.place, tab.current
        ));
    }
    out.push(']');
    out
}

/// Запомнить вкладки. Строка на вкладку: «текущая», место в истории,
/// место в тексте и адреса — полями через `U+001F`.
fn remember(arg: &str) {
    let tabs: Vec<store::Opened> = arg
        .lines()
        .filter_map(|line| {
            let mut fields = line.split(FIELD);
            let current = fields.next()? == "1";
            let at = fields.next()?.parse().ok()?;
            let place = fields.next()?.parse().ok()?;
            let addresses: Vec<String> = fields
                .filter(|address| !address.is_empty())
                .map(str::to_owned)
                .collect();
            Some(store::Opened {
                addresses,
                at,
                place,
                current,
            })
        })
        .collect();
    store::remember(&tabs);
}

/// Точки входа в документацию проекта. Дюжина проб на CDN: зовётся фоном,
/// после того как страница уже на экране.
fn docs(typed: &str) -> String {
    let Ok(Address::Repo(repo)) = address::parse(typed) else {
        return "[]".to_owned();
    };
    let found = repo::documentation(&repo, UserAgent::Honest);
    let mut out = String::from("[");
    for (index, entry) in found.into_iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        let target = repo::entry_address(&repo, entry.path);
        entry_json(&mut out, &entry.title, &target);
    }
    out.push(']');
    out
}

fn document(key: &str) -> Option<Document> {
    core()
        .documents
        .iter()
        .rev()
        .find(|(known, _)| known == key)
        .map(|(_, document)| document.clone())
}

fn savename(key: &str) -> String {
    let mut out = String::from("{\"name\":");
    match document(key) {
        Some(document) => json_string(&mut out, &save::suggested_name(&document)),
        None => out.push_str("null"),
    }
    out.push('}');
    out
}

/// Сохранить статью в названный файл. Файл — во временной папке
/// приложения: куда его положить у читателя, решает системный диалог,
/// и копирует туда Kotlin.
fn save(key: &str, path: &str) -> String {
    let Some(document) = document(key) else {
        return "{\"ok\":false,\"headline\":\"Nothing to save yet\"}".to_owned();
    };
    match save::write(std::path::Path::new(path), &document, UserAgent::Honest) {
        Ok(saved) => format!(
            "{{\"ok\":true,\"images\":{},\"missed\":{}}}",
            saved.images, saved.missed
        ),
        Err(error) => {
            let mut out = String::from("{\"ok\":false,\"headline\":");
            json_string(&mut out, describe(&error).headline);
            out.push('}');
            out
        }
    }
}

/// Скачать или взять из кэша, затем разобрать.
fn image(source: &str, look: Look) -> Vec<u8> {
    let source = if source.starts_with("http://") || source.starts_with("https://") {
        Source::Web(source.to_owned())
    } else {
        Source::File(PathBuf::from(source))
    };
    let cached = match &source {
        Source::Web(url) => core().blobs.get(url),
        Source::File(_) => None,
    };
    // Нет в памяти — недельная копия на диске, её тоже в память.
    let disk = Cache::open();
    let cached = cached.or_else(|| match &source {
        Source::Web(url) => disk.image(url).map(|bytes| {
            let bytes = Arc::new(bytes);
            core().blobs.put(url.clone(), bytes.clone());
            bytes
        }),
        Source::File(_) => None,
    });
    let decoded = match cached {
        Some(bytes) => media::decode(&bytes, None, look),
        // В кэш — только то, что разобралось: битые байты хранить незачем.
        None => media::grab(&source, UserAgent::Honest).and_then(|(bytes, mime)| {
            let raster = media::decode(&bytes, mime.as_deref(), look)?;
            if let Source::Web(url) = &source {
                disk.keep_image(url, &bytes);
                core().blobs.put(url.clone(), Arc::new(bytes));
            }
            Ok(raster)
        }),
    };
    match decoded {
        Ok(raster) => {
            let mut out = Vec::with_capacity(9 + raster.rgba.len());
            out.push(0);
            out.extend_from_slice(&raster.width.to_be_bytes());
            out.extend_from_slice(&raster.height.to_be_bytes());
            out.extend_from_slice(&raster.rgba);
            out
        }
        Err(error) => {
            let mut out = vec![1];
            out.extend_from_slice(describe(&error).headline.as_bytes());
            out
        }
    }
}
