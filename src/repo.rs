//! Режим репозитория: markdown приезжает из репозитория как есть.
//!
//! Ни Readability, ни конвертации здесь нет — формат родной, качество
//! рендера стопроцентное. Работы ровно две: сказать, по какому адресу
//! лежит файл, и вернуть ссылкам то, чего в сыром `.md` нет.
//!
//! Второе неочевидно, пока не откроешь README глазами. Гитхаб, когда
//! показывает файл, дописывает к нему контекст репозитория: относительная
//! ссылка `docs/x.md` превращается в ссылку на файл в той же ветке,
//! `logo.png` — в адрес на CDN. В самом файле ничего этого нет, и без
//! разворота документация рассыпается на первой же внутренней ссылке —
//! то есть ровно там, где стоит гейт M2.
//!
//! Разворачиваем в адреса самого хостинга (`blob/HEAD/...`), а не в свою
//! короткую форму: такую ссылку `address::parse` узнаёт и возвращает
//! в режим репозитория, а сохранённый `.md` и «открыть в системном
//! браузере» продолжают работать у тех, у кого Brevier не стоит.

use comrak::Arena;
use comrak::nodes::NodeValue;

use crate::address::{Repo, RepoHost};
use crate::error::Error;
use crate::fetch::{self, UserAgent};

/// Под какими именами в репозитории лежит входная страница. Порядок —
/// порядок проверки; каждая попытка это запрос к CDN, а не к API,
/// поэтому лимита они не тратят.
const README: [&str; 4] = ["README.md", "readme.md", "README.markdown", "Readme.md"];

/// Что в режиме репозитория читается как документ. Всё остальное —
/// исходный код и картинки — честно уходит на страницу хостинга.
const DOCUMENT: [&str; 4] = [".md", ".markdown", ".mdown", ".txt"];

/// Файл репозитория, доехавший до читателя.
pub struct Loaded {
    /// Путь внутри репозитория — тот, что попадёт в адресную строку.
    pub path: String,
    pub markdown: String,
}

/// Точка входа в документацию репозитория.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Documentation {
    /// Как назвать это читателю. Язык интерфейса — английский.
    pub title: String,
    /// Путь внутри репозитория.
    pub path: String,
}

/// Где генераторы документации держат свой конфиг. Список получен замером
/// гейта M2, а не чтением документации: у helix конфиг лежит в `book/`,
/// у zed — в `docs/`, у rust-lang/book — в корне.
const CONFIGS: [(&str, Generator); 8] = [
    ("book.toml", Generator::MdBook),
    ("book/book.toml", Generator::MdBook),
    ("docs/book.toml", Generator::MdBook),
    ("doc/book.toml", Generator::MdBook),
    ("guide/book.toml", Generator::MdBook),
    ("mkdocs.yml", Generator::MkDocs),
    ("mkdocs.yaml", Generator::MkDocs),
    ("docs/mkdocs.yml", Generator::MkDocs),
];

/// Куда смотреть, если конфига не нашлось. Тот же результат, но без
/// уверенности, каким генератором он собран.
const PLAIN: [&str; 5] = [
    "docs/SUMMARY.md",
    "docs/index.md",
    "docs/README.md",
    "doc/index.md",
    "documentation/README.md",
];

/// Имена, которые хостинг показывает своей обвязкой, а не ссылкой из README:
/// вкладка «Contributing», плашка «Security policy», кодекс сообщества.
/// В README ссылки на них может не быть вовсе — гитхаб рисует их сам,
/// вокруг страницы. У нас этой обвязки нет, и без пробы читатель таких
/// файлов не увидит никаким способом: замер гейта M2 нашёл двенадцать
/// из них на десяти проектах.
///
/// Третий элемент — искать ли файл ещё и в служебном каталоге хостинга.
/// Туда обвязку убирают, чтобы не засорять корень, и хостинг про это
/// место знает; у CHANGELOG и LICENSE такого места нет, они всегда в корне.
///
/// CHANGELOG обвязкой не показывается — там у хостинга релизы, — но
/// из README на него ссылаются не всегда, а читателю незнакомого проекта
/// он нужен сразу после README.
const BOILERPLATE: [(&str, &str, bool); 6] = [
    ("Changelog", "CHANGELOG.md", false),
    ("Contributing", "CONTRIBUTING.md", true),
    ("Code of conduct", "CODE_OF_CONDUCT.md", true),
    ("Security", "SECURITY.md", true),
    ("Support", "SUPPORT.md", true),
    ("License", "LICENSE.md", false),
];

/// Куда хостинг убирает обвязку, когда её не держат в корне.
fn hidden_dir(host: RepoHost) -> &'static str {
    match host {
        RepoHost::GitHub => ".github",
        RepoHost::GitLab => ".gitlab",
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Generator {
    MdBook,
    MkDocs,
}

impl Generator {
    fn title(self) -> &'static str {
        match self {
            Generator::MdBook => "Documentation (mdBook)",
            Generator::MkDocs => "Documentation (MkDocs)",
        }
    }

    /// Как из конфига вывести файл, с которого документацию читают.
    /// У mdbook это оглавление `SUMMARY.md` в каталоге `src`, у mkdocs —
    /// `index.md` в каталоге `docs`. Оба каталога настраиваются, оба
    /// имеют умолчание — у rust-lang/book ключа `src` нет вовсе.
    fn index(self, config: &str) -> (String, &'static str) {
        match self {
            Generator::MdBook => (
                value(config, "src").unwrap_or_else(|| "src".into()),
                "SUMMARY.md",
            ),
            Generator::MkDocs => (
                value(config, "docs_dir").unwrap_or_else(|| "docs".into()),
                "index.md",
            ),
        }
    }
}

/// Значение ключа в `book.toml` или `mkdocs.yml`. Свой разбор на два
/// ключа дешевле, чем крейт toml плюс крейт yaml ради одной строки
/// в каждом; форматы здесь пересекаются ровно настолько, насколько нужно.
fn value(config: &str, key: &str) -> Option<String> {
    for line in config.lines() {
        let line = line.trim();
        // Секции нас не интересуют: `src` встречается только в `[book]`,
        // а `docs_dir` в yaml лежит на верхнем уровне.
        let Some(rest) = line.strip_prefix(key) else {
            continue;
        };
        let rest = rest.trim_start();
        let Some(rest) = rest.strip_prefix(['=', ':']) else {
            continue;
        };
        let found = rest.trim().trim_matches(['"', '\'']).trim_end_matches('/');
        if !found.is_empty() && !found.starts_with('#') {
            return Some(found.to_owned());
        }
    }
    None
}

/// Найти точки входа в документацию репозитория.
///
/// README ссылается на собранный сайт, а не на исходные `.md`, и связь
/// обрывается прямо на входе: замер гейта M2 показал, что так теряется
/// 72% документации — 777 файлов из 1085. Генераторы кладут её
/// по соглашению, и соглашение проверяется пробой. Запросы идут на CDN,
/// где лимита нет, и параллельно — иначе дюжина проб стоила бы секунд.
///
/// Второй источник — известные имена обвязки (`BOILERPLATE`): их хостинг
/// рисует вокруг страницы сам, ссылки из README на них может не быть,
/// и без пробы они недостижимы вовсе.
pub fn documentation(repo: &Repo, ua: UserAgent) -> Vec<Documentation> {
    let configs: Vec<(&str, Generator, Option<String>)> = probe(
        CONFIGS.iter().map(|(path, _)| *path).collect(),
        repo,
        ua,
        true,
    )
    .into_iter()
    .zip(CONFIGS)
    .map(|(body, (path, generator))| (path, generator, body))
    .collect();

    let mut wanted: Vec<(String, String)> = Vec::new();
    for (path, generator, body) in configs {
        let Some(body) = body else { continue };
        let (dir, index) = generator.index(&body);
        let base = path.rsplit_once('/').map(|(d, _)| d).unwrap_or("");
        let full = [base, dir.as_str(), index]
            .iter()
            .filter(|part| !part.is_empty())
            .cloned()
            .collect::<Vec<_>>()
            .join("/");
        wanted.push((generator.title().to_owned(), full));
    }
    for path in PLAIN {
        wanted.push(("Documentation".to_owned(), path.to_owned()));
    }
    // Обвязка идёт после документации: читатель пришёл читать проект,
    // а не правила участия в нём.
    for (title, name, hidden) in BOILERPLATE {
        wanted.push((title.to_owned(), name.to_owned()));
        if hidden {
            wanted.push((
                title.to_owned(),
                format!("{}/{name}", hidden_dir(repo.host)),
            ));
        }
    }

    let paths: Vec<&str> = wanted.iter().map(|(_, path)| path.as_str()).collect();
    let found = probe(paths, repo, ua, false);
    select(&wanted, found)
}

/// Оставить по строке на смысл. Один и тот же файл приходит под разными
/// путями — `CONTRIBUTING.md` в корне и в служебном каталоге, `docs/index.md`
/// из конфига mkdocs и из списка известных путей, — а две одинаково
/// подписанные строки выбирать читателю не помогают. Порядок списка — это
/// порядок предпочтения, поэтому остаётся первая из них.
///
/// Цена записана честно: репозиторий с двумя книгами одного генератора
/// покажет одну.
fn select(wanted: &[(String, String)], found: Vec<Option<String>>) -> Vec<Documentation> {
    let mut out: Vec<Documentation> = Vec::new();
    for ((title, path), body) in wanted.iter().zip(found) {
        if body.is_none() || out.iter().any(|d| d.path == *path || d.title == *title) {
            continue;
        }
        out.push(Documentation {
            title: title.clone(),
            path: path.clone(),
        });
    }
    out
}

/// Скачать несколько путей разом. Возвращает тело для тех, что нашлись,
/// в том же порядке. Потоки, а не последовательность: дюжина проб подряд
/// это несколько секунд ожидания на открытии репозитория.
fn probe(paths: Vec<&str>, repo: &Repo, ua: UserAgent, need_body: bool) -> Vec<Option<String>> {
    std::thread::scope(|scope| {
        let handles: Vec<_> = paths
            .iter()
            .map(|path| {
                let url = repo.raw_url(path);
                scope.spawn(move || match fetch::fetch(&url, ua) {
                    Ok(page) if need_body => Some(page.body),
                    Ok(_) => Some(String::new()),
                    Err(_) => None,
                })
            })
            .collect();
        handles
            .into_iter()
            .map(|handle| handle.join().unwrap_or(None))
            .collect()
    })
}

/// Что за путь напечатан в адресе.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    /// Файл, который мы умеем показать: markdown или простой текст.
    Document,
    /// Каталог: `gh:o/n/docs` печатают, имея в виду `docs/README.md`.
    Directory,
    /// Исходник, картинка, что угодно ещё — не наш формат.
    Other,
}

/// Чем считать путь. Расширение решает: точка в последнем сегменте —
/// файл, нет точки — каталог. Правило по форме, а не по списку имён.
pub fn target(path: &str) -> Target {
    let lower = path.trim_matches('/').to_ascii_lowercase();
    if DOCUMENT.iter().any(|ext| lower.ends_with(ext)) {
        return Target::Document;
    }
    let tail = lower.rsplit('/').next().unwrap_or(&lower);
    if tail.contains('.') {
        Target::Other
    } else {
        Target::Directory
    }
}

/// Открыть файл репозитория. Путь не указан — ищем README; указан каталог —
/// ищем README в нём.
pub fn open(repo: &Repo, ua: UserAgent) -> Result<Loaded, Error> {
    let mut last = None;

    for path in candidates(repo.path.as_deref()) {
        match fetch::fetch(&repo.raw_url(&path), ua) {
            Ok(page) => {
                return Ok(Loaded {
                    markdown: expand(&page.body, repo, &path),
                    path,
                });
            }
            // 404 у одного кандидата — не ответ, пробуем следующий.
            Err(Error::HttpStatus(404)) => last = Some(Error::HttpStatus(404)),
            Err(other) => return Err(other),
        }
    }

    Err(last.unwrap_or(Error::HttpStatus(404)))
}

/// Что пробуем скачать по такому пути, по порядку.
fn candidates(path: Option<&str>) -> Vec<String> {
    let Some(path) = path.map(|p| p.trim_matches('/')).filter(|p| !p.is_empty()) else {
        return README.iter().map(|name| (*name).to_owned()).collect();
    };

    match target(path) {
        Target::Directory => README.iter().map(|name| format!("{path}/{name}")).collect(),
        // `Other` сюда не доходит: его отправляют на страницу хостинга
        // раньше. Но если дойдёт — честнее попробовать, чем молча решить
        // за читателя, что файла нет.
        Target::Document | Target::Other => vec![path.to_owned()],
    }
}

/// Дописать файлу контекст репозитория: развернуть относительные ссылки
/// и картинки в абсолютные адреса.
///
/// Правим по тексту, а не печатью разметки: обратная печать comrak
/// экранирует живой текст, поэтому её нет ни здесь, ни в тракте вывода
/// (см. шапку `markdown.rs`). comrak остаётся парсером — он говорит,
/// какие адреса в файле есть и что из них картинка.
pub fn expand(markdown: &str, repo: &Repo, path: &str) -> String {
    let markdown = flatten_html(markdown);
    let markdown = markdown.as_str();

    let arena = Arena::new();
    let root = comrak::parse_document(&arena, markdown, &crate::markdown::options());

    let mut targets: Vec<(String, bool)> = Vec::new();
    for node in root.descendants() {
        let (url, image) = match &node.data.borrow().value {
            NodeValue::Link(link) => (link.url.clone(), false),
            NodeValue::Image(image) => (image.url.clone(), true),
            _ => continue,
        };
        if is_relative(&url) && !targets.iter().any(|(seen, _)| *seen == url) {
            targets.push((url, image));
        }
    }

    let mut text = markdown.to_owned();
    for (url, image) in targets {
        let inside = resolve(path, &url);
        // Картинке нужны байты, значит CDN. Ссылке нужна страница, которую
        // поймёт и наша адресная строка, и чужой браузер.
        let absolute = if image {
            repo.raw_url(&inside)
        } else {
            repo.blob_url(&inside)
        };
        text = retarget(&text, &url, &absolute);
    }
    mentions(&text, repo)
}

/// Свести HTML внутри markdown к markdown же.
///
/// README пишут с вёрсткой: `<div align="center">` вокруг логотипа,
/// `<picture>` с тёмной и светлой темой, `<h1 align>` вместо `#`, значки
/// абзацем из `<a><img></a>`. Хостинг это рисует, а у нас сырой тег так
/// и остался бы в тексте — ровно тот мусор вёрстки, который рубрика
/// считает дефектом.
///
/// Отдельного решения тут не нужно: конвертер HTML → markdown в проекте
/// уже есть, тот же самый, которым переваривается веб-страница. Куски
/// приходят незакрытыми (`<div>` в одном блоке, `</div>` в другом) —
/// htmd на них отдаёт пустую строку, и это правильный ответ.
fn flatten_html(markdown: &str) -> String {
    let arena = Arena::new();
    let root = comrak::parse_document(&arena, markdown, &crate::markdown::options());
    let lines = line_starts(markdown);

    let mut edits: Vec<(usize, usize, String)> = Vec::new();
    for node in root.descendants() {
        let block = match &node.data.borrow().value {
            NodeValue::HtmlBlock(_) => true,
            NodeValue::HtmlInline(_) => false,
            _ => continue,
        };

        let Some((from, to)) = span(&node.data.borrow().sourcepos, &lines, markdown) else {
            continue;
        };
        let Ok(converted) = crate::markdown::from_html(&markdown[from..to]) else {
            continue;
        };

        let converted = converted.trim();
        // Блок стоял отдельным абзацем — пусть абзацем и остаётся;
        // строчный тег внутри предложения строку рвать не должен.
        let replacement = match (block, converted.is_empty()) {
            (_, true) => String::new(),
            (true, false) => format!("{converted}\n"),
            (false, false) => converted.to_owned(),
        };
        edits.push((from, to - from, replacement));
    }

    apply(markdown, edits)
}

/// Упоминания, которые хостинг превращает в ссылки при показе: `@user` —
/// человек, `#123` — обсуждение. В самом файле это просто текст, и читатель
/// об этом не догадывается — видит `@BurntSushi` и не может по нему пойти.
///
/// Трогаем только текстовые узлы и только вне ссылок: содержимое блока кода
/// и код-спана текстовыми узлами не является вовсе, поэтому `#[derive]`
/// и `user@host` в примерах команд остаются собой. Это не осторожность,
/// а причина, по которой замена идёт через разбор, а не поиском по строке.
fn mentions(markdown: &str, repo: &Repo) -> String {
    let arena = Arena::new();
    let root = comrak::parse_document(&arena, markdown, &crate::markdown::options());
    let lines = line_starts(markdown);

    let mut edits: Vec<(usize, usize, String)> = Vec::new();
    for node in root.descendants() {
        if !matches!(node.data.borrow().value, NodeValue::Text(_)) {
            continue;
        }
        // Внутри ссылки не трогаем: вложенных ссылок в markdown нет,
        // и `[@user](...)` внутри чужой ссылки её сломает.
        if node.ancestors().skip(1).any(|up| {
            matches!(
                up.data.borrow().value,
                NodeValue::Link(_) | NodeValue::Image(_)
            )
        }) {
            continue;
        }

        let Some((from, to)) = span(&node.data.borrow().sourcepos, &lines, markdown) else {
            continue;
        };
        find_mentions(&markdown[from..to], repo, from, &mut edits);
    }

    apply(markdown, edits)
}

/// Кусок исходного текста, который занимает узел. `None` — если разбор
/// показал на то, чего в тексте нет: резать по такому нельзя.
fn span(
    sourcepos: &comrak::nodes::Sourcepos,
    lines: &[usize],
    text: &str,
) -> Option<(usize, usize)> {
    let from = offset(lines, sourcepos.start.line, sourcepos.start.column)?;
    let to = offset(lines, sourcepos.end.line, sourcepos.end.column + 1)?;
    let inside = from < to && to <= text.len();
    (inside && text.is_char_boundary(from) && text.is_char_boundary(to)).then_some((from, to))
}

/// Внести правки в текст. С конца: иначе первая же сдвинет все следующие.
fn apply(text: &str, mut edits: Vec<(usize, usize, String)>) -> String {
    edits.sort_by_key(|(at, _, _)| *at);
    let mut out = text.to_owned();
    for (at, len, replacement) in edits.into_iter().rev() {
        out.replace_range(at..at + len, &replacement);
    }
    out
}

/// Найти `@user` и `#123` в куске текста; `base` — его смещение в документе.
fn find_mentions(text: &str, repo: &Repo, base: usize, edits: &mut Vec<(usize, usize, String)>) {
    let bytes = text.as_bytes();
    let mut i = 0;

    while i < bytes.len() {
        let sign = bytes[i];
        if sign != b'@' && sign != b'#' {
            i += 1;
            continue;
        }
        // Перед знаком должно быть начало или разделитель. Иначе `foo@1.2`
        // и `rgb#fff` в тексте превращаются в ссылки на чей-то профиль.
        let boundary = i == 0
            || matches!(
                bytes[i - 1],
                b' ' | b'\t' | b'\n' | b'(' | b'[' | b'<' | b'"' | b'\''
            );
        if !boundary {
            i += 1;
            continue;
        }

        let name = match sign {
            b'@' => take_handle(&bytes[i + 1..]),
            _ => take_number(&bytes[i + 1..]),
        };
        let Some(name) = name else {
            i += 1;
            continue;
        };

        let label = &text[i..i + 1 + name.len()];
        let url = if sign == b'@' {
            repo.host_url(&name)
        } else {
            repo.issue_url(&name)
        };
        edits.push((base + i, label.len(), format!("[{label}]({url})")));
        i += 1 + name.len();
    }
}

/// Имя пользователя по правилам хостингов: буквы, цифры и дефис внутри,
/// до 39 знаков, дефисом не кончается.
fn take_handle(rest: &[u8]) -> Option<String> {
    if !rest.first()?.is_ascii_alphanumeric() {
        return None;
    }
    let end = rest
        .iter()
        .take(39)
        .position(|b| !(b.is_ascii_alphanumeric() || *b == b'-'))
        .unwrap_or(rest.len().min(39));
    let name = std::str::from_utf8(&rest[..end])
        .ok()?
        .trim_end_matches('-');
    (!name.is_empty()).then(|| name.to_owned())
}

/// Номер обсуждения. Без верхней границы смысла нет: пять цифр — номер,
/// пятнадцать — уже что-то другое.
fn take_number(rest: &[u8]) -> Option<String> {
    let end = rest
        .iter()
        .position(|b| !b.is_ascii_digit())
        .unwrap_or(rest.len());
    (1..=6)
        .contains(&end)
        .then(|| String::from_utf8_lossy(&rest[..end]).into_owned())
}

/// Смещения начал строк — по ним sourcepos переводится в байты.
fn line_starts(text: &str) -> Vec<usize> {
    let mut starts = vec![0];
    starts.extend(text.match_indices('\n').map(|(i, _)| i + 1));
    starts
}

fn offset(lines: &[usize], line: usize, column: usize) -> Option<usize> {
    Some(lines.get(line.checked_sub(1)?)? + column.checked_sub(1)?)
}

/// Адрес, который надо разворачивать: не схема, не якорь, не протокольная
/// ссылка `//host/path`.
fn is_relative(url: &str) -> bool {
    let url = url.trim();
    !url.is_empty()
        && !url.starts_with('#')
        && !url.starts_with("//")
        && !url.contains("://")
        && !url.starts_with("mailto:")
}

/// Путь внутри репозитория, к которому ведёт ссылка из файла `current`.
///
/// Якорь остаётся приклеенным к пути: он адресует место в целевом файле,
/// и по нему потом прокручивают страницу.
fn resolve(current: &str, target: &str) -> String {
    let (target, anchor) = match target.split_once('#') {
        Some((path, anchor)) => (path, Some(anchor)),
        None => (target, None),
    };

    let mut parts: Vec<&str> = if target.starts_with('/') {
        Vec::new()
    } else {
        match current.rsplit_once('/') {
            Some((dir, _)) => dir.split('/').filter(|p| !p.is_empty()).collect(),
            None => Vec::new(),
        }
    };

    for segment in target.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            other => parts.push(other),
        }
    }

    let path = parts.join("/");
    match anchor {
        Some(anchor) => format!("{path}#{anchor}"),
        None => path,
    }
}

/// Заменить адрес во всех местах, где markdown разрешает его записать:
/// в самой ссылке и в отдельном определении внизу файла. Ссылками
/// «по имени» README пользуются охотно, и без второй формы половина
/// разворота проходит мимо.
fn retarget(markdown: &str, from: &str, to: &str) -> String {
    let mut out = String::with_capacity(markdown.len());
    let mut rest = markdown;

    loop {
        // Начало адреса: `](` у ссылки в тексте, `]:` у определения внизу файла.
        let opening = [rest.find("]("), rest.find("]:")];
        let Some(index) = opening.into_iter().flatten().min() else {
            break;
        };

        let head = index + "](".len();
        let (spaces, tail) = split_spaces(&rest[head..]);
        let (target, after) = split_target(tail);

        out.push_str(&rest[..head]);
        out.push_str(spaces);
        out.push_str(if target == from { to } else { target });
        rest = after;
    }

    out.push_str(rest);
    out
}

/// Пробелы после `](` или `]:` — их положено сохранить как было.
fn split_spaces(text: &str) -> (&str, &str) {
    let end = text
        .find(|c: char| c != ' ' && c != '\t')
        .unwrap_or(text.len());
    text.split_at(end)
}

/// Сам адрес: до пробела, закрывающей скобки или конца строки. Форму
/// `<адрес>` пропускаем как есть — она редкая, и трогать её незачем.
fn split_target(text: &str) -> (&str, &str) {
    let end = text
        .find([')', ' ', '\t', '\n', '\r', '"', '\''])
        .unwrap_or(text.len());
    text.split_at(end)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::address::{self, Address, RepoHost};

    fn repo() -> Repo {
        Repo {
            host: RepoHost::GitHub,
            owner: "tokio-rs".to_owned(),
            name: "tokio".to_owned(),
            path: None,
            source: None,
        }
    }

    #[test]
    fn relative_links_come_back_as_repository_addresses() {
        // Свойство, на котором держится весь режим: ссылку, которую мы
        // вписали в документ, наш же разбор обязан узнать и вернуть
        // в режим репозитория, а не открыть страницей github.
        let md = expand("см. [гайд](docs/guide.md)", &repo(), "README.md");
        let url = md
            .split_once("](")
            .and_then(|(_, tail)| tail.split_once(')'))
            .map(|(url, _)| url.to_owned())
            .expect("ссылка на месте");

        match address::parse(&url).unwrap() {
            Address::Repo(back) => {
                assert_eq!(back.path.as_deref(), Some("docs/guide.md"));
                assert_eq!(back.name, "tokio");
            }
            other => panic!("ожидался репозиторий, вышло {other:?}"),
        }
    }

    #[test]
    fn pictures_go_to_the_cdn_and_links_to_the_page() {
        // Картинке нужны байты, ссылке — страница. Разные адреса,
        // и перепутать их значит показать читателю html вместо png.
        let md = expand(
            "![схема](assets/flow.png) и [текст](CONTRIBUTING.md)",
            &repo(),
            "README.md",
        );
        assert!(
            md.contains("(https://raw.githubusercontent.com/tokio-rs/tokio/HEAD/assets/flow.png)")
        );
        assert!(md.contains("(https://github.com/tokio-rs/tokio/blob/HEAD/CONTRIBUTING.md)"));
    }

    #[test]
    fn links_by_name_are_expanded_too() {
        // Ссылки «по имени» с определением внизу файла README любят,
        // и без второй формы половина разворота проходит мимо.
        let md = expand("см. [гайд][g]\n\n[g]: docs/guide.md", &repo(), "README.md");
        assert!(
            md.contains("[g]: https://github.com/tokio-rs/tokio/blob/HEAD/docs/guide.md"),
            "определение не развернулось:\n{md}"
        );
    }

    #[test]
    fn what_is_already_absolute_is_left_alone() {
        let md = expand(
            "[сеть](https://example.com/a) и [место](#anchor) и [почта](mailto:a@b.c)",
            &repo(),
            "README.md",
        );
        assert!(md.contains("(https://example.com/a)"));
        assert!(md.contains("(#anchor)"));
        assert!(md.contains("(mailto:a@b.c)"));
    }

    #[test]
    fn a_badge_keeps_its_two_addresses_apart() {
        // `[![значок](значок.svg)](куда-ведёт)` — самая частая конструкция
        // в README, и в ней картинка и ссылка стоят вплотную.
        let md = expand("[![сборка](badge.svg)](docs/ci.md)", &repo(), "README.md");
        assert!(md.contains("(https://raw.githubusercontent.com/tokio-rs/tokio/HEAD/badge.svg)"));
        assert!(md.contains("(https://github.com/tokio-rs/tokio/blob/HEAD/docs/ci.md)"));
    }

    /// Что вернула проба: `Some` — файл есть.
    fn probed(wanted: &[(&str, &str)], present: &[&str]) -> Vec<Documentation> {
        let wanted: Vec<(String, String)> = wanted
            .iter()
            .map(|(title, path)| ((*title).to_owned(), (*path).to_owned()))
            .collect();
        let found = wanted
            .iter()
            .map(|(_, path)| present.contains(&path.as_str()).then(String::new))
            .collect();
        select(&wanted, found)
    }

    #[test]
    fn the_same_file_in_two_places_is_one_row() {
        // `CONTRIBUTING.md` лежит и в корне, и в служебном каталоге —
        // для читателя это одна и та же вкладка хостинга.
        let found = probed(
            &[
                ("Contributing", "CONTRIBUTING.md"),
                ("Contributing", ".github/CONTRIBUTING.md"),
            ],
            &["CONTRIBUTING.md", ".github/CONTRIBUTING.md"],
        );
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].path, "CONTRIBUTING.md");
    }

    #[test]
    fn the_hidden_directory_answers_when_the_root_is_empty() {
        let found = probed(
            &[
                ("Security", "SECURITY.md"),
                ("Security", ".github/SECURITY.md"),
            ],
            &[".github/SECURITY.md"],
        );
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].path, ".github/SECURITY.md");
    }

    #[test]
    fn documentation_comes_before_the_boilerplate() {
        // И один и тот же путь, пришедший из конфига и из списка известных,
        // не удваивается.
        let found = probed(
            &[
                ("Documentation (MkDocs)", "docs/index.md"),
                ("Documentation", "docs/index.md"),
                ("Contributing", "CONTRIBUTING.md"),
            ],
            &["docs/index.md", "CONTRIBUTING.md"],
        );
        let rows: Vec<&str> = found.iter().map(|d| d.path.as_str()).collect();
        assert_eq!(rows, ["docs/index.md", "CONTRIBUTING.md"]);
        assert_eq!(found[0].title, "Documentation (MkDocs)");
    }

    #[test]
    fn the_hidden_directory_belongs_to_the_host() {
        assert_eq!(hidden_dir(RepoHost::GitHub), ".github");
        assert_eq!(hidden_dir(RepoHost::GitLab), ".gitlab");
    }

    #[test]
    fn the_documentation_root_comes_from_the_config() {
        // helix: конфиг в `book/`, ключ есть. Путь считается от конфига,
        // а не от корня репозитория.
        let (dir, index) = Generator::MdBook.index("[book]\nlanguage = \"en\"\nsrc = \"src\"\n");
        assert_eq!((dir.as_str(), index), ("src", "SUMMARY.md"));
    }

    #[test]
    fn a_missing_key_means_the_generator_default() {
        // rust-lang/book: ключа `src` в конфиге нет вовсе, и это не ошибка —
        // у mdbook есть умолчание. Читать надо `src/SUMMARY.md`.
        let (dir, _) =
            Generator::MdBook.index("[book]\ntitle = \"The Rust Programming Language\"\n");
        assert_eq!(dir, "src");
        let (dir, index) = Generator::MkDocs.index("site_name: uv\ntheme:\n  name: material\n");
        assert_eq!((dir.as_str(), index), ("docs", "index.md"));
    }

    #[test]
    fn the_value_is_read_from_both_formats() {
        assert_eq!(
            value("src = \"book/src\"", "src").as_deref(),
            Some("book/src")
        );
        assert_eq!(
            value("docs_dir: mydocs", "docs_dir").as_deref(),
            Some("mydocs")
        );
        assert_eq!(
            value("docs_dir: 'my docs'", "docs_dir").as_deref(),
            Some("my docs")
        );
        // Хвостовой слэш убираем: путь потом склеивается через `/`.
        assert_eq!(value("src = \"src/\"", "src").as_deref(), Some("src"));
        // Чужой ключ, начинающийся так же, не считается.
        assert_eq!(value("source = \"x\"", "src"), None);
        assert_eq!(value("[book]\ntitle = \"t\"", "src"), None);
    }

    #[test]
    fn markup_inside_the_file_is_flattened() {
        // И заодно проверка порядка проходов: картинка из html приходит
        // с относительным адресом, и развернуть его должен следующий проход.
        let md = expand(
            "<div align=\"center\">\n  <h1>bat</h1>\n  <img src=\"doc/logo.svg\" alt=\"логотип\">\n</div>\n\nтекст\n",
            &repo(),
            "README.md",
        );
        assert!(!md.contains("<div"), "сырая вёрстка осталась:\n{md}");
        assert!(md.contains("# bat"), "{md}");
        assert!(
            md.contains("(https://raw.githubusercontent.com/tokio-rs/tokio/HEAD/doc/logo.svg)"),
            "{md}"
        );
    }

    #[test]
    fn mentions_become_links() {
        let md = expand("спасибо @BurntSushi, см. #123", &repo(), "README.md");
        assert!(
            md.contains("[@BurntSushi](https://github.com/BurntSushi)"),
            "{md}"
        );
        assert!(
            md.contains("[#123](https://github.com/tokio-rs/tokio/issues/123)"),
            "{md}"
        );
    }

    #[test]
    fn code_keeps_its_grid_and_its_at_signs() {
        // Ровно та причина, по которой замена идёт через разбор, а не
        // поиском по строке: в примерах команд `@` и `#` — не упоминания.
        let md = expand(
            "```\nssh user@host  # комментарий\n```\n\nа `#[derive(Debug)]` тоже\n",
            &repo(),
            "README.md",
        );
        assert!(md.contains("ssh user@host  # комментарий"), "{md}");
        assert!(md.contains("`#[derive(Debug)]`"), "{md}");
    }

    #[test]
    fn a_version_is_not_a_person() {
        // `foo@1.2` и `rgb#fff` не упоминания: перед знаком должен стоять
        // разделитель, иначе ссылками обрастает половина текста.
        let md = expand(
            "ставится как npm i pkg@1.2, цвет #fff",
            &repo(),
            "README.md",
        );
        assert!(!md.contains("]("), "лишние ссылки:\n{md}");
    }

    #[test]
    fn a_mention_inside_a_link_is_left_alone() {
        // Вложенных ссылок в markdown нет: подстановка внутрь чужой ссылки
        // сломала бы её.
        let md = expand("[@user и #1](docs/a.md)", &repo(), "README.md");
        assert!(
            md.contains("[@user и #1](https://github.com/tokio-rs/tokio/blob/HEAD/docs/a.md)"),
            "{md}"
        );
    }

    #[test]
    fn paths_are_resolved_against_the_file_that_links() {
        assert_eq!(resolve("docs/guide.md", "api.md"), "docs/api.md");
        assert_eq!(resolve("docs/guide.md", "./api.md"), "docs/api.md");
        assert_eq!(resolve("docs/a/b.md", "../c.md"), "docs/c.md");
        assert_eq!(resolve("docs/guide.md", "/README.md"), "README.md");
        assert_eq!(resolve("README.md", "docs/guide.md"), "docs/guide.md");
        // Якорь адресует место в целевом файле — он едет вместе с путём.
        assert_eq!(resolve("README.md", "docs/g.md#start"), "docs/g.md#start");
    }

    #[test]
    fn a_path_says_whether_it_is_a_document() {
        assert_eq!(target("README.md"), Target::Document);
        assert_eq!(target("docs/guide.markdown"), Target::Document);
        assert_eq!(target("docs"), Target::Directory);
        assert_eq!(target("docs/"), Target::Directory);
        assert_eq!(target("src/main.rs"), Target::Other);
        assert_eq!(target("logo.png"), Target::Other);
    }

    #[test]
    fn a_directory_means_the_readme_inside_it() {
        assert_eq!(candidates(Some("docs"))[0], "docs/README.md");
        assert_eq!(candidates(Some("docs/guide.md")), vec!["docs/guide.md"]);
        assert_eq!(candidates(None)[0], "README.md");
    }
}
