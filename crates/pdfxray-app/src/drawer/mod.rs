//! The drawer under the canvas: tabs with live counts, drag-resizable by its top edge.
//!
//! Collapsed it is a strip of tabs; see [`crate::state::drawer`] for when it opens.

pub mod diagnostics;
pub mod hit_stack;
pub mod operators;
pub mod paint_order;
pub mod references;
pub mod speech;
mod table;
pub mod validation;

use egui::text::{LayoutJob, TextFormat, TextWrapping};
use egui::{CursorIcon, Frame, Label, Margin, RichText, Role, Sense, Shape, Stroke, Ui, WidgetInfo, vec2};

use pdfxray_core::Severity;
use pdfxray_core::numeric::usize_from_u32;

use crate::components::{Badge, Tone, focus};
use crate::state::drawer::{Opened, STRIP_H};
use crate::state::{AppState, DrawerTab};
use crate::theme::{fonts, tokens};

const HANDLE_H: f32 = 5.0;

/// The tone of a tab's count: neutral, except where the count is of problems. Diagnostics take
/// the colour of the worst one (information stays neutral) and failed validation checks are
/// errors.
pub fn badge_tone(state: &AppState, tab: DrawerTab) -> Tone {
    let Some(loaded) = &state.document else {
        return Tone::Neutral;
    };
    match tab {
        DrawerTab::Diagnostics => match loaded.info.diagnostics.iter().map(|d| d.severity).max() {
            Some(Severity::Error) => Tone::Err,
            Some(Severity::Warning) => Tone::Warn,
            Some(Severity::Info) | None => Tone::Neutral,
        },
        DrawerTab::Validation if count(state, tab) > 0 => Tone::Err,
        _ => Tone::Neutral,
    }
}

/// How many rows the tab lists: its count badge. The Speech tab counts what the reader announces
/// once the document has been read for it; until then it has no badge (see [`has_count`]).
pub fn count(state: &AppState, tab: DrawerTab) -> usize {
    let Some(loaded) = &state.document else {
        return 0;
    };
    match tab {
        DrawerTab::HitStack => state
            .drawer
            .hit_stack
            .as_ref()
            .map_or(0, |stack| stack.entries.len()),
        DrawerTab::Diagnostics => loaded.info.diagnostics.len(),
        DrawerTab::Validation => loaded.doc.validation().map_or(0, |run| run.findings.len()),
        DrawerTab::Operators => loaded
            .doc
            .captured(state.canvas.page)
            .map_or(0, |captured| captured.ops.len()),
        DrawerTab::PaintOrder => usize_from_u32(state.op_total()),
        DrawerTab::References => references::count(state),
        DrawerTab::Speech => state
            .speech_if_read(state.drawer.speech.preset)
            .map_or(0, |data| data.spoken),
    }
}

/// True when the tab shows its count: every tab does, but Speech, whose count costs a read of the
/// whole document and so waits until the tab has been looked at.
pub fn has_count(state: &AppState, tab: DrawerTab) -> bool {
    tab != DrawerTab::Speech || state.speech_if_read(state.drawer.speech.preset).is_some()
}

/// Draws the drawer host.
pub fn ui(ui: &mut Ui, state: &mut AppState) {
    // A tab that does not belong to this entry (left over from another one) gives way to the first.
    let tabs = DrawerTab::for_rail(state.nav.rail, state.canvas.view.stepper);
    if !tabs.contains(&state.drawer.tab) {
        // While stepping, the stepper takes the tabs of Operators and of what is left over.
        state.drawer.settle_tab(if tabs.contains(&DrawerTab::PaintOrder) {
            DrawerTab::PaintOrder
        } else {
            tabs[0]
        });
    }
    let validated = state.validation_finished();
    let stepping = state.canvas.view.stepper;
    let opened = state.drawer.follow(stepping, validated);
    if opened != Opened::No {
        ui.ctx().request_repaint();
    }
    if opened == Opened::Validated && tabs.contains(&DrawerTab::Validation) {
        state.drawer.show_tab(DrawerTab::Validation);
    }
    if !state.drawer.open {
        tab_bar(ui, state);
        return;
    }
    // The transcript's note and count need the document read for its reader.
    if state.drawer.tab == DrawerTab::Speech {
        state.speech(state.drawer.speech.preset);
    }
    resize_handle(ui, state);
    tab_bar(ui, state);
    Frame::new()
        .inner_margin(Margin::symmetric(10, 4))
        .show(ui, |ui| match state.drawer.tab {
            DrawerTab::HitStack => hit_stack::ui(ui, state),
            DrawerTab::Diagnostics => diagnostics::ui(ui, state),
            DrawerTab::Validation => validation::ui(ui, state),
            DrawerTab::Operators => operators::ui(ui, state),
            DrawerTab::PaintOrder => paint_order::ui(ui, state),
            DrawerTab::References => references::ui(ui, state),
            DrawerTab::Speech => speech::ui(ui, state),
        });
}

/// The top edge: dragging it up makes the drawer taller.
fn resize_handle(ui: &mut Ui, state: &mut AppState) {
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), HANDLE_H), Sense::drag());
    response.widget_info(|| WidgetInfo::labeled(Role::Splitter, true, "Resize drawer"));
    if response.dragged() {
        state.drawer.height = (state.drawer.height - response.drag_delta().y).max(tokens::DRAWER_MIN_H);
    }
    if response.hovered() || response.dragged() {
        ui.painter().hline(
            rect.x_range(),
            rect.min.y + 1.0,
            Stroke::new(2.0, tokens::ACCENT.gamma_multiply(0.6)),
        );
    }
    response.on_hover_cursor(CursorIcon::ResizeVertical);
}

/// Space between the strip's edge and the first tab, and the last tab and the edge.
const STRIP_INSET: f32 = 10.0;

/// How the tabs of the strip are set: the space between tabs, between a title and its count, and
/// whether the tabs that are not open keep a count that is not a problem.
#[derive(Debug, Clone, Copy)]
struct Strip {
    gap: f32,
    inner: f32,
    quiet: bool,
}

/// The looks of the strip, from the roomiest; the first one whose tabs all fit is used, so a tab
/// is never cut at the edge of a narrow drawer.
const STRIPS: [Strip; 3] = [
    Strip {
        gap: 12.0,
        inner: 6.0,
        quiet: false,
    },
    Strip {
        gap: 8.0,
        inner: 4.0,
        quiet: false,
    },
    Strip {
        gap: 8.0,
        inner: 4.0,
        quiet: true,
    },
];

impl Strip {
    /// Whether `tab` shows its count: counts that are problems stay, and so does the one of the
    /// open tab, when the strip is `quiet`.
    fn counts(self, state: &AppState, tab: DrawerTab) -> bool {
        has_count(state, tab)
            && (!self.quiet || state.drawer.tab == tab || badge_tone(state, tab) != Tone::Neutral)
    }

    /// The width of all the tabs, the open one in medium like the rest.
    fn width(self, ui: &Ui, state: &AppState, tabs: &[DrawerTab]) -> f32 {
        // Each tab with a gap after it, less the one after the last.
        let widths: f32 = tabs
            .iter()
            .map(|&tab| {
                let title = ui
                    .painter()
                    .layout_no_wrap(
                        tab.title().to_owned(),
                        fonts::medium(tokens::text::BODY),
                        tokens::TEXT,
                    )
                    .size()
                    .x;
                let badge = if self.counts(state, tab) {
                    self.inner + Badge::new(count(state, tab)).width(ui)
                } else {
                    0.0
                };
                title + badge + self.gap
            })
            .sum();
        (widths - self.gap).max(0.0)
    }
}

fn tab_bar(ui: &mut Ui, state: &mut AppState) {
    let width = ui.available_width();
    let (bar, _) = ui.allocate_exact_size(vec2(width, STRIP_H), Sense::hover());
    let mut tabs = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(bar.shrink2(vec2(STRIP_INSET, 0.0)))
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    let all = DrawerTab::for_rail(state.nav.rail, state.canvas.view.stepper);
    let room = width - 2.0 * STRIP_INSET;
    let strip = STRIPS
        .into_iter()
        .find(|strip| strip.width(ui, state, all) <= room)
        .unwrap_or(STRIPS[STRIPS.len() - 1]);
    tabs.spacing_mut().item_spacing.x = strip.gap;
    for &tab in all {
        let active = state.drawer.tab == tab;
        let count = count(state, tab);
        let tab_id = tabs.make_persistent_id(("drawer-tab", tab));
        // The tab was laid out before the pointer reached it, so it follows last frame's hover.
        let was_hovered = tabs
            .ctx()
            .read_response(tab_id)
            .is_some_and(|last| last.hovered());
        // The wash goes under the tab, whose size is known only once it is laid out.
        let wash = tabs.painter().add(Shape::Noop);
        let inner = tabs.scope(|ui| {
            ui.spacing_mut().item_spacing.x = strip.inner;
            ui.horizontal(|ui| {
                let ink = if active || was_hovered {
                    tokens::TEXT
                } else {
                    tokens::TEXT_2
                };
                // The active tab is medium; the others keep its width, so activating one moves
                // nothing.
                let medium = ui.painter().layout_no_wrap(
                    tab.title().to_owned(),
                    fonts::medium(tokens::text::BODY),
                    ink,
                );
                let font = if active {
                    fonts::medium(tokens::text::BODY)
                } else {
                    fonts::ui(tokens::text::BODY)
                };
                ui.add_sized(
                    medium.size(),
                    Label::new(RichText::new(tab.title()).font(font).color(ink)).selectable(false),
                );
                if strip.counts(state, tab) {
                    Badge::new(count).tone(badge_tone(state, tab)).show(ui);
                }
            });
        });
        let rect = inner.response.rect.expand2(vec2(2.0, 0.0));
        let click = tabs
            .interact(rect, tab_id, Sense::click())
            .on_hover_cursor(CursorIcon::PointingHand);
        click.widget_info(|| WidgetInfo::selected(Role::Button, true, active, tab.title()));
        let target = rect.expand2(vec2(4.0, -6.0));
        if click.hovered() && !active {
            tabs.painter().set(
                wash,
                Shape::rect_filled(target, tokens::RADIUS_SM, tokens::NEUTRAL_HOVER),
            );
        }
        focus::ring(ui, &click, target, tokens::RADIUS_SM);
        if active && state.drawer.open {
            ui.painter()
                .hline(rect.x_range(), bar.max.y - 1.0, Stroke::new(2.0, tokens::ACCENT));
        }
        if click.clicked() {
            state.drawer.click_tab(tab);
        }
    }
    let notes: Vec<String> = match state.drawer.tab {
        DrawerTab::HitStack => hit_stack::header_note(state).into_iter().collect(),
        DrawerTab::Operators => operators::header_note(state).into_iter().collect(),
        DrawerTab::Diagnostics => diagnostics::header_note(state).into_iter().collect(),
        DrawerTab::Validation => validation::header_note(state).into_iter().collect(),
        DrawerTab::Speech => speech::header_notes(state),
        DrawerTab::References | DrawerTab::PaintOrder => {
            state.drawer.tab.hint().map(str::to_owned).into_iter().collect()
        }
    };
    if state.drawer.open {
        tab_hint(ui, bar, tabs.min_rect().max.x, &notes);
    }
}

/// Room kept between the last tab and the note.
const HINT_GAP: f32 = 10.0;

/// The dim line at the right end of the tab bar: the first of `notes` (longest first) that fits
/// between the last tab and the edge, or the last one cut with a `…`.
fn tab_hint(ui: &Ui, bar: egui::Rect, tabs_end: f32, notes: &[String]) {
    let right = bar.max.x - 12.0;
    let room = (right - tabs_end - HINT_GAP).max(0.0);
    let layout = |note: &str| {
        let mut job = LayoutJob::single_section(
            note.to_owned(),
            TextFormat::simple(fonts::mono(tokens::mono::SMALL), tokens::TEXT_3),
        );
        job.wrap = TextWrapping {
            max_width: room,
            max_rows: 1,
            break_anywhere: true,
            overflow_character: Some('…'),
        };
        ui.painter().layout_job(job)
    };
    let fitting = notes
        .iter()
        .map(|note| layout(note))
        .find(|galley| galley.rows.len() == 1 && !galley.elided);
    let Some(galley) = fitting.or_else(|| notes.last().map(|note| layout(note))) else {
        return;
    };
    let at = egui::pos2(right - galley.size().x, bar.center().y - galley.size().y / 2.0);
    ui.painter().galley(at, galley, tokens::TEXT_3);
}
