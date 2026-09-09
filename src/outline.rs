//! Оглавление статьи и типографская модель, по которой оно считается.
//!
//! Живёт в ядре, а не в интерфейсе: разбор заголовков и выбор вех от тулкита
//! не зависят, а любой второй интерфейс получит их готовыми и с тестами.


/// Типографика. Числа связаны между собой, поэтому и живут вместе: мера
/// задана в кеглях, а не в пикселях, чтобы при смене размера шрифта строка
/// оставалась той же длины в знаках. Тридцать три кегля — это около
/// 65 знаков при средней ширине знака примерно в половину кегля.
pub const TEXT_SIZE: f32 = 16.5;
/// Мера. У гротеска знак шире, чем у антиквы, и прежние тридцать три кегля
/// давали строку в неполные шестьдесят знаков — коротко.
pub const MEASURE_IN_EMS: f32 = 36.0;
pub const MEASURE: f32 = TEXT_SIZE * MEASURE_IN_EMS;

/// Межстрочный интервал. Умолчание iced (1.3) собрано для интерфейса,
/// где строки короткие; на мере в 65 знаков глаз на обратном ходе
/// соскакивает на соседнюю строку. Полтора с небольшим — книжная норма.
pub const LINE_HEIGHT: f32 = 1.55;
/// В заголовке строки короткие, и полуторный интервал разваливает его
/// на отдельные строки. Плотнее.
pub const HEADING_LINE_HEIGHT: f32 = 1.15;

/// Шкала заголовков в долях кегля. Умолчание iced — вдвое на первом уровне
/// и минус четверть на каждом следующем; на экране это даёт заголовок
/// в 34 пункта, который спорит с текстом, а не ведёт к нему.
pub const HEADINGS: [f32; 6] = [1.75, 1.45, 1.28, 1.14, 1.05, 1.0];

/// Вес заголовков по уровням, в единицах Pango.
///
/// Крупные — светлые: на большом кегле жир кричит, а не ведёт, и в книжной
/// вёрстке крупный заголовок всегда светлее, чем кажется. Мелкие, наоборот,
/// требуют веса — иначе они ничем не отличаются от текста. Веса взяты те,
/// что есть в комплекте: 300, 400, 500 и 700.
pub const HEADING_WEIGHTS: [i32; 6] = [300, 300, 400, 500, 500, 700];

/// Оглавление показываем, только если оно что-то даёт.
pub const MIN_HEADINGS: usize = 3;

/// Ниже этой высоты оглавление не нужно: страница и так вся под рукой.
pub const MIN_DOC_HEIGHT: f32 = 1800.0;
/// Через сколько высоты ставить веху, когда заголовков в статье нет.
pub const WAYPOINT_EVERY: f32 = 900.0;
pub const MAX_WAYPOINTS: usize = 14;

/// Заголовок в оглавлении и его место в документе — долей от полной высоты.
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub level: u8,
    pub title: String,
    /// Доля от полной высоты — для тулкитов, которые умеют только её.
    pub at: f32,
    /// Смещение заголовка в исходном markdown, в байтах. Тулкиту,
    /// который умеет метки в тексте, доля не нужна — нужно это.
    pub offset: usize,
}

/// Якорь заголовка — то, чем он назван в ссылке `#…`.
///
/// Генераторы статических сайтов лепят якорь из текста заголовка по правилу
/// github: в нижний регистр, пробелы в дефисы, знаки препинания долой.
/// Точного стандарта нет, и мелкие расхождения между генераторами (два дефиса
/// подряд там, где другой поставит один) лечатся тем, что через эту же функцию
/// пропускается и то, что написано в ссылке: сравниваются не строки, а их
/// приведённые виды.
///
/// Буквы не только латинские: кириллический якорь в ссылке приезжает
/// процентными кодами, поэтому сначала раскодируем.
pub fn anchor(text: &str) -> String {
    let text = percent_decode(text);
    let mut out = String::with_capacity(text.len());

    for ch in text.chars() {
        if ch.is_alphanumeric() || ch == '_' {
            out.extend(ch.to_lowercase());
        } else if (ch == '-' || ch.is_whitespace()) && !out.ends_with('-') {
            out.push('-');
        }
    }
    out.trim_matches('-').to_owned()
}

/// `%D0%BF` → байты. Своё, потому что ради десяти строк тащить крейт незачем,
/// а `url` наружу декодер не отдаёт.
fn percent_decode(text: &str) -> String {
    if !text.contains('%') {
        return text.to_owned();
    }
    let bytes = text.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;

    while i < bytes.len() {
        match (bytes[i], bytes.get(i + 1), bytes.get(i + 2)) {
            (b'%', Some(&high), Some(&low)) => match hex(high).zip(hex(low)) {
                Some((high, low)) => {
                    out.push(high * 16 + low);
                    i += 3;
                }
                None => {
                    out.push(bytes[i]);
                    i += 1;
                }
            },
            _ => {
                out.push(bytes[i]);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn hex(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

/// Сколько знаков влезает в строку на нашей мере.
const CHARS_PER_LINE: f32 = MEASURE_IN_EMS * 2.0;
/// Высота блока-картинки: кнопка-заглушка плюс подпись.
const IMAGE_BLOCK: f32 = 90.0;

/// Блок разметки с его местом по высоте.
struct Block {
    at: f32,
    offset: usize,
    kind: Kind,
}

enum Kind {
    Heading { level: u8, title: String },
    Paragraph { lead: String },
}

/// Оглавление статьи.
///
/// Заголовки есть не везде: половина статей в вебе — сплошной текст без
/// единого `##`. Оглавление там всё равно нужно, иначе длинную статью
/// не с чего листать, — только вехами служат не заголовки, а начала
/// абзацев, расставленные примерно через экран. На короткой странице
/// не нужно ни то, ни другое: она и так вся под рукой.
///
/// Прыгать по документу iced умеет только долями от полной высоты
/// (`snap_to`), а спросить, где лежит виджет, нечем: `visible_bounds`
/// в 0.14 нет. Поэтому высоту считаем сами по разметке — так же, как её
/// потом разложит рендерер. Это оценка, а не измерение; систематическая
/// ошибка масштаба безвредна, она сокращается в доле.
pub fn outline(source: &str) -> Vec<Entry> {
    let (blocks, height) = scan(source);

    if height < MIN_DOC_HEIGHT {
        return Vec::new();
    }

    let mut headings: Vec<Entry> = blocks
        .iter()
        .filter_map(|block| match &block.kind {
            Kind::Heading { level, title } => Some(Entry {
                level: *level,
                title: title.clone(),
                at: block.at,
                offset: block.offset,
            }),
            Kind::Paragraph { .. } => None,
        })
        .collect();

    // Название статьи — не раздел: оно и так наверху, и в счёт разделов
    // не идёт. Иначе статья с двумя разделами считалась бы за три.
    if matches!(headings.first(), Some(first) if first.level == 1 && first.at == 0.0) {
        headings.remove(0);
    }

    let entries = if headings.len() >= MIN_HEADINGS {
        headings
    } else {
        waypoints(&blocks, height)
    };

    into_fractions(entries, height)
}

/// Перевести высоты в доли от полной.
fn into_fractions(mut entries: Vec<Entry>, height: f32) -> Vec<Entry> {
    let total = height.max(1.0);
    for entry in &mut entries {
        entry.at = (entry.at / total).clamp(0.0, 1.0);
    }
    entries
}

/// Вехи по началам абзацев — примерно через экран.
fn waypoints(blocks: &[Block], height: f32) -> Vec<Entry> {
    let leads: Vec<&Block> = blocks
        .iter()
        .filter(|block| matches!(block.kind, Kind::Paragraph { .. }))
        .collect();
    if leads.is_empty() {
        return Vec::new();
    }

    let wanted = ((height / WAYPOINT_EVERY).round() as usize).clamp(2, MAX_WAYPOINTS);
    let mut entries: Vec<Entry> = Vec::with_capacity(wanted);
    let mut taken = 0usize;

    for step in 0..wanted {
        let target = height * (step as f32 + 0.5) / wanted as f32;
        // Ближайший абзац к цели, но не тот, что уже взяли.
        let Some((index, block)) = leads
            .iter()
            .enumerate()
            .skip(taken)
            .min_by(|(_, a), (_, b)| {
                (a.at - target)
                    .abs()
                    .total_cmp(&(b.at - target).abs())
            })
        else {
            break;
        };
        taken = index + 1;

        if let Kind::Paragraph { lead } = &block.kind {
            entries.push(Entry {
                level: 1,
                title: lead.clone(),
                at: block.at,
                offset: block.offset,
            });
        }
    }
    entries
}

/// Разложить разметку на блоки и посчитать высоту так, как её разложит
/// рендерер: абзац занимает столько строк, сколько знаков не влезло
/// в меру, у кода строка своя, у картинки — фиксированный блок.
fn scan(source: &str) -> (Vec<Block>, f32) {
    let line_px = TEXT_SIZE * LINE_HEIGHT;
    let gap = TEXT_SIZE * 0.95;
    let code_line = TEXT_SIZE * 0.88 * 1.35;

    let mut blocks = Vec::new();
    let mut height = 0.0f32;
    let mut in_code = false;
    let mut offset = 0usize;

    // `lines()` теряет смещения, а они нужны тулкиту с метками в тексте,
    // поэтому идём по строкам вместе с переводами.
    for raw in source.split_inclusive('\n') {
        let here = offset;
        offset += raw.len();
        let text = raw.trim();

        if text.starts_with("```") {
            in_code = !in_code;
            height += code_line;
            continue;
        }
        if in_code {
            height += code_line;
            continue;
        }
        if text.is_empty() {
            height += gap;
            continue;
        }
        if let Some((level, title)) = heading(text) {
            blocks.push(Block {
                at: height,
                offset: here,
                kind: Kind::Heading { level, title },
            });
            height += TEXT_SIZE * HEADINGS[usize::from(level - 1)] * HEADING_LINE_HEIGHT
                + TEXT_SIZE * 1.4;
            continue;
        }
        if text.starts_with("![") {
            height += IMAGE_BLOCK;
            continue;
        }

        let rows = (text.chars().count() as f32 / CHARS_PER_LINE).ceil().max(1.0);
        if rows >= 2.0 {
            // Вехой может быть только настоящий абзац, а не строка списка
            // или подпись: у коротких строк начало ничего не говорит.
            blocks.push(Block {
                at: height,
                offset: here,
                kind: Kind::Paragraph { lead: lead(text) },
            });
        }
        height += rows * line_px;
    }

    (blocks, height)
}

/// Начало абзаца как подпись к вехе: до первой границы слова после сорока
/// знаков. Смысл в том, чтобы читатель узнал место, а не прочитал абзац.
pub fn lead(text: &str) -> String {
    let plain = plain(text);
    let mut out = String::new();

    for word in plain.split_whitespace() {
        if out.chars().count() + word.chars().count() > 40 {
            out.push('…');
            return out;
        }
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(word);
    }
    out
}

/// `## Заголовок` → уровень и текст без разметки.
fn heading(line: &str) -> Option<(u8, String)> {
    let level = line.chars().take_while(|c| *c == '#').count();
    if level == 0 || level > 6 {
        return None;
    }
    let rest = line[level..].strip_prefix(' ')?.trim();
    (!rest.is_empty()).then(|| (level as u8, plain(rest)))
}

/// Снять разметку с текста заголовка: в оглавлении нужен только он сам.
fn plain(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    let mut depth = 0usize;

    while let Some(c) = chars.next() {
        match c {
            // `[текст](ссылка)` — оставляем текст, адрес выбрасываем.
            '[' => {}
            ']' if chars.peek() == Some(&'(') => {
                depth = 1;
                chars.next();
            }
            '(' if depth > 0 => depth += 1,
            ')' if depth > 0 => depth -= 1,
            _ if depth > 0 => {}
            '*' | '_' | '`' => {}
            '\\' => {
                if let Some(escaped) = chars.next() {
                    out.push(escaped);
                }
            }
            _ => out.push(c),
        }
    }
    out.trim().to_owned()
}


/// Обрезать подпись по границе знака, с многоточием.
pub fn clip(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_owned();
    }
    let mut out: String = text.chars().take(max.saturating_sub(1)).collect();
    out = out.trim_end().to_owned();
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Достаточно длинный кусок текста, чтобы страница не считалась короткой.
    fn filler() -> String {
        let para = "Длинный абзац статьи, в котором знаков хватает на несколько \
строк нашей меры, иначе страница выйдет короткой и оглавления не получит.";
        vec![para; 12].join("\n\n")
    }

    #[test]
    fn outline_skips_the_article_title() {
        let text = filler();
        let doc = format!(
            "# Название статьи\n\n{text}\n\n## Первый\n\n{text}\n\n## Второй\n\n{text}\n\n## Третий\n"
        );
        let entries = outline(&doc);
        assert_eq!(entries.len(), 3, "название в оглавление не идёт: {entries:?}");
        assert_eq!(entries[0].title, "Первый");
        assert_eq!(entries[0].level, 2);
    }

    #[test]
    fn outline_places_headings_in_order() {
        let text = filler();
        let doc = format!(
            "# Название\n\n{text}\n\n## Начало\n\n{text}\n\n## Середина\n\n{text}\n\n## Конец\n\n{text}\n"
        );
        let entries = outline(&doc);
        assert_eq!(entries.len(), 3, "{entries:?}");
        assert!(entries.windows(2).all(|w| w[0].at < w[1].at), "порядок нарушен: {entries:?}");
        let middle = entries[1].at;
        assert!((0.35..0.7).contains(&middle), "середина не в середине: {middle}");
        assert!(entries.iter().all(|e| (0.0..=1.0).contains(&e.at)));
    }

    #[test]
    fn heading_text_loses_its_markup() {
        assert_eq!(
            heading("## [Ссылка](https://example.com) и **жирное**"),
            Some((2, "Ссылка и жирное".to_owned()))
        );
        assert_eq!(heading("#Не заголовок"), None);
        assert_eq!(heading("Обычный текст"), None);
    }

    #[test]
    fn code_fences_do_not_become_headings() {
        let text = filler();
        let doc = format!(
            "# Название\n\n```\n# это комментарий, а не заголовок\n```\n\n\
## Первый\n\n{text}\n\n## Второй\n\n{text}\n\n## Третий\n\n{text}\n"
        );
        let entries = outline(&doc);
        assert_eq!(entries.len(), 3, "комментарий в коде — не заголовок: {entries:?}");
        assert!(entries.iter().all(|e| e.title != "это комментарий, а не заголовок"));
    }

    #[test]
    fn a_short_page_gets_no_contents() {
        let doc = "# Заметка\n\nОдин абзац, и на этом всё.\n";
        assert!(outline(doc).is_empty(), "короткой странице оглавление не нужно");
    }

    #[test]
    fn a_long_page_without_headings_gets_waypoints() {
        let para = "Длинный абзац статьи, в котором достаточно знаков, чтобы \
он занял несколько строк на нашей мере и попал в разметку вехой.";
        let doc = format!("# Название\n\n{}\n", vec![para; 40].join("\n\n"));
        let entries = outline(&doc);
        assert!(entries.len() >= 2, "вехи должны появиться: {}", entries.len());
        assert!(entries.len() <= MAX_WAYPOINTS);
        assert!(entries.windows(2).all(|w| w[0].at <= w[1].at), "порядок вех нарушен");
        assert!(entries.iter().all(|e| !e.title.is_empty()));
    }

    #[test]
    fn a_waypoint_label_is_short() {
        let long = "Это очень длинное начало абзаца, которое ни в какое оглавление целиком не влезет";
        let label = lead(long);
        assert!(label.chars().count() <= 42, "подпись слишком длинная: {label:?}");
        assert!(label.ends_with('…'));
    }

    #[test]
    fn an_anchor_is_built_like_github_builds_it() {
        assert_eq!(anchor("What the week was about"), "what-the-week-was-about");
        assert_eq!(anchor("Don't panic!"), "dont-panic");
        assert_eq!(anchor("  Method  "), "method");
    }

    #[test]
    fn the_link_and_the_heading_meet_in_the_middle() {
        // Слева — то, что стоит в href, справа — заголовок статьи.
        assert_eq!(anchor("#a--b"), anchor("A — B"));
        assert_eq!(anchor("%D0%9F%D0%BE%D0%B4%D0%B2%D0%BE%D0%B4%D0%BD%D1%8B%D0%B5"), "подводные");
    }

    #[test]
    fn a_long_title_fits_the_tab() {
        let clipped = clip("Announcing Rust 1.81.0 | Rust Blog", 24);
        assert!(clipped.chars().count() <= 24, "{clipped:?}");
        assert!(clipped.ends_with('…'));
        assert_eq!(clip("Коротко", 24), "Коротко");
    }
}
