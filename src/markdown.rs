//! HTML → Markdown.
//!
//! Markdown здесь — внутреннее представление, а не формат файлов, и диалект
//! объявлен: CommonMark + GFM.
//!
//! Обратной печати через comrak в тракте вывода нет, хотя соблазн был:
//! канонизировать результат конвертации выглядело правильным. Но writer comrak
//! экранирует `!`, `_`, `[`, `<` безусловно (`cm.rs`), и русский текст
//! превращается в «публикация\!». На M1 это съест рендерер и никто не заметит,
//! а на M0 вывод читают глазами в `less` — там это ровно тот мусор вёрстки,
//! который рубрика велит считать дефектом. comrak остаётся парсером: на нём
//! собираются ссылки, на нём же будет рендер.

use std::borrow::Cow;
use std::collections::HashMap;

use comrak::nodes::NodeValue;
use comrak::{Arena, Options};
use htmd::element_handler::{HandlerResult, Handlers};
use htmd::options::{
    BulletListMarker, CodeBlockStyle, HeadingStyle, LinkStyle, Options as HtmdOptions,
};
use htmd::{Element, HtmlToMarkdown};

use crate::error::Error;
use crate::extract::{self, Article};

/// Теги, из которых нечего читать. Readability большую часть уже вырезал,
/// но `--raw` идёт мимо него.
const SKIP: &[&str] = &[
    "script", "style", "noscript", "iframe", "svg", "form", "button", "object", "embed", "canvas",
];

/// Выше этого картинка уже не распорка, а изображение. Два пикселя,
/// а не один: рамки и линейки верстают и в два.
///
/// Знает о нём и `extract::deicon`: распорка подходит под значок по всем
/// признакам, а выбрасывать её до конвертации нельзя — в её ширине
/// записана вложенность треда.
pub(crate) const SPACER_PX: u32 = 2;

/// Уже этого распорка отступом не является: пиксель-счётчик объявляет
/// себя единицей на единицу, и принимать его за уровень вложенности
/// значит загнать в цитату всю страницу.
const MIN_INDENT_PX: u32 = 8;

/// Глубже отступ ничего не добавляет: на HN бывает и десятый уровень,
/// а в колонке шириной в 65 знаков он съел бы саму реплику.
const MAX_NEST: usize = 3;

/// Метка отступа между конвертацией и `tidy`. Управляющий знак, которого
/// в тексте не бывает; в выводе её быть не может — `tidy` снимает все
/// до единой, нашлась вложенность или нет.
const INDENT_MARK: char = '\u{1}';

/// Статья: заголовок, автор, текст.
/// Что получилось из страницы.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Статья: связный текст, ради которого читатель и пришёл.
    Article,
    /// Список ссылок: главная блога, лента раздела, каталог. Статьи здесь
    /// нет, но есть куда пойти дальше.
    Listing,
}

/// Переваренная страница вместе с ответом на вопрос «что это было».
#[derive(Debug, Clone)]
pub struct Reading {
    pub markdown: String,
    pub kind: Kind,
}

pub fn from_article(article: &Article) -> Result<Reading, Error> {
    // Извлечение принесло одну карточку из ленты — показываем ленту целиком.
    // Решение принято там, где есть исходное дерево (`extract::listing`):
    // здесь остаётся перевести её в markdown. Миниатюры подставлять не надо,
    // картинки в ней свои.
    if let Some(html) = &article.listing_html {
        return Ok(Reading {
            markdown: strip_chrome(&heading(article.title.trim(), &tidy(&to_markdown(html)?))),
            kind: Kind::Listing,
        });
    }

    let body = convert(&article.content_html, &article.notes.numbers)?;
    // Заголовок приезжает дважды: `<title>` страницы и `<h1>` в самом тексте.
    let (title, body) = dedup_title(article.title.trim(), body.trim());

    let mut doc = String::with_capacity(body.len() + 128);
    if !title.is_empty() {
        doc.push_str("# ");
        doc.push_str(&title);
        doc.push_str("\n\n");
    }
    if let Some(byline) = &article.byline {
        doc.push_str(byline.trim());
        doc.push_str("\n\n");
    }
    doc.push_str(body.trim());
    doc.push('\n');

    let doc = tidy(&doc);
    // Лента — не ошибка. Раньше на неё отвечали «статьи нет», и читатель,
    // открывший главную блога, не получал ничего: ни списка, ни ссылок,
    // по которым он бы ушёл в статью. Страница показывается как есть,
    // а «это список, а не статья» интерфейс говорит словами.
    let (text, kind) = match strip_teasers(&doc) {
        Teasers::Article(text) => (text, Kind::Article),
        Teasers::Listing => (doc, Kind::Listing),
    };

    let text = strip_chrome(&text);
    let markdown = match kind {
        Kind::Article => text,
        Kind::Listing => with_thumbs(&text, &article.thumbs),
    };
    // Сноски дописываем последними, уже после вычитания хвоста: тело сноски
    // из одной ссылки для правила про витрину неотличимо от анонса,
    // и весь блок уехал бы в вырезанное.
    let markdown = with_notes(&markdown, &article.notes)?;

    Ok(Reading { markdown, kind })
}

/// Дописать сноски единым блоком в конец статьи.
///
/// В тексте на их месте уже стоит `[^N]` — это сделал конвертер. Тела
/// приходят html-ом и переводятся здесь: в одну строку каждое, потому что
/// сноска в GFM продолжается отступом, а лишний отступ в `less` читается
/// как блок кода.
fn with_notes(markdown: &str, notes: &extract::Notes) -> Result<String, Error> {
    if notes.bodies.is_empty() {
        return Ok(markdown.to_owned());
    }

    let mut out = unbracket(markdown.trim_end()).into_owned();
    out.push('\n');
    for (index, body) in notes.bodies.iter().enumerate() {
        let text = squeeze_lines(&to_markdown(body)?);
        if text.is_empty() {
            continue;
        }
        out.push_str(&format!("\n[^{}]: {text}\n", index + 1));
    }
    Ok(out)
}

/// Скобки вокруг метки сноски, оставшиеся от вёрстки.
///
/// Сайт пишет `[1]`, где ссылка только на цифре: у Грэма — `[` плюс ссылка
/// плюс `]`, у википедии скобки лежат внутри ссылки и уходят вместе с ней.
/// Первый случай оставляет в тексте `\[[^1]\]`, и это не сноска, а мусор
/// вокруг неё.
fn unbracket(markdown: &str) -> Cow<'_, str> {
    if !markdown.contains("[^") {
        return Cow::Borrowed(markdown);
    }
    let mut out = String::with_capacity(markdown.len());
    let mut rest = markdown;
    while let Some(at) = rest.find("[^") {
        let (head, tail) = rest.split_at(at);
        let Some(close) = tail.find(']') else {
            out.push_str(head);
            out.push_str(tail);
            return Cow::Owned(out);
        };
        let (mark, after) = tail.split_at(close + 1);
        let opened = head.strip_suffix("\\[").or_else(|| head.strip_suffix('['));
        let closed = after
            .strip_prefix("\\]")
            .or_else(|| after.strip_prefix(']'));
        match (opened, closed) {
            (Some(head), Some(after)) => {
                out.push_str(head);
                out.push_str(mark);
                rest = after;
            }
            _ => {
                out.push_str(head);
                out.push_str(mark);
                rest = after;
            }
        }
    }
    out.push_str(rest);
    Cow::Owned(out)
}

/// Многострочное тело сноски — в одну строку.
fn squeeze_lines(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Чем сайт дописывает к заголовку своё имя: «Заголовок | Сайт».
const TITLE_TAILS: [&str; 5] = [" | ", " — ", " – ", " · ", " :: "];

/// Короче этого заголовок под правило про хвост не идёт: у коротких
/// совпадений слишком велика вероятность, что они случайны.
const TITLE_MIN: usize = 12;

/// Заголовок, приехавший дважды: `<title>` страницы и `<h1>` в тексте.
///
/// Читателю это две почти одинаковые строки подряд. Правило узкое нарочно:
/// сверяется **первая строка тела** и только если она **заголовок**. Повтор
/// абзацем бывает содержанием — карточка инфобокса на википедии начинается
/// с имени статьи, и это не дубль вёрстки, а подпись карточки.
///
/// Если `<title>` длиннее ровно на имя сайта («… | Derek Sivers»), берём
/// короткий вид: имя сайта читатель видит в адресной строке, а заголовок
/// уезжает и в корешок вкладки, и в имя сохранённого файла.
fn dedup_title(title: &str, body: &str) -> (String, String) {
    let mut lines: Vec<&str> = body.lines().collect();
    let Some(first) = lines.iter().position(|line| !line.trim().is_empty()) else {
        return (title.to_owned(), body.to_owned());
    };

    let line = lines[first].trim();
    if !line.starts_with('#') {
        return (title.to_owned(), body.to_owned());
    }
    let head = plain(line.trim_start_matches('#').trim());
    if head.is_empty() {
        return (title.to_owned(), body.to_owned());
    }

    let title_plain = plain(title);
    let same = title_plain.eq_ignore_ascii_case(&head);
    // Хвост сайта отрезаем только у настоящего заголовка: «FAQ | Сайт»
    // и раздел «FAQ» — совпадение случайное.
    let tailed = head.chars().count() >= TITLE_MIN
        && TITLE_TAILS.iter().any(|tail| {
            title_plain
                .to_lowercase()
                .starts_with(&format!("{}{tail}", head.to_lowercase()))
        });

    if !same && !tailed {
        return (title.to_owned(), body.to_owned());
    }

    lines.remove(first);
    let title = if tailed { head } else { title.to_owned() };
    (title, lines.join("\n"))
}

/// Текст строки без разметки: ссылки — своим текстом, выделение снято,
/// пробелы сведены. Для сверки двух написаний одного и того же.
fn plain(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut rest = line;
    while let Some(start) = rest.find('[') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        match after
            .split_once("](")
            .and_then(|(text, tail)| tail.split_once(')').map(|(_, tail)| (text, tail)))
        {
            Some((text, tail)) => {
                out.push_str(text);
                rest = tail;
            }
            None => {
                out.push('[');
                rest = after;
            }
        }
    }
    out.push_str(rest);

    let out: String = out
        .chars()
        .filter(|c| !matches!(c, '*' | '_' | '`'))
        .collect();
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Поставить документу заголовок страницы, если своего у него нет.
fn heading(title: &str, body: &str) -> String {
    if title.is_empty() || body.trim_start().starts_with("# ") {
        return body.to_owned();
    }
    format!("# {title}\n\n{}", body.trim_start())
}

/// Вернуть ленте миниатюры записей.
///
/// Картинку карточки извлечение выбрасывает, потому что сайт пометил её
/// `aria-hidden` (см. `extract::thumbs`). Ставим её обратно перед заголовком
/// записи — и только там, где заголовок и правда ведёт на другую страницу,
/// а картинка нашлась ровно по его адресу. Отсебятины тут нет: обе половины
/// карточки взяты с самой страницы и стояли рядом.
fn with_thumbs(md: &str, thumbs: &HashMap<String, String>) -> String {
    if thumbs.is_empty() {
        return md.to_owned();
    }

    let mut out = String::with_capacity(md.len());
    let mut previous = "";

    for line in md.lines() {
        if let Some((title, url)) = teaser_link(line)
            && let Some(src) = thumbs.get(url)
            // Картинка уже стоит рядом — второй раз не надо.
            && !previous.contains(src.as_str())
        {
            // В `alt` — заголовок записи: с выключенными картинками рамка
            // должна говорить, что за ней, а не молчать.
            let alt = if title.contains(['[', ']']) {
                ""
            } else {
                title
            };
            out.push_str(&format!("![{alt}]({src})\n\n"));
        }
        out.push_str(line);
        out.push('\n');
        if !line.trim().is_empty() {
            previous = line;
        }
    }

    out
}

/// Страница целиком, без Readability (`--raw`). Нужен, чтобы отличать
/// «извлечение промахнулось» от «конвертация промахнулась».
pub fn from_html(html: &str) -> Result<String, Error> {
    Ok(tidy(&to_markdown(html)?))
}

/// Убрать то, что мешает читать: хвостовые пробелы, дыры в три и больше пустых
/// строк, выравнивание таблиц пробелами. Внутрь блоков кода не лезем — там
/// значим каждый символ.
fn tidy(md: &str) -> String {
    let mut out = String::with_capacity(md.len());
    let mut blanks = 0;
    let mut in_code = false;

    for line in md.lines() {
        if line.trim_start().starts_with("```") {
            in_code = !in_code;
            blanks = 0;
            out.push_str(line.trim_end());
            out.push('\n');
            continue;
        }
        if in_code {
            out.push_str(line);
            out.push('\n');
            continue;
        }

        if is_empty_item(line) {
            continue;
        }

        let line = line.trim_end();
        if line.is_empty() {
            blanks += 1;
            if blanks > 1 {
                continue;
            }
        } else {
            blanks = 0;
        }

        out.push_str(&squeeze_table_row(line));
        out.push('\n');
    }

    let out = unwrap_single_column_tables(&out);
    let out = nest_indents(&out);
    let trimmed = out.trim_end().to_owned();
    if trimmed.is_empty() {
        trimmed
    } else {
        trimmed + "\n"
    }
}

/// Пункт списка без содержания — маркер и больше ничего.
///
/// Так приезжает виджет, который на живой странице дорисовывает JS:
/// разметка списка в html есть, а пунктов в ней нет. На ленте habr это
/// пять пустых строк под заголовком «Новости». Горизонтальную линейку
/// (`---`) не трогаем: маркер там не один.
fn is_empty_item(line: &str) -> bool {
    let text = line.trim();
    if matches!(text, "-" | "*" | "+") {
        return true;
    }
    text.strip_suffix('.')
        .is_some_and(|number| !number.is_empty() && number.chars().all(|c| c.is_ascii_digit()))
}

/// Отступ, свёрстанный распоркой, — это вложенность.
///
/// Дерево ответов на форумах старой школы записано не разметкой, а шириной
/// пустой картинки в начале строки: hacker news ставит `width="40"` на
/// уровень. Конвертер про отступ ничего не знает и сплющивает тред в один
/// поток, где ответ на ответ неотличим от новой реплики.
///
/// Шаг отступа не выдумываем и не берём из сайта: он считается по самому
/// документу как наибольший общий делитель объявленных ширин. Тогда
/// правило не знает ни одной константы, привязанной к вёрстке, и уровень
/// не съедет, если какой-то из них в треде не встретилось.
///
/// Лестницу требуем доказать: меньше трёх меток или меньше двух разных
/// ширин — это не дерево, а разделительная полоска из того же пикселя,
/// и тогда метки просто снимаются.
fn nest_indents(md: &str) -> String {
    let widths: Vec<u32> = md.lines().filter_map(marker_width).collect();
    let mut ladder: Vec<u32> = widths.iter().copied().filter(|w| *w > 0).collect();
    ladder.sort_unstable();
    ladder.dedup();

    let step = if widths.len() >= 3 && ladder.len() >= 2 {
        ladder.iter().copied().reduce(gcd).unwrap_or(0)
    } else {
        0
    };

    let mut out: Vec<String> = Vec::with_capacity(md.lines().count());
    let mut depth = 0;

    for line in md.lines() {
        let Some(width) = marker_width(line) else {
            let mark = "> ".repeat(depth);
            if line.trim().is_empty() {
                // Пустой строкой реплика не начинается: цитата открылась бы
                // пустым знаком, а под ним — сам текст.
                if out
                    .last()
                    .is_none_or(|last| last.trim_matches(['>', ' ']).is_empty())
                {
                    continue;
                }
                out.push(mark.trim_end().to_owned());
            } else {
                out.push(mark + line);
            }
            continue;
        };

        // Между репликами — пустая строка без знака цитаты: со знаком
        // соседние ответы одного уровня склеились бы в одну цитату.
        while out
            .last()
            .is_some_and(|last| last.trim_matches(['>', ' ']).is_empty())
        {
            out.pop();
        }
        if !out.is_empty() {
            out.push(String::new());
        }
        depth = if step == 0 {
            0
        } else {
            (width as usize / step as usize).min(MAX_NEST)
        };
    }

    let mut joined = out.join("\n");
    joined.push('\n');
    joined
}

/// Ширина из метки отступа. Метка занимает строку целиком — её так
/// и ставил обработчик картинки.
fn marker_width(line: &str) -> Option<u32> {
    line.trim().strip_prefix(INDENT_MARK)?.parse().ok()
}

fn gcd(a: u32, b: u32) -> u32 {
    if b == 0 { a } else { gcd(b, a % b) }
}

/// Таблица в один столбец — не данные, а рамка вёрстки: так сделан инфобокс
/// википедии. В markdown она превращается в колонку из палок и дефисов,
/// читать которую невозможно. Разворачиваем в обычные абзацы.
fn unwrap_single_column_tables(md: &str) -> String {
    let lines: Vec<&str> = md.lines().collect();
    let mut out = String::with_capacity(md.len());
    let mut i = 0;

    while i < lines.len() {
        if !is_table_row(lines[i]) {
            out.push_str(lines[i]);
            out.push('\n');
            i += 1;
            continue;
        }

        let start = i;
        while i < lines.len() && is_table_row(lines[i]) {
            i += 1;
        }
        let table = &lines[start..i];

        if table.iter().all(|row| split_cells(row).len() == 1) {
            for row in table {
                let cell = split_cells(row)[0].trim();
                if cell.is_empty() || is_separator(cell) {
                    continue;
                }
                out.push_str(cell);
                out.push_str("\n\n");
            }
        } else {
            for row in table {
                out.push_str(row);
                out.push('\n');
            }
        }
    }

    out
}

/// Подписи кнопок, которые Readability приносит вместе со статьёй.
const UI_LABELS: &[&str] = &[
    "поделиться",
    "сохранить в закладки",
    "добавить в закладки",
    "в закладки",
    "share",
    "share this",
    "skip to content",
];

/// Метки, которые считаем интерфейсом только с двоеточием: «Автор» без него —
/// это может быть подпись, а не подпись кнопки.
const UI_LABELS_WITH_COLON: &[&str] = &["автор", "материалы по теме", "читайте также"];

/// Хвостовые уведомления: призыв подписаться и просьба сообщить об опечатке.
/// Это единственное место, где правило опирается на слова, а не на форму, —
/// у такого блока формы нет, это обычный абзац в конце текста. Слова взяты
/// общеязыковые, не по сайтам: «подписывайтесь» пишут все, кто ведёт канал.
const TAIL_NOTICES: &[&str] = &[
    "подписывайтесь",
    "подписаться на",
    "нашли ошибку",
    "сообщить об ошибке",
    "ctrl+enter",
];

/// Расширения файлов картинок: по ним отличаем «открыть в полном размере»
/// от анонса чужой статьи.
const IMAGE_SUFFIXES: &[&str] = &[
    ".jpg", ".jpeg", ".png", ".webp", ".gif", ".avif", ".svg", ".bmp",
];

/// Единицы, которые превращают число в счётчик: просмотры и время чтения.
const COUNTER_UNITS: &[&str] = &[
    "",
    "k",
    "к",
    "m",
    "тыс",
    "тыс.",
    "мин",
    "мин.",
    "минута",
    "минуты",
    "минут",
    "min",
    "min read",
    "мин чтения",
];

/// Обрезки интерфейса вокруг статьи.
///
/// Readability вырезает боковые колонки и подвал, но мелочь, прижатую к тексту,
/// оставляет: счётчик голосов в конце поста (dtf, vc.ru), «5 мин» и «6.9K»
/// под заголовком (habr), «Поделиться:» и «Материалы по теме:» (rb.ru),
/// «Добавить в закладки» (postnauka). Каждая мелочь по отдельности пустяк,
/// но по рубрике это футер, и на нём падают страницы.
fn strip_chrome(md: &str) -> String {
    let mut lines: Vec<&str> = Vec::new();
    let mut in_code = false;

    for line in md.lines() {
        if line.trim_start().starts_with("```") {
            in_code = !in_code;
        }
        if in_code || !is_chrome(line) {
            lines.push(line);
        }
    }

    drop_link_quotes(&mut lines);
    drop_repeated_captions(&mut lines);
    drop_repeats(&mut lines);
    drop_dangling_tail(&mut lines);

    // На месте выброшенных строк остались дыры из пустых — их схлопывает tidy.
    tidy(&lines.join("\n"))
}

fn is_chrome(line: &str) -> bool {
    let line = strip_leading_image(line.trim());
    let line = line.trim().trim_start_matches('#').trim();
    let line = line.trim_matches('*').trim();
    let (line, had_colon) = match line.strip_suffix(':') {
        Some(without) => (without.trim(), true),
        None => (line, false),
    };

    if line.is_empty() || line.chars().count() > 40 {
        return false;
    }

    let lowered = line.to_lowercase();
    if had_colon {
        // «1:» и «3:» — сноски в тексте (martinfowler.com), а не счётчики.
        // С двоеточием бывают только подписи кнопок.
        return UI_LABELS.contains(&lowered.as_str())
            || UI_LABELS_WITH_COLON.contains(&lowered.as_str());
    }
    is_counter(&lowered) || UI_LABELS.contains(&lowered.as_str())
}

/// `![Время прочтения](url) 5 минут` — картинка плюс счётчик; картинку
/// отбрасываем и смотрим, что осталось.
fn strip_leading_image(line: &str) -> &str {
    let Some(rest) = line.strip_prefix("![") else {
        return line;
    };
    let Some(after_alt) = rest.split_once("](") else {
        return line;
    };
    match after_alt.1.split_once(')') {
        Some((_, tail)) => tail,
        None => line,
    }
}

/// «20», «6.9K», «5 мин» — число со счётчиковой единицей и ничего больше.
fn is_counter(line: &str) -> bool {
    let digits_end = line
        .find(|c: char| !(c.is_ascii_digit() || c == '.' || c == ',' || c == ' '))
        .unwrap_or(line.len());
    let (number, unit) = line.split_at(digits_end);

    number.chars().any(|c| c.is_ascii_digit()) && COUNTER_UNITS.contains(&unit.trim())
}

/// Цитата, собранная из одних ссылок, — это «читайте также», а не цитата.
/// Так habr дописывает к статье подборку чужих материалов.
fn drop_link_quotes(lines: &mut Vec<&str>) {
    let mut result: Vec<&str> = Vec::with_capacity(lines.len());
    let mut i = 0;

    while i < lines.len() {
        if !lines[i].starts_with('>') {
            result.push(lines[i]);
            i += 1;
            continue;
        }

        let start = i;
        while i < lines.len() && lines[i].starts_with('>') {
            i += 1;
        }
        let quote = &lines[start..i];

        let lead_in = quote[0]
            .trim_start_matches('>')
            .trim()
            .trim_matches('*')
            .trim()
            .ends_with(':');
        let links = quote.iter().filter(|l| l.contains("](http")).count();

        if !(lead_in && links >= 2) {
            result.extend_from_slice(quote);
        }
    }

    *lines = result;
}

/// Короче этого подпись не считаем повтором: у коротких совпадений
/// слишком велика вероятность, что они случайны.
const CAPTION_MIN: usize = 12;

/// Подпись под картинкой, дословно повторяющая её `alt`.
///
/// Сайты кладут один и тот же текст и в атрибут, и отдельным абзацем — так
/// делает habr. В окне это выходит дважды подряд: сначала подписью
/// к заглушке картинки, потом обычным абзацем. Правило про повторы
/// такое не ловит: строки со скобками оно не трогает, иначе рвутся
/// многострочные ссылки.
fn drop_repeated_captions(lines: &mut Vec<&str>) {
    let mut drop = vec![false; lines.len()];

    for i in 0..lines.len() {
        let Some(alt) = image_alt(lines[i]) else {
            continue;
        };
        if alt.chars().count() < CAPTION_MIN {
            continue;
        }
        let above = lines[..i].iter().rposition(|line| !line.trim().is_empty());
        let below = lines[i + 1..]
            .iter()
            .position(|line| !line.trim().is_empty())
            .map(|offset| i + 1 + offset);

        for neighbour in [above, below].into_iter().flatten() {
            if same_text(lines[neighbour], alt) {
                drop[neighbour] = true;
            }
        }
    }

    let mut kept = lines.iter().enumerate();
    *lines = std::iter::from_fn(|| kept.next())
        .filter(|(i, _)| !drop[*i])
        .map(|(_, line)| *line)
        .collect();
}

/// Тот же текст с точностью до пробелов.
///
/// habr печатает подпись дважды, но в видимом тексте ставит неразрывные
/// пробелы, а в `alt` — обычные. Побайтово это разные строки, глазами —
/// одна и та же, и читателю достаётся дубль.
fn same_text(line: &str, alt: &str) -> bool {
    let words = |text: &str| text.split_whitespace().collect::<Vec<_>>().join(" ");
    words(line) == words(alt)
}

/// `![подпись](url)` целой строкой — вернуть подпись.
fn image_alt(line: &str) -> Option<&str> {
    let text = line.trim();
    let rest = text.strip_prefix("![")?;
    if !text.ends_with(')') {
        return None;
    }
    let (alt, _) = rest.rsplit_once("](")?;
    (!alt.is_empty()).then_some(alt)
}

/// На сколько строк назад смотрит поиск повтора.
const REPEAT_WINDOW: usize = 12;

/// Дословно повторённый рядом абзац — мусор вёрстки, а не текст автора.
/// Сайты кладут анонс и в мета-описание, и первым абзацем (devby.io),
/// печатают биографию автора под текстом дважды (shazoo.ru), повторяют
/// заглушку `<video>` у каждого ролика (nngroup).
///
/// Оба ограничения правила выведены из поломок на корпусе, а не из осторожности.
/// Окно в дюжину строк: на арзамасе один и тот же курс перечислен в двух
/// разных списках за сотни строк друг от друга, и это не повтор вёрстки,
/// а содержание страницы. Строки со скобками не трогаем совсем: там же ссылки
/// разбиты на несколько строк, и выброшенное продолжение `](url)` оставляет
/// в тексте открытую скобку — вместо мусора получается сломанная разметка.
/// Заголовки, списки, таблицы, цитаты и код повторяются законно.
fn drop_repeats(lines: &mut Vec<&str>) {
    let mut seen: HashMap<&str, usize> = HashMap::new();
    let mut result: Vec<&str> = Vec::with_capacity(lines.len());
    let mut in_code = false;

    for (i, line) in lines.iter().enumerate() {
        if line.trim_start().starts_with("```") {
            in_code = !in_code;
            result.push(line);
            continue;
        }
        let text = line.trim();
        let prose = !in_code
            && text.chars().count() >= 40
            && !text.contains(['[', ']'])
            && !text.starts_with(['#', '-', '*', '>', '|', '!', '+']);
        if !prose {
            result.push(line);
            continue;
        }
        let near = seen
            .insert(text, i)
            .is_some_and(|was| i - was <= REPEAT_WINDOW);
        if !near {
            result.push(line);
        }
    }

    *lines = result;
}

/// Подводка, у которой отняли продолжение: «Материалы по теме:» и ничего после.
fn drop_dangling_tail(lines: &mut Vec<&str>) {
    while let Some(last) = lines.last() {
        let raw = last.trim();
        let divider = matches!(raw, "" | "* * *" | "***" | "---" | "___" | "-----");
        let text = raw.trim_matches('*').trim();
        let dangling = text.ends_with(':') && text.chars().count() <= 80;
        let lowered = text.to_lowercase();
        let notice = TAIL_NOTICES.iter().any(|phrase| lowered.contains(phrase));
        if dangling || divider || notice {
            lines.pop();
        } else {
            break;
        }
    }
}

/// Что осталось от страницы после вычитания анонсов чужих статей.
enum Teasers {
    Article(String),
    /// Не статья, а лента: одни анонсы. Честнее сказать «статьи нет».
    Listing,
}

/// Доля документа, после которой заголовок-ссылка считается уже не частью
/// статьи, а хвостовым виджетом «читайте ещё».
const TAIL_STARTS_AT: f32 = 0.6;

/// Вырезать анонсы чужих статей.
///
/// Признак один: заголовок, который целиком является ссылкой на другую
/// страницу. В статье так не пишут — там заголовки либо простые, либо ссылаются
/// на якорь внутри страницы (`#anchor`), а вот витрины анонсов устроены именно
/// так. Дальше решает место: три и больше таких заголовка с самого начала —
/// это лента (habr отдавал ленту блога компании как статью); один в хвосте —
/// виджет «читайте ещё», которым fasterthanli.me дописывал к статье кусок
/// другой статьи.
///
/// Место решает и в обратную сторону, и это выстрадано внешним замером
/// 11 сентября 2026: раньше заголовок-анонс резал документ до конца
/// **с любого места**, и страница, у которой бейдж раздела стоит третьей
/// строкой, теряла всё. У smithsonianmag («SmartNews — Keeping you current»,
/// целиком ссылка) от статьи в 745 слов доезжало десять, у foxnews —
/// восемнадцать из 542. Вне хвоста заголовок-ссылка остаётся: лишняя строка
/// читателю дешевле потерянного абзаца.
fn strip_teasers(md: &str) -> Teasers {
    let lines: Vec<&str> = md.lines().collect();
    let teasers: Vec<usize> = lines
        .iter()
        .enumerate()
        .filter(|(_, line)| is_teaser_heading(line))
        .map(|(i, _)| i)
        .collect();

    let tail_starts = (lines.len() as f32 * TAIL_STARTS_AT) as usize;
    if teasers.len() >= 3 && teasers[0] < tail_starts {
        return Teasers::Listing;
    }

    // И заголовок-анонс, и виджет без заголовка режут только хвост: для
    // виджета это требование стоит внутри `teaser_grid_start` (сетка обязана
    // доходить до конца документа), для заголовка — здесь. Берём то,
    // что встретилось раньше.
    let heading = teasers.iter().copied().find(|&at| at >= tail_starts);
    let widget = teaser_grid_start(&lines);
    let (mut cut, label) = match (heading, widget) {
        (Some(heading), Some(at)) if at < heading => (at, true),
        (Some(heading), _) => (heading, false),
        (None, Some(at)) => (at, true),
        (None, None) => return Teasers::Article(md.to_owned()),
    };

    // Подводка к виджету («Here's another article just for you:») остаётся
    // висеть без продолжения — убираем и её.
    while cut > 0 {
        let previous = lines[cut - 1].trim_end();
        if previous.is_empty() || previous.ends_with(':') {
            cut -= 1;
        } else {
            break;
        }
    }
    // Шапка сетки анонсов: «Сейчас на главной», «### Новости», «Изображение
    // в превью:» — заголовки и метки, которые без своей сетки повисают.
    // Снимаем их подряд и только над сеткой: там короткий заголовок —
    // это её название, а не последний раздел статьи.
    while label && cut > 0 {
        let bare = lines[cut - 1]
            .trim()
            .trim_start_matches('#')
            .trim_matches('*')
            .trim();
        let heading = lines[cut - 1].trim_start().starts_with('#');
        let short = bare.chars().count() <= 60 && !bare.ends_with(['.', '!', '?', '…']);
        if bare.is_empty() || (short && (heading || !bare.starts_with(['|', '>', '-', '`', '[']))) {
            cut -= 1;
        } else {
            break;
        }
    }

    let kept = lines[..cut].join("\n");
    Teasers::Article(kept.trim_end().to_owned() + "\n")
}

/// Сколько анонсов подряд превращают хвост в сетку «читайте ещё».
const TEASER_GRID: usize = 2;

/// Где начинается сетка анонсов, которой заканчивается документ.
///
/// Форма узкая, и это выстрадано: первая версия считала анонсом всякий список
/// со ссылками и всякую картинку-ссылку в хвосте — и съела «Stabilized APIs»
/// у блога Rust, список примеров у nngroup, врезку с демо у alistapart.
/// В живой статье и список ссылок, и картинка со ссылкой — нормальный текст.
///
/// Что отличает сетку: анонсов **несколько**, они картинки-ссылки на страницы,
/// и они занимают **весь хвост** до конца документа. Подпись под анонсом
/// принимаем, только если прямо над ней анонс, — иначе назад уехал бы
/// последний абзац статьи.
fn teaser_grid_start(lines: &[&str]) -> Option<usize> {
    let mut start = None;
    let mut teasers = 0;
    let mut i = lines.len();

    while i > 0 {
        i -= 1;
        if lines[i].trim().is_empty() {
            continue;
        }
        if is_teaser_paragraph(lines[i]) {
            teasers += 1;
            start = Some(i);
            continue;
        }
        let above = lines[..i].iter().rposition(|line| !line.trim().is_empty());
        match above {
            Some(a) if is_teaser_paragraph(lines[a]) => continue,
            _ => break,
        }
    }

    // Сетка на весь документ — это не статья с хвостом, и резать нечего.
    start.filter(|&at| at > 0 && teasers >= TEASER_GRID)
}

/// Абзац, который целиком является ссылкой на другую страницу.
///
/// Картинка-ссылка встречается и внутри статьи — так делают «открыть
/// в полном размере», и ixbt в одном документе даёт обе формы. Отличает их
/// цель ссылки: файл картинки — это увеличение, страница — анонс.
fn is_teaser_paragraph(line: &str) -> bool {
    let text = line.trim();
    if !text.starts_with('[') {
        return false;
    }
    let Some(url) = sole_link_target(text) else {
        return false;
    };
    !points_at_image(url)
}

/// Цель ссылки, если строка — ровно одна ссылка и ничего кроме неё.
fn sole_link_target(text: &str) -> Option<&str> {
    let inner = text.strip_prefix('[')?;
    let (_, target) = inner.rsplit_once("](")?;
    let url = target.strip_suffix(')')?;
    if url.contains(char::is_whitespace) || !url.starts_with("http") {
        return None;
    }
    Some(url)
}

fn points_at_image(url: &str) -> bool {
    let path = url.split(['?', '#']).next().unwrap_or(url).to_lowercase();
    IMAGE_SUFFIXES.iter().any(|ext| path.ends_with(ext))
}

fn is_teaser_heading(line: &str) -> bool {
    teaser_link(line).is_some()
}

/// Заголовок-анонс, разобранный на текст и адрес.
fn teaser_link(line: &str) -> Option<(&str, &str)> {
    let rest = line.trim_start().strip_prefix('#')?;
    let text = rest.trim_start_matches('#').trim();
    let inner = text.strip_prefix('[')?;
    // Ровно одна ссылка на другую страницу и ничего кроме неё.
    let (title, target) = inner.split_once("](")?;
    let url = target.strip_suffix(')')?;
    if url.contains(&['[', ']'][..]) || !(url.starts_with("http://") || url.starts_with("https://"))
    {
        return None;
    }
    Some((title, url))
}

/// htmd выравнивает столбцы по самой длинной ячейке. В инфобоксе википедии это
/// строка-разделитель в шестьсот дефисов: в `less` не читается, а в диффе
/// корпуса правка одной ячейки переписывает таблицу целиком. GFM выравнивания
/// не требует — сжимаем.
fn squeeze_table_row(line: &str) -> Cow<'_, str> {
    if !is_table_row(line) {
        return Cow::Borrowed(line);
    }

    let cells: Vec<&str> = split_cells(line)
        .into_iter()
        .map(|cell| {
            let cell = cell.trim();
            if !is_separator(cell) {
                return cell;
            }
            // Разделитель: `-----` → `---`, двоеточия выравнивания целы.
            match (cell.starts_with(':'), cell.ends_with(':')) {
                (true, true) => ":---:",
                (true, false) => ":---",
                (false, true) => "---:",
                (false, false) => "---",
            }
        })
        .collect();

    Cow::Owned(format!("| {} |", cells.join(" | ")))
}

/// Строка таблицы: палки по краям и хоть что-то между ними.
fn is_table_row(line: &str) -> bool {
    line.len() > 1 && line.starts_with('|') && line.ends_with('|')
}

/// Ячейки разделяет `|`; экранированный `\|` — часть содержимого.
fn split_cells(line: &str) -> Vec<&str> {
    if !is_table_row(line) {
        return Vec::new();
    }
    let inner = &line[1..line.len() - 1];
    let mut cells = Vec::new();
    let mut start = 0;
    let mut escaped = false;

    for (i, ch) in inner.char_indices() {
        match ch {
            _ if escaped => escaped = false,
            '\\' => escaped = true,
            '|' => {
                cells.push(&inner[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    cells.push(&inner[start..]);
    cells
}

fn is_separator(cell: &str) -> bool {
    let core = cell.trim_matches(':');
    !core.is_empty() && core.chars().all(|c| c == '-')
}

/// Исходящие ссылки статьи, в порядке появления, без повторов.
///
/// Нужны для навигационного замера M0: страница может конвертироваться
/// идеально, а все ссылки с неё вести в SPA, пейволлы и PDF.
pub fn links(md: &str) -> Vec<String> {
    let arena = Arena::new();
    let root = comrak::parse_document(&arena, md, &options());

    let mut seen = Vec::new();
    for node in root.descendants() {
        let NodeValue::Link(link) = &node.data.borrow().value else {
            continue;
        };
        let url = &link.url;
        if (url.starts_with("http://") || url.starts_with("https://")) && !seen.contains(url) {
            seen.push(url.clone());
        }
    }
    seen
}

/// Картинки статьи, в порядке появления, без повторов.
///
/// Адрес отдаём ровно таким, как он записан в тексте: по нему потом идёт
/// замена ссылки при сохранении, и «поправленный» адрес там не совпадёт.
pub fn images(md: &str) -> Vec<String> {
    let arena = Arena::new();
    let root = comrak::parse_document(&arena, md, &options());

    let mut seen: Vec<String> = Vec::new();
    for node in root.descendants() {
        let NodeValue::Image(image) = &node.data.borrow().value else {
            continue;
        };
        if !image.url.trim().is_empty() && !seen.contains(&image.url) {
            seen.push(image.url.clone());
        }
    }
    seen
}

fn to_markdown(html: &str) -> Result<String, Error> {
    convert(html, &HashMap::new())
}

/// То же, но со сносками: ссылка на снятую сноску становится `[^N]`.
///
/// Номера приходят готовыми — их раздаёт [`extract::notes`], там, где ещё
/// видно и ссылку, и её цель. Обработчик замыкает их в себе: другого
/// способа донести знание о статье до обработчика htmd нет.
fn convert(html: &str, notes: &HashMap<String, usize>) -> Result<String, Error> {
    let notes = notes.clone();
    let converter = HtmlToMarkdown::builder()
        .skip_tags(SKIP.to_vec())
        .add_handler(vec!["code"], code_handler)
        .add_handler(vec!["span"], span_handler)
        .add_handler(
            vec!["a"],
            move |handlers: &dyn Handlers, element: Element| match note_number(&element, &notes) {
                Some(number) => Some(format!("[^{number}]").into()),
                None => anchor_handler(handlers, element),
            },
        )
        .add_handler(vec!["img"], image_handler)
        .options(HtmdOptions {
            heading_style: HeadingStyle::Atx,
            bullet_list_marker: BulletListMarker::Dash,
            // htmd ставит три пробела после маркера — наследство turndown.
            // Markdown пишут иначе, и в `less` лишний отступ заметен.
            ul_bullet_spacing: 1,
            ol_number_spacing: 1,
            link_style: LinkStyle::Inlined,
            code_block_style: CodeBlockStyle::Fenced,
            ..Default::default()
        })
        .build();

    converter.convert(html).map_err(Error::Convert)
}

/// Блок кода, свёрстанный без `<pre>`.
///
/// htmd считает блоком только `<code>` внутри `<pre>`, всё остальное — код-спан.
/// Но верстают и иначе: fasterthanli.me заворачивает код в
/// `<figure><code>…</code></figure>`, и статья на 65 КБ приходила без единого
/// ограждения кода — зато с 198 строками непарных бэктиков. Признак берём по
/// содержимому: перевод строки внутри. Инлайновый код с переносом внутри
/// теоретически поймается ложно, но так верстают редко, а цена ошибки в другую
/// сторону — нечитаемая статья в техническом блоге.
fn code_handler(handlers: &dyn Handlers, element: Element) -> Option<HandlerResult> {
    let content = handlers.walk_children(element.node).content;
    let code = content.trim_matches('\n');

    if !code.contains('\n') {
        return handlers.fallback(element);
    }

    let fence = "`".repeat(3.max(longest_backtick_run(code) + 1));
    let language = language_from_attrs(&element).unwrap_or_default();

    Some(format!("\n\n{fence}{language}\n{code}\n{fence}\n\n").into())
}

/// `<span>` с переводом строки внутри.
///
/// htmd срезает переводы строк по краям каждого span'а (`span.rs`), а подсветка
/// синтаксиса верстает строку кода цепочкой span'ов, где перевод строки —
/// последний из них: without.boats отдавал блок кода, склеенный в одну строку.
/// Вне блоков кода терять нечего — там переводы строк давно схлопнуты в пробелы,
/// а лишние пустые строки уберёт `tidy`.
fn span_handler(handlers: &dyn Handlers, element: Element) -> Option<HandlerResult> {
    let content = handlers.walk_children(element.node).content;
    if !content.contains('\n') {
        return handlers.fallback(element);
    }
    Some(content.into())
}

/// Ссылка без текста — мусор, а не ссылка.
///
/// Такие оставляют якоря заголовков (github, fasterthanli.me): `[](#why-rust)`.
/// Нажать не на что, читать нечего.
fn anchor_handler(handlers: &dyn Handlers, element: Element) -> Option<HandlerResult> {
    if handlers
        .walk_children(element.node)
        .content
        .trim()
        .is_empty()
    {
        return None;
    }
    handlers.fallback(element)
}

/// Распорка — не картинка.
///
/// Старая вёрстка отступает текст пустой картинкой: `<img src="s.gif"
/// height="1" width="120">`. Иллюстрацией такое не бывает нигде — один
/// пиксель высоты это либо отступ, либо счётчик посещений, — а читателю
/// достаётся строкой `![](…)`, в окне ещё и рамкой на якоре: на треде
/// hacker news таких сто одна.
///
/// Ширину, прежде чем выбросить, записываем меткой: в ней записана
/// глубина ответа, и разбирает её `nest_indents`. Ширину меньше
/// `MIN_INDENT_PX` отступом не считаем и пишем ноль — им объявляет себя
/// пиксель-счётчик, а не уровень.
fn image_handler(handlers: &dyn Handlers, element: Element) -> Option<HandlerResult> {
    let Some(height) = px(&element, "height") else {
        return handlers.fallback(element);
    };
    if height > SPACER_PX {
        return handlers.fallback(element);
    }

    let width = px(&element, "width").unwrap_or(0);
    let width = if width >= MIN_INDENT_PX { width } else { 0 };
    Some(format!("\n\n{INDENT_MARK}{width}\n\n").into())
}

/// Размер, объявленный атрибутом. Пиксели в атрибуте пишут числом;
/// проценты и `px` внутри `style` — не наше дело, там не распорки.
/// Номер сноски, на которую ведёт ссылка, — или `None`, если это обычная
/// ссылка. Решает не вид ссылки, а список снятых сносок: форму разобрали
/// по дереву, здесь остаётся сверка по цели.
fn note_number(element: &Element, notes: &HashMap<String, usize>) -> Option<usize> {
    if notes.is_empty() {
        return None;
    }
    let href = element
        .attrs
        .iter()
        .find(|attr| &attr.name.local == "href")?
        .value
        .trim();
    notes.get(href.strip_prefix('#')?).copied()
}

fn px(element: &Element, name: &str) -> Option<u32> {
    element
        .attrs
        .iter()
        .find(|attr| &attr.name.local == name)?
        .value
        .trim()
        .parse()
        .ok()
}

fn longest_backtick_run(content: &str) -> usize {
    let mut longest = 0;
    let mut run = 0;
    for ch in content.chars() {
        if ch == '`' {
            run += 1;
            longest = longest.max(run);
        } else {
            run = 0;
        }
    }
    longest
}

/// Язык блока кода: из атрибута или из класса.
///
/// Атрибут кладёт `extract::keep_lang` — он снимает язык до извлечения
/// и проносит мимо чистки классов, которую Readability делает всему
/// дереву. Класс остаётся ради разметки, до Readability не доезжавшей:
/// куски вёрстки внутри README.
fn language_from_attrs(element: &Element) -> Option<String> {
    let attr = |name: &str| {
        element
            .attrs
            .iter()
            .find(|attr| &attr.name.local == name)
            .map(|attr| attr.value.to_string())
    };

    let named = extract::LANG_ATTRS.iter().find_map(|name| attr(name));
    if let Some(language) = named.as_deref().and_then(extract::language_token) {
        return Some(language);
    }

    attr("class")?.split_whitespace().find_map(|class| {
        extract::LANG_PREFIXES
            .iter()
            .find_map(|prefix| class.strip_prefix(prefix))
            .and_then(extract::language_token)
    })
}

/// CommonMark + GFM. Таблицы — обязательная часть, из-за них GFM и выбран.
///
/// Диалект у продукта один, поэтому и настройки одни: этими же разбирает
/// документ окно. Свой набор рядом расходится молча — так в окне уже жили
/// списки задач, которые отрисовщик умеет рисовать, а разбор не включал.
pub fn options() -> Options<'static> {
    let mut options = Options::default();
    options.extension.table = true;
    options.extension.strikethrough = true;
    options.extension.autolink = true;
    options.extension.tasklist = true;
    // Оповещения github (`> [!NOTE]`): без этого читателю достаётся цитата,
    // первой строкой которой написано «[!NOTE]». В README они сплошь,
    // и рисует их хостинг коробкой, а не текстом.
    options.extension.alerts = true;
    // Сноски: единый вид, к которому сводятся все веб-формы — см.
    // `extract::notes`. Без этого `[^1]` доехало бы до читателя текстом.
    options.extension.footnotes = true;
    // Ширина колонки — дело рендерера и читателя, не файла: строка = абзац,
    // так диффы корпуса показывают правку, а не переливание переносов.
    options.render.width = 0;
    options
}

#[cfg(test)]
mod tests {

    #[test]
    fn a_title_that_arrived_twice_is_printed_once() {
        // Дословный повтор: заголовок страницы и `<h1>` в тексте.
        let (title, body) = dedup_title(
            "Relax for the same result",
            "## Relax for the same result\n\nТекст.",
        );
        assert_eq!(title, "Relax for the same result");
        assert_eq!(body.trim(), "Текст.");

        // Тот же заголовок плюс имя сайта: короткий вид честнее.
        let (title, body) = dedup_title(
            "Relax for the same result | Derek Sivers",
            "## Relax for the same result\n\nТекст.",
        );
        assert_eq!(title, "Relax for the same result");
        assert_eq!(body.trim(), "Текст.");

        // Разметку в заголовке снимаем только для сверки, из текста
        // выбрасывается вся строка целиком.
        let (title, body) = dedup_title("Дункан Высокий", "## [Дункан **Высокий**](/a)\n\nТекст.");
        assert_eq!(title, "Дункан Высокий");
        assert_eq!(body.trim(), "Текст.");
    }

    #[test]
    fn a_heading_of_its_own_stays() {
        // Раздел, который не повторяет заголовок, — структура автора.
        let (title, body) = dedup_title("Статья про меру", "## Как мы считали\n\nТекст.");
        assert_eq!(title, "Статья про меру");
        assert!(body.starts_with("## Как мы считали"));

        // Короткое совпадение с хвостом — случайность, а не имя сайта.
        let (title, body) = dedup_title("FAQ | Сайт", "## FAQ\n\nТекст.");
        assert_eq!(title, "FAQ | Сайт");
        assert!(body.starts_with("## FAQ"));

        // Первым идёт абзац, а не заголовок: карточка инфобокса повторяет
        // имя статьи законно.
        let (title, body) = dedup_title("Дункан Высокий", "Дункан Высокий\n\nТекст.");
        assert_eq!(title, "Дункан Высокий");
        assert!(body.starts_with("Дункан Высокий"));
    }
    use super::*;

    #[test]
    fn tables_survive_the_round_trip() {
        let md =
            from_html("<table><tr><th>a</th><th>b</th></tr><tr><td>1</td><td>2</td></tr></table>")
                .unwrap();
        assert!(md.contains("| a | b |"), "таблица потерялась:\n{md}");
    }

    #[test]
    fn headings_and_links() {
        let md = from_html(r#"<h2>Titl</h2><p>text <a href="https://e.com">link</a></p>"#).unwrap();
        assert!(md.contains("## Titl"));
        assert!(md.contains("[link](https://e.com)"));
    }

    #[test]
    fn code_without_pre_becomes_a_fenced_block() {
        let md = from_html("<figure><code>fn main() {\n    println!(\"hi\");\n}</code></figure>")
            .unwrap();
        assert_eq!(md, "```\nfn main() {\n    println!(\"hi\");\n}\n```\n");
    }

    #[test]
    fn highlighted_code_keeps_its_line_breaks() {
        let md = from_html(
            "<pre><code><span><span>enum</span> <span>Foo</span><span> {</span><span>\n</span></span>\
             <span><span>    Bar,</span><span>\n</span></span><span><span>}</span></span></code></pre>",
        )
        .unwrap();
        assert_eq!(md, "```\nenum Foo {\n    Bar,\n}\n```\n");
    }

    #[test]
    fn short_inline_code_stays_inline() {
        let md = from_html("<p>вызов <code>main()</code> тут</p>").unwrap();
        assert_eq!(md, "вызов `main()` тут\n");
    }

    #[test]
    fn pre_code_still_works_and_keeps_language() {
        let md =
            from_html("<pre><code class=\"language-rust\">let x = 1;\nlet y = 2;</code></pre>")
                .unwrap();
        assert_eq!(md, "```rust\nlet x = 1;\nlet y = 2;\n```\n");
    }

    /// Язык в атрибуте: так он приезжает от `extract::keep_lang` после
    /// чистки классов, и так его пишет блог Rust (`data-lang="plain"`).
    #[test]
    fn a_language_in_an_attribute_reaches_the_fence() {
        let md = from_html("<pre><code data-lang=\"shell\">cd /tmp\nls</code></pre>").unwrap();
        assert_eq!(md, "```shell\ncd /tmp\nls\n```\n");
    }

    /// Подпись к блоку языком не является: сайты кладут в это место
    /// что угодно, а попадает оно прямо в текст статьи.
    #[test]
    fn a_caption_is_not_a_language() {
        let md = from_html("<pre><code data-lang=\"Shell session\">ls\ncd</code></pre>").unwrap();
        assert_eq!(md, "```shell\nls\ncd\n```\n");

        let md = from_html("<pre><code data-lang=\"<b>ужас</b>\">ls\ncd</code></pre>").unwrap();
        assert_eq!(md, "```\nls\ncd\n```\n");
    }

    /// Строка обсуждения, свёрстанная как на hacker news: распорка нужной
    /// ширины, ячейка голосования и сам текст.
    fn comment(width: u32, text: &str) -> String {
        format!(
            "<tr><td><table><tbody><tr>\
             <td><img src=\"s.gif\" height=\"1\" width=\"{width}\"></td>\
             <td><center><a href=\"/vote\"></a></center></td>\
             <td><div><p>{text}</p></div></td>\
             </tr></tbody></table></td></tr>"
        )
    }

    fn thread(rows: &[(u32, &str)]) -> String {
        let body: String = rows.iter().map(|(w, t)| comment(*w, t)).collect();
        from_html(&format!("<table><tbody>{body}</tbody></table>")).unwrap()
    }

    #[test]
    fn a_spacer_is_not_a_picture() {
        // Пустая картинка в пиксель высотой — отступ или счётчик, но
        // не иллюстрация. На треде hacker news таких сто одна.
        let md = from_html("<p><img src=\"s.gif\" height=\"1\" width=\"0\">текст</p>").unwrap();
        assert!(!md.contains("!["), "{md}");
        assert!(md.contains("текст"), "{md}");
    }

    #[test]
    fn a_picture_with_a_size_survives() {
        let md = from_html("<p><img src=\"a.png\" width=\"600\" height=\"400\"></p>").unwrap();
        assert!(md.contains("![](a.png)"), "{md}");
    }

    #[test]
    fn a_ladder_of_spacers_becomes_nesting() {
        let md = thread(&[
            (0, "Корень треда."),
            (40, "Ответ."),
            (80, "Ответ на ответ."),
            (0, "Новая ветка."),
        ]);
        assert_eq!(
            md,
            "Корень треда.\n\n> Ответ.\n\n> > Ответ на ответ.\n\nНовая ветка.\n"
        );
    }

    #[test]
    fn the_step_comes_from_the_document() {
        // Уровня в сорок пикселей в треде не встретилось — шаг всё равно
        // сорок: он наибольший общий делитель, а не первая попавшаяся ширина.
        let md = thread(&[(0, "Корень."), (80, "Второй уровень."), (120, "Третий.")]);
        assert!(md.contains("\n> > Второй уровень."), "{md}");
        assert!(md.contains("\n> > > Третий."), "{md}");
    }

    #[test]
    fn deeper_than_three_levels_does_not_indent_further() {
        // В колонке шириной в 65 знаков десятый уровень съел бы реплику.
        let md = thread(&[(0, "Корень."), (40, "Раз."), (360, "Девятый уровень.")]);
        assert!(md.contains("\n> > > Девятый уровень."), "{md}");
        assert!(!md.contains("> > > > "), "{md}");
    }

    #[test]
    fn a_lone_spacer_does_not_quote_the_page() {
        // Полоска-разделитель из того же пикселя — не лестница отступов,
        // и загонять за ней полстраницы в цитату нельзя.
        let md = from_html(
            "<p>До полоски.</p><p><img src=\"line.gif\" height=\"1\" width=\"500\"></p><p>После.</p>",
        )
        .unwrap();
        assert_eq!(md, "До полоски.\n\nПосле.\n");
    }

    #[test]
    fn a_counting_pixel_is_not_an_indent() {
        // Счётчик объявляет себя единицей на единицу. Уровнем вложенности
        // такая ширина не бывает — иначе в цитату уехала бы вся страница.
        let md = thread(&[(1, "Первый абзац."), (1, "Второй."), (1, "Третий.")]);
        assert_eq!(md, "Первый абзац.\n\nВторой.\n\nТретий.\n");
    }

    #[test]
    fn images_are_listed_once_and_in_order() {
        let md = "![a](x.png)\n\n![b](https://e.com/y.jpg)\n\n![a again](x.png)";
        assert_eq!(images(md), vec!["x.png", "https://e.com/y.jpg"]);
    }

    #[test]
    fn empty_anchors_are_dropped() {
        let html = r##"<p>текст <a href="#anchor"></a> и <a href="https://e.com">ссылка</a></p>"##;
        let md = from_html(html).unwrap();
        assert_eq!(md, "текст и [ссылка](https://e.com)\n");
    }

    #[test]
    fn text_is_not_over_escaped() {
        // comrak на этом месте выдавал «Ура\!» и «a\_b».
        let md = from_html("<p>Ура! Вот так: a_b, 3 &lt; 5.</p>").unwrap();
        assert_eq!(md, "Ура! Вот так: a\\_b, 3 < 5.\n");
    }

    #[test]
    fn wide_tables_are_squeezed() {
        let md = from_html(
            "<table><tr><th>очень длинный заголовок столбца</th><th>б</th></tr>\
             <tr><td>к</td><td>д</td></tr></table>",
        )
        .unwrap();
        assert_eq!(
            md,
            "| очень длинный заголовок столбца | б |\n| --- | --- |\n| к | д |\n"
        );
    }

    #[test]
    fn counters_and_button_labels_are_dropped() {
        let md = "# Статья\n\nАвтор\n\n5 мин\n\n6.9K\n\nПоделиться:\n\nТекст статьи.\n\n20\n";
        assert_eq!(strip_chrome(md), "# Статья\n\nАвтор\n\nТекст статьи.\n");
    }

    #[test]
    fn reading_time_with_an_icon_is_a_counter_too() {
        let md = "# Статья\n\n![Время прочтения](https://e.com/clock.svg) 5 минут\n\nТекст.\n";
        assert_eq!(strip_chrome(md), "# Статья\n\nТекст.\n");
    }

    #[test]
    fn numbered_footnote_markers_survive() {
        // martinfowler.com размечает сноски строкой «1:» — это не счётчик.
        let md = "# Статья\n\n1:\n\nПервая сноска.\n\n3:\n\nТретья.\n";
        assert_eq!(strip_chrome(md), md);
    }

    #[test]
    fn numbers_inside_the_text_survive() {
        // Счётчик — это отдельная строка, а не число в абзаце или в списке.
        let md = "# Статья\n\nВ 2024 году было 20 попыток.\n\n- 20\n";
        assert_eq!(strip_chrome(md), md.trim_end().to_owned() + "\n");
    }

    #[test]
    fn a_quote_of_links_is_a_read_also_block() {
        let md = "# Статья\n\n> **Возможно, вас заинтересуют:**\n>\n> → [Раз](https://e.com/1)\n> → [Два](https://e.com/2)\n\nПоследний абзац.\n";
        assert_eq!(strip_chrome(md), "# Статья\n\nПоследний абзац.\n");
    }

    #[test]
    fn an_ordinary_quote_with_a_link_survives() {
        let md =
            "# Статья\n\n> Цитата с мыслью и [ссылкой](https://e.com/1) внутри.\n\nДальше текст.\n";
        assert_eq!(strip_chrome(md), md);
    }

    #[test]
    fn dangling_lead_in_at_the_end_is_cut() {
        let md = "# Статья\n\nТекст.\n\n* * *\n\n**Материалы по теме:**\n";
        assert_eq!(strip_chrome(md), "# Статья\n\nТекст.\n");
    }

    #[test]
    fn code_is_not_touched_by_chrome_cleanup() {
        let md = "# Статья\n\n```\n20\nПоделиться:\n```\n\nТекст.\n";
        assert_eq!(strip_chrome(md), md);
    }

    #[test]
    fn trailing_teaser_widget_is_cut() {
        let md = "# Статья\n\nТекст статьи, ради которого всё затевалось.\n\nЕщё абзац.\n\n                  Здесь текст, а дальше начинается витрина.\n\nHere's another article just for you:\n\n                  ## [Другая статья](https://e.com/other)\n\nАнонс другой статьи.\n";
        let Teasers::Article(kept) = strip_teasers(md) else {
            panic!("статью приняли за ленту");
        };
        assert!(
            kept.ends_with("а дальше начинается витрина.\n"),
            "не отрезано:\n{kept}"
        );
        assert!(!kept.contains("another article"));
    }

    /// Бейдж раздела — тоже заголовок-ссылка, но стоит он в начале,
    /// и после него идёт вся статья. Резать по нему значит выбросить её
    /// целиком: так у smithsonianmag доезжало десять слов из семисот сорока.
    #[test]
    fn a_heading_link_at_the_top_leaves_the_article_alone() {
        let md = "# Статья\n\n### [SmartNews](https://e.com/smart-news/)\n\nПервый абзац, ради которого всё затевалось.\n\nВторой абзац, он тоже должен доехать.\n\nТретий абзац, и на нём статья кончается.\n";
        let Teasers::Article(kept) = strip_teasers(md) else {
            panic!("статью приняли за ленту");
        };
        assert!(kept.contains("Третий абзац"), "статью срезало:\n{kept}");
    }

    #[test]
    fn a_page_of_teasers_is_not_an_article() {
        let md = "# Блог компании\n\n## [Первая](https://e.com/1)\n\nанонс\n\n                  ## [Вторая](https://e.com/2)\n\nанонс\n\n## [Третья](https://e.com/3)\n\nанонс\n";
        assert!(matches!(strip_teasers(md), Teasers::Listing));
    }

    /// Главная блога — не ошибка и не пустая страница: читателю нужен
    /// список, по которому он уйдёт в статью.
    #[test]
    fn a_listing_is_shown_with_its_links_intact() {
        let article = Article {
            title: "Блог компании".to_owned(),
            byline: None,
            content_html: "<h2><a href=\"https://e.com/1\">Первая</a></h2><p>анонс</p>\
                <h2><a href=\"https://e.com/2\">Вторая</a></h2><p>анонс</p>\
                <h2><a href=\"https://e.com/3\">Третья</a></h2><p>анонс</p>"
                .to_owned(),
            thumbs: HashMap::new(),
            listing_html: None,
            notes: Default::default(),
        };

        let reading = from_article(&article).unwrap();
        assert_eq!(reading.kind, Kind::Listing);
        assert_eq!(links(&reading.markdown).len(), 3);
        assert!(reading.markdown.contains("Третья"));
    }

    #[test]
    fn an_empty_list_item_is_dropped() {
        let md = "# Статья\n\n## Новости\n\n-\n-\n-\n\nТекст.\n";
        assert_eq!(tidy(md), "# Статья\n\n## Новости\n\nТекст.\n");
    }

    #[test]
    fn a_rule_is_not_an_empty_item() {
        let md = "# Статья\n\nТекст.\n\n---\n\nЕщё текст.\n";
        assert_eq!(tidy(md), md);
    }

    /// Карточку ленты сайт помечает `aria-hidden`, и картинка до нас
    /// не доезжает. Ставим её обратно — перед заголовком её же записи.
    #[test]
    fn a_listing_gets_its_thumbnails_back() {
        let mut thumbs = HashMap::new();
        thumbs.insert(
            "https://e.com/2".to_owned(),
            "https://e.com/img/2.webp".to_owned(),
        );
        let article = Article {
            title: "Блог".to_owned(),
            byline: None,
            content_html: "<h2><a href=\"https://e.com/1\">Первая</a></h2><p>анонс</p>\
                <h2><a href=\"https://e.com/2\">Вторая</a></h2><p>анонс</p>\
                <h2><a href=\"https://e.com/3\">Третья</a></h2><p>анонс</p>"
                .to_owned(),
            thumbs,
            listing_html: None,
            notes: Default::default(),
        };

        let reading = from_article(&article).unwrap();
        let lines: Vec<&str> = reading.markdown.lines().collect();
        let picture = lines
            .iter()
            .position(|line| line.starts_with("![Вторая](https://e.com/img/2.webp)"))
            .expect("миниатюры нет");
        let heading = lines
            .iter()
            .position(|line| line.starts_with("## [Вторая]"))
            .expect("заголовка нет");
        assert!(picture < heading, "миниатюра должна стоять над записью");
        // Записям без миниатюры картинку не выдумываем.
        assert_eq!(images(&reading.markdown).len(), 1);
    }

    /// Статья миниатюр не получает: там это была бы отсебятина.
    #[test]
    fn an_article_gets_no_thumbnails() {
        let mut thumbs = HashMap::new();
        thumbs.insert(
            "https://e.com/1".to_owned(),
            "https://e.com/img/1.webp".to_owned(),
        );
        let article = Article {
            title: "Статья".to_owned(),
            byline: None,
            content_html: "<p>Первый абзац со <a href=\"https://e.com/1\">ссылкой</a>.</p>\
                <h2>Раздел</h2><p>Второй абзац.</p>"
                .to_owned(),
            thumbs,
            listing_html: None,
            notes: Default::default(),
        };

        let reading = from_article(&article).unwrap();
        assert_eq!(reading.kind, Kind::Article);
        assert!(images(&reading.markdown).is_empty());
    }

    /// Обычная статья остаётся статьёй: признак ленты не должен срабатывать
    /// на тексте со ссылками.
    #[test]
    fn an_article_is_still_an_article() {
        let article = Article {
            title: "Статья".to_owned(),
            byline: None,
            content_html: "<p>Первый абзац со <a href=\"https://e.com/1\">ссылкой</a>.</p>\
                <h2>Раздел</h2><p>Второй абзац.</p>"
                .to_owned(),
            thumbs: HashMap::new(),
            listing_html: None,
            notes: Default::default(),
        };

        let reading = from_article(&article).unwrap();
        assert_eq!(reading.kind, Kind::Article);
    }

    #[test]
    fn anchor_headings_are_not_teasers() {
        // Так размечены заголовки в документации Rust и на fasterthanli.me.
        let md = "# Статья\n\n## [Раздел](#section)\n\nтекст\n\n## [Другой](#other)\n\nтекст\n\n                  ## [Третий](#third)\n\nтекст\n";
        let Teasers::Article(kept) = strip_teasers(md) else {
            panic!("якоря приняли за анонсы");
        };
        assert_eq!(kept, md);
    }

    #[test]
    fn single_column_tables_become_paragraphs() {
        // Инфобокс википедии: рамка вёрстки, а не данные.
        let md =
            from_html("<table><tr><th>Rust</th></tr><tr><td>2012 год</td></tr></table>").unwrap();
        assert_eq!(md, "Rust\n\n2012 год\n");
    }

    #[test]
    fn a_lone_pipe_is_not_a_table() {
        // gamedeveloper.com отдавал строку из одной палки, и на ней всё падало.
        assert_eq!(tidy("текст\n|\nещё текст\n"), "текст\n|\nещё текст\n");
        assert!(split_cells("|").is_empty());
    }

    #[test]
    fn real_tables_stay_tables() {
        let md =
            from_html("<table><tr><th>а</th><th>б</th></tr><tr><td>1</td><td>2</td></tr></table>")
                .unwrap();
        assert!(md.contains("| а | б |"), "таблица развалилась:\n{md}");
    }

    #[test]
    fn pipes_inside_code_blocks_are_left_alone() {
        let md = from_html("<pre><code>| не | таблица |\n|  а  |  код  |</code></pre>").unwrap();
        assert_eq!(md, "```\n| не | таблица |\n|  а  |  код  |\n```\n");
    }

    #[test]
    fn blank_line_runs_collapse() {
        let md = from_html("<p>раз</p><div></div><div></div><p>два</p>").unwrap();
        assert_eq!(md, "раз\n\nдва\n");
    }

    #[test]
    fn links_are_collected_once_and_absolute_only() {
        let md = from_html(
            r#"<p><a href="https://e.com/a">1</a> <a href="https://e.com/a">1</a>
               <a href="/relative">2</a> <a href="mailto:x@e.com">3</a></p>"#,
        )
        .unwrap();
        assert_eq!(links(&md), vec!["https://e.com/a".to_owned()]);
    }

    #[test]
    fn scripts_are_dropped() {
        let md = from_html("<p>text</p><script>alert('x')</script>").unwrap();
        assert!(!md.contains("alert"), "скрипт просочился:\n{md}");
    }
}

#[cfg(test)]
mod tail_tests {
    use super::*;

    fn article(body: &str) -> String {
        match strip_teasers(body) {
            Teasers::Article(text) => strip_chrome(&text),
            Teasers::Listing => "СТАТЬИ НЕТ".to_owned(),
        }
    }

    #[test]
    fn image_link_to_a_page_in_the_tail_is_a_teaser() {
        assert!(is_teaser_paragraph(
            "[![](https://cdn.example.com/preview/1.png?w=600&h=300)](https://example.com/live/other.html)"
        ));
    }

    #[test]
    fn tail_grid_of_image_links_is_cut() {
        let doc = "# Заголовок\n\nПервый абзац статьи, достаточно длинный.\n\n[![](https://cdn.example.com/a.jpg?w=877)](https://cdn.example.com/a.jpg)\n\nВторой абзац статьи, тоже длинный и осмысленный.\n\n[![](https://cdn.example.com/p1.png?w=600)](https://example.com/live/one.html)\n\nАнонс первой чужой статьи.\n\n[![](https://cdn.example.com/p2.png?w=600)](https://example.com/live/two.html)\n\nАнонс второй чужой статьи.\n";
        let out = article(doc);
        assert!(
            out.contains("Второй абзац"),
            "тело статьи должно остаться:\n{out}"
        );
        assert!(!out.contains("one.html"), "анонсы должны уйти:\n{out}");
        // Картинка-ссылка на саму картинку — это увеличение, она остаётся.
        assert!(
            out.contains("a.jpg"),
            "увеличение картинки не трогаем:\n{out}"
        );
    }

    #[test]
    fn a_lone_image_link_is_not_a_grid() {
        let doc = "# Заголовок\n\nТекст статьи, вполне себе содержательный абзац.\n\n[![](https://cdn.example.com/promo.png)](https://example.com/promo)\n\nПодпись к врезке, которая на самом деле часть статьи.\n";
        let out = article(doc);
        assert!(
            out.contains("promo"),
            "одна картинка-ссылка — не сетка:\n{out}"
        );
    }

    #[test]
    fn repeated_paragraph_next_to_itself_is_dropped() {
        let doc = "# Заголовок\n\nЛид статьи, который сайт печатает дважды подряд.\n\n![](https://cdn.example.com/i.jpg)\n\nЛид статьи, который сайт печатает дважды подряд.\n";
        assert_eq!(article(doc).matches("Лид статьи").count(), 1);
    }

    #[test]
    fn a_far_away_repeat_is_content_not_chrome() {
        let far = "\n\n".to_owned()
            + &vec![
                "Прочий текст статьи, довольно длинная строка.";
                20
            ]
            .join("\n\n");
        let line = "Название курса, повторённое в двух разных списках страницы.";
        let doc = format!("# Заголовок\n\n{line}{far}\n\n{line}\n");
        assert_eq!(article(&doc).matches(line).count(), 2);
    }

    #[test]
    fn caption_repeating_the_alt_text_is_dropped() {
        let caption = "Это Тристан Бакмастер, математик, и он очень зол";
        let doc = format!(
            "# Заголовок\n\nАбзац статьи, достаточно длинный для проверки.\n\n![{caption}](https://cdn.example.com/i.jpg)\n\n{caption}\n\nСледующий абзац статьи.\n"
        );
        let out = article(&doc);
        assert_eq!(
            out.matches(caption).count(),
            1,
            "подпись должна остаться одна:\n{out}"
        );
        assert!(out.contains("!["), "сама картинка остаётся:\n{out}");
    }

    #[test]
    fn a_caption_with_hard_spaces_still_matches() {
        // habr ставит неразрывные пробелы в тексте и обычные в alt.
        let alt = "Это Тристан Бакмастер, математик, и он очень зол";
        let caption = alt.replace(' ', "\u{a0}");
        let doc = format!(
            "# Заголовок\n\nАбзац статьи, достаточно длинный для проверки.\n\n![{alt}](https://cdn.example.com/i.jpg)\n\n{caption}\n\nСледующий абзац статьи.\n"
        );
        let out = article(&doc);
        assert!(
            !out.contains(&caption),
            "подпись с неразрывными пробелами должна уйти:\n{out}"
        );
    }

    #[test]
    fn a_short_caption_is_left_alone() {
        let doc = "# Заголовок\n\nАбзац статьи, достаточно длинный для проверки.\n\n![Схема](https://cdn.example.com/i.jpg)\n\nСхема\n";
        assert_eq!(article(doc).matches("Схема").count(), 2);
    }

    #[test]
    fn tail_notice_is_dropped() {
        let doc = "# Заголовок\n\nПоследний абзац статьи, вполне осмысленный и длинный.\n\nНашли ошибку в тексте — выделите её и нажмите Ctrl+Enter.\n";
        let out = article(doc);
        assert!(
            !out.contains("Ctrl+Enter"),
            "хвостовое уведомление должно уйти:\n{out}"
        );
        assert!(out.contains("Последний абзац"));
    }

    #[test]
    fn image_link_to_an_image_is_a_zoom_not_a_teaser() {
        assert!(!is_teaser_paragraph(
            "[![](https://cdn.example.com/small.jpg?w=877)](https://cdn.example.com/original.jpg)"
        ));
    }
}
