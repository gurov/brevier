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
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Repo {
    pub host: RepoHost,
    pub owner: String,
    pub name: String,
    /// Путь внутри репозитория, если он был в адресе.
    pub path: Option<String>,
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
        }
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
                    Some(path) => format!("{prefix}:{}/{}/{path}", repo.owner, repo.name),
                    None => format!("{prefix}:{}/{}", repo.owner, repo.name),
                }
            }
            Address::File(path) => path.display().to_string(),
        }
    }
}

/// Разобрать то, что напечатано в адресной строке.
pub fn parse(input: &str) -> Result<Address, Error> {
    let text = input.trim();
    if text.is_empty() {
        return Err(Error::BadUrl(String::new()));
    }

    if let Some(rest) = text.strip_prefix("gh:") {
        return shorthand(RepoHost::GitHub, rest);
    }
    if let Some(rest) = text.strip_prefix("gl:") {
        return shorthand(RepoHost::GitLab, rest);
    }

    if let Some((scheme, rest)) = split_scheme(text) {
        return match scheme {
            "http" | "https" => Ok(from_url(text, rest)),
            "file" => Ok(Address::File(PathBuf::from(
                rest.trim_start_matches("//").to_owned(),
            ))),
            other => Err(Error::UnsupportedScheme(other.to_owned())),
        };
    }

    if looks_like_path(text) {
        return Ok(Address::File(PathBuf::from(text)));
    }

    // Голый домен: человек печатает «danluu.com», а не «https://danluu.com».
    if text.contains('.') && !text.contains(char::is_whitespace) {
        let url = format!("https://{text}");
        return Ok(from_url(&url, text));
    }

    Err(Error::BadUrl(text.to_owned()))
}

fn shorthand(host: RepoHost, rest: &str) -> Result<Address, Error> {
    let mut parts = rest.trim_matches('/').splitn(3, '/');
    let owner = parts.next().unwrap_or_default();
    let name = parts.next().unwrap_or_default();
    if owner.is_empty() || name.is_empty() {
        return Err(Error::BadUrl(rest.to_owned()));
    }
    Ok(Address::Repo(Repo {
        host,
        owner: owner.to_owned(),
        name: name.to_owned(),
        path: parts.next().filter(|p| !p.is_empty()).map(str::to_owned),
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
        || (text.ends_with(".md") && !text.contains(' ') && PathBuf::from(text).exists())
}

#[cfg(test)]
mod tests {
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
    fn nonsense_is_not_an_address() {
        assert!(parse("").is_err());
        assert!(parse("что почитать").is_err());
    }
}
