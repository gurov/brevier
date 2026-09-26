//! Разбор JSON в дерево — ровно столько, сколько нужно ядру.
//!
//! Своё, а не `serde_json`: ядро читает JSON в одном месте (JSON Feed),
//! и тянуть ради него фреймворк сериализации в тракт, где его нет, дороже,
//! чем полторы сотни строк разбора. Вход — из сети, поэтому глубина
//! вложенности ограничена: массив в массиве на десять тысяч уровней
//! уронил бы рекурсию.

/// Значение JSON. Число хранится текстом: ядру числа не нужны, а
/// преобразование ради того, чтобы его не использовать, — лишний код.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    Number(String),
    String(String),
    Array(Vec<Value>),
    /// Пары в порядке документа. Ключи в JSON Feed уникальны, и таблица
    /// тут ничего не ускорит.
    Object(Vec<(String, Value)>),
}

/// Глубже этого вложенность не читаем: у настоящих документов пять-шесть
/// уровней, а сотни бывают только у тех, кто хочет уронить разбор.
const MAX_DEPTH: usize = 64;

impl Value {
    /// Поле объекта по имени.
    pub fn get(&self, key: &str) -> Option<&Value> {
        match self {
            Value::Object(pairs) => pairs.iter().find(|(name, _)| name == key).map(|(_, v)| v),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::String(text) => Some(text),
            _ => None,
        }
    }

    pub fn as_array(&self) -> &[Value] {
        match self {
            Value::Array(items) => items,
            _ => &[],
        }
    }

    /// Строковое поле объекта, непустое.
    pub fn text(&self, key: &str) -> Option<&str> {
        self.get(key)
            .and_then(Value::as_str)
            .filter(|text| !text.trim().is_empty())
    }
}

/// Разобрать документ целиком. `None` — не JSON или хвост после значения.
pub fn parse(text: &str) -> Option<Value> {
    let mut parser = Parser {
        bytes: text.as_bytes(),
        text,
        at: 0,
    };
    // Метка порядка байтов перед документом законна для текста, но не для
    // разбора.
    if text.starts_with('\u{feff}') {
        parser.at = '\u{feff}'.len_utf8();
    }
    let value = parser.value(0)?;
    parser.skip_space();
    (parser.at == parser.bytes.len()).then_some(value)
}

struct Parser<'a> {
    bytes: &'a [u8],
    text: &'a str,
    at: usize,
}

impl Parser<'_> {
    fn skip_space(&mut self) {
        while self
            .bytes
            .get(self.at)
            .is_some_and(|byte| matches!(byte, b' ' | b'\t' | b'\n' | b'\r'))
        {
            self.at += 1;
        }
    }

    fn eat(&mut self, byte: u8) -> Option<()> {
        self.skip_space();
        (self.bytes.get(self.at) == Some(&byte)).then(|| self.at += 1)
    }

    fn literal(&mut self, word: &str) -> Option<()> {
        self.text[self.at..]
            .starts_with(word)
            .then(|| self.at += word.len())
    }

    fn value(&mut self, depth: usize) -> Option<Value> {
        if depth > MAX_DEPTH {
            return None;
        }
        self.skip_space();
        match self.bytes.get(self.at)? {
            b'{' => self.object(depth),
            b'[' => self.array(depth),
            b'"' => self.string().map(Value::String),
            b't' => self.literal("true").map(|_| Value::Bool(true)),
            b'f' => self.literal("false").map(|_| Value::Bool(false)),
            b'n' => self.literal("null").map(|_| Value::Null),
            b'-' | b'0'..=b'9' => self.number(),
            _ => None,
        }
    }

    fn object(&mut self, depth: usize) -> Option<Value> {
        self.at += 1;
        let mut pairs = Vec::new();
        if self.eat(b'}').is_some() {
            return Some(Value::Object(pairs));
        }
        loop {
            self.skip_space();
            if self.bytes.get(self.at) != Some(&b'"') {
                return None;
            }
            let key = self.string()?;
            self.eat(b':')?;
            let value = self.value(depth + 1)?;
            pairs.push((key, value));
            if self.eat(b',').is_some() {
                continue;
            }
            self.eat(b'}')?;
            return Some(Value::Object(pairs));
        }
    }

    fn array(&mut self, depth: usize) -> Option<Value> {
        self.at += 1;
        let mut items = Vec::new();
        if self.eat(b']').is_some() {
            return Some(Value::Array(items));
        }
        loop {
            items.push(self.value(depth + 1)?);
            if self.eat(b',').is_some() {
                continue;
            }
            self.eat(b']')?;
            return Some(Value::Array(items));
        }
    }

    fn number(&mut self) -> Option<Value> {
        let start = self.at;
        while self
            .bytes
            .get(self.at)
            .is_some_and(|byte| matches!(byte, b'-' | b'+' | b'.' | b'e' | b'E' | b'0'..=b'9'))
        {
            self.at += 1;
        }
        let number = &self.text[start..self.at];
        number
            .parse::<f64>()
            .ok()
            .map(|_| Value::Number(number.to_owned()))
    }

    /// Строка с открывающей кавычки: экранирование, `\u` и суррогатные пары
    /// (эмодзи в заголовке приходит двумя половинами).
    fn string(&mut self) -> Option<String> {
        self.at += 1;
        let mut out = String::new();
        loop {
            let rest = &self.text[self.at..];
            let stop = rest.find(['"', '\\'])?;
            let plain = &rest[..stop];
            // Управляющий знак без экранирования — не JSON.
            if plain.chars().any(|ch| (ch as u32) < 0x20) {
                return None;
            }
            out.push_str(plain);
            self.at += stop;
            if self.bytes[self.at] == b'"' {
                self.at += 1;
                return Some(out);
            }
            self.at += 1;
            let escaped = *self.bytes.get(self.at)?;
            self.at += 1;
            match escaped {
                b'"' => out.push('"'),
                b'\\' => out.push('\\'),
                b'/' => out.push('/'),
                b'b' => out.push('\u{8}'),
                b'f' => out.push('\u{c}'),
                b'n' => out.push('\n'),
                b'r' => out.push('\r'),
                b't' => out.push('\t'),
                b'u' => {
                    let high = self.hex4()?;
                    let code = if (0xD800..0xDC00).contains(&high) {
                        // Первая половина пары: вторая обязана идти следом.
                        if !self.text[self.at..].starts_with("\\u") {
                            return None;
                        }
                        self.at += 2;
                        let low = self.hex4()?;
                        if !(0xDC00..0xE000).contains(&low) {
                            return None;
                        }
                        0x10000 + ((high - 0xD800) << 10) + (low - 0xDC00)
                    } else {
                        high
                    };
                    out.push(char::from_u32(code)?);
                }
                _ => return None,
            }
        }
    }

    fn hex4(&mut self) -> Option<u32> {
        let digits = self.text.get(self.at..self.at + 4)?;
        let value = u32::from_str_radix(digits, 16).ok()?;
        self.at += 4;
        Some(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_document_becomes_a_tree() {
        let value = parse(
            r#" { "title": "Блог \"тест\"", "n": -1.5e3, "ok": true, "none": null,
                 "items": [ {"id": "1"}, [], {} ] } "#,
        )
        .unwrap();
        assert_eq!(value.text("title"), Some("Блог \"тест\""));
        assert_eq!(value.get("n"), Some(&Value::Number("-1.5e3".to_owned())));
        assert_eq!(value.get("ok"), Some(&Value::Bool(true)));
        assert_eq!(value.get("none"), Some(&Value::Null));
        assert_eq!(value.get("items").unwrap().as_array().len(), 3);
        assert_eq!(
            value.get("items").unwrap().as_array()[0].text("id"),
            Some("1")
        );
    }

    #[test]
    fn escapes_and_surrogate_pairs_are_read() {
        let value = parse(r#""a\né😀\/""#).unwrap();
        assert_eq!(value.as_str(), Some("a\né😀/"));
        // Половина пары без второй — не строка.
        assert_eq!(parse(r#""\ud83d""#), None);
    }

    #[test]
    fn broken_or_hostile_input_is_refused() {
        assert_eq!(parse(r#"{"a": 1,}"#), None);
        assert_eq!(parse(r#"{"a" 1}"#), None);
        assert_eq!(parse(r#"[1, 2"#), None);
        assert_eq!(parse(r#"{} tail"#), None);
        assert_eq!(parse("\"raw\ncontrol\""), None);
        let deep = "[".repeat(10_000) + &"]".repeat(10_000);
        assert_eq!(parse(&deep), None);
        assert_eq!(parse(&"[".repeat(3)), None);
    }
}
