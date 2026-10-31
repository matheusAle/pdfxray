//! One short plain-language note per content-stream operator of ISO 32000-2 table A.1.

use crate::Matrix;
use crate::num;

/// Longest note, in characters.
pub(super) const MAX_NOTE_CHARS: usize = 60;
/// Longest quoted text inside a note before it is cut.
const MAX_QUOTED: usize = 30;

/// Every operator [`note`] knows, in the order of the specification's operator summary.
pub(super) const OPERATORS: [&str; 73] = [
    "w", "J", "j", "M", "d", "ri", "i", "gs", "q", "Q", "cm", "m", "l", "c", "v", "y", "h", "re", "S", "s",
    "f", "F", "f*", "B", "B*", "b", "b*", "n", "W", "W*", "BT", "ET", "Tc", "Tw", "Tz", "TL", "Tf", "Tr",
    "Ts", "Td", "TD", "Tm", "T*", "Tj", "TJ", "'", "\"", "d0", "d1", "CS", "cs", "SC", "SCN", "sc", "scn",
    "G", "g", "RG", "rg", "K", "k", "sh", "BI", "ID", "EI", "Do", "MP", "DP", "BMC", "BDC", "EMC", "BX",
    "EX",
];

/// The note for `op`, or a generic one for an operator that is not in the specification.
pub(super) fn note(op: &str, operands: &[String], text: Option<&str>) -> String {
    let o = Operands(operands);
    let note = match op {
        // General graphics state.
        "w" => format!("set line width to {}", o.num(0)),
        "J" => format!("set line cap: {}", cap(&o.num(0))),
        "j" => format!("set line join: {}", join(&o.num(0))),
        "M" => format!("set miter limit to {}", o.num(0)),
        "d" => dash(&o),
        "ri" => format!("set rendering intent: {}", o.name(0)),
        "i" => format!("set flatness tolerance to {}", o.num(0)),
        "gs" => format!("apply graphics-state settings {}", o.name(0)),
        // Special graphics state.
        "q" => "save graphics state".into(),
        "Q" => "restore graphics state".into(),
        "cm" => transform(&o),
        // Path construction.
        "m" => format!("start a path at x {}, y {}", o.num(0), o.num(1)),
        "l" => format!("line to x {}, y {}", o.num(0), o.num(1)),
        "c" => format!("curve to x {}, y {}", o.num(4), o.num(5)),
        "v" => format!(
            "curve to x {}, y {} (first handle at the start)",
            o.num(2),
            o.num(3)
        ),
        "y" => format!("curve to x {}, y {} (last handle at the end)", o.num(2), o.num(3)),
        "h" => "close the current subpath".into(),
        "re" => format!(
            "rectangle {} × {} at x {}, y {}",
            o.num(2),
            o.num(3),
            o.num(0),
            o.num(1)
        ),
        // Path painting.
        "S" => "stroke the path".into(),
        "s" => "close and stroke the path".into(),
        "f" | "F" => "fill the path (non-zero winding)".into(),
        "f*" => "fill the path (even-odd)".into(),
        "B" => "fill and stroke (non-zero winding)".into(),
        "B*" => "fill and stroke (even-odd)".into(),
        "b" => "close, fill and stroke (non-zero)".into(),
        "b*" => "close, fill and stroke (even-odd)".into(),
        "n" => "end the path without painting".into(),
        // Clipping.
        "W" => "clip to the path (non-zero winding)".into(),
        "W*" => "clip to the path (even-odd)".into(),
        // Text objects.
        "BT" => "begin text".into(),
        "ET" => "end text".into(),
        // Text state.
        "Tc" => format!("set character spacing to {}", o.num(0)),
        "Tw" => format!("set word spacing to {}", o.num(0)),
        "Tz" => format!("set horizontal scaling to {}%", o.num(0)),
        "TL" => format!("set line spacing to {}", o.num(0)),
        "Tf" => format!("font {} at {} pt", o.name(0), o.num(1)),
        "Tr" => format!("text mode: {}", render_mode(&o.num(0))),
        "Ts" => format!("raise text by {}", o.num(0)),
        // Text positioning.
        "Td" => format!("move to x {}, y {}", o.num(0), o.num(1)),
        "TD" => format!("move to x {}, y {} and set line spacing", o.num(0), o.num(1)),
        "Tm" => format!("set text matrix, origin x {}, y {}", o.num(4), o.num(5)),
        "T*" => "move to the next line".into(),
        // Text showing.
        "Tj" => show_text(text.or_else(|| o.literal(0)), "show text"),
        "TJ" => show_text(text, "show text with spacing adjustments"),
        "'" => show_text(text.or_else(|| o.literal(0)), "next line, show text"),
        "\"" => show_text(text.or_else(|| o.literal(2)), "set spacing, next line, show text"),
        // Type 3 glyph metrics.
        "d0" => format!("glyph width {} (coloured glyph)", o.num(0)),
        "d1" => format!("glyph width {} (uses current colour)", o.num(0)),
        // Colour.
        "CS" => format!("stroke colour space: {}", o.name(0)),
        "cs" => format!("fill colour space: {}", o.name(0)),
        "SC" | "SCN" => format!("stroke colour {}", o.all_numbers()),
        "sc" | "scn" => format!("fill colour {}", o.all_numbers()),
        "G" => format!("stroke grey {}", o.num(0)),
        "g" => format!("fill grey {}", o.num(0)),
        "RG" => format!("stroke colour RGB {} {} {}", o.num(0), o.num(1), o.num(2)),
        "rg" => format!("fill colour RGB {} {} {}", o.num(0), o.num(1), o.num(2)),
        "K" => format!(
            "stroke colour CMYK {} {} {} {}",
            o.num(0),
            o.num(1),
            o.num(2),
            o.num(3)
        ),
        "k" => format!(
            "fill colour CMYK {} {} {} {}",
            o.num(0),
            o.num(1),
            o.num(2),
            o.num(3)
        ),
        // Shading, inline images, XObjects.
        "sh" => format!("paint shading {}", o.name(0)),
        "BI" => "begin inline image".into(),
        "ID" => "inline image data starts".into(),
        "EI" => "end inline image".into(),
        "Do" => format!("draw image or form /{}", o.name(0)),
        // Marked content.
        "MP" => format!("marker point: {}", o.name(0)),
        "DP" => format!("marker point: {} with properties", o.name(0)),
        "BMC" => bmc(&o),
        "BDC" => bdc(&o),
        "EMC" => "end tagged content".into(),
        // Compatibility.
        "BX" => "begin compatibility section".into(),
        "EX" => "end compatibility section".into(),
        other => format!("unknown operator {other}"),
    };
    limit(&note)
}

/// True when [`note`] has a specific entry for `op`.
pub(super) fn is_known(op: &str) -> bool {
    OPERATORS.contains(&op)
}

/// Operand access that never panics: a missing operand reads as `?`.
struct Operands<'a>(&'a [String]);

impl Operands<'_> {
    /// Operand `i` as a tidy number.
    fn num(&self, i: usize) -> String {
        self.0.get(i).map_or_else(
            || "?".to_owned(),
            |s| s.parse::<f64>().map_or_else(|_| s.clone(), num),
        )
    }

    /// Operand `i` as a name without its slash (`F1` for `/F1`).
    fn name(&self, i: usize) -> String {
        self.0
            .get(i)
            .map_or_else(|| "?".to_owned(), |s| s.strip_prefix('/').unwrap_or(s).to_owned())
    }

    /// The text of a literal string operand `(...)` at `i`, when it is plain printable text.
    fn literal(&self, i: usize) -> Option<&str> {
        let s = self.0.get(i)?;
        let inner = s.strip_prefix('(')?.strip_suffix(')')?;
        (!inner.contains('\\')).then_some(inner)
    }

    /// Every numeric operand, space separated.
    fn all_numbers(&self) -> String {
        let nums: Vec<String> = self
            .0
            .iter()
            .filter_map(|s| s.parse::<f64>().ok())
            .map(num)
            .collect();
        if nums.is_empty() {
            self.0
                .iter()
                .map(|s| s.trim_start_matches('/'))
                .collect::<Vec<_>>()
                .join(" ")
        } else {
            nums.join(" ")
        }
    }
}

fn cap(value: &str) -> &'static str {
    match value {
        "0" => "flat ends",
        "1" => "round ends",
        "2" => "square ends",
        _ => "unknown",
    }
}

fn join(value: &str) -> &'static str {
    match value {
        "0" => "sharp corners",
        "1" => "round corners",
        "2" => "bevelled corners",
        _ => "unknown",
    }
}

fn render_mode(value: &str) -> &'static str {
    match value {
        "0" => "fill",
        "1" => "stroke",
        "2" => "fill and stroke",
        "3" => "invisible",
        "4" => "fill, add to clip",
        "5" => "stroke, add to clip",
        "6" => "fill, stroke, add to clip",
        "7" => "add to clip only",
        _ => "unknown",
    }
}

fn dash(o: &Operands<'_>) -> String {
    let pattern = o.0.first().map_or("", |s| s.trim_matches(['[', ']']).trim());
    if pattern.is_empty() {
        "set solid lines".to_owned()
    } else {
        format!("set dashed lines: {pattern}")
    }
}

/// `cm`: what the matrix does, in words.
fn transform(o: &Operands<'_>) -> String {
    let values: Vec<f64> = o.0.iter().filter_map(|s| s.parse().ok()).collect();
    let Ok(six) = <[f64; 6]>::try_from(values) else {
        return "change the coordinate system".to_owned();
    };
    let m = Matrix::from(six);
    if m.is_identity() {
        return "change the coordinate system (no change)".to_owned();
    }
    let d = m.decompose();
    let mut parts = Vec::new();
    if d.uniform && (d.scale_x - 1.0).abs() > 1e-6 {
        parts.push(format!("scale {}×", num(d.scale_x)));
    } else if !d.uniform {
        parts.push(format!("scale {} × {}", num(d.scale_x), num(d.scale_y)));
    }
    if d.rotation_deg.abs() > 1e-6 {
        parts.push(format!("rotate {}°", num(d.rotation_deg)));
    }
    if d.skew_deg.abs() > 1e-6 {
        parts.push(format!("skew {}°", num(d.skew_deg)));
    }
    if m.e != 0.0 || m.f != 0.0 {
        parts.push(format!("move to {}, {}", num(m.e), num(m.f)));
    }
    format!("place: {}", parts.join(", "))
}

fn show_text(text: Option<&str>, fallback: &str) -> String {
    match text {
        Some(t) if !t.is_empty() => format!("draw “{}”", cut(t, MAX_QUOTED)),
        _ => fallback.to_owned(),
    }
}

fn bmc(o: &Operands<'_>) -> String {
    match o.name(0).as_str() {
        "Artifact" => "start artifact (not real content)".to_owned(),
        tag => format!("start tagged content: {tag}"),
    }
}

fn bdc(o: &Operands<'_>) -> String {
    let tag = o.name(0);
    let props = o.0.get(1).map_or("", String::as_str);
    if tag == "Artifact" {
        let kinds: Vec<String> = ["Type", "Subtype"]
            .iter()
            .filter_map(|key| dict_name(props, key))
            .collect();
        return if kinds.is_empty() {
            "start artifact (not real content)".to_owned()
        } else {
            format!("start artifact: {}", kinds.join(" "))
        };
    }
    match dict_int(props, "MCID") {
        Some(mcid) => format!("start tagged content: {tag}, MCID {mcid}"),
        None if props.starts_with('/') => format!("start tagged content: {tag} ({props})"),
        None => format!("start tagged content: {tag}"),
    }
}

/// The name after `/key` in an inline dictionary written as text.
fn dict_name(dict: &str, key: &str) -> Option<String> {
    let after = value_after(dict, key)?;
    let name = after.strip_prefix('/')?;
    let end = name
        .find(|c: char| c.is_whitespace() || "/<>[]()".contains(c))
        .unwrap_or(name.len());
    Some(name[..end].to_owned())
}

/// The integer after `/key` in an inline dictionary written as text.
fn dict_int(dict: &str, key: &str) -> Option<i64> {
    let after = value_after(dict, key)?;
    let end = after
        .find(|c: char| !(c.is_ascii_digit() || c == '-'))
        .unwrap_or(after.len());
    after[..end].parse().ok()
}

fn value_after<'d>(dict: &'d str, key: &str) -> Option<&'d str> {
    let needle = format!("/{key}");
    let mut from = 0;
    while let Some(found) = dict[from..].find(&needle) {
        let at = from + found + needle.len();
        let rest = &dict[at..];
        // `/Type` must not match `/TypeX`.
        if rest.starts_with(|c: char| c.is_whitespace() || c == '/') {
            return Some(rest.trim_start());
        }
        from = at;
    }
    None
}

/// `text` cut to `max` characters with an ellipsis.
fn cut(text: &str, max: usize) -> String {
    let flat: String = text
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    if flat.chars().count() <= max {
        return flat;
    }
    let mut out: String = flat.chars().take(max.saturating_sub(1)).collect();
    out.push('…');
    out
}

/// A note shortened to [`MAX_NOTE_CHARS`] characters.
fn limit(note: &str) -> String {
    cut(note, MAX_NOTE_CHARS)
}
