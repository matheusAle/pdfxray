//! An encrypted file: what pdfxray can and cannot show, and the encryption settings in plain
//! words. Nothing is decrypted.

use egui::{Align, Frame, Layout, Margin, RichText, Sense, Stroke, Ui, vec2};
use pdfxray_core::EncryptionInfo;

use super::card;
use crate::components::{Icon, Tone, ToolButton};
use crate::state::{AppState, RailItem};
use crate::theme::{fonts, tokens};

/// Headline of the card.
pub const TITLE: &str = "This file is encrypted";

/// Under the headline.
pub const SUBTITLE: &str = "Its file structure is readable; page content is not.";

/// The paragraph of the card.
pub const EXPLANATION: &str = "Encrypted PDFs scramble their streams and strings. pdfxray can still show the \
    objects, page boxes and encryption settings, but it can't draw the pages or read tags and text in \
    this version.";

/// What can and cannot be done with an encrypted file, in the order the card lists it.
pub const CAPABILITIES: [(&str, bool); 4] = [
    ("Objects & xref", true),
    ("Page boxes", true),
    ("Rendering", false),
    ("Tags & text", false),
];

/// Name of the button that goes to the Objects entry.
pub const BROWSE_OBJECTS: &str = "Browse objects";

/// Name of the button that goes to the Pages entry.
pub const PAGE_BOXES: &str = "Page boxes";

/// One line of the encryption table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    /// What it is.
    pub key: &'static str,
    /// What it says.
    pub value: String,
    /// [`Tone::Ok`] marks good news.
    pub tone: Tone,
}

/// The line above the table: which object holds the settings.
pub fn heading(info: &EncryptionInfo) -> String {
    info.obj
        .map_or_else(|| "/Encrypt  inline".to_owned(), |obj| format!("/Encrypt  {obj}"))
}

/// The encryption table: filter, version, password, permissions and metadata.
pub fn rows(info: &EncryptionInfo) -> Vec<Row> {
    let plain = |key, value: String| Row {
        key,
        value,
        tone: Tone::Neutral,
    };
    let filter = if info.filter == "Standard" {
        "/Standard (password based)".to_owned()
    } else {
        format!("/{}", info.filter)
    };
    let number = |n: Option<u32>| n.map_or_else(|| "?".to_owned(), |n| n.to_string());
    let cipher = match info.algorithm.split_once(' ') {
        Some((name, bits)) if bits.ends_with("-bit") => format!("{name}, {bits}"),
        _ => info.algorithm.clone(),
    };
    let (password, tone) = match info.user_password_empty {
        Some(true) => ("empty: opens without asking", Tone::Ok),
        Some(false) => ("required", Tone::Neutral),
        None => ("unknown", Tone::Neutral),
    };
    let mark = |allowed: bool| if allowed { '✓' } else { '×' };
    let permissions = &info.permissions;
    vec![
        plain("Filter", filter),
        plain(
            "Version",
            format!("V {} · R {} · {cipher}", number(info.v), number(info.r)),
        ),
        Row {
            key: "User password",
            value: password.to_owned(),
            tone,
        },
        plain(
            "Permissions",
            format!(
                "print {} · copy {} · edit {}",
                mark(permissions.print),
                mark(permissions.copy),
                mark(permissions.modify)
            ),
        ),
        plain(
            "Metadata",
            if info.metadata_encrypted {
                "encrypted"
            } else {
                "clear"
            }
            .to_owned(),
        ),
    ]
}

/// The cipher in a few characters: `AES-128`, `RC4-40`; `None` when it has no short name.
pub fn cipher(info: &EncryptionInfo) -> Option<String> {
    let family = match info.algorithm.split(' ').next() {
        Some("AESV2" | "AESV3") => "AES",
        Some("RC4") => "RC4",
        _ => return None,
    };
    Some(format!("{family}-{}", info.length_bits))
}

const WIDTH: f32 = 600.0;
/// Height of the card, to centre it before it is laid out.
const HEIGHT: f32 = 453.0;
const ROW_H: f32 = 25.0;

/// Draws the card of the open document.
pub fn ui(ui: &mut Ui, state: &mut AppState) {
    let Some(info) = state
        .document
        .as_ref()
        .and_then(|loaded| loaded.encryption.clone())
    else {
        return;
    };
    let mut go = None;
    ui.with_layout(Layout::top_down(Align::Center), |ui| {
        ui.add_space(((ui.available_height() - HEIGHT) / 2.0).max(24.0));
        card(tokens::SURFACE, Margin::same(28)).show(ui, |ui| {
            ui.set_width(WIDTH - 56.0);
            ui.with_layout(Layout::top_down(Align::Min), |ui| {
                header(ui);
                ui.add_space(16.0);
                ui.label(
                    RichText::new(EXPLANATION)
                        .font(fonts::ui(tokens::text::LARGE))
                        .line_height(Some(20.0))
                        .color(tokens::TEXT_2),
                );
                ui.add_space(16.0);
                table(ui, &info);
                ui.add_space(14.0);
                capabilities(ui);
                ui.add_space(16.0);
                go = buttons(ui);
            });
        });
    });
    if let Some(rail) = go {
        state.set_rail(rail);
    }
}

fn header(ui: &mut Ui) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 14.0;
        let (well, _) = ui.allocate_exact_size(vec2(44.0, 44.0), Sense::hover());
        ui.painter()
            .rect_filled(well, tokens::RADIUS_LG, tokens::WARN.gamma_multiply(0.14));
        Icon::Lock.paint(ui, well, 22.0, tokens::WARN);
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 2.0;
            ui.add_space(4.0);
            ui.label(
                RichText::new(TITLE)
                    .font(fonts::semibold(tokens::text::TITLE + 2.0))
                    .color(tokens::TEXT),
            );
            ui.label(
                RichText::new(SUBTITLE)
                    .font(fonts::ui(tokens::text::BODY))
                    .color(tokens::TEXT_3),
            );
        });
    });
}

fn table(ui: &mut Ui, info: &EncryptionInfo) {
    Frame::new()
        .fill(tokens::BG)
        .stroke(Stroke::new(1.0, tokens::BORDER))
        .corner_radius(tokens::RADIUS_LG)
        .inner_margin(Margin::symmetric(12, 8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 0.0;
            ui.label(
                RichText::new(heading(info))
                    .font(fonts::mono(tokens::mono::SMALL))
                    .color(tokens::TEXT_3),
            );
            ui.add_space(2.0);
            for row in rows(info) {
                let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), ROW_H), Sense::hover());
                let key = egui::Rect::from_min_size(rect.min, vec2(130.0, ROW_H));
                let ink = if row.tone == Tone::Ok {
                    tokens::OK
                } else {
                    tokens::TEXT
                };
                let (key_text, value_text) = (key_text(row.key), value_text(&row.value, ink));
                // Both columns are centred in the row, and the two fonts differ in height: the
                // value moves to sit on the key's baseline.
                let rise = baseline_rise(ui, key, &key_text, &value_text);
                // Narrowing the rect from one side moves its middle by half as much, and keeps the
                // value inside the row.
                let value = egui::Rect::from_min_max(
                    rect.min + vec2(130.0, 2.0 * rise.max(0.0)),
                    rect.max + vec2(0.0, 2.0 * rise.min(0.0)),
                );
                ui.scope_builder(
                    egui::UiBuilder::new()
                        .max_rect(key)
                        .layout(Layout::left_to_right(Align::Center)),
                    |ui| {
                        ui.label(key_text);
                    },
                );
                ui.scope_builder(
                    egui::UiBuilder::new()
                        .max_rect(value)
                        .layout(Layout::left_to_right(Align::Center)),
                    |ui| {
                        ui.label(value_text);
                    },
                );
            }
        });
}

/// The label of a row of the table: dim, in the UI font.
fn key_text(key: &str) -> RichText {
    RichText::new(key)
        .font(fonts::ui(tokens::text::BODY))
        .color(tokens::TEXT_3)
}

/// The value of a row of the table: mono, in `ink`.
fn value_text(value: &str, ink: egui::Color32) -> RichText {
    RichText::new(value)
        .font(fonts::mono(tokens::mono::BODY))
        .color(ink)
}

/// How far down the value of a row moves to put its baseline on the key's, both being labels
/// centred in a row `key` high at the same top. They are laid out the way a label does, in a
/// probe of the row, so the glyph offsets are the ones that get drawn.
fn baseline_rise(ui: &mut Ui, row: egui::Rect, key: &RichText, value: &RichText) -> f32 {
    let probe = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(row)
            .layout(Layout::left_to_right(Align::Center)),
    );
    let baseline = |text: &RichText| {
        let galley = egui::WidgetText::from(text.clone()).into_galley(
            &probe,
            Some(egui::TextWrapMode::Extend),
            f32::INFINITY,
            egui::TextStyle::Body,
        );
        let first = galley.rows.first();
        let to_baseline = first
            .and_then(|row| row.glyphs.first().map(|glyph| row.pos.y + glyph.pos.y))
            .unwrap_or(galley.size().y);
        // Centred: the line's top sits half its height above the middle of the row.
        to_baseline - galley.size().y / 2.0
    };
    baseline(key) - baseline(value)
}

fn capabilities(ui: &mut Ui) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        for (name, can) in CAPABILITIES {
            let (icon, ink, text) = if can {
                (Icon::Check, tokens::OK, tokens::TEXT_2)
            } else {
                (Icon::Close, tokens::TEXT_3, tokens::TEXT_3)
            };
            icon.show(ui, 13.0, ink);
            ui.label(
                RichText::new(name)
                    .font(fonts::ui(tokens::text::SMALL))
                    .color(text),
            );
            ui.add_space(12.0);
        }
    });
}

/// The two buttons; the rail entry the user chose.
fn buttons(ui: &mut Ui) -> Option<RailItem> {
    let mut go = None;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        if ToolButton::new(BROWSE_OBJECTS)
            .icon(Icon::Braces)
            .primary()
            .show(ui)
            .clicked()
        {
            go = Some(RailItem::Objects);
        }
        if ToolButton::new(PAGE_BOXES)
            .icon(Icon::Boxes)
            .secondary()
            .show(ui)
            .clicked()
        {
            go = Some(RailItem::Pages);
        }
    });
    go
}

#[cfg(test)]
mod tests {
    use pdfxray_core::{ObjRef, Permissions};
    use pretty_assertions::assert_eq;

    use super::*;

    /// A V 4 / R 4 file with AES-128 that opens with the empty password, the one of the design.
    fn aes_128() -> EncryptionInfo {
        EncryptionInfo {
            obj: Some(ObjRef::new(9, 0)),
            filter: "Standard".to_owned(),
            sub_filter: None,
            v: Some(4),
            r: Some(4),
            length_bits: 128,
            algorithm: "AESV2 128-bit".to_owned(),
            permissions: Permissions {
                print: true,
                modify: false,
                copy: false,
                annotate: false,
                fill_forms: false,
                extract_accessibility: true,
                assemble: false,
                print_high_quality: true,
            },
            p: Some(-3904),
            metadata_encrypted: true,
            user_password_empty: Some(true),
        }
    }

    fn table(info: &EncryptionInfo) -> Vec<(&'static str, String, Tone)> {
        rows(info)
            .into_iter()
            .map(|row| (row.key, row.value, row.tone))
            .collect()
    }

    #[test]
    fn the_table_says_the_settings_in_words() {
        let info = aes_128();

        assert_eq!(heading(&info), "/Encrypt  9 0 R");
        assert_eq!(
            table(&info),
            [
                ("Filter", "/Standard (password based)".to_owned(), Tone::Neutral),
                ("Version", "V 4 · R 4 · AESV2, 128-bit".to_owned(), Tone::Neutral),
                (
                    "User password",
                    "empty: opens without asking".to_owned(),
                    Tone::Ok
                ),
                (
                    "Permissions",
                    "print ✓ · copy × · edit ×".to_owned(),
                    Tone::Neutral
                ),
                ("Metadata", "encrypted".to_owned(), Tone::Neutral),
            ]
        );
        assert_eq!(cipher(&info).as_deref(), Some("AES-128"));
    }

    #[test]
    fn what_is_not_known_is_said_so() {
        let mut info = aes_128();
        info.obj = None;
        info.filter = "Adobe.PubSec".to_owned();
        info.v = None;
        info.r = None;
        info.algorithm = "unknown".to_owned();
        info.user_password_empty = None;
        info.metadata_encrypted = false;
        info.permissions.copy = true;

        let table = table(&info);

        assert_eq!(heading(&info), "/Encrypt  inline");
        assert_eq!(table[0].1, "/Adobe.PubSec");
        assert_eq!(table[1].1, "V ? · R ? · unknown");
        assert_eq!((table[2].1.as_str(), table[2].2), ("unknown", Tone::Neutral));
        assert_eq!(table[3].1, "print ✓ · copy ✓ · edit ×");
        assert_eq!(table[4].1, "clear");
        assert_eq!(cipher(&info), None);
        info.user_password_empty = Some(false);
        assert_eq!(rows(&info)[2].value, "required");
    }

    #[test]
    fn the_cipher_has_a_short_name() {
        let mut info = aes_128();
        info.algorithm = "RC4 40-bit".to_owned();
        info.length_bits = 40;
        assert_eq!(cipher(&info).as_deref(), Some("RC4-40"));
        info.algorithm = "AESV3 256-bit".to_owned();
        info.length_bits = 256;
        assert_eq!(cipher(&info).as_deref(), Some("AES-256"));
        info.algorithm = "None".to_owned();
        assert_eq!(cipher(&info), None);
    }
}
