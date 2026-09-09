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
