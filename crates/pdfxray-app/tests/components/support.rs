//! Helpers shared by the `components` tests.

use super::*;

pub(crate) fn builder<S>(size: egui::Vec2) -> HarnessBuilder<S> {
    // Animations off, so nothing here can be caught in the middle of one (see tests/motion.rs
    // for the motion itself).
    pdfxray_app::theme::motion::set_reduce_motion(true);
    Harness::builder()
        .with_size(size)
        .with_context_setup(pdfxray_app::theme::install)
}

/// A panel-coloured canvas with `content` on it.
pub(crate) fn on_surface(ui: &mut egui::Ui, content: impl FnOnce(&mut egui::Ui)) {
    ui.painter().rect_filled(ui.max_rect(), 0.0, tokens::BG);
    egui::Frame::new()
        .inner_margin(egui::Margin::same(12))
        .show(ui, content);
}

/// Every shape of the last frame, groups flattened.
pub(crate) fn shapes_of<S>(harness: &Harness<'_, S>) -> Vec<egui::Shape> {
    pub(crate) fn flatten(shape: &egui::Shape, out: &mut Vec<egui::Shape>) {
        match shape {
            egui::Shape::Vec(shapes) => shapes.iter().for_each(|shape| flatten(shape, out)),
            other => out.push(other.clone()),
        }
    }
    let mut out = Vec::new();
    for clipped in &harness.output().shapes {
        flatten(&clipped.shape, &mut out);
    }
    out
}

/// The filled rectangles of the frame that cover `rect` exactly.
pub(crate) fn rect_shapes_at(shapes: &[egui::Shape], rect: egui::Rect) -> Vec<egui::epaint::RectShape> {
    shapes
        .iter()
        .filter_map(|shape| match shape {
            egui::Shape::Rect(r)
                if (r.rect.min - rect.min).length() < 0.6 && (r.rect.max - rect.max).length() < 0.6 =>
            {
                Some(r.clone())
            }
            _ => None,
        })
        .collect()
}

/// The colour a text shape with exactly `text` is painted in, when it has an override.
pub(crate) fn text_ink(shapes: &[egui::Shape], text: &str) -> Option<egui::Color32> {
    shapes.iter().find_map(|shape| match shape {
        egui::Shape::Text(t) if t.galley.text() == text => t.override_text_color,
        _ => None,
    })
}

pub(crate) fn is_on(node: &egui_kittest::Node<'_>) -> bool {
    node.accesskit_node().toggled() == Some(egui::accesskit::Toggled::True)
}
