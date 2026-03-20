use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::{self, Display, Formatter};

#[derive(Clone, Debug, PartialEq)]
pub enum JsonValue {
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Array(Vec<JsonValue>),
    Object(BTreeMap<String, JsonValue>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JsonError {
    message: String,
    offset: usize,
}

impl JsonError {
    fn new(message: impl Into<String>, offset: usize) -> Self {
        Self {
            message: message.into(),
            offset,
        }
    }

    pub fn offset(&self) -> usize {
        self.offset
    }
}

impl Display for JsonError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{} at byte {}", self.message, self.offset)
    }
}

impl Error for JsonError {}

impl JsonValue {
    pub fn object(entries: impl IntoIterator<Item = (impl Into<String>, JsonValue)>) -> Self {
        let mut object = BTreeMap::new();
        for (key, value) in entries {
            object.insert(key.into(), value);
        }
        Self::Object(object)
    }

    pub fn stringify_canonical(&self) -> String {
        match self {
            Self::Null => "null".to_string(),
            Self::Bool(value) => value.to_string(),
            Self::Number(value) => format_number(*value),
            Self::String(value) => format!("\"{}\"", escape_string(value)),
            Self::Array(values) => {
                let body = values
                    .iter()
                    .map(Self::stringify_canonical)
                    .collect::<Vec<_>>()
                    .join(",");
                format!("[{body}]")
            }
            Self::Object(entries) => {
                let body = entries
                    .iter()
                    .map(|(key, value)| {
                        format!("\"{}\":{}", escape_string(key), value.stringify_canonical())
                    })
                    .collect::<Vec<_>>()
                    .join(",");
                format!("{{{body}}}")
            }
        }
    }

    pub fn parse(input: &str) -> Result<Self, JsonError> {
        let mut parser = Parser::new(input);
        let value = parser.parse_value()?;
        parser.skip_whitespace();
        if parser.peek().is_some() {
            return Err(JsonError::new(
                "unexpected trailing characters",
                parser.offset,
            ));
        }
        Ok(value)
    }

    pub fn as_object(&self) -> Option<&BTreeMap<String, JsonValue>> {
        match self {
            Self::Object(value) => Some(value),
            _ => None,
        }
    }

    pub fn as_array(&self) -> Option<&[JsonValue]> {
        match self {
            Self::Array(value) => Some(value),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::String(value) => Some(value),
            _ => None,
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Self::Number(value) => Some(*value),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Bool(value) => Some(*value),
            _ => None,
        }
    }

    pub fn as_u64(&self) -> Option<u64> {
        match self {
            Self::Number(value) if value.fract() == 0.0 && *value >= 0.0 => Some(*value as u64),
            _ => None,
        }
    }

    pub fn get<'a>(&'a self, key: &str) -> Option<&'a JsonValue> {
        self.as_object()?.get(key)
    }
}

fn format_number(value: f64) -> String {
    if !value.is_finite() {
        return "null".to_string();
    }

    if value == 0.0 {
        return "0".to_string();
    }

    if value.fract() == 0.0 && value.abs() < 9_007_199_254_740_992.0 {
        return format!("{value:.0}");
    }

    let mut text = format!("{value:.15}");
    while text.contains('.') && text.ends_with('0') {
        text.pop();
    }
    if text.ends_with('.') {
        text.pop();
    }
    text
}

fn escape_string(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            '\u{08}' => escaped.push_str("\\b"),
            '\u{0c}' => escaped.push_str("\\f"),
            ch if ch.is_control() => escaped.push_str(&format!("\\u{:04x}", ch as u32)),
            ch => escaped.push(ch),
        }
    }
    escaped
}

struct Parser<'a> {
    input: &'a str,
    offset: usize,
}

impl<'a> Parser<'a> {
    fn new(input: &'a str) -> Self {
        Self { input, offset: 0 }
    }

    fn parse_value(&mut self) -> Result<JsonValue, JsonError> {
        self.skip_whitespace();
        match self.peek() {
            Some('n') => self.parse_null(),
            Some('t') | Some('f') => self.parse_bool(),
            Some('"') => self.parse_string().map(JsonValue::String),
            Some('[') => self.parse_array(),
            Some('{') => self.parse_object(),
            Some('-' | '0'..='9') => self.parse_number(),
            Some(ch) => Err(JsonError::new(
                format!("unexpected character '{ch}'"),
                self.offset,
            )),
            None => Err(JsonError::new("unexpected end of input", self.offset)),
        }
    }

    fn parse_null(&mut self) -> Result<JsonValue, JsonError> {
        self.expect_literal("null")?;
        Ok(JsonValue::Null)
    }

    fn parse_bool(&mut self) -> Result<JsonValue, JsonError> {
        if self.consume_literal("true") {
            Ok(JsonValue::Bool(true))
        } else if self.consume_literal("false") {
            Ok(JsonValue::Bool(false))
        } else {
            Err(JsonError::new("invalid boolean literal", self.offset))
        }
    }

    fn parse_string(&mut self) -> Result<String, JsonError> {
        self.expect('"')?;
        let mut output = String::new();
        while let Some(ch) = self.next() {
            match ch {
                '"' => return Ok(output),
                '\\' => {
                    let escaped = self
                        .next()
                        .ok_or_else(|| JsonError::new("unterminated escape", self.offset))?;
                    match escaped {
                        '"' => output.push('"'),
                        '\\' => output.push('\\'),
                        '/' => output.push('/'),
                        'b' => output.push('\u{08}'),
                        'f' => output.push('\u{0c}'),
                        'n' => output.push('\n'),
                        'r' => output.push('\r'),
                        't' => output.push('\t'),
                        'u' => output.push(self.parse_unicode_escape()?),
                        other => {
                            return Err(JsonError::new(
                                format!("unsupported escape '{other}'"),
                                self.offset,
                            ));
                        }
                    }
                }
                other => output.push(other),
            }
        }
        Err(JsonError::new("unterminated string", self.offset))
    }

    fn parse_unicode_escape(&mut self) -> Result<char, JsonError> {
        let start = self.offset;
        let hex = self.take_n(4)?;
        let code_point = u32::from_str_radix(hex, 16)
            .map_err(|_| JsonError::new("invalid unicode escape", start))?;
        char::from_u32(code_point)
            .ok_or_else(|| JsonError::new("invalid unicode code point", start))
    }

    fn parse_array(&mut self) -> Result<JsonValue, JsonError> {
        self.expect('[')?;
        let mut values = Vec::new();
        loop {
            self.skip_whitespace();
            if self.consume(']') {
                break;
            }
            values.push(self.parse_value()?);
            self.skip_whitespace();
            if self.consume(']') {
                break;
            }
            self.expect(',')?;
        }
        Ok(JsonValue::Array(values))
    }

    fn parse_object(&mut self) -> Result<JsonValue, JsonError> {
        self.expect('{')?;
        let mut values = BTreeMap::new();
        loop {
            self.skip_whitespace();
            if self.consume('}') {
                break;
            }
            let key = self.parse_string()?;
            self.skip_whitespace();
            self.expect(':')?;
            let value = self.parse_value()?;
            values.insert(key, value);
            self.skip_whitespace();
            if self.consume('}') {
                break;
            }
            self.expect(',')?;
        }
        Ok(JsonValue::Object(values))
    }

    fn parse_number(&mut self) -> Result<JsonValue, JsonError> {
        let start = self.offset;
        if self.consume('-') && !matches!(self.peek(), Some('0'..='9')) {
            return Err(JsonError::new("invalid number literal", start));
        }

        self.consume_digits();

        if self.consume('.') {
            if !matches!(self.peek(), Some('0'..='9')) {
                return Err(JsonError::new("invalid fractional number", self.offset));
            }
            self.consume_digits();
        }

        if matches!(self.peek(), Some('e' | 'E')) {
            self.offset += 1;
            self.consume('+');
            self.consume('-');
            if !matches!(self.peek(), Some('0'..='9')) {
                return Err(JsonError::new("invalid exponent", self.offset));
            }
            self.consume_digits();
        }

        let text = &self.input[start..self.offset];
        let value = text
            .parse::<f64>()
            .map_err(|_| JsonError::new("invalid number literal", start))?;
        if !value.is_finite() {
            return Err(JsonError::new(
                "non-finite numbers are not supported",
                start,
            ));
        }
        Ok(JsonValue::Number(value))
    }

    fn consume_digits(&mut self) {
        while matches!(self.peek(), Some('0'..='9')) {
            self.offset += 1;
        }
    }

    fn skip_whitespace(&mut self) {
        while matches!(self.peek(), Some(' ' | '\n' | '\r' | '\t')) {
            self.offset += 1;
        }
    }

    fn take_n(&mut self, len: usize) -> Result<&'a str, JsonError> {
        if self.offset + len > self.input.len() {
            return Err(JsonError::new("unexpected end of input", self.offset));
        }
        let slice = &self.input[self.offset..self.offset + len];
        self.offset += len;
        Ok(slice)
    }

    fn expect_literal(&mut self, literal: &str) -> Result<(), JsonError> {
        if self.consume_literal(literal) {
            Ok(())
        } else {
            Err(JsonError::new(
                format!("expected literal '{literal}'"),
                self.offset,
            ))
        }
    }

    fn consume_literal(&mut self, literal: &str) -> bool {
        if self.input[self.offset..].starts_with(literal) {
            self.offset += literal.len();
            true
        } else {
            false
        }
    }

    fn expect(&mut self, expected: char) -> Result<(), JsonError> {
        if self.consume(expected) {
            Ok(())
        } else {
            Err(JsonError::new(
                format!("expected '{expected}'"),
                self.offset,
            ))
        }
    }

    fn consume(&mut self, expected: char) -> bool {
        match self.peek() {
            Some(value) if value == expected => {
                self.offset += value.len_utf8();
                true
            }
            _ => false,
        }
    }

    fn next(&mut self) -> Option<char> {
        let ch = self.peek()?;
        self.offset += ch.len_utf8();
        Some(ch)
    }

    fn peek(&self) -> Option<char> {
        self.input[self.offset..].chars().next()
    }
}

#[cfg(test)]
mod tests {
    use super::JsonValue;

    #[test]
    fn canonical_objects_are_sorted() {
        let value =
            JsonValue::object([("z", JsonValue::Number(1.0)), ("a", JsonValue::Bool(true))]);
        assert_eq!(value.stringify_canonical(), r#"{"a":true,"z":1}"#);
    }

    #[test]
    fn round_trips_strings_arrays_and_numbers() {
        let source = r#"{"name":"CD3","values":[1,2.5,-3],"ok":true}"#;
        let parsed = JsonValue::parse(source).expect("valid json");
        assert_eq!(
            parsed.stringify_canonical(),
            r#"{"name":"CD3","ok":true,"values":[1,2.5,-3]}"#
        );
    }
}
