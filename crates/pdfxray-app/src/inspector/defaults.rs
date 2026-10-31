//! The "3 default values (Blend Normal, Alpha 1, no `SMask`)" line.
//!
//! A value that is what the PDF would have been without anyone setting it says nothing, so the
//! sections that list state fold those values into one dim line. Clicking it lists them as rows.

use egui::{Id, Label, Rect, Role, Sense, Ui, WidgetInfo, vec2};

use crate::components::{Icon, PropRow, tooltip};
use crate::theme::{fonts, tokens};

/// A value that has its default.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DefaultValue {
    /// The key, as the row would show it when expanded.
    pub key: &'static str,
    /// The value, as the row would show it when expanded.
    pub value: String,
    /// How the line names it: `Blend Normal`, `no SMask`.
    pub short: String,
}

impl DefaultValue {
    /// A default `value` of `key` that the line names `short`.
    pub fn new(key: &'static str, value: impl Into<String>, short: impl Into<String>) -> Self {
        Self {
            key,
            value: value.into(),
            short: short.into(),
        }
    }
}

/// `3 default values`: the accessible name of the line.
pub fn name(count: usize) -> String {
    format!("{count} default value{}", if count == 1 { "" } else { "s" })
}

/// `3 default values (Blend Normal, Alpha 1, no SMask)`. Whether the line is open is shown by
/// its chevron alone.
pub fn line(items: &[DefaultValue]) -> String {
    let shorts: Vec<&str> = items.iter().map(|item| item.short.as_str()).collect();
    format!("{} ({})", name(items.len()), shorts.join(", "))
}

fn open_id(key: &str) -> Id {
    Id::unique(("pdfxray-defaults", key))
}

/// Draws the line for `items` (nothing when there are none) and, once clicked, the rows. `key`
/// names the section the line belongs to, for the remembered state.
pub fn show(ui: &mut Ui, key: &str, items: &[DefaultValue]) {
    if items.is_empty() {
        return;
    }
    let id = open_id(key);
    let open = ui
        .data_mut(|data| data.get_persisted::<bool>(id))
        .unwrap_or(false);
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), tokens::ROW_H), Sense::click());
    let label = name(items.len());
    response.widget_info(|| WidgetInfo::selected(Role::Button, true, open, &label));
    let hovered = response.hovered();
    if hovered {
        ui.painter().rect_filled(rect, 0.0, tokens::NEUTRAL_HOVER);
    }
    let ink = if hovered { tokens::TEXT_2 } else { tokens::TEXT_3 };
    let icon = Rect::from_min_size(rect.min + vec2(8.0, 0.0), vec2(14.0, rect.height()));
    let chevron = if open {
        Icon::ChevronDown
    } else {
        Icon::ChevronRight
    };
    chevron.paint(ui, icon, 12.0, ink);
    let text = Rect::from_min_max(rect.min + vec2(26.0, 0.0), rect.max - vec2(8.0, 0.0));
    ui.scope_builder(
        egui::UiBuilder::new()
            .max_rect(text)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
        |ui| {
            ui.add(
                Label::new(
                    egui::RichText::new(line(items))
                        .font(fonts::ui(tokens::text::SMALL))
                        .color(ink),
                )
                .selectable(false)
                .truncate(),
            );
        },
    );
    if response.clicked() {
        ui.data_mut(|data| data.insert_persisted(id, !open));
    }
    // The line is cut to the row; the tooltip lists every default, one to a line.
    let shorts: Vec<&str> = items.iter().map(|item| item.short.as_str()).collect();
    tooltip::text(
        response.on_hover_cursor(egui::CursorIcon::PointingHand),
        shorts.join("\n"),
    );
    if open {
        for item in items {
            PropRow::new(item.key, &item.value).show(ui);
        }
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    fn graphics() -> Vec<DefaultValue> {
        vec![
            DefaultValue::new("Blend", "Normal", "Blend Normal"),
            DefaultValue::new("Alpha", "ca 1.0 · CA 1.0", "Alpha 1"),
            DefaultValue::new("SMask", "None", "no SMask"),
        ]
    }

    #[test]
    fn the_line_counts_and_names_the_defaults() {
        assert_eq!(
            line(&graphics()),
            "3 default values (Blend Normal, Alpha 1, no SMask)"
        );
        assert_eq!(line(&graphics()[..1]), "1 default value (Blend Normal)");
        // Smoke: the accessible name carries the count alone.
        assert_eq!(name(3), "3 default values");
    }
}
