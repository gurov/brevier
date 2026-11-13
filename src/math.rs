//! Формула своими силами (#16): MathML страницы — текстом или рисунком.
//!
//! MathML, прочитанный как текст, врёт: дробь ½ выходила числом «12»,
//! а «a² + b²» — «a2+b2». При этом он есть почти везде, где есть формула:
//! KaTeX, arXiv и Википедия кладут его в страницу, и это единственная
//! разметка, где у формулы видна структура, а не только исходник TeX.
//!
//! Как в книге, два вида. Что читается в строку — буквы, числа, индексы —
//! остаётся текстом в наборе читателя: его можно найти, скопировать,
//! подчеркнуть (`spelled`). Всё двумерное — дроби, корни, пределы, матрицы —
//! рисуется здесь же, без сети: `formulary` раскладывает MathML по таблице
//! MATH шрифта, и рисунок идёт дальше обычной картинкой (`drawn`).
//! На 630 формулах четырёх статей первых вышло около 80%.

use std::borrow::Cow;

use dom_query::NodeRef;

/// Адрес нарисованной формулы: обычный `data:` — копия в архиве и файл `.md`
/// не зависят от нас, — но с пометкой, по которой `media` узнаёт своё.
pub const DRAWN: &str = "data:image/svg+xml;formula=1;base64,";

/// Формула текстом: HTML с `<em>`, `<sup>` и `<sub>`, либо `None`, если
/// в строку она не ложится.
pub fn spelled(math: &NodeRef) -> Option<String> {
    let mut speller = Speller::default();
    speller.row(math)?;
    let text = speller.out.trim();
    (!text.is_empty()).then(|| text.to_owned())
}

/// Что стояло перед знаком: от этого зависит, унарный ли он.
#[derive(Default, Clone, Copy, PartialEq)]
enum Before {
    #[default]
    Start,
    Operand,
    Operator,
}

#[derive(Default)]
struct Speller {
    out: String,
    before: Before,
    /// Внутри индекса: знаки без пробелов, а индекс у индекса строка не выдержит.
    script: bool,
}

/// Знаки, которые в наборе отбивают пробелами.
const SPACED: &str = "=≠<>≤≥≈≡≅∼∝≪≫→←↔⇒⇐⇔↦∈∉∋⊂⊃⊆⊇∪∩∧∨+−±∓×÷⋅·∘⊕⊗";
/// Унарными бывают только эти.
const UNARY: &str = "+−±∓";
const OPENING: &str = "([{⟨⌊⌈";
const CLOSING: &str = ")]}⟩⌋⌉";
/// Знаки с пределами: `∑` с индексами сбоку — уже не то, что написал автор.
const BIG: &str = "∑∏∐∫∬∭∮⋃⋂⋁⋀⨁⨂";
/// Верхний индекс, который в наборе стоит вровень: штрих не поднимают.
const PRIMES: &str = "′″‴⁗*∗†‡";

impl Speller {
    fn row(&mut self, node: &NodeRef) -> Option<()> {
        for child in node.element_children() {
            self.node(&child)?;
        }
        Some(())
    }

    fn node(&mut self, node: &NodeRef) -> Option<()> {
        let name = node.node_name()?;
        match name.as_ref() {
            "mrow" | "mstyle" | "mpadded" => self.row(node),
            "semantics" => self.node(&node.first_element_child()?),
            "annotation" | "annotation-xml" => Some(()),
            "mi" => self.identifier(node),
            "mn" => {
                self.put(&escape(node.text().trim()));
                self.before = Before::Operand;
                Some(())
            }
            "mtext" => {
                self.put(&escape(&node.text().replace('\u{a0}', " ")));
                self.before = Before::Operand;
                Some(())
            }
            "mo" => self.operator(node),
            "mspace" => {
                if wide(node.attr("width").as_deref()) && !self.out.ends_with(' ') {
                    self.out.push(' ');
                }
                Some(())
            }
            "msup" | "msub" | "msubsup" => self.scripts(node, name.as_ref()),
            _ => None,
        }
    }

    fn put(&mut self, text: &str) {
        self.out.push_str(text);
    }

    fn identifier(&mut self, node: &NodeRef) -> Option<()> {
        let text = node.text();
        let text = text.trim();
        let letter = text.chars().count() == 1 && text.chars().all(char::is_alphabetic);
        let shown = escape(text);
        match node.attr("mathvariant").as_deref() {
            None | Some("italic") if letter => self.italic(&shown),
            None | Some("normal") => self.put(&shown),
            Some("italic") => self.italic(&shown),
            Some("bold") => self.put(&format!("<strong>{shown}</strong>")),
            // Готические, ажурные и прочие буквы строка не наберёт.
            _ => return None,
        }
        self.before = Before::Operand;
        Some(())
    }

    /// Соседние буквы — одним курсивом: `*a**b*` markdown прочитал бы не так.
    fn italic(&mut self, text: &str) {
        match self.out.strip_suffix("</em>") {
            Some(rest) => {
                let keep = rest.len();
                self.out.truncate(keep);
                self.out.push_str(text);
                self.out.push_str("</em>");
            }
            None => self.put(&format!("<em>{text}</em>")),
        }
    }

    fn operator(&mut self, node: &NodeRef) -> Option<()> {
        let text = node.text();
        let text = text.trim();
        let text = if text == "-" { "−" } else { text };
        match text {
            // Невидимое умножение и применение функции.
            "" | "\u{2061}" | "\u{2062}" | "\u{2063}" | "\u{2064}" => {}
            "," | ";" => {
                self.put(text);
                if !self.script {
                    self.out.push(' ');
                }
                self.before = Before::Operator;
            }
            _ if OPENING.contains(text) => {
                self.put(text);
                self.before = Before::Start;
            }
            _ if CLOSING.contains(text) => {
                self.put(text);
                self.before = Before::Operand;
            }
            _ if SPACED.contains(text) => {
                let unary = self.before != Before::Operand && UNARY.contains(text);
                let shown = escape(text);
                if unary || self.script {
                    self.put(&shown);
                } else {
                    if !self.out.ends_with(' ') {
                        self.out.push(' ');
                    }
                    self.put(&shown);
                    self.out.push(' ');
                }
                self.before = Before::Operator;
            }
            _ => {
                self.put(&escape(text));
                self.before = Before::Operand;
            }
        }
        Some(())
    }

    fn scripts(&mut self, node: &NodeRef, name: &str) -> Option<()> {
        if self.script {
            return None;
        }
        let kids = node.element_children();
        let (base, tags): (&NodeRef, &[(&str, &NodeRef)]) = match (name, kids.as_slice()) {
            ("msup", [base, top]) => (base, &[("sup", top)]),
            ("msub", [base, bottom]) => (base, &[("sub", bottom)]),
            ("msubsup", [base, bottom, top]) => (base, &[("sub", bottom), ("sup", top)]),
            _ => return None,
        };
        if BIG.contains(base.text().trim()) && !base.text().trim().is_empty() {
            return None;
        }
        self.node(base)?;
        for (tag, script) in tags {
            let text = script.text();
            let text = text.trim();
            if *tag == "sup" && !text.is_empty() && text.chars().all(|sign| PRIMES.contains(sign)) {
                self.put(&escape(text));
                continue;
            }
            self.put(&format!("<{tag}>"));
            self.script = true;
            self.before = Before::Start;
            self.node(script)?;
            self.script = false;
            self.put(&format!("</{tag}>"));
        }
        self.before = Before::Operand;
        Some(())
    }
}

/// Шире тонкой шпации: такой пробел в тексте заметен, меньшие — кернинг.
fn wide(width: Option<&str>) -> bool {
    let Some(width) = width else { return false };
    let number: String = width
        .chars()
        .take_while(|sign| sign.is_ascii_digit() || *sign == '.')
        .collect();
    number.parse::<f32>().is_ok_and(|em| em >= 0.25) && width.ends_with("em")
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Исходник формулы без обёртки: MathJax заворачивает его в `{\displaystyle …}`,
/// и читателю она не нужна.
pub fn bare(source: &str) -> &str {
    let text = source.trim();
    text.strip_prefix('{')
        .and_then(|text| text.strip_suffix('}'))
        .map(str::trim)
        .and_then(|text| text.strip_prefix("\\displaystyle").or(Some(text)))
        .unwrap_or(text)
        .trim()
}

/// Markdown для чтения снаружи: cli и файл `.md`. Нарисованная формула там
/// стала бы мегабайтами base64 в `less` — уходит исходником `$…$`, который
/// понимают GitHub, Obsidian и Pandoc.
pub fn exported(md: &str) -> Cow<'_, str> {
    let opening = format!("]({DRAWN}");
    if !md.contains(&opening) {
        return Cow::Borrowed(md);
    }
    let mut out = String::with_capacity(md.len());
    let mut rest = md;
    while let Some(at) = rest.find(&opening) {
        let Some(start) = rest[..at].rfind("![") else {
            break;
        };
        let Some(length) = rest[at..].find(')') else {
            break;
        };
        out.push_str(&rest[..start]);
        let source = unescaped(&rest[start + 2..at]);
        let source = bare(&source);
        if source.is_empty() {
            out.push_str("[formula]");
        } else {
            out.push('$');
            out.push_str(source);
            out.push('$');
        }
        rest = &rest[at + length + 1..];
    }
    out.push_str(rest);
    Cow::Owned(out)
}

/// Подпись картинки в markdown экранирована: `\\frac`, `\_`, `\[`.
fn unescaped(alt: &str) -> String {
    let mut out = String::with_capacity(alt.len());
    let mut signs = alt.chars().peekable();
    while let Some(sign) = signs.next() {
        match (sign, signs.peek()) {
            ('\\', Some(next)) if next.is_ascii_punctuation() => {
                out.push(*next);
                signs.next();
            }
            _ => out.push(sign),
        }
    }
    out
}

/// Что видно на нарисованной формуле — строкой, для поиска по странице.
/// От формулы остаётся только исходник TeX, а искать по нему как есть нельзя:
/// «the» находило бы `\theta`, «a» — каждую `\frac`. Поэтому команды уходят,
/// греческие буквы и знаки встают своими символами, имена функций — как
/// набраны, а пробелы вокруг знаков — как у формулы текстом (`spelled`):
/// нарисованная находится теми же словами, что нашли бы набранную, —
/// «a2 + b2», «sin», «θ».
pub fn seen(source: &str) -> String {
    let mut seer = Seer::default();
    let mut tokens = tex_tokens(bare(source)).into_iter().peekable();
    seer.sequence(&mut tokens, false);
    seer.out.trim().to_owned()
}

#[derive(Debug, Clone, PartialEq)]
enum Tex {
    /// `\name` — буквы; или `\` и один знак: `\{`, `\,`, `\\`.
    Command(String),
    Open,
    Close,
    Up,
    Down,
    Sign(char),
}

fn tex_tokens(source: &str) -> Vec<Tex> {
    let mut out = Vec::new();
    let mut signs = source.chars().peekable();
    while let Some(sign) = signs.next() {
        match sign {
            '\\' => {
                let mut name = String::new();
                while let Some(&next) = signs.peek() {
                    if !next.is_ascii_alphabetic() {
                        break;
                    }
                    name.push(next);
                    signs.next();
                }
                if name.is_empty()
                    && let Some(next) = signs.next()
                {
                    name.push(next);
                }
                out.push(Tex::Command(name));
            }
            '{' => out.push(Tex::Open),
            '}' => out.push(Tex::Close),
            '^' => out.push(Tex::Up),
            '_' => out.push(Tex::Down),
            // В формуле TeX пробелы не значат ничего.
            _ if sign.is_whitespace() => {}
            _ => out.push(Tex::Sign(sign)),
        }
    }
    out
}

/// Чтение TeX в строку — с теми же правилами, что `Speller` для MathML.
#[derive(Default)]
struct Seer {
    out: String,
    before: Before,
    script: bool,
}

type Tokens = std::iter::Peekable<std::vec::IntoIter<Tex>>;

impl Seer {
    /// До закрывающей скобки группы (`group`) или до конца.
    fn sequence(&mut self, tokens: &mut Tokens, group: bool) {
        while let Some(token) = tokens.next() {
            if token == Tex::Close {
                if group {
                    return;
                }
                continue;
            }
            self.token(token, tokens);
        }
    }

    fn token(&mut self, token: Tex, tokens: &mut Tokens) {
        match token {
            Tex::Open => self.sequence(tokens, true),
            Tex::Close => {}
            Tex::Up | Tex::Down => {
                let outer = self.script;
                self.script = true;
                self.before = Before::Start;
                if let Some(next) = tokens.next() {
                    self.token(next, tokens);
                }
                self.script = outer;
                self.before = Before::Operand;
            }
            Tex::Sign(sign) => self.sign(sign),
            Tex::Command(name) => self.command(&name, tokens),
        }
    }

    fn command(&mut self, name: &str, tokens: &mut Tokens) {
        match name {
            // Имя окружения, цвет, метка — не то, что видно.
            "begin" | "end" | "color" | "textcolor" | "label" | "tag" | "hspace" | "vspace" => {
                skip_group(tokens);
            }
            // Размер скобки — не скобка; `\left.` — пустая.
            "left" | "right" | "big" | "Big" | "bigg" | "Bigg" | "bigl" | "bigr" | "Bigl"
            | "Bigr" | "biggl" | "biggr" | "Biggl" | "Biggr" => {
                if tokens.peek() == Some(&Tex::Sign('.')) {
                    tokens.next();
                }
            }
            "\\" | "quad" | "qquad" | " " => self.gap(),
            "{" => self.sign('{'),
            "}" => self.sign('}'),
            "%" | "$" | "#" | "&" | "_" => self.operand(name),
            "|" => self.operand("‖"),
            _ if FUNCTIONS.contains(&name) => self.operand(name),
            // Прочие команды — оформление: `\frac`, `\mathbf`, `\hat`, отбивки
            // `\,` и `\;`; их группы читаются дальше как есть.
            _ => {
                if let Some(sign) = symbol(name) {
                    self.sign(sign);
                }
            }
        }
    }

    fn gap(&mut self) {
        if !self.out.is_empty() && !self.out.ends_with(' ') {
            self.out.push(' ');
        }
        self.before = Before::Start;
    }

    fn operand(&mut self, text: &str) {
        self.out.push_str(text);
        self.before = Before::Operand;
    }

    fn sign(&mut self, sign: char) {
        let sign = match sign {
            '-' => '−',
            '\'' => '′',
            _ => sign,
        };
        match sign {
            '&' | '~' => self.gap(),
            ',' | ';' => {
                self.out.push(sign);
                if !self.script {
                    self.out.push(' ');
                }
                self.before = Before::Operator;
            }
            _ if OPENING.contains(sign) => {
                self.out.push(sign);
                self.before = Before::Start;
            }
            _ if CLOSING.contains(sign) => self.operand(&sign.to_string()),
            _ if SPACED.contains(sign) => {
                let unary = self.before != Before::Operand && UNARY.contains(sign);
                if unary || self.script {
                    self.out.push(sign);
                } else {
                    if !self.out.is_empty() && !self.out.ends_with(' ') {
                        self.out.push(' ');
                    }
                    self.out.push(sign);
                    self.out.push(' ');
                }
                self.before = Before::Operator;
            }
            _ => self.operand(&sign.to_string()),
        }
    }
}

/// Пропустить группу `{…}` целиком, если она следующая.
fn skip_group(tokens: &mut Tokens) {
    if tokens.peek() != Some(&Tex::Open) {
        return;
    }
    tokens.next();
    let mut depth = 1;
    for token in tokens.by_ref() {
        match token {
            Tex::Open => depth += 1,
            Tex::Close => {
                depth -= 1;
                if depth == 0 {
                    return;
                }
            }
            _ => {}
        }
    }
}

/// Имена функций: на рисунке они набраны буквами.
const FUNCTIONS: &[&str] = &[
    "sin", "cos", "tan", "cot", "sec", "csc", "arcsin", "arccos", "arctan", "sinh", "cosh", "tanh",
    "coth", "log", "ln", "lg", "exp", "lim", "limsup", "liminf", "max", "min", "sup", "inf", "det",
    "dim", "ker", "deg", "gcd", "lcm", "arg", "hom", "mod", "bmod", "Pr",
];

/// Буква или знак команды TeX — так, как его рисует формула.
fn symbol(name: &str) -> Option<char> {
    const GREEK: &[(&str, char)] = &[
        ("alpha", 'α'),
        ("beta", 'β'),
        ("gamma", 'γ'),
        ("delta", 'δ'),
        ("epsilon", 'ϵ'),
        ("varepsilon", 'ε'),
        ("zeta", 'ζ'),
        ("eta", 'η'),
        ("theta", 'θ'),
        ("vartheta", 'ϑ'),
        ("iota", 'ι'),
        ("kappa", 'κ'),
        ("lambda", 'λ'),
        ("mu", 'μ'),
        ("nu", 'ν'),
        ("xi", 'ξ'),
        ("pi", 'π'),
        ("varpi", 'ϖ'),
        ("rho", 'ρ'),
        ("varrho", 'ϱ'),
        ("sigma", 'σ'),
        ("varsigma", 'ς'),
        ("tau", 'τ'),
        ("upsilon", 'υ'),
        ("phi", 'ϕ'),
        ("varphi", 'φ'),
        ("chi", 'χ'),
        ("psi", 'ψ'),
        ("omega", 'ω'),
        ("Gamma", 'Γ'),
        ("Delta", 'Δ'),
        ("Theta", 'Θ'),
        ("Lambda", 'Λ'),
        ("Xi", 'Ξ'),
        ("Pi", 'Π'),
        ("Sigma", 'Σ'),
        ("Upsilon", 'Υ'),
        ("Phi", 'Φ'),
        ("Psi", 'Ψ'),
        ("Omega", 'Ω'),
    ];
    const SIGNS: &[(&str, char)] = &[
        ("cdot", '⋅'),
        ("times", '×'),
        ("div", '÷'),
        ("pm", '±'),
        ("mp", '∓'),
        ("le", '≤'),
        ("leq", '≤'),
        ("ge", '≥'),
        ("geq", '≥'),
        ("ne", '≠'),
        ("neq", '≠'),
        ("approx", '≈'),
        ("equiv", '≡'),
        ("cong", '≅'),
        ("sim", '∼'),
        ("propto", '∝'),
        ("ll", '≪'),
        ("gg", '≫'),
        ("to", '→'),
        ("rightarrow", '→'),
        ("leftarrow", '←'),
        ("gets", '←'),
        ("leftrightarrow", '↔'),
        ("Rightarrow", '⇒'),
        ("implies", '⇒'),
        ("Leftarrow", '⇐'),
        ("Leftrightarrow", '⇔'),
        ("iff", '⇔'),
        ("mapsto", '↦'),
        ("in", '∈'),
        ("notin", '∉'),
        ("ni", '∋'),
        ("subset", '⊂'),
        ("supset", '⊃'),
        ("subseteq", '⊆'),
        ("supseteq", '⊇'),
        ("cup", '∪'),
        ("cap", '∩'),
        ("land", '∧'),
        ("wedge", '∧'),
        ("lor", '∨'),
        ("vee", '∨'),
        ("circ", '∘'),
        ("oplus", '⊕'),
        ("otimes", '⊗'),
        ("infty", '∞'),
        ("partial", '∂'),
        ("nabla", '∇'),
        ("sum", '∑'),
        ("prod", '∏'),
        ("coprod", '∐'),
        ("int", '∫'),
        ("iint", '∬'),
        ("iiint", '∭'),
        ("oint", '∮'),
        ("bigcup", '⋃'),
        ("bigcap", '⋂'),
        ("forall", '∀'),
        ("exists", '∃'),
        ("neg", '¬'),
        ("lnot", '¬'),
        ("emptyset", '∅'),
        ("varnothing", '∅'),
        ("ldots", '…'),
        ("dots", '…'),
        ("cdots", '⋯'),
        ("sqrt", '√'),
        ("langle", '⟨'),
        ("rangle", '⟩'),
        ("lfloor", '⌊'),
        ("rfloor", '⌋'),
        ("lceil", '⌈'),
        ("rceil", '⌉'),
        ("prime", '′'),
        ("mid", '|'),
        ("ell", 'ℓ'),
        ("hbar", 'ℏ'),
    ];
    GREEK
        .iter()
        .chain(SIGNS)
        .find(|(known, _)| *known == name)
        .map(|(_, sign)| *sign)
}

#[cfg(feature = "images")]
pub use drawing::{drawn, picture, tinted};

/// Рисунок формулы: только там, где есть картинки.
#[cfg(not(feature = "images"))]
pub fn drawn(_math: &NodeRef) -> Option<String> {
    None
}

#[cfg(feature = "images")]
mod drawing {
    use std::sync::OnceLock;

    use base64::Engine;
    use base64::engine::general_purpose::STANDARD;
    use dom_query::NodeRef;
    use formulary::{LayoutOptions, MathFont, layout, parse, svg::to_svg};

    use super::DRAWN;

    /// Без засечек, в родстве с набором Brevier: рядом с Noto Sans формула
    /// STIX выглядела цитатой из другой книги.
    ///
    /// Лежит в `assets/math/`, а не с гарнитурами: `assets/fonts/` целиком едет
    /// ассетами в APK, и шрифт встал бы там второй раз, рядом с ядром.
    const FONT: &[u8] = include_bytes!("../assets/math/NotoSansMath-Regular.ttf");

    /// В чём считаем раскладку: дальше она масштабируется как вектор.
    const UNITS: f32 = 100.0;
    /// Поле `to_svg` со всех сторон, в единицах раскладки.
    const MARGIN: f32 = 2.0;
    /// MathML больше этого в формулу читателю не ложится: это не формула,
    /// а чья-то шутка, и держать над ней поток незачем.
    const MAX_MATHML: usize = 64 * 1024;

    /// По этой метке `media` узнаёт, что вектор наш и его можно красить.
    const MARK: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" data-formula="1""#;

    /// Что из MathML доходит до раскладки. Цвета — нет: цвет задаёт читатель.
    const KEPT: [&str; 20] = [
        "display",
        "displaystyle",
        "scriptlevel",
        "mathvariant",
        "linethickness",
        "stretchy",
        "fence",
        "separator",
        "lspace",
        "rspace",
        "minsize",
        "maxsize",
        "largeop",
        "movablelimits",
        "accent",
        "accentunder",
        "form",
        "symmetric",
        "width",
        "columnalign",
    ];

    /// Длины, которым нужен предел. Разметка чужая: `maxsize="1000000em"`
    /// крейт честно собрал бы скобкой в километр, а настоящие формулы
    /// укладываются в несколько кеглей.
    const LENGTHS: [&str; 6] = [
        "linethickness",
        "lspace",
        "rspace",
        "minsize",
        "maxsize",
        "width",
    ];
    const MAX_LENGTH: f32 = 100.0;

    fn font() -> Option<&'static MathFont<'static>> {
        static PARSED: OnceLock<Option<MathFont<'static>>> = OnceLock::new();
        PARSED.get_or_init(|| MathFont::new(FONT, 0).ok()).as_ref()
    }

    /// Формула рисунком: адрес для `<img>`, либо `None`, если не вышло.
    pub fn drawn(math: &NodeRef) -> Option<String> {
        let mut xml = String::new();
        write_xml(math, &mut xml);
        if xml.len() > MAX_MATHML {
            return None;
        }
        // Крейт молодой, а входит в него чужая разметка: его паника не повод
        // терять страницу.
        let svg = std::panic::catch_unwind(|| svg_of(&xml)).ok()??;
        Some(format!("{DRAWN}{}", STANDARD.encode(svg)))
    }

    /// Байты рисунка по адресу из [`drawn`].
    pub fn picture(address: &str) -> Option<Vec<u8>> {
        STANDARD.decode(address.strip_prefix(DRAWN)?).ok()
    }

    /// Покрасить свой рисунок цветом текста. Чужой вектор — `None`.
    pub fn tinted(svg: &[u8], ink: [u8; 3]) -> Option<Vec<u8>> {
        if !svg.starts_with(MARK.as_bytes()) {
            return None;
        }
        let text = std::str::from_utf8(svg).ok()?;
        let [red, green, blue] = ink;
        let fill = format!(r##"fill="#{red:02x}{green:02x}{blue:02x}""##);
        Some(text.replace(r#"fill="black""#, &fill).into_bytes())
    }

    fn svg_of(xml: &str) -> Option<String> {
        let tree = parse(xml).ok()?;
        // Незнакомое крейт рисует строкой, молча теряя смысл: перечёркнутое
        // (`menclose`) вышло бы неперечёркнутым. Пусть решает вызывающий.
        if !tree.warnings.is_empty() {
            return None;
        }
        let laid = layout(&tree, font()?, &LayoutOptions { font_size: UNITS });
        let svg = to_svg(&laid, font()?);
        // `ex` у `media::sink` — половина кегля, а единица раскладки — сотая.
        let ex = |units: f32| units * 2.0 / UNITS;
        let width = ex(laid.width + 2.0 * MARGIN);
        let height = ex(laid.ascent + laid.descent + 2.0 * MARGIN);
        let depth = ex(laid.descent + MARGIN);
        let end = svg.find('>')?;
        let view = svg[..end]
            .split_once("viewBox=")?
            .1
            .split(" width=")
            .next()?;
        Some(format!(
            r#"{MARK} viewBox={view} width="{width:.3}ex" height="{height:.3}ex" style="vertical-align: -{depth:.3}ex"{}"#,
            &svg[end..]
        ))
    }

    fn sane(name: &str, value: &str) -> bool {
        if !LENGTHS.contains(&name) {
            return true;
        }
        let number: String = value
            .trim()
            .chars()
            .take_while(|sign| sign.is_ascii_digit() || matches!(sign, '.' | '-' | '+'))
            .collect();
        number
            .parse::<f32>()
            .map_or(true, |length| length.abs() <= MAX_LENGTH)
    }

    /// Чистый XML для разбора. Страница даёт HTML, а не XML: `&nbsp;`, чужие
    /// префиксы, комментарии — их `formulary` принял бы за текст: Википедия
    /// пишет `<mo>&#x2212;<!-- − --></mo>`, и знаки удваивались.
    fn write_xml(node: &NodeRef, out: &mut String) {
        let Some(name) = node.node_name() else {
            return;
        };
        if matches!(name.as_ref(), "annotation" | "annotation-xml") {
            return;
        }
        out.push('<');
        out.push_str(&name);
        for attr in node.attrs() {
            if KEPT.contains(&attr.name.local.as_ref()) && sane(&attr.name.local, &attr.value) {
                out.push(' ');
                out.push_str(&attr.name.local);
                out.push_str("=\"");
                out.push_str(
                    &attr
                        .value
                        .replace('&', "&amp;")
                        .replace('"', "&quot;")
                        .replace('<', "&lt;"),
                );
                out.push('"');
            }
        }
        out.push('>');
        for child in node.children_it(false) {
            if child.is_element() {
                write_xml(&child, out);
            } else if child.is_text() {
                out.push_str(&super::escape(&child.text()));
            }
        }
        out.push_str("</");
        out.push_str(&name);
        out.push('>');
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dom_query::Document;

    fn spell(mathml: &str) -> Option<String> {
        let doc = Document::from(format!("<html><body>{mathml}</body></html>"));
        let math = doc.select("math");
        spelled(math.nodes().first().unwrap())
    }

    const POWER: &str = "<math><msup><mi>a</mi><mn>2</mn></msup><mo>+</mo><msup><mi>b</mi><mn>2</mn></msup><mo>=</mo><msup><mi>c</mi><mn>2</mn></msup></math>";

    /// Текст формулы, набранной строкой, без разметки: то, по чему её находит поиск.
    fn spelled_text(mathml: &str) -> String {
        let html = spell(mathml).unwrap();
        let doc = Document::from(format!("<html><body>{html}</body></html>"));
        doc.select("body").text().to_string()
    }

    #[test]
    fn a_drawn_formula_is_found_by_the_words_a_spelled_one_would_be() {
        // Та же формула нарисованной и набранной — один и тот же текст для поиска.
        assert_eq!(seen("a^2 + b^2 = c^2"), spelled_text(POWER));
        assert_eq!(seen("{\\displaystyle a^{2}+b^{2}=c^{2}}"), "a2 + b2 = c2");
    }

    #[test]
    fn tex_commands_are_not_searchable_words() {
        // «the» не должно находить θ, «frac» — дробь, «pmatrix» — матрицу.
        assert_eq!(seen("\\theta = \\frac{\\pi}{2}"), "θ = π2");
        assert_eq!(
            seen("\\begin{pmatrix} a & b \\\\ c & d \\end{pmatrix}"),
            "a b c d"
        );
        assert_eq!(seen("\\mathbf{x} \\cdot \\hat{y}"), "x ⋅ y");
        assert_eq!(seen("\\color{red}{x}"), "x");
    }

    #[test]
    fn functions_brackets_and_signs_read_as_drawn() {
        assert_eq!(seen("\\sin x + \\log_2 n"), "sinx + log2n");
        assert_eq!(seen("\\left( x \\right."), "(x");
        assert_eq!(seen("\\left\\{ x \\right\\}"), "{x}");
        assert_eq!(seen("-x - y"), "−x − y");
        assert_eq!(seen("x_{i,j} \\le \\infty"), "xi,j ≤ ∞");
        assert_eq!(seen("f'(x) \\to \\sqrt{2}"), "f′(x) → √2");
        assert_eq!(seen("50\\% \\& \\$5"), "50%&$5");
        assert_eq!(seen(""), "");
    }

    #[test]
    fn powers_and_operators_read_as_text() {
        assert_eq!(
            spell(POWER).unwrap(),
            "<em>a</em><sup>2</sup> + <em>b</em><sup>2</sup> = <em>c</em><sup>2</sup>"
        );
    }

    #[test]
    fn a_minus_at_the_start_is_a_sign_not_a_subtraction() {
        assert_eq!(
            spell("<math><mo>-</mo><mi>b</mi><mo>-</mo><mn>4</mn></math>").unwrap(),
            "−<em>b</em> − 4"
        );
    }

    #[test]
    fn neighbouring_letters_share_one_italic() {
        assert_eq!(
            spell("<math><mn>4</mn><mi>a</mi><mo>&#x2062;</mo><mi>c</mi></math>").unwrap(),
            "4<em>ac</em>"
        );
    }

    #[test]
    fn a_function_name_stays_upright() {
        assert_eq!(
            spell("<math><mi>sin</mi><mo>(</mo><mi>x</mi><mo>)</mo></math>").unwrap(),
            "sin(<em>x</em>)"
        );
    }

    #[test]
    fn a_script_is_set_tight() {
        assert_eq!(
            spell(
                "<math><msub><mi>a</mi><mrow><mi>i</mi><mo>+</mo><mn>1</mn></mrow></msub></math>"
            )
            .unwrap(),
            "<em>a</em><sub><em>i</em>+1</sub>"
        );
    }

    #[test]
    fn a_prime_is_not_raised() {
        assert_eq!(
            spell("<math><msup><mi>f</mi><mo>′</mo></msup></math>").unwrap(),
            "<em>f</em>′"
        );
    }

    #[test]
    fn what_a_line_cannot_hold_is_left_to_the_drawing() {
        for mathml in [
            "<math><mfrac><mn>1</mn><mn>2</mn></mfrac></math>",
            "<math><msqrt><mi>x</mi></msqrt></math>",
            "<math><munderover><mo>∑</mo><mi>i</mi><mi>n</mi></munderover></math>",
            "<math><msubsup><mo>∑</mo><mi>i</mi><mi>n</mi></msubsup></math>",
            "<math><msup><mi>e</mi><msup><mi>x</mi><mn>2</mn></msup></msup></math>",
            "<math><mi mathvariant=\"double-struck\">R</mi></math>",
            "<math><mtable><mtr><mtd><mi>a</mi></mtd></mtr></mtable></math>",
        ] {
            assert_eq!(spell(mathml), None, "{mathml}");
        }
    }

    #[test]
    fn the_display_wrapper_is_dropped() {
        assert_eq!(bare("{\\displaystyle b^2}"), "b^2");
        assert_eq!(bare("  a+b "), "a+b");
    }

    #[test]
    fn a_drawn_formula_leaves_as_its_source() {
        let md = format!("Корень ![\\\\frac{{1}}{{2}}]({DRAWN}AAAA) и дальше.");
        assert_eq!(exported(&md), "Корень $\\frac{1}{2}$ и дальше.");
    }

    #[test]
    fn a_drawn_formula_without_a_source_says_so() {
        let md = format!("![]({DRAWN}AAAA)");
        assert_eq!(exported(&md), "[formula]");
    }

    #[test]
    fn text_without_a_drawing_is_returned_as_it_is() {
        assert!(matches!(exported("просто текст"), Cow::Borrowed(_)));
    }

    #[cfg(feature = "images")]
    mod drawing {
        use super::*;

        fn draw(mathml: &str) -> Option<String> {
            let doc = Document::from(format!("<html><body>{mathml}</body></html>"));
            let math = doc.select("math");
            drawn(math.nodes().first().unwrap())
        }

        #[test]
        fn a_fraction_becomes_a_picture_that_comes_back_whole() {
            let address = draw("<math><mfrac><mn>1</mn><mn>2</mn></mfrac></math>").unwrap();
            let svg = String::from_utf8(picture(&address).unwrap()).unwrap();
            assert!(svg.contains("vertical-align"));
            assert!(svg.contains("<path"));
        }

        /// Комментарий Википедии в знаке: без чистого XML он удваивал минус.
        #[test]
        fn a_comment_in_a_sign_does_not_double_it() {
            let with = draw("<math><mfrac><mo>&#x2212;<!-- − --></mo><mn>2</mn></mfrac></math>");
            let without = draw("<math><mfrac><mo>&#x2212;</mo><mn>2</mn></mfrac></math>");
            assert_eq!(with.unwrap(), without.unwrap());
        }

        #[test]
        fn html_entities_and_strange_attributes_do_not_stop_the_drawing() {
            let drawing = draw(
                "<math xlink:href=\"#a\" data-x=\"1\"><mfrac><mtext>a&nbsp;b</mtext><mn>2</mn></mfrac></math>",
            );
            assert!(drawing.is_some());
        }

        /// Страница не заставит собирать скобку в километр.
        #[test]
        fn absurd_sizes_are_ignored() {
            let huge = r#"<math><mfrac><mo stretchy="true" minsize="1000000em" maxsize="1000000em">(</mo><mspace width="1000000em"/></mfrac></math>"#;
            let plain = r#"<math><mfrac><mo stretchy="true">(</mo><mspace/></mfrac></math>"#;
            assert_eq!(draw(huge), draw(plain));
        }

        #[test]
        fn what_would_lose_its_meaning_is_not_drawn() {
            assert_eq!(
                draw("<math><menclose notation=\"updiagonalstrike\"><mi>x</mi></menclose></math>"),
                None
            );
        }

        #[test]
        fn only_our_own_vector_takes_the_ink() {
            let address = draw("<math><mfrac><mn>1</mn><mn>2</mn></mfrac></math>").unwrap();
            let svg = picture(&address).unwrap();
            let ink = tinted(&svg, [0xee, 0xdd, 0xcc]).unwrap();
            assert!(String::from_utf8(ink).unwrap().contains("#eeddcc"));
            assert_eq!(tinted(b"<svg fill=\"black\"></svg>", [1, 2, 3]), None);
        }
    }
}
