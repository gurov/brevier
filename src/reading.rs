//! Прогресс чтения (#19): где читатель остановился и что прочитал.
//!
//! Решено в ROADMAP («Reading progress»): длинную страницу читают в несколько
//! заходов, и открывать её каждый раз сверху незачем. «Назад», «вперёд»
//! и сессия место уже возвращают; это — для страницы, открытой заново:
//! по ссылке, из истории, из закладок, в другой вкладке.
//!
//! **Предлагается, а не делается.** Страница открывается сверху, а строка
//! состояния предлагает «Continue from 43%»: открыть ссылку ещё раз — бывает
//! и намеренное перечитывание. Адрес с `#якорем` идёт к якорю, предложения нет.
//!
//! **Десять минут чтения.** Время идёт, только пока страница на экране
//! и читатель здесь: окно активно, страницу листали или трогали в последние
//! минуты. Это решает интерфейс — он и зовёт [`Progress::tick`] только тогда.
//! Страница, забытая открытой на ночь, восемь часов прочитанной не считается.
//! Заход короче десяти минут не записывается вовсе.
//!
//! **Что прочитано, а не докуда.** Текст делится на куски по [`CHUNK`]
//! знаков; кусок прочитан, когда простоял на экране [`DWELL`] секунд подряд.
//! Пролистанное мимо не считается, и прыжок по оглавлению к последнему
//! разделу статью не «прочитывает».
//!
//! **Считает ядро.** Интерфейс сообщает видимый кусок текста и получает доли
//! прочитанного для строк полки — одна реализация на окно и телефон, как
//! у модели страницы. Смещения — в знаках текста страницы (`Page::text`);
//! телефон переводит свои UTF-16 сам ([`crate::page::chars_of_utf16`]).
//!
//! **Файл свой:** `reading.tsv`, строка на страницу — адрес, отпечаток
//! текста, место, время, прочитанное. Потолок по числу строк, старое уходит
//! первым; «забыть всё» опустошает файл.
//!
//! Цена записана в ROADMAP: смещения верны, пока текст тот же, а страницы
//! меняются. Отпечаток не совпал — место переносится по якорю заголовка
//! (записано от ближайшего якоря выше), а прочитанное сбрасывается.

use std::fs;
use std::path::PathBuf;

use crate::page::Mark;
use crate::store::{self, Stamp};

/// Знаков в куске. Строка текста — около 65 знаков, кусок — две строки:
/// мельче не нужно, крупнее — грубо для полоски у строки полки.
pub const CHUNK: usize = 120;

/// Секунд на экране подряд, после которых кусок прочитан.
pub const DWELL: u32 = 3;

/// Секунд чтения, после которых страницу запоминаем: десять минут.
pub const REMEMBER: u32 = 600;

/// Сколько секунд этого захода нужно, чтобы его место заменило записанное.
/// Открыть страницу и полминуты смотреть на её начало, не приняв «Continue»,
/// — не значит перечитывать её сначала: прошлое место терять нельзя.
const PLACE_AFTER: u32 = 120;

/// Как часто переписывать запись, когда страница уже запомнена.
const SAVE_EVERY: u32 = 30;

/// Строк в `reading.tsv`. Тысяча долгих чтений — годы.
const KEEP: usize = 1000;

/// Ниже и выше этого «продолжить» не предлагаем: в самом начале читать
/// и так сверху, а дочитанное до конца продолжать незачем.
const OFFER_FROM: u32 = 3;
const OFFER_TO: u32 = 97;

/// Сколько экранов должно быть в разделе, чтобы делить его своими вехами.
const LONG_SECTION_SCREENS: usize = 3;

/// Прогресс одной открытой страницы.
#[derive(Debug, Clone)]
pub struct Progress {
    /// Адрес без `#якоря`: страница та же.
    pub address: String,
    pub fingerprint: u64,
    total: usize,
    seconds: u32,
    place: usize,
    /// Сколько секунд подряд кусок на экране; ноль — не на экране.
    dwell: Vec<u32>,
    read: Vec<bool>,
    /// Секунд чтения на момент последней записи — чтобы писать не на каждый тик.
    saved_at: u32,
    /// Место из записи и сколько секунд читают в этом заходе: пока заход
    /// короче [`PLACE_AFTER`], в запись идёт прежнее место.
    kept_place: Option<usize>,
    visit: u32,
    anchors: Vec<(String, usize)>,
}

/// Запись в `reading.tsv`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Saved {
    pub stamp: Stamp,
    pub address: String,
    pub fingerprint: u64,
    pub place: usize,
    pub seconds: u32,
    /// Прочитанное — промежутками `[from, to)` в знаках.
    pub read: Vec<(usize, usize)>,
    /// Место от ближайшего якоря выше: имя якоря и сколько знаков от него.
    pub anchor: Option<(String, usize)>,
}

impl Progress {
    /// Начать счёт по странице. `anchors` — якоря заголовков (`Page::anchors`).
    pub fn new(address: &str, text: &str, anchors: &[(String, usize)]) -> Self {
        let total = text.chars().count();
        let chunks = total.div_ceil(CHUNK).max(1);
        Self {
            address: without_fragment(address).to_owned(),
            fingerprint: fingerprint(text),
            total,
            seconds: 0,
            place: 0,
            dwell: vec![0; chunks],
            read: vec![false; chunks],
            saved_at: 0,
            kept_place: None,
            visit: 0,
            anchors: anchors.to_vec(),
        }
    }

    /// Поднять записанное: прочитанное и время — если текст тот же,
    /// иначе только место, перенесённое по якорю.
    pub fn restore(&mut self, saved: &Saved) {
        if saved.fingerprint == self.fingerprint {
            self.seconds = saved.seconds;
            self.saved_at = saved.seconds;
            self.place = saved.place.min(self.total);
            for &(from, to) in &saved.read {
                self.mark(from, to);
            }
        } else if let Some(place) = self.carried(saved) {
            self.place = place;
        }
        self.kept_place = Some(self.place);
    }

    /// Где читатель остановился, если запись есть: смещение, — для «Continue».
    /// Без записи или когда текст сменился и якоря нет — `None`.
    pub fn resume_at(&self, saved: &Saved) -> Option<usize> {
        if saved.fingerprint == self.fingerprint {
            Some(saved.place.min(self.total))
        } else {
            self.carried(saved)
        }
    }

    fn carried(&self, saved: &Saved) -> Option<usize> {
        let (name, delta) = saved.anchor.as_ref()?;
        let (_, at) = self.anchors.iter().find(|(known, _)| known == name)?;
        Some((at + delta).min(self.total))
    }

    /// Секунда чтения: видимый кусок текста `[from, to)` и сколько секунд
    /// прошло. Зовёт интерфейс, только пока читатель здесь. Ответ — пора ли
    /// записать (`Readings::put`): страница только что стала запомненной
    /// или с прошлой записи прошло [`SAVE_EVERY`] секунд.
    pub fn tick(&mut self, from: usize, to: usize, seconds: u32) -> bool {
        let (from, to) = (from.min(self.total), to.min(self.total));
        self.place = from;
        self.seconds = self.seconds.saturating_add(seconds);
        self.visit = self.visit.saturating_add(seconds);
        let first = from / CHUNK;
        // Кусок на краю виден частично — его не считаем: строка, торчащая
        // из-под нижнего края, ещё не прочитана.
        let last = to.saturating_sub(1) / CHUNK;
        for (index, dwell) in self.dwell.iter_mut().enumerate() {
            let shown = to > from && index >= first && index <= last;
            *dwell = if shown {
                dwell.saturating_add(seconds)
            } else {
                0
            };
            if *dwell >= DWELL {
                self.read[index] = true;
            }
        }
        if !self.remembered() {
            return false;
        }
        if self.seconds - self.saved_at >= SAVE_EVERY || self.saved_at < REMEMBER {
            self.saved_at = self.seconds;
            return true;
        }
        false
    }

    /// Отметить место без чтения — после прыжка, перед уходом со страницы.
    pub fn set_place(&mut self, place: usize) {
        self.place = place.min(self.total);
    }

    pub fn place(&self) -> usize {
        self.place
    }

    /// Длина текста в знаках.
    pub fn total(&self) -> usize {
        self.total
    }

    /// Забыть прочитанное и время — после «забыть всё»; место остаётся.
    pub fn reset(&mut self) {
        self.seconds = 0;
        self.saved_at = 0;
        self.dwell.iter_mut().for_each(|dwell| *dwell = 0);
        self.read.iter_mut().for_each(|read| *read = false);
    }

    /// Прочитано ли столько, чтобы запомнить.
    pub fn remembered(&self) -> bool {
        self.seconds >= REMEMBER
    }

    /// Докуда дошли, в процентах длины текста.
    pub fn percent(&self) -> u32 {
        percent(self.place, self.total)
    }

    /// Доли прочитанного по разделам: раздел — от своей строки полки до
    /// следующей, последний — до конца текста. `starts` — смещения строк.
    pub fn shares(&self, starts: &[usize]) -> Vec<f32> {
        (0..starts.len())
            .map(|index| {
                let from = starts[index].min(self.total);
                let to = starts
                    .get(index + 1)
                    .copied()
                    .unwrap_or(self.total)
                    .min(self.total);
                self.share(from, to)
            })
            .collect()
    }

    /// Доля прочитанного в промежутке `[from, to)`.
    pub fn share(&self, from: usize, to: usize) -> f32 {
        if to <= from {
            return 0.0;
        }
        let read: usize = (from..to)
            .step_by(1)
            .filter(|at| self.read.get(at / CHUNK).copied().unwrap_or(false))
            .count();
        read as f32 / (to - from) as f32
    }

    /// Запись для `reading.tsv`.
    pub fn saved(&self, stamp: Stamp) -> Saved {
        Saved {
            stamp,
            address: self.address.clone(),
            fingerprint: self.fingerprint,
            place: self.recorded_place(),
            seconds: self.seconds,
            read: stretches(&self.read, self.total),
            anchor: self
                .anchors
                .iter()
                .filter(|(_, at)| *at <= self.recorded_place())
                .max_by_key(|(_, at)| *at)
                .map(|(name, at)| (name.clone(), self.recorded_place() - at)),
        }
    }

    /// Какое место писать: этого захода — если в нём уже читали, иначе
    /// прежнее.
    fn recorded_place(&self) -> usize {
        match self.kept_place {
            Some(kept) if self.visit < PLACE_AFTER => kept,
            _ => self.place,
        }
    }

    fn mark(&mut self, from: usize, to: usize) {
        let to = to.min(self.total);
        if to <= from {
            return;
        }
        for index in from / CHUNK..=(to - 1) / CHUNK {
            if let Some(read) = self.read.get_mut(index) {
                *read = true;
            }
        }
    }
}

/// Предложить ли «Continue from N%» и откуда: страница запомнена, место
/// не в самом начале и не в самом конце. Ответ — смещение и проценты.
pub fn offer(progress: &Progress, saved: &Saved) -> Option<(usize, u32)> {
    let at = progress.resume_at(saved)?;
    let share = percent(at, progress.total);
    (OFFER_FROM..=OFFER_TO)
        .contains(&share)
        .then_some((at, share))
}

/// Вехи внутри длинного раздела — на уровень ниже, для того раздела, где
/// читатель сейчас. Раздел длиннее [`LONG_SECTION_SCREENS`] экранов делится
/// началами абзацев (`leads`) примерно по экрану; подпись — доля документа
/// и начало абзаца: «40% · Beginning of the paragraph…».
pub fn inner_waypoints(
    leads: &[Mark],
    from: usize,
    to: usize,
    screen: usize,
    total: usize,
    level: u8,
) -> Vec<Mark> {
    if screen == 0 || to <= from || to - from < screen * LONG_SECTION_SCREENS {
        return Vec::new();
    }
    let mut out: Vec<Mark> = Vec::new();
    let mut next = from + screen;
    for lead in leads {
        if lead.offset < next || lead.offset >= to {
            continue;
        }
        // Последний кусок короче полэкрана — веха перед самым концом раздела
        // ничего не даёт.
        if to - lead.offset < screen / 2 {
            break;
        }
        out.push(Mark {
            level: level + 1,
            title: waypoint_label(lead, total),
            offset: lead.offset,
            heading: false,
        });
        next = lead.offset + screen;
    }
    out
}

/// Подпись вехи с её местом: «40% · Начало абзаца…».
pub fn waypoint_label(mark: &Mark, total: usize) -> String {
    format!("{}% · {}", percent(mark.offset, total), mark.title)
}

fn percent(at: usize, total: usize) -> u32 {
    if total == 0 {
        return 0;
    }
    ((at.min(total) as f64 / total as f64) * 100.0).round() as u32
}

fn without_fragment(address: &str) -> &str {
    address.split('#').next().unwrap_or(address)
}

/// Отпечаток текста: FNV-1a. Совпал — смещения прежние.
pub fn fingerprint(text: &str) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in text.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    hash
}

/// Прочитанные куски — промежутками в знаках.
fn stretches(read: &[bool], total: usize) -> Vec<(usize, usize)> {
    let mut out: Vec<(usize, usize)> = Vec::new();
    for (index, &done) in read.iter().enumerate() {
        if !done {
            continue;
        }
        let (from, to) = (index * CHUNK, ((index + 1) * CHUNK).min(total));
        match out.last_mut() {
            Some(last) if last.1 == from => last.1 = to,
            _ => out.push((from, to)),
        }
    }
    out
}

/// `reading.tsv`: запомненные страницы.
#[derive(Debug, Default)]
pub struct Readings {
    path: Option<PathBuf>,
    entries: Vec<Saved>,
}

impl Readings {
    pub fn open() -> Self {
        match store::data_dir() {
            Some(dir) => Self::at(dir.join("reading.tsv")),
            None => Self::default(),
        }
    }

    pub fn at(path: impl Into<PathBuf>) -> Self {
        let path = path.into();
        let text = fs::read_to_string(&path).unwrap_or_default();
        Self {
            entries: text.lines().filter_map(parse_line).collect(),
            path: Some(path),
        }
    }

    /// Запись о странице, если она запомнена. Адрес — без `#якоря`.
    pub fn find(&self, address: &str) -> Option<&Saved> {
        let address = without_fragment(address);
        self.entries
            .iter()
            .rev()
            .find(|saved| saved.address == address)
    }

    /// Положить запись: прежняя о той же странице уходит, новая — в конец;
    /// сверх потолка уходит самое старое.
    pub fn put(&mut self, saved: Saved) {
        self.entries.retain(|known| known.address != saved.address);
        self.entries.push(saved);
        if self.entries.len() > KEEP {
            let extra = self.entries.len() - KEEP;
            self.entries.drain(..extra);
        }
        self.write();
    }

    pub fn forget(&mut self) {
        self.entries.clear();
        self.write();
    }

    fn write(&self) {
        let Some(path) = &self.path else { return };
        if let Some(dir) = path.parent()
            && fs::create_dir_all(dir).is_err()
        {
            return;
        }
        let text: String = self.entries.iter().map(line_of).collect();
        let _ = fs::write(path, text);
    }
}

fn line_of(saved: &Saved) -> String {
    let read: Vec<String> = saved
        .read
        .iter()
        .map(|(from, to)| format!("{from}-{to}"))
        .collect();
    let anchor = match &saved.anchor {
        Some((name, delta)) => format!("{}+{delta}", name.replace(['\t', '\n'], " ")),
        None => String::new(),
    };
    format!(
        "{}\t{}\t{:016x}\t{}\t{}\t{}\t{}\n",
        saved.stamp.text(),
        saved.address.replace(['\t', '\n'], " "),
        saved.fingerprint,
        saved.place,
        saved.seconds,
        read.join(","),
        anchor,
    )
}

fn parse_line(line: &str) -> Option<Saved> {
    let mut parts = line.split('\t');
    let stamp = Stamp::parse(parts.next()?)?;
    let address = parts.next()?.to_owned();
    let fingerprint = u64::from_str_radix(parts.next()?, 16).ok()?;
    let place = parts.next()?.parse().ok()?;
    let seconds = parts.next()?.parse().ok()?;
    let read = parts
        .next()
        .unwrap_or_default()
        .split(',')
        .filter_map(|pair| {
            let (from, to) = pair.split_once('-')?;
            Some((from.parse().ok()?, to.parse().ok()?))
        })
        .collect();
    let anchor = parts.next().and_then(|text| {
        let (name, delta) = text.rsplit_once('+')?;
        Some((name.to_owned(), delta.parse().ok()?))
    });
    (!address.is_empty()).then_some(Saved {
        stamp,
        address,
        fingerprint,
        place,
        seconds,
        read,
        anchor,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(chars: usize) -> String {
        "a".repeat(chars)
    }

    fn stamp() -> Stamp {
        Stamp::parse("2026-10-09T20:00:00+02:00").unwrap()
    }

    /// Кусок прочитан, только простояв на экране три секунды подряд:
    /// пролистанное не считается.
    #[test]
    fn a_stretch_is_read_only_after_staying_on_screen() {
        let mut progress = Progress::new("https://e.org/a", &text(CHUNK * 10), &[]);
        // Пролистали: каждый кусок на экране по секунде.
        for step in 0..10 {
            progress.tick(step * CHUNK, (step + 1) * CHUNK, 1);
        }
        assert_eq!(progress.share(0, CHUNK * 10), 0.0);
        // Задержались на первых трёх кусках.
        for _ in 0..DWELL {
            progress.tick(0, CHUNK * 3, 1);
        }
        assert_eq!(progress.share(0, CHUNK * 3), 1.0);
        assert_eq!(progress.share(CHUNK * 3, CHUNK * 10), 0.0);
        assert_eq!(progress.place(), 0);
    }

    /// Прыжок к последнему разделу читает только его, а не всё до него.
    #[test]
    fn a_jump_reads_only_where_it_lands() {
        let mut progress = Progress::new("https://e.org/a", &text(CHUNK * 20), &[]);
        for _ in 0..5 {
            progress.tick(CHUNK * 18, CHUNK * 20, 1);
        }
        let shares = progress.shares(&[0, CHUNK * 10, CHUNK * 18]);
        assert_eq!(shares, vec![0.0, 0.0, 1.0]);
        assert_eq!(progress.percent(), 90);
    }

    /// Записать пора только после десяти минут, а потом — не на каждый тик.
    #[test]
    fn a_page_is_remembered_after_ten_minutes_of_reading() {
        let mut progress = Progress::new("https://e.org/a#part", &text(CHUNK * 4), &[]);
        assert_eq!(progress.address, "https://e.org/a");
        let mut saves = 0;
        for _ in 0..REMEMBER - 1 {
            saves += usize::from(progress.tick(0, CHUNK, 1));
        }
        assert_eq!(saves, 0);
        assert!(!progress.remembered());
        assert!(progress.tick(0, CHUNK, 1));
        assert!(progress.remembered());
        // Дальше — раз в полминуты.
        let later: usize = (0..SAVE_EVERY)
            .map(|_| usize::from(progress.tick(0, CHUNK, 1)))
            .sum();
        assert_eq!(later, 1);
    }

    /// Запись переживает файл; тот же текст — всё на месте; другой текст —
    /// место по якорю, прочитанное сброшено; без якоря — ничего.
    #[test]
    fn a_reading_survives_the_file_and_a_changed_page() {
        let dir = std::env::temp_dir().join(format!("brevier-reading-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let path = dir.join("reading.tsv");
        let anchors = vec![("intro".to_owned(), 0), ("part-two".to_owned(), CHUNK * 10)];
        let mut progress = Progress::new("https://e.org/a", &text(CHUNK * 20), &anchors);
        for _ in 0..DWELL {
            progress.tick(CHUNK * 12, CHUNK * 14, 1);
        }
        let saved = progress.saved(stamp());
        assert_eq!(saved.anchor, Some(("part-two".to_owned(), CHUNK * 2)));
        let mut readings = Readings::at(&path);
        readings.put(saved.clone());
        let again = Readings::at(&path);
        assert_eq!(again.find("https://e.org/a#x"), Some(&saved));

        // Тот же текст: место и прочитанное.
        let mut same = Progress::new("https://e.org/a", &text(CHUNK * 20), &anchors);
        same.restore(&saved);
        assert_eq!(same.place(), CHUNK * 12);
        assert_eq!(same.share(CHUNK * 12, CHUNK * 14), 1.0);
        assert_eq!(offer(&same, &saved), Some((CHUNK * 12, 60)));

        // Текст сменился, якорь уехал: место — от него, прочитанное — нет.
        let moved = vec![("intro".to_owned(), 0), ("part-two".to_owned(), CHUNK * 11)];
        let mut changed = Progress::new("https://e.org/a", &text(CHUNK * 21), &moved);
        changed.restore(&saved);
        assert_eq!(changed.place(), CHUNK * 13);
        assert_eq!(changed.share(0, CHUNK * 21), 0.0);

        // Якоря нет — и предлагать нечего.
        let bare = Progress::new("https://e.org/a", &text(CHUNK * 21), &[]);
        assert_eq!(offer(&bare, &saved), None);

        readings.forget();
        assert!(Readings::at(&path).find("https://e.org/a").is_none());
    }

    /// Открыть страницу и посмотреть на её начало — не значит перечитывать:
    /// прошлое место остаётся в записи, пока заход не наберёт двух минут.
    #[test]
    fn a_glance_at_the_top_keeps_the_old_place() {
        let mut progress = Progress::new("https://e.org/a", &text(CHUNK * 20), &[]);
        let saved = Saved {
            stamp: stamp(),
            address: "https://e.org/a".to_owned(),
            fingerprint: progress.fingerprint,
            place: CHUNK * 12,
            seconds: REMEMBER + 100,
            read: vec![(0, CHUNK * 12)],
            anchor: None,
        };
        progress.restore(&saved);
        for _ in 0..PLACE_AFTER - 1 {
            progress.tick(0, CHUNK * 2, 1);
        }
        assert_eq!(progress.saved(stamp()).place, CHUNK * 12);
        progress.tick(0, CHUNK * 2, 1);
        assert_eq!(progress.saved(stamp()).place, 0);
    }

    /// В самом начале и в самом конце «продолжить» не предлагаем.
    #[test]
    fn continue_is_offered_only_in_the_middle() {
        let progress = Progress::new("https://e.org/a", &text(1000), &[]);
        let at = |place: usize| Saved {
            stamp: stamp(),
            address: "https://e.org/a".to_owned(),
            fingerprint: progress.fingerprint,
            place,
            seconds: REMEMBER,
            read: Vec::new(),
            anchor: None,
        };
        assert_eq!(offer(&progress, &at(10)), None);
        assert_eq!(offer(&progress, &at(430)), Some((430, 43)));
        assert_eq!(offer(&progress, &at(990)), None);
    }

    /// Длинный раздел делится вехами по экрану, на уровень ниже, с долей.
    #[test]
    fn a_long_section_gets_waypoints_of_its_own() {
        let leads: Vec<Mark> = (0..40)
            .map(|n| Mark {
                level: 0,
                title: format!("Paragraph {n}…"),
                offset: n * 500,
                heading: false,
            })
            .collect();
        // Раздел 2000..12000, экран — 2000 знаков, документ — 20000.
        let inner = inner_waypoints(&leads, 2000, 12000, 2000, 20000, 2);
        let offsets: Vec<usize> = inner.iter().map(|mark| mark.offset).collect();
        assert_eq!(offsets, vec![4000, 6000, 8000, 10000]);
        assert_eq!(inner[0].title, "20% · Paragraph 8…");
        assert_eq!(inner[0].level, 3);
        // Короткий раздел не делится.
        assert!(inner_waypoints(&leads, 2000, 6000, 2000, 20000, 2).is_empty());
    }

    #[test]
    fn the_file_skips_broken_lines() {
        assert!(parse_line("garbage").is_none());
        assert!(parse_line("2026-10-09T20:00:00Z\t\t00\t0\t0\t\t").is_none());
        let line = line_of(&Saved {
            stamp: stamp(),
            address: "gh:o/n".to_owned(),
            fingerprint: 7,
            place: 5,
            seconds: 601,
            read: vec![(0, 120), (240, 360)],
            anchor: None,
        });
        let saved = parse_line(line.trim_end()).unwrap();
        assert_eq!(saved.read, vec![(0, 120), (240, 360)]);
        assert_eq!(saved.anchor, None);
    }
}
