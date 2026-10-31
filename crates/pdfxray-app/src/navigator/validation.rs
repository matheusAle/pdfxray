//! The Validation entry: the profile picker, the counters and the failed checks grouped by rule.
//!
//! Before a run (and while veraPDF is missing, being installed or running) the list gives way to
//! the state that explains what to do; the workers behind them are in [`crate::validation`].

use egui::text::{LayoutJob, TextFormat, TextWrapping};
use egui::{
    Align2, Color32, CursorIcon, FontId, Frame, Id, Margin, Painter, ProgressBar, Rect, RectAlign, RichText,
    Role, ScrollArea, Sense, Stroke, StrokeKind, TextEdit, Ui, WidgetInfo, pos2, vec2,
};
use pdfxray_core::numeric::{f32_from_f64, f64_from_u64, usize_from_u32};
use pdfxray_core::{Document, ValidationFinding, ValidationRun};
use pdfxray_verapdf::Profile;

use crate::components::{Icon, Tone, ToolButton, focus, popover};
use crate::findings::{self, RuleGroup};
use crate::state::AppState;
use crate::theme::{fonts, tokens};
use crate::validation::{Availability, Environment};

/// Label of the button that starts a run.
pub const VALIDATE_LABEL: &str = "Validate";

/// Label of the button that runs the file again.
pub const RERUN_LABEL: &str = "Re-run";

/// Label of the button that cancels a run.
pub const CANCEL_LABEL: &str = "Cancel";

/// Label of the button that downloads veraPDF.
pub const INSTALL_LABEL: &str = "Install veraPDF (≈34 MB download)";

/// Label of the button that applies the typed veraPDF path.
pub const USE_LABEL: &str = "Use";

/// Label of the button that looks for veraPDF again.
pub const RECHECK_LABEL: &str = "Re-check";

/// Name of the profile picker.
pub const PROFILE_LABEL: &str = "Profile";

/// Shown while looking for veraPDF.
pub const CHECKING_TEXT: &str = "Looking for veraPDF…";

/// Shown when no Java runtime was found.
pub const JAVA_MISSING_TEXT: &str =
    "Java 11 or newer is required to run veraPDF and was not found. Install Java, then press Re-check.";

/// Shown when Java is there but veraPDF is not.
pub const NOT_INSTALLED_TEXT: &str = "veraPDF was not found on this computer.";

/// Shown before the first run on a document.
pub const NO_RESULTS_TEXT: &str = "Choose a profile and press Validate to check this file.";

/// Shown when a run found nothing to fix.
pub const NO_FAILURES_TEXT: &str = "No failed checks.";

/// How many nodes a rule lists before the rest folds into "+N more".
pub const SHOWN_NODES: usize = 3;

const GROUP_H: f32 = 46.0;
/// A node of a rule is a tree row.
const NODE_H: f32 = tokens::ROW_H;
/// Where a rule's fail mark and the dots of its nodes are centred, from the left of the list.
const DOT_X: f32 = 32.0;
/// Where the title of a rule and the names of its nodes start.
const TEXT_X: f32 = 48.0;
const PICKER_H: f32 = 34.0;
const TILE_H: f32 = 54.0;

/// The standard a profile checks, shown after its name in the picker.
pub fn standard_of(profile: &Profile) -> &'static str {
    match profile {
        Profile::PdfUa1 => "ISO 14289-1",
        Profile::PdfUa2 => "ISO 14289-2",
        Profile::Wcag22Machine => "WCAG 2.2",
        Profile::Wtpdf10Accessibility => "Well-Tagged PDF 1.0",
        Profile::Iso32000_1Tagged => "ISO 32000-1",
        Profile::Iso32000_2Tagged => "ISO 32000-2",
        Profile::PdfA1a | Profile::PdfA1b => "ISO 19005-1",
        Profile::PdfA2a | Profile::PdfA2b | Profile::PdfA2u => "ISO 19005-2",
        Profile::PdfA3a | Profile::PdfA3b | Profile::PdfA3u => "ISO 19005-3",
        Profile::PdfA4 | Profile::PdfA4e | Profile::PdfA4f => "ISO 19005-4",
        Profile::Custom(_) => "custom profile",
    }
}

/// What the picker shows for the chosen profile: `PDF/UA-1 · ISO 14289-1`.
pub fn profile_line(profile: &Profile) -> String {
    format!("{} · {}", profile.label(), standard_of(profile))
}

/// The Validation entry of the navigator.
pub(super) fn ui(ui: &mut Ui, state: &mut AppState) {
    if state.validation.environment == Environment::Unchecked {
        state.recheck_verapdf(ui.ctx());
    }
    profile_picker(ui, state);
    ui.add_space(8.0);
    environment(ui, state);
    if let Some(error) = &state.validation.error {
        ui.colored_label(tokens::ERR, error);
    }
    run_controls(ui, state);
    results(ui, state);
}

/// What the navigator header shows at its right end: Re-run once there are results.
pub(super) fn header_action(ui: &mut Ui, state: &mut AppState) {
    let has_results = state
        .document
        .as_ref()
        .is_some_and(|loaded| loaded.doc.validation().is_some());
    if !has_results || state.validation.is_running() {
        return;
    }
    ui.add_enabled_ui(state.validation.is_ready(), |ui| {
        if ToolButton::new(RERUN_LABEL)
            .icon(Icon::Validation)
            .show(ui)
            .clicked()
        {
            state.validate(ui.ctx());
        }
    });
}

fn profile_picker(ui: &mut Ui, state: &mut AppState) {
    let enabled = !state.validation.is_running();
    let line = profile_line(&state.validation.profile);
    let sense = if enabled { Sense::click() } else { Sense::hover() };
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), PICKER_H), Sense::hover());
    let response = ui.interact(rect, ui.make_persistent_id("profile-picker"), sense);
    response.widget_info(|| WidgetInfo {
        label: Some(PROFILE_LABEL.to_owned()),
        current_text_value: Some(line.clone()),
        enabled,
        ..WidgetInfo::new(Role::ComboBox)
    });
    let response = if enabled {
        response.on_hover_cursor(CursorIcon::PointingHand)
    } else {
        response
    };
    let hovered = response.hovered() && enabled;
    ui.painter().rect(
        rect,
        tokens::RADIUS_MD,
        if hovered {
            tokens::SURFACE_3
        } else {
            tokens::SURFACE_2
        },
        Stroke::new(1.0, tokens::BORDER_STRONG),
        StrokeKind::Inside,
    );
    focus::ring(ui, &response, rect, tokens::RADIUS_MD);
    let ink = if enabled { tokens::TEXT } else { tokens::TEXT_3 };
    Icon::Validation.paint(
        ui,
        Rect::from_center_size(pos2(rect.min.x + 18.0, rect.center().y), vec2(16.0, 16.0)),
        14.0,
        tokens::TEXT_2,
    );
    ui.painter().text(
        pos2(rect.min.x + 34.0, rect.center().y),
        Align2::LEFT_CENTER,
        &line,
        fonts::ui(tokens::text::LARGE),
        ink,
    );
    Icon::ChevronDown.paint(
        ui,
        Rect::from_center_size(pos2(rect.max.x - 16.0, rect.center().y), vec2(16.0, 16.0)),
        14.0,
        tokens::TEXT_3,
    );

    let mut picked = None;
    popover::show(
        &response,
        RectAlign::BOTTOM_START,
        popover::inner_width(rect.width()),
        |ui| {
            ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
                for profile in Profile::builtin() {
                    if profile_row(ui, profile, *profile == state.validation.profile) {
                        picked = Some(profile.clone());
                    }
                }
            });
        },
    );
    if let Some(profile) = picked {
        egui::Popup::close_all(ui.ctx());
        state.change_profile(ui.ctx(), profile);
    }
}

/// One choice of the picker: the profile and the standard behind it.
fn profile_row(ui: &mut Ui, profile: &Profile, chosen: bool) -> bool {
    let name = profile.label();
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), tokens::ROW_H), Sense::hover());
    let response = ui
        .interact(rect, ui.make_persistent_id(("profile", &name)), Sense::click())
        .on_hover_cursor(egui::CursorIcon::PointingHand);
    response.widget_info(|| WidgetInfo::selected(Role::Button, true, chosen, name.clone()));
    if chosen {
        ui.painter()
            .rect_filled(rect, tokens::RADIUS_SM, tokens::ACCENT_DIM);
    } else if response.hovered() {
        ui.painter()
            .rect_filled(rect, tokens::RADIUS_SM, tokens::NEUTRAL_HOVER);
    }
    focus::ring(ui, &response, rect, tokens::RADIUS_SM);
    ui.painter().text(
        pos2(rect.min.x + 8.0, rect.center().y),
        Align2::LEFT_CENTER,
        &name,
        fonts::ui(tokens::text::BODY),
        if chosen { tokens::ACCENT } else { tokens::TEXT },
    );
    ui.painter().text(
        pos2(rect.max.x - 8.0, rect.center().y),
        Align2::RIGHT_CENTER,
        standard_of(profile),
        fonts::mono(tokens::mono::SMALL),
        tokens::TEXT_3,
    );
    response.on_hover_cursor(CursorIcon::PointingHand).clicked()
}

/// A note about the environment, in a card.
fn note(ui: &mut Ui, tone: Tone, content: impl FnOnce(&mut Ui)) {
    let (fill, border, _) = tone.colors();
    Frame::new()
        .fill(fill)
        .stroke(Stroke::new(1.0, border))
        .corner_radius(tokens::RADIUS_MD)
        .inner_margin(Margin::same(8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 6.0;
            content(ui);
        });
    ui.add_space(6.0);
}

fn environment(ui: &mut Ui, state: &mut AppState) {
    match state.validation.environment.clone() {
        Environment::Unchecked | Environment::Checking => {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(CHECKING_TEXT);
            });
        }
        Environment::Known(Availability::Ready { version }) => {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(format!("Using veraPDF {version}"))
                        .font(fonts::mono(tokens::mono::SMALL))
                        .color(tokens::TEXT_3),
                );
                ui.collapsing("veraPDF location", |ui| location_row(ui, state));
            });
        }
        Environment::Known(Availability::JavaMissing) => {
            note(ui, Tone::Warn, |ui| {
                ui.colored_label(tokens::WARN, JAVA_MISSING_TEXT);
                location_row(ui, state);
            });
        }
        Environment::Known(Availability::NotInstalled) => {
            note(ui, Tone::Warn, |ui| {
                ui.colored_label(tokens::WARN, NOT_INSTALLED_TEXT);
                install_row(ui, state);
                location_row(ui, state);
            });
        }
    }
}

fn install_row(ui: &mut Ui, state: &mut AppState) {
    if let Some((done, total)) = state.validation.install_progress() {
        let text = progress_text(done, total);
        let bar = match total {
            Some(total) if total > 0 => ProgressBar::new(fraction(done, total)),
            _ => ProgressBar::new(0.0).animate(true),
        };
        ui.label("Installing veraPDF…");
        ui.add(bar.desired_width(200.0).text(text));
    } else if ToolButton::new(INSTALL_LABEL).primary().show(ui).clicked() {
        state.install_verapdf(ui.ctx());
    }
}

fn location_row(ui: &mut Ui, state: &mut AppState) {
    ui.label("veraPDF path:");
    ui.add(
        TextEdit::singleline(&mut state.validation.explicit_path)
            .desired_width(ui.available_width())
            .hint_text("verapdf launcher, jar or install folder"),
    );
    ui.horizontal(|ui| {
        let used = ToolButton::new(USE_LABEL).show(ui).clicked();
        let rechecked = ToolButton::new(RECHECK_LABEL).show(ui).clicked();
        if used || rechecked {
            state.recheck_verapdf(ui.ctx());
        }
    });
}

/// `done / total` as 0..=1.
fn fraction(done: u64, total: u64) -> f32 {
    // Progress bars do not need more than f32 precision.
    let ratio = f64_from_u64(done) / f64_from_u64(total);
    f32_from_f64(ratio.clamp(0.0, 1.0))
}

/// Download progress as text, e.g. `12.5 / 34.0 MB`.
pub fn progress_text(done: u64, total: Option<u64>) -> String {
    let mb = |bytes: u64| f64_from_u64(bytes) / 1_000_000.0;
    match total {
        Some(total) => format!("{:.1} / {:.1} MB", mb(done), mb(total)),
        None => format!("{:.1} MB", mb(done)),
    }
}

/// The run in progress with Cancel, or Validate while there are no results yet.
fn run_controls(ui: &mut Ui, state: &mut AppState) {
    if let Some(label) = state.validation.running_profile().map(Profile::label) {
        ui.horizontal(|ui| {
            ui.spinner();
            ui.label(format!("Running {label}…"));
            if ToolButton::new(CANCEL_LABEL).show(ui).clicked() {
                state.cancel_validation();
            }
        });
        ui.add_space(4.0);
        return;
    }
    let has_results = state
        .document
        .as_ref()
        .is_some_and(|loaded| loaded.doc.validation().is_some());
    if has_results {
        return;
    }
    ui.horizontal(|ui| {
        let enabled = state.validation.is_ready() && state.document.is_some();
        ui.add_enabled_ui(enabled, |ui| {
            if ToolButton::new(VALIDATE_LABEL)
                .icon(Icon::Validation)
                .primary()
                .show(ui)
                .clicked()
            {
                state.validate(ui.ctx());
            }
        });
    });
    ui.add_space(4.0);
    ui.label(RichText::new(NO_RESULTS_TEXT).color(tokens::TEXT_3));
}

fn results(ui: &mut Ui, state: &mut AppState) {
    let Some(loaded) = &state.document else {
        return;
    };
    let Some(run) = loaded.doc.validation() else {
        return;
    };
    counters(ui, run);
    ui.add_space(8.0);
    if run.findings.is_empty() {
        ui.label(RichText::new(NO_FAILURES_TEXT).color(tokens::TEXT_3));
        return;
    }
    let words: Vec<String> = state
        .nav
        .filter
        .split_whitespace()
        .map(str::to_lowercase)
        .collect();
    let groups: Vec<(RuleGroup, Vec<u32>)> = findings::groups(run)
        .into_iter()
        .filter_map(|group| {
            let shown: Vec<u32> = group
                .findings
                .iter()
                .copied()
                .filter(|index| matches(&loaded.doc, &group, &run.findings[usize_from_u32(*index)], &words))
                .collect();
            (!shown.is_empty()).then_some((group, shown))
        })
        .collect();
    if groups.is_empty() {
        ui.label(
            RichText::new(format!("No findings match “{}”.", state.nav.filter.trim())).color(tokens::TEXT_3),
        );
        return;
    }
    let current = state.current_finding();
    let reveal = std::mem::take(&mut state.validation.reveal_in_list);
    let mut action = ListAction::default();
    ScrollArea::vertical()
        .id_salt("findings")
        .auto_shrink(false)
        .show(ui, |ui| {
            // The rows of a rule are as far apart as the rows of a tree.
            ui.spacing_mut().item_spacing.y = 4.0;
            for (group, shown) in &groups {
                let list = GroupList {
                    group,
                    shown,
                    doc: &loaded.doc,
                    run,
                    current,
                    open: !state.validation.folded.contains(&group.rule),
                    all: state.validation.listed_all.contains(&group.rule),
                    reveal,
                };
                group_ui(ui, &list, &mut action);
                ui.add_space(4.0);
            }
        });
    if let Some(rule) = action.toggled
        && !state.validation.folded.remove(&rule)
    {
        state.validation.folded.insert(rule);
    }
    if let Some(rule) = action.listed_all {
        state.validation.listed_all.insert(rule);
    }
    if let Some(index) = action.selected {
        state.select_finding(index);
    }
}

/// True when every word of the filter is in the rule, its title or the finding's node.
fn matches(doc: &Document, group: &RuleGroup, finding: &ValidationFinding, words: &[String]) -> bool {
    if words.is_empty() {
        return true;
    }
    let target = finding
        .target
        .as_ref()
        .map(ToString::to_string)
        .unwrap_or_default();
    let haystack = format!(
        "{} {} {} {} {target}",
        finding.rule,
        group.title,
        finding.description,
        findings::node_label(doc, finding)
    )
    .to_lowercase();
    words.iter().all(|word| haystack.contains(word))
}

/// What the user did in the list this frame.
#[derive(Debug, Default)]
struct ListAction {
    selected: Option<u32>,
    toggled: Option<String>,
    listed_all: Option<String>,
}

/// One rule and the findings of it that pass the filter.
struct GroupList<'a> {
    group: &'a RuleGroup,
    shown: &'a [u32],
    doc: &'a Document,
    run: &'a ValidationRun,
    current: Option<u32>,
    open: bool,
    all: bool,
    reveal: bool,
}

/// The three tiles: failed checks, failed rules, passed checks.
fn counters(ui: &mut Ui, run: &ValidationRun) {
    let rules = findings::groups(run).len();
    let tiles = [
        (
            crate::navigator::thousands(to_usize(run.checks_failed)),
            "failed",
            tokens::ERR,
        ),
        (crate::navigator::thousands(rules), "rules", tokens::WARN),
        (
            crate::navigator::thousands(to_usize(run.checks_passed)),
            "passed",
            tokens::OK,
        ),
    ];
    let gap = 6.0;
    let width = (ui.available_width() - gap * 2.0) / 3.0;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = gap;
        for (number, caption, color) in tiles {
            Frame::new()
                .fill(tokens::SURFACE_2)
                .stroke(Stroke::new(1.0, tokens::BORDER))
                .corner_radius(tokens::RADIUS_MD)
                .inner_margin(Margin::symmetric(10, 6))
                .show(ui, |ui| {
                    ui.set_width(width - 20.0 - 2.0);
                    ui.set_min_height(TILE_H - 12.0 - 2.0);
                    ui.spacing_mut().item_spacing.y = 0.0;
                    ui.label(RichText::new(number).font(fonts::mono_bold(18.0)).color(color));
                    ui.label(
                        RichText::new(caption)
                            .font(fonts::ui(tokens::text::SMALL))
                            .color(tokens::TEXT_3),
                    );
                });
        }
    });
}

fn to_usize(n: u64) -> usize {
    usize::try_from(n).unwrap_or(usize::MAX)
}

fn group_ui(ui: &mut Ui, list: &GroupList<'_>, action: &mut ListAction) {
    let group = list.group;
    let id = ui.make_persistent_id(("rule", &group.rule));
    if group_row(ui, id, group, list.shown.len(), list.open).clicked() {
        action.toggled = Some(group.rule.clone());
    }
    if !list.open {
        return;
    }
    let visible = if list.all { list.shown.len() } else { SHOWN_NODES };
    for &index in list.shown.iter().take(visible) {
        let finding = &list.run.findings[usize_from_u32(index)];
        if node_row(
            ui,
            id.with(index),
            list.doc,
            finding,
            list.current == Some(index),
            list.reveal,
        ) {
            action.selected = Some(index);
        }
    }
    let hidden = list.shown.len().saturating_sub(visible);
    if hidden > 0 && more_row(ui, id.with("more"), list, hidden) {
        action.listed_all = Some(group.rule.clone());
    }
}

/// `text` on at most `rows` lines of `width`; what does not fit ends in an ellipsis.
fn fitted(
    painter: &Painter,
    text: &str,
    font: FontId,
    color: Color32,
    width: f32,
    rows: usize,
) -> std::sync::Arc<egui::Galley> {
    let mut job = LayoutJob::single_section(text.to_owned(), TextFormat::simple(font, color));
    job.wrap = TextWrapping {
        max_width: width.max(10.0),
        max_rows: rows,
        break_anywhere: false,
        overflow_character: Some('…'),
    };
    painter.layout_job(job)
}

/// The header of a rule: chevron, fail mark, title (on up to two lines), rule id and the count.
fn group_row(ui: &mut Ui, id: Id, group: &RuleGroup, count: usize, open: bool) -> egui::Response {
    let pill = ui.painter().layout_no_wrap(
        count.to_string(),
        fonts::mono_bold(tokens::mono::SMALL),
        tokens::ERR,
    );
    let pill_size = vec2(pill.size().x + 12.0, 16.0);
    let width = ui.available_width();
    let text_w = width - TEXT_X - pill_size.x - 18.0;
    let title = fitted(
        ui.painter(),
        &group.title,
        fonts::ui(tokens::text::LARGE),
        tokens::TEXT,
        text_w,
        2,
    );
    let id_line = fitted(
        ui.painter(),
        &group.short,
        fonts::mono_semibold(tokens::mono::SMALL),
        tokens::TEXT_3,
        text_w,
        1,
    );
    let height = (title.size().y + id_line.size().y + 20.0).max(GROUP_H);
    let (rect, _) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
    let response = ui
        .interact(rect, id, Sense::click())
        .on_hover_cursor(egui::CursorIcon::PointingHand);
    let name = format!("{} · {}", group.title, group.short);
    response.widget_info(|| WidgetInfo::selected(Role::Button, true, open, name.clone()));
    let fill = if response.hovered() {
        tokens::SURFACE_3
    } else {
        tokens::SURFACE_2
    };
    ui.painter()
        .rect_filled(rect.shrink2(vec2(1.0, 0.0)), tokens::RADIUS_MD, fill);
    let chevron = if open {
        Icon::ChevronDown
    } else {
        Icon::ChevronRight
    };
    chevron.paint(
        ui,
        Rect::from_center_size(pos2(rect.min.x + 14.0, rect.min.y + 17.0), vec2(14.0, 14.0)),
        12.0,
        tokens::TEXT_3,
    );
    let painter = ui.painter();
    findings::fail_mark(
        painter,
        Rect::from_center_size(pos2(rect.min.x + DOT_X, rect.min.y + 17.0), vec2(14.0, 14.0)),
        tokens::ERR,
    );
    let (fill, border, ink) = Tone::Err.colors();
    let pill_rect = Rect::from_min_size(pos2(rect.max.x - 10.0 - pill_size.x, rect.min.y + 9.0), pill_size);
    painter.rect(pill_rect, 8.0, fill, Stroke::new(1.0, border), StrokeKind::Inside);
    painter.galley(pill_rect.center() - pill.size() / 2.0, pill, ink);
    let text_x = rect.min.x + TEXT_X;
    let id_y = rect.min.y + 9.0 + title.size().y + 2.0;
    painter.galley(pos2(text_x, rect.min.y + 9.0), title, tokens::TEXT);
    painter.galley(pos2(text_x, id_y), id_line, tokens::TEXT_3);
    response
}

/// One node of a rule: its dot, its name and its page. True when clicked.
fn node_row(
    ui: &mut Ui,
    id: Id,
    doc: &Document,
    finding: &ValidationFinding,
    selected: bool,
    reveal: bool,
) -> bool {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), NODE_H), Sense::hover());
    let response = ui
        .interact(rect, id, Sense::click())
        .on_hover_cursor(egui::CursorIcon::PointingHand);
    let label = findings::node_label(doc, finding);
    let page = findings::page_of(doc, finding).map(|page| format!("p{}", page + 1));
    let name = match &page {
        Some(page) => format!("{label} {page}"),
        None => label.clone(),
    };
    response.widget_info(|| WidgetInfo::selected(Role::Button, true, selected, name.clone()));
    if selected && reveal {
        ui.scroll_to_rect(rect, None);
    }
    let kind = finding.target.as_ref().map_or(tokens::TEXT_3, tokens::kind_color);
    let painter = ui.painter();
    if selected {
        painter.rect(
            rect.shrink2(vec2(2.0, 0.5)),
            tokens::RADIUS_SM,
            tokens::ACCENT_DIM,
            Stroke::new(1.0, tokens::ACCENT),
            StrokeKind::Inside,
        );
    } else if response.hovered() {
        painter.rect_filled(rect.shrink2(vec2(2.0, 0.5)), tokens::RADIUS_SM, tokens::SURFACE_2);
    }
    painter.circle_filled(pos2(rect.min.x + DOT_X, rect.center().y), 3.0, kind);
    let mut x = rect.min.x + TEXT_X;
    let galley = painter.layout_no_wrap(label, fonts::mono(tokens::mono::BODY), kind);
    let at = pos2(x, rect.center().y - galley.size().y / 2.0);
    x += galley.size().x + 8.0;
    painter.galley(at, galley, kind);
    if let Some(page) = page {
        painter.text(
            pos2(x, rect.center().y),
            Align2::LEFT_CENTER,
            page,
            fonts::mono(tokens::mono::SMALL),
            tokens::TEXT_3,
        );
    }
    response.clicked()
}

/// `+4 more p2-p5`: lists the rest of the nodes when clicked.
fn more_row(ui: &mut Ui, id: Id, list: &GroupList<'_>, hidden: usize) -> bool {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), NODE_H), Sense::hover());
    let response = ui
        .interact(rect, id, Sense::click())
        .on_hover_cursor(egui::CursorIcon::PointingHand);
    let pages: Vec<u32> = list
        .shown
        .iter()
        .skip(list.shown.len() - hidden)
        .filter_map(|index| findings::page_of(list.doc, &list.run.findings[usize_from_u32(*index)]))
        .collect();
    let text = match (pages.iter().min(), pages.iter().max()) {
        (Some(low), Some(high)) if low == high => format!("+{hidden} more p{}", low + 1),
        (Some(low), Some(high)) => format!("+{hidden} more p{}-p{}", low + 1, high + 1),
        _ => format!("+{hidden} more"),
    };
    response.widget_info(|| WidgetInfo::labeled(Role::Button, true, text.clone()));
    if response.hovered() {
        ui.painter()
            .rect_filled(rect.shrink2(vec2(2.0, 0.5)), tokens::RADIUS_SM, tokens::SURFACE_2);
    }
    ui.painter().circle_filled(
        pos2(rect.min.x + DOT_X, rect.center().y),
        3.0,
        tokens::KIND_CONTENT.gamma_multiply(0.6),
    );
    ui.painter().text(
        pos2(rect.min.x + TEXT_X, rect.center().y),
        Align2::LEFT_CENTER,
        &text,
        fonts::mono(tokens::mono::BODY),
        tokens::TEXT_3,
    );
    response.clicked()
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn the_picker_names_the_standard_after_the_profile() {
        assert_eq!(profile_line(&Profile::PdfUa1), "PDF/UA-1 · ISO 14289-1");
        assert_eq!(profile_line(&Profile::PdfA2b), "PDF/A-2b · ISO 19005-2");
        assert_eq!(
            profile_line(&Profile::Wcag22Machine),
            "WCAG 2.2 (machine) · WCAG 2.2"
        );
    }

    #[test]
    fn download_progress_reads_in_megabytes() {
        assert_eq!(progress_text(12_500_000, Some(34_000_000)), "12.5 / 34.0 MB");
        assert_eq!(progress_text(2_000_000, None), "2.0 MB");
    }

    #[test]
    fn a_share_of_a_download_is_between_zero_and_one() {
        assert!((fraction(1, 4) - 0.25).abs() < f32::EPSILON);
        assert!((fraction(9, 4) - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn every_builtin_profile_has_a_standard() {
        assert!(Profile::builtin().iter().all(|p| !standard_of(p).is_empty()));
    }
}
