//! The Objects entry: the document's own objects, the resources of the page on the canvas and
//! every object of the file, as one virtualised list.

use egui::{Align, Layout, Rect, Response, RichText, ScrollArea, Sense, Ui, UiBuilder, vec2};
use pdfxray_core::numeric::f32_from_usize;
use pdfxray_core::{NodeId, ResKind};

use super::thousands;
use crate::components::{Annotation, Tone, TreeRow, context_menu};
use crate::state::{AppState, ObjectRow, ResourceGroup};
use crate::theme::{fonts, tokens};

/// Shown when the file has no object to list. A filter that leaves none says so with its query.
pub const NO_OBJECTS: &str = "No objects.";

/// What the header says after "Objects": `412 objects · 3 revs`.
pub(super) fn header_text(state: &mut AppState) -> Option<String> {
    let loaded = state.document.as_ref()?;
    let revisions = loaded.info.revision_count;
    let viewed = state.viewed_revision();
    let count = state.object_rows().len();
    Some(match viewed {
        Some(rev) => format!("{} objects · rev {} of {revisions}", thousands(count), rev + 1),
        None => format!(
            "{} objects · {revisions} rev{}",
            thousands(count),
            if revisions == 1 { "" } else { "s" }
        ),
    })
}

/// One line of the list, before it is drawn.
#[derive(Debug, Clone, Copy)]
enum Line {
    Section(Section),
    Document(usize),
    Group(usize),
    Resource(usize, usize),
    /// An index into the filtered objects.
    Object(usize),
    /// The line that says no object is listed, under the heading of the list.
    Empty,
}

#[derive(Debug, Clone, Copy)]
enum Section {
    Document,
    Resources,
    Objects,
}

pub(super) fn ui(ui: &mut Ui, state: &mut AppState) {
    let words: Vec<String> = state
        .nav
        .filter
        .split_whitespace()
        .map(str::to_lowercase)
        .collect();
    let rows = state.object_rows();
    let document_rows = state.document_rows();
    let read_only = state.viewed_revision().is_some();
    let groups = if read_only {
        std::sync::Arc::default()
    } else {
        state.page_resources()
    };
    let matches = |text: &str| {
        let text = text.to_lowercase();
        words.iter().all(|w| text.contains(w))
    };

    let document: Vec<usize> = (0..document_rows.len())
        .filter(|&i| {
            let row = &document_rows[i];
            matches(&format!("{} {}", row.label, row.detail.as_deref().unwrap_or("")))
        })
        .collect();
    let resources: Vec<ResourceGroup> = groups
        .iter()
        .filter_map(|group| {
            let rows: Vec<_> = group
                .rows
                .iter()
                .filter(|r| matches(&format!("{} {} {}", group.kind.key(), r.name, r.detail)))
                .cloned()
                .collect();
            (!rows.is_empty()).then_some(ResourceGroup {
                kind: group.kind,
                rows,
            })
        })
        .collect();
    let objects: Vec<usize> = (0..rows.len())
        .filter(|&i| words.iter().all(|w| rows[i].haystack.contains(w)))
        .collect();

    let filtering = !words.is_empty();
    let mut lines = Vec::new();
    if !document.is_empty() {
        lines.push(Line::Section(Section::Document));
        lines.extend(document.iter().map(|&i| Line::Document(i)));
    }
    if !resources.is_empty() {
        lines.push(Line::Section(Section::Resources));
        for (g, group) in resources.iter().enumerate() {
            lines.push(Line::Group(g));
            if state.objects.open_groups.contains(&group.kind) || filtering {
                lines.extend((0..group.rows.len()).map(|r| Line::Resource(g, r)));
            }
        }
    }
    lines.push(Line::Section(Section::Objects));
    let head = lines.len();
    lines.extend((0..objects.len()).map(Line::Object));
    if objects.is_empty() {
        lines.push(Line::Empty);
    }
    let empty = if filtering {
        format!("No objects match “{}”.", state.nav.filter.trim())
    } else {
        NO_OBJECTS.to_owned()
    };

    let selected = state.selection.primary.clone();
    let reveal = reveal_offset(state, selected.as_ref(), &rows, &objects, head, ui);
    let context = Context {
        document_rows: &document_rows,
        resources: &resources,
        rows: &rows,
        objects: &objects,
        selected: selected.as_ref(),
        page: state.canvas.page,
        revision: state.viewed_revision().unwrap_or_else(|| {
            state.document.as_ref().map_or(0, |l| {
                u32::try_from(l.info.revision_count.saturating_sub(1)).unwrap_or(0)
            })
        }),
        open: &state.objects.open_groups,
        filtering,
        empty: &empty,
    };

    let mut clicked = Clicked::None;
    let mut menu = None;
    let mut area = ScrollArea::vertical().id_salt("objects-nav").auto_shrink(false);
    if let Some(offset) = reveal {
        area = area.vertical_scroll_offset(offset);
    }
    area.show_rows(ui, tokens::ROW_H, lines.len(), |ui, range| {
        for index in range {
            if let Some(result) = draw_line(ui, &context, lines[index], index, &mut menu) {
                clicked = result;
            }
        }
    });

    if let Some((id, row)) = menu {
        context_menu::attach(&row, state, &id);
    }
    match clicked {
        Clicked::None => {}
        Clicked::Select(id) => {
            state.objects.seen_selection = Some(id.clone());
            state.select(id);
        }
        Clicked::Group(kind) => {
            if !state.objects.open_groups.remove(&kind) {
                state.objects.open_groups.insert(kind);
            }
        }
        Clicked::Resource(index) => {
            let row = resources.iter().flat_map(|g| g.rows.iter()).nth(index).cloned();
            if let Some(row) = row {
                let target = row.obj.or_else(|| state.resource_target(&row.id));
                let id = target.map_or(row.id, NodeId::Object);
                state.objects.seen_selection = Some(id.clone());
                state.select(id);
            }
        }
    }
    if state.objects.seen_selection != state.selection.primary {
        state.objects.seen_selection.clone_from(&state.selection.primary);
    }
}

/// Where to scroll to when the selection changed somewhere else than in this list.
fn reveal_offset(
    state: &mut AppState,
    selected: Option<&NodeId>,
    rows: &[ObjectRow],
    objects: &[usize],
    head: usize,
    ui: &Ui,
) -> Option<f32> {
    let changed_elsewhere = state.objects.seen_selection.as_ref() != selected;
    if !(changed_elsewhere || state.objects.reveal) {
        return None;
    }
    state.objects.reveal = false;
    let Some(NodeId::Object(r)) = selected else {
        return None;
    };
    let position = objects.iter().position(|&i| rows[i].r == *r)?;
    let row_h = tokens::ROW_H + ui.spacing().item_spacing.y;
    // Centred, as far as the list allows.
    let top = f32_from_usize(head + position) * row_h - ui.available_height() / 2.0;
    Some(top.max(0.0))
}

struct Context<'a> {
    document_rows: &'a [crate::state::DocumentRow],
    resources: &'a [ResourceGroup],
    rows: &'a [ObjectRow],
    objects: &'a [usize],
    selected: Option<&'a NodeId>,
    page: u32,
    /// The revision the trailer row names.
    revision: u32,
    open: &'a std::collections::HashSet<ResKind>,
    filtering: bool,
    /// What the list says when no object is in it.
    empty: &'a str,
}

enum Clicked {
    None,
    Select(NodeId),
    Group(ResKind),
    /// The n-th resource row of the filtered groups.
    Resource(usize),
}

/// The row that was right-clicked, or has its menu open, and the node it stands for.
type Menu = Option<(NodeId, Response)>;

/// Remembers `row` as the one to attach the menu of `node` to when it was right-clicked.
fn menu_for(menu: &mut Menu, row: &Response, node: NodeId) {
    if row.secondary_clicked() || row.context_menu_opened() {
        *menu = Some((node, row.clone()));
    }
}

fn draw_line(ui: &mut Ui, cx: &Context<'_>, line: Line, index: usize, menu: &mut Menu) -> Option<Clicked> {
    let id = ui.make_persistent_id(("objects-line", index));
    match line {
        Line::Section(section) => {
            section_header(ui, cx, section);
            None
        }
        Line::Empty => {
            let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), tokens::ROW_H), Sense::hover());
            ui.scope_builder(
                UiBuilder::new()
                    .max_rect(Rect::from_min_max(rect.min + vec2(6.0, 0.0), rect.max))
                    .layout(Layout::left_to_right(Align::Center)),
                |ui| {
                    ui.label(RichText::new(cx.empty).color(tokens::TEXT_3));
                },
            );
            None
        }
        Line::Document(i) => {
            let row = &cx.document_rows[i];
            let detail = match (&row.detail, &row.id) {
                (Some(detail), _) => Some(detail.clone()),
                (None, NodeId::Trailer) => Some(format!("rev {}", cx.revision + 1)),
                (None, _) => None,
            };
            let result = TreeRow::new(0, &row.label)
                .detail(detail.as_deref())
                .dot(tokens::TEXT_3)
                .selected(cx.selected == Some(&row.id))
                .show(ui, id);
            menu_for(menu, &result.row, row.id.clone());
            result.row.clicked().then(|| Clicked::Select(row.id.clone()))
        }
        Line::Group(g) => {
            let group = &cx.resources[g];
            let annotations = [Annotation::Text(group.rows.len().to_string())];
            let result = TreeRow::new(0, group.kind.key())
                .dot(kind_color(group.kind))
                .expandable(cx.open.contains(&group.kind) || cx.filtering)
                .annotations(&annotations)
                .show(ui, id);
            (result.row.clicked() || result.toggled).then_some(Clicked::Group(group.kind))
        }
        Line::Resource(g, r) => {
            let row = &cx.resources[g].rows[r];
            let detail = match row.obj {
                Some(obj) => format!("{} {obj}", row.detail),
                None => row.detail.clone(),
            };
            let mut annotations = Vec::new();
            if row.not_embedded {
                annotations.push(Annotation::Tag("not embedded".to_owned(), Tone::Warn));
            }
            if row.finding {
                annotations.push(Annotation::Dot {
                    color: tokens::ERR,
                    name: "Validation finding".to_owned(),
                });
            }
            if let Some(uses) = row.uses {
                annotations.push(Annotation::Text(format!("×{uses}")));
            }
            let result = TreeRow::new(1, &row.name)
                .detail(Some(&detail))
                .dot(kind_color(cx.resources[g].kind))
                .annotations(&annotations)
                .show(ui, id);
            menu_for(
                menu,
                &result.row,
                row.obj.map_or_else(|| row.id.clone(), NodeId::Object),
            );
            result.row.clicked().then(|| {
                let before: usize = cx.resources[..g].iter().map(|g| g.rows.len()).sum();
                Clicked::Resource(before + r)
            })
        }
        Line::Object(i) => {
            let row = &cx.rows[cx.objects[i]];
            let label = format!("{} {} obj", row.r.num, row.r.generation);
            let target = NodeId::Object(row.r);
            let mut annotations = Vec::new();
            if let Some(word) = &row.diagnostic {
                annotations.push(Annotation::Tag(word.clone(), Tone::Warn));
            }
            if let Some(rev) = row.changed_in {
                annotations.push(Annotation::Text(format!("rev {rev}")));
            }
            let result = TreeRow::new(0, &label)
                .mono_label()
                .detail(Some(&row.detail))
                .dot(tokens::TEXT_3)
                .selected(cx.selected == Some(&target))
                .annotations(&annotations)
                .show(ui, id);
            menu_for(menu, &result.row, target.clone());
            result.row.clicked().then_some(Clicked::Select(target))
        }
    }
}

fn section_header(ui: &mut Ui, cx: &Context<'_>, section: Section) {
    let (title, count) = match section {
        Section::Document => ("Document".to_owned(), None),
        Section::Resources => (format!("Resources · page {}", cx.page + 1), None),
        // The count is of the rows the filter leaves.
        Section::Objects => ("All objects".to_owned(), Some(cx.objects.len())),
    };
    let width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(vec2(width, tokens::ROW_H), Sense::hover());
    ui.scope_builder(
        UiBuilder::new()
            .max_rect(Rect::from_min_max(rect.min + vec2(6.0, 0.0), rect.max))
            .layout(Layout::left_to_right(Align::Center)),
        |ui| {
            ui.label(fonts::section_header(&title));
            if let Some(count) = count {
                ui.label(
                    RichText::new(thousands(count))
                        .font(fonts::mono(tokens::mono::SMALL))
                        .color(tokens::TEXT_3),
                );
            }
        },
    );
}

/// The kind dot of a resource category.
fn kind_color(kind: ResKind) -> egui::Color32 {
    match kind {
        ResKind::Font | ResKind::XObject => tokens::KIND_CONTENT,
        ResKind::ColorSpace | ResKind::Pattern | ResKind::Shading => tokens::OK,
        ResKind::ExtGState | ResKind::Properties => tokens::KIND_ANNOT,
    }
}
