//! Тракт «сеть → content-type → вывод» целиком, на живом сокете.
//!
//! Юнит-тесты проверяют конвертацию, но не то, ради чего затевался M0:
//! что придёт по проводу и как мы это разберём. Сервер поднимается на
//! localhost, ответы заготовлены, сеть наружу не нужна.

use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::process::{Command, Output};
use std::thread;

struct Route {
    path: &'static str,
    content_type: &'static str,
    body: Vec<u8>,
}

fn route(path: &'static str, content_type: &'static str, body: &str) -> Route {
    Route {
        path,
        content_type,
        body: body.as_bytes().to_vec(),
    }
}

/// Поднимает сервер на свободном порту и возвращает его базовый адрес.
/// Поток фоновый: живёт до конца процесса тестов.
fn serve(routes: Vec<Route>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("порт");
    let base = format!("http://{}", listener.local_addr().unwrap());

    thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(stream) = stream else { continue };
            answer(stream, &routes);
        }
    });

    base
}

fn answer(mut stream: TcpStream, routes: &[Route]) {
    let mut request = String::new();
    if BufReader::new(&stream).read_line(&mut request).is_err() {
        return;
    }
    let path = request.split_whitespace().nth(1).unwrap_or("/").to_owned();

    let response = match routes.iter().find(|r| r.path == path) {
        // Переезд: тело маршрута — куда.
        Some(route) if route.content_type == REDIRECT => format!(
            "HTTP/1.1 301 Moved Permanently\r\nLocation: {}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            String::from_utf8_lossy(&route.body)
        )
        .into_bytes(),
        Some(route) => {
            let mut head = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                route.content_type,
                route.body.len()
            )
            .into_bytes();
            head.extend_from_slice(&route.body);
            head
        }
        None => {
            b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec()
        }
    };

    let _ = stream.write_all(&response);
}

/// Вместо типа содержимого у маршрута: ответить 301 на адрес из тела.
const REDIRECT: &str = "redirect";

fn brevier(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_brevier"))
        .args(args)
        .output()
        .expect("запуск brevier")
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

const ARTICLE: &str = r#"<html><head><title>Настоящий заголовок</title></head><body>
<nav><a href="/">главное меню сайта</a></nav>
<article><h1>Настоящий заголовок</h1>
<p>Первый абзац статьи, достаточно длинный, чтобы Readability счёл его содержимым,
а не случайным довеском где-то с краю страницы, рядом со служебными блоками.</p>
<p>Второй абзац со <a href="/дальше">ссылкой</a> и ещё немного текста для веса,
потому что короткие страницы алгоритм извлечения не считает статьями вовсе.</p></article>
<footer>копирайт и прочий низ страницы</footer></body></html>"#;

#[test]
fn html_article_becomes_markdown() {
    let base = serve(vec![route("/a", "text/html; charset=utf-8", ARTICLE)]);
    let out = brevier(&[&format!("{base}/a")]);

    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let md = stdout(&out);
    assert!(
        md.starts_with("# Настоящий заголовок"),
        "нет заголовка:\n{md}"
    );
    assert!(md.contains("Первый абзац"));
    assert!(!md.contains("копирайт"), "подвал просочился:\n{md}");
    assert!(!md.contains("меню"), "навигация просочилась:\n{md}");
}

#[test]
fn markdown_is_served_as_is() {
    // Родной формат не трогаем: ни экранирования, ни переформатирования.
    let source = "# Заголовок\n\nТекст с _подчёркиванием_ и восклицанием!\n";
    let base = serve(vec![route("/doc.md", "text/markdown", source)]);

    let out = brevier(&[&format!("{base}/doc.md")]);
    assert_eq!(stdout(&out), source);
}

#[test]
fn plain_text_is_served_as_is() {
    let source = "   отступ сохраняется\nи перевод строки тоже\n";
    let base = serve(vec![route("/t", "text/plain", source)]);

    assert_eq!(stdout(&brevier(&[&format!("{base}/t")])), source);
}

#[test]
fn pdf_is_refused_with_its_own_code() {
    let base = serve(vec![route("/f.pdf", "application/pdf", "%PDF-1.7")]);
    let out = brevier(&[&format!("{base}/f.pdf")]);

    assert_eq!(out.status.code(), Some(4));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("application/pdf"),
        "не сказано, что за тип: {err}"
    );
}

#[test]
fn meta_refresh_is_followed() {
    let base = serve(vec![
        route(
            "/old",
            "text/html",
            r#"<html><head><meta http-equiv="refresh" content="0; url=/new">
               </head><body><p>Click here</p></body></html>"#,
        ),
        route("/new", "text/html", ARTICLE),
    ]);

    let md = stdout(&brevier(&[&format!("{base}/old")]));
    assert!(
        md.starts_with("# Настоящий заголовок"),
        "не дошли до цели:\n{md}"
    );
    assert!(!md.contains("Click here"));
}

#[test]
fn refresh_loop_terminates() {
    // Враждебный ввод: две страницы, отправляющие друг к другу. Бюджет обрывает
    // хождение, читатель получает последнюю страницу, а не зависший процесс.
    let base = serve(vec![
        route(
            "/ping",
            "text/html",
            r#"<meta http-equiv="refresh" content="0; url=/pong"><p>пинг</p>"#,
        ),
        route(
            "/pong",
            "text/html",
            r#"<meta http-equiv="refresh" content="0; url=/ping"><p>понг</p>"#,
        ),
    ]);

    let out = brevier(&[&format!("{base}/ping")]);
    assert!(out.status.code().is_some(), "процесс не завершился сам");
    let md = stdout(&out);
    assert!(md.contains("пинг") || md.contains("понг"), "пусто:\n{md}");
}

#[test]
fn raw_mode_skips_extraction() {
    let base = serve(vec![route("/a", "text/html", ARTICLE)]);
    let md = stdout(&brevier(&["--raw", &format!("{base}/a")]));

    // Без Readability на месте остаётся и навигация, и подвал.
    assert!(md.contains("главное меню сайта"), "нет навигации:\n{md}");
    assert!(md.contains("копирайт"), "нет подвала:\n{md}");
}

#[test]
fn html_mode_prints_extracted_html() {
    let base = serve(vec![route("/a", "text/html", ARTICLE)]);
    let out = stdout(&brevier(&["--html", &format!("{base}/a")]));

    assert!(out.contains("<p"), "это не html:\n{out}");
    assert!(!out.contains("копирайт"), "подвал не вырезан:\n{out}");
}

#[test]
fn windows_1251_is_decoded() {
    // Доюникодный веб никуда не делся, и он в основном русскоязычный.
    let body = vec![
        0xcf, 0xf0, 0xe8, 0xe2, 0xe5, 0xf2, 0x2c, 0x20, 0xec, 0xe8, 0xf0, 0x21, 0x20, 0xdd, 0xf2,
        0xee, 0x20, 0xf2, 0xe5, 0xea, 0xf1, 0xf2, 0x20, 0xe2, 0x20, 0xea, 0xee, 0xe4, 0xe8, 0xf0,
        0xee, 0xe2, 0xea, 0xe5, 0x20, 0x63, 0x70, 0x31, 0x32, 0x35, 0x31, 0x2e,
    ];
    let base = serve(vec![Route {
        path: "/cp",
        content_type: "text/plain; charset=windows-1251",
        body,
    }]);

    assert_eq!(
        stdout(&brevier(&[&format!("{base}/cp")])),
        "Привет, мир! Это текст в кодировке cp1251."
    );
}

#[test]
fn links_are_absolute() {
    let base = serve(vec![route("/a", "text/html", ARTICLE)]);
    let out = stdout(&brevier(&["--links", &format!("{base}/a")]));

    assert_eq!(out.trim(), format!("{base}/дальше"));
}

#[test]
fn check_counts_the_redirect_chain() {
    let base = serve(vec![
        route("/1", REDIRECT, "/2"),
        route("/2", REDIRECT, "/3"),
        route("/3", REDIRECT, "/4"),
        route("/4", REDIRECT, "/a"),
        route("/a", "text/html", ARTICLE),
    ]);

    // Четыре переезда — больше обычной жизни сайта, это находка.
    let long = stdout(&brevier(&["--check", "--min", "0", &format!("{base}/1")]));
    assert!(long.contains("access-redirects"), "{long}");
    assert!(long.contains("moved 4 times"), "{long}");

    // Три — нет: http → https, www, косая черта.
    let usual = stdout(&brevier(&["--check", "--min", "0", &format!("{base}/2")]));
    assert!(!usual.contains("· access-redirects"), "{usual}");
}

#[test]
fn check_takes_a_list_and_draws_the_lowest_score() {
    let base = serve(vec![
        route("/a", "text/html", ARTICLE),
        route("/doc.md", "text/markdown", "# Заголовок\n\nТекст.\n"),
    ]);
    let badge = std::env::temp_dir().join(format!("brevier-badge-{}.svg", std::process::id()));

    let out = brevier(&[
        "--check",
        "--min",
        "90",
        "--badge",
        badge.to_str().unwrap(),
        &format!("{base}/doc.md"),
        &format!("{base}/a"),
    ]);
    let text = stdout(&out);
    // Два отчёта подряд, через черту.
    assert_eq!(text.matches("# Check\n").count(), 2, "{text}");
    assert!(text.contains("\n---\n"));
    // Markdown сайта — сто, статья без языка, автора и даты — ниже
    // порога в девяносто, и код возврата идёт по худшей.
    assert_eq!(out.status.code(), Some(1));

    let svg = std::fs::read_to_string(&badge).expect("значок записан");
    let _ = std::fs::remove_file(&badge);
    let lowest = text
        .lines()
        .filter_map(|line| line.strip_prefix("**Score "))
        .filter_map(|rest| rest.split(' ').next()?.parse::<u32>().ok())
        .min()
        .unwrap();
    assert!(lowest < 100);
    assert!(svg.contains(&format!(">{lowest}</text>")), "{svg}");
}

const FEED: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<rss version="2.0"><channel><title>Лента блога</title><link>/</link>
<item><title>Первая запись</title><link>/posts/1</link>
<pubDate>Fri, 25 Sep 2026 10:00:00 +0300</pubDate>
<description>&lt;p&gt;Подводка первой записи.&lt;/p&gt;</description></item>
</channel></rss>"#;

#[test]
fn a_feed_opens_as_a_list_of_links_under_any_of_its_types() {
    let base = serve(vec![
        route("/rss", "application/rss+xml", FEED),
        route("/xml", "text/xml; charset=utf-8", FEED),
        // Сервер ошибся с типом: лента видна по телу.
        route("/html", "text/html", FEED),
    ]);
    for path in ["/rss", "/xml", "/html"] {
        let out = brevier(&[&format!("{base}{path}")]);
        assert_eq!(out.status.code(), Some(0), "{path}");
        assert_eq!(
            stdout(&out),
            format!(
                "# Лента блога\n\n*[127.0.0.1]({base}/)*\n\n## [Первая запись]({base}/posts/1)\n\n\
                 *25 September 2026*\n\nПодводка первой записи.\n"
            ),
            "{path}"
        );
        let err = String::from_utf8_lossy(&out.stderr);
        assert!(err.contains("a feed"), "{path}: {err}");
    }
}

#[test]
fn a_feed_names_its_encoding_in_the_declaration() {
    // koi8-r, а заголовок ответа о кодировке молчит — как у старых русских лент.
    let mut body = b"<?xml version=\"1.0\" encoding=\"koi8-r\"?><rss><channel><title>".to_vec();
    body.extend_from_slice(&[0xf0, 0xd2, 0xc9, 0xd7, 0xc5, 0xd4]);
    body.extend_from_slice(b"</title></channel></rss>");
    let base = serve(vec![Route {
        path: "/koi",
        content_type: "application/rss+xml",
        body,
    }]);
    let out = stdout(&brevier(&[&format!("{base}/koi")]));
    assert!(out.starts_with("# Привет\n"), "{out}");
}

#[test]
fn xml_that_is_not_a_feed_is_refused_like_any_other_type() {
    let base = serve(vec![route(
        "/sitemap.xml",
        "application/xml",
        r#"<?xml version="1.0"?><urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9"/>"#,
    )]);
    let out = brevier(&[&format!("{base}/sitemap.xml")]);
    assert_eq!(out.status.code(), Some(4));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("application/xml"), "{err}");
}

#[test]
fn a_feed_file_on_disk_opens_like_one_from_the_network() {
    let dir = std::env::temp_dir().join(format!("brevier-wire-feed-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let feed = dir.join("saved.rss");
    std::fs::write(
        &feed,
        FEED.replace("<link>/</link>", "<link>https://blog.example/</link>")
            .replace(
                "<link>/posts/1</link>",
                "<link>https://blog.example/posts/1</link>",
            ),
    )
    .unwrap();
    let out = brevier(&[feed.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(0));
    assert!(
        stdout(&out).contains("## [Первая запись](https://blog.example/posts/1)"),
        "{}",
        stdout(&out)
    );

    // XML, который не лента, исходником не показываем.
    let sitemap = dir.join("sitemap.xml");
    std::fs::write(&sitemap, "<?xml version=\"1.0\"?><urlset/>").unwrap();
    let out = brevier(&[sitemap.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(4));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_json_feed_is_read_under_plain_json() {
    let base = serve(vec![route(
        "/feed.json",
        "application/json",
        r#"{"version": "https://jsonfeed.org/version/1.1", "title": "JSON blog",
            "items": [{"id": "1", "url": "/p/1", "title": "One", "date_published": "2026-09-01T10:00:00Z"}]}"#,
    )]);
    let out = brevier(&[&format!("{base}/feed.json")]);
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(
        stdout(&out),
        format!("# JSON blog\n\n## [One]({base}/p/1)\n\n*1 September 2026*\n")
    );
    // JSON, который не лента, — прежний отказ по типу.
    let base = serve(vec![route("/api", "application/json", r#"{"ok": true}"#)]);
    assert_eq!(brevier(&[&format!("{base}/api")]).status.code(), Some(4));
}

#[test]
fn a_feed_link_names_the_feed_itself() {
    let base = serve(vec![route("/rss", "application/rss+xml", FEED)]);
    // `feed:http://…` — старая ссылка «подписаться»: это та же лента.
    let out = brevier(&[&format!("feed:{base}/rss")]);
    assert_eq!(out.status.code(), Some(0));
    assert!(
        stdout(&out).starts_with("# Лента блога\n"),
        "{}",
        stdout(&out)
    );
}
