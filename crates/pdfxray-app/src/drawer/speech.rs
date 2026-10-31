//! The Speech tab: what a screen reader announces, as a transcript.
//!
//! One row per announcement of the document, grouped by page: its number, a chip in the role's
//! colour, the announced text and small chips for what is special about it (the text comes from
//! `/Alt`, the language changes, the table headers read with a cell, a footnote link, a link
//! target). The reader is picked at the top: the spec-neutral structure reading or one of the
//! viewer approximations, which say so and list in a tooltip how they differ and on what
//! evidence. *Show skipped* adds what is not announced, dimmed, with the reason.
//!
//! Resting on a row previews its node on the canvas; a click selects it and takes the canvas
//! there. A node selected elsewhere highlights its row and scrolls the transcript to it.

use std::fmt::Write as _;

use egui::text::{LayoutJob, TextFormat, TextWrapping};
use egui::{
    Align, Label, Layout, Rect, RichText, Role, ScrollArea, Sense, Stroke, StrokeKind, Ui, WidgetInfo,
};
use egui::{pos2, vec2};
use pdfxray_core::numeric::f32_from_usize;
use pdfxray_core::{
    LinkTarget, NodeId, NoteRole, PageIx, SkipReason, SpeechItem, SpeechPreset, Substitution,
};

use super::table;
use crate::components::{Chip, Segment, segmented, switch, tooltip};
use crate::roles::role_color;
use crate::state::{AppState, SpeechData};
use crate::theme::{fonts, tokens};

/// The label of the chip that says the reader is modelled, not recorded.
pub const APPROXIMATION_LABEL: &str = "approximation";

/// The name of the switch that lists what is not announced.
pub const SHOW_SKIPPED_LABEL: &str = "Show skipped";

/// Shown when the reader announces nothing.
pub const NOTHING_ANNOUNCED: &str = "Nothing is announced under this reader.";

/// Added when the file has no structure tree, which most readers need.
pub const UNTAGGED_NOTE: &str = "The file has no structure tree.";

/// The last line of the tooltip of an approximation.
pub const NOT_RECORDED: &str =
    "Built from public documentation and source code. Not recorded from a screen reader.";

/// The chip of a row whose item is not announced is dimmed to this share of its colour.
const DIM: f32 = 0.45;

const NUMBER_W: f32 = 34.0;
const CHIP_W: f32 = 72.0;
const PAD: f32 = 8.0;
const FLAG_H: f32 = 16.0;
/// Width of the `+n` chip that stands for the flags that did not fit.
const MORE_W: f32 = 26.0;
/// Space between two chips at the right end of a row.
const FLAG_GAP: f32 = 4.0;
/// Room the announced text keeps before flags take the rest of the row.
const TEXT_MIN_W: f32 = 140.0;
/// Header cells named on a row before the rest are counted.
const HEADERS_SHOWN: usize = 3;
/// Characters of a header cell's text on a chip.
const HEADER_CHARS: usize = 18;

/// The short name of a reader, for the picker and the header note.
pub const fn preset_title(preset: SpeechPreset) -> &'static str {
    match preset {
        SpeechPreset::Structure => "Structure (spec)",
        SpeechPreset::AcrobatWindows => "Acrobat (NVDA/JAWS)",
        SpeechPreset::FirefoxPdfJs => "Firefox",
        SpeechPreset::ChromiumDefault => "Chrome/Edge",
    }
}

/// The note at the right end of the tab bar while the transcript shows:
/// `Acrobat (NVDA/JAWS) · approximation · 42 spoken · 6 skipped`. Nothing until the document has
/// been read for the reader.
pub fn header_note(state: &AppState) -> Option<String> {
    notes(state).map(|(full, _)| full)
}

/// The note and the shorter one the tab bar falls back to when the tabs leave too little room:
/// without the name of the reader, which the picker shows anyway.
pub fn header_notes(state: &AppState) -> Vec<String> {
    notes(state)
        .map(|(full, short)| vec![full, short])
        .unwrap_or_default()
}

fn notes(state: &AppState) -> Option<(String, String)> {
    let preset = state.drawer.speech.preset;
    let data = state.speech_if_read(preset)?;
    let mut parts = Vec::new();
    if preset.is_approximation() {
        parts.push(APPROXIMATION_LABEL.to_owned());
    }
    parts.push(format!("{} spoken", data.spoken));
    parts.push(format!("{} skipped", data.skipped));
    let short = parts.join(" · ");
    Some((format!("{} · {short}", preset_title(preset)), short))
}

/// Why `item` is not announced, in words: `artifact`, `Private`, `hidden by Alt on Sect`, `empty`.
/// `None` for an item that is announced.
pub fn skip_text(data: &SpeechData, item: &SpeechItem) -> Option<String> {
    Some(match item.skipped.as_ref()? {
        SkipReason::Artifact => "artifact".to_owned(),
        SkipReason::Private => "Private".to_owned(),
        SkipReason::Empty => "empty".to_owned(),
        SkipReason::CoveredBy(by) => {
            let cover = data.item_of(by);
            let how = cover
                .and_then(|cover| cover.substituted_by)
                .map_or("a replacement", |by| substitution_text(by));
            let role = cover.map_or("an element", |cover| cover.declared_role.as_str());
            format!("hidden by {how} on {role}")
        }
    })
}

const fn substitution_text(by: Substitution) -> &'static str {
    match by {
        Substitution::Alt => "Alt",
        Substitution::ActualText => "ActualText",
        Substitution::Expansion => "Expansion",
    }
}

/// The chips of a row, left to right: the substitution, the language change, the table headers
/// the cell is read with, the footnote link and the link target.
pub fn flags(data: &SpeechData, item: &SpeechItem) -> Vec<String> {
    let mut flags = Vec::new();
    if let Some(by) = item.substituted_by {
        flags.push(substitution_text(by).to_owned());
    }
    if item.also_actual_text.is_some() {
        flags.push("also ActualText".to_owned());
    }
    if let Some(change) = &item.lang_change {
        flags.push(format!("lang: {}", change.to.as_deref().unwrap_or("unknown")));
    }
    if let Some(cell) = &item.cell {
        flags.extend(cell.headers.iter().take(HEADERS_SHOWN).map(|header| {
            let name = data.item_of(&header.cell).map_or("", |h| h.text.as_str());
            format!("header: {}", shorten(name, HEADER_CHARS))
        }));
        if cell.headers.len() > HEADERS_SHOWN {
            flags.push(format!("+{}", cell.headers.len() - HEADERS_SHOWN));
        }
    }
    if let Some(note) = &item.note {
        let arrow = match note.role {
            NoteRole::Reference => "↗",
            NoteRole::Note => "↙",
        };
        flags.push(format!("note {arrow} {} apart", note.items_apart));
    }
    if let Some(link) = &item.link {
        flags.push(match &link.target {
            Some(LinkTarget::Uri(uri)) => format!("link → {uri}"),
            Some(LinkTarget::Page(page)) => format!("link → p{}", page + 1),
            Some(LinkTarget::Other(other)) => format!("link → {other}"),
            None => link.subtype.to_lowercase(),
        });
    }
    if item.invisible {
        flags.push("invisible".to_owned());
    }
    flags
}

/// `text`, or its first `max` characters and a `…`.
fn shorten(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        text.to_owned()
    } else {
        text.chars().take(max).chain(std::iter::once('…')).collect()
    }
}

/// What a row shows for the announced text: the text, or `(no name)` for an item that has none.
const NO_NAME: &str = "(no name)";

fn shown_text(item: &SpeechItem) -> &str {
    if item.text.is_empty() { NO_NAME } else { &item.text }
}

/// What the role chip of a row says: the role the reader announces, `text` for text it announces
/// without one, and for an item it does not announce the element's own type.
fn chip_text(item: &SpeechItem) -> &str {
    match (&item.role, item.skipped.is_some()) {
        (Some(role), _) => role,
        (None, true) if !item.declared_role.is_empty() => &item.declared_role,
        (None, true) => "—",
        (None, false) => "text",
    }
}

/// The accessible name of a row: `Item 5: Figure · Revenue chart · Alt`; an item that is not
/// announced says why: `Item 3 (not announced: hidden by Alt on Sect): P · Revenue grew…`.
pub fn row_name(data: &SpeechData, item: &SpeechItem) -> String {
    let mut name = format!("Item {}", item.index);
    if let Some(why) = skip_text(data, item) {
        let _ = write!(name, " (not announced: {why})");
    }
    let _ = write!(name, ": {} · {}", chip_text(item), shown_text(item));
    for flag in flags(data, item) {
        name.push_str(" · ");
        name.push_str(&flag);
    }
    name
}

/// The accessible name of a page separator.
pub fn page_name(page: PageIx) -> String {
    format!("Page {}", page + 1)
}

/// One line of the list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Line {
    /// The start of a page's items.
    Page(PageIx),
    /// An item, by its position in the order.
    Item(usize),
}

/// The lines of the list: items (the skipped ones only when asked for), a separator before the
/// first item of each page.
fn lines(data: &SpeechData, show_skipped: bool) -> Vec<Line> {
    let mut lines = Vec::with_capacity(data.order.items.len() + 1);
    let mut page = None;
    for (at, item) in data.order.items.iter().enumerate() {
        if item.skipped.is_some() && !show_skipped {
            continue;
        }
        if let Some(on) = item.page
            && page != Some(on)
        {
            page = Some(on);
            lines.push(Line::Page(on));
        }
        lines.push(Line::Item(at));
    }
    lines
}

/// True when `item` is what the selection is: its node, or the marked content it was read from.
fn is_selected(item: &SpeechItem, selection: Option<&NodeId>) -> bool {
    selection.is_some_and(|id| item.node == *id || item.marked_content.contains(id))
}

/// What the list looked like last frame, to tell what changed and where it is scrolled.
#[derive(Debug, Clone)]
struct Seen {
    /// The reader, whether skipped items are listed, and the selection.
    key: (SpeechPreset, bool, Option<NodeId>),
    /// How far the list is scrolled.
    offset: f32,
    /// How tall the part of the list in view is.
    view: f32,
}

/// Draws the tab.
pub(super) fn ui(ui: &mut Ui, state: &mut AppState) {
    let preset = controls(ui, state);
    let Some(data) = state.speech(preset) else {
        return;
    };
    if data.order.items.is_empty() {
        let tagged = state.document.as_ref().is_some_and(|loaded| loaded.tagged);
        let text = if tagged {
            NOTHING_ANNOUNCED.to_owned()
        } else {
            format!("{NOTHING_ANNOUNCED} {UNTAGGED_NOTE}")
        };
        ui.label(RichText::new(text).color(tokens::TEXT_3));
        return;
    }
    let show_skipped = state.drawer.speech.show_skipped;
    let lines = lines(&data, show_skipped);
    let selection = state.selection.primary.clone();
    let selected_line = lines.iter().position(
        |line| matches!(line, Line::Item(at) if is_selected(&data.order.items[*at], selection.as_ref())),
    );

    let mut area = ScrollArea::vertical()
        .id_salt(("speech", preset, show_skipped))
        .auto_shrink(false);
    // The transcript follows a selection made elsewhere (and starts on it when it is opened).
    let memory = ui.make_persistent_id("speech-seen");
    let seen: Option<Seen> = ui.data(|d| d.get_temp(memory));
    let key = (preset, show_skipped, selection.clone());
    if seen.as_ref().is_none_or(|seen| seen.key != key)
        && let Some(at) = selected_line
    {
        // Line numbers are far below f32's exact integer range. A line is its height and the
        // space the list leaves between lines.
        let top = f32_from_usize(at) * (tokens::ROW_H + ui.spacing().item_spacing.y);
        let (offset, view) = seen
            .as_ref()
            .map_or((0.0, ui.available_height()), |seen| (seen.offset, seen.view));
        if top < offset || top + tokens::ROW_H > offset + view {
            area = area.vertical_scroll_offset((top - (view - tokens::ROW_H) / 2.0).max(0.0));
        }
    }

    let mut clicked = None;
    let mut hovered = None;
    let shown = area.show_rows(ui, tokens::ROW_H, lines.len(), |ui, rows| {
        for line in &lines[rows] {
            match *line {
                Line::Page(page) => separator(ui, page),
                Line::Item(at) => {
                    let item = &data.order.items[at];
                    let selected = is_selected(item, selection.as_ref());
                    let response = item_row(ui, &data, item, selected);
                    if response.clicked() {
                        clicked = Some(at);
                    }
                    if response.hovered() {
                        hovered = Some(item.node.clone());
                    }
                }
            }
        }
    });
    let seen = Seen {
        key,
        offset: shown.state.clamped_offset().y,
        view: shown.inner_rect.height(),
    };
    ui.data_mut(|d| d.insert_temp(memory, seen));
    if hovered.is_some() {
        state.canvas.preview = hovered;
    }
    if let Some(at) = clicked {
        state.select_speech_item(&data.order.items[at]);
    }
}

/// The picker of the reader, the approximation label and the switch for skipped items. Returns
/// the reader now picked.
fn controls(ui: &mut Ui, state: &mut AppState) -> SpeechPreset {
    let mut preset = state.drawer.speech.preset;
    let mut show_skipped = state.drawer.speech.show_skipped;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 10.0;
        let segments = SpeechPreset::ALL.map(|p| Segment::new(p, preset_title(p)));
        segmented(ui, &mut preset, &segments);
        if preset.is_approximation() {
            let chip = Chip::new(APPROXIMATION_LABEL).show(ui);
            tooltip::custom(chip, |ui| rules(ui, preset));
        }
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            switch(ui, &mut show_skipped, SHOW_SKIPPED_LABEL);
        });
    });
    ui.add_space(2.0);
    state.drawer.speech.preset = preset;
    state.drawer.speech.show_skipped = show_skipped;
    preset
}

/// The tooltip of the approximation label: how the reader differs from the spec reading, rule by
/// rule, and on what evidence.
fn rules(ui: &mut Ui, preset: SpeechPreset) {
    ui.set_max_width(360.0);
    ui.spacing_mut().item_spacing.y = 4.0;
    ui.label(
        RichText::new(format!("Approximation of {}", preset.label()))
            .font(fonts::semibold(tokens::text::BODY))
            .color(tokens::TEXT),
    );
    for rule in preset.rules() {
        ui.add_space(2.0);
        ui.label(
            RichText::new(rule.topic)
                .font(fonts::medium(tokens::text::BODY))
                .color(tokens::TEXT),
        );
        ui.add(Label::new(RichText::new(rule.behaviour).color(tokens::TEXT_2)).wrap());
        ui.add(
            Label::new(
                RichText::new(rule.evidence)
                    .font(fonts::mono(tokens::mono::SMALL))
                    .color(tokens::TEXT_3),
            )
            .wrap(),
        );
    }
    ui.add_space(2.0);
    ui.add(Label::new(RichText::new(NOT_RECORDED).color(tokens::TEXT_3)).wrap());
}

/// The line that opens a page's items: its name and a hairline.
fn separator(ui: &mut Ui, page: PageIx) {
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), tokens::ROW_H), Sense::hover());
    let name = page_name(page);
    response.widget_info(|| WidgetInfo::labeled(Role::Heading, true, name.clone()));
    if !ui.is_rect_visible(rect) {
        return;
    }
    let galley = ui
        .painter()
        .layout_job(fonts::section_header(&name.to_uppercase()));
    let y = rect.center().y;
    let after = rect.min.x + PAD + galley.size().x + 10.0;
    ui.painter().galley(
        pos2(rect.min.x + PAD, y - galley.size().y / 2.0),
        galley,
        tokens::TEXT_3,
    );
    ui.painter()
        .hline(after..=rect.max.x - PAD, y, Stroke::new(1.0, tokens::BORDER));
}

/// How many of the leading chips of `widths` are left out for lack of `room` (the width that
/// may go to the chips, gaps included, the text keeping the rest). The last chip is drawn first,
/// at the right end; where any are left out a `+n` chip takes a place of [`MORE_W`] too.
fn hidden_flags(widths: &[f32], room: f32) -> usize {
    let mut left = room;
    for (n, width) in widths.iter().enumerate().rev() {
        let more = if n > 0 { MORE_W + FLAG_GAP } else { 0.0 };
        if left - width < more {
            return n + 1;
        }
        left -= width + FLAG_GAP;
    }
    0
}

/// One announcement.
fn item_row(ui: &mut Ui, data: &SpeechData, item: &SpeechItem, selected: bool) -> egui::Response {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), tokens::ROW_H), Sense::hover());
    let response = ui.interact(
        rect,
        ui.make_persistent_id(("speech-row", item.index)),
        Sense::click(),
    );
    let name = row_name(data, item);
    response.widget_info(|| WidgetInfo::selected(Role::Button, true, selected, name.clone()));
    if !ui.is_rect_visible(rect) {
        return response;
    }
    let skip = skip_text(data, item);
    let dim = skip.is_some();
    let response = table::row_background(ui, rect, response, selected);
    let painter = ui.painter();
    let y = rect.center().y;
    // The number.
    let number = painter.layout_no_wrap(
        item.index.to_string(),
        fonts::mono(tokens::mono::SMALL),
        tokens::TEXT_3,
    );
    painter.galley(
        pos2(rect.min.x + PAD, y - number.size().y / 2.0),
        number,
        tokens::TEXT_3,
    );
    // The role chip, in the role's colour.
    let color = role_color(item.class);
    let fill = if dim { color.gamma_multiply(DIM) } else { color };
    let mut role = LayoutJob::single_section(
        chip_text(item).to_owned(),
        TextFormat::simple(fonts::bold(tokens::text::SMALL), tokens::BG),
    );
    role.wrap = TextWrapping {
        max_width: CHIP_W - 14.0,
        max_rows: 1,
        break_anywhere: true,
        overflow_character: Some('…'),
    };
    let galley = painter.layout_job(role);
    let chip_x = rect.min.x + PAD + NUMBER_W;
    let chip_rect = Rect::from_min_size(
        pos2(chip_x, y - galley.size().y / 2.0 - 2.0),
        vec2(galley.size().x + 10.0, galley.size().y + 4.0),
    );
    painter.rect_filled(chip_rect, tokens::RADIUS_SM, fill);
    painter.galley(chip_rect.min + vec2(5.0, 2.0), galley, tokens::BG);
    // The chips at the right end, as many as leave the text its room.
    let text_x = chip_x + CHIP_W + 4.0;
    let mut right = rect.max.x - PAD;
    let mut chips: Vec<String> = flags(data, item);
    if let Some(why) = &skip {
        chips.push(why.clone());
    }
    let galleys: Vec<_> = chips
        .iter()
        .map(|text| painter.layout_no_wrap(text.clone(), fonts::mono(tokens::mono::SMALL), tokens::TEXT_2))
        .collect();
    let widths: Vec<f32> = galleys.iter().map(|galley| galley.size().x + 12.0).collect();
    // The leading chips that did not fit; the `+n` chip stands for them.
    let hidden = hidden_flags(&widths, right - text_x - TEXT_MIN_W);
    for (n, galley) in galleys.into_iter().enumerate().skip(hidden).rev() {
        let width = widths[n];
        let at = Rect::from_min_size(pos2(right - width, y - FLAG_H / 2.0), vec2(width, FLAG_H));
        let skip_chip = skip.is_some() && n + 1 == chips.len();
        if skip_chip {
            painter.rect_stroke(
                at,
                tokens::RADIUS_SM,
                Stroke::new(1.0, tokens::BORDER_STRONG),
                StrokeKind::Inside,
            );
        } else {
            painter.rect_filled(at, tokens::RADIUS_SM, tokens::SURFACE_3);
        }
        let ink = if dim { tokens::TEXT_3 } else { tokens::TEXT_2 };
        painter.galley_with_override_text_color(
            at.min + vec2(6.0, (FLAG_H - galley.size().y) / 2.0),
            galley,
            ink,
        );
        right = at.min.x - FLAG_GAP;
    }
    if hidden > 0 {
        let galley = painter.layout_no_wrap(
            format!("+{hidden}"),
            fonts::mono(tokens::mono::SMALL),
            tokens::TEXT_3,
        );
        let at = Rect::from_min_size(pos2(right - MORE_W, y - FLAG_H / 2.0), vec2(MORE_W, FLAG_H));
        painter.rect_filled(at, tokens::RADIUS_SM, tokens::SURFACE_3);
        painter.galley_with_override_text_color(at.center() - galley.size() / 2.0, galley, tokens::TEXT_3);
        right = at.min.x - FLAG_GAP;
    }
    // The announced text, cut to what is left.
    let ink = if dim || item.text.is_empty() {
        tokens::TEXT_3
    } else {
        tokens::TEXT
    };
    let mut job = LayoutJob::single_section(
        shown_text(item).to_owned(),
        TextFormat::simple(fonts::ui(tokens::text::BODY), ink),
    );
    job.wrap = TextWrapping {
        max_width: (right - 6.0 - text_x).max(0.0),
        max_rows: 1,
        break_anywhere: true,
        overflow_character: Some('…'),
    };
    let galley = painter.layout_job(job);
    painter.galley(pos2(text_x, y - galley.size().y / 2.0), galley, ink);
    if hidden > 0 {
        response.on_hover_text(chips[..hidden].join("\n"))
    } else {
        response
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;
    use crate::test_support::fixture;

    #[test]
    fn flags_that_do_not_fit_are_counted_from_the_front_and_leave_room_for_the_plus_chip() {
        // Three chips of 40 with 4 between: all fit in 3 * 40 + 2 * 4.
        assert_eq!(hidden_flags(&[40.0, 40.0, 40.0], 128.0), 0);
        // One short and the first goes; the `+1` chip (MORE_W and a gap) then has to fit too.
        assert_eq!(hidden_flags(&[40.0, 40.0, 40.0], 127.0), 1);
        assert_eq!(
            hidden_flags(&[40.0, 40.0, 40.0], 80.0 + FLAG_GAP + MORE_W + FLAG_GAP),
            1
        );
        assert_eq!(
            hidden_flags(&[40.0, 40.0, 40.0], 80.0 + FLAG_GAP + MORE_W + FLAG_GAP - 1.0),
            2
        );
        // Smoke: nothing fits, and everything is hidden; no flags hide nothing.
        assert_eq!(hidden_flags(&[40.0, 40.0, 40.0], 10.0), 3);
        assert_eq!(hidden_flags(&[], 10.0), 0);
    }

    fn data(name: &str, preset: SpeechPreset) -> SpeechData {
        let doc = fixture(name);
        SpeechData::new(doc.speech_order(preset))
    }

    #[test]
    fn a_skipped_item_says_why_in_words() {
        let hidden = data("speech-alt-hides", SpeechPreset::Structure);
        let artifacts = data("speech-artifacts", SpeechPreset::Structure);

        assert_eq!(
            hidden
                .order
                .items
                .iter()
                .map(|item| skip_text(&hidden, item))
                .collect::<Vec<_>>(),
            [
                None,
                None,
                Some("hidden by Alt on Sect".to_owned()),
                Some("hidden by Alt on Sect".to_owned()),
                None,
                None,
                None
            ]
        );
        assert_eq!(
            skip_text(&artifacts, &artifacts.order.items[0]).as_deref(),
            Some("artifact")
        );
        // Smoke: what is announced has no reason.
        assert_eq!(skip_text(&artifacts, &artifacts.order.items[1]), None);
    }

    #[test]
    fn the_flags_of_a_row_are_its_substitution_language_headers_note_and_link() {
        let alt = data("speech-alt-hides", SpeechPreset::Structure);
        let lang = data("speech-lang", SpeechPreset::Structure);
        let table = data("table-scope-pdf2", SpeechPreset::Structure);
        let links = data("speech-link-objr", SpeechPreset::Structure);
        let notes = data("speech-footnote", SpeechPreset::Structure);

        assert_eq!(flags(&alt, &alt.order.items[1]), ["Alt"]);
        assert_eq!(flags(&lang, &lang.order.items[1]), ["lang: de-DE"]);
        assert_eq!(
            flags(&table, &table.order.items[4]),
            ["header: Q1", "header: North"]
        );
        assert_eq!(
            flags(&links, &links.order.items[1]),
            ["link → https://example.com/report"]
        );
        assert_eq!(flags(&links, &links.order.items[2]), ["link → p1"]);
        assert_eq!(flags(&notes, &notes.order.items[1]), ["note ↗ 1 apart"]);
        assert_eq!(flags(&notes, &notes.order.items[3]), ["note ↙ 1 apart"]);
        // Smoke: a plain paragraph has none.
        assert_eq!(flags(&alt, &alt.order.items[0]), Vec::<String>::new());
    }

    #[test]
    fn the_list_has_a_separator_before_each_page_and_leaves_the_skipped_out_unless_asked() {
        let artifacts = data("speech-artifacts", SpeechPreset::Structure);
        let long = data("long-doc", SpeechPreset::Structure);

        assert_eq!(
            lines(&artifacts, false),
            [Line::Page(0), Line::Item(1), Line::Item(2)]
        );
        assert_eq!(
            lines(&artifacts, true),
            [
                Line::Page(0),
                Line::Item(0),
                Line::Item(1),
                Line::Item(2),
                Line::Item(3)
            ]
        );
        let on_page_two = lines(&long, false)
            .iter()
            .position(|line| *line == Line::Page(1))
            .unwrap();
        // Three paragraphs and a heading in front of it, and the first page's own separator.
        assert_eq!(on_page_two, 5);
    }

    #[test]
    fn the_chip_says_the_announced_role_text_for_none_and_the_element_for_a_skipped_item() {
        let artifacts = data("speech-artifacts", SpeechPreset::Structure);
        let chrome = data("speech-alt-hides", SpeechPreset::ChromiumDefault);

        assert_eq!(chip_text(&artifacts.order.items[0]), "Artifact");
        assert_eq!(chip_text(&artifacts.order.items[1]), "H1");
        assert_eq!(chip_text(&chrome.order.items[0]), "text");
    }

    #[test]
    fn long_header_names_are_cut_with_an_ellipsis() {
        assert_eq!(shorten("Region", 18), "Region");
        assert_eq!(shorten("Revenue per region in euros", 10), "Revenue pe…");
    }

    #[test]
    fn rows_are_named_with_their_number_role_text_flags_and_skip_reason() {
        let hidden = data("speech-alt-hides", SpeechPreset::Structure);

        assert_eq!(
            row_name(&hidden, &hidden.order.items[1]),
            "Item 2: Sect · Summary of results · Alt"
        );
        assert_eq!(
            row_name(&hidden, &hidden.order.items[2]),
            "Item 3 (not announced: hidden by Alt on Sect): P · Revenue grew in every region."
        );
        assert_eq!(page_name(0), "Page 1");
    }
}
