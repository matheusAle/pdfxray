//! The PDF object model, exactly as written in the file.

use std::fmt::{self, Write as _};

use crate::{ObjRef, Span};

/// A value together with the bytes it was read from.
#[derive(Debug, Clone, PartialEq)]
pub struct Spanned<T> {
    /// The value.
    pub value: T,
    /// Where it was read from.
    pub span: Span,
}

impl<T> Spanned<T> {
    /// Pairs a value with its span.
    pub const fn new(value: T, span: Span) -> Self {
        Self { value, span }
    }
}

/// A PDF name with `#xx` escapes decoded.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub struct Name(pub Vec<u8>);

impl Name {
    /// Creates a name from raw (already decoded) bytes.
    pub fn new(bytes: impl Into<Vec<u8>>) -> Self {
        Self(bytes.into())
    }

    /// The decoded bytes.
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    /// The decoded bytes as text, replacing invalid UTF-8.
    pub fn to_string_lossy(&self) -> String {
        String::from_utf8_lossy(&self.0).into_owned()
    }
}

impl PartialEq<[u8]> for Name {
    fn eq(&self, other: &[u8]) -> bool {
        self.0 == other
    }
}

impl PartialEq<&[u8]> for Name {
    fn eq(&self, other: &&[u8]) -> bool {
        self.0 == *other
    }
}

impl PartialEq<str> for Name {
    fn eq(&self, other: &str) -> bool {
        self.0 == other.as_bytes()
    }
}

impl PartialEq<&str> for Name {
    fn eq(&self, other: &&str) -> bool {
        self.0 == other.as_bytes()
    }
}

/// Bytes that end a regular token (ISO 32000-2 §7.2.3, white space).
pub(crate) const fn is_whitespace(b: u8) -> bool {
    matches!(b, 0 | 9 | 10 | 12 | 13 | 32)
}

/// Bytes that delimit tokens (ISO 32000-2 §7.2.3).
pub(crate) const fn is_delimiter(b: u8) -> bool {
    matches!(
        b,
        b'(' | b')' | b'<' | b'>' | b'[' | b']' | b'{' | b'}' | b'/' | b'%'
    )
}

/// Bytes that can be part of a number, keyword or name.
pub(crate) const fn is_regular(b: u8) -> bool {
    !is_whitespace(b) && !is_delimiter(b)
}

impl fmt::Display for Name {
    /// `/Name`, re-escaping every byte that cannot appear literally as `#xx`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_char('/')?;
        for &b in &self.0 {
            if is_regular(b) && (0x21..=0x7E).contains(&b) && b != b'#' {
                f.write_char(char::from(b))?;
            } else {
                write!(f, "#{b:02X}")?;
            }
        }
        Ok(())
    }
}

/// A PDF string: the decoded bytes and the syntax form it was written in.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct PdfString {
    /// The bytes after resolving escapes (literal) or digit pairs (hex).
    pub bytes: Vec<u8>,
    /// True when written as `<...>`, false when written as `(...)`.
    pub hex: bool,
}

impl fmt::Display for PdfString {
    /// Re-serialises in the original form: hex as `<4A..>`, literal as `(..)` with escapes.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.hex {
            f.write_char('<')?;
            for b in &self.bytes {
                write!(f, "{b:02X}")?;
            }
            return f.write_char('>');
        }
        f.write_char('(')?;
        for &b in &self.bytes {
            match b {
                b'(' => f.write_str("\\(")?,
                b')' => f.write_str("\\)")?,
                b'\\' => f.write_str("\\\\")?,
                b'\n' => f.write_str("\\n")?,
                b'\r' => f.write_str("\\r")?,
                b'\t' => f.write_str("\\t")?,
                0x08 => f.write_str("\\b")?,
                0x0C => f.write_str("\\f")?,
                0x20..=0x7E => f.write_char(char::from(b))?,
                _ => write!(f, "\\{b:03o}")?,
            }
        }
        f.write_char(')')
    }
}

/// A dictionary that keeps entry order and duplicate keys.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Dict {
    /// Entries in file order.
    pub entries: Vec<(Spanned<Name>, Spanned<Object>)>,
}

impl Dict {
    /// The value of the first entry named `key`.
    pub fn get(&self, key: &[u8]) -> Option<&Spanned<Object>> {
        self.entries
            .iter()
            .find(|(k, _)| k.value.0 == key)
            .map(|(_, v)| v)
    }

    /// Every value for `key`, in file order (more than one when the key is duplicated).
    pub fn get_all<'a>(&'a self, key: &'a [u8]) -> impl Iterator<Item = &'a Spanned<Object>> + 'a {
        self.entries
            .iter()
            .filter(move |(k, _)| k.value.0 == key)
            .map(|(_, v)| v)
    }

    /// The keys in file order, duplicates included.
    pub fn keys(&self) -> impl Iterator<Item = &Name> {
        self.entries.iter().map(|(k, _)| &k.value)
    }

    /// Number of entries, duplicates included.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// True when there are no entries.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The first value of `key` when it is an integer.
    pub fn get_int(&self, key: &[u8]) -> Option<i64> {
        self.get(key).and_then(|v| v.value.as_int())
    }

    /// The first value of `key` when it is a name.
    pub fn get_name(&self, key: &[u8]) -> Option<&Name> {
        self.get(key).and_then(|v| v.value.as_name())
    }
}

impl fmt::Display for Dict {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("<<")?;
        for (k, v) in &self.entries {
            write!(f, " {} {}", k.value, v.value)?;
        }
        f.write_str(" >>")
    }
}

/// A stream: its dictionary and where its raw (still encoded) bytes are in the file.
#[derive(Debug, Clone, PartialEq)]
pub struct Stream {
    /// The stream dictionary.
    pub dict: Dict,
    /// The data bytes between `stream` and `endstream`, using the actual length.
    pub data: Span,
    /// The `/Length` the dictionary declares, resolved when indirect; `None` if absent or unusable.
    pub declared_length: Option<i64>,
}

/// A direct PDF object.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum Object {
    /// `null`
    #[default]
    Null,
    /// `true` / `false`
    Bool(bool),
    /// An integer.
    Int(i64),
    /// A real number.
    Real(f64),
    /// A string.
    String(PdfString),
    /// A name.
    Name(Name),
    /// An array.
    Array(Vec<Spanned<Object>>),
    /// A dictionary.
    Dict(Dict),
    /// A stream (only as the value of an indirect object).
    Stream(Stream),
    /// An indirect reference `num gen R`.
    Ref(ObjRef),
}

impl Object {
    /// The integer value of an `Int`.
    pub fn as_int(&self) -> Option<i64> {
        match self {
            Self::Int(v) => Some(*v),
            _ => None,
        }
    }

    /// The numeric value of an `Int` or `Real`.
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            // Exact for every integer a PDF reasonably contains; large ones lose low bits.
            Self::Int(v) => Some(crate::numeric::f64_from_i64(*v)),
            Self::Real(v) => Some(*v),
            _ => None,
        }
    }

    /// The name of a `Name`.
    pub fn as_name(&self) -> Option<&Name> {
        match self {
            Self::Name(n) => Some(n),
            _ => None,
        }
    }

    /// The dictionary of a `Dict` or of a `Stream`.
    pub fn as_dict(&self) -> Option<&Dict> {
        match self {
            Self::Dict(d) => Some(d),
            Self::Stream(s) => Some(&s.dict),
            _ => None,
        }
    }

    /// The elements of an `Array`.
    pub fn as_array(&self) -> Option<&[Spanned<Object>]> {
        match self {
            Self::Array(a) => Some(a),
            _ => None,
        }
    }

    /// The reference of a `Ref`.
    pub fn as_ref_obj(&self) -> Option<ObjRef> {
        match self {
            Self::Ref(r) => Some(*r),
            _ => None,
        }
    }

    /// The stream of a `Stream`.
    pub fn as_stream(&self) -> Option<&Stream> {
        match self {
            Self::Stream(s) => Some(s),
            _ => None,
        }
    }
}

impl fmt::Display for Object {
    /// PDF syntax. Reals never carry trailing zeros (`2.5`, not `2.50`) but keep a `.0` when
    /// integral so they re-parse as reals. Streams print their dictionary and the data length only.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Null => f.write_str("null"),
            Self::Bool(b) => write!(f, "{b}"),
            Self::Int(v) => write!(f, "{v}"),
            Self::Real(v) => {
                let text = if v.is_finite() {
                    v.to_string()
                } else {
                    "0".to_owned()
                };
                f.write_str(&text)?;
                if !text.contains('.') {
                    f.write_str(".0")?;
                }
                Ok(())
            }
            Self::String(s) => s.fmt(f),
            Self::Name(n) => n.fmt(f),
            Self::Array(items) => {
                f.write_char('[')?;
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        f.write_char(' ')?;
                    }
                    item.value.fmt(f)?;
                }
                f.write_char(']')
            }
            Self::Dict(d) => d.fmt(f),
            Self::Stream(s) => write!(f, "{} stream[{} bytes]", s.dict, s.data.len()),
            Self::Ref(r) => r.fmt(f),
        }
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    fn name(s: &str) -> Spanned<Name> {
        Spanned::new(Name::new(s), Span::new(0, 0))
    }

    fn val(o: Object) -> Spanned<Object> {
        Spanned::new(o, Span::new(0, 0))
    }

    #[test]
    fn names_are_re_escaped() {
        assert_eq!(Name::new("Font").to_string(), "/Font");
        assert_eq!(Name::new("A B").to_string(), "/A#20B");
        assert_eq!(Name::new("a#b/c(d)").to_string(), "/a#23b#2Fc#28d#29");
        assert_eq!(Name::new(vec![0x00, 0xFF]).to_string(), "/#00#FF");
        assert_eq!(Name::new("").to_string(), "/");
    }

    #[test]
    fn strings_keep_their_syntax_form() {
        let literal = PdfString {
            bytes: b"a(b)\\\n\x01\xFF".to_vec(),
            hex: false,
        };
        let hex = PdfString {
            bytes: vec![0x4A, 0x0B, 0xFF],
            hex: true,
        };

        assert_eq!(literal.to_string(), "(a\\(b\\)\\\\\\n\\001\\377)");
        assert_eq!(hex.to_string(), "<4A0BFF>");
        assert_eq!(PdfString::default().to_string(), "()");
    }

    #[test]
    fn reals_have_no_trailing_zeros() {
        assert_eq!(Object::Real(2.5).to_string(), "2.5");
        assert_eq!(Object::Real(0.5).to_string(), "0.5");
        assert_eq!(Object::Real(-3.0).to_string(), "-3.0");
        assert_eq!(Object::Real(f64::NAN).to_string(), "0.0");
        assert_eq!(Object::Int(-7).to_string(), "-7");
    }

    #[test]
    fn containers_display_as_pdf_syntax() {
        let dict = Dict {
            entries: vec![
                (name("Type"), val(Object::Name(Name::new("Page")))),
                (
                    name("Kids"),
                    val(Object::Array(vec![
                        val(Object::Ref(ObjRef::new(3, 0))),
                        val(Object::Null),
                    ])),
                ),
                (name("Type"), val(Object::Bool(true))),
            ],
        };

        assert_eq!(
            Object::Dict(dict.clone()).to_string(),
            "<< /Type /Page /Kids [3 0 R null] /Type true >>"
        );
        assert_eq!(dict.len(), 3);
        assert_eq!(
            dict.get(b"Type").map(|v| &v.value),
            Some(&Object::Name(Name::new("Page")))
        );
        assert_eq!(dict.get_all(b"Type").count(), 2);
        assert_eq!(
            dict.keys().map(Name::to_string_lossy).collect::<Vec<_>>(),
            ["Type", "Kids", "Type"]
        );
        assert_eq!(Dict::default().to_string(), "<< >>");
    }

    #[test]
    fn accessors_match_variants() {
        assert_eq!(Object::Int(4).as_int(), Some(4));
        assert_eq!(Object::Int(4).as_f64(), Some(4.0));
        assert_eq!(Object::Real(1.5).as_int(), None);
        assert_eq!(Object::Null.as_dict(), None);
        assert_eq!(
            Object::Ref(ObjRef::new(1, 2)).as_ref_obj(),
            Some(ObjRef::new(1, 2))
        );
        assert_eq!(Name::new("A"), "A");
    }
}
