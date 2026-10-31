//! The crumbs and the title of the selected node. Right-clicking the title lists what can be done
//! with the node (see [`crate::actions`]).

use egui::{Align, Frame, Label, Layout, Margin, Response, RichText, Sense, Ui, Vec2, vec2};
use pdfxray_core::capture::{OpRecord, PaintKind};
use pdfxray_core::numeric::usize_from_u32;
use pdfxray_core::{Document, NodeId, Props};

use super::Action;
use crate::components::crumb::{MAX_TEXT_W, SEPARATOR_WIDTH, separator};
use crate::components::{Crumb, Icon, ToolButton, tooltip};
use crate::state::AppState;
use crate::state::path::crumb_label;
use crate::theme::{fonts, tokens};

/// Name of the button that keeps the inspector on its node.
pub const PIN_LABEL: &str = "Pin";

/// The icon of a node's kind.
pub fn icon_of(id: &NodeId) -> Icon {
    match id {
        NodeId::StructElem(_) => Icon::Tags,
        NodeId::MarkedContent { .. } | NodeId::Op { .. } => Icon::Content,
        NodeId::Glyph { .. } => Icon::Font,
        NodeId::Artifact { .. } => Icon::Artifacts,
        NodeId::Page(_) | NodeId::PageBox { .. } => Icon::Pages,
        NodeId::Object(_) | NodeId::Trailer | NodeId::Resource { .. } | NodeId::Group(_) => Icon::Objects,
        NodeId::Revision(_) => Icon::Revisions,
        NodeId::Document => Icon::File,
        NodeId::Diagnostic(_) => Icon::Warning,
        NodeId::Validation(_) => Icon::Validation,
    }
}

/// The title, the id line and the icon of a node. An operator that draws an image or a form is
/// named for it (`Image XObject /Im1`), a glyph for its character (`Glyph 'A'`).
fn heading(doc: &Document, id: &NodeId, props: Option<&Props>) -> (String, String, Icon) {
    let plain = || {
        let title = props.map_or_else(|| id.to_string(), |p| p.title.clone());
        let line = match props {
            Some(p) => format!("{id} · {}", p.kind),
            None => id.to_string(),
        };
        (title, line, icon_of(id))
    };
    match id {
        NodeId::Op { page, ix } => {
            let Some(op) = doc
                .captured(*page)
                .and_then(|c| c.ops.get(usize_from_u32(*ix)))
                .filter(|op| op.ix == *ix)
            else {
                return plain();
            };
            let painted_image = op.item_count > 0
                && doc.captured(*page).is_some_and(|c| {
                    op.first_item
                        .and_then(|f| c.items.get(usize_from_u32(f)))
                        .is_some_and(|item| matches!(item.kind, PaintKind::Image { .. }))
                });
            if op.operator == "Do" {
                let name = op.operands.first().cloned().unwrap_or_default();
                let (word, icon) = if painted_image {
                    ("Image", Icon::Image)
                } else {
                    ("Form", Icon::Content)
                };
                let object = state_xobject(doc, *page, op).map_or_else(String::new, |o| format!(" · {o}"));
                return (format!("{word} XObject {name}"), format!("{id}{object}"), icon);
            }
            let (title, _, icon) = plain();
            (title, format!("{id} · op #{ix}"), icon)
        }
        NodeId::Glyph { page, op, .. } => {
            let shown = doc
                .glyph_info(id)
                .and_then(|info| info.unicode)
                .filter(|u| !u.is_empty());
            let operator = doc
                .captured(*page)
                .and_then(|c| c.ops.get(usize_from_u32(*op)))
                .map_or("", |o| o.operator.as_str());
            (
                shown.map_or_else(|| "Glyph".to_owned(), |u| format!("Glyph '{u}'")),
                format!("{id} · in {operator} op #{op}"),
                Icon::Font,
            )
        }
        _ => plain(),
    }
}

/// The object a `Do` drew, when it is an indirect object.
fn state_xobject(doc: &Document, page: u32, op: &OpRecord) -> Option<pdfxray_core::ObjRef> {
    let captured = doc.captured(page)?;
    captured
        .xobjects
        .iter()
        .find(|x| x.op == op.ix)
        .and_then(|x| x.obj)
}

/// The short name of a node in the breadcrumb.
fn title_of(doc: &Document, id: &NodeId) -> String {
    crumb_label(doc, id)
}

/// One crumb of a breadcrumb line: its text, its kind colour and the node it selects.
pub(super) struct CrumbItem {
    pub(super) name: String,
    pub(super) color: egui::Color32,
    pub(super) id: NodeId,
}

/// The width the first crumb is cut at when the chain does not fit: it is the file name for most
/// nodes, and a long one must not push the selection out of the row. When the chain fits, the
/// name keeps whatever room the other crumbs leave.
const FIRST_MAX_TEXT_W: f32 = 120.0;
/// What a crumb adds around its name: margins, dot, gap and stroke (see `Crumb::width`).
const CRUMB_CHROME: f32 = 2.0 * 5.0 + 6.0 + 5.0 + 2.0;
/// The least of its name the selection keeps when the row is short.
const LAST_MIN_TEXT_W: f32 = 40.0;

/// A breadcrumb on one line. When it does not fit, the crumbs after the first fold into "···"
/// (its tooltip names them), oldest first, until the rest fits; the selection always shows, and
/// a name that is still too long is cut with an ellipsis.
pub(super) fn crumb_line(ui: &mut Ui, items: &[CrumbItem], action: &mut Action) {
    let Some(last) = items.len().checked_sub(1) else {
        return;
    };
    Frame::new()
        .inner_margin(Margin::symmetric(12, 8))
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing = Vec2::new(2.0, 4.0);
            let row = ui.available_width();
            let gap = SEPARATOR_WIDTH + 4.0;
            // `first_max` is the width the file name is cut at.
            let crumb = |i: usize, first_max: f32| {
                let item = &items[i];
                let crumb = Crumb::new(&item.name, item.color).current(i == last);
                if i == 0 { crumb.max_text(first_max) } else { crumb }
            };
            let alone = (row - CRUMB_CHROME).max(LAST_MIN_TEXT_W);
            let floor = if last > 0 { FIRST_MAX_TEXT_W } else { alone };
            let widths: Vec<f32> = (0..items.len()).map(|i| crumb(i, floor).width(ui)).collect();
            let ellipsis = Crumb::more().width(ui) + gap;
            // Crumbs 1..=hidden are folded; the first and the selection never are.
            let mut hidden = 0;
            loop {
                let shown: f32 = widths
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| *i == 0 || *i > hidden)
                    .map(|(i, w)| if i == 0 { *w } else { w + gap })
                    .sum();
                let width = shown + if hidden > 0 { ellipsis } else { 0.0 };
                if width <= row || hidden + 1 >= last {
                    break;
                }
                hidden += 1;
            }
            // With nothing folded, the name keeps the room the row has left.
            let first_max = if hidden == 0 {
                let used: f32 = widths
                    .iter()
                    .enumerate()
                    .map(|(i, w)| if i == 0 { *w } else { w + gap })
                    .sum();
                floor + (row - used - 2.0).max(0.0)
            } else {
                floor
            };
            // With crumbs folded the selection gets what the first one and the dots leave.
            let last_max = if hidden > 0 && last > 0 {
                (row - widths[0] - ellipsis - 2.0 * gap - CRUMB_CHROME).clamp(LAST_MIN_TEXT_W, MAX_TEXT_W)
            } else {
                MAX_TEXT_W
            };
            ui.horizontal(|ui| {
                for (i, item) in items.iter().enumerate() {
                    if (1..=hidden).contains(&i) {
                        if i == 1 {
                            separator(ui);
                            let names: Vec<&str> =
                                items[1..=hidden].iter().map(|c| c.name.as_str()).collect();
                            tooltip::text(Crumb::more().show(ui), names.join(" › "));
                        }
                        continue;
                    }
                    if i > 0 {
                        separator(ui);
                    }
                    let shown = if i == last && last > 0 {
                        crumb(i, first_max).max_text(last_max)
                    } else {
                        crumb(i, first_max)
                    };
                    if shown.show(ui).clicked() {
                        action.navigate = Some(item.id.clone());
                    }
                }
            });
        });
}

/// The path from the document to the selection, one crumb per ancestor; clicking one selects it.
pub(super) fn crumbs(ui: &mut Ui, state: &AppState, selection: &NodeId, action: &mut Action) {
    let Some(loaded) = &state.document else {
        return;
    };
    let doc = &loaded.doc;
    let items: Vec<CrumbItem> = state
        .path_to(selection)
        .into_iter()
        .map(|id| CrumbItem {
            name: title_of(doc, &id),
            color: tokens::kind_color(&id),
            id,
        })
        .collect();
    crumb_line(ui, &items, action);
}

/// What the title block reports.
pub(super) struct TitleOutput {
    /// The pin button was pressed.
    pub pin_pressed: bool,
    /// The title and the line under it: a right-click on it opens the node's menu.
    pub area: Response,
}

/// Most lines the title takes before it is cut with an ellipsis.
const TITLE_ROWS: usize = 2;
/// Width kept for the pin button at the right end of the title.
const PIN_W: f32 = 34.0;

/// The icon well, the title in 17 px (it wraps onto a second line, then is cut with an ellipsis and
/// the whole name goes to its tooltip), the line
/// under it and, when `pin` is `Some`, the pin button (pressed when `pin` is `Some(true)`).
pub(super) fn block(
    ui: &mut Ui,
    icon: impl FnOnce(&mut Ui),
    name: &str,
    line: &str,
    pin: Option<bool>,
) -> TitleOutput {
    let (pin_pressed, area) = Frame::new()
        .inner_margin(Margin::symmetric(12, 6))
        .show(ui, |ui| {
            ui.horizontal_top(|ui| {
                let mut pin_pressed = false;
                Frame::new()
                    .fill(tokens::SURFACE_3)
                    .corner_radius(tokens::RADIUS_MD)
                    .inner_margin(Margin::same(8))
                    .show(ui, icon);
                let reserved = if pin.is_some() { PIN_W } else { 0.0 };
                let width = (ui.available_width() - reserved - ui.spacing().item_spacing.x).max(60.0);
                // The title, and under it the id line (the row around them runs across).
                let text = ui.allocate_ui_with_layout(vec2(width, 0.0), Layout::top_down(Align::Min), |ui| {
                    ui.spacing_mut().item_spacing.y = 1.0;
                    // A long name is cut after two lines (the Name row has all of it).
                    let mut title = fonts::inspector_title(name);
                    title.wrap.max_rows = TITLE_ROWS;
                    title.wrap.max_width = width;
                    let cut = ui.painter().layout_job(title.clone()).elided;
                    let shown = ui.add(Label::new(title).wrap().selectable(false));
                    if cut {
                        tooltip::text(shown, name);
                    }
                    ui.add(
                        Label::new(
                            RichText::new(line)
                                .font(fonts::mono(tokens::mono::SMALL))
                                .color(tokens::TEXT_3),
                        )
                        .wrap()
                        .selectable(false),
                    );
                });
                // Over the text only, so the pin keeps its own clicks.
                let area = ui.interact(text.response.rect, ui.scope_id().with("title-menu"), Sense::CLICK);
                if let Some(pinned) = pin {
                    ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
                        let button = ToolButton::new(PIN_LABEL)
                            .icon(Icon::Pin)
                            .icon_only(true)
                            .active(pinned)
                            .show(ui);
                        pin_pressed = button.clicked();
                        tooltip::text(
                            button,
                            "Keep the inspector on this node while the selection moves",
                        );
                    });
                }
                (pin_pressed, area)
            })
            .inner
        })
        .inner;
    TitleOutput { pin_pressed, area }
}

/// The icon, the title, the id line and the pin button of the selection. `named` replaces the
/// title and the line under it.
pub(super) fn title(
    ui: &mut Ui,
    state: &AppState,
    id: &NodeId,
    props: Option<&Props>,
    named: Option<(String, String)>,
) -> TitleOutput {
    let (name, line, icon) = state.document.as_ref().map_or_else(
        || (id.to_string(), id.to_string(), icon_of(id)),
        |loaded| heading(&loaded.doc, id, props),
    );
    let (name, line) = named.unwrap_or((name, line));
    let pinned = state.selection.pinned.is_some();
    block(
        ui,
        |ui| {
            icon.show(ui, 16.0, tokens::kind_color(id));
        },
        &name,
        &line,
        Some(pinned),
    )
}
