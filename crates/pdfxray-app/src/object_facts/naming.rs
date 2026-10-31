//! How an object is named for the user: its kind, title, type line and subtitle.

use pdfxray_core::PageIx;

use super::{ObjectKind, StreamFacts, human_bytes};

pub(super) fn classify(
    type_word: &str,
    content: bool,
    subtype: Option<&str>,
    object_type: Option<&str>,
) -> ObjectKind {
    match type_word {
        "stream" if content => ObjectKind::ContentStream,
        "stream" => match subtype {
            Some("/Form") => ObjectKind::Form,
            Some("/Image") => ObjectKind::Image,
            _ => ObjectKind::Stream,
        },
        "dictionary" => match object_type {
            Some("/Page") => ObjectKind::Page,
            Some("/Font") => ObjectKind::Font,
            _ => ObjectKind::Dictionary,
        },
        "array" => ObjectKind::Array,
        _ => ObjectKind::Scalar,
    }
}

pub(super) fn title_of(kind: ObjectKind, object_type: Option<&str>, subtype: Option<&str>) -> String {
    let named = |name: Option<&str>, fallback: &str| {
        name.map_or_else(|| fallback.to_owned(), |n| n.trim_start_matches('/').to_owned())
    };
    match kind {
        ObjectKind::ContentStream => "Content stream".to_owned(),
        ObjectKind::Form => "Form XObject".to_owned(),
        ObjectKind::Image => "Image".to_owned(),
        ObjectKind::Stream => match object_type {
            Some("/XRef") => "Cross-reference stream".to_owned(),
            Some("/ObjStm") => "Object stream".to_owned(),
            Some("/Metadata") => "Metadata stream".to_owned(),
            _ => "Stream".to_owned(),
        },
        ObjectKind::Page => "Page".to_owned(),
        ObjectKind::Font => "Font".to_owned(),
        ObjectKind::Dictionary => named(object_type.or(subtype), "Dictionary"),
        ObjectKind::Array => "Array".to_owned(),
        ObjectKind::Scalar => "Value".to_owned(),
        ObjectKind::Trailer => "Trailer".to_owned(),
    }
}

pub(super) fn type_line(type_word: &str, kind: ObjectKind) -> String {
    let role = match kind {
        ObjectKind::ContentStream => Some("page drawing"),
        ObjectKind::Form => Some("form drawing"),
        ObjectKind::Image => Some("image"),
        ObjectKind::Page => Some("page"),
        ObjectKind::Font => Some("font"),
        _ => None,
    };
    match role {
        Some(role) => format!("{type_word} · {role}"),
        None => type_word.to_owned(),
    }
}

pub(super) fn subtitle_of(
    kind: ObjectKind,
    page: Option<PageIx>,
    ops: Option<usize>,
    rows: &[(String, String)],
    stream: Option<&StreamFacts>,
    title: &str,
) -> String {
    let value = |key: &str| rows.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str());
    let operators = |n: usize| format!("{n} operator{}", if n == 1 { "" } else { "s" });
    match (kind, page, ops) {
        (ObjectKind::ContentStream, Some(page), Some(n)) => {
            format!("Page {}'s drawing instructions · {}", page + 1, operators(n))
        }
        (ObjectKind::ContentStream, Some(page), None) => format!("Page {}'s drawing instructions", page + 1),
        (ObjectKind::Form, _, Some(n)) => format!("Drawing instructions of a form · {}", operators(n)),
        _ => match kind {
            ObjectKind::Image => format!(
                "Image · {}×{}",
                value("/Width").unwrap_or("?"),
                value("/Height").unwrap_or("?")
            ),
            ObjectKind::Font => format!(
                "Font · {}",
                value("/BaseFont").map_or("unnamed", |name| name.trim_start_matches('/'))
            ),
            ObjectKind::Array => format!("Array · {} items", rows.len()),
            ObjectKind::Scalar => "A single value".to_owned(),
            _ => match stream {
                Some(stream) => format!("{title} · {}", human_bytes(stream.encoded_len)),
                None => format!("{title} · {} keys", rows.len()),
            },
        },
    }
}
