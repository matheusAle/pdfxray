//! The Diagnostics tab: open errors, engine problems and the reader's findings.
//!
//! On the Pages entry it is narrowed to what concerns page boxes and rotation. Besides the
//! reader's `box-outside-mediabox` it lists two notes the app derives from the pages themselves
//! (the reader does not report them as problems): `box-inherited` and `rotate-nonzero`.

use std::collections::BTreeMap;

use egui::{
    Align, Label, Layout, Rect, RichText, Role, ScrollArea, Sense, Ui, UiBuilder, WidgetInfo, pos2, vec2,
};
use pdfxray_core::{BoxKind, BoxSource, DocInfo, Document, NodeId, ObjRef, PageIx, Severity};

use super::table;
use crate::components::Icon;
use crate::state::AppState;
use crate::theme::{fonts, tokens};

/// Shown when the file produced no diagnostics.
pub const NO_DIAGNOSTICS: &str = "No diagnostics.";

/// Shown when the filter leaves nothing to list.
pub const NO_PAGE_BOX_NOTES: &str = "Nothing to note about the page boxes.";

/// The note at the right end of the tab bar while the list is narrowed to page boxes.
pub const FILTER_NOTE: &str = "filtered: page boxes";

/// The note at the right end of the tab bar while the worst problems are listed first.
pub const SORTED_NOTE: &str = "sorted by severity";

/// Name of the link that lifts the filter.
pub const CLEAR_FILTER_LABEL: &str = "Clear filter";

/// One line of the tab.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    /// How serious it is.
    pub severity: Severity,
    /// The diagnostic code.
    pub code: &'static str,
    /// What is wrong, or worth knowing.
    pub message: String,
    /// The right column: where it is.
    pub place: String,
    /// What a click selects.
    pub target: NodeId,
}

/// The note at the right end of the tab bar, if the list is filtered.
pub fn header_note(state: &AppState) -> Option<String> {
    if state.drawer.page_boxes_only {
        Some(FILTER_NOTE.to_owned())
    } else {
        state.drawer.sorted_by_severity.then(|| SORTED_NOTE.to_owned())
    }
}

/// Every diagnostic the reader found, one row each.
pub fn all_rows(info: &DocInfo) -> Vec<Row> {
    info.diagnostics
        .iter()
        .zip(0..)
        .map(|(d, index)| Row {
            severity: d.severity,
            code: d.code,
            message: d.message.clone(),
            place: d.object.map_or_else(
                || {
                    if d.code.starts_with("startxref") || d.code.starts_with("trailer") {
                        "trailer".to_owned()
                    } else {
                        format!("bytes {}..{}", d.span[0], d.span[1])
                    }
                },
                |object| object.to_string(),
            ),
            target: NodeId::Diagnostic(index),
        })
        .collect()
}

/// The rows about page boxes and rotation: the reader's `box-outside-mediabox`, then the
/// inherited boxes (one row per box and `/Pages` node, listing its pages) and the rotated pages.
pub fn page_box_rows(doc: &Document, info: &DocInfo) -> Vec<Row> {
    let pages: Vec<(PageIx, &pdfxray_core::PageInfo)> = (0..PageIx::try_from(doc.page_count())
        .unwrap_or(PageIx::MAX))
        .filter_map(|ix| doc.page(ix).map(|page| (ix, page)))
        .collect();
    let mut rows = Vec::new();
    for diagnostic in info
        .diagnostics
        .iter()
        .filter(|d| d.code == "box-outside-mediabox")
    {
        let page = pages
            .iter()
            .find(|(_, info)| info.obj.is_some() && info.obj == diagnostic.object);
        let kind = BoxKind::ALL
            .into_iter()
            .find(|kind| diagnostic.message.starts_with(kind.key()));
        let Some(((ix, _), kind)) = page.zip(kind) else {
            continue;
        };
        rows.push(Row {
            severity: diagnostic.severity,
            code: diagnostic.code,
            message: diagnostic.message.clone(),
            place: format!(
                "page {} · {}",
                ix + 1,
                diagnostic.object.map_or_else(String::new, |o| o.to_string())
            ),
            target: NodeId::PageBox { page: *ix, kind },
        });
    }
    let mut inherited: BTreeMap<(BoxKind, ObjRef), Vec<PageIx>> = BTreeMap::new();
    for (ix, info) in &pages {
        for page_box in &info.boxes {
            if let BoxSource::Inherited(from) = page_box.source {
                inherited.entry((page_box.kind, from)).or_default().push(*ix);
            }
        }
    }
    for ((kind, from), pages) in inherited {
        rows.push(Row {
            severity: Severity::Info,
            code: "box-inherited",
            message: format!(
                "{} for {} comes from /Pages {from}, not the page itself",
                kind.key(),
                page_list(&pages)
            ),
            place: format!(
                "{} {}",
                pages.len(),
                if pages.len() == 1 { "page" } else { "pages" }
            ),
            target: NodeId::PageBox { page: pages[0], kind },
        });
    }
    for (ix, info) in &pages {
        if info.rotate != 0 {
            rows.push(Row {
                severity: Severity::Info,
                code: "rotate-nonzero",
                message: rotation_message(*ix, info),
                place: format!("page {}", ix + 1),
                target: NodeId::Page(*ix),
            });
        }
    }
    rows
}

/// `page 4`, `pages 1–3` or `pages 1, 3, 7–9`: zero-based `pages`, ascending, as one-based runs.
fn page_list(pages: &[PageIx]) -> String {
    let mut runs: Vec<(PageIx, PageIx)> = Vec::new();
    for &page in pages {
        match runs.last_mut() {
            Some((_, end)) if *end + 1 == page => *end = page,
            _ => runs.push((page, page)),
        }
    }
    let text: Vec<String> = runs
        .iter()
        .map(|&(from, to)| match to - from {
            0 => (from + 1).to_string(),
            1 => format!("{}, {}", from + 1, to + 1),
            _ => format!("{}–{}", from + 1, to + 1),
        })
        .collect();
    format!(
        "{} {}",
        if pages.len() == 1 { "page" } else { "pages" },
        text.join(", ")
    )
}

/// `Page 4 is rotated 90°: shown landscape, coordinates stay portrait`.
fn rotation_message(ix: PageIx, info: &pdfxray_core::PageInfo) -> String {
    let orientation = |wide: bool| if wide { "landscape" } else { "portrait" };
    let (width, height) = (info.crop_box.width(), info.crop_box.height());
    let kept = orientation(width > height);
    let what = if info.rotate % 180 == 90 {
        format!("shown {}, coordinates stay {kept}", orientation(height > width))
    } else {
        format!("shown upside down, coordinates stay {kept}")
    };
    format!("Page {} is rotated {}°: {what}", ix + 1, info.rotate)
}

pub(super) fn ui(ui: &mut Ui, state: &mut AppState) {
    if let Some(error) = &state.open_error {
        ui.label(RichText::new(error).color(tokens::ERR));
    }
    let Some(loaded) = &state.document else {
        return;
    };
    let info = &loaded.info;
    if let Some(error) = info.engine_error {
        ui.label(RichText::new(format!("Rendering engine: {error:?}")).color(tokens::WARN));
    }
    let filtered = state.drawer.page_boxes_only;
    let mut rows = if filtered {
        page_box_rows(&loaded.doc, info)
    } else {
        all_rows(info)
    };
    if state.drawer.sorted_by_severity {
        // Errors, then warnings, then notes; a stable sort keeps the file's order within each.
        rows.sort_by_key(|row| std::cmp::Reverse(row.severity));
    }
    let mut clear = false;
    if filtered {
        let width = ui.available_width();
        ui.allocate_ui_with_layout(vec2(width, 18.0), Layout::right_to_left(Align::Center), |ui| {
            clear = ui
                .link(RichText::new(CLEAR_FILTER_LABEL).font(fonts::ui(tokens::text::SMALL)))
                .clicked();
        });
    }
    if rows.is_empty() && info.engine_error.is_none() {
        let text = if filtered {
            NO_PAGE_BOX_NOTES
        } else {
            NO_DIAGNOSTICS
        };
        ui.label(RichText::new(text).color(tokens::TEXT_3));
    }
    let mut clicked = None;
    ScrollArea::vertical()
        .id_salt("diagnostics")
        .auto_shrink(false)
        .show(ui, |ui| {
            for (n, row) in rows.iter().enumerate() {
                if line(ui, n, row) {
                    clicked = Some(row.target.clone());
                }
            }
        });
    if clear {
        state.drawer.page_boxes_only = false;
    }
    if let Some(target) = clicked {
        state.select(target);
    }
}

const ICON_W: f32 = 22.0;
/// Width of the code column, and the least it is cut to.
const CODE_W: f32 = 190.0;
const CODE_MIN_W: f32 = 120.0;
/// Width of the place column, and the least it is cut to (`3 0 R`).
const PLACE_W: f32 = 130.0;
const PLACE_MIN_W: f32 = 56.0;
/// The message is what the row is for: the other columns give way to keep it this wide.
const MESSAGE_MIN_W: f32 = 150.0;
/// Space a row spends outside its columns: the left inset and icon, the gaps and the right inset.
const FIXED_W: f32 = 4.0 + ICON_W + 6.0 + 8.0 + 6.0;

/// The widths of the code and place columns in a row `width` wide: their own, unless the message
/// would then be narrower than [`MESSAGE_MIN_W`]; the place gives way first, then the code.
fn side_columns(width: f32) -> (f32, f32) {
    let missing = (MESSAGE_MIN_W - (width - FIXED_W - CODE_W - PLACE_W)).max(0.0);
    let place = (PLACE_W - missing).max(PLACE_MIN_W);
    let code = (CODE_W - (missing - (PLACE_W - place))).clamp(CODE_MIN_W, CODE_W);
    (code, place)
}

/// One row; true when it was clicked.
fn line(ui: &mut Ui, n: usize, row: &Row) -> bool {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), tokens::ROW_H), Sense::hover());
    let response = ui.interact(rect, ui.make_persistent_id(("diagnostic-row", n)), Sense::click());
    response.widget_info(|| WidgetInfo::labeled(Role::Button, true, row.code));
    let response = table::row_background(ui, rect, response, false);
    let (icon, color) = match row.severity {
        Severity::Info => (Icon::Info, tokens::ACCENT),
        Severity::Warning => (Icon::Warning, tokens::WARN),
        Severity::Error => (Icon::Warning, tokens::ERR),
    };
    let left = rect.min.x + 4.0;
    icon.paint(
        ui,
        Rect::from_min_size(pos2(left, rect.min.y), vec2(ICON_W, rect.height())),
        14.0,
        color,
    );
    let (code_w, place_w) = side_columns(rect.width());
    let code_x = left + ICON_W;
    let place_x = rect.max.x - 6.0 - place_w;
    let message_x = code_x + code_w;
    cell(
        ui,
        Rect::from_min_max(pos2(code_x, rect.min.y), pos2(message_x - 6.0, rect.max.y)),
        Layout::left_to_right(Align::Center),
        RichText::new(row.code)
            .font(fonts::mono_bold(tokens::mono::BODY))
            .color(color),
    );
    cell(
        ui,
        Rect::from_min_max(pos2(message_x, rect.min.y), pos2(place_x - 8.0, rect.max.y)),
        Layout::left_to_right(Align::Center),
        RichText::new(&row.message)
            .font(fonts::ui(tokens::text::BODY))
            .color(tokens::TEXT_2),
    );
    cell(
        ui,
        Rect::from_min_max(pos2(place_x, rect.min.y), pos2(rect.max.x - 6.0, rect.max.y)),
        Layout::right_to_left(Align::Center),
        RichText::new(&row.place)
            .font(fonts::mono(tokens::mono::SMALL))
            .color(tokens::TEXT_3),
    );
    response.clicked()
}

fn cell(ui: &mut Ui, rect: Rect, layout: Layout, text: RichText) {
    ui.scope_builder(UiBuilder::new().max_rect(rect).layout(layout), |ui| {
        ui.add(Label::new(text).selectable(false).truncate());
    });
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;
    use crate::test_support::fixture;

    #[test]
    fn the_side_columns_give_way_so_the_message_keeps_its_room() {
        // Wide enough: both keep their own width.
        assert_eq!(side_columns(900.0), (CODE_W, PLACE_W));
        // The drawer at the narrowest window (388 points of row): the message is not squeezed to a
        // few letters, the place and then the code are cut.
        let (code, place) = side_columns(388.0);
        let message = 388.0 - FIXED_W - code - place;
        assert!(message >= MESSAGE_MIN_W - 0.5, "message {message}");
        assert_eq!((place, code <= CODE_W), (PLACE_MIN_W, true));
        assert!((CODE_MIN_W..CODE_W).contains(&code), "code {code}");
        // Smoke: in between, only the place gives way.
        let (code, place) = side_columns(FIXED_W + CODE_W + PLACE_W + MESSAGE_MIN_W - 30.0);
        assert_eq!((code, place), (CODE_W, PLACE_W - 30.0));
    }

    fn rows_of(name: &str) -> Vec<Row> {
        let doc = fixture(name);
        page_box_rows(&doc, &doc.info())
    }

    fn summary(rows: &[Row]) -> Vec<(Severity, &'static str, &str, &str)> {
        rows.iter()
            .map(|r| (r.severity, r.code, r.message.as_str(), r.place.as_str()))
            .collect()
    }

    #[test]
    fn page_boxes_lists_the_problem_the_inherited_box_and_the_turned_page() {
        let rows = rows_of("page-boxes");

        assert_eq!(
            summary(&rows),
            [
                (
                    Severity::Warning,
                    "box-outside-mediabox",
                    "TrimBox of Page 3 [150 150 250 250] extends outside its MediaBox [0 0 200 200]",
                    "page 3 · 5 0 R"
                ),
                (
                    Severity::Info,
                    "box-inherited",
                    "MediaBox for page 2 comes from /Pages 2 0 R, not the page itself",
                    "1 page"
                ),
                (
                    Severity::Info,
                    "rotate-nonzero",
                    "Page 1 is rotated 90°: shown portrait, coordinates stay landscape",
                    "page 1"
                ),
            ]
        );
        // A click selects the box, the box, and the page.
        assert_eq!(
            rows.iter().map(|r| r.target.clone()).collect::<Vec<_>>(),
            [
                NodeId::PageBox {
                    page: 2,
                    kind: BoxKind::Trim
                },
                NodeId::PageBox {
                    page: 1,
                    kind: BoxKind::Media
                },
                NodeId::Page(0),
            ]
        );
    }

    #[test]
    fn inherited_boxes_are_grouped_by_the_pages_node_and_rotations_describe_the_turn() {
        let rows = rows_of("box-matrix");

        assert_eq!(
            summary(&rows),
            [
                (
                    Severity::Info,
                    "box-inherited",
                    "MediaBox for page 3 comes from /Pages 2 0 R, not the page itself",
                    "1 page"
                ),
                (
                    Severity::Info,
                    "box-inherited",
                    "CropBox for pages 3, 4 comes from /Pages 2 0 R, not the page itself",
                    "2 pages"
                ),
                (
                    Severity::Info,
                    "rotate-nonzero",
                    "Page 2 is rotated 180°: shown upside down, coordinates stay landscape",
                    "page 2"
                ),
                (
                    Severity::Info,
                    "rotate-nonzero",
                    "Page 3 is rotated 270°: shown landscape, coordinates stay portrait",
                    "page 3"
                ),
            ]
        );
    }

    #[test]
    fn a_plain_file_has_nothing_to_note_and_page_lists_read_as_runs() {
        assert_eq!(rows_of("minimal-one-page"), []);
        assert_eq!(page_list(&[3]), "page 4");
        assert_eq!(page_list(&[0, 1, 2]), "pages 1–3");
        assert_eq!(page_list(&[0, 2, 6, 7, 8]), "pages 1, 3, 7–9");
    }

    #[test]
    fn every_diagnostic_of_the_reader_is_a_row_with_its_place() {
        let doc = fixture("page-boxes");
        let rows = all_rows(&doc.info());

        assert_eq!(rows.len(), doc.info().diagnostics.len());
        assert_eq!(rows[0].code, "box-outside-mediabox");
        assert_eq!(rows[0].place, "5 0 R");
        assert_eq!(rows[0].target, NodeId::Diagnostic(0));
    }
}
