//! A small parser for direct PDF objects, independent of the writer, used to re-derive the rich
//! structure facts from the bytes. It reads Latin-1 text (one char per byte) and understands
//! dictionaries, arrays, names (with `#xx` escapes), literal and hex strings, numbers, booleans,
//! null and `N G R` references.

use crate::common::{latin1, object_offset};

/// A direct PDF object.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Obj {
    Null,
    Bool(bool),
    Num(f64),
    Name(String),
    Str(String),
    Arr(Vec<Obj>),
    /// Entries in file order.
    Dict(Vec<(String, Obj)>),
    Ref(u32),
}

impl Obj {
    pub(crate) fn get(&self, key: &str) -> Option<&Obj> {
        match self {
            Obj::Dict(entries) => entries.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    pub(crate) fn name(&self) -> Option<&str> {
        match self {
            Obj::Name(n) => Some(n),
            _ => None,
        }
    }

    pub(crate) fn string(&self) -> Option<&str> {
        match self {
            Obj::Str(s) => Some(s),
            _ => None,
        }
    }

    pub(crate) fn int(&self) -> Option<i64> {
        match self {
            Obj::Num(n) => Some(*n as i64),
            _ => None,
        }
    }

    pub(crate) fn reference(&self) -> Option<u32> {
        match self {
            Obj::Ref(n) => Some(*n),
            _ => None,
        }
    }

    pub(crate) fn array(&self) -> &[Obj] {
        match self {
            Obj::Arr(items) => items,
            _ => &[],
        }
    }

    pub(crate) fn numbers(&self) -> Vec<f64> {
        self.array()
            .iter()
            .filter_map(|o| match o {
                Obj::Num(n) => Some(*n),
                _ => None,
            })
            .collect()
    }

    /// Canonical PDF syntax, see the `structure` module docs of the facts crate.
    pub(crate) fn syntax(&self) -> String {
        match self {
            Obj::Null => "null".to_owned(),
            Obj::Bool(b) => b.to_string(),
            Obj::Num(n) => n.to_string(),
            Obj::Name(n) => format!("/{n}"),
            Obj::Str(s) => format!("({s})"),
            Obj::Arr(items) => format!(
                "[{}]",
                items.iter().map(Obj::syntax).collect::<Vec<_>>().join(" ")
            ),
            Obj::Dict(entries) => format!(
                "<< {} >>",
                entries
                    .iter()
                    .map(|(k, v)| format!("/{k} {}", v.syntax()))
                    .collect::<Vec<_>>()
                    .join(" ")
            ),
            Obj::Ref(n) => format!("{n} 0 R"),
        }
    }
}

struct Parser<'a> {
    text: &'a [u8],
    at: usize,
}

fn is_space(b: u8) -> bool {
    matches!(b, b' ' | b'\n' | b'\r' | b'\t' | 0 | 12)
}

fn is_delim(b: u8) -> bool {
    matches!(b, b'(' | b')' | b'<' | b'>' | b'[' | b']' | b'/' | b'%')
}

impl Parser<'_> {
    fn skip_space(&mut self) {
        while self.at < self.text.len() && is_space(self.text[self.at]) {
            self.at += 1;
        }
    }

    fn word(&mut self) -> String {
        let start = self.at;
        while self.at < self.text.len() && !is_space(self.text[self.at]) && !is_delim(self.text[self.at]) {
            self.at += 1;
        }
        latin1(&self.text[start..self.at])
    }

    fn object(&mut self) -> Obj {
        self.skip_space();
        match self.text[self.at] {
            b'<' if self.text.get(self.at + 1) == Some(&b'<') => {
                self.at += 2;
                let mut entries = Vec::new();
                loop {
                    self.skip_space();
                    if self.text[self.at..].starts_with(b">>") {
                        self.at += 2;
                        break;
                    }
                    assert_eq!(self.text[self.at], b'/', "dictionary key at {}", self.at);
                    let Obj::Name(key) = self.object() else {
                        unreachable!("keys are names")
                    };
                    entries.push((key, self.object()));
                }
                Obj::Dict(entries)
            }
            b'<' => {
                self.at += 1;
                let start = self.at;
                while self.text[self.at] != b'>' {
                    self.at += 1;
                }
                let hex: String = latin1(&self.text[start..self.at]).split_whitespace().collect();
                self.at += 1;
                Obj::Str(
                    hex.as_bytes()
                        .chunks(2)
                        .map(|p| char::from(u8::from_str_radix(std::str::from_utf8(p).unwrap(), 16).unwrap()))
                        .collect(),
                )
            }
            b'[' => {
                self.at += 1;
                let mut items = Vec::new();
                loop {
                    self.skip_space();
                    if self.text[self.at] == b']' {
                        self.at += 1;
                        break;
                    }
                    items.push(self.object());
                }
                Obj::Arr(items)
            }
            b'/' => {
                self.at += 1;
                let raw = self.word();
                let mut name = String::new();
                let bytes = raw.as_bytes();
                let mut i = 0;
                while i < bytes.len() {
                    if bytes[i] == b'#' {
                        name.push(char::from(u8::from_str_radix(&raw[i + 1..i + 3], 16).unwrap()));
                        i += 3;
                    } else {
                        name.push(char::from(bytes[i]));
                        i += 1;
                    }
                }
                Obj::Name(name)
            }
            b'(' => {
                self.at += 1;
                let mut depth = 1;
                let mut out = String::new();
                while depth > 0 {
                    let b = self.text[self.at];
                    self.at += 1;
                    match b {
                        b'\\' => {
                            out.push(char::from(self.text[self.at]));
                            self.at += 1;
                        }
                        b'(' => {
                            depth += 1;
                            out.push('(');
                        }
                        b')' => {
                            depth -= 1;
                            if depth > 0 {
                                out.push(')');
                            }
                        }
                        other => out.push(char::from(other)),
                    }
                }
                Obj::Str(out)
            }
            _ => {
                let word = self.word();
                match word.as_str() {
                    "true" => Obj::Bool(true),
                    "false" => Obj::Bool(false),
                    "null" => Obj::Null,
                    _ => {
                        let n: f64 = word
                            .parse()
                            .unwrap_or_else(|_| panic!("number expected, got {word:?}"));
                        // An integer followed by `G R` is a reference.
                        let save = self.at;
                        if word.bytes().all(|b| b.is_ascii_digit()) {
                            self.skip_space();
                            let gen_word = self.word();
                            self.skip_space();
                            let r = self.word();
                            if gen_word.bytes().all(|b| b.is_ascii_digit())
                                && !gen_word.is_empty()
                                && r == "R"
                            {
                                return Obj::Ref(n as u32);
                            }
                        }
                        self.at = save;
                        Obj::Num(n)
                    }
                }
            }
        }
    }
}

/// Parses the one direct object that starts at `text[at..]`.
pub(crate) fn parse_at(text: &[u8], at: usize) -> Obj {
    Parser { text, at }.object()
}

/// Parses the body of indirect object `num` (for streams: the stream dictionary).
pub(crate) fn parse_object(pdf: &[u8], num: u32) -> Obj {
    let offset = object_offset(pdf, num).unwrap_or_else(|| panic!("object {num} not found"));
    let header = format!("{num} 0 obj\n");
    parse_at(pdf, offset + header.len())
}
