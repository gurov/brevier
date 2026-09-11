//! Brevier — M0. Одна строчка, которую этот этап обязан отработать:
//!
//!     brevier https://example.com/article | less

use std::io::{self, Read, Write};
use std::process::ExitCode;

use brevier::error::Error;
use brevier::fetch::{ContentKind, UserAgent};
use brevier::{Address, address, extract, fetch, markdown};

const HELP: &str = "\
brevier — a JavaScript-free reader: fetches a page, extracts the article,
prints it as Markdown (CommonMark + GFM).

Usage: brevier [options] <url|gh:owner/repo|gl:owner/repo>

Options:
      --ua <honest|browser>  User-Agent to send (default: honest)
      --stdin                read the page's HTML from stdin instead of
                             fetching it; the url tells where it came from
      --raw                  skip extraction, convert the whole page
      --html                 print the extracted HTML instead of Markdown
      --links                print the article's outgoing links, one per line
      --docs                 for a repository: entry points into its
                             documentation, one per line
      --save                 write the article to a file instead of stdout;
                             `.md` is the text, `.zip` the text plus its
                             images (needs the `save` feature)
  -o, --output <path>        where --save puts it; without it the name comes
                             from the article's title
  -h, --help                 this text
  -V, --version              version

Exit codes: 1 bad url, 2 network, 3 http status,
            4 content type, 5 nothing extracted, 6 conversion
";

struct Args {
    url: String,
    ua: UserAgent,
    /// HTML приходит из stdin, а не из сети. Адрес при этом всё равно нужен:
    /// по нему разворачиваются относительные ссылки.
    stdin: bool,
    raw: bool,
    html: bool,
    links: bool,
    docs: bool,
    /// Статья ложится на диск, а не в stdout.
    save: bool,
    /// Куда именно. Выбор читателя старше нашего предложения — и здесь,
    /// и в диалоге окна. В сборке без `save` поле не читает никто, но разбор
    /// аргументов один на обе: флаг должен внятно отвечать и там, где
    /// сохранения нет.
    #[cfg_attr(not(feature = "save"), allow(dead_code))]
    output: Option<String>,
}

enum Parsed {
    Run(Box<Args>),
    /// `--help` / `--version`: печатаем и выходим с нулём.
    Print(String),
    Usage(String),
}

fn main() -> ExitCode {
    let args = match parse_args(std::env::args().skip(1)) {
        Parsed::Run(args) => args,
        Parsed::Print(text) => return out(&text),
        Parsed::Usage(message) => {
            eprintln!("brevier: {message}\n\n{HELP}");
            return ExitCode::from(1);
        }
    };

    brevier::init_crypto();

    if args.save {
        return save_page(&args);
    }

    match run(&args) {
        Ok(text) => out(&text),
        Err(e) => {
            eprintln!("brevier: {e}");
            ExitCode::from(e.exit_code())
        }
    }
}

/// `--save`: статья ложится на диск, а не в stdout. Всю работу делает ядро
/// (`save::write`), сюда достаётся только выбор имени.
#[cfg(feature = "save")]
fn save_page(args: &Args) -> ExitCode {
    use std::path::PathBuf;

    let document = match document(args) {
        Ok(document) => document,
        Err(e) => {
            eprintln!("brevier: {e}");
            return ExitCode::from(e.exit_code());
        }
    };

    let path = match args.output.as_deref() {
        Some(path) => PathBuf::from(path),
        None => {
            let name = PathBuf::from(brevier::save::suggested_name(&document));
            // Имя придумали мы, а на диске уже что-то лежит: затирать чужой
            // файл молча нельзя. С `-o` такого вопроса нет — там выбрал читатель.
            if name.exists() {
                eprintln!(
                    "brevier: {} is already here; pass -o <path> to choose another name",
                    name.display()
                );
                return ExitCode::from(1);
            }
            name
        }
    };

    match brevier::save::write(&path, &document, args.ua) {
        Ok(saved) => {
            if saved.missed > 0 {
                let images = if saved.missed == 1 { "image" } else { "images" };
                eprintln!("brevier: {} {images} did not come", saved.missed);
            }
            // В stdout — только путь: по нему сохранённое подхватывает
            // следующая команда в конвейере.
            out(&format!("{}\n", saved.path.display()))
        }
        Err(e) => {
            eprintln!("brevier: {e}");
            ExitCode::from(e.exit_code())
        }
    }
}

/// Сборка без `save` — в ней и zip нет. Молчать об этом нельзя: читатель
/// просил файл, а файла не будет.
#[cfg(not(feature = "save"))]
fn save_page(_args: &Args) -> ExitCode {
    eprintln!("brevier: this build cannot save; rebuild with `--features save`");
    ExitCode::from(1)
}

/// Документ целиком — то же, что показывает окно. Тракт тот же, что у печати:
/// страница из сети, репозиторий или уже готовый HTML из stdin.
#[cfg(feature = "save")]
fn document(args: &Args) -> Result<brevier::Document, Error> {
    if args.stdin {
        let mut html = String::new();
        io::stdin()
            .read_to_string(&mut html)
            .map_err(Error::Convert)?;
        return brevier::from_html(&html, &args.url);
    }
    brevier::open(&address::parse(&args.url)?, args.ua)
}

fn run(args: &Args) -> Result<String, Error> {
    // Точки входа в документацию — вопрос к репозиторию, а не к документу:
    // отвечаем на него до того, как что-то скачано и разобрано.
    if args.docs {
        let Some(Address::Repo(repo)) = repository(args) else {
            return Err(Error::BadUrl(format!("{} is not a repository", args.url)));
        };
        let mut out = String::new();
        for entry in brevier::repo::documentation(&repo, args.ua) {
            out.push_str(&format!("{}\t{}\n", entry.path, entry.title));
        }
        return Ok(out);
    }

    let text = if args.stdin {
        let mut html = String::new();
        io::stdin()
            .read_to_string(&mut html)
            .map_err(Error::Convert)?;
        page_text(&html, &args.url, args)?
    } else {
        match repository(args) {
            // Репозиторий читается своим трактом: конвертировать нечего,
            // формат родной. Работа там в другом — развернуть ссылки внутри
            // документа, которых в сыром `.md` нет.
            Some(address) => {
                let document = brevier::open(&address, args.ua)?;
                // Каталог без README приезжает списком ссылок. Говорим
                // об этом той же строкой и туда же, что и на вебе:
                // в stdout идёт только документ.
                if document.kind == markdown::Kind::Listing {
                    eprintln!("brevier: a list of links, not an article");
                }
                document.markdown
            }
            None => {
                let page = fetch::fetch(&args.url, args.ua)?;

                match page.kind {
                    // Родной формат: конвертировать нечего, трогать текст автора — тем более.
                    ContentKind::Markdown => page.body,
                    // Простой текст markdown-ом не является — отдаём как есть.
                    ContentKind::Text => page.body,
                    ContentKind::Html => page_text(&page.body, &page.url, args)?,
                }
            }
        }
    };

    if args.links {
        let mut out = markdown::links(&text).join("\n");
        out.push('\n');
        return Ok(out);
    }
    Ok(text)
}

/// Страница в том виде, в каком её печатают: тракт один и для скачанного
/// тела, и для поданного в stdin. Адрес нужен и без сети — по нему
/// разворачиваются относительные ссылки и решается, что тут за документ.
fn page_text(html: &str, url: &str, args: &Args) -> Result<String, Error> {
    if args.raw {
        return markdown::from_html(html);
    }

    let article = extract::extract(html, url)?;
    if args.html {
        return Ok(article.content_html);
    }

    let reading = markdown::from_article(&article)?;
    // Сказать, что это не статья, надо так, чтобы не испортить `| less`
    // и перенаправление в файл: в stdout идёт только документ.
    if reading.kind == markdown::Kind::Listing {
        eprintln!("brevier: a list of links, not an article");
    }
    Ok(reading.markdown)
}

/// Адрес репозитория — или `None`, если это обычная страница. `--raw`
/// и `--html` спрашивают про извлечение из веба, которого в репозитории
/// нет вовсе; при них тракт всегда веб-овый.
fn repository(args: &Args) -> Option<Address> {
    if args.raw || args.html {
        return None;
    }
    match address::parse(&args.url) {
        Ok(address @ Address::Repo(_)) => Some(address),
        _ => None,
    }
}

/// Печать с оглядкой на `| less`: если пейджер закрыли раньше, чем мы дописали,
/// это не ошибка, а нормальный конец чтения.
fn out(text: &str) -> ExitCode {
    let mut stdout = io::stdout();
    match stdout
        .write_all(text.as_bytes())
        .and_then(|()| stdout.flush())
    {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) if e.kind() == io::ErrorKind::BrokenPipe => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("brevier: {e}");
            ExitCode::from(7)
        }
    }
}

fn parse_args(args: impl Iterator<Item = String>) -> Parsed {
    let mut url: Option<String> = None;
    let mut ua = UserAgent::default();
    let mut stdin = false;
    let mut raw = false;
    let mut html = false;
    let mut links = false;
    let mut docs = false;
    let mut save = false;
    let mut output: Option<String> = None;

    let mut args = args.peekable();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" | "--help" => return Parsed::Print(HELP.to_owned()),
            "-V" | "--version" => {
                return Parsed::Print(format!("brevier {}\n", env!("CARGO_PKG_VERSION")));
            }
            "--stdin" => stdin = true,
            "--raw" => raw = true,
            "--html" => html = true,
            "--links" => links = true,
            "--docs" => docs = true,
            "--save" => save = true,
            "-o" | "--output" => match args.next() {
                Some(path) => output = Some(path),
                None => return Parsed::Usage("-o needs a path".to_owned()),
            },
            _ if arg.starts_with("--output=") => {
                output = Some(arg["--output=".len()..].to_owned());
            }
            "--ua" => match args.next() {
                Some(value) => match UserAgent::parse(&value) {
                    Some(parsed) => ua = parsed,
                    None => return Parsed::Usage(format!("unknown user agent `{value}`")),
                },
                None => return Parsed::Usage("--ua needs a value".to_owned()),
            },
            _ if arg.starts_with("--ua=") => match UserAgent::parse(&arg["--ua=".len()..]) {
                Some(parsed) => ua = parsed,
                None => return Parsed::Usage(format!("unknown user agent `{arg}`")),
            },
            _ if arg.starts_with('-') && arg != "-" => {
                return Parsed::Usage(format!("unknown option `{arg}`"));
            }
            _ if url.is_none() => url = Some(arg),
            _ => return Parsed::Usage(format!("only one url at a time, got `{arg}` too")),
        }
    }

    // Точки входа ищутся в репозитории, а в stdin приходит страница:
    // вопросы разные, и молча предпочесть один другому нельзя.
    if stdin && docs {
        return Parsed::Usage("--docs asks a repository, not stdin".to_owned());
    }
    // Сохранение отдаёт статью, а эти флаги спрашивают про другое — что
    // внутри страницы до конвертации, куда она ведёт, что в репозитории.
    // Молча предпочесть одно другому нельзя.
    if save && (raw || html || links || docs) {
        return Parsed::Usage(
            "--save writes the article; --raw, --html, --links and --docs ask other questions"
                .to_owned(),
        );
    }
    if output.is_some() && !save {
        return Parsed::Usage(
            "-o says where to write, but nothing is being written: add --save".to_owned(),
        );
    }

    match url {
        // Адрес обязателен и при `--stdin`: сеть он не трогает, но без него
        // относительные ссылки страницы разворачивать не во что.
        Some(url) => Parsed::Run(Box::new(Args {
            url,
            ua,
            stdin,
            raw,
            html,
            links,
            docs,
            save,
            output,
        })),
        None => Parsed::Usage("no url given".to_owned()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Parsed {
        parse_args(args.iter().map(|s| (*s).to_owned()))
    }

    #[test]
    fn url_and_flags() {
        let Parsed::Run(args) = parse(&["--ua", "browser", "--raw", "https://e.com/a"]) else {
            panic!("не разобралось");
        };
        assert_eq!(args.url, "https://e.com/a");
        assert_eq!(args.ua, UserAgent::Browser);
        assert!(args.raw && !args.html);
    }

    #[test]
    fn ua_defaults_to_honest() {
        let Parsed::Run(args) = parse(&["https://e.com/a"]) else {
            panic!("не разобралось");
        };
        assert_eq!(args.ua, UserAgent::Honest);
    }

    #[test]
    fn html_comes_from_stdin_but_the_url_is_still_needed() {
        let Parsed::Run(args) = parse(&["--stdin", "https://e.com/a"]) else {
            panic!("не разобралось");
        };
        assert!(args.stdin);
        assert_eq!(args.url, "https://e.com/a");

        // Адрес нужен и здесь: по нему разворачиваются ссылки страницы.
        assert!(matches!(parse(&["--stdin"]), Parsed::Usage(_)));
        // А точки входа в документацию спрашивают репозиторий, не страницу.
        assert!(matches!(
            parse(&["--stdin", "--docs", "gh:o/n"]),
            Parsed::Usage(_)
        ));
    }

    #[test]
    fn saving_takes_a_place_to_write() {
        let Parsed::Run(args) = parse(&["--save", "-o", "/tmp/a.zip", "https://e.com/a"]) else {
            panic!("не разобралось");
        };
        assert!(args.save);
        assert_eq!(args.output.as_deref(), Some("/tmp/a.zip"));

        let Parsed::Run(args) = parse(&["--save", "--output=a.md", "https://e.com/a"]) else {
            panic!("не разобралось");
        };
        assert_eq!(args.output.as_deref(), Some("a.md"));

        // Без `-o` имя берётся из заголовка статьи — это не ошибка.
        let Parsed::Run(args) = parse(&["--save", "https://e.com/a"]) else {
            panic!("не разобралось");
        };
        assert!(args.save && args.output.is_none());
    }

    #[test]
    fn saving_answers_a_different_question_than_the_printing_flags() {
        assert!(matches!(
            parse(&["--save", "--html", "https://e.com/a"]),
            Parsed::Usage(_)
        ));
        assert!(matches!(
            parse(&["--save", "--docs", "gh:o/n"]),
            Parsed::Usage(_)
        ));
        // Место для записи без самой записи — тоже недоразумение.
        assert!(matches!(
            parse(&["-o", "a.md", "https://e.com/a"]),
            Parsed::Usage(_)
        ));
        assert!(matches!(parse(&["--save", "-o"]), Parsed::Usage(_)));
    }

    #[test]
    fn url_is_required() {
        assert!(matches!(parse(&["--raw"]), Parsed::Usage(_)));
        assert!(matches!(
            parse(&["--ua=nonsense", "https://e.com"]),
            Parsed::Usage(_)
        ));
    }
}
