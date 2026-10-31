//! The title bar: the file, one chip that opens its summary and revisions, and Validate.

use egui::text::{LayoutJob, TextFormat};
use std::sync::Arc;

use egui::{
    Align, Color32, Galley, Layout, Rect, RectAlign, Response, RichText, Role, Sense, Stroke, Ui, UiBuilder,
    WidgetInfo, pos2, vec2,
};
use pdfxray_core::{Document, RevisionInfo, XrefKind};

use crate::components::{Icon, ToolButton, focus, popover, tooltip};
use crate::object_facts::human_bytes;
use crate::state::{AppState, LoadedDocument};
use crate::states::{Screen, broken, encrypted};
use crate::theme::{fonts, tokens};

/// What the last validation run says about PDF/UA: the level it checked (`UA-1`) and whether the
/// file passed. `None` before a run and for runs against other standards.
///
/// The title bar only shows what a run proved: the file's own XMP claim (`pdfuaid:part`) is not
/// read by the core, and a claim nobody checked would look like a verified one.
pub fn ua_verdict(doc: &Document) -> Option<(String, bool)> {
    let run = doc.validation()?;
    let at = run.profile.find("UA-")?;
    let level: String = run.profile[at..]
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '-')
        .collect();
    Some((level, run.compliant))
}

/// Space between the bar's edges and its content.
const PAD: f32 = 14.0;
/// Room for the macOS traffic lights, which the window draws over the bar: three 12 pt lights
/// 8 pt apart, then the gap to the file name.
const TRAFFIC_LIGHTS: f32 = if cfg!(target_os = "macos") {
    52.0 + 16.0 + 12.0
} else {
    0.0
};
/// Space between the file name and the chip.
const FILE_GAP: f32 = 8.0;

pub(super) fn ui(ui: &mut Ui, state: &mut AppState) {
    let bar = ui.max_rect();
    super::separator(ui, bar.bottom());
    drag_window(ui, bar);

    let content = bar.shrink2(vec2(PAD, 0.0));
    let mut left = egui::Rect::from_min_max(content.min + vec2(TRAFFIC_LIGHTS, 0.0), content.max);
    if state.screen().has_workspace() {
        // The file name gives way to Validate: a long name is cut, never drawn under it.
        left.max.x -= validate_button().width(ui) + FILE_GAP;
    }
    ui.scope_builder(
        UiBuilder::new()
            .max_rect(left)
            .layout(Layout::left_to_right(Align::Center)),
        |ui| {
            ui.spacing_mut().item_spacing.x = FILE_GAP;
            file_cluster(ui, state);
        },
    );
    if !state.screen().has_workspace() {
        return;
    }
    ui.scope_builder(
        UiBuilder::new()
            .max_rect(content)
            .layout(Layout::right_to_left(Align::Center)),
        |ui| right_cluster(ui, state),
    );
}

/// The bar is the window's title bar on macOS, where the content runs under a transparent one:
/// dragging its empty space moves the window and a double click zooms it, as the native bar
/// would. The controls are laid out later, so they sit on top and keep their own clicks.
fn drag_window(ui: &Ui, bar: egui::Rect) {
    if !cfg!(target_os = "macos") {
        return;
    }
    let response = ui.interact(
        bar,
        ui.make_persistent_id("window-drag"),
        egui::Sense::click_and_drag(),
    );
    if response.double_clicked() {
        let zoomed = ui.input(|i| i.viewport().maximized.unwrap_or(false));
        ui.ctx()
            .send_viewport_cmd(egui::ViewportCommand::Maximized(!zoomed));
    } else if response.drag_started() {
        ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
    }
}

fn file_cluster(ui: &mut Ui, state: &mut AppState) {
    Icon::Document.show(ui, 16.0, tokens::ACCENT);
    // While a file opens, the bar only names it: nothing else is known yet.
    let shown = match state.screen() {
        Screen::Empty => Some(crate::APP_TITLE.to_owned()),
        Screen::Opening => Some(
            state
                .opening_name()
                .unwrap_or_else(|| crate::APP_TITLE.to_owned()),
        ),
        Screen::Encrypted | Screen::Workspace => None,
    };
    let Some(loaded) = state.document.as_ref().filter(|_| shown.is_none()) else {
        let name = shown.as_deref().unwrap_or(crate::APP_TITLE);
        file_name(ui, name, ui.available_width());
        return;
    };
    let runs = chip_runs(loaded, state.viewed_revision());
    // The chip keeps its width; the name takes what is left.
    let room = ui.available_width() - chip_width(ui, &runs) - FILE_GAP;
    file_name(ui, &loaded.info.name, room);
    let chip = title_chip(ui, &runs);
    let mut picked = None;
    popover::show(&chip, RectAlign::BOTTOM_START, SUMMARY_W, |ui| {
        summary(ui, loaded);
        ui.add_space(14.0);
        picked = revisions(ui, loaded, state.viewed_revision());
    });
    if let Some(index) = picked {
        state.set_revision(Some(index));
        egui::Popup::close_all(ui.ctx());
    }
}

/// The file name, cut with an ellipsis where it would pass `max` points; its tooltip then says
/// it in full.
fn file_name(ui: &mut Ui, name: &str, max: f32) {
    let job = fonts::title(name, tokens::text::LARGE, tokens::TEXT);
    let full = ui.painter().layout_job(job.clone()).size().x;
    let max = max.max(0.0);
    let response = ui
        .scope(|ui| {
            ui.set_max_width(max);
            ui.add(egui::Label::new(job).truncate().sense(Sense::hover()))
        })
        .inner;
    if full > max {
        tooltip::text(response, name);
    }
}

/// One stretch of the chip's text and the colour it is drawn in.
struct Run {
    text: String,
    color: Color32,
}

impl Run {
    fn new(text: impl Into<String>, color: Color32) -> Self {
        Self {
            text: text.into(),
            color,
        }
    }
}

/// What the merged chip says, run by run: `PDF 2.0 · UA-2 · rev 3/3`. Only what needs a look is
/// coloured, so a sound file reads as plain text and a problem is the one thing with a colour.
///
/// * The middle run is the encryption (its tags and PDF/UA are hidden), else what a validation
///   run proved about PDF/UA, else whether the file is tagged.
/// * A damaged index adds a compact `damaged` run.
/// * Viewing an older revision adds `read-only`.
fn chip_runs(loaded: &LoadedDocument, viewed: Option<u32>) -> Vec<Run> {
    let info = &loaded.info;
    let mut runs = vec![Run::new(format!("PDF {}", info.header_version), tokens::TEXT_2)];
    runs.push(if loaded.encryption.is_some() {
        Run::new("Encrypted", tokens::WARN)
    } else {
        match (ua_verdict(&loaded.doc), loaded.tagged) {
            (Some((level, true)), _) => Run::new(level, tokens::OK),
            (Some((level, false)), _) => Run::new(format!("{level} failed"), tokens::ERR),
            (None, true) => Run::new("Tagged", tokens::TEXT_2),
            (None, false) => Run::new("Untagged", tokens::WARN),
        }
    });
    if broken::damage(info).is_some() {
        runs.push(Run::new("damaged", tokens::ERR));
    }
    let count = info.revision_count;
    match viewed {
        Some(index) => {
            runs.push(Run::new(format!("rev {}/{count}", index + 1), tokens::WARN));
            runs.push(Run::new("read-only", tokens::WARN));
        }
        None => runs.push(Run::new(format!("rev {count}/{count}"), tokens::TEXT_2)),
    }
    runs
}

/// Height of the chip.
const CHIP_H: f32 = 22.0;
/// Side of the history icon.
const CHIP_ICON: f32 = 11.0;
/// Side of the chevron.
const CHEVRON: f32 = 10.0;

/// Space inside the chip: left of the icon, between the parts, right of the chevron.
const CHIP_LEFT: f32 = 7.0;
const CHIP_GAP: f32 = 5.0;
const CHIP_RIGHT: f32 = 6.0;

/// The chip's text laid out in one line, and its accessible name.
fn chip_text(ui: &Ui, runs: &[Run]) -> (Arc<Galley>, String) {
    let font = fonts::mono(tokens::mono::SMALL);
    let mut job = LayoutJob::default();
    let mut label = String::new();
    for (at, run) in runs.iter().enumerate() {
        if at > 0 {
            job.append(SEPARATOR, 0.0, TextFormat::simple(font.clone(), tokens::TEXT_3));
            label.push_str(SEPARATOR);
        }
        job.append(&run.text, 0.0, TextFormat::simple(font.clone(), run.color));
        label.push_str(&run.text);
    }
    (ui.painter().layout_job(job), label)
}

fn chip_width_of(galley: &Galley) -> f32 {
    CHIP_LEFT + CHIP_ICON + CHIP_GAP + galley.size().x + CHIP_GAP + CHEVRON + CHIP_RIGHT
}

/// The width [`title_chip`] takes.
fn chip_width(ui: &Ui, runs: &[Run]) -> f32 {
    chip_width_of(&chip_text(ui, runs).0)
}

/// The merged chip: a history icon, the runs and a chevron, one button that opens the summary.
/// Neutral tones only; it is a control, not a status light.
fn title_chip(ui: &mut Ui, runs: &[Run]) -> Response {
    let (galley, label) = chip_text(ui, runs);
    let width = chip_width_of(&galley);
    let (rect, response) = ui.allocate_exact_size(vec2(width, CHIP_H), Sense::click());
    response.widget_info(|| WidgetInfo::labeled(Role::Button, true, label.clone()));
    let open = egui::Popup::from_toggle_button_response(&response).is_open();
    let fill = if open || response.hovered() {
        tokens::NEUTRAL_ACTIVE
    } else {
        tokens::NEUTRAL_HOVER
    };
    let painter = ui.painter();
    painter.rect(
        rect,
        tokens::RADIUS_MD,
        fill,
        Stroke::new(1.0, tokens::POPOVER_HAIRLINE),
        egui::StrokeKind::Inside,
    );
    focus::ring(ui, &response, rect, tokens::RADIUS_MD);
    let mut x = rect.min.x + CHIP_LEFT;
    let centre =
        |x: f32, side: f32| Rect::from_center_size(pos2(x + side / 2.0, rect.center().y), vec2(side, side));
    Icon::History.paint(ui, centre(x, CHIP_ICON), CHIP_ICON, tokens::TEXT_3);
    x += CHIP_ICON + CHIP_GAP;
    ui.painter().galley(
        pos2(x, rect.center().y - galley.size().y / 2.0),
        galley.clone(),
        tokens::TEXT_2,
    );
    x += galley.size().x + CHIP_GAP;
    Icon::ChevronDown.paint(ui, centre(x, CHEVRON), CHEVRON, tokens::TEXT_3);
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Between the runs of the chip.
const SEPARATOR: &str = " · ";

/// Width of the popover.
const SUMMARY_W: f32 = 320.0;

/// The file's facts: the first part of the popover.
fn summary(ui: &mut Ui, loaded: &LoadedDocument) {
    let info = &loaded.info;
    ui.label(fonts::section_header("File"));
    ui.add_space(4.0);
    let tagged = if loaded.encryption.is_some() {
        "unknown (encrypted)"
    } else if loaded.tagged {
        "yes"
    } else {
        "no"
    };
    let ua = match (ua_verdict(&loaded.doc), loaded.doc.validation()) {
        (Some((level, true)), _) => format!("{level} · compliant"),
        (Some((level, false)), run) => format!(
            "{level} · not compliant · {} failed checks",
            run.map_or(0, |run| run.findings.len())
        ),
        (None, _) => "not validated".to_owned(),
    };
    let encryption = loaded.encryption.as_ref().map_or_else(
        || "none".to_owned(),
        |encryption| encrypted::cipher(encryption).unwrap_or_else(|| "encrypted".to_owned()),
    );
    let damage = broken::damage(info).map_or_else(
        || "none".to_owned(),
        |damage| format!("damaged xref · {} problems", damage.problems),
    );
    let rows = [
        ("Name", info.name.clone()),
        ("Size", human_bytes(info.byte_len)),
        ("Header", format!("PDF {}", info.header_version)),
        ("Pages", info.page_count.to_string()),
        ("Tagged", tagged.to_owned()),
        ("PDF/UA", ua),
        ("Encryption", encryption),
        ("Damage", damage),
    ];
    egui::Grid::new("file-summary")
        .num_columns(2)
        .spacing(vec2(14.0, 4.0))
        .show(ui, |ui| {
            for (key, value) in rows {
                ui.label(RichText::new(key).color(tokens::TEXT_3));
                ui.label(
                    RichText::new(value)
                        .font(fonts::mono(tokens::mono::BODY))
                        .color(tokens::TEXT),
                );
                ui.end_row();
            }
        });
}

/// The timeline of revisions, newest first. Picking one shows the objects as of that revision,
/// read-only; returns the one that was clicked.
fn revisions(ui: &mut Ui, loaded: &LoadedDocument, viewed: Option<u32>) -> Option<u32> {
    ui.label(fonts::section_header("Revisions"));
    ui.add_space(4.0);
    let count = loaded.info.revision_count;
    let shown = viewed.unwrap_or_else(|| u32::try_from(count.saturating_sub(1)).unwrap_or(0));
    let mut picked = None;
    for revision in loaded.doc.revisions().into_iter().rev() {
        if timeline_row(ui, &revision, revision.index == shown) {
            picked = Some(revision.index);
        }
    }
    picked
}

/// Height of a revision row.
const TIMELINE_ROW_H: f32 = 40.0;
/// Space above the name line of a revision row (and below its detail line).
const TIMELINE_TOP: f32 = 6.0;
/// Height of the name line of a revision row.
const TIMELINE_NAME_H: f32 = 16.0;

/// One revision of the timeline; true when it was clicked.
fn timeline_row(ui: &mut Ui, revision: &RevisionInfo, current: bool) -> bool {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), TIMELINE_ROW_H), egui::Sense::hover());
    let name = format!("rev {}", revision.index + 1);
    let response = ui.interact(
        rect,
        ui.make_persistent_id(("revision", revision.index)),
        egui::Sense::click(),
    );
    response.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, current, name.clone()));
    if current {
        ui.painter()
            .rect_filled(rect, tokens::RADIUS_SM, tokens::ACCENT_DIM);
    } else if response.hovered() {
        ui.painter()
            .rect_filled(rect, tokens::RADIUS_SM, tokens::NEUTRAL_HOVER);
    }
    focus::ring(ui, &response, rect, tokens::RADIUS_SM);
    let dot = egui::pos2(
        rect.min.x + 12.0,
        rect.min.y + TIMELINE_TOP + TIMELINE_NAME_H / 2.0,
    );
    ui.painter().circle_filled(
        dot,
        4.0,
        if revision.is_latest {
            tokens::ACCENT
        } else {
            tokens::TEXT_3
        },
    );
    let kind = match revision.xref_kind {
        XrefKind::Table => "xref table".to_owned(),
        XrefKind::Stream(r) => format!("xref stream {r}"),
    };
    let objects = revision.objects_defined.len();
    let detail = format!(
        "{kind} @ {} · {objects} object{}",
        revision.xref_offset,
        if objects == 1 { "" } else { "s" }
    );
    let text = egui::Rect::from_min_max(
        rect.min + vec2(26.0, TIMELINE_TOP),
        rect.max - vec2(8.0, TIMELINE_TOP),
    );
    ui.scope_builder(
        UiBuilder::new()
            .max_rect(text)
            .layout(Layout::top_down(Align::Min)),
        |ui| {
            ui.spacing_mut().item_spacing.y = 1.0;
            // The name line is as tall as its text, not as a button: the dot is centred on it.
            ui.spacing_mut().interact_size.y = TIMELINE_NAME_H;
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(&name)
                        .font(fonts::medium(tokens::text::BODY))
                        .color(tokens::TEXT),
                );
                if revision.is_latest {
                    ui.label(
                        RichText::new("latest")
                            .font(fonts::mono(tokens::mono::SMALL))
                            .color(tokens::ACCENT),
                    );
                }
            });
            ui.add(
                egui::Label::new(
                    RichText::new(detail)
                        .font(fonts::mono(tokens::mono::SMALL))
                        .color(tokens::TEXT_3),
                )
                .truncate(),
            );
        },
    );
    response.on_hover_cursor(egui::CursorIcon::PointingHand).clicked()
}

/// The Validate button of the right cluster.
fn validate_button() -> ToolButton<'static> {
    ToolButton::new("Validate")
        .icon(Icon::Validation)
        .primary()
        .tall()
}

fn right_cluster(ui: &mut Ui, state: &mut AppState) {
    let validate = validate_button().show(ui);
    if validate.clicked() {
        state.request_validation(ui.ctx());
    }
}
