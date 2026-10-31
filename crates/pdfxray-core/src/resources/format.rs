//! Small formatting helpers shared by the resource summaries and inspector rows.

use pdfxray_raw::{Dict, Object};

use crate::{PropGroup, num};

/// Longest value shown inline before it is cut.
const VALUE_PREVIEW: usize = 160;
/// Array elements shown before `…`.
pub(super) const MAX_SHOWN_ITEMS: usize = 12;

/// `/FontDescriptor /Flags` bits (ISO 32000-2 table 121), 1-based.
pub(super) const FLAG_NAMES: [(u8, &str); 9] = [
    (1, "FixedPitch"),
    (2, "Serif"),
    (3, "Symbolic"),
    (4, "Script"),
    (6, "Nonsymbolic"),
    (7, "Italic"),
    (17, "AllCap"),
    (18, "SmallCap"),
    (19, "ForceBold"),
];

/// The three keys that hold an embedded font program.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ProgramKey {
    /// Type 1 font program.
    FontFile,
    /// TrueType font program.
    FontFile2,
    /// Type 1C, `CIDFontType0C` or `OpenType` font program.
    FontFile3,
}

impl ProgramKey {
    pub(super) const ALL: [Self; 3] = [Self::FontFile, Self::FontFile2, Self::FontFile3];

    /// The dictionary key without the slash.
    pub(super) fn key(self) -> &'static str {
        match self {
            Self::FontFile => "FontFile",
            Self::FontFile2 => "FontFile2",
            Self::FontFile3 => "FontFile3",
        }
    }
}

/// Appends a row when `value` is present.
pub(super) fn opt(g: PropGroup, key: &str, value: Option<String>) -> PropGroup {
    match value {
        Some(v) => g.row(key, v),
        None => g,
    }
}

/// `ABCDEF` of `ABCDEF+Arial`, the subset tag of a subsetted font.
pub(super) fn subset_prefix(base_font: &str) -> Option<&str> {
    let (tag, _) = base_font.split_once('+')?;
    (tag.len() == 6 && tag.bytes().all(|b| b.is_ascii_uppercase())).then_some(tag)
}

/// `/S /Transparency, /CS /DeviceRGB` for the entries of `d` except `/Type`.
pub(super) fn show_dict_entries(d: &Dict) -> String {
    let parts: Vec<String> = d
        .entries
        .iter()
        .filter(|(k, _)| k.value != *"Type")
        .map(|(k, v)| format!("{} {}", k.value, show(&v.value)))
        .collect();
    if parts.is_empty() {
        "empty".to_owned()
    } else {
        parts.join(", ")
    }
}

/// Short PDF-syntax rendering of any value.
pub(super) fn show(o: &Object) -> String {
    let text = match o {
        Object::Real(v) => num(*v),
        Object::Array(items) => {
            let mut parts: Vec<String> = items
                .iter()
                .take(MAX_SHOWN_ITEMS)
                .map(|i| show(&i.value))
                .collect();
            if items.len() > MAX_SHOWN_ITEMS {
                parts.push("…".to_owned());
            }
            format!("[{}]", parts.join(" "))
        }
        Object::Dict(d) => format!("<< {} keys >>", d.len()),
        Object::Stream(s) => format!("stream << {} keys >>", s.dict.len()),
        other => other.to_string(),
    };
    truncate(&text)
}

/// Cuts `s` after [`VALUE_PREVIEW`] characters.
pub(super) fn truncate(s: &str) -> String {
    if s.chars().count() <= VALUE_PREVIEW {
        return s.to_owned();
    }
    let mut out: String = s.chars().take(VALUE_PREVIEW).collect();
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn subset_tags_need_six_capitals() {
        assert_eq!(subset_prefix("ABCDEF+Arial"), Some("ABCDEF"));
        assert_eq!(subset_prefix("Abcdef+Arial"), None);
        assert_eq!(subset_prefix("ABCDE+Arial"), None);
        assert_eq!(subset_prefix("Helvetica"), None);
    }

    #[test]
    fn long_values_are_cut() {
        let long = "x".repeat(200);

        assert_eq!(truncate(&long).chars().count(), VALUE_PREVIEW + 1);
        assert_eq!(truncate("short"), "short");
    }
}
