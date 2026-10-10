//! Архив прочитанного (#8): копия каждой прочитанной страницы, навсегда.
//!
//! Решено в ROADMAP («The archive»): включён по умолчанию и сжат. Недельные
//! копии (`cache`) — ускорение, и живут неделю в кэше, который выбрасывают
//! не глядя; архив — данные читателя, и лежит там же, где история
//! и закладки: `$XDG_DATA_HOME/brevier/archive`. Кэш хранит документ
//! для мгновенного показа, архив — текст, который останется, когда страницы
//! в сети уже не будет.
//!
//! **Файл на страницу:** `archive/<host>/<дата>-<слаг>.md.lz4`. Внутри —
//! markdown с шапкой YAML (`title`, `source`, `read`), сжатый стандартным
//! кадром LZ4: `lz4 -d` открывает копию без Brevier, а Brevier читает
//! её же как любой `.md` — шапку он прячет, заголовок берёт из неё.
//! Между читателем и его текстом — только распаковщик, который у него уже
//! есть. Базы нет: жизнь чтения — тысячи страниц, не миллионы, и поиск
//! по ним — проход по файлам со скоростью памяти.
//!
//! **Что кладём** — то же, что и в недельный кэш: статью из сети или из
//! репозитория. Ленту — нет: её открывают ради нового, а не ради текста.
//! Свои страницы и локальные файлы — нет: первые и так на диске в своём
//! виде, вторые — сами себе копия.
//!
//! **Сколько живёт.** Строка истории указывает на свою копию; копия живёт,
//! пока на неё указывает история (журнал подрезается своим `KEEP`), а у
//! закладки последняя копия остаётся навсегда. Уборка — на запуске, в фоне.
//! «Забыть всё» опустошает архив вместе с журналом; переключатель
//! в настройках перестаёт класть новое.
//!
//! **Пишет только интерфейс**, как и историю: cli — инструмент конвейера.
//! Читать архив cli может: `brevier brevier:archive/<путь>` печатает копию
//! как есть.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use crate::address::Address;
use crate::markdown::Kind;
use crate::store::{self, Stamp};
use crate::{Document, cache};

/// Хвост имени копии.
pub const EXTENSION: &str = ".md.lz4";

/// Длина слага в знаках. Имя файла узнают глазами в списке каталога,
/// а не читают: полстроки заголовка хватает.
const SLUG_CHARS: usize = 60;

/// Сколько копий одной даты и одного слага, прежде чем сдаться. Разные
/// страницы под одним заголовком в один день — «News», «Live» — бывают;
/// десять — уже нет.
const SAME_NAME: usize = 10;

/// Сколько байт распаковываем, чтобы прочитать шапку. Шапка — три строки.
const HEAD_BYTES: usize = 4096;

/// Свежую копию уборка не трогает: её могли записать, пока уборка читала
/// журнал, и ссылка на неё в журнале ещё не появилась.
const GRACE: Duration = Duration::from_secs(24 * 3600);

/// Сколько найденного показывать. Дальше уточняют запрос.
const RESULTS: usize = 200;

/// Знаков текста вокруг найденного слова.
const SNIPPET_CHARS: usize = 90;

/// Архив: папка с копиями. `None` — положить некуда; тогда архива нет,
/// а чтение идёт как шло.
#[derive(Debug, Clone)]
pub struct Archive {
    dir: Option<PathBuf>,
}

/// Копия, как она лежит: шапка и текст.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Copy {
    /// Путь внутри архива: `<host>/<дата>-<слаг>.md.lz4`.
    pub path: String,
    pub title: String,
    /// Адрес страницы в том виде, в каком его показывает строка Brevier.
    pub source: String,
    pub read: Stamp,
    /// Файл целиком, с шапкой: ровно то, что даст `lz4 -d`.
    pub markdown: String,
}

impl Archive {
    /// Архив на своём месте — рядом с историей (`store::data_dir`).
    pub fn open() -> Self {
        Self {
            dir: store::data_dir().map(|dir| dir.join("archive")),
        }
    }

    /// Архив в названной папке — для тестов.
    pub fn at(dir: impl Into<PathBuf>) -> Self {
        Self {
            dir: Some(dir.into()),
        }
    }

    /// Положить копию прочитанной страницы. Ответ — путь копии внутри
    /// архива: его записывает строка истории. `None` — класть нечего
    /// (лента, своя страница) или некуда.
    ///
    /// Та же страница, прочитанная в тот же день ещё раз, переписывает свою
    /// копию: имя у неё то же. В другой день — новая копия рядом: страница
    /// могла измениться, а прочитано было то, что было.
    pub fn keep(&self, document: &Document, read: &Stamp) -> Option<String> {
        let dir = self.dir.as_ref()?;
        if !worth_keeping(document) {
            return None;
        }
        let source = document.address.display();
        let host = host_of(&document.address)?;
        let date = date_of(read);
        let slug = slug_of(&document.title, &source);
        let text = file_text(&document.title, &source, read, &document.markdown);
        let bytes = compress(text.as_bytes())?;

        for number in 1..=SAME_NAME {
            let name = if number == 1 {
                format!("{date}-{slug}{EXTENSION}")
            } else {
                format!("{date}-{slug}-{number}{EXTENSION}")
            };
            let path = dir.join(&host).join(&name);
            // Имя занято другой страницей — берём следующее.
            if let Some(head) = read_head(&path)
                && head.source != source
            {
                continue;
            }
            cache::write_atomically(&path, &bytes);
            return path.exists().then(|| format!("{host}/{name}"));
        }
        None
    }

    /// Копия по пути внутри архива. Путь проверяется: из адресной строки
    /// приходит что угодно, а выйти за пределы архива он не должен.
    pub fn read(&self, path: &str) -> Option<Copy> {
        let file = self.dir.as_ref()?.join(checked(path)?);
        let bytes = fs::read(file).ok()?;
        let text = decompress(&bytes)?;
        let head = parse_head(&text)?;
        Some(Copy {
            path: path.to_owned(),
            title: head.title,
            source: head.source,
            read: head.read,
            markdown: text,
        })
    }

    /// Последняя копия этой страницы, шапкой без текста: её предлагает
    /// страница отказа (#9). Ищем только в папке хоста — все копии страницы
    /// лежат там, и весь архив ради одной страницы не читаем. Решётка
    /// адреса не в счёт: в копию страница ложится без неё.
    pub fn latest(&self, address: &Address) -> Option<Copy> {
        let dir = self.dir.as_ref()?;
        let host = host_of(address)?;
        let shown = address.display();
        let source = shown.split('#').next().unwrap_or_default();
        fs::read_dir(dir.join(&host))
            .ok()?
            .flatten()
            .filter_map(|file| {
                let name = file.file_name().to_str()?.to_owned();
                if !name.ends_with(EXTENSION) {
                    return None;
                }
                let head = read_head(&file.path())?;
                (head.source == source).then(|| Copy {
                    path: format!("{host}/{name}"),
                    title: head.title,
                    source: head.source,
                    read: head.read,
                    markdown: String::new(),
                })
            })
            .max_by_key(|copy| moment(&copy.read))
    }

    /// Все копии, свежие сверху: шапки без текста.
    pub fn list(&self) -> Vec<Copy> {
        let mut copies: Vec<Copy> = self
            .files()
            .into_iter()
            .filter_map(|(path, file)| {
                let head = read_head(&file)?;
                Some(Copy {
                    path,
                    title: head.title,
                    source: head.source,
                    read: head.read,
                    markdown: String::new(),
                })
            })
            .collect();
        copies.sort_by_key(|copy| std::cmp::Reverse(moment(&copy.read)));
        copies
    }

    /// Страница `brevier:archive`: все копии по дням, как история.
    pub fn page(&self) -> String {
        let mut out = String::from("# Archive\n\n");
        let copies = self.list();
        if copies.is_empty() {
            out.push_str(
                "Nothing here yet. Every page you read is kept here, as a compressed \
                 Markdown file, so you can read it again even when the site is gone.\n",
            );
            out.push_str(&self.footer());
            return out;
        }
        let today = Stamp::now(copies[0].read.offset).days;
        let mut day = None;
        for copy in &copies {
            if day != Some(copy.read.days) {
                if day.is_some() {
                    out.push('\n');
                }
                day = Some(copy.read.days);
                out.push_str(&format!(
                    "## {}\n\n",
                    store::name_of_day(copy.read.days, today)
                ));
            }
            out.push_str(&format!(
                "- {} {} — {}\n",
                copy.read.clock(),
                store::link(&shown_title(copy), &copy_address(&copy.path)),
                store::source_of(&copy.source),
            ));
        }
        out.push_str(&self.footer());
        out
    }

    /// Поиск по всему архиву: страницы, где есть каждое слово запроса,
    /// свежие сверху, с куском текста вокруг первого найденного.
    pub fn search(&self, query: &str) -> Vec<(Copy, String)> {
        let words: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
        if words.is_empty() {
            return Vec::new();
        }
        let mut found = Vec::new();
        for (path, file) in self.files() {
            let Some(text) = fs::read(&file).ok().and_then(|bytes| decompress(&bytes)) else {
                continue;
            };
            let Some(head) = parse_head(&text) else {
                continue;
            };
            let body = crate::markdown::body(&text);
            let lower = body.to_lowercase();
            if !words.iter().all(|word| lower.contains(word.as_str())) {
                continue;
            }
            let snippet = snippet(body, &words);
            found.push((
                Copy {
                    path,
                    title: head.title,
                    source: head.source,
                    read: head.read,
                    markdown: String::new(),
                },
                snippet,
            ));
        }
        found.sort_by_key(|(copy, _)| std::cmp::Reverse(moment(&copy.read)));
        found.truncate(RESULTS);
        found
    }

    /// Страница с найденным: `brevier:archive?q=…`.
    pub fn search_page(&self, query: &str) -> String {
        let query = query.trim();
        let mut out = format!("# Archive: {}\n\n", escape_text(query));
        let found = self.search(query);
        match found.len() {
            0 => out.push_str("No page in your archive has all of these words.\n"),
            1 => out.push_str("One page.\n"),
            RESULTS => out.push_str(&format!("The newest {RESULTS} pages.\n")),
            count => out.push_str(&format!("{count} pages.\n")),
        }
        for (copy, snippet) in &found {
            out.push_str(&format!(
                "\n## {}\n\n*{} · {}*\n",
                store::link(&shown_title(copy), &copy_address(&copy.path)),
                store::source_of(&copy.source),
                long_date(&copy.read),
            ));
            if !snippet.is_empty() {
                out.push_str(&format!("\n{snippet}\n"));
            }
        }
        out
    }

    /// Уборка: копия остаётся, если на неё указывает журнал (`kept`), или если
    /// это последняя копия страницы из закладок (`marked` — их адреса). Свежие
    /// не трогаем вовсе (`GRACE`). Пустые папки хостов убираем следом.
    pub fn prune(&self, kept: &HashSet<String>, marked: &HashSet<String>) {
        self.prune_at(kept, marked, SystemTime::now());
    }

    fn prune_at(&self, kept: &HashSet<String>, marked: &HashSet<String>, now: SystemTime) {
        let Some(dir) = &self.dir else { return };
        let mut newest: HashMap<String, (i64, String)> = HashMap::new();
        let mut heads = Vec::new();
        for (path, file) in self.files() {
            let Some(head) = read_head(&file) else {
                continue;
            };
            if marked.contains(&head.source) {
                let when = moment(&head.read);
                let entry = newest
                    .entry(head.source.clone())
                    .or_insert((when, path.clone()));
                if when > entry.0 {
                    *entry = (when, path.clone());
                }
            }
            heads.push((path, file));
        }
        let marked_copies: HashSet<String> = newest.into_values().map(|(_, path)| path).collect();
        for (path, file) in heads {
            if kept.contains(&path) || marked_copies.contains(&path) {
                continue;
            }
            let young = fs::metadata(&file)
                .and_then(|meta| meta.modified())
                .ok()
                .and_then(|modified| now.duration_since(modified).ok())
                .is_none_or(|age| age < GRACE);
            if !young {
                let _ = fs::remove_file(&file);
            }
        }
        if let Ok(hosts) = fs::read_dir(dir) {
            for host in hosts.flatten() {
                // Удаляется только пустая: `remove_dir` с содержимым не справится.
                let _ = fs::remove_dir(host.path());
            }
        }
    }

    /// Забыть всё: копии — такой же след прочитанного, как журнал.
    pub fn forget(&self) {
        if let Some(dir) = &self.dir {
            let _ = fs::remove_dir_all(dir);
        }
    }

    /// Где лежит архив и как его читать без Brevier — подвал обеих страниц.
    fn footer(&self) -> String {
        let place = match &self.dir {
            Some(dir) => store::where_it_is(dir),
            None => "nowhere: there is no home folder".to_owned(),
        };
        format!(
            "\n---\n\nThese are plain files in {place}, one per page; `lz4 -d` opens any of \
             them without Brevier. To search them, type `brevier:archive?q=` and your words \
             in the address bar (Ctrl+Shift+F in the window).\n"
        )
    }

    /// Файлы копий: путь внутри архива и полный путь.
    fn files(&self) -> Vec<(String, PathBuf)> {
        let Some(dir) = &self.dir else {
            return Vec::new();
        };
        let mut out = Vec::new();
        let Ok(hosts) = fs::read_dir(dir) else {
            return out;
        };
        for host in hosts.flatten() {
            let Some(host_name) = host.file_name().to_str().map(str::to_owned) else {
                continue;
            };
            let Ok(files) = fs::read_dir(host.path()) else {
                continue;
            };
            for file in files.flatten() {
                let Some(name) = file.file_name().to_str().map(str::to_owned) else {
                    continue;
                };
                if name.ends_with(EXTENSION) {
                    out.push((format!("{host_name}/{name}"), file.path()));
                }
            }
        }
        out
    }
}

/// Адрес копии в Brevier.
pub fn copy_address(path: &str) -> String {
    format!("brevier:archive/{path}")
}

/// Годится ли документ в архив: статья из сети или из репозитория.
fn worth_keeping(document: &Document) -> bool {
    document.kind == Kind::Article && matches!(document.address, Address::Web(_) | Address::Repo(_))
}

/// Папка копии — хост страницы, без `www.`: так их и ищут глазами.
fn host_of(address: &Address) -> Option<String> {
    let external = address.external();
    let host = url::Url::parse(&external).ok()?.host_str()?.to_lowercase();
    let host = host.trim_start_matches("www.").to_owned();
    (!host.is_empty() && !host.contains(['/', '\\']) && host != "." && host != "..").then_some(host)
}

/// Дата прочтения по местным часам: `2026-10-09`.
fn date_of(read: &Stamp) -> String {
    read.text().split('T').next().unwrap_or_default().to_owned()
}

/// Слаг из заголовка: буквы и цифры любого письма, остальное — дефис.
/// Кириллица остаётся кириллицей: имя файла читает человек, а не URL.
/// Пустой заголовок — последний кусок адреса.
fn slug_of(title: &str, source: &str) -> String {
    let make = |text: &str| -> String {
        let mut slug = String::new();
        for ch in text.chars() {
            if ch.is_alphanumeric() {
                slug.extend(ch.to_lowercase());
            } else if !slug.ends_with('-') && !slug.is_empty() {
                slug.push('-');
            }
            if slug.chars().count() >= SLUG_CHARS {
                break;
            }
        }
        slug.trim_end_matches('-').to_owned()
    };
    let slug = make(title);
    if !slug.is_empty() {
        return slug;
    }
    let last = source
        .split(['?', '#'])
        .next()
        .unwrap_or(source)
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or_default();
    let slug = make(last);
    if slug.is_empty() {
        "page".to_owned()
    } else {
        slug
    }
}

/// Путь внутри архива, если он законный: ровно `хост/имя.md.lz4`, без
/// выхода наверх и без абсолютных путей.
fn checked(path: &str) -> Option<PathBuf> {
    let mut parts = path.split('/');
    let (host, name) = (parts.next()?, parts.next()?);
    if parts.next().is_some() {
        return None;
    }
    // Двоеточие — тоже выход: на Windows `C:` — диск, а `имя:поток` —
    // скрытый поток файла. В имени хоста и в слаге его не бывает.
    let fine = |part: &str| {
        !part.is_empty() && part != "." && part != ".." && !part.contains(['\\', '\0', ':'])
    };
    (fine(host) && fine(name) && name.ends_with(EXTENSION)).then(|| Path::new(host).join(name))
}

/// Текст файла: шапка YAML и markdown страницы.
fn file_text(title: &str, source: &str, read: &Stamp, markdown: &str) -> String {
    format!(
        "---\ntitle: {}\nsource: {}\nread: {}\n---\n\n{}\n",
        quoted(title),
        quoted(source),
        read.text(),
        markdown.trim_end()
    )
}

/// Строка YAML в двойных кавычках: обратная косая и кавычка экранируются,
/// перевод строки в заголовке не нужен вовсе.
fn quoted(text: &str) -> String {
    let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
    format!("\"{}\"", flat.replace('\\', r"\\").replace('"', "\\\""))
}

fn unquoted(value: &str) -> String {
    let value = value.trim();
    let Some(inner) = value.strip_prefix('"').and_then(|v| v.strip_suffix('"')) else {
        return value.to_owned();
    };
    let mut out = String::with_capacity(inner.len());
    let mut chars = inner.chars();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            if let Some(next) = chars.next() {
                out.push(next);
            }
        } else {
            out.push(ch);
        }
    }
    out
}

/// Шапка копии.
struct Head {
    title: String,
    source: String,
    read: Stamp,
}

fn parse_head(text: &str) -> Option<Head> {
    let rest = text.strip_prefix("---\n")?;
    let end = rest.find("\n---")?;
    let (mut title, mut source, mut read) = (String::new(), None, None);
    for line in rest[..end].lines() {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        match key.trim() {
            "title" => title = unquoted(value),
            "source" => source = Some(unquoted(value)),
            "read" => read = Stamp::parse(value.trim()),
            _ => {}
        }
    }
    Some(Head {
        title,
        source: source.filter(|source| !source.is_empty())?,
        read: read?,
    })
}

/// Шапка файла, без распаковки всего текста.
fn read_head(file: &Path) -> Option<Head> {
    let reader = fs::File::open(file).ok()?;
    let mut decoder = lz4_flex::frame::FrameDecoder::new(reader);
    let mut head = vec![0; HEAD_BYTES];
    let mut filled = 0;
    while filled < head.len() {
        match decoder.read(&mut head[filled..]) {
            Ok(0) => break,
            Ok(read) => filled += read,
            Err(_) => return None,
        }
    }
    head.truncate(filled);
    // Обрезка могла прийтись на середину знака — шапке это не мешает.
    let text = String::from_utf8_lossy(&head);
    parse_head(&text)
}

fn compress(bytes: &[u8]) -> Option<Vec<u8>> {
    let mut encoder = lz4_flex::frame::FrameEncoder::new(Vec::new());
    encoder.write_all(bytes).ok()?;
    encoder.finish().ok()
}

fn decompress(bytes: &[u8]) -> Option<String> {
    let mut decoder = lz4_flex::frame::FrameDecoder::new(bytes);
    let mut text = String::new();
    decoder.read_to_string(&mut text).ok()?;
    Some(text)
}

/// Момент прочтения в секундах от эпохи — для порядка «свежие сверху».
fn moment(read: &Stamp) -> i64 {
    read.days * 86_400 + i64::from(read.seconds) - i64::from(read.offset)
}

/// «9 October 2026».
pub fn long_date(read: &Stamp) -> String {
    let (year, month, day) = store::civil_from_days(read.days);
    format!("{day} {} {year}", store::MONTHS[month as usize - 1])
}

fn shown_title(copy: &Copy) -> String {
    if copy.title.is_empty() {
        copy.source.clone()
    } else {
        copy.title.clone()
    }
}

/// Кусок текста вокруг первого найденного слова, одной строкой, со словами
/// запроса жирным.
fn snippet(body: &str, words: &[String]) -> String {
    let flat: String = body
        .lines()
        .filter(|line| !line.trim_start().starts_with('#') && !line.trim_start().starts_with('!'))
        .collect::<Vec<_>>()
        .join(" ");
    let flat = flat.split_whitespace().collect::<Vec<_>>().join(" ");
    let chars: Vec<char> = flat.chars().collect();
    let lower: Vec<char> = flat.chars().flat_map(char::to_lowercase).collect();
    // Нижний регистр бывает длиннее исходного (немецкое ß в верхнем) —
    // тогда ищем по исходному и места не сверяем.
    if lower.len() != chars.len() {
        return String::new();
    }
    let lower_text: String = lower.iter().collect();
    let Some(at) = words.iter().find_map(|word| {
        lower_text
            .find(word.as_str())
            .map(|byte| lower_text[..byte].chars().count())
    }) else {
        return String::new();
    };
    let start = at.saturating_sub(SNIPPET_CHARS / 3);
    let end = (start + SNIPPET_CHARS * 2).min(chars.len());
    let mut out = String::new();
    if start > 0 {
        out.push('…');
    }
    let piece: String = chars[start..end].iter().collect();
    out.push_str(&bold_words(&piece, words));
    if end < chars.len() {
        out.push('…');
    }
    out
}

/// Слова запроса жирным, остальное — экранированным текстом.
fn bold_words(piece: &str, words: &[String]) -> String {
    let chars: Vec<char> = piece.chars().collect();
    let lower: Vec<char> = piece.chars().flat_map(char::to_lowercase).collect();
    if lower.len() != chars.len() {
        return escape_text(piece);
    }
    let mut marked = vec![false; chars.len()];
    for word in words {
        let needle: Vec<char> = word.chars().collect();
        if needle.is_empty() {
            continue;
        }
        let mut i = 0;
        while i + needle.len() <= lower.len() {
            if lower[i..i + needle.len()] == needle[..] {
                marked[i..i + needle.len()]
                    .iter_mut()
                    .for_each(|m| *m = true);
                i += needle.len();
            } else {
                i += 1;
            }
        }
    }
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        let bold = marked[i];
        let mut j = i;
        while j < chars.len() && marked[j] == bold {
            j += 1;
        }
        let run: String = chars[i..j].iter().collect();
        if bold {
            out.push_str(&format!("**{}**", escape_text(&run)));
        } else {
            out.push_str(&escape_text(&run));
        }
        i = j;
    }
    out
}

/// Текст, который markdown не должен принять за разметку.
fn escape_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        if matches!(
            ch,
            '\\' | '*' | '_' | '[' | ']' | '`' | '<' | '>' | '#' | '|' | '~'
        ) {
            out.push('\\');
        }
        out.push(ch);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::address::parse;

    fn temporary(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "brevier-archive-{}-{name}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    fn article(url: &str, title: &str, text: &str) -> Document {
        Document {
            address: parse(url).unwrap(),
            title: title.to_owned(),
            markdown: format!("# {title}\n\n{text}\n"),
            kind: Kind::Article,
            served: false,
            site: Vec::new(),
            feeds: Vec::new(),
            lang: None,
            next: None,
            archived: None,
        }
    }

    fn stamp(text: &str) -> Stamp {
        Stamp::parse(text).unwrap()
    }

    /// Копия — стандартный кадр LZ4 с markdown внутри: шапка и текст,
    /// и её путь — хост и дата со слагом.
    #[test]
    fn a_page_read_is_kept_as_compressed_markdown() {
        let dir = temporary("keep");
        let archive = Archive::at(&dir);
        let read = stamp("2026-10-09T09:36:28+02:00");
        let page = article(
            "https://www.danluu.com/keyboard-latency/",
            "Keyboard \"latency\"",
            "If you look at gaming keyboards…",
        );
        let path = archive.keep(&page, &read).expect("не положили");
        assert_eq!(path, "danluu.com/2026-10-09-keyboard-latency.md.lz4");

        let bytes = fs::read(dir.join(&path)).unwrap();
        // Магическое число кадра LZ4: `lz4 -d` его узнает.
        assert_eq!(&bytes[..4], &[0x04, 0x22, 0x4d, 0x18]);
        let text = decompress(&bytes).unwrap();
        assert!(
            text.starts_with("---\ntitle: \"Keyboard \\\"latency\\\"\"\n"),
            "{text}"
        );
        assert!(text.contains("source: \"https://www.danluu.com/keyboard-latency/\"\n"));
        assert!(text.contains("read: 2026-10-09T09:36:28+02:00\n---\n\n# Keyboard"));

        let copy = archive.read(&path).unwrap();
        assert_eq!(copy.title, "Keyboard \"latency\"");
        assert_eq!(copy.source, "https://www.danluu.com/keyboard-latency/");
        assert_eq!(copy.markdown, text);
        // Сам Brevier читает копию как обычный markdown: заголовок из шапки.
        assert_eq!(
            crate::markdown::front_matter_title(&copy.markdown).as_deref(),
            Some("Keyboard \"latency\"")
        );
    }

    /// Странице отказа (#9) — последняя копия именно этой страницы: соседка
    /// по хосту не в счёт, решётка в адресе не мешает.
    #[test]
    fn the_latest_copy_of_a_page_is_found_by_its_address() {
        let dir = temporary("latest");
        let archive = Archive::at(&dir);
        let url = "https://danluu.com/keyboard-latency/";
        let page = article(url, "Keyboard latency", "Old text.");
        archive.keep(&page, &stamp("2026-10-01T09:00:00Z")).unwrap();
        let page = article(url, "Keyboard latency", "New text.");
        archive.keep(&page, &stamp("2026-10-07T09:00:00Z")).unwrap();
        let other = article("https://danluu.com/input-lag/", "Input lag", "…");
        archive
            .keep(&other, &stamp("2026-10-08T09:00:00Z"))
            .unwrap();

        let copy = archive
            .latest(&parse(&format!("{url}#computers")).unwrap())
            .expect("не нашли");
        assert_eq!(copy.path, "danluu.com/2026-10-07-keyboard-latency.md.lz4");
        assert_eq!(copy.read, stamp("2026-10-07T09:00:00Z"));
        assert_eq!(
            archive.latest(&parse("https://danluu.com/branch-prediction/").unwrap()),
            None
        );
        assert_eq!(
            archive.latest(&parse("https://example.org/").unwrap()),
            None
        );
    }

    /// Ленту, свои страницы и файлы не кладём.
    #[test]
    fn only_articles_from_the_web_and_repositories_are_kept() {
        let archive = Archive::at(temporary("worth"));
        let read = stamp("2026-10-09T09:00:00Z");
        let mut listing = article("https://blog.example.org/", "Blog", "…");
        listing.kind = Kind::Listing;
        assert_eq!(archive.keep(&listing, &read), None);
        let file = article("/tmp/notes.md", "Notes", "…");
        assert_eq!(archive.keep(&file, &read), None);
        let repo = article("gh:rust-lang/book", "The Rust Book", "…");
        assert_eq!(
            archive.keep(&repo, &read).as_deref(),
            Some("github.com/2026-10-09-the-rust-book.md.lz4")
        );
    }

    /// Та же страница в тот же день — та же копия; другая страница под тем
    /// же заголовком — рядом, со своим номером; в другой день — новая копия.
    #[test]
    fn names_are_shared_only_by_the_same_page_on_the_same_day() {
        let archive = Archive::at(temporary("names"));
        let monday = stamp("2026-10-05T10:00:00+02:00");
        let one = article("https://news.example.org/a/1", "Live", "first");
        let two = article("https://news.example.org/a/2", "Live", "second");
        let first = archive.keep(&one, &monday).unwrap();
        assert_eq!(archive.keep(&one, &monday).unwrap(), first);
        assert_eq!(
            archive.keep(&two, &monday).unwrap(),
            "news.example.org/2026-10-05-live-2.md.lz4"
        );
        let tuesday = stamp("2026-10-06T10:00:00+02:00");
        assert_eq!(
            archive.keep(&one, &tuesday).unwrap(),
            "news.example.org/2026-10-06-live.md.lz4"
        );
    }

    /// Кириллица остаётся в имени; заголовка нет — слаг из адреса.
    #[test]
    fn slugs_keep_letters_of_any_script() {
        assert_eq!(
            slug_of("Котлета по-киевски: рецепт", ""),
            "котлета-по-киевски-рецепт"
        );
        assert_eq!(
            slug_of("", "https://e.org/posts/hello-world/?x=1"),
            "hello-world"
        );
        assert_eq!(slug_of("", "https://e.org/"), "e-org");
        assert_eq!(slug_of("…", "https://e.org/!!!/"), "page");
    }

    /// Из адресной строки приходит что угодно: выйти из архива путь не должен.
    #[test]
    fn a_path_cannot_leave_the_archive() {
        let archive = Archive::at(temporary("escape"));
        for path in [
            "../history.tsv",
            "host/../../etc/passwd.md.lz4",
            "/etc/passwd.md.lz4",
            "host/name.txt",
            "a/b/c.md.lz4",
            "..",
            "C:/notes.md.lz4",
            "example.org/x.md.lz4:hidden.md.lz4",
        ] {
            assert!(checked(path).is_none(), "{path}");
            assert!(archive.read(path).is_none(), "{path}");
        }
        assert!(checked("example.org/2026-10-09-x.md.lz4").is_some());
    }

    /// Список — свежие сверху, по дням; поиск — все слова запроса, с куском
    /// текста, где они стоят.
    #[test]
    fn the_archive_lists_and_searches_its_copies() {
        let archive = Archive::at(temporary("search"));
        let old = stamp("2026-10-01T08:00:00Z");
        let new = stamp("2026-10-09T08:00:00Z");
        archive.keep(
            &article(
                "https://a.example.org/x",
                "Borrow checker",
                "The borrow checker rejects this code.",
            ),
            &old,
        );
        archive.keep(
            &article(
                "https://b.example.org/y",
                "Lifetimes",
                "Lifetimes help the Borrow checker reason.",
            ),
            &new,
        );
        archive.keep(
            &article("https://c.example.org/z", "Cooking", "Nothing about Rust."),
            &new,
        );

        let list = archive.list();
        assert_eq!(
            list.iter()
                .map(|copy| copy.title.as_str())
                .collect::<Vec<_>>()[2],
            "Borrow checker"
        );
        let page = archive.page();
        assert!(page.starts_with("# Archive\n\n## "), "{page}");
        assert!(page.contains("(brevier:archive/a.example.org/2026-10-01-borrow-checker.md.lz4)"));
        assert!(page.contains("`lz4 -d`"));

        let found = archive.search("borrow CHECKER");
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].0.title, "Lifetimes");
        assert!(
            found[0].1.contains("**Borrow** **checker**"),
            "{}",
            found[0].1
        );
        assert!(archive.search("borrow cooking").is_empty());

        let results = archive.search_page("borrow checker");
        assert!(
            results.starts_with("# Archive: borrow checker\n\n2 pages.\n"),
            "{results}"
        );
        assert!(archive.search_page("[x]").starts_with("# Archive: \\[x\\]"));
    }

    /// Уборка: копия с журналом остаётся, без него — уходит, у закладки
    /// остаётся последняя копия; свежую не трогаем.
    #[test]
    fn pruning_keeps_what_history_and_bookmarks_point_to() {
        let dir = temporary("prune");
        let archive = Archive::at(&dir);
        let page = |url: &str, title: &str| article(url, title, "text");
        let a = archive
            .keep(
                &page("https://a.org/1", "Kept"),
                &stamp("2026-01-01T08:00:00Z"),
            )
            .unwrap();
        let b = archive
            .keep(
                &page("https://b.org/1", "Gone"),
                &stamp("2026-01-01T08:00:00Z"),
            )
            .unwrap();
        let older = archive
            .keep(
                &page("https://c.org/1", "Marked"),
                &stamp("2026-01-01T08:00:00Z"),
            )
            .unwrap();
        let newer = archive
            .keep(
                &page("https://c.org/1", "Marked"),
                &stamp("2026-02-01T08:00:00Z"),
            )
            .unwrap();
        let kept: HashSet<String> = [a.clone()].into();
        let marked: HashSet<String> = ["https://c.org/1".to_owned()].into();

        // Сейчас всё свежее — уборка ничего не трогает.
        archive.prune(&kept, &marked);
        assert_eq!(archive.list().len(), 4);

        // Через двое суток — уже трогает.
        let later = SystemTime::now() + Duration::from_secs(48 * 3600);
        archive.prune_at(&kept, &marked, later);
        assert!(dir.join(&a).exists());
        assert!(!dir.join(&b).exists());
        assert!(!dir.join(&older).exists());
        assert!(dir.join(&newer).exists());
        // Папка хоста без копий убрана.
        assert!(!dir.join("b.org").exists());
    }

    #[test]
    fn forgetting_empties_the_archive() {
        let dir = temporary("forget");
        let archive = Archive::at(&dir);
        archive.keep(
            &article("https://a.org/1", "One", "x"),
            &stamp("2026-10-09T08:00:00Z"),
        );
        archive.forget();
        assert!(!dir.exists());
        assert!(archive.list().is_empty());
        assert!(archive.page().contains("Nothing here yet"));
    }
}
