//! Snippets shared by several fixtures.

use crate::Built;
use crate::expected::{Expected, TextOpFacts};
use crate::writer::Output;

/// A Type 1 standard font that needs no embedding.
pub(super) const HELVETICA: &str = "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>";

/// A page dictionary using the conventional layout of most fixtures: parent `2 0 R`, contents
/// `4 0 R` and font `F1` at `5 0 R`. `extra` is spliced in verbatim (media box, struct parents...).
pub(super) fn page_dict(extra: &str) -> String {
    format!("<< /Type /Page /Parent 2 0 R {extra} /Resources << /Font << /F1 5 0 R >> >> /Contents 4 0 R >>")
}

/// Like [`page_dict`] with full control over the resources: `resources` is spliced inside the
/// `/Resources` dictionary verbatim, contents are `4 0 R`.
pub(super) fn page_dict_with(extra: &str, resources: &str) -> String {
    format!("<< /Type /Page /Parent 2 0 R {extra} /Resources << {resources} >> /Contents 4 0 R >>")
}

/// A structure element dictionary. `page` is the object number for `/Pg`; `kids` is the raw
/// `/K` value.
pub(super) fn struct_elem(role: &str, parent: u32, page: Option<u32>, kids: &str) -> String {
    let pg = page.map_or_else(String::new, |p| format!(" /Pg {p} 0 R"));
    format!("<< /Type /StructElem /S /{role} /P {parent} 0 R{pg} /K {kids} >>")
}

/// Pairs the finished writer output with its facts.
pub(super) fn built(out: Output, expected: Expected) -> Built {
    Built {
        pdf: out.bytes,
        expected,
    }
}

/// The zlib stream (`/FlateDecode`) of `data`, see [`crate::deflate`].
pub(super) fn zlib(data: &[u8]) -> Vec<u8> {
    crate::deflate::zlib(data)
}

/// A text operator on page 0 with the initial text state (Helvetica `F1` at 12, fill mode, no
/// spacing, 100 % scaling) at the origin; fixtures override what differs with struct update syntax.
pub(super) fn text_op(op_index: u32, operator: &str, text: &str) -> TextOpFacts {
    TextOpFacts {
        page: 0,
        op_index,
        operator: operator.to_owned(),
        text: text.to_owned(),
        font: "F1".to_owned(),
        base_font: Some("Helvetica".to_owned()),
        font_size: 12.0,
        effective_size: 12.0,
        render_mode: 0,
        char_spacing: 0.0,
        word_spacing: 0.0,
        horizontal_scaling: 100.0,
        rise: 0.0,
        leading: 0.0,
        origin: [0.0, 0.0],
    }
}

/// A Type 1 standard font dictionary for `base_font`.
pub(super) fn type1_font(base_font: &str) -> String {
    format!("<< /Type /Font /Subtype /Type1 /BaseFont /{base_font} >>")
}

/// A tagged line of text on page 0: `/<tag> << /MCID n >> BDC` around `text` in Helvetica `F1` at
/// `size`, with its origin at `(x, y)`.
pub(super) fn tagged_text(tag: &str, mcid: i32, size: u32, (x, y): (i32, i32), text: &str) -> String {
    format!("/{tag} << /MCID {mcid} >> BDC BT /F1 {size} Tf {x} {y} Td ({text}) Tj ET EMC")
}
