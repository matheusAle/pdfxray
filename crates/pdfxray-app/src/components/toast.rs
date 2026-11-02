//! A quiet confirmation after a copy: "✓ Copied MCID 9 · Do /Im1 · page 1", in the popover
//! style at the foot of the panel the copy came from, for [`TOAST_FOR`] seconds. Assistive tech
//! announces it: it is a polite live region.
//!
//! [`copied`] asks for one; [`show`] draws it, once per frame after the panels.

use egui::accesskit::Live;
use egui::{Align2, Area, Context, Id, Order, Rect, RichText, Role, Sense, Ui, WidgetInfo, vec2};

use super::Icon;
use crate::theme::{effects, fonts, tokens};

/// How long a toast stays, in seconds.
pub const TOAST_FOR: f64 = 1.6;
/// Longest piece of the copied text a toast repeats; the rest ends in an ellipsis.
const MAX_CHARS: usize = 48;
/// Space between the toast and the corner of the panel it sits in.
const INSET: f32 = 12.0;

#[derive(Debug, Clone)]
struct Toast {
    /// What was copied, as the toast shows it.
    what: String,
    /// When it goes, in the input's time.
    until: f64,
    /// The panel it sits at the foot of.
    around: Rect,
}

fn slot() -> Id {
    Id::unique("pdfxray-toast")
}

/// The id of the toast's node, the live region assistive tech announces.
fn node_id() -> Id {
    slot().with("node")
}

/// What the toast shows for `copied`: one line, cut after [`MAX_CHARS`] characters.
fn shown_text(copied: &str) -> String {
    let line = copied.split_whitespace().collect::<Vec<_>>().join(" ");
    if line.chars().count() <= MAX_CHARS {
        return line;
    }
    let mut cut: String = line.chars().take(MAX_CHARS).collect();
    cut.push('…');
    cut
}

/// Asks for the toast "Copied `copied`" at the foot of `around` for [`TOAST_FOR`] seconds.
pub fn copied(ctx: &Context, copied: &str, around: Rect) {
    let now = ctx.input(|i| i.time);
    let toast = Toast {
        what: shown_text(copied),
        until: now + TOAST_FOR,
        around,
    };
    ctx.data_mut(|d| d.insert_temp(slot(), toast));
    ctx.request_repaint();
}

/// Draws the toast while it lasts; call it once per frame, after the panels.
pub fn show(ctx: &Context) {
    let now = ctx.input(|i| i.time);
    let Some(toast) = ctx.data(|d| d.get_temp::<Toast>(slot())) else {
        return;
    };
    if now >= toast.until {
        ctx.data_mut(|d| d.remove::<Toast>(slot()));
        return;
    }
    ctx.request_repaint_after(std::time::Duration::from_secs_f64(toast.until - now));
    Area::new(slot().with("area"))
        .order(Order::Tooltip)
        .interactable(false)
        .pivot(Align2::LEFT_BOTTOM)
        .fixed_pos(toast.around.left_bottom() + vec2(INSET, -INSET))
        .show(ctx, |ui| {
            effects::frosted_panel()
                .inner_margin(egui::Margin::symmetric(10, 6))
                .corner_radius(tokens::RADIUS_MD)
                .show(ui, |ui| body(ui, &toast));
        });
}

fn body(ui: &mut Ui, toast: &Toast) {
    let inner = ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        Icon::Check.show(ui, 13.0, tokens::OK);
        ui.label(
            RichText::new("Copied")
                .font(fonts::ui(tokens::text::BODY))
                .color(tokens::TEXT),
        );
        ui.label(
            RichText::new(&toast.what)
                .font(fonts::mono(tokens::mono::BODY))
                .color(tokens::TEXT_2),
        );
    });
    // One node for the whole line, announced politely when it appears.
    let name = format!("Copied {}", toast.what);
    let node = ui.interact(inner.response.rect, node_id(), Sense::hover());
    node.widget_info(|| WidgetInfo::labeled(Role::Status, true, name.clone()));
    ui.ctx().accesskit_node_builder(node.id, |builder| {
        builder.set_live(Live::Polite);
    });
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn a_toast_repeats_a_short_value_whole_and_cuts_a_long_one() {
        assert_eq!(
            shown_text("MCID 9 · Do /Im1 · page 1"),
            "MCID 9 · Do /Im1 · page 1"
        );
        let long = format!("[{}]", vec!["278"; 40].join(" "));
        let cut = shown_text(&long);
        assert_eq!(cut.chars().count(), MAX_CHARS + 1);
        assert!(cut.starts_with("[278 278") && cut.ends_with('…'), "{cut}");
        // Smoke: line breaks become spaces, so the toast stays one line.
        assert_eq!(shown_text("a\nb"), "a b");
    }
}
