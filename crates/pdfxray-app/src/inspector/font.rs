//! The FONT section: which font the node was set in and how it is embedded.

use egui::Ui;

use super::Action;
use super::structure::Cx;
use crate::components::PropRow;
use pdfxray_core::{FontFacts, NodeId};

/// Title of the section.
pub const TITLE: &str = "Font";

/// `/F1  →  41 0 R`.
pub fn resource_text(font: &FontFacts) -> String {
    match (&font.resource, font.obj) {
        (Some(name), Some(obj)) => format!("/{name}  →  {obj}"),
        (Some(name), None) => format!("/{name}"),
        (None, Some(obj)) => obj.to_string(),
        (None, None) => "(none)".to_owned(),
    }
}

/// `subset · FontFile3 44 0 R`, `full · FontFile2`, or `not embedded`.
pub fn embedded_text(font: &FontFacts) -> String {
    if !font.embedded {
        return "not embedded".to_owned();
    }
    let kind = if font.subset_prefix.is_some() {
        "subset"
    } else {
        "full"
    };
    let program = match (&font.font_file, font.font_file_obj) {
        (Some(key), Some(obj)) => format!(" · {key} {obj}"),
        (Some(key), None) => format!(" · {key}"),
        _ => String::new(),
    };
    format!("{kind}{program}")
}

/// `/Type0 · CIDFontType0`.
pub fn subtype_text(font: &FontFacts) -> String {
    let base = font
        .subtype
        .as_deref()
        .map_or_else(String::new, |s| format!("/{s}"));
    match &font.cid_subtype {
        Some(cid) => format!("{base} · {cid}"),
        None => base,
    }
}

pub(super) fn show(ui: &mut Ui, cx: &Cx<'_>, action: &mut Action) {
    let Some(font) = &cx.font else {
        return;
    };
    cx.section(ui, TITLE, |ui| {
        let row = PropRow::new("Resource", &resource_text(font))
            .link(font.obj.is_some())
            .show(ui);
        if row.value.clicked() {
            action.navigate = font.obj.map(NodeId::Object);
        }
        if let Some(base) = &font.base_font {
            PropRow::new("BaseFont", base).show(ui);
        }
        PropRow::new("Subtype", &subtype_text(font)).show(ui);
        let embedded = embedded_text(font);
        let row = PropRow::new("Embedded", &embedded)
            .link(font.font_file_obj.is_some())
            .warn(!font.embedded)
            .show(ui);
        if row.value.clicked() {
            action.navigate = font.font_file_obj.map(NodeId::Object);
        }
        if let Some(encoding) = &font.encoding {
            PropRow::new("Encoding", encoding).show(ui);
        }
    });
}
