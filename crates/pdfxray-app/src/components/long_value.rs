//! Long values of a PDF object: arrays, literal strings and hex strings ("2.13 Spec · long
//! arrays: wrap, then fold").
//!
//! A value never runs off the right edge. It wraps at the column width with a hanging indent
//! under its first item (after `[`, `(` or `<`); a line never splits an item of an array, so a
//! reference such as `3 0 R` stays whole. Past [`FOLD_AFTER`] lines it folds behind a focusable
//! "⌄ 194 more values" button, with the closing bracket and a quiet count ("224 numbers") after
//! it so the end of the value is never hidden. Enter or a click unfolds it in place.
//!
//! [`wrap`] is the pure layout; [`fold_line`] draws the button line. The property rows of the
//! inspector and the decoded object view paint the lines themselves.

use std::ops::Range;

use egui::{
    Align2, Color32, Context, CursorIcon, FontId, Id, Rect, Response, Role, Sense, Stroke, StrokeKind, Ui,
    WidgetInfo, pos2, vec2,
};

use super::Icon;
use super::focus;
use crate::object_facts::thousands;
use crate::theme::{fonts, tokens};

/// Lines a value shows before the rest folds behind the "more" button.
pub const FOLD_AFTER: usize = 3;
/// Height of the line that holds the fold button.
pub const FOLD_LINE_H: f32 = 22.0;
/// Height of the fold button.
const BUTTON_H: f32 = 18.0;
/// Space between the button's sides and its chevron and text.
const BUTTON_PAD: f32 = 7.0;
/// Size of the chevron and the gap after it.
const CHEVRON: f32 = 11.0;
const CHEVRON_GAP: f32 = 4.0;
/// Space between the button and the closer, and between the closer and the count.
const CLOSER_GAP: f32 = 14.0;
const COUNT_GAP: f32 = 18.0;

/// What a value is, which says where its lines may break and what its fold counts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// `[…]`: breaks between items, counts items.
    Array,
    /// `(…)`: breaks between words (inside a word that is too long), counts characters.
    Literal,
    /// `<…>`: breaks anywhere, counts bytes.
    Hex,
    /// Anything else: wraps between words, never folds.
    Plain,
}

/// What `text` is: an array, a literal string, a hex string or something else.
pub fn kind_of(text: &str) -> Kind {
    let wrapped =
        |open: char, close: char| text.len() >= 2 && text.starts_with(open) && text.ends_with(close);
    if wrapped('[', ']') {
        Kind::Array
    } else if wrapped('(', ')') {
        Kind::Literal
    } else if wrapped('<', '>') && !text.starts_with("<<") {
        Kind::Hex
    } else {
        Kind::Plain
    }
}

/// The folded part of a value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fold {
    /// The button's text without its chevron: `194 more values`.
    pub more: String,
    /// The closing bracket, shown after the button: `]`, `)` or `>`.
    pub closer: &'static str,
    /// The quiet count after the closer: `224 numbers`.
    pub count: String,
}

/// A value laid out in lines.
#[derive(Debug, Clone, PartialEq)]
pub struct Wrapped {
    /// The byte range of each line in the text, without the spaces it ends with.
    pub lines: Vec<Range<usize>>,
    /// How far every line after the first is indented: the width of the opening bracket.
    pub indent: f32,
    /// What is folded away, when the value is folded.
    pub fold: Option<Fold>,
}

impl Wrapped {
    /// How many lines the value takes, the fold line included.
    pub fn line_count(&self) -> usize {
        self.lines.len() + usize::from(self.fold.is_some())
    }
}

/// Lays `text` out in lines of at most `width` points, `advance` giving the width of a piece of
/// it. Unless `unfolded`, an array or a string that needs more than [`FOLD_AFTER`] lines keeps
/// that many and folds the rest.
pub fn wrap(text: &str, width: f32, unfolded: bool, mut advance: impl FnMut(&str) -> f32) -> Wrapped {
    let kind = kind_of(text);
    let items = match kind {
        Kind::Array => array_items(text),
        _ => Vec::new(),
    };
    let first = match kind {
        Kind::Array => items.first().map_or(0, |item| item.start),
        Kind::Literal | Kind::Hex => 1,
        Kind::Plain => 0,
    };
    let indent = if first > 0 && first < text.len() {
        advance(&text[..first])
    } else {
        0.0
    };
    let atoms: Box<dyn Iterator<Item = Range<usize>> + '_> = match kind {
        Kind::Array if !items.is_empty() => Box::new(array_atoms(text, &items)),
        Kind::Hex => Box::new(char_atoms(text)),
        _ => Box::new(word_atoms(text)),
    };
    let fold_at = if unfolded || kind == Kind::Plain {
        None
    } else {
        Some(FOLD_AFTER)
    };
    let (lines, folded_from) = greedy(text, atoms, width, indent, fold_at, &mut advance);
    let fold = folded_from.map(|at| fold_of(text, kind, &items, at));
    Wrapped { lines, indent, fold }
}

/// [`wrap`] measured in `font` by `ui`'s fonts.
pub fn wrap_in(ui: &Ui, text: &str, font: &FontId, width: f32, unfolded: bool) -> Wrapped {
    ui.fonts_mut(|fonts| {
        wrap(text, width, unfolded, |piece| {
            piece.chars().map(|c| fonts.glyph_width(font, c)).sum()
        })
    })
}

/// Fills lines greedily with `atoms`, splitting an atom that is wider than a line by
/// characters. With `fold_at`, stops when that many lines are full and more would follow, and
/// returns where the rest starts.
fn greedy(
    text: &str,
    atoms: impl Iterator<Item = Range<usize>>,
    width: f32,
    indent: f32,
    fold_at: Option<usize>,
    advance: &mut impl FnMut(&str) -> f32,
) -> (Vec<Range<usize>>, Option<usize>) {
    let mut lines = Vec::new();
    let mut start = 0;
    let mut end = 0;
    let mut used = 0.0_f32;
    let room = |lines: &Vec<Range<usize>>| {
        if lines.is_empty() {
            width
        } else {
            (width - indent).max(1.0)
        }
    };
    // Ends the current line before `at`. Returns true when the rest folds.
    let next_line = |lines: &mut Vec<Range<usize>>, start: &mut usize, end: usize, at: usize| {
        lines.push(*start..end);
        *start = at;
        fold_at.is_some_and(|limit| lines.len() == limit)
    };
    for atom in atoms {
        let piece = &text[atom.clone()];
        let trimmed_end = atom.start + piece.trim_end().len();
        let ink = advance(&text[atom.start..trimmed_end]);
        if used > 0.0 && used + ink > room(&lines) {
            if next_line(&mut lines, &mut start, end, atom.start) {
                return (lines, Some(atom.start));
            }
            used = 0.0;
        }
        if used == 0.0 && ink > room(&lines) {
            // Wider than a whole line: it breaks between characters.
            for (offset, c) in text[atom.start..trimmed_end].char_indices() {
                let at = atom.start + offset;
                let w = advance(&text[at..at + c.len_utf8()]);
                if used > 0.0 && used + w > room(&lines) {
                    if next_line(&mut lines, &mut start, end, at) {
                        return (lines, Some(at));
                    }
                    used = 0.0;
                }
                used += w;
                end = at + c.len_utf8();
            }
            used += advance(&text[trimmed_end..atom.end]);
            continue;
        }
        used += advance(piece);
        end = trimmed_end;
    }
    if start < text.len() || lines.is_empty() {
        lines.push(start..end.max(start));
    }
    (lines, None)
}

/// What is folded when the value is cut at byte `at`.
fn fold_of(text: &str, kind: Kind, items: &[Range<usize>], at: usize) -> Fold {
    let inner_end = text.len().saturating_sub(1);
    let (hidden, total, more, unit, closer) = match kind {
        Kind::Array => {
            let hidden = items.iter().filter(|item| item.end > at).count();
            let (one, many) = item_noun(text, items);
            let unit = if items.len() == 1 { one } else { many };
            (hidden, items.len(), ("value", "values"), unit, "]")
        }
        Kind::Literal => {
            let total = text[1..inner_end].chars().count();
            let hidden = text[at.min(inner_end)..inner_end].chars().count();
            let unit = if total == 1 { "character" } else { "characters" };
            (hidden, total, ("character", "characters"), unit, ")")
        }
        Kind::Hex | Kind::Plain => {
            let digits = |s: &str| s.chars().filter(char::is_ascii_hexdigit).count();
            let total = digits(&text[1..inner_end]).div_ceil(2);
            let hidden = digits(&text[at.min(inner_end)..inner_end]).div_ceil(2);
            let unit = if total == 1 { "byte" } else { "bytes" };
            (hidden, total, ("byte", "bytes"), unit, ">")
        }
    };
    let more_word = if hidden == 1 { more.0 } else { more.1 };
    Fold {
        more: format!("{} more {more_word}", thousands(hidden)),
        closer,
        count: format!("{} {unit}", thousands(total)),
    }
}

/// What the items of an array are, as a noun (one, many): `numbers` when they all are, else
/// `items`.
fn item_noun(text: &str, items: &[Range<usize>]) -> (&'static str, &'static str) {
    let mut kinds = items.iter().map(|item| ItemKind::of(&text[item.clone()]));
    let Some(first) = kinds.next() else {
        return ("item", "items");
    };
    if kinds.any(|kind| kind != first) {
        return ("item", "items");
    }
    match first {
        ItemKind::Number => ("number", "numbers"),
        ItemKind::Reference => ("reference", "references"),
        ItemKind::Name => ("name", "names"),
        ItemKind::String => ("string", "strings"),
        ItemKind::Array => ("array", "arrays"),
        ItemKind::Dictionary => ("dictionary", "dictionaries"),
        ItemKind::Other => ("item", "items"),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ItemKind {
    Number,
    Reference,
    Name,
    String,
    Array,
    Dictionary,
    Other,
}

impl ItemKind {
    fn of(item: &str) -> Self {
        if item.starts_with('/') {
            Self::Name
        } else if item.starts_with("<<") {
            Self::Dictionary
        } else if item.starts_with('(') || item.starts_with('<') {
            Self::String
        } else if item.starts_with('[') {
            Self::Array
        } else if item.ends_with(" R") && item.split_whitespace().count() == 3 {
            Self::Reference
        } else if item.parse::<f64>().is_ok() {
            Self::Number
        } else {
            Self::Other
        }
    }
}

/// The atoms of an array: one per item, each with the spaces after it; the first one carries
/// the opening bracket, the last one the closing bracket.
fn array_atoms<'a>(text: &'a str, items: &'a [Range<usize>]) -> impl Iterator<Item = Range<usize>> + 'a {
    items.iter().enumerate().map(move |(i, item)| {
        let start = if i == 0 { 0 } else { item.start };
        let end = items.get(i + 1).map_or(text.len(), |next| next.start);
        start..end
    })
}

/// The atoms of a string: one per word, each with the spaces after it.
fn word_atoms(text: &str) -> impl Iterator<Item = Range<usize>> + '_ {
    let mut starts = vec![0];
    let mut after_space = false;
    for (at, c) in text.char_indices() {
        if c.is_whitespace() {
            after_space = true;
        } else if after_space {
            starts.push(at);
            after_space = false;
        }
    }
    let ends: Vec<usize> = starts.iter().skip(1).copied().chain([text.len()]).collect();
    starts.into_iter().zip(ends).map(|(start, end)| start..end)
}

/// The atoms of a hex string: one per character; the first one carries `<`, the last one `>`.
fn char_atoms(text: &str) -> impl Iterator<Item = Range<usize>> + '_ {
    // Where each character between the brackets starts, the first one excepted: the atoms
    // break there.
    let inner_end = text.len().saturating_sub(1);
    let mut breaks: Vec<usize> = text
        .char_indices()
        .map(|(at, _)| at)
        .filter(|&at| at > 1 && at < inner_end)
        .collect();
    breaks.push(text.len());
    let starts = std::iter::once(0).chain(breaks.clone());
    starts.zip(breaks).map(|(start, end)| start..end)
}

/// The byte ranges of the top-level items of an array `[…]`; `n g R` is one item.
fn array_items(text: &str) -> Vec<Range<usize>> {
    let bytes = text.as_bytes();
    let end = bytes.len().saturating_sub(1);
    let mut items: Vec<Range<usize>> = Vec::new();
    let mut at = 1;
    while at < end {
        if bytes[at].is_ascii_whitespace() {
            at += 1;
            continue;
        }
        let next = skip_object(bytes, at, end);
        items.push(at..next);
        at = next.max(at + 1);
    }
    merge_references(text, &items)
}

/// Joins `n g R` triples into one item.
fn merge_references(text: &str, items: &[Range<usize>]) -> Vec<Range<usize>> {
    let integer = |r: &Range<usize>| {
        let s = &text[r.clone()];
        !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())
    };
    let mut merged = Vec::with_capacity(items.len());
    let mut i = 0;
    while i < items.len() {
        if i + 2 < items.len()
            && integer(&items[i])
            && integer(&items[i + 1])
            && &text[items[i + 2].clone()] == "R"
        {
            merged.push(items[i].start..items[i + 2].end);
            i += 3;
        } else {
            merged.push(items[i].clone());
            i += 1;
        }
    }
    merged
}

const fn is_delimiter(b: u8) -> bool {
    matches!(
        b,
        b'(' | b')' | b'<' | b'>' | b'[' | b']' | b'{' | b'}' | b'/' | b'%'
    )
}

/// Where the object that starts at `at` ends (exclusive), not past `end`.
fn skip_object(bytes: &[u8], at: usize, end: usize) -> usize {
    match bytes[at] {
        b'(' => skip_literal(bytes, at, end),
        b'<' if bytes.get(at + 1) == Some(&b'<') => skip_nested(bytes, at, end, b"<<", b">>"),
        b'<' => bytes[at..end]
            .iter()
            .position(|&b| b == b'>')
            .map_or(end, |p| at + p + 1),
        b'[' => skip_nested(bytes, at, end, b"[", b"]"),
        // A name or a bare token; a name's slash is a delimiter only where it starts.
        _ => {
            let mut i = at + 1;
            while i < end && !bytes[i].is_ascii_whitespace() && !is_delimiter(bytes[i]) {
                i += 1;
            }
            i
        }
    }
}

fn skip_literal(bytes: &[u8], at: usize, end: usize) -> usize {
    let mut depth = 0_usize;
    let mut i = at;
    while i < end {
        match bytes[i] {
            b'\\' => i += 1,
            b'(' => depth += 1,
            b')' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return i + 1;
                }
            }
            _ => {}
        }
        i += 1;
    }
    end
}

fn skip_nested(bytes: &[u8], at: usize, end: usize, open: &[u8], close: &[u8]) -> usize {
    let mut depth = 0_usize;
    let mut i = at;
    while i < end {
        if bytes[i] == b'(' {
            i = skip_literal(bytes, i, end);
            continue;
        }
        if bytes[i..].starts_with(open) {
            depth += 1;
            i += open.len();
            continue;
        }
        if bytes[i..].starts_with(close) {
            depth = depth.saturating_sub(1);
            i += close.len();
            if depth == 0 {
                return i;
            }
            continue;
        }
        i += 1;
    }
    end
}

/// The memory slot of whether the value `id` was unfolded.
fn unfolded_slot(id: Id) -> Id {
    id.with("long-value-unfolded")
}

/// True once the value `id` was unfolded.
pub fn is_unfolded(ctx: &Context, id: Id) -> bool {
    ctx.data(|d| d.get_temp::<bool>(unfolded_slot(id)))
        .unwrap_or(false)
}

/// The width of the fold line's button for `fold`.
fn button_width(ui: &Ui, fold: &Fold) -> f32 {
    let text = ui
        .painter()
        .layout_no_wrap(fold.more.clone(), fonts::ui(tokens::text::SMALL), tokens::TEXT);
    BUTTON_PAD * 2.0 + CHEVRON + CHEVRON_GAP + text.size().x
}

/// Draws the fold line of the value `id` from `left`, centred on `y`: the "⌄ 194 more values"
/// button, then `fold.closer` in `closer_font` and `closer_ink`, then the count. A click or
/// Enter on the button unfolds the value in place. Returns the button's response.
pub fn fold_line(
    ui: &mut Ui,
    id: Id,
    left: f32,
    y: f32,
    fold: &Fold,
    closer_font: FontId,
    closer_ink: Color32,
) -> Response {
    let width = button_width(ui, fold);
    let rect = Rect::from_min_size(pos2(left, y - BUTTON_H / 2.0), vec2(width, BUTTON_H));
    let response = ui.interact(rect, id.with("long-value-fold"), Sense::click());
    let name = fold.more.clone();
    response.widget_info(|| WidgetInfo::labeled(Role::Button, true, name.clone()));
    let fill = if response.hovered() {
        tokens::SURFACE_3
    } else {
        tokens::SURFACE_2
    };
    let painter = ui.painter();
    painter.rect(
        rect,
        tokens::RADIUS_FOLD,
        fill,
        Stroke::new(1.0, tokens::FOLD_HAIRLINE),
        StrokeKind::Inside,
    );
    focus::ring(ui, &response, rect, tokens::RADIUS_FOLD);
    let chevron = Rect::from_center_size(
        pos2(rect.min.x + BUTTON_PAD + CHEVRON / 2.0, y),
        vec2(CHEVRON, CHEVRON),
    );
    Icon::ChevronDown.paint(ui, chevron, CHEVRON, tokens::TEXT_2);
    let painter = ui.painter();
    painter.text(
        pos2(chevron.max.x + CHEVRON_GAP, y),
        Align2::LEFT_CENTER,
        &fold.more,
        fonts::ui(tokens::text::SMALL),
        tokens::TEXT,
    );
    let closer = painter.text(
        pos2(rect.max.x + CLOSER_GAP, y),
        Align2::LEFT_CENTER,
        fold.closer,
        closer_font,
        closer_ink,
    );
    painter.text(
        pos2(closer.max.x + COUNT_GAP, y),
        Align2::LEFT_CENTER,
        &fold.count,
        fonts::mono(tokens::mono::SMALL),
        tokens::TEXT_3,
    );
    let response = response.on_hover_cursor(CursorIcon::PointingHand);
    if response.clicked() {
        ui.data_mut(|d| d.insert_temp(unfolded_slot(id), true));
        ui.ctx().request_repaint();
    }
    response
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    /// Every character is 1 point wide: widths are character counts.
    fn chars(piece: &str) -> f32 {
        f32::from(u16::try_from(piece.chars().count()).unwrap_or(u16::MAX))
    }

    fn shown<'a>(text: &'a str, wrapped: &Wrapped) -> Vec<&'a str> {
        wrapped.lines.iter().map(|line| &text[line.clone()]).collect()
    }

    fn widths(n: usize) -> String {
        let items: Vec<String> = (0..n).map(|i| (100 + i).to_string()).collect();
        format!("[{}]", items.join(" "))
    }

    #[test]
    fn values_are_told_apart_by_their_brackets() {
        assert_eq!(kind_of("[1 2 3]"), Kind::Array);
        assert_eq!(kind_of("(Hello)"), Kind::Literal);
        assert_eq!(kind_of("<0A1B>"), Kind::Hex);
        assert_eq!(kind_of("<< /A 1 >>"), Kind::Plain);
        // Smoke: a word is plain.
        assert_eq!(kind_of("missing"), Kind::Plain);
    }

    #[test]
    fn an_array_wraps_between_items_with_a_hanging_indent_after_the_bracket() {
        let text = "[278 278 355 556 556 889]";

        let wrapped = wrap(text, 13.0, false, chars);

        assert_eq!(shown(text, &wrapped), ["[278 278 355", "556 556 889]"]);
        assert!((wrapped.indent - 1.0).abs() < f32::EPSILON, "{}", wrapped.indent);
        assert_eq!(wrapped.fold, None);
        // Smoke: no line is wider than the column, the indent counted.
        assert!(
            wrapped
                .lines
                .iter()
                .skip(1)
                .all(|line| chars(&text[line.clone()]) + 1.0 <= 13.0)
        );
    }

    #[test]
    fn a_reference_is_never_split_across_two_lines() {
        let text = "[12 0 R 13 0 R 14 0 R]";

        let wrapped = wrap(text, 10.0, false, chars);

        assert_eq!(shown(text, &wrapped), ["[12 0 R", "13 0 R", "14 0 R]"]);
        // Smoke: three references are counted as three items.
        assert_eq!(array_items(text).len(), 3);
    }

    #[test]
    fn past_three_lines_an_array_folds_and_names_what_is_hidden() {
        // 224 numbers of three digits: ten fit on a line of 40.
        let text = widths(224);

        let wrapped = wrap(&text, 40.0, false, chars);

        assert_eq!(wrapped.lines.len(), FOLD_AFTER);
        assert_eq!(
            wrapped.fold,
            Some(Fold {
                more: "194 more values".to_owned(),
                closer: "]",
                count: "224 numbers".to_owned(),
            })
        );
        assert_eq!(
            &text[wrapped.lines[2].clone()],
            "120 121 122 123 124 125 126 127 128 129"
        );
        // Smoke: unfolded, every item is shown and nothing folds.
        let open = wrap(&text, 40.0, true, chars);
        assert_eq!(open.fold, None);
        assert_eq!(open.lines.len(), 23);
        assert!(text[open.lines.last().unwrap().clone()].ends_with("323]"));
    }

    #[test]
    fn three_lines_or_fewer_never_fold() {
        // 29 items: ten, ten, then nine and the bracket.
        let text = widths(29);

        let wrapped = wrap(&text, 40.0, false, chars);

        assert_eq!(wrapped.lines.len(), 3);
        assert_eq!(wrapped.fold, None);
        // Smoke: one more item makes a fourth line, which folds.
        assert!(wrap(&widths(31), 40.0, false, chars).fold.is_some());
    }

    #[test]
    fn mixed_items_are_counted_as_items_and_references_as_references() {
        let refs = format!(
            "[{}]",
            (1..=40).map(|i| format!("{i} 0 R")).collect::<Vec<_>>().join(" ")
        );
        let folded = wrap(&refs, 30.0, false, chars).fold.expect("folded");
        assert!(folded.count == "40 references", "{folded:?}");

        let mixed = format!(
            "[{}]",
            (0..40)
                .map(|i| format!("/N{i} {i}"))
                .collect::<Vec<_>>()
                .join(" ")
        );
        let folded = wrap(&mixed, 30.0, false, chars).fold.expect("folded");
        assert_eq!(folded.count, "80 items");
        // Smoke: the button always counts values.
        assert!(folded.more.ends_with("more values"), "{folded:?}");
    }

    #[test]
    fn a_long_string_wraps_between_words_and_folds_by_characters() {
        let words = vec!["word"; 40].join(" ");
        let text = format!("({words})");

        let wrapped = wrap(&text, 20.0, false, chars);

        assert_eq!(shown(&text, &wrapped)[0], "(word word word word");
        assert!((wrapped.indent - 1.0).abs() < f32::EPSILON, "{}", wrapped.indent);
        let fold = wrapped.fold.expect("folded");
        assert_eq!((fold.closer, fold.count.as_str()), (")", "199 characters"));
        // The three lines end at byte 60; what follows the space after them is hidden.
        assert_eq!(wrapped.lines[2].end, 60);
        assert_eq!(fold.more, "139 more characters");
        // Smoke: the string is 199 characters long, 60 of them shown.
        assert_eq!(text.chars().count() - 2, 139 + 60);
    }

    #[test]
    fn a_hex_string_breaks_anywhere_and_counts_bytes() {
        let text = format!("<{}>", "0A".repeat(100));

        let wrapped = wrap(&text, 21.0, false, chars);

        assert_eq!(shown(&text, &wrapped)[0], format!("<{}", "0A".repeat(10)));
        assert_eq!(shown(&text, &wrapped)[1], "0A".repeat(10));
        let fold = wrapped.fold.expect("folded");
        assert_eq!(
            (fold.more.as_str(), fold.closer, fold.count.as_str()),
            ("70 more bytes", ">", "100 bytes")
        );
        // Smoke: a short hex string is one line.
        let short = wrap("<0A1B>", 21.0, false, chars);
        assert_eq!((short.lines.len(), short.lines[0].clone()), (1, 0..6));
    }

    #[test]
    fn a_word_wider_than_the_column_breaks_between_its_characters() {
        let text = "abcdefghij";

        let wrapped = wrap(text, 4.0, false, chars);

        assert_eq!(shown(text, &wrapped), ["abcd", "efgh", "ij"]);
        // Smoke: plain text never folds.
        assert_eq!(wrapped.fold, None);
    }

    #[test]
    fn nested_arrays_strings_and_dictionaries_are_one_item_each() {
        let text = "[[1 2] (a b) <</A 1>> /Name 3.5 <0F> 7 0 R true]";

        let items: Vec<&str> = array_items(text).into_iter().map(|r| &text[r]).collect();

        assert_eq!(
            items,
            [
                "[1 2]", "(a b)", "<</A 1>>", "/Name", "3.5", "<0F>", "7 0 R", "true"
            ]
        );
        // Smoke: the count follows the items.
        assert_eq!(item_noun(text, &array_items(text)), ("item", "items"));
    }
}
