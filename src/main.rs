//! Brevier — M0. Одна строчка, которую этот этап обязан отработать:
//!
//!     brevier https://example.com/article | less

mod error;
mod extract;
mod fetch;
mod markdown;

use std::io::{self, Write};
use std::process::ExitCode;

use error::Error;
use fetch::{ContentKind, UserAgent};

const HELP: &str = "\
brevier — a JavaScript-free reader: fetches a page, extracts the article,
prints it as Markdown (CommonMark + GFM).

Usage: brevier [options] <url>

Options:
      --ua <honest|browser>  User-Agent to send (default: honest)
      --raw                  skip extraction, convert the whole page
      --html                 print the extracted HTML instead of Markdown
      --links                print the article's outgoing links, one per line
  -h, --help                 this text
  -V, --version              version

Exit codes: 1 bad url, 2 network, 3 http status,
            4 content type, 5 nothing extracted, 6 conversion
";

struct Args {
    url: String,
    ua: UserAgent,
    raw: bool,
    html: bool,
    links: bool,
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

    // rustls собран без встроенного провайдера, выбираем явно.
    let _ = rustls::crypto::ring::default_provider().install_default();

    match run(&args) {
        Ok(text) => out(&text),
        Err(e) => {
            eprintln!("brevier: {e}");
            ExitCode::from(e.exit_code())
        }
    }
}

fn run(args: &Args) -> Result<String, Error> {
    let page = fetch::fetch(&args.url, args.ua)?;

    let text = match page.kind {
        // Родной формат: конвертировать нечего, трогать текст автора — тем более.
        ContentKind::Markdown => page.body,
        // Простой текст markdown-ом не является — отдаём как есть.
        ContentKind::Text => page.body,
        ContentKind::Html if args.raw => markdown::from_html(&page.body)?,
        ContentKind::Html => {
            let article = extract::extract(&page.body, &page.url)?;
            if args.html {
                return Ok(article.content_html);
            }
            markdown::from_article(&article)?
        }
    };

    if args.links {
        let mut out = markdown::links(&text).join("\n");
        out.push('\n');
        return Ok(out);
    }
    Ok(text)
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
    let mut raw = false;
    let mut html = false;
    let mut links = false;

    let mut args = args.peekable();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" | "--help" => return Parsed::Print(HELP.to_owned()),
            "-V" | "--version" => {
                return Parsed::Print(format!("brevier {}\n", env!("CARGO_PKG_VERSION")));
            }
            "--raw" => raw = true,
            "--html" => html = true,
            "--links" => links = true,
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

    match url {
        Some(url) => Parsed::Run(Box::new(Args {
            url,
            ua,
            raw,
            html,
            links,
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
    fn url_is_required() {
        assert!(matches!(parse(&["--raw"]), Parsed::Usage(_)));
        assert!(matches!(
            parse(&["--ua=nonsense", "https://e.com"]),
            Parsed::Usage(_)
        ));
    }
}
