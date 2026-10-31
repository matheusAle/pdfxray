//! The inspector for a raw object: the path from the trailer, what it is, its diagnostics, what
//! it looks like and where it sits in the file.

use egui::{Frame, Margin, ScrollArea, Ui};
use pdfxray_core::NodeId;

use super::{Action, header, problem, relevance, summary};
use crate::components::{Icon, PropRow, context_menu, object_preview, section};
use crate::object_facts::{ObjectFacts, ObjectKind, human_bytes, thousands};
use crate::state::AppState;
use crate::theme::tokens;

/// Name of the diagnostics section.
pub const DIAGNOSTICS_SECTION: &str = "Diagnostics";

/// Name of the section with the picture of the object.
pub const PREVIEW_SECTION: &str = "Preview";

/// Name of the object section.
pub const OBJECT_SECTION: &str = "Object";

/// Name of the stream section.
pub const STREAM_SECTION: &str = "Stream";

/// The icon of an object by what it is.
fn icon_of(kind: ObjectKind) -> Icon {
    match kind {
        ObjectKind::ContentStream | ObjectKind::Form => Icon::Content,
        ObjectKind::Image => Icon::Image,
        ObjectKind::Font => Icon::Font,
        ObjectKind::Page => Icon::Pages,
        _ => Icon::Braces,
    }
}

/// Draws the whole inspector body for an object.
pub(super) fn ui(ui: &mut Ui, state: &mut AppState, facts: &ObjectFacts, action: &mut Action) {
    crumbs(ui, facts, action);
    let id = facts.object.map(NodeId::Object);
    let title = title(ui, facts);
    if let Some(id) = &id {
        context_menu::attach(&title.area, state, id);
    }
    summary::show(ui, &summary::of_object(facts));
    let problems = match &id {
        Some(id) => problem::of(state, id),
        None => Vec::new(),
    };
    let rail = state.nav.rail;
    ScrollArea::vertical()
        .id_salt("inspector-object")
        .auto_shrink(false)
        .show(ui, |ui| {
            action.details = problem::show(ui, &problems);
            let open = |title: &str| relevance::starts_open(rail, title);
            if !facts.diagnostics.is_empty() {
                section::show_with(
                    ui,
                    DIAGNOSTICS_SECTION,
                    DIAGNOSTICS_SECTION,
                    open(DIAGNOSTICS_SECTION),
                    |ui| {
                        Frame::new().inner_margin(Margin::symmetric(8, 4)).show(ui, |ui| {
                            for diagnostic in &facts.diagnostics {
                                if let Some(range) = super::diagnostic_card::show(ui, diagnostic) {
                                    state.show_bytes(range);
                                }
                                ui.add_space(6.0);
                            }
                        });
                    },
                );
            }
            if let Some(visual) = &facts.visual {
                section::show_with(
                    ui,
                    PREVIEW_SECTION,
                    PREVIEW_SECTION,
                    open(PREVIEW_SECTION),
                    |ui| {
                        Frame::new().inner_margin(Margin::symmetric(8, 4)).show(ui, |ui| {
                            object_preview::inspector(ui, state, visual);
                        });
                        for fact in object_preview::facts(visual) {
                            PropRow::new(fact.key, &fact.value)
                                .warn(fact.warn)
                                .value_size(tokens::mono::PREVIEW_VALUE)
                                .show(ui);
                        }
                    },
                );
            }
            section::show_with(ui, OBJECT_SECTION, OBJECT_SECTION, open(OBJECT_SECTION), |ui| {
                object_rows(ui, facts);
            });
            if let Some(stream) = &facts.stream {
                section::show_with(ui, STREAM_SECTION, STREAM_SECTION, open(STREAM_SECTION), |ui| {
                    let filter = if stream.filters.is_empty() {
                        "none".to_owned()
                    } else {
                        stream.filters.join(" + ")
                    };
                    PropRow::new("Filter", &filter).show(ui);
                    let encoded = format!(
                        "{} ({} bytes)",
                        human_bytes(stream.encoded_len),
                        thousands(stream.encoded_len)
                    );
                    PropRow::new("Encoded", &encoded)
                        .copy_text(&stream.encoded_len.to_string())
                        .show(ui);
                    let decoded = match stream.decoded_len {
                        Some(n) if stream.complete => format!("{} ({} bytes)", human_bytes(n), thousands(n)),
                        Some(_) => "partial".to_owned(),
                        None => "could not be decoded".to_owned(),
                    };
                    PropRow::new("Decoded", &decoded).warn(!stream.complete).show(ui);
                    let declared = stream.declared_length.map_or_else(
                        || "missing".to_owned(),
                        |n| thousands(usize::try_from(n).unwrap_or(0)),
                    );
                    let wrong = stream
                        .declared_length
                        .is_none_or(|n| usize::try_from(n).ok() != Some(stream.encoded_len));
                    PropRow::new("/Length", &declared).warn(wrong).show(ui);
                });
            }
        });
}

fn crumbs(ui: &mut Ui, facts: &ObjectFacts, action: &mut Action) {
    let trailer = header::CrumbItem {
        name: "trailer".to_owned(),
        color: tokens::TEXT_3,
        id: NodeId::Trailer,
    };
    let objects = facts.crumbs.iter().map(|r| header::CrumbItem {
        name: r.to_string(),
        color: tokens::TEXT_3,
        id: NodeId::Object(*r),
    });
    let items: Vec<header::CrumbItem> = std::iter::once(trailer).chain(objects).collect();
    header::crumb_line(ui, &items, action);
}

fn title(ui: &mut Ui, facts: &ObjectFacts) -> header::TitleOutput {
    let id = facts.object.map(NodeId::Object);
    let id_text = id.as_ref().map(ToString::to_string).unwrap_or_default();
    let line = match &facts.relation {
        Some(relation) => format!("{id_text} · {relation}"),
        None => id_text,
    };
    header::block(
        ui,
        |ui| {
            icon_of(facts.kind).show(ui, 16.0, tokens::ACCENT);
        },
        &facts.title,
        &line,
        None,
    )
}

fn object_rows(ui: &mut Ui, facts: &ObjectFacts) {
    let Some(r) = facts.object else {
        return;
    };
    PropRow::new("Number", &format!("{} {}", r.num, r.generation)).show(ui);
    PropRow::new("Type", &facts.type_line).show(ui);
    let (position, warn) = match (facts.position.offset, facts.position.in_object_stream) {
        (Some(offset), _) => (
            format!(
                "byte {} · index {}",
                thousands(offset),
                if facts.position.index_ok { "ok" } else { "wrong" }
            ),
            !facts.position.index_ok,
        ),
        (None, Some(stream)) => (format!("inside object stream {stream}"), false),
        (None, None) => ("not in the file".to_owned(), true),
    };
    PropRow::new("Position", &position).warn(warn).show(ui);
    let number = facts.defined_in + 1;
    let last = if facts.changed_later {
        format!("rev {number} (latest edit)")
    } else if facts.defined_in == 0 {
        "rev 1 (original)".to_owned()
    } else {
        format!("rev {number}")
    };
    PropRow::new("Last changed", &last)
        .warn(facts.changed_later)
        .show(ui);
}
