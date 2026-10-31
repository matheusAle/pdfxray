//! The Pages entry: a strip of page thumbnails, the five boxes of the page on the canvas (with
//! where each comes from and an eye toggle for its outline) and the page's transform.
//!
//! The filter jumps: `4` is page 4, `A4` the pages of that paper, `rot 90` the turned ones,
//! `612x792` the pages of that size.

use egui::{
    Align, Color32, Frame, Label, Layout, Margin, Rect, RichText, Role, ScrollArea, Sense, Stroke,
    StrokeKind, Ui, UiBuilder, WidgetInfo, pos2, vec2,
};
use pdfxray_core::numeric::{f32_from_f64, f32_from_usize, i64_from_f64_round, usize_from_f32_trunc};
use pdfxray_core::{BoxKind, BoxSource, NodeId, PageBox, PageInfo, PageIx, num};

use super::select_with_modifiers;
use crate::center::canvas::overlay::box_color;
use crate::components::{Icon, PropRow, context_menu, tooltip};
use crate::state::AppState;
use crate::theme::{fonts, tokens};
use crate::units::{Lengths, Unit};

/// Shown when the file has no pages.
pub const NO_PAGES: &str = "This file has no pages.";

/// Name of the eye toggle that hides the outline of `kind`.
pub fn hide_label(kind: BoxKind) -> String {
    format!("Hide {}", kind.key())
}

/// Name of the eye toggle that shows the outline of `kind` again.
pub fn show_label(kind: BoxKind) -> String {
    format!("Show {}", kind.key())
}

/// Name of the thumbnail of the one-based page `number`.
pub fn thumbnail_label(number: PageIx) -> String {
    format!("Page {number}")
}

/// Title of the list of boxes of the one-based page `number`.
pub fn boxes_title(number: PageIx) -> String {
    format!("PAGE {number} BOXES")
}

/// Title of the transform list of the one-based page `number`.
pub fn transform_title(number: PageIx) -> String {
    format!("PAGE {number} TRANSFORM")
}

/// Where a box value comes from, in one line: `own`, `inherited · /Pages 3 0 R` or `not set ·
/// defaults to MediaBox`.
pub fn provenance(kind: BoxKind, source: BoxSource) -> String {
    match source {
        BoxSource::Own => "own".to_owned(),
        BoxSource::Inherited(from) => format!("inherited · /Pages {from}"),
        // A page without a MediaBox has the US Letter paper of the specification.
        BoxSource::Default(of) if of == kind => "not set · defaults to US Letter".to_owned(),
        BoxSource::Default(of) => format!("not set · defaults to {}", of.key()),
    }
}

/// The text after the title in the navigator header: `48 pages · 2 sizes`.
pub fn header_text(state: &AppState) -> Option<String> {
    let loaded = state.document.as_ref()?;
    let count = loaded.doc.page_count();
    let sizes = distinct_sizes(state);
    Some(format!(
        "{count} {} · {sizes} {}",
        if count == 1 { "page" } else { "pages" },
        if sizes == 1 { "size" } else { "sizes" }
    ))
}

/// How many different display sizes (after `/Rotate` and `/UserUnit`) the pages have.
fn distinct_sizes(state: &AppState) -> usize {
    let mut sizes: Vec<(i64, i64)> = pages(state)
        .map(|info| {
            let (w, h) = info.display_size();
            (hundredths(w), hundredths(h))
        })
        .collect();
    sizes.sort_unstable();
    sizes.dedup();
    sizes.len()
}

/// `points` in hundredths, to compare sizes without float equality.
fn hundredths(points: f64) -> i64 {
    i64_from_f64_round(points * 100.0)
}

fn pages(state: &AppState) -> impl Iterator<Item = &PageInfo> {
    let doc = state.document.as_ref().map(|loaded| &loaded.doc);
    (0..state.page_count()).filter_map(move |ix| doc?.page(PageIx::try_from(ix).ok()?))
}

/// What the filter of the Pages entry asks for.
#[derive(Debug, Clone, PartialEq)]
pub enum PageQuery {
    /// Every page.
    All,
    /// The page with this one-based number.
    Number(PageIx),
    /// The pages turned by this many degrees.
    Rotation(u16),
    /// The pages of this display size, in points, in either orientation.
    Size(f64, f64),
    /// The pages of the paper whose name contains this lower-case text.
    Paper(String),
}

impl PageQuery {
    /// Reads the filter text.
    pub fn parse(text: &str) -> Self {
        let text = text.trim().to_lowercase();
        if text.is_empty() {
            return Self::All;
        }
        if let Ok(number) = text.parse::<PageIx>() {
            return Self::Number(number);
        }
        if let Some(degrees) = rotation(&text) {
            return Self::Rotation(degrees);
        }
        if let Some((w, h)) = size(&text) {
            return Self::Size(w, h);
        }
        Self::Paper(text)
    }

    /// Whether the page with zero-based index `ix` is asked for.
    pub fn matches(&self, ix: PageIx, info: &PageInfo) -> bool {
        match self {
            Self::All => true,
            Self::Number(number) => ix + 1 == *number,
            Self::Rotation(degrees) => info.rotate == *degrees,
            Self::Size(w, h) => {
                let (pw, ph) = info.display_size();
                let close = |a: f64, b: f64| (a - b).abs() <= 1.0;
                (close(pw, *w) && close(ph, *h)) || (close(pw, *h) && close(ph, *w))
            }
            Self::Paper(name) => info
                .print_figures()
                .paper_name
                .is_some_and(|paper| paper.to_lowercase().contains(name)),
        }
    }
}

/// `rot 90`, `rotate 270`, `rot90`, `r90` or `↻90`: the degrees.
fn rotation(text: &str) -> Option<u16> {
    let rest = ["rotation", "rotate", "rot", "↻", "r"]
        .iter()
        .find_map(|prefix| text.strip_prefix(prefix))?;
    let rest = rest.trim().trim_end_matches('°').trim();
    let degrees: u16 = rest.parse().ok()?;
    Some(degrees % 360)
}

/// `612x792` or `612 × 792`: width and height.
fn size(text: &str) -> Option<(f64, f64)> {
    let (w, h) = text.split_once(['x', '×'])?;
    Some((w.trim().parse().ok()?, h.trim().parse().ok()?))
}

/// The zero-based pages the filter lets through.
fn matching(state: &AppState) -> Vec<PageIx> {
    let query = PageQuery::parse(&state.nav.filter);
    let Some(loaded) = &state.document else {
        return Vec::new();
    };
    (0..PageIx::try_from(loaded.doc.page_count()).unwrap_or(PageIx::MAX))
        .filter(|ix| loaded.doc.page(*ix).is_some_and(|info| query.matches(*ix, info)))
        .collect()
}

pub(super) fn ui(ui: &mut Ui, state: &mut AppState) {
    let Some(info) = state
        .document
        .as_ref()
        .and_then(|loaded| loaded.doc.page(state.canvas.page))
        .cloned()
    else {
        ui.label(RichText::new(NO_PAGES).color(tokens::TEXT_3));
        return;
    };
    let lengths = state.lengths();
    let matches = matching(state);
    jump(state, &matches);
    ScrollArea::vertical()
        .id_salt("pages-list")
        .auto_shrink(false)
        .show(ui, |ui| {
            strip(ui, state, &matches);
            let number = state.canvas.page + 1;
            heading(ui, &boxes_title(number));
            for page_box in &info.boxes {
                box_row(ui, state, number - 1, page_box, &lengths);
            }
            heading(ui, &transform_title(number));
            transform(ui, &info, &lengths);
        });
}

/// A new filter text takes the canvas to the first page it asks for, unless the page on the
/// canvas is one of them already.
fn jump(state: &mut AppState, matches: &[PageIx]) {
    if state.nav.jumped_for == state.nav.filter {
        return;
    }
    state.nav.jumped_for.clone_from(&state.nav.filter);
    if let Some(first) = matches.first()
        && !matches.contains(&state.canvas.page)
    {
        state.select(NodeId::Page(*first));
    }
}

fn heading(ui: &mut Ui, text: &str) {
    ui.add_space(12.0);
    ui.label(fonts::section_header(text));
    ui.add_space(4.0);
}

// ---- thumbnails -----------------------------------------------------------------------------

/// Width of one thumbnail slot.
const SLOT_W: f32 = 62.0;
/// The square a thumbnail fits in.
const THUMB: f32 = 56.0;
/// Height of a slot: the thumbnail and its caption.
const SLOT_H: f32 = THUMB + 20.0;
/// Space between slots.
const SLOT_GAP: f32 = 4.0;

/// Where slot `n` starts along the strip.
fn slot_x(n: usize) -> f32 {
    (SLOT_W + SLOT_GAP) * f32_from_usize(n)
}

/// The slots, as `first..last`, that show in the part of the strip between `min` and `max`.
fn visible_slots(min: f32, max: f32, count: usize) -> (usize, usize) {
    let stride = SLOT_W + SLOT_GAP;
    let first = usize_from_f32_trunc((min / stride).floor().max(0.0)).min(count);
    let last = usize_from_f32_trunc((max / stride).ceil().max(0.0)).min(count);
    (first, last)
}

fn strip(ui: &mut Ui, state: &mut AppState, matches: &[PageIx]) {
    if matches.is_empty() {
        ui.label(
            RichText::new(format!("No pages match “{}”.", state.nav.filter.trim())).color(tokens::TEXT_3),
        );
        return;
    }
    let current = matches.iter().position(|page| *page == state.canvas.page);
    let followed = ui.make_persistent_id("strip-follows");
    let moved = ui.data(|data| data.get_temp::<PageIx>(followed)) != Some(state.canvas.page);
    ui.data_mut(|data| data.insert_temp(followed, state.canvas.page));
    ScrollArea::horizontal()
        .id_salt("pages-strip")
        .auto_shrink([false, true])
        .show_viewport(ui, |ui, viewport| {
            let (area, _) = ui.allocate_exact_size(vec2(slot_x(matches.len()), SLOT_H), Sense::hover());
            let slot = |n: usize| Rect::from_min_size(area.min + vec2(slot_x(n), 0.0), vec2(SLOT_W, SLOT_H));
            if let (true, Some(at)) = (moved, current) {
                ui.scroll_to_rect(slot(at), None);
            }
            let (first, last) = visible_slots(viewport.min.x, viewport.max.x, matches.len());
            // The thumbnail thread renders the page it was asked for last first, so ask from the
            // right: the leftmost page is the first one to appear.
            for page in matches[first..last].iter().rev() {
                state.thumbnail(ui.ctx(), *page);
            }
            for (n, page) in matches.iter().enumerate().take(last).skip(first) {
                thumbnail(ui, state, *page, slot(n));
            }
        });
}

fn thumbnail(ui: &mut Ui, state: &mut AppState, page: PageIx, slot: Rect) {
    let Some((width, height, rotate)) = state
        .document
        .as_ref()
        .and_then(|loaded| loaded.doc.page(page))
        .map(|info| {
            let (w, h) = info.display_size();
            (w, h, info.rotate)
        })
    else {
        return;
    };
    let number = page + 1;
    let selected = state.canvas.page == page;
    let response = ui
        .interact(slot, ui.make_persistent_id(("page-thumb", page)), Sense::click())
        .on_hover_cursor(egui::CursorIcon::PointingHand);
    let name = thumbnail_label(number);
    response.widget_info(|| WidgetInfo::selected(Role::Button, true, selected, &name));
    let texture = state.thumbnail(ui.ctx(), page);

    let aspect = if height > 0.0 {
        f32_from_f64(width / height)
    } else {
        1.0
    };
    let size = if aspect >= 1.0 {
        vec2(THUMB, THUMB / aspect)
    } else {
        vec2(THUMB * aspect, THUMB)
    };
    let body = Rect::from_center_size(pos2(slot.center().x, slot.min.y + THUMB / 2.0), size);
    let painter = ui.painter();
    // Under the pointer the whole slot (thumbnail and number) takes the hover wash.
    if response.hovered() && !selected {
        painter.rect_filled(slot, tokens::RADIUS_SM, tokens::NEUTRAL_HOVER);
    }
    painter.rect_filled(body, 2.0, Color32::WHITE);
    match &texture {
        Some(texture) => {
            painter.image(
                texture.id(),
                body,
                Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                Color32::WHITE,
            );
        }
        None => {
            painter.rect_filled(body, 2.0, tokens::SURFACE_3);
        }
    }
    let (stroke, kind) = if selected {
        (Stroke::new(2.0, tokens::ACCENT), StrokeKind::Outside)
    } else if response.hovered() {
        (Stroke::new(1.0, tokens::TEXT_2), StrokeKind::Outside)
    } else {
        (Stroke::new(1.0, tokens::BORDER_STRONG), StrokeKind::Outside)
    };
    painter.rect_stroke(body, 2.0, stroke, kind);

    let caption = if rotate == 0 {
        number.to_string()
    } else {
        format!("{number} ↻{rotate}°")
    };
    let ink = if selected { tokens::ACCENT } else { tokens::TEXT_3 };
    let caption_rect = Rect::from_min_max(pos2(slot.min.x, slot.min.y + THUMB + 3.0), slot.max);
    ui.scope_builder(
        UiBuilder::new()
            .max_rect(caption_rect)
            .layout(Layout::top_down(Align::Center)),
        |ui| {
            ui.add(
                Label::new(
                    RichText::new(caption)
                        .font(fonts::mono(tokens::mono::SMALL))
                        .color(ink),
                )
                .selectable(false),
            );
        },
    );
    context_menu::attach(&response, state, &NodeId::Page(page));
    if response.clicked() {
        state.select(NodeId::Page(page));
    }
}

// ---- boxes ----------------------------------------------------------------------------------

/// Height of a box row: the name line and the provenance line.
const ROW_H: f32 = 44.0;
/// Height of the name line inside a box row.
const NAME_LINE_H: f32 = 20.0;
/// Side of the eye toggle.
const EYE: f32 = 26.0;

fn box_row(ui: &mut Ui, state: &mut AppState, page: PageIx, page_box: &PageBox, lengths: &Lengths) {
    let kind = page_box.kind;
    let id = NodeId::PageBox { page, kind };
    let selected = state.selection.primary.as_ref() == Some(&id);
    let shown = state.canvas.box_shown(kind);
    let defaulted = matches!(page_box.source, BoxSource::Default(_));
    let color = box_color(kind);

    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), ROW_H), Sense::hover());
    let row = ui
        .interact(rect, ui.make_persistent_id(("box-row", kind)), Sense::click())
        .on_hover_cursor(egui::CursorIcon::PointingHand);
    row.widget_info(|| WidgetInfo::selected(Role::Button, true, selected, kind.key()));
    let eye_rect = Rect::from_center_size(
        pos2(rect.max.x - 8.0 - EYE / 2.0, rect.center().y),
        vec2(EYE, EYE),
    );
    let eye = ui
        .interact(eye_rect, ui.make_persistent_id(("box-eye", kind)), Sense::click())
        .on_hover_cursor(egui::CursorIcon::PointingHand);
    let eye_name = if shown { hide_label(kind) } else { show_label(kind) };
    eye.widget_info(|| WidgetInfo::labeled(Role::Button, true, &eye_name));

    if selected {
        ui.painter().rect(
            rect.shrink2(vec2(0.0, 1.0)),
            tokens::RADIUS_MD,
            tokens::ACCENT_DIM,
            Stroke::new(1.0, tokens::ACCENT.gamma_multiply(0.45)),
            StrokeKind::Inside,
        );
    } else if row.hovered() || eye.hovered() {
        ui.painter()
            .rect_filled(rect.shrink2(vec2(0.0, 1.0)), tokens::RADIUS_MD, tokens::SURFACE_2);
    }
    let name_ink = if defaulted { tokens::TEXT_3 } else { tokens::TEXT };
    let text_rect = Rect::from_min_max(
        pos2(rect.min.x + 32.0, rect.min.y + 5.0),
        pos2(eye_rect.min.x - 4.0, rect.max.y - 2.0),
    );
    let corners = format!(
        "[{} {} {} {}]",
        lengths.number(page_box.rect.x0),
        lengths.number(page_box.rect.y0),
        lengths.number(page_box.rect.x1),
        lengths.number(page_box.rect.y1)
    );
    let name_line = ui.scope_builder(UiBuilder::new().max_rect(text_rect), |ui| {
        ui.spacing_mut().item_spacing.y = 1.0;
        // The name line is as tall as its text, not a control: with the 28 pt control height
        // the provenance line would end below the row.
        ui.spacing_mut().interact_size.y = NAME_LINE_H;
        let line = ui.horizontal(|ui| {
            ui.add(
                Label::new(
                    RichText::new(kind.key())
                        .font(fonts::medium(tokens::text::LARGE))
                        .color(name_ink),
                )
                .selectable(false),
            );
            ui.add(
                Label::new(
                    RichText::new(corners)
                        .font(fonts::mono(tokens::mono::SMALL))
                        .color(tokens::TEXT_2),
                )
                .selectable(false)
                .truncate(),
            );
        });
        ui.add(
            Label::new(
                RichText::new(provenance(kind, page_box.source))
                    .font(fonts::ui(tokens::text::SMALL))
                    .color(tokens::TEXT_3),
            )
            .selectable(false)
            .truncate(),
        );
        line.response.rect.center().y
    });
    // The swatch is centred on the line of the name, not on the row.
    let swatch = Rect::from_center_size(pos2(rect.min.x + 16.0, name_line.inner), vec2(12.0, 12.0));
    ui.painter()
        .rect_stroke(swatch, 3.0, Stroke::new(1.5, color), StrokeKind::Inside);
    let (icon, ink) = if shown {
        (Icon::Eye, tokens::TEXT_2)
    } else {
        (Icon::Hide, tokens::TEXT_3)
    };
    icon.paint(ui, eye_rect, 15.0, if eye.hovered() { tokens::TEXT } else { ink });
    tooltip::text(eye.clone(), &eye_name);

    context_menu::attach(&row, state, &id);
    if eye.clicked() {
        state.canvas.toggle_box(kind);
    } else if row.clicked() {
        select_with_modifiers(ui, state, id);
    }
}

// ---- transform ------------------------------------------------------------------------------

/// `1.0`, `2.5`: a number with at least one decimal.
fn decimal(value: f64) -> String {
    let text = num(value);
    if text.contains('.') {
        text
    } else {
        format!("{text}.0")
    }
}

/// The value of the `UserUnit` row: `1.0 (1 unit = 1 pt)`.
pub fn user_unit_text(user_unit: f64, lengths: &Lengths) -> String {
    format!("{} (1 unit = {})", decimal(user_unit), lengths.format(1.0))
}

/// The value of the Display size row: `612 × 792 pt · US Letter`; the paper is left out for a
/// size without a common name.
pub fn display_size_text(info: &PageInfo, unit: Unit) -> String {
    // `display_size` already applied the page's `/UserUnit`, so only the unit is converted.
    let (w, h) = info.display_size();
    let size = format!("{} × {}", unit.number(w), unit.format(h));
    match info.print_figures().paper_name {
        Some(paper) => format!("{size} · {paper}"),
        None => size,
    }
}

fn transform(ui: &mut Ui, info: &PageInfo, lengths: &Lengths) {
    Frame::new().inner_margin(Margin::ZERO).show(ui, |ui| {
        PropRow::new("Rotate", &format!("{}°", info.rotate)).show(ui);
        PropRow::new("UserUnit", &user_unit_text(info.user_unit, lengths)).show(ui);
        let size = display_size_text(info, lengths.unit);
        // The paper name wraps as a whole, and the copy keeps its plain spaces.
        let shown = size.split_once(" · ").map_or_else(
            || size.clone(),
            |(figures, paper)| format!("{figures} · {}", paper.replace(' ', "\u{a0}")),
        );
        PropRow::new("Display size", &shown).copy_text(&size).show(ui);
    });
}

#[cfg(test)]
mod tests {
    use pdfxray_core::{Document, ObjRef};
    use pretty_assertions::assert_eq;

    use super::*;
    use crate::test_support::fixture;

    fn matching_pages(doc: &Document, text: &str) -> Vec<PageIx> {
        let query = PageQuery::parse(text);
        (0..PageIx::try_from(doc.page_count()).unwrap())
            .filter(|ix| query.matches(*ix, doc.page(*ix).unwrap()))
            .collect()
    }

    #[test]
    fn the_filter_reads_numbers_rotations_sizes_and_papers() {
        assert_eq!(PageQuery::parse(""), PageQuery::All);
        assert_eq!(PageQuery::parse(" 4 "), PageQuery::Number(4));
        assert_eq!(PageQuery::parse("rot 90"), PageQuery::Rotation(90));
        assert_eq!(PageQuery::parse("Rotate 270"), PageQuery::Rotation(270));
        assert_eq!(PageQuery::parse("rot180"), PageQuery::Rotation(180));
        assert_eq!(PageQuery::parse("↻90°"), PageQuery::Rotation(90));
        assert_eq!(PageQuery::parse("612x792"), PageQuery::Size(612.0, 792.0));
        assert_eq!(PageQuery::parse("595.3 × 841.9"), PageQuery::Size(595.3, 841.9));
        assert_eq!(PageQuery::parse("A4"), PageQuery::Paper("a4".to_owned()));
        // A word that starts with an `r` is not a rotation.
        assert_eq!(PageQuery::parse("letter"), PageQuery::Paper("letter".to_owned()));
    }

    #[test]
    fn the_filter_finds_the_pages_of_the_box_fixtures() {
        let boxes = fixture("page-boxes");
        let matrix = fixture("box-matrix");

        // page-boxes: page 1 /Rotate 90, 280 x 380; page 2 300 x 400; page 3 400 x 400 (UserUnit 2).
        assert_eq!(matching_pages(&boxes, "rot 90"), [0]);
        assert_eq!(matching_pages(&boxes, "2"), [1]);
        assert_eq!(matching_pages(&boxes, "300x400"), [1]);
        assert_eq!(matching_pages(&boxes, "400 x 300"), [1], "either orientation");
        assert_eq!(matching_pages(&boxes, "9"), Vec::<PageIx>::new());
        assert_eq!(matching_pages(&boxes, "").len(), 3);
        // box-matrix: pages 2 and 3 are turned by 180 and 270.
        assert_eq!(matching_pages(&matrix, "rot 180"), [1]);
        assert_eq!(matching_pages(&matrix, "rot 270"), [2]);
        assert_eq!(matching_pages(&matrix, "rot 0"), [0, 3]);
        assert_eq!(matching_pages(&matrix, "a4"), Vec::<PageIx>::new());
    }

    #[test]
    fn provenance_says_where_a_box_comes_from() {
        assert_eq!(provenance(BoxKind::Trim, BoxSource::Own), "own");
        assert_eq!(
            provenance(BoxKind::Media, BoxSource::Inherited(ObjRef::new(3, 0))),
            "inherited · /Pages 3 0 R"
        );
        assert_eq!(
            provenance(BoxKind::Crop, BoxSource::Default(BoxKind::Media)),
            "not set · defaults to MediaBox"
        );
        assert_eq!(
            provenance(BoxKind::Bleed, BoxSource::Default(BoxKind::Crop)),
            "not set · defaults to CropBox"
        );
        assert_eq!(
            provenance(BoxKind::Media, BoxSource::Default(BoxKind::Media)),
            "not set · defaults to US Letter"
        );
    }

    #[test]
    fn transform_values_follow_the_unit() {
        let doc = fixture("page-boxes");
        let (pt, mm) = (Lengths::new(Unit::Pt, 1.0), Lengths::new(Unit::Mm, 2.0));
        let framed = doc.page(0).unwrap();
        let scaled = doc.page(2).unwrap();

        assert_eq!(display_size_text(framed, Unit::Pt), "280 × 380 pt");
        assert_eq!(user_unit_text(1.0, &pt), "1.0 (1 unit = 1 pt)");
        assert_eq!(user_unit_text(2.0, &mm), "2.0 (1 unit = 0.71 mm)");
        // 200 x 200 user units at UserUnit 2 are 400 x 400 pt = 141.11 mm.
        assert_eq!(display_size_text(scaled, Unit::Pt), "400 × 400 pt");
        assert_eq!(display_size_text(scaled, Unit::Mm), "141.11 × 141.11 mm");
        assert_eq!(decimal(2.5), "2.5");
    }
}
