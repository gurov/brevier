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
    /// Тот же репозиторий обычной ссылкой. Пока режима репозитория нет,
    /// по ней и открываем — это честная деградация, а не заглушка.
    ///
    /// Вставленную ссылку возвращаем как есть: собрать её заново нельзя,
    /// потому что путь к файлу на хостинге идёт через `blob/<ветка>`,
    /// а ветку мы выбросили — она деталь хостинга, а не адрес документа.
    /// Короткая форма с путём до M2 открывает корень репозитория.
    pub fn web_url(&self) -> String {
        match &self.source {
            Some(url) => url.clone(),
            None => format!("https://{}/{}/{}", self.host.domain(), self.owner, self.name),
        }
    }
}

impl Address {
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

/// `owner/repo`, `owner/repo/blob/<ref>/путь`, `owner/repo/tree/<ref>/путь`.
fn repo_from_path(host: RepoHost, path: &str) -> Option<Repo> {
    let path = path.split(['?', '#']).next().unwrap_or(path);
    let mut parts = path.split('/').filter(|p| !p.is_empty());
    let owner = parts.next()?.to_owned();
    let name = parts.next()?.to_owned();

    let inner = match parts.next() {
        // Ветка в адресе нам не нужна: имя ветки — деталь хостинга, а читателю
        // нужен файл. На M2 ветку возьмём из самого репозитория.
        Some("blob" | "tree") => {
            let _branch = parts.next();
            let rest: Vec<&str> = parts.collect();
            (!rest.is_empty()).then(|| rest.join("/"))
        }
        Some(_) => return None,
        None => None,
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
        assert_eq!((repo.owner.as_str(), repo.name.as_str()), ("rust-lang", "rust"));
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
