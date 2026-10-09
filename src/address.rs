//! Адресная строка — она же командная строка продукта.
//!
//! Одно поле принимает всё, что человек может туда положить: полный URL,
//! голый домен, `gh:owner/repo`, вставленную ссылку на github и путь
//! к локальному `.md`. Разбор отделён от загрузки: строку понимаем уже
//! сейчас, а режим репозитория приезжает на M2 — до тех пор такой адрес
//! честно открывается как обычная веб-страница.

use std::path::PathBuf;

use crate::error::Error;

/// Куда ведёт напечатанное.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Address {
    /// Веб-страница: скачать, извлечь статью, свести к markdown.
    Web(String),
    /// Репозиторий с документацией.
    Repo(Repo),
    /// Локальный файл: формат родной, конвертировать нечего.
    File(PathBuf),
    /// Страница самой программы: история, а дальше и закладки. Адрес у неё
    /// настоящий (`brevier:history`), потому что иначе её не положить
    /// ни в историю вкладки, ни в адресную строку — а браузеры, у которых
    /// это `chrome://history` и `about:history`, ровно за это её и держат.
    Internal(Internal),
}

/// Какая из внутренних страниц.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Internal {
    History,
    Bookmarks,
    /// Проверка страницы (`--check`): что стоит между ней и чтением.
    /// Адрес проверяемой страницы — часть адреса проверки
    /// (`brevier:check/https://…`), иначе отчёт не положить ни в историю
    /// вкладки, ни в сессию. Без него — страница о том, как проверять.
    Check(Option<String>),
    /// Архив прочитанного (#8): весь список, поиск по нему или одна копия.
    Archive(ArchivePage),
    /// Снимок страницы у Wayback Machine (#9): `brevier:wayback/https://…`.
    /// Свой адрес, а не адрес снимка: какой снимок последний живой, узнают
    /// только по нажатию, запросом к Wayback.
    Wayback(String),
}

/// Что из архива открыто.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArchivePage {
    /// `brevier:archive` — все копии по дням.
    List,
    /// `brevier:archive?q=слова` — страницы, где есть все слова.
    Search(String),
    /// `brevier:archive/<хост>/<файл>.md.lz4` — сама копия.
    Copy(String),
}

impl Internal {
    /// Имя после `brevier:` — оно же то, что читатель печатает.
    pub fn name(&self) -> &'static str {
        match self {
            Internal::History => "history",
            Internal::Bookmarks => "bookmarks",
            Internal::Check(_) => "check",
            Internal::Archive(_) => "archive",
            Internal::Wayback(_) => "wayback",
        }
    }

    fn of_name(name: &str) -> Option<Self> {
        let name = name.trim_start_matches("//");
        match name.trim_end_matches('/') {
            "history" => return Some(Internal::History),
            "bookmarks" => return Some(Internal::Bookmarks),
            "check" => return Some(Internal::Check(None)),
            "archive" => return Some(Internal::Archive(ArchivePage::List)),
            _ => {}
        }
        // Поиск по архиву печатают руками, со словами через пробел; из ссылки
        // он приходит с процентными кодами. Плюс не трогаем: «c++» — запрос.
        if let Some(query) = name.strip_prefix("archive?q=") {
            let query = percent_decoded(query);
            let query = query.trim();
            return Some(Internal::Archive(if query.is_empty() {
                ArchivePage::List
            } else {
                ArchivePage::Search(query.to_owned())
            }));
        }
        if let Some(path) = name.strip_prefix("archive/") {
            let path = percent_decoded(path);
            return (path.ends_with(crate::archive::EXTENSION) && !path.contains(".."))
                .then_some(Internal::Archive(ArchivePage::Copy(path)));
        }
        // Снимок ищут у веб-страницы, как и проверяют только её.
        if let Some(page) = name.strip_prefix("wayback/") {
            return match parse(page) {
                Ok(Address::Web(url)) => Some(Internal::Wayback(url)),
                _ => None,
            };
        }
        // Проверяют веб-страницу: у репозитория и файла сайта нет, а своя
        // страница проверки не требует. Голый домен разбирается как везде.
        match parse(name.strip_prefix("check/")?) {
            Ok(Address::Web(url)) => Some(Internal::Check(Some(url))),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Repo {
    pub host: RepoHost,
    pub owner: String,
    pub name: String,
    /// Путь внутри репозитория, если он был в адресе.
    pub path: Option<String>,
    /// Просят сам каталог, а не README в нём: ссылка вида `tree/HEAD/путь`
    /// или короткая форма с косой чертой на конце (`gh:o/n/crates/`).
    /// Так хостинг и отличает каталог от файла, и мы не изобретаем своего.
    pub listing: bool,
    /// Исходный URL, если адрес пришёл ссылкой, а не короткой формой.
    pub source: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepoHost {
    GitHub,
    GitLab,
}

impl RepoHost {
    fn domain(self) -> &'static str {
        match self {
            RepoHost::GitHub => "github.com",
            RepoHost::GitLab => "gitlab.com",
        }
    }

    fn of_domain(domain: &str) -> Option<Self> {
        match domain.trim_start_matches("www.") {
            "github.com" => Some(RepoHost::GitHub),
            "gitlab.com" => Some(RepoHost::GitLab),
            _ => None,
        }
    }
}

impl Repo {
    /// Тот же файл обычной ссылкой на хостинг. По ней открываются вещи,
    /// которые режим репозитория не показывает, — исходники и картинки.
    ///
    /// Вставленную ссылку возвращаем как есть: пользователь дал точный
    /// адрес, включая ветку, и угадывать за него нечего.
    pub fn web_url(&self) -> String {
        match (&self.source, &self.path) {
            (Some(url), _) => url.clone(),
            (None, path) if self.listing => self.tree_url(path.as_deref().unwrap_or("")),
            (None, Some(path)) => self.blob_url(path),
            (None, None) => format!(
                "https://{}/{}/{}",
                self.host.domain(),
                self.owner,
                self.name
            ),
        }
    }

    /// Адрес файла на CDN хостинга: сырые байты, без страницы вокруг.
    ///
    /// Ветку не называем: `HEAD` хостинг сам разворачивает в основную,
    /// поэтому отдельного запроса «а какая тут ветка по умолчанию»
    /// не нужно — а он стоил бы обращения к API, где лимит.
    pub fn raw_url(&self, path: &str) -> String {
        let (owner, name) = (&self.owner, &self.name);
        match self.host {
            RepoHost::GitHub => {
                format!("https://raw.githubusercontent.com/{owner}/{name}/HEAD/{path}")
            }
            RepoHost::GitLab => format!("https://gitlab.com/{owner}/{name}/-/raw/HEAD/{path}"),
        }
    }

    /// Профиль человека на хостинге. Нужен для `@user`: в сыром файле это
    /// просто текст, а хостинг при показе делает из него ссылку.
    pub fn host_url(&self, handle: &str) -> String {
        format!("https://{}/{handle}", self.host.domain())
    }

    /// Обсуждение по номеру — то, что в тексте написано как `#123`.
    /// У github ссылка на issue сама уводит на pull request, если номер
    /// оказался его, поэтому различать их не нужно.
    pub fn issue_url(&self, number: &str) -> String {
        let (owner, name) = (&self.owner, &self.name);
        match self.host {
            RepoHost::GitHub => format!("https://github.com/{owner}/{name}/issues/{number}"),
            RepoHost::GitLab => format!("https://gitlab.com/{owner}/{name}/-/issues/{number}"),
        }
    }

    /// Адрес файла страницей хостинга. В него разворачиваются ссылки внутри
    /// документа: такую ссылку [`parse`] узнаёт и возвращает читателя
    /// в режим репозитория, а у того, кто откроет её без Brevier, она просто
    /// работает. Своя короткая форма ни того, ни другого не умеет.
    pub fn blob_url(&self, path: &str) -> String {
        let (owner, name) = (&self.owner, &self.name);
        match self.host {
            RepoHost::GitHub => format!("https://github.com/{owner}/{name}/blob/HEAD/{path}"),
            RepoHost::GitLab => format!("https://gitlab.com/{owner}/{name}/-/blob/HEAD/{path}"),
        }
    }

    /// Адрес каталога страницей хостинга. Ссылки на подкаталоги в листинге
    /// ведут сюда — и [`parse`] узнаёт их обратно, как и ссылки на файлы.
    pub fn tree_url(&self, path: &str) -> String {
        let (owner, name) = (&self.owner, &self.name);
        match self.host {
            RepoHost::GitHub => format!("https://github.com/{owner}/{name}/tree/HEAD/{path}"),
            RepoHost::GitLab => format!("https://gitlab.com/{owner}/{name}/-/tree/HEAD/{path}"),
        }
    }

    /// Где спросить, что лежит в каталоге.
    ///
    /// Единственное место, где режим репозитория трогает API: каталогов
    /// CDN не отдаёт вовсе. Спрашиваем один каталог и только по требованию
    /// читателя — не дерево целиком: у gitlab оно постранично, у github
    /// усекается на больших репозиториях, и стоит это того же лимита.
    pub fn listing_api(&self, path: &str) -> String {
        let (owner, name) = (&self.owner, &self.name);
        match self.host {
            RepoHost::GitHub => format!(
                "https://api.github.com/repos/{owner}/{name}/contents/{}?ref=HEAD",
                encode_path(path)
            ),
            // Проект у gitlab адресуется одним полем, поэтому косая черта
            // внутри имени группы тоже уезжает в процентный код.
            RepoHost::GitLab => format!(
                "https://gitlab.com/api/v4/projects/{}%2F{}/repository/tree?path={}&ref=HEAD&per_page=100",
                encode_segment(owner),
                encode_segment(name),
                encode_segment(path)
            ),
        }
    }
}

/// Процентные коды в UTF-8: `%D0%BA` → «к». Битый код остаётся как был.
fn percent_decoded(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let Some(byte) = std::str::from_utf8(&bytes[i + 1..i + 3])
                .ok()
                .and_then(|hex| u8::from_str_radix(hex, 16).ok())
        {
            out.push(byte);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
impl Repo {
    /// Короткая форма этого адреса — то, что увидит читатель в строке.
    /// Печатает её [`Address::display`]; тестам нужен тот же путь,
    /// а не его копия.
    fn display_for_test(&self) -> String {
        Address::Repo(self.clone()).display()
    }
}

/// Процентное кодирование пути: разделители каталогов остаются собой,
/// остальное неразрешённое уезжает в коды. Своё, потому что нужно
/// ровно здесь и ровно на это.
fn encode_path(path: &str) -> String {
    path.split('/')
        .map(encode_segment)
        .collect::<Vec<_>>()
        .join("/")
}

fn encode_segment(segment: &str) -> String {
    let mut out = String::with_capacity(segment.len());
    for byte in segment.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char);
            }
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}

impl Address {
    /// Чем этот адрес открыть в системном браузере.
    ///
    /// Не то же самое, что [`display`](Self::display): короткую форму
    /// `gh:owner/repo` чужой браузер не понимает, ему нужен настоящий URL.
    pub fn external(&self) -> String {
        match self {
            Address::Web(url) => url.clone(),
            Address::Repo(repo) => repo.web_url(),
            Address::File(path) => format!("file://{}", path.display()),
            // Проверка — о странице, и снаружи ей соответствует сама страница.
            Address::Internal(Internal::Check(Some(url))) => url.clone(),
            // Браузеру — календарь Wayback: в нём видны все снимки страницы.
            Address::Internal(Internal::Wayback(url)) => crate::hosts::wayback_calendar(url),
            // Остальным внутренним страницам снаружи соответствия нет. Пустая
            // строка здесь честнее выдуманного адреса: окно по ней и понимает,
            // что отдавать чужому браузеру нечего.
            Address::Internal(_) => String::new(),
        }
    }

    /// Адрес проверки этой страницы — `brevier:check/…`. Проверяют
    /// веб-страницу; у проверки это она сама, то есть перепроверка.
    /// Репозиторию, файлу и своим страницам проверять нечего.
    pub fn check(&self) -> Option<Address> {
        match self {
            Address::Web(url) => Some(Address::Internal(Internal::Check(Some(url.clone())))),
            Address::Internal(Internal::Check(Some(_))) => Some(self.clone()),
            _ => None,
        }
    }

    /// Страница самой программы: сети за ней нет, и наружу её не отдать.
    pub fn is_internal(&self) -> bool {
        matches!(self, Address::Internal(_))
    }

    /// Как показать адрес в строке. Для веба — сам URL, для репозитория —
    /// короткая форма, которую человек и напечатал бы.
    pub fn display(&self) -> String {
        match self {
            Address::Web(url) => url.clone(),
            Address::Repo(repo) => {
                let prefix = match repo.host {
                    RepoHost::GitHub => "gh",
                    RepoHost::GitLab => "gl",
                };
                match &repo.path {
                    // Каталог показываем с косой чертой на конце: по ней
                    // напечатанное разбирается обратно в тот же каталог,
                    // а не в README внутри него.
                    Some(path) if repo.listing => {
                        format!("{prefix}:{}/{}/{path}/", repo.owner, repo.name)
                    }
                    Some(path) => format!("{prefix}:{}/{}/{path}", repo.owner, repo.name),
                    None if repo.listing => format!("{prefix}:{}/{}/", repo.owner, repo.name),
                    None => format!("{prefix}:{}/{}", repo.owner, repo.name),
                }
            }
            Address::File(path) => path.display().to_string(),
            Address::Internal(Internal::Check(Some(url))) => format!("brevier:check/{url}"),
            Address::Internal(Internal::Wayback(url)) => format!("brevier:wayback/{url}"),
            Address::Internal(Internal::Archive(ArchivePage::Search(query))) => {
                format!("brevier:archive?q={query}")
            }
            Address::Internal(Internal::Archive(ArchivePage::Copy(path))) => {
                format!("brevier:archive/{path}")
            }
            Address::Internal(page) => format!("brevier:{}", page.name()),
        }
    }
}

/// Разобрать то, что напечатано в адресной строке.
pub fn parse(input: &str) -> Result<Address, Error> {
    let text = input.trim();
    if text.is_empty() {
        return Err(Error::BadUrl(String::new()));
    }

    // `?` впереди — поиск, что бы ни шло дальше: так ищут и то, что само
    // похоже на адрес («?danluu.com»).
    if let Some(query) = text.strip_prefix('?') {
        let query = query.trim();
        if query.is_empty() {
            return Err(Error::BadUrl(text.to_owned()));
        }
        return Ok(search(query));
    }

    let spaced = text.contains(char::is_whitespace);
    if !spaced {
        if let Some(rest) = text.strip_prefix("gh:") {
            return shorthand(RepoHost::GitHub, rest);
        }
        if let Some(rest) = text.strip_prefix("gl:") {
            return shorthand(RepoHost::GitLab, rest);
        }
    }

    if let Some((scheme, rest)) = split_scheme(text)
        && (!spaced || KNOWN_SCHEMES.contains(&scheme))
    {
        // `имя:порт` — не схема, а хост в локальной сети или у себя
        // на машине: dev-сервер, NAS. Туда ходят по http.
        if is_port(rest) {
            let url = format!("http://{text}");
            return Ok(from_url(&url, text));
        }
        return match scheme {
            "http" | "https" => Ok(from_url(text, rest)),
            // `feed:` — старая схема ссылок «подписаться»: `feed://host/path`
            // или `feed:https://host/path`. Это адрес ленты, и мы её читаем.
            // Без явной схемы — https: сайты, живущие на одном http, переезд
            // с https делают сами, а обратное небезопасно.
            "feed" => {
                let inner = rest.trim_start();
                if inner.starts_with("http://") || inner.starts_with("https://") {
                    parse(inner)
                } else if let Some(rest) = inner.strip_prefix("//") {
                    let url = format!("https://{rest}");
                    Ok(from_url(&url, rest))
                } else {
                    Err(Error::BadUrl(text.to_owned()))
                }
            }
            "file" => Ok(Address::File(PathBuf::from(
                rest.trim_start_matches("//").to_owned(),
            ))),
            // Своя схема — для своих страниц. Имя после двоеточия одно
            // на всю программу, поэтому неизвестное это опечатка, а не
            // адрес, который стоило бы попробовать загрузить.
            "brevier" => Internal::of_name(rest)
                .map(Address::Internal)
                .ok_or_else(|| Error::BadUrl(text.to_owned())),
            other => Err(Error::UnsupportedScheme(other.to_owned())),
        };
    }

    if looks_like_path(text) {
        return Ok(Address::File(PathBuf::from(text)));
    }

    if !spaced {
        // Своя машина — по http: у dev-сервера сертификата нет.
        let host = text.split(['/', '?', '#']).next().unwrap_or(text);
        if host.eq_ignore_ascii_case("localhost") {
            let url = format!("http://{text}");
            return Ok(from_url(&url, text));
        }
        // Голый домен: человек печатает «danluu.com», а не «https://danluu.com».
        if is_domain(host) {
            let url = format!("https://{text}");
            return Ok(from_url(&url, text));
        }
    }

    // Не адрес — значит, запрос (#25): так ведёт себя любой браузер,
    // и отвечать на слова «это не адрес» значило бы бросить читателя.
    Ok(search(text))
}

/// Схемы, которые мы открываем. Незнакомая схема с пробелами после неё —
/// это текст («rust: borrow checker»), а не адрес.
const KNOWN_SCHEMES: [&str; 5] = ["http", "https", "feed", "file", "brevier"];

/// Адрес выдачи поисковика по запросу.
fn search(query: &str) -> Address {
    Address::Web(crate::hosts::search_url(query))
}

/// После двоеточия — порт: цифры, а за ними конец или путь.
fn is_port(rest: &str) -> bool {
    let digits = rest.chars().take_while(char::is_ascii_digit).count();
    digits > 0
        && digits <= 5
        && matches!(rest[digits..].chars().next(), None | Some('/' | '?' | '#'))
}

/// Похоже на имя хоста в вебе: через точку, и последнее звено — буквенный
/// домен верхнего уровня, либо это IPv4. «3.14» и «e.g.» доменами не считаем:
/// лучше найти их поиском, чем стучаться в несуществующий хост.
fn is_domain(host: &str) -> bool {
    let host = host.rsplit_once('@').map_or(host, |(_, host)| host);
    let host = host.split(':').next().unwrap_or(host).trim_end_matches('.');
    let labels: Vec<&str> = host.split('.').collect();
    if labels.len() < 2 || labels.iter().any(|label| label.is_empty()) {
        return false;
    }
    let ipv4 = labels.len() == 4
        && labels
            .iter()
            .all(|label| label.len() <= 3 && label.chars().all(|c| c.is_ascii_digit()));
    let tld = labels.last().copied().unwrap_or_default();
    let named =
        tld.chars().count() >= 2 && tld.chars().all(char::is_alphabetic) || tld.starts_with("xn--");
    let clean = labels.iter().all(|label| {
        label
            .chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
    });
    ipv4 || (named && clean)
}

fn shorthand(host: RepoHost, rest: &str) -> Result<Address, Error> {
    // Косая черта на конце — это «сам каталог», как на хостинге.
    let listing = rest.trim_end().ends_with('/');
    let mut parts = rest.trim_matches('/').splitn(3, '/');
    let owner = parts.next().unwrap_or_default();
    let name = parts.next().unwrap_or_default();
    if owner.is_empty() || name.is_empty() {
        return Err(Error::BadUrl(rest.to_owned()));
    }
    let path = parts.next().filter(|p| !p.is_empty()).map(str::to_owned);
    Ok(Address::Repo(Repo {
        host,
        owner: owner.to_owned(),
        name: name.to_owned(),
        // Правило одно на любую глубину, включая корень: черта на конце —
        // это каталог. Иначе адрес листинга не разбирался бы обратно
        // в листинг, а на этом свойстве держится весь режим.
        listing,
        path,
        source: None,
    }))
}

/// URL со схемой: смотрим на хост — не github ли это.
fn from_url(url: &str, after_scheme: &str) -> Address {
    let authority_and_path = after_scheme.trim_start_matches("//");
    let (authority, path) = match authority_and_path.split_once('/') {
        Some((a, p)) => (a, p),
        None => (authority_and_path, ""),
    };
    let domain = authority.split(['@', ':']).next_back().unwrap_or(authority);

    match RepoHost::of_domain(domain) {
        Some(host) => match repo_from_path(host, path) {
            Some(repo) => Address::Repo(Repo {
                source: Some(url.to_owned()),
                ..repo
            }),
            None => Address::Web(url.to_owned()),
        },
        None => Address::Web(url.to_owned()),
    }
}

/// `owner/repo`, `owner/repo/blob/<ref>/путь`, `owner/repo/tree/<ref>/путь`,
/// а у gitlab — ещё и подгруппы: `group/sub/project/-/blob/<ref>/путь`.
fn repo_from_path(host: RepoHost, path: &str) -> Option<Repo> {
    let path = path.split(['?', '#']).next().unwrap_or(path);

    // У gitlab проект живёт в подгруппах любой глубины, и от пути внутри
    // проекта его отделяет `/-/`. У github такого разделителя нет: там
    // всегда ровно owner и repo, а дальше уже `blob` или `tree`.
    let (owner, name, rest) = match host {
        RepoHost::GitLab => {
            let (project, tail) = path.split_once("/-/").unwrap_or((path, ""));
            let mut segments: Vec<&str> = project.split('/').filter(|p| !p.is_empty()).collect();
            let name = segments.pop()?;
            if segments.is_empty() {
                return None;
            }
            (segments.join("/"), name.to_owned(), tail)
        }
        RepoHost::GitHub => {
            let mut segments = path.splitn(3, '/').filter(|p| !p.is_empty());
            let owner = segments.next()?;
            let name = segments.next()?;
            (
                owner.to_owned(),
                name.to_owned(),
                segments.next().unwrap_or(""),
            )
        }
    };

    let rest: Vec<&str> = rest.split('/').filter(|p| !p.is_empty()).collect();
    // `blob` — файл, `tree` — каталог. Хостинг различает их сам, и в этом
    // месте у нас единственный способ узнать, чего просят.
    let listing = matches!(rest.first().copied(), Some("tree"));
    let inner = match rest.first().copied() {
        None => None,
        // Ветку из адреса выбрасываем: имя ветки — деталь хостинга,
        // а читателю нужен файл. `HEAD` хостинг развернёт сам.
        Some("blob" | "tree" | "raw") => {
            let tail = rest.get(2..).unwrap_or_default();
            (!tail.is_empty()).then(|| tail.join("/"))
        }
        // issues, pulls, settings, releases — это не документация.
        Some(_) => return None,
    };

    Some(Repo {
        host,
        owner,
        name,
        listing,
        path: inner,
        source: None,
    })
}

fn split_scheme(text: &str) -> Option<(&str, &str)> {
    let (scheme, rest) = text.split_once(':')?;
    let valid = !scheme.is_empty()
        && scheme
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'));
    valid.then_some((scheme, rest))
}

fn looks_like_path(text: &str) -> bool {
    text.starts_with('/')
        || text.starts_with("./")
        || text.starts_with("../")
        || text.starts_with('~')
        || (FILE_KINDS.iter().any(|kind| text.ends_with(kind))
            && !text.contains(' ')
            && PathBuf::from(text).exists())
}

/// Что открываем файлом, если его напечатали без пути: markdown и ленты.
/// Имя вида `feed.xml` без проверки на диске было бы доменом.
const FILE_KINDS: &[&str] = &[".md", ".rss", ".atom", ".xml", ".json"];

#[cfg(test)]
mod tests {

    #[test]
    fn a_feed_link_is_the_address_of_a_feed() {
        assert_eq!(
            parse("feed://example.org/rss.xml").unwrap(),
            Address::Web("https://example.org/rss.xml".to_owned())
        );
        assert_eq!(
            parse("feed:https://example.org/atom").unwrap(),
            Address::Web("https://example.org/atom".to_owned())
        );
        assert_eq!(
            parse("feed:http://example.org/atom").unwrap(),
            Address::Web("http://example.org/atom".to_owned())
        );
        assert!(matches!(parse("feed:nonsense"), Err(Error::BadUrl(_))));
    }

    #[test]
    fn our_own_page_has_an_address_like_everything_else() {
        // Внутренняя страница обязана разбираться обратно: она лежит
        // в истории вкладки и печатается в адресной строке, а значит
        // однажды приедет в `parse` из них же.
        let history = Address::Internal(Internal::History);
        assert_eq!(history.display(), "brevier:history");
        assert_eq!(parse("brevier:history").unwrap(), history);
        assert_eq!(parse("brevier://history/").unwrap(), history);
        // Снаружи её открыть нечем, и выдумывать адрес для этого не станем.
        assert!(history.external().is_empty());
        assert_eq!(
            parse("brevier:bookmarks").unwrap(),
            Address::Internal(Internal::Bookmarks)
        );
        assert!(matches!(parse("brevier:nowhere"), Err(Error::BadUrl(_))));
    }

    /// Архив (#8): список, поиск со словами через пробел или процентными
    /// кодами, копия по пути. Путь наверх адресом не является.
    #[test]
    fn the_archive_has_addresses_for_its_list_its_search_and_its_copies() {
        let list = Address::Internal(Internal::Archive(ArchivePage::List));
        assert_eq!(parse("brevier:archive").unwrap(), list);
        assert_eq!(list.display(), "brevier:archive");
        // Пустой запрос — тот же список.
        assert_eq!(parse("brevier:archive?q= ").unwrap(), list);

        let search = parse("brevier:archive?q=borrow checker").unwrap();
        assert_eq!(
            search,
            Address::Internal(Internal::Archive(ArchivePage::Search(
                "borrow checker".to_owned()
            )))
        );
        assert_eq!(parse(&search.display()).unwrap(), search);
        // Из ссылки — процентными кодами; плюс остаётся плюсом.
        assert_eq!(
            parse("brevier:archive?q=%D0%BA%D0%BE%D1%82%20c++").unwrap(),
            Address::Internal(Internal::Archive(ArchivePage::Search("кот c++".to_owned())))
        );

        let copy = parse("brevier:archive/danluu.com/2026-10-09-keyboard-latency.md.lz4").unwrap();
        assert_eq!(
            copy,
            Address::Internal(Internal::Archive(ArchivePage::Copy(
                "danluu.com/2026-10-09-keyboard-latency.md.lz4".to_owned()
            )))
        );
        assert_eq!(parse(&copy.display()).unwrap(), copy);
        assert!(copy.external().is_empty());
        assert!(parse("brevier:archive/../history.tsv").is_err());
        assert!(parse("brevier:archive/../../x.md.lz4").is_err());
    }

    /// Снимок Wayback (#9) — адрес страницы внутри своего; снаружи —
    /// календарь снимков.
    #[test]
    fn a_wayback_address_carries_the_page() {
        let wayback = parse("brevier:wayback/https://example.com/a?b=c").unwrap();
        assert_eq!(
            wayback,
            Address::Internal(Internal::Wayback("https://example.com/a?b=c".to_owned()))
        );
        assert_eq!(parse(&wayback.display()).unwrap(), wayback);
        assert_eq!(
            wayback.external(),
            "https://web.archive.org/web/*/https://example.com/a?b=c"
        );
        assert!(parse("brevier:wayback/gh:o/n").is_err());
        assert!(parse("brevier:wayback").is_err());
    }

    #[test]
    fn a_check_carries_the_page_it_checks() {
        let check = parse("brevier:check/https://example.com/a?b=c#d").unwrap();
        assert_eq!(
            check,
            Address::Internal(Internal::Check(Some(
                "https://example.com/a?b=c#d".to_owned()
            )))
        );
        // Разбирается обратно из того, что показано в строке.
        assert_eq!(parse(&check.display()).unwrap(), check);
        // Снаружи ей соответствует проверяемая страница.
        assert_eq!(check.external(), "https://example.com/a?b=c#d");
        // Голый домен — как в адресной строке.
        assert_eq!(
            parse("brevier:check/example.com").unwrap().display(),
            "brevier:check/https://example.com"
        );
        // Без адреса — страница о том, как проверять.
        assert_eq!(
            parse("brevier:check").unwrap(),
            Address::Internal(Internal::Check(None))
        );
        // Проверяют веб: репозиторий и опечатка — не адрес проверки.
        assert!(parse("brevier:check/gh:o/n").is_err());
        assert!(parse("brevier:checks").is_err());

        // Адрес проверки берётся у страницы, а у проверки — она сама.
        let page = parse("https://example.com/a").unwrap();
        let check = page.check().unwrap();
        assert_eq!(check.display(), "brevier:check/https://example.com/a");
        assert_eq!(check.check(), Some(check.clone()));
        assert!(parse("gh:o/n").unwrap().check().is_none());
    }

    #[test]
    fn a_trailing_slash_asks_for_the_directory_itself() {
        // Правило одно на любую глубину: черта на конце — каталог.
        let Address::Repo(repo) = parse("gh:o/n/docs/").unwrap() else {
            panic!("не репозиторий");
        };
        assert!(repo.listing);
        assert_eq!(repo.path.as_deref(), Some("docs"));
        assert_eq!(repo.display_for_test(), "gh:o/n/docs/");

        let Address::Repo(root) = parse("gh:o/n/").unwrap() else {
            panic!("не репозиторий");
        };
        assert!(root.listing && root.path.is_none());

        // А без черты ждут README — и каталога, и репозитория.
        let Address::Repo(readme) = parse("gh:o/n/docs").unwrap() else {
            panic!("не репозиторий");
        };
        assert!(!readme.listing);
    }

    #[test]
    fn a_directory_address_survives_the_round_trip() {
        // Адрес каталога, напечатанный нами, обязан разобраться обратно
        // в тот же каталог: на этом свойстве держится хождение по листингу.
        for host in [RepoHost::GitHub, RepoHost::GitLab] {
            let source = Repo {
                host,
                owner: "o".to_owned(),
                name: "n".to_owned(),
                path: Some("crates/cli".to_owned()),
                listing: true,
                source: None,
            };
            let Address::Repo(back) = parse(&source.web_url()).unwrap() else {
                panic!("не репозиторий");
            };
            assert!(back.listing, "{:?}", source.web_url());
            assert_eq!(back.path.as_deref(), Some("crates/cli"));

            // И короткая форма тоже.
            let Address::Repo(short) = parse(&source.display_for_test()).unwrap() else {
                panic!("не репозиторий");
            };
            assert!(short.listing);
            assert_eq!(short.path.as_deref(), Some("crates/cli"));
        }
    }
    use super::*;

    fn repo(input: &str) -> Repo {
        match parse(input).expect("должно разобраться") {
            Address::Repo(repo) => repo,
            other => panic!("ожидался репозиторий, вышло {other:?}"),
        }
    }

    #[test]
    fn plain_url_is_a_web_page() {
        assert_eq!(
            parse("https://danluu.com/keyboard-latency/").unwrap(),
            Address::Web("https://danluu.com/keyboard-latency/".to_owned())
        );
    }

    #[test]
    fn bare_domain_gets_https() {
        assert_eq!(
            parse("danluu.com/keyboard-latency/").unwrap(),
            Address::Web("https://danluu.com/keyboard-latency/".to_owned())
        );
    }

    #[test]
    fn shorthand_is_a_repository() {
        let repo = repo("gh:rust-lang/rust");
        assert_eq!(repo.host, RepoHost::GitHub);
        assert_eq!(
            (repo.owner.as_str(), repo.name.as_str()),
            ("rust-lang", "rust")
        );
        assert_eq!(repo.path, None);
    }

    #[test]
    fn pasted_github_url_switches_to_repository_mode() {
        let repo = repo("https://github.com/rust-lang/rust/blob/master/README.md");
        assert_eq!(repo.path.as_deref(), Some("README.md"));
        // Пока режима репозитория нет, читаем ровно ту же страницу вебом —
        // корпус на этой ссылке не должен заметить разницы.
        assert_eq!(
            repo.web_url(),
            "https://github.com/rust-lang/rust/blob/master/README.md"
        );
    }

    #[test]
    fn gitlab_subgroups_are_part_of_the_project_path() {
        // У gitlab проект живёт в подгруппах любой глубины; отделяет их
        // от пути внутри проекта `/-/`, которого у github нет вовсе.
        let repo = repo("https://gitlab.com/gnome/world/podcasts/-/blob/main/README.md");
        assert_eq!(repo.host, RepoHost::GitLab);
        assert_eq!(repo.owner, "gnome/world");
        assert_eq!(repo.name, "podcasts");
        assert_eq!(repo.path.as_deref(), Some("README.md"));
    }

    #[test]
    fn a_rewritten_link_parses_back_into_the_same_repository() {
        // Ссылки внутри документа мы разворачиваем в адреса хостинга.
        // Если разбор их не узнаёт, читатель на первой же внутренней
        // ссылке вываливается из режима репозитория в веб.
        for host in [RepoHost::GitHub, RepoHost::GitLab] {
            let source = Repo {
                host,
                owner: "o".to_owned(),
                name: "n".to_owned(),
                path: None,
                listing: false,
                source: None,
            };
            let back = repo(&source.blob_url("docs/guide.md"));
            assert_eq!(back.host, host);
            assert_eq!(back.owner, "o");
            assert_eq!(back.name, "n");
            assert_eq!(back.path.as_deref(), Some("docs/guide.md"));
        }
    }

    #[test]
    fn github_pages_that_are_not_repositories_stay_web() {
        assert!(matches!(
            parse("https://github.com/rust-lang/rust/issues/12345").unwrap(),
            Address::Web(_)
        ));
        assert!(matches!(
            parse("https://github.com/settings").unwrap(),
            Address::Web(_)
        ));
    }

    #[test]
    fn local_paths_are_files() {
        assert_eq!(
            parse("./TODO.md").unwrap(),
            Address::File(PathBuf::from("./TODO.md"))
        );
        assert_eq!(
            parse("/home/reader/notes.md").unwrap(),
            Address::File(PathBuf::from("/home/reader/notes.md"))
        );
    }

    #[test]
    fn other_schemes_are_refused_by_name() {
        assert!(matches!(
            parse("gemini://example.org/"),
            Err(Error::UnsupportedScheme(s)) if s == "gemini"
        ));
    }

    #[test]
    fn nothing_is_not_an_address() {
        assert!(parse("").is_err());
        assert!(parse("   ").is_err());
        assert!(parse("?").is_err());
    }

    /// Не адрес — запрос (#25).
    #[test]
    fn words_are_a_search() {
        let searched = |text: &str| match parse(text) {
            Ok(Address::Web(url)) => crate::hosts::search_query(&url),
            _ => None,
        };
        assert_eq!(searched("что почитать").as_deref(), Some("что почитать"));
        assert_eq!(searched("rust").as_deref(), Some("rust"));
        assert_eq!(
            searched("rust: borrow checker").as_deref(),
            Some("rust: borrow checker")
        );
        assert_eq!(searched("3.14").as_deref(), Some("3.14"));
        assert_eq!(searched("e.g.").as_deref(), Some("e.g."));
        assert_eq!(searched("what is rust?").as_deref(), Some("what is rust?"));
        // `?` впереди ищет и то, что само похоже на адрес.
        assert_eq!(searched("?danluu.com").as_deref(), Some("danluu.com"));
        assert_eq!(searched(" ? gh:o/n ").as_deref(), Some("gh:o/n"));
    }

    #[test]
    fn addresses_are_still_addresses() {
        let web = |text: &str| match parse(text) {
            Ok(Address::Web(url)) => url,
            other => panic!("{text}: {other:?}"),
        };
        assert_eq!(web("danluu.com"), "https://danluu.com");
        assert_eq!(
            web("ru.wikipedia.org/wiki/Rust"),
            "https://ru.wikipedia.org/wiki/Rust"
        );
        assert_eq!(web("1.1.1.1"), "https://1.1.1.1");
        assert_eq!(web("localhost"), "http://localhost");
        assert_eq!(web("localhost:8080/notes"), "http://localhost:8080/notes");
        assert_eq!(web("nas:5000"), "http://nas:5000");
        assert_eq!(web("пример.рф"), "https://пример.рф");
        assert!(matches!(
            parse("mailto:reader@example.org"),
            Err(Error::UnsupportedScheme(s)) if s == "mailto"
        ));
        assert_eq!(
            parse("~/My Notes.md").unwrap(),
            Address::File(PathBuf::from("~/My Notes.md"))
        );
    }
}
