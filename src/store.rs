//! Что и как храним на диске.
//!
//! Решено 12 сентября 2026, подсмотрено у зрелых браузеров и упрощено
//! до того, что этому продукту действительно нужно.
//!
//! **Место — по соглашениям ОС, а не одной папкой-профилем.** Chrome
//! и Firefox держат историю, настройки и кэш в одном профиле; так сложилось
//! исторически, и держится это на том, что профилей у них много. У нас
//! профиль один, зато есть три разных судьбы у данных: историю и закладки
//! читатель бы унёс с собой, настройки правил бы руками, а кэш выбросил
//! не глядя. Поэтому раскладка обычная для настольного приложения:
//! `$XDG_DATA_HOME/brevier` (история, закладки, сессия),
//! `$XDG_CONFIG_HOME/brevier` (настройки), `$XDG_CACHE_HOME/brevier` (кэш).
//! На macOS — `~/Library/Application Support/Brevier` и `~/Library/Caches`,
//! на Windows — `%LOCALAPPDATA%\Brevier`.
//!
//! **Формат — строки, а не база.** У браузеров это SQLite, и для их объёмов
//! это правильно. Нам он стоил бы С-библиотеки в проекте, который продаёт
//! memory safety, — той же ценой мы уже отказались от `syntect`. История
//! чтения одного человека это тысячи строк, а не миллионы; дописывание
//! строки в конец файла переживает падение программы не хуже, а прочитать
//! и починить свой файл читатель может обычным редактором. Разделитель —
//! табуляция, потому что она единственная не встречается в адресах
//! и заголовках сама собой; что всё-таки встретилось — экранируем.
//!
//! **Пишет только окно.** `brevier <url> | less` — инструмент конвейера,
//! и молча писать в историю читателя он не должен; в браузерах то же
//! правило действует для headless-режима.
//!
//! Жильцов здесь двое: журнал посещённого (`history.tsv`) и сессия —
//! открытые вкладки (`session.tsv`). Формат у них один, и это то самое,
//! ради чего вопрос решался целиком: второе хранилище со своей судьбой
//! не заводится.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::address::Address;

/// Сколько визитов держим. Chrome хранит 90 дней, Firefox — около 180;
/// мера у них времени, у нас — числа строк, потому что за размер файла
/// отвечает именно оно. Пять тысяч визитов это годы чтения и меньше
/// полумегабайта.
const KEEP: usize = 5000;

/// Сколько подсказок показываем. Больше восьми — это уже не подсказка,
/// а список, который надо читать.
pub const HINTS: usize = 8;

/// Один визит: когда, куда, что там было.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Visit {
    pub stamp: Stamp,
    /// Адрес в том виде, в каком его показывает строка, — он же разбирается
    /// обратно (`address::parse`), поэтому хранить разобранный вид незачем.
    pub address: String,
    pub title: String,
}

/// Подсказка адресной строки: что показать и что подставить.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hint {
    pub address: String,
    pub title: String,
    /// Сколько раз читатель сюда приходил. Видно в подсказке не будет,
    /// но по нему они и упорядочены.
    pub visits: usize,
}

/// История посещённого.
///
/// Журнал, а не таблица: визит дописывается строкой в конец. Свод
/// по адресам считается при чтении — тысячи строк это микросекунды,
/// а журнал переживает обрыв и правится руками.
#[derive(Debug, Default)]
pub struct Store {
    path: Option<PathBuf>,
    visits: Vec<Visit>,
    /// Удалось ли открыть файл на запись. Если нет, история не ведётся,
    /// и сказать об этом надо прямо — на самой странице истории.
    writable: bool,
}

impl Store {
    /// Открыть историю в обычном месте. Не получилось — работаем без неё:
    /// читалка без истории читает, а падать из-за неё нельзя.
    pub fn open() -> Self {
        match data_dir() {
            Some(dir) => Self::at(dir.join("history.tsv")),
            None => Self::default(),
        }
    }

    /// Открыть историю в названном файле. Отдельно от [`open`](Self::open)
    /// ради тестов: настоящую историю читателя они трогать не должны.
    pub fn at(path: impl Into<PathBuf>) -> Self {
        let path = path.into();
        let text = fs::read_to_string(&path).unwrap_or_default();
        let mut visits: Vec<Visit> = text.lines().filter_map(parse_line).collect();

        // Журнал подрезаем при открытии, а не при каждой записи: файл растёт
        // по строчке в минуту, и переписывать его ради этого каждый раз
        // значит менять дешёвое дописывание на дорогую перезапись.
        let long = visits.len() > KEEP;
        if long {
            visits.drain(..visits.len() - KEEP);
        }

        let writable = match path.parent() {
            Some(dir) => fs::create_dir_all(dir).is_ok(),
            None => true,
        };
        let mut store = Self {
            path: Some(path),
            visits,
            writable,
        };
        if long {
            store.rewrite();
        }
        store
    }

    pub fn visits(&self) -> &[Visit] {
        &self.visits
    }

    /// Записать визит. Смещение от UTC приносит тот, кто записывает:
    /// у ядра часового пояса нет, а у окна он есть (`GLib`). Записанное
    /// смещение потом и показывает время — даже если читатель переехал
    /// или сменилось летнее время.
    pub fn record(&mut self, address: &Address, title: &str, offset: i32) {
        // Внутренние страницы в историю не идут: список истории, стоящий
        // в списке истории, — мусор. Браузеры поступают так же со своими
        // `chrome://`.
        if matches!(address, Address::Internal(_)) {
            return;
        }
        let address = address.display();
        // Перезагрузка страницы записью не считается — ровно то же правило,
        // что и у истории вперёд-назад (`history.rs`).
        if self.visits.last().map(|last| last.address.as_str()) == Some(address.as_str()) {
            return;
        }
        let visit = Visit {
            stamp: Stamp::now(offset),
            // Заголовок пишет не наш код: в нём попадаются и перевод строки,
            // и двойные пробелы. В строке журнала и в строке подсказки
            // он обязан быть одной строкой.
            title: title.split_whitespace().collect::<Vec<_>>().join(" "),
            address,
        };
        self.append(&visit);
        self.visits.push(visit);
    }

    /// Похожие на напечатанное адреса, лучшие сверху.
    ///
    /// Порядок как у зрелых браузеров и по тем же причинам: сначала то,
    /// что совпало с начала имени хоста (человек печатает адрес слева
    /// направо), потом совпавшее где-то внутри адреса, потом совпавшее
    /// только заголовком. Внутри разряда — куда ходили чаще, а при равном
    /// счёте позже.
    pub fn suggest(&self, typed: &str, limit: usize) -> Vec<Hint> {
        let needle = typed.trim().to_lowercase();
        if needle.is_empty() {
            return Vec::new();
        }

        let mut counts: HashMap<&str, usize> = HashMap::new();
        for visit in &self.visits {
            *counts.entry(visit.address.as_str()).or_default() += 1;
        }

        let mut best: Vec<(u8, usize, usize, Hint)> = Vec::new();
        let mut seen: HashSet<&str> = HashSet::new();
        for (age, visit) in self.visits.iter().enumerate().rev() {
            let address = visit.address.to_lowercase();
            // Схему в сравнение не берём вовсе: её не печатают, зато буква
            // `s` из `https` иначе совпадает с половиной истории.
            let searchable = host_start(&address)
                .and_then(|start| address.get(start..))
                .unwrap_or(&address);
            let rank = if searchable.starts_with(&needle) {
                3
            } else if searchable.contains(&needle) {
                2
            } else if visit.title.to_lowercase().contains(&needle) {
                1
            } else {
                continue;
            };
            // Один адрес — одна подсказка: заголовок берём у самого свежего
            // визита, он же встретится первым.
            if !seen.insert(visit.address.as_str()) {
                continue;
            }
            let visits = counts.get(visit.address.as_str()).copied().unwrap_or(1);
            best.push((
                rank,
                visits,
                age,
                Hint {
                    address: visit.address.clone(),
                    title: visit.title.clone(),
                    visits,
                },
            ));
        }

        best.sort_by(|a, b| b.0.cmp(&a.0).then(b.1.cmp(&a.1)).then(b.2.cmp(&a.2)));
        best.into_iter()
            .take(limit)
            .map(|(.., hint)| hint)
            .collect()
    }

    /// Страница истории — обычный markdown, как всё остальное в продукте.
    ///
    /// Устроена как у браузеров: сверху свежее, записи сгруппированы по дням,
    /// у каждой время и заголовок ссылкой. Своего вида у неё нет и не нужно:
    /// markdown — внутреннее представление, а рисует его тот же отрисовщик,
    /// что и статью, поэтому история набрана той же типографикой.
    pub fn page(&self) -> String {
        let mut out = String::from("# History\n\n");

        if !self.writable {
            out.push_str(&format!(
                "**History is not being saved.** {} cannot be written.\n\n",
                self.path
                    .as_deref()
                    .map(where_it_is)
                    .unwrap_or_else(|| "The history file".to_owned()),
            ));
        }

        if self.visits.is_empty() {
            out.push_str("Nothing here yet. Pages you read show up on this list.\n");
            return out;
        }

        // «Сегодня» считаем по часам последней записи: своего часового пояса
        // у ядра нет, а у визита он записан. Читатель, закрывший программу
        // вчера, увидит вчерашний день вчерашним.
        let offset = self
            .visits
            .last()
            .map(|visit| visit.stamp.offset)
            .unwrap_or(0);
        let today = Stamp::now(offset).days;

        let mut day = None;
        for visit in self.visits.iter().rev() {
            if day != Some(visit.stamp.days) {
                // Пустая строка между днями, но не перед первым: заголовок
                // страницы и так стоит через отбивку.
                if day.is_some() {
                    out.push('\n');
                }
                day = Some(visit.stamp.days);
                out.push_str(&format!("## {}\n\n", name_of_day(visit.stamp.days, today)));
            }
            let title = if visit.title.is_empty() {
                visit.address.clone()
            } else {
                visit.title.clone()
            };
            // Откуда страница, приписываем только если адрес этого не говорит
            // сам: у `gh:owner/repo` источник и адрес — одна и та же строка.
            let source = source_of(&visit.address);
            let source = if source == visit.address {
                String::new()
            } else {
                format!(" — {source}")
            };
            out.push_str(&format!(
                "- {} {}{source}\n",
                visit.stamp.clock(),
                link(&title, &visit.address),
            ));
        }

        if let Some(path) = self.path.as_deref().filter(|_| self.writable) {
            out.push_str(&format!(
                "\n---\n\nThis list is a plain text file: {}. Delete a line to forget a page.\n",
                where_it_is(path),
            ));
        }
        out
    }

    /// Дописать строку в конец журнала. Ошибку глотаем: сказать о ней
    /// есть где — на самой странице истории, — а ронять чтение нельзя.
    fn append(&mut self, visit: &Visit) {
        let Some(path) = &self.path else { return };
        if !self.writable {
            return;
        }
        let line = format!(
            "{}\t{}\t{}\n",
            visit.stamp.text(),
            escape(&visit.address),
            escape(&visit.title)
        );
        let written = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .and_then(|mut file| file.write_all(line.as_bytes()));
        self.writable = written.is_ok();
    }

    /// Переписать журнал целиком — после подрезки.
    fn rewrite(&mut self) {
        let Some(path) = &self.path else { return };
        if !self.writable {
            return;
        }
        let mut text = String::new();
        for visit in &self.visits {
            text.push_str(&format!(
                "{}\t{}\t{}\n",
                visit.stamp.text(),
                escape(&visit.address),
                escape(&visit.title)
            ));
        }
        self.writable = fs::write(path, text).is_ok();
    }
}

/// Сколько шагов «назад» помним на вкладку. Больше полусотни не помнит
/// и сам читатель, а файл сессии должен оставаться обозримым.
const DEPTH: usize = 50;

/// Вкладка, какой её застали: весь её путь, место в этом пути и место
/// в тексте.
///
/// Хранить только текущий адрес было бы дешевле, но вернувшаяся вкладка
/// с мёртвыми «назад» и «вперёд» — это не та вкладка, которую закрыли.
/// Браузеры хранят то же самое и по той же причине.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Opened {
    pub addresses: Vec<String>,
    /// Какой из адресов открыт сейчас.
    pub at: usize,
    /// Смещение в буфере той строки, что стояла у верхнего края. Не пиксели:
    /// они зависят от ширины окна, кегля и ступени масштаба, а смещение
    /// в тексте — ни от чего. Тем же приёмом держится место при смене
    /// масштаба (`redraw`).
    pub place: i32,
    /// Эта вкладка была впереди.
    pub current: bool,
}

/// Строка-пояснение в начале файла сессии. Файл читательский, и он должен
/// объяснять себя сам — как и журнал истории, только там формат очевиден
/// из данных, а здесь нет.
const SESSION_HEADER: &str =
    "# Brevier session: here|tab <entry you were on> <scroll offset> <addresses…>\n";

/// Что было открыто в прошлый раз.
pub fn session() -> Vec<Opened> {
    match data_dir() {
        Some(dir) => session_at(dir.join("session.tsv")),
        None => Vec::new(),
    }
}

/// Запомнить открытое.
pub fn remember(tabs: &[Opened]) {
    if let Some(dir) = data_dir() {
        remember_at(dir.join("session.tsv"), tabs);
    }
}

/// То же, но в названном файле — отдельно ради тестов, как и `Store::at`:
/// сессию читателя они трогать не должны.
pub fn session_at(path: impl AsRef<Path>) -> Vec<Opened> {
    let text = fs::read_to_string(path.as_ref()).unwrap_or_default();
    text.lines().filter_map(parse_tab).collect()
}

/// Пишется целиком: вкладок десятки, а не тысячи, и дописывать тут нечего —
/// сессия это не журнал, а слепок.
pub fn remember_at(path: impl AsRef<Path>, tabs: &[Opened]) {
    let path = path.as_ref();
    if let Some(dir) = path.parent()
        && fs::create_dir_all(dir).is_err()
    {
        return;
    }
    let mut text = String::from(SESSION_HEADER);
    for tab in tabs {
        // Вкладка без адреса — это начальная страница; запоминать в ней
        // нечего, а восстанавливать её незачем: пустая вкладка и так
        // заводится сама.
        if tab.addresses.is_empty() {
            continue;
        }
        // Хвост истории режем со стороны старого, а место пересчитываем:
        // выбросить то, на чём стоим, было бы хуже, чем забыть начало пути.
        let extra = tab.addresses.len().saturating_sub(DEPTH);
        let kept = &tab.addresses[extra..];
        let at = tab.at.saturating_sub(extra).min(kept.len() - 1);

        text.push_str(if tab.current { "here" } else { "tab" });
        text.push_str(&format!("\t{at}\t{}", tab.place.max(0)));
        for address in kept {
            text.push('\t');
            text.push_str(&escape(address));
        }
        text.push('\n');
    }
    let _ = fs::write(path, text);
}

fn parse_tab(line: &str) -> Option<Opened> {
    if line.starts_with('#') || line.trim().is_empty() {
        return None;
    }
    let mut parts = line.split('\t');
    let current = match parts.next()? {
        "here" => true,
        "tab" => false,
        _ => return None,
    };
    let at: usize = parts.next()?.parse().ok()?;
    let place: i32 = parts.next()?.parse().ok()?;
    let addresses: Vec<String> = parts.map(unescape).filter(|a| !a.is_empty()).collect();
    if addresses.is_empty() {
        return None;
    }
    Some(Opened {
        at: at.min(addresses.len() - 1),
        place: place.max(0),
        addresses,
        current,
    })
}

/// Куда кладём то, что переживает запуск. Данные, настройки и кэш — три
/// разные папки: см. шапку модуля.
pub fn data_dir() -> Option<PathBuf> {
    // Своя папка старше всего остального: ей открывают чистый профиль,
    // как `--user-data-dir` у браузеров.
    if let Some(own) = std::env::var_os("BREVIER_DATA_DIR") {
        return Some(PathBuf::from(own));
    }
    place("XDG_DATA_HOME", ".local/share", "Application Support")
}

/// Где лежат настройки. Пока ими никто не пользуется — раскладку решаем
/// один раз и целиком, иначе заведётся второе хранилище со своей судьбой.
pub fn config_dir() -> Option<PathBuf> {
    if let Some(own) = std::env::var_os("BREVIER_DATA_DIR") {
        return Some(PathBuf::from(own));
    }
    place("XDG_CONFIG_HOME", ".config", "Application Support")
}

/// Где лежит то, что не жалко потерять.
pub fn cache_dir() -> Option<PathBuf> {
    if let Some(own) = std::env::var_os("BREVIER_DATA_DIR") {
        return Some(PathBuf::from(own));
    }
    place("XDG_CACHE_HOME", ".cache", "Caches")
}

fn place(variable: &str, under_home: &str, apple: &str) -> Option<PathBuf> {
    let name = if cfg!(target_os = "linux") {
        "brevier"
    } else {
        "Brevier"
    };

    if cfg!(target_os = "windows") {
        let base = std::env::var_os("LOCALAPPDATA").map(PathBuf::from)?;
        return Some(base.join(name));
    }

    if cfg!(target_os = "macos") {
        let home = std::env::var_os("HOME").map(PathBuf::from)?;
        return Some(home.join("Library").join(apple).join(name));
    }

    let base = match std::env::var_os(variable) {
        Some(set) if !set.is_empty() => PathBuf::from(set),
        _ => std::env::var_os("HOME")
            .map(PathBuf::from)?
            .join(under_home),
    };
    Some(base.join(name))
}

/// Путь в том виде, в каком его показывают читателю: домашняя папка
/// сокращается тильдой, как принято в любой консоли.
fn where_it_is(path: &Path) -> String {
    let shown = match std::env::var_os("HOME").map(PathBuf::from) {
        Some(home) => match path.strip_prefix(&home) {
            Ok(rest) => format!("~/{}", rest.display()),
            Err(_) => path.display().to_string(),
        },
        None => path.display().to_string(),
    };
    format!("`{shown}`")
}

/// Откуда страница. У браузеров в этом месте стоит домен: адрес спрятан
/// под заголовком, и без домена два одинаковых заголовка не различить.
fn source_of(address: &str) -> String {
    if let Some(start) = host_start(address) {
        let host = &address[start..];
        let host = host.split(['/', '?', '#']).next().unwrap_or(host);
        return host.trim_start_matches("www.").to_owned();
    }
    // Репозиторий: `gh:owner/name/path` — источник тут проект, а не хост.
    if let Some((prefix, rest)) = address.split_once(':') {
        let mut parts = rest.trim_start_matches('/').split('/');
        if let (Some(owner), Some(name)) = (parts.next(), parts.next()) {
            return format!("{prefix}:{owner}/{name}");
        }
    }
    "file".to_owned()
}

/// Где в адресе начинается имя хоста. Печатают адрес слева направо,
/// но схему при этом чаще опускают — значит совпадение «с начала»
/// считается от хоста, а не от первого знака строки.
fn host_start(address: &str) -> Option<usize> {
    address.find("://").map(|at| at + 3)
}

/// Ссылка markdown-ом. Заголовок страницы пишет не наш код, поэтому
/// квадратные скобки в нём — обычное дело, и экранировать их обязаны мы.
fn link(text: &str, target: &str) -> String {
    let text = text
        .replace('\\', r"\\")
        .replace('[', r"\[")
        .replace(']', r"\]");
    if target.contains(['(', ')', ' ', '<', '>']) {
        format!("[{text}](<{target}>)")
    } else {
        format!("[{text}]({target})")
    }
}

/// Как называется день. Сегодня и вчера — словами, дальше датой:
/// ровно так устроена страница истории в браузерах, и по делу — «вторник,
/// 9 сентября» ищется глазами быстрее, чем «три дня назад».
fn name_of_day(days: i64, today: i64) -> String {
    match today - days {
        0 => "Today".to_owned(),
        1 => "Yesterday".to_owned(),
        _ => {
            let (year, month, day) = civil_from_days(days);
            format!(
                "{}, {day} {} {year}",
                WEEKDAYS[weekday_of(days)],
                MONTHS[month as usize - 1],
            )
        }
    }
}

const WEEKDAYS: [&str; 7] = [
    "Thursday",
    "Friday",
    "Saturday",
    "Sunday",
    "Monday",
    "Tuesday",
    "Wednesday",
];

const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

/// День недели. Отсчёт от эпохи: 1 января 1970 был четвергом, отсюда
/// и порядок в таблице выше.
fn weekday_of(days: i64) -> usize {
    days.rem_euclid(7) as usize
}

/// Отметка времени: местные часы плюс смещение, с которым их записали.
///
/// Хранится в виде ISO-8601 (`2026-09-12T14:23:05+03:00`) — единственный
/// формат даты, который человек читает без пособия, а разбирается он
/// подстрокой. Смещение записано в самой отметке, поэтому показ времени
/// не требует знать часовой пояс вовсе.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stamp {
    /// Суток от эпохи по местным часам.
    pub days: i64,
    /// Секунд от полуночи по ним же.
    pub seconds: i32,
    /// Смещение от UTC в секундах.
    pub offset: i32,
}

impl Stamp {
    pub fn now(offset: i32) -> Self {
        let unix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|since| since.as_secs() as i64)
            .unwrap_or_default();
        Self::of_unix(unix, offset)
    }

    pub fn of_unix(unix: i64, offset: i32) -> Self {
        let local = unix + offset as i64;
        Self {
            days: local.div_euclid(86_400),
            seconds: local.rem_euclid(86_400) as i32,
            offset,
        }
    }

    /// Часы и минуты — то, что видно на странице истории.
    pub fn clock(&self) -> String {
        format!("{:02}:{:02}", self.seconds / 3600, self.seconds % 3600 / 60)
    }

    /// Как отметка лежит в файле.
    pub fn text(&self) -> String {
        let (year, month, day) = civil_from_days(self.days);
        let (hour, minute, second) = (
            self.seconds / 3600,
            self.seconds % 3600 / 60,
            self.seconds % 60,
        );
        let zone = if self.offset == 0 {
            "Z".to_owned()
        } else {
            let sign = if self.offset < 0 { '-' } else { '+' };
            let away = self.offset.abs();
            format!("{sign}{:02}:{:02}", away / 3600, away % 3600 / 60)
        };
        format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}{zone}")
    }

    fn parse(text: &str) -> Option<Self> {
        let (date, rest) = text.split_once('T')?;
        let mut parts = date.split('-');
        let year: i64 = parts.next()?.parse().ok()?;
        let month: u32 = parts.next()?.parse().ok()?;
        let day: u32 = parts.next()?.parse().ok()?;

        let (clock, zone) = match rest.find(['Z', '+']) {
            Some(at) => rest.split_at(at),
            // Минус ищем только после часов: в самих часах его не бывает,
            // а вот в смещении он и есть половина случаев.
            None => match rest.rfind('-') {
                Some(at) => rest.split_at(at),
                None => (rest, ""),
            },
        };
        let mut clock = clock.split(':');
        let hour: i32 = clock.next()?.parse().ok()?;
        let minute: i32 = clock.next()?.parse().ok()?;
        let second: i32 = clock.next().unwrap_or("0").parse().ok()?;

        let offset = match zone {
            "" | "Z" => 0,
            away => {
                let sign = if away.starts_with('-') { -1 } else { 1 };
                let mut parts = away[1..].split(':');
                let hours: i32 = parts.next()?.parse().ok()?;
                let minutes: i32 = parts.next().unwrap_or("0").parse().ok()?;
                sign * (hours * 3600 + minutes * 60)
            }
        };

        Some(Self {
            days: days_from_civil(year, month, day),
            seconds: hour * 3600 + minute * 60 + second,
            offset,
        })
    }
}

/// Суток от эпохи по календарной дате и обратно. Алгоритм Хиннанта:
/// год начинается с марта, и високосный день тогда оказывается последним
/// днём года, а не посреди него. Своё, потому что крейт времени ради
/// двух функций в проект не едет.
fn days_from_civil(year: i64, month: u32, day: u32) -> i64 {
    let year = year - i64::from(month <= 2);
    let era = year.div_euclid(400);
    let year_of_era = year - era * 400;
    let month = month as i64;
    let day_of_year = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + day as i64 - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let days = days + 719_468;
    let era = days.div_euclid(146_097);
    let day_of_era = days - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let months = (5 * day_of_year + 2) / 153;
    let day = (day_of_year - (153 * months + 2) / 5 + 1) as u32;
    let month = (months + if months < 10 { 3 } else { -9 }) as u32;
    (year + i64::from(month <= 2), month, day)
}

fn parse_line(line: &str) -> Option<Visit> {
    let mut parts = line.split('\t');
    let stamp = Stamp::parse(parts.next()?)?;
    let address = unescape(parts.next()?);
    if address.is_empty() {
        return None;
    }
    Some(Visit {
        stamp,
        address,
        title: unescape(parts.next().unwrap_or_default()),
    })
}

/// Табуляция и перевод строки — разделители файла, поэтому в поле они
/// приезжают записанными, а не собой. Обратная косая уезжает первой,
/// иначе она съела бы то, что мы сами и написали.
fn escape(text: &str) -> String {
    text.replace('\\', r"\\")
        .replace('\t', r"\t")
        .replace('\n', r"\n")
        .replace('\r', "")
}

fn unescape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text.chars();
    while let Some(letter) = rest.next() {
        if letter != '\\' {
            out.push(letter);
            continue;
        }
        match rest.next() {
            Some('t') => out.push('\t'),
            Some('n') => out.push('\n'),
            Some('\\') => out.push('\\'),
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::address::{Repo, RepoHost};

    fn temporary(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "brevier-store-{}-{name}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = fs::remove_dir_all(&dir);
        dir.join("history.tsv")
    }

    fn web(url: &str) -> Address {
        Address::Web(url.to_owned())
    }

    #[test]
    fn the_calendar_agrees_with_itself() {
        for (days, date) in [
            (0, (1970, 1, 1)),
            (20_708, (2026, 9, 12)),
            (-1, (1969, 12, 31)),
            (11_016, (2000, 2, 29)),
        ] {
            assert_eq!(civil_from_days(days), date, "день {days}");
            assert_eq!(
                days_from_civil(date.0, date.1, date.2),
                days,
                "дата {date:?}"
            );
        }
    }

    #[test]
    fn the_epoch_was_a_thursday() {
        assert_eq!(WEEKDAYS[weekday_of(0)], "Thursday");
        // 12 сентября 2026 — суббота, проверено календарём.
        assert_eq!(
            WEEKDAYS[weekday_of(days_from_civil(2026, 9, 12))],
            "Saturday"
        );
    }

    #[test]
    fn a_stamp_survives_the_round_trip() {
        for text in [
            "2026-09-12T14:23:05+03:00",
            "2026-01-02T00:00:00Z",
            "2026-06-30T23:59:59-07:30",
        ] {
            let stamp = Stamp::parse(text).expect(text);
            assert_eq!(stamp.text(), text);
        }
    }

    #[test]
    fn the_clock_is_local_and_the_offset_is_written_down() {
        // Один и тот же миг, записанный в двух поясах, показывает разное
        // время — и это ровно то, чего мы хотим: читатель видит свои часы.
        let moscow = Stamp::of_unix(1_757_680_980, 3 * 3600);
        let utc = Stamp::of_unix(1_757_680_980, 0);
        assert_eq!(moscow.clock(), "15:43");
        assert_eq!(utc.clock(), "12:43");
        assert_eq!(moscow.days, utc.days);
    }

    #[test]
    fn tabs_and_breaks_come_back_as_they_went() {
        let title = "A\ttitle\nwith \\ everything";
        assert_eq!(unescape(&escape(title)), title);
        assert!(!escape(title).contains('\t'));
    }

    #[test]
    fn a_visit_survives_closing_the_program() {
        let path = temporary("visits");
        let mut store = Store::at(&path);
        store.record(
            &web("https://danluu.com/keyboard-latency/"),
            "Keyboard latency",
            0,
        );
        store.record(&web("https://sive.rs/faq"), "FAQ", 0);

        let again = Store::at(&path);
        assert_eq!(again.visits().len(), 2);
        assert_eq!(again.visits()[0].title, "Keyboard latency");
        assert_eq!(again.visits()[1].address, "https://sive.rs/faq");
    }

    #[test]
    fn reloading_the_same_page_is_not_a_second_visit() {
        let path = temporary("reload");
        let mut store = Store::at(&path);
        store.record(&web("https://danluu.com/"), "danluu", 0);
        store.record(&web("https://danluu.com/"), "danluu", 0);
        assert_eq!(store.visits().len(), 1);

        // А вот возвращение после другой страницы — уже визит.
        store.record(&web("https://sive.rs/"), "sivers", 0);
        store.record(&web("https://danluu.com/"), "danluu", 0);
        assert_eq!(store.visits().len(), 3);
    }

    #[test]
    fn internal_pages_stay_out_of_the_history() {
        let path = temporary("internal");
        let mut store = Store::at(&path);
        store.record(
            &Address::Internal(crate::address::Internal::History),
            "History",
            0,
        );
        assert!(store.visits().is_empty());
    }

    #[test]
    fn the_repository_mode_keeps_its_short_form() {
        let path = temporary("repo");
        let mut store = Store::at(&path);
        store.record(
            &Address::Repo(Repo {
                host: RepoHost::GitHub,
                owner: "BurntSushi".to_owned(),
                name: "ripgrep".to_owned(),
                path: None,
                listing: false,
                source: None,
            }),
            "ripgrep",
            0,
        );
        assert_eq!(store.visits()[0].address, "gh:BurntSushi/ripgrep");
        assert_eq!(
            source_of(&store.visits()[0].address),
            "gh:BurntSushi/ripgrep"
        );
    }

    #[test]
    fn what_starts_with_the_host_comes_first() {
        let path = temporary("suggest");
        let mut store = Store::at(&path);
        // Заголовком совпадает, адресом — нет.
        store.record(&web("https://example.test/1"), "Sive and the art", 0);
        // Совпадает серединой адреса.
        store.record(&web("https://mirror.test/sive.rs/faq"), "Mirror", 0);
        // Совпадает с начала хоста — это и есть то, что печатают.
        store.record(&web("https://sive.rs/faq"), "FAQ", 0);

        let hints = store.suggest("sive", HINTS);
        assert_eq!(hints.len(), 3);
        assert_eq!(hints[0].address, "https://sive.rs/faq");
        assert_eq!(hints[1].address, "https://mirror.test/sive.rs/faq");
        assert_eq!(hints[2].address, "https://example.test/1");
        assert!(store.suggest("   ", HINTS).is_empty());
    }

    #[test]
    fn the_scheme_is_not_part_of_the_search() {
        let path = temporary("scheme");
        let mut store = Store::at(&path);
        store.record(&web("https://danluu.com/"), "danluu", 0);
        // `s` есть в «https» у каждого адреса на свете — и это не совпадение,
        // а шум: схему читатель не печатает.
        assert!(store.suggest("s", HINTS).is_empty());
        assert!(store.suggest("http", HINTS).is_empty());
        assert_eq!(store.suggest("danluu", HINTS).len(), 1);
    }

    #[test]
    fn where_you_go_often_outranks_where_you_went_once() {
        let path = temporary("often");
        let mut store = Store::at(&path);
        for _ in 0..2 {
            store.record(&web("https://danluu.com/keyboard-latency/"), "Latency", 0);
            store.record(&web("https://danluu.com/about/"), "About", 0);
        }
        store.record(&web("https://danluu.com/keyboard-latency/"), "Latency", 0);
        store.record(&web("https://danluu.com/deconstruct-files/"), "Files", 0);

        let hints = store.suggest("danluu", HINTS);
        // Три адреса, а не шесть визитов: подсказка про адрес, а не про раз.
        assert_eq!(hints.len(), 3);
        assert_eq!(hints[0].visits, 3);
        assert_eq!(hints[0].address, "https://danluu.com/keyboard-latency/");
        assert_eq!(hints[2].address, "https://danluu.com/deconstruct-files/");
    }

    #[test]
    fn the_page_groups_by_day_and_links_every_row() {
        let path = temporary("page");
        let mut store = Store::at(&path);
        store.record(
            &web("https://danluu.com/keyboard-latency/"),
            "Keyboard latency",
            0,
        );

        // Вчерашний визит подкладываем прямо в журнал: часы мы не двигаем.
        let today = store.visits()[0].stamp;
        store.visits.insert(
            0,
            Visit {
                stamp: Stamp {
                    days: today.days - 1,
                    seconds: 9 * 3600 + 5 * 60,
                    offset: 0,
                },
                address: "gh:BurntSushi/ripgrep".to_owned(),
                title: "ripgrep".to_owned(),
            },
        );

        let page = store.page();
        assert!(page.starts_with("# History\n\n## Today\n"));
        assert!(page.contains("## Today"));
        assert!(page.contains("## Yesterday"));
        assert!(
            page.contains("[Keyboard latency](https://danluu.com/keyboard-latency/) — danluu.com")
        );
        // Адрес репозитория сам называет источник — приписывать его второй раз
        // незачем.
        assert!(page.contains("09:05 [ripgrep](gh:BurntSushi/ripgrep)\n"));
        // Свежее сверху, как на любой странице истории.
        assert!(page.find("## Today").unwrap() < page.find("## Yesterday").unwrap());
    }

    #[test]
    fn an_empty_history_says_so() {
        let page = Store::at(temporary("empty")).page();
        assert!(page.contains("Nothing here yet"));
    }

    #[test]
    fn brackets_in_a_title_do_not_break_the_link() {
        let path = temporary("brackets");
        let mut store = Store::at(&path);
        store.record(&web("https://example.test/"), "Rust 1.90 [stable]", 0);
        assert!(
            store
                .page()
                .contains(r"[Rust 1.90 \[stable\]](https://example.test/)")
        );
    }

    #[test]
    fn a_title_is_always_one_line() {
        let path = temporary("oneline");
        let mut store = Store::at(&path);
        store.record(&web("https://example.test/"), "  Two\n   lines  ", 0);
        assert_eq!(store.visits()[0].title, "Two lines");
        assert_eq!(Store::at(&path).visits()[0].title, "Two lines");
    }

    fn opened(addresses: &[&str], at: usize, place: i32, current: bool) -> Opened {
        Opened {
            addresses: addresses.iter().map(|a| (*a).to_owned()).collect(),
            at,
            place,
            current,
        }
    }

    #[test]
    fn a_session_comes_back_as_it_went() {
        let path = temporary("session").with_file_name("session.tsv");
        let tabs = vec![
            opened(&["https://a.test/", "https://b.test/"], 1, 4200, false),
            opened(&["gh:BurntSushi/ripgrep"], 0, 0, true),
        ];
        remember_at(&path, &tabs);
        assert_eq!(session_at(&path), tabs);

        // Файл объясняет себя сам: читателю его открывать и править.
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.starts_with("# Brevier session:"));
        assert!(text.contains("here\t0\t0\tgh:BurntSushi/ripgrep"));
    }

    #[test]
    fn an_empty_tab_is_not_worth_remembering() {
        let path = temporary("empty-tab").with_file_name("session.tsv");
        remember_at(&path, &[opened(&[], 0, 0, true)]);
        assert!(session_at(&path).is_empty());
    }

    #[test]
    fn a_deep_history_is_cut_from_the_old_end() {
        let path = temporary("deep").with_file_name("session.tsv");
        let addresses: Vec<String> = (0..DEPTH + 5)
            .map(|n| format!("https://example.test/{n}"))
            .collect();
        let tabs = vec![Opened {
            at: addresses.len() - 1,
            addresses,
            place: 0,
            current: true,
        }];
        remember_at(&path, &tabs);

        let back = session_at(&path);
        assert_eq!(back[0].addresses.len(), DEPTH);
        // Резали начало, а стоим по-прежнему на последней странице.
        assert_eq!(back[0].addresses[0], "https://example.test/5");
        assert_eq!(back[0].at, DEPTH - 1);
    }

    #[test]
    fn a_hand_broken_session_loses_only_the_broken_line() {
        let path = temporary("broken-session").with_file_name("session.tsv");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(
            &path,
            "# заголовок\nмусор\nhere\tничего\t0\thttps://a.test/\ntab\t0\t12\thttps://b.test/\n",
        )
        .unwrap();
        let back = session_at(&path);
        assert_eq!(back.len(), 1);
        assert_eq!(back[0].addresses[0], "https://b.test/");
        assert_eq!(back[0].place, 12);
    }

    #[test]
    fn a_long_log_is_trimmed_on_opening() {
        let path = temporary("trim");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let mut text = String::new();
        for number in 0..KEEP + 10 {
            text.push_str(&format!(
                "2026-09-12T10:00:00Z\thttps://example.test/{number}\tpage {number}\n"
            ));
        }
        fs::write(&path, text).unwrap();

        let store = Store::at(&path);
        assert_eq!(store.visits().len(), KEEP);
        // Подрезка доехала до файла, а не осталась в памяти.
        assert_eq!(Store::at(&path).visits().len(), KEEP);
        assert_eq!(store.visits()[0].address, "https://example.test/10");
    }

    #[test]
    fn a_broken_line_does_not_take_the_file_with_it() {
        let path = temporary("broken");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(
            &path,
            "не отметка\tпочти\tстрока\n\n2026-09-12T10:00:00Z\thttps://example.test/\tfine\n",
        )
        .unwrap();
        let store = Store::at(&path);
        assert_eq!(store.visits().len(), 1);
        assert_eq!(store.visits()[0].title, "fine");
    }
}
