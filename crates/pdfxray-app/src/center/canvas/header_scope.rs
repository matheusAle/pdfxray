//! Header scope on the canvas: the direction a table header cell heads, drawn in its `TH` chip,
//! and the echo of the cells it heads.
//!
//! The chip of a `TH` carries an arrow after its name, in the chip's text colour: → for `Row`, ↓
//! for `Column`, and for `Both` two arrows leaving one corner. A header without `/Scope` has an
//! outlined chip and its arrow at 60 %: the direction its position gives. A `/Scope` that the
//! position contradicts adds an amber ring and an amber `!`, so the conflict does not rest on
//! colour alone. A header that cells name by `/ID` adds a `#`: those IDs decide which cells it
//! heads. The role colour stays the table's whatever the scope.
//!
//! While a `TH` is hovered or selected, a band in the table role colour runs over the cells it
//! heads, from 25 % next to the header to 3 % at the far end, with a chevron on each row (or
//! column) it reaches. Otherwise nothing is drawn, to keep the canvas calm.

use std::collections::{HashMap, HashSet};

use egui::epaint::Mesh;
use egui::{Color32, Pos2, Rect as UiRect, Shape, Stroke, pos2, vec2};
use pdfxray_core::{HeaderScope, NodeId, PageIx, Rect, ScopeSource, TableCell, TableScope};

use super::geometry::PageGeometry;
use super::table_overlay;
use crate::state::AppState;

/// The echo's band next to the header and at its far end.
const ECHO_NEAR: f32 = 0.25;
const ECHO_FAR: f32 = 0.03;

/// Half the width of an echo chevron, and its stroke.
const CHEVRON: f32 = 4.0;
const CHEVRON_STROKE: f32 = 1.5;

/// `Row`, `Column` or `Both`, as `/Scope` writes it.
pub const fn scope_name(scope: TableScope) -> &'static str {
    match scope {
        TableScope::Row => "Row",
        TableScope::Column => "Column",
        TableScope::Both => "Both",
    }
}

/// The inspector's Scope row: `Column · declared`, `Column · inferred`, `Row · declared,
/// conflicts with position`.
pub fn scope_text(scope: &HeaderScope) -> String {
    let source = match scope.source {
        ScopeSource::Declared => "declared",
        ScopeSource::Inferred => "inferred",
        ScopeSource::Conflict => "declared, conflicts with position",
    };
    format!("{} · {source}", scope_name(scope.scope))
}

/// How many cells a header heads: `2 cells`.
pub fn governs_text(count: usize) -> String {
    format!("{count} cell{}", if count == 1 { "" } else { "s" })
}

/// The accessible name of a `TH` chip: `TH · Column · declared`, and `· IDs decide` when cells
/// name it by `/ID`.
pub fn chip_name(name: &str, scope: &HeaderScope) -> String {
    let ids = if scope.named_by.is_empty() {
        ""
    } else {
        " · IDs decide"
    };
    format!("{name} · {}{ids}", scope_text(scope))
}

/// What a header with this scope heads, in words.
const fn reach(scope: TableScope) -> &'static str {
    match scope {
        TableScope::Row => "the cells to its right",
        TableScope::Column => "the cells below",
        TableScope::Both => "the cells to its right and below",
    }
}

/// `row`, `column` or `row and column`.
const fn kind_word(scope: TableScope) -> &'static str {
    match scope {
        TableScope::Row => "row",
        TableScope::Column => "column",
        TableScope::Both => "row and column",
    }
}

/// The first line of a chip's hint: `TH South · says Column`, or `TH Q1 · Column`.
pub fn hint_title(name: &str, text: &str, scope: &HeaderScope) -> String {
    let says = if scope.source == ScopeSource::Conflict {
        "says "
    } else {
        ""
    };
    let text = if text.is_empty() { "(empty)" } else { text };
    format!("{name} {text} · {says}{}", scope_name(scope.scope))
}

/// What the scope means, in plain words, for a chip's hint and the inspector's conflict card.
pub fn hint(scope: &HeaderScope) -> String {
    let position = scope.place.reads_as();
    let mut text = match scope.source {
        ScopeSource::Declared => format!(
            "/Scope /{}: governs {}.",
            scope_name(scope.scope),
            reach(scope.scope)
        ),
        ScopeSource::Inferred => format!(
            "No /Scope: inferred from position. It sits in {}, so it reads as a {} header.",
            scope.place.phrase(),
            kind_word(position)
        ),
        ScopeSource::Conflict => format!(
            "Says {}, but sits in {}, so it reads as a {} header. Screen readers will announce it for \
             {} instead of its {}.",
            scope_name(scope.scope),
            scope.place.phrase(),
            kind_word(position),
            reach(scope.scope),
            kind_word(position)
        ),
    };
    if !scope.named_by.is_empty() {
        text.push_str(" Cells name it by /ID: their /Headers decide which cells it heads.");
    }
    text
}

/// A header cell on a page: its scope, the start of its text and the cells it heads.
#[derive(Debug, Clone)]
pub(super) struct HeaderMark {
    pub(super) scope: HeaderScope,
    pub(super) text: String,
    pub(super) echo: Echo,
}

/// The cells a header heads, as the echo draws them: page-space slots of the grid.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Echo {
    /// The header's own slot.
    header: Rect,
    /// The cells past its last row: the column part.
    down: Vec<Rect>,
    /// The others, beside it: the row part (and any cell a `/Headers` list places elsewhere).
    right: Vec<Rect>,
}

impl Echo {
    /// The echo of header cell `header` of `cells`, with the slot of each cell on the page by its
    /// index; `None` when the header is not on the page.
    pub(super) fn of(cells: &[TableCell], header: usize, slots: &HashMap<usize, Rect>) -> Option<Self> {
        let head = cells.get(header)?;
        let scope = head.header_scope.as_ref()?;
        let mut echo = Self {
            header: *slots.get(&header)?,
            down: Vec::new(),
            right: Vec::new(),
        };
        let governs: HashSet<&NodeId> = scope.governs.iter().collect();
        for (at, cell) in cells.iter().enumerate() {
            if !governs.contains(&cell.node) {
                continue;
            }
            let Some(slot) = slots.get(&at) else { continue };
            if cell.row >= head.row + head.row_span {
                echo.down.push(*slot);
            } else {
                echo.right.push(*slot);
            }
        }
        Some(echo)
    }
}

/// The header cells of the tables on `page`, by element.
pub(super) fn marks_on(state: &mut AppState, page: PageIx) -> HashMap<NodeId, HeaderMark> {
    let boxes = state.tag_boxes(page);
    let mut marks = HashMap::new();
    for data in state.tables_on(page) {
        let slots: HashMap<usize, Rect> = table_overlay::placed(&data, &boxes).into_iter().collect();
        for (at, cell) in data.model.cells.iter().enumerate() {
            let (Some(scope), Some(echo)) = (
                cell.header_scope.as_ref(),
                Echo::of(&data.model.cells, at, &slots),
            ) else {
                continue;
            };
            marks.insert(
                cell.node.clone(),
                HeaderMark {
                    scope: scope.clone(),
                    text: data.text(at).to_owned(),
                    echo,
                },
            );
        }
    }
    marks
}

/// The shapes of the echo of a header on the page at `geo`, in `color`: one band over the cells
/// below it and one over the cells beside it, each fading away from the header, with a chevron
/// on each row or column it reaches.
pub(super) fn echo_shapes(geo: &PageGeometry, echo: &Echo, color: Color32) -> Vec<Shape> {
    let header = geo.rect_to_screen(echo.header);
    let mut shapes = Vec::new();
    for part in [&echo.down, &echo.right] {
        let cells: Vec<UiRect> = part.iter().map(|rect| geo.rect_to_screen(*rect)).collect();
        let Some(band) = cells.iter().copied().reduce(UiRect::union) else {
            continue;
        };
        let away = Away::of(header, band);
        shapes.push(gradient(band, away, color));
        // One chevron per row (a band that fades down) or column (one that fades sideways).
        let mut lines: Vec<f32> = cells
            .iter()
            .map(|cell| if away.vertical() { cell.min.y } else { cell.min.x })
            .collect();
        lines.sort_by(f32::total_cmp);
        lines.dedup_by(|a, b| (*a - *b).abs() < 1.0);
        for line in lines {
            let at = if away.vertical() {
                let x = header
                    .center()
                    .x
                    .clamp(band.min.x + CHEVRON, band.max.x - CHEVRON);
                let y = if away == Away::Down {
                    line + 6.0
                } else {
                    line - 6.0
                };
                pos2(x, y)
            } else {
                let y = header
                    .center()
                    .y
                    .clamp(band.min.y + CHEVRON, band.max.y - CHEVRON);
                let x = if away == Away::Right {
                    line + 6.0
                } else {
                    line - 6.0
                };
                pos2(x, y)
            };
            shapes.push(chevron(at, away, color));
        }
    }
    shapes
}

/// Which way a band leads away from its header, on screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Away {
    Down,
    Up,
    Right,
    Left,
}

impl Away {
    /// From `header` to `band`: down when the band starts below the header, and so on; a band
    /// that overlaps the header leads right.
    fn of(header: UiRect, band: UiRect) -> Self {
        let centre = header.center();
        if centre.y <= band.min.y {
            Self::Down
        } else if centre.y >= band.max.y {
            Self::Up
        } else if centre.x >= band.max.x {
            Self::Left
        } else {
            Self::Right
        }
    }

    const fn vertical(self) -> bool {
        matches!(self, Self::Down | Self::Up)
    }
}

/// `rect` filled with `color` from 25 % on the side next to the header to 3 % at the far side.
fn gradient(rect: UiRect, away: Away, color: Color32) -> Shape {
    let (near, far) = (color.gamma_multiply(ECHO_NEAR), color.gamma_multiply(ECHO_FAR));
    // Corners clockwise from the top left.
    let colors = match away {
        Away::Down => [near, near, far, far],
        Away::Up => [far, far, near, near],
        Away::Right => [near, far, far, near],
        Away::Left => [far, near, near, far],
    };
    let mut mesh = Mesh::default();
    for (corner, color) in [
        rect.left_top(),
        rect.right_top(),
        rect.right_bottom(),
        rect.left_bottom(),
    ]
    .into_iter()
    .zip(colors)
    {
        mesh.colored_vertex(corner, color);
    }
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    Shape::mesh(mesh)
}

/// A chevron at `at` pointing `away`.
fn chevron(at: Pos2, away: Away, color: Color32) -> Shape {
    let (c, h) = (CHEVRON, CHEVRON / 2.0);
    let points = match away {
        Away::Down => vec![at + vec2(-c, -h), at + vec2(0.0, h), at + vec2(c, -h)],
        Away::Up => vec![at + vec2(-c, h), at + vec2(0.0, -h), at + vec2(c, h)],
        Away::Right => vec![at + vec2(-h, -c), at + vec2(h, 0.0), at + vec2(-h, c)],
        Away::Left => vec![at + vec2(h, -c), at + vec2(-h, 0.0), at + vec2(h, c)],
    };
    Shape::line(points, Stroke::new(CHEVRON_STROKE, color))
}

#[cfg(test)]
#[allow(clippy::float_cmp)]
mod tests {
    use pdfxray_core::{HeaderPlace, RowGroup};
    use pretty_assertions::assert_eq;

    use super::*;

    fn scope(scope: TableScope, source: ScopeSource, place: HeaderPlace) -> HeaderScope {
        HeaderScope {
            scope,
            source,
            place,
            named_by: Vec::new(),
            governs: Vec::new(),
        }
    }

    #[test]
    fn the_scope_row_says_the_direction_and_where_it_comes_from() {
        assert_eq!(
            scope_text(&scope(
                TableScope::Column,
                ScopeSource::Declared,
                HeaderPlace::TopRow
            )),
            "Column · declared"
        );
        assert_eq!(
            scope_text(&scope(
                TableScope::Column,
                ScopeSource::Inferred,
                HeaderPlace::TopRow
            )),
            "Column · inferred"
        );
        assert_eq!(
            scope_text(&scope(
                TableScope::Row,
                ScopeSource::Conflict,
                HeaderPlace::TopRow
            )),
            "Row · declared, conflicts with position"
        );
        assert_eq!(
            [governs_text(0), governs_text(1), governs_text(2)],
            ["0 cells", "1 cell", "2 cells"]
        );
    }

    #[test]
    fn a_hint_explains_each_state_in_plain_words() {
        let conflict = scope(
            TableScope::Column,
            ScopeSource::Conflict,
            HeaderPlace::FirstColumn,
        );
        assert_eq!(hint_title("TH", "South", &conflict), "TH South · says Column");
        assert_eq!(
            hint(&conflict),
            "Says Column, but sits in the first column, so it reads as a row header. Screen readers \
             will announce it for the cells below instead of its row."
        );
        let inferred = scope(TableScope::Column, ScopeSource::Inferred, HeaderPlace::TopRow);
        assert_eq!(
            hint(&inferred),
            "No /Scope: inferred from position. It sits in the top row, so it reads as a column header."
        );
        let mut named = scope(TableScope::Row, ScopeSource::Declared, HeaderPlace::FirstColumn);
        assert_eq!(hint(&named), "/Scope /Row: governs the cells to its right.");
        assert_eq!(chip_name("TH", &named), "TH · Row · declared");
        named.named_by = vec![NodeId::Document];
        assert_eq!(chip_name("TH", &named), "TH · Row · declared · IDs decide");
        // Smoke: the hint of a named header says the IDs decide.
        assert!(hint(&named).ends_with("their /Headers decide which cells it heads."));
    }

    fn cell(node: u32, row: u32, col: u32, row_span: u32) -> TableCell {
        TableCell {
            node: NodeId::StructElem(pdfxray_core::ObjRef::new(node, 0)),
            header: false,
            group: RowGroup::Body,
            row,
            col,
            row_span,
            col_span: 1,
            scope: None,
            id: None,
            headers: Vec::new(),
            missing_header_ids: Vec::new(),
            header_scope: None,
        }
    }

    #[test]
    fn the_echo_splits_the_cells_a_header_heads_into_below_and_beside() {
        // A corner header two rows tall, a cell beside it, one below it, and one it does not head.
        let mut corner = cell(1, 0, 0, 2);
        let mut both = scope(TableScope::Both, ScopeSource::Declared, HeaderPlace::TopRow);
        both.governs = vec![cell(2, 1, 1, 1).node, cell(3, 2, 0, 1).node];
        corner.header_scope = Some(both);
        let cells = [corner, cell(2, 1, 1, 1), cell(3, 2, 0, 1), cell(4, 2, 1, 1)];
        let slots: HashMap<usize, Rect> = [0.0, 1.0, 2.0, 3.0]
            .into_iter()
            .enumerate()
            .map(|(at, x)| (at, Rect::new(x, 0.0, x + 1.0, 1.0)))
            .collect();

        let echo = Echo::of(&cells, 0, &slots).unwrap();

        assert_eq!(echo.header, slots[&0]);
        assert_eq!(echo.right, [slots[&1]]);
        assert_eq!(echo.down, [slots[&2]]);
        // Smoke: a header that is not on the page has no echo, and a data cell none either.
        let mut off_page = slots.clone();
        off_page.remove(&0);
        assert_eq!(Echo::of(&cells, 0, &off_page), None);
        assert_eq!(Echo::of(&cells, 1, &slots), None);
    }

    #[test]
    fn a_band_leads_away_from_its_header() {
        let header = UiRect::from_min_size(pos2(0.0, 0.0), vec2(10.0, 10.0));
        let below = UiRect::from_min_size(pos2(0.0, 12.0), vec2(10.0, 30.0));
        let beside = UiRect::from_min_size(pos2(12.0, 0.0), vec2(30.0, 10.0));
        let before = UiRect::from_min_size(pos2(-40.0, 0.0), vec2(30.0, 10.0));

        assert_eq!(Away::of(header, below), Away::Down);
        assert_eq!(Away::of(header, beside), Away::Right);
        assert_eq!(Away::of(header, before), Away::Left);
        assert_eq!(Away::of(below, header), Away::Up);
    }
}
