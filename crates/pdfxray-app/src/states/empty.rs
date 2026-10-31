//! No file open: a drop zone with the logo, "Try a sample" and the recent files.

use std::path::Path;

use egui::{
    Align, CursorIcon, Label, Layout, Margin, Rect, RichText, Role, Sense, Ui, UiBuilder, WidgetInfo, pos2,
    vec2,
};

use super::card;
use crate::brand;
use crate::components::{Chip, Icon, Tone, ToolButton, focus, tooltip};
use crate::recents::{Recent, folder_text, relative_time};
use crate::state::{AppState, describe};
use crate::theme::{fonts, tokens};

/// Size of the logo at the top of the drop zone.
const LOGO: f32 = 72.0;

/// Headline of the drop zone.
pub const DROP_TITLE: &str = "Drop a PDF to inspect it";

/// The two lines under the headline.
pub const DROP_SUBTITLE: &str = "See its tags, layout boxes, drawing instructions and raw objects.\npdfxray only reads the file; it never changes it.";

/// Name of the drop zone's open button.
pub const OPEN_FILE_LABEL: &str = "Open file…";

/// Name of the button that opens the built-in sample.
pub const SAMPLE_LABEL: &str = "Try a sample";

/// Heading of the recent files.
pub const RECENT_HEADING: &str = "RECENT";

/// Shown in place of the time of a recent file that is no longer there.
pub const NOT_FOUND: &str = "not found";

/// Starts the line under the recent files when the list could not be saved.
pub const RECENTS_NOT_SAVED: &str = "Recent files are not saved";

/// Starts the line under the recent files when the saved list could not be read.
pub const RECENTS_NOT_READ: &str = "Recent files could not be read";

/// Accessible name of the row that opens a recent file.
pub fn recent_label(name: &str) -> String {
    format!("Open {name}")
}

const WIDTH: f32 = 560.0;
const ROW_H: f32 = 46.0;
const TIME_W: f32 = 84.0;
/// Room for the Tagged or Untagged chip.
const CHIP_W: f32 = 64.0;

/// Draws the drop zone and the recent files, and why the last file could not be opened, if it
/// could not.
pub fn ui(ui: &mut Ui, state: &mut AppState) {
    ui.with_layout(Layout::top_down(Align::Center), |ui| {
        ui.add_space((ui.available_height() * 0.2).max(24.0));
        drop_zone(ui, state);
        if let Some(error) = &state.open_error {
            ui.add_space(14.0);
            ui.label(RichText::new(error).color(tokens::ERR));
        }
        if !state.recents.entries().is_empty() {
            ui.add_space(30.0);
            recent_files(ui, state);
        }
        let recents_problem = match (&state.recents_error, state.recents.load_error()) {
            (Some(error), _) => Some(format!("{RECENTS_NOT_SAVED}: {}", describe(error))),
            (None, Some(error)) => Some(format!("{RECENTS_NOT_READ}: {}", describe(error))),
            (None, None) => None,
        };
        if let Some(problem) = recents_problem {
            ui.add_space(10.0);
            ui.label(
                RichText::new(problem)
                    .font(fonts::mono(tokens::mono::SMALL))
                    .color(tokens::TEXT_3),
            );
        }
    });
}

fn drop_zone(ui: &mut Ui, state: &mut AppState) {
    card(
        tokens::SURFACE,
        Margin {
            left: 40,
            right: 40,
            top: 48,
            bottom: 40,
        },
    )
    .show(ui, |ui| {
        ui.set_width(WIDTH - 80.0);
        ui.vertical_centered(|ui| {
            brand::logo(ui, LOGO);
            ui.add_space(14.0);
            ui.label(
                RichText::new(DROP_TITLE)
                    .font(fonts::semibold(tokens::text::TITLE + 2.0))
                    .color(tokens::TEXT),
            );
            ui.add_space(8.0);
            ui.label(
                RichText::new(DROP_SUBTITLE)
                    .font(fonts::ui(tokens::text::LARGE))
                    .line_height(Some(20.0))
                    .color(tokens::TEXT_3),
            );
            ui.add_space(20.0);
            let sample = ToolButton::new(SAMPLE_LABEL).icon(Icon::Flask).secondary();
            let open = ToolButton::new(OPEN_FILE_LABEL)
                .icon(Icon::Open)
                .primary()
                .hint("⌘O");
            // The two buttons side by side, centred as a pair.
            let pair = open.width(ui) + sample.width(ui) + 8.0;
            ui.horizontal(|ui| {
                ui.add_space(((ui.available_width() - pair) / 2.0).max(0.0));
                ui.spacing_mut().item_spacing.x = 8.0;
                let open = open.show(ui);
                if open.clicked() {
                    state.request_open();
                }
                tooltip::with_shortcut(open, "Choose a PDF", "⌘O");
                let sample = sample.show(ui);
                if sample.clicked() {
                    state.open_sample();
                }
                tooltip::text(sample, "Open a small tagged PDF that comes with pdfxray");
            });
        });
    });
}

fn recent_files(ui: &mut Ui, state: &mut AppState) {
    let now = (state.clock)();
    let home = state.recents.home().map(Path::to_path_buf);
    let mut chosen = None;
    ui.allocate_ui_with_layout(vec2(WIDTH, 0.0), Layout::top_down(Align::Min), |ui| {
        ui.label(
            RichText::new(RECENT_HEADING)
                .font(fonts::mono(tokens::mono::SMALL))
                .color(tokens::TEXT_3),
        );
        ui.add_space(6.0);
        ui.spacing_mut().item_spacing.y = 0.0;
        for (n, recent) in state.recents.entries().iter().enumerate() {
            if row(ui, n, recent, now, home.as_deref()) {
                chosen = Some(recent.path.clone());
            }
        }
    });
    if let Some(path) = chosen {
        state.open_path(&path);
    }
}

/// One recent file; true when it was clicked and can be opened.
fn row(ui: &mut Ui, n: usize, recent: &Recent, now: u64, home: Option<&Path>) -> bool {
    let (rect, _) = ui.allocate_exact_size(vec2(WIDTH, ROW_H), Sense::hover());
    let response = ui.interact(rect, ui.make_persistent_id(("recent", n)), Sense::click());
    let open = recent_label(&recent.name);
    response.widget_info(|| WidgetInfo::labeled(Role::Button, !recent.missing, open.clone()));
    let live = !recent.missing;
    if response.hovered() && live {
        ui.painter()
            .rect_filled(rect, tokens::RADIUS_MD, tokens::SURFACE_2);
    }
    if live {
        focus::ring(ui, &response, rect, tokens::RADIUS_MD);
    }
    let ink = if live { tokens::TEXT } else { tokens::TEXT_3 };
    Icon::File.paint(
        ui,
        Rect::from_min_size(rect.min + vec2(8.0, 0.0), vec2(24.0, rect.height())),
        16.0,
        tokens::TEXT_3,
    );

    let right = Rect::from_min_max(
        pos2(rect.max.x - 12.0 - TIME_W, rect.min.y),
        pos2(rect.max.x - 12.0, rect.max.y),
    );
    let beside_time = Rect::from_min_max(
        pos2(right.min.x - 16.0 - CHIP_W, rect.min.y),
        pos2(right.min.x - 16.0, rect.max.y),
    );
    let text = Rect::from_min_max(
        pos2(rect.min.x + 40.0, rect.min.y),
        pos2(beside_time.min.x - 8.0, rect.max.y),
    );
    ui.scope_builder(
        UiBuilder::new()
            .max_rect(text)
            .layout(Layout::top_down(Align::Min)),
        |ui| {
            ui.add_space(7.0);
            ui.spacing_mut().item_spacing.y = 1.0;
            ui.add(
                Label::new(
                    RichText::new(&recent.name)
                        .font(fonts::ui(tokens::text::LARGE))
                        .color(ink),
                )
                .truncate(),
            );
            let pages = if recent.pages == 1 { "page" } else { "pages" };
            ui.add(
                Label::new(
                    RichText::new(format!(
                        "{} · {} {pages}",
                        folder_text(&recent.path, home),
                        recent.pages
                    ))
                    .font(fonts::mono(tokens::mono::SMALL))
                    .color(tokens::TEXT_3),
                )
                .truncate(),
            );
        },
    );
    if live {
        ui.scope_builder(
            UiBuilder::new()
                .max_rect(beside_time)
                .layout(Layout::right_to_left(Align::Center)),
            |ui| {
                let chip = if recent.tagged {
                    Chip::new("Tagged").tone(Tone::Ok)
                } else {
                    Chip::new("Untagged").tone(Tone::Warn)
                };
                chip.show(ui);
            },
        );
    }
    let when = if live {
        relative_time(now, recent.opened_at)
    } else {
        NOT_FOUND.to_owned()
    };
    ui.scope_builder(
        UiBuilder::new()
            .max_rect(right)
            .layout(Layout::right_to_left(Align::Center)),
        |ui| {
            ui.label(
                RichText::new(when)
                    .font(fonts::ui(tokens::text::BODY))
                    .color(tokens::TEXT_3),
            );
        },
    );
    live && response.on_hover_cursor(CursorIcon::PointingHand).clicked()
}
