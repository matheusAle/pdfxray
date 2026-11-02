//! One property: a dim key, a mono value and a copy icon that only shows while the row is
//! hovered or has the keyboard focus. Copying flips the icon to a check for [`COPIED_FOR`]
//! seconds and shows the "Copied" toast.
//!
//! The row is the Tab stop ("2.13 Spec · copy without a pointer"), not the icon: a focused row
//! looks like a hovered one with the inset focus ring. `⌘C` copies the value, `⇧⌘C` the key and
//! the value, `Enter` follows a link value, and `⇧F10`, the context-menu key or a right-click open
//! the value menu (see [`crate::components::context_menu::attach_value`]).
//!
//! An array, a literal string or a hex string that needs more than one line wraps with a hanging
//! indent and folds past three lines (see [`long_value`]); Copy value still copies all of it.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use egui::containers::Popup;
use egui::{
    Align, Color32, Context, Event, Id, Key, Label, Layout, Modifiers, PointerButton, Pos2, Rect, Response,
    RichText, Role, Sense, Ui, UiBuilder, Vec2, WidgetInfo, vec2,
};

use pdfxray_core::numeric::f32_from_usize;

use super::icon::Icon;
use super::long_value::{self, Wrapped};
use super::{focus, prop_hints, toast, tooltip};
use crate::theme::{fonts, motion, tokens};

/// How long the check mark replaces the copy icon.
pub const COPIED_FOR: f64 = 1.2;

/// Width of the key column.
pub const KEY_W: f32 = 116.0;
/// Where the value column starts, from the left end of a row.
pub const VALUE_X: f32 = PAD + KEY_W;
const ICON_BOX: f32 = 20.0;
const PAD: f32 = 8.0;
const MIN_H: f32 = tokens::ROW_H;
const SWATCH: f32 = 14.0;
const TRAILING: f32 = 18.0;
/// Longest value the row's accessible name repeats; the rest ends in an ellipsis.
const NAME_CHARS: usize = 80;

/// Where a row leaves its value menu while it is open.
fn context_slot() -> Id {
    Id::unique("pdfxray-prop-row-context")
}

/// What a value menu is about: one property row.
#[derive(Debug, Clone)]
pub struct ValueTarget {
    /// The row; the menu belongs to it.
    pub row: Response,
    /// The key, as the row shows it.
    pub key: String,
    /// What Copy value copies: the whole value.
    pub copy: String,
    /// Whether the value is a link, which Follow link follows.
    pub link: bool,
    /// The panel the row is in, where the toast of a copy goes.
    pub around: Rect,
}

impl ValueTarget {
    /// What Copy key and value copies: `Key: value`.
    pub fn key_and_value(&self) -> String {
        key_and_value(&self.key, &self.copy)
    }
}

/// What Copy key and value copies for `key` and `value`: `Key: value`.
pub fn key_and_value(key: &str, value: &str) -> String {
    format!("{key}: {value}")
}

/// The row whose value menu is open this frame, if any. The panel that owns the rows draws the
/// menu with its own node actions (see `context_menu::attach_value`): the rows themselves know
/// nothing of the app state. Taking it clears it.
pub fn take_context(ctx: &Context) -> Option<ValueTarget> {
    ctx.data_mut(|d| {
        let target = d.get_temp::<ValueTarget>(context_slot());
        d.remove::<ValueTarget>(context_slot());
        target
    })
}

/// The id of the value menu of the row `row`.
pub fn menu_id(row: Id) -> Id {
    row.with("value-menu")
}

/// Where the value menu of `row` opens, and whether it was opened from the keyboard (its first
/// item then takes the focus).
#[derive(Debug, Clone, Copy)]
pub struct MenuAnchor {
    /// The menu's top-left corner.
    pub at: Pos2,
    /// Opened with `⇧F10` or the context-menu key.
    pub keyboard: bool,
}

fn anchor_slot(row: Id) -> Id {
    menu_id(row).with("anchor")
}

/// Opens the value menu of `row` at `at`.
fn open_menu(ctx: &Context, row: Id, anchor: MenuAnchor) {
    ctx.data_mut(|d| d.insert_temp(anchor_slot(row), anchor));
    Popup::open_id(ctx, menu_id(row));
}

/// Where the open value menu of `row` sits. Taking the keyboard flag clears it, so the first
/// item takes the focus once.
pub fn menu_anchor(ctx: &Context, row: Id) -> Option<MenuAnchor> {
    ctx.data_mut(|d| {
        let anchor = d.get_temp::<MenuAnchor>(anchor_slot(row))?;
        d.insert_temp(
            anchor_slot(row),
            MenuAnchor {
                keyboard: false,
                ..anchor
            },
        );
        Some(anchor)
    })
}

fn follow_slot(row: Id) -> Id {
    row.with("follow-requested")
}

/// Asks the row `row` to report its link as followed on the next frame: Follow link in its menu.
pub fn request_follow(ctx: &Context, row: Id) {
    ctx.data_mut(|d| d.insert_temp(follow_slot(row), true));
    ctx.request_repaint();
}

/// Copies `text` for the row `row`: the clipboard, the check on its icon and the toast at the
/// foot of `around`.
pub fn copy(ctx: &Context, row: Id, text: &str, around: Rect) {
    ctx.copy_text(text.to_owned());
    let now = ctx.input(|i| i.time);
    ctx.data_mut(|d| d.insert_temp(row.with("copied"), now + COPIED_FOR));
    ctx.request_repaint_after(std::time::Duration::from_secs_f64(COPIED_FOR));
    toast::copied(ctx, text, around);
}

/// The text of a value as a flash key: it changes when the value does.
fn flash_key(value: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    value.hash(&mut hasher);
    hasher.finish()
}

/// What the keyboard asked of a focused row this frame.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct Keys {
    copy: bool,
    copy_both: bool,
    menu: bool,
}

impl Keys {
    fn read(ui: &Ui) -> Self {
        ui.input(|i| {
            // The modifiers of the key event itself: the input's own may have changed again
            // within the same frame.
            let pressed = |key: Key, held: fn(Modifiers) -> bool| {
                i.events.iter().any(|event| {
                    matches!(event, Event::Key { key: k, pressed: true, modifiers, .. }
                        if *k == key && held(*modifiers))
                })
            };
            Self {
                // macOS turns ⌘C into a copy event; a bare key press is what tests send.
                copy: i.events.iter().any(|e| matches!(e, Event::Copy))
                    || pressed(Key::C, |m| m.command_only()),
                copy_both: pressed(Key::C, |m| m.command && m.shift && !m.alt),
                menu: pressed(Key::F10, |m| m.shift_only()) || pressed(Key::Menu, |_| true),
            }
        })
    }
}

/// The accessible name of a row: `Key: value`, the value cut after [`NAME_CHARS`] characters.
/// A row without a key (the second of a list under one key) is named by its value alone.
fn row_name(key: &str, value: &str) -> String {
    let value = if value.chars().count() <= NAME_CHARS {
        value.to_owned()
    } else {
        let mut cut: String = value.chars().take(NAME_CHARS).collect();
        cut.push('…');
        cut
    };
    if key.is_empty() {
        value
    } else {
        key_and_value(key, &value)
    }
}

/// What the user did with a [`PropRow`].
#[derive(Debug)]
pub struct PropOutput {
    /// The response of the value: of its text for a link, of its column otherwise. Tooltips
    /// about the value go on it.
    pub value: Response,
    /// The value was copied this frame, with the icon or the keyboard.
    pub copied: bool,
    /// The link was followed: a click on it, `Enter` on the row, or Follow link in its menu.
    pub followed: bool,
}

/// A property row.
#[derive(Debug, Clone)]
#[must_use = "call `show`"]
pub struct PropRow<'a> {
    key: &'a str,
    value: &'a str,
    copy: Option<&'a str>,
    link: bool,
    warn: bool,
    good: bool,
    swatch: Option<Color32>,
    trailing: Option<(Icon, Color32)>,
    value_size: f32,
}

impl<'a> PropRow<'a> {
    /// A row showing `value` under `key`.
    pub const fn new(key: &'a str, value: &'a str) -> Self {
        Self {
            key,
            value,
            copy: None,
            link: false,
            warn: false,
            good: false,
            swatch: None,
            trailing: None,
            value_size: tokens::mono::BODY,
        }
    }

    /// Copies `text` instead of the shown value.
    pub const fn copy_text(mut self, text: &'a str) -> Self {
        self.copy = Some(text);
        self
    }

    /// Styles the value as a link: accent colour, clickable, followed with `Enter`.
    pub const fn link(mut self, link: bool) -> Self {
        self.link = link;
        self
    }

    /// Shows the value in the warning colour.
    pub const fn warn(mut self, warn: bool) -> Self {
        self.warn = warn;
        self
    }

    /// Shows the value in the success colour.
    pub const fn good(mut self, good: bool) -> Self {
        self.good = good;
        self
    }

    /// A colour swatch in front of the value.
    pub const fn swatch(mut self, color: Color32) -> Self {
        self.swatch = Some(color);
        self
    }

    /// An icon at the right end of the value, e.g. a check that confirms it.
    pub const fn trailing(mut self, icon: Icon, color: Color32) -> Self {
        self.trailing = Some((icon, color));
        self
    }

    /// The size of the value's mono text, [`tokens::mono::BODY`] unless set.
    pub const fn value_size(mut self, size: f32) -> Self {
        self.value_size = size;
        self
    }

    /// Lays the row out across the available width.
    #[expect(clippy::too_many_lines, reason = "one row, painted top to bottom")]
    pub fn show(self, ui: &mut Ui) -> PropOutput {
        let width = ui.available_width();
        let lead = if self.swatch.is_some() { SWATCH + 6.0 } else { 0.0 };
        let tail = if self.trailing.is_some() { TRAILING } else { 0.0 };
        let value_w = (width - KEY_W - ICON_BOX - PAD * 2.0 - lead - tail).max(40.0);
        let font = fonts::mono(self.value_size);
        // The id the row is about to get: whether its value was unfolded is kept under it.
        let id = ui.next_auto_id();
        let unfolded = long_value::is_unfolded(ui.ctx(), id);
        // An array or a string that needs more than one line wraps with a hanging indent and
        // folds; any other value wraps as text.
        let long: Option<Wrapped> = (long_value::kind_of(self.value) != long_value::Kind::Plain)
            .then(|| long_value::wrap_in(ui, self.value, &font, value_w, unfolded))
            .filter(|wrapped| wrapped.line_count() > 1);
        // Wrapped height, measured up front so the hover rect is known before painting.
        let galley = ui
            .painter()
            .layout(self.value.to_owned(), font.clone(), Color32::WHITE, value_w);
        // The first line of the value is what the key, the swatch and the copy icon line up
        // with. It is centred in the first MIN_H band, wrapped or not, so it sits at the same
        // height in every row; the lines that wrap go below it with the same room at the foot.
        let line_h = galley.rows.first().map_or(MIN_H, |row| row.height());
        let top = ((MIN_H - line_h) / 2.0).max(0.0);
        let text_h = long.as_ref().map_or(galley.size().y, |wrapped| {
            line_h * f32_from_usize(wrapped.lines.len())
                + if wrapped.fold.is_some() {
                    long_value::FOLD_LINE_H
                } else {
                    0.0
                }
        });
        let text_w = long.as_ref().map_or(galley.size().x.min(value_w), |_| value_w);
        let height = (text_h + 2.0 * top).max(MIN_H);
        let first_y = top + line_h / 2.0;

        // The row is the keyboard's stop: Tab lands on it, not on its icon.
        let (rect, row) = ui.allocate_exact_size(vec2(width, height), Sense::click());
        debug_assert_eq!(row.id, id, "the row took the id it was measured under");
        let role = if self.link { Role::Link } else { Role::ListItem };
        let name = row_name(self.key, self.value);
        row.widget_info(|| WidgetInfo::labeled(role, true, name.clone()));
        let focused = row.has_focus();
        // While its menu is open, the row keeps the look of the one the menu is about.
        let menu_open = Popup::is_id_open(ui.ctx(), menu_id(id));
        let hovered = row.hovered() || ui.rect_contains_pointer(rect) || menu_open;
        let now = ui.input(|i| i.time);
        let copied_until: f64 = ui.data(|d| d.get_temp(id.with("copied"))).unwrap_or(0.0);
        let flashing = now < copied_until;
        if flashing {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_secs_f64(copied_until - now));
        }

        // A value that changes under the same row flashes surface-3 for 400 ms; the first time
        // a row is drawn is quiet.
        let flash = motion::flash(ui.ctx(), id.with("flash"), flash_key(self.value), motion::FLASH);
        if flash > 0.0 {
            ui.painter()
                .rect_filled(rect, 0.0, tokens::SURFACE_3.gamma_multiply(flash));
        }
        // A focused row looks like a hovered one, plus the ring.
        if hovered || focused {
            ui.painter()
                .rect_filled(rect, tokens::RADIUS_SM, tokens::NEUTRAL_HOVER);
        }
        focus::row_ring(ui, &row, rect, tokens::RADIUS_SM);

        let key_rect = Rect::from_min_size(rect.min + vec2(PAD, top), vec2(KEY_W - 4.0, line_h));
        put_label(
            ui,
            key_rect,
            Label::new(RichText::new(self.key).color(tokens::TEXT_3)).truncate(),
        );
        if let Some(text) = prop_hints::hint(self.key) {
            key_hint(ui, id, key_rect, self.key, text);
        }

        let value_rect = Rect::from_min_size(rect.min + vec2(VALUE_X + lead, 0.0), vec2(value_w, height));
        // What the value's text covers: a link is only a link there, not across the column.
        let text_rect = Rect::from_min_size(value_rect.min + vec2(0.0, top), vec2(text_w, text_h));
        if let Some(color) = self.swatch {
            let at = egui::pos2(rect.min.x + VALUE_X, rect.min.y + first_y);
            let chip = Rect::from_center_size(at + vec2(SWATCH / 2.0, 0.0), Vec2::splat(SWATCH));
            ui.painter().rect_filled(chip, 3.0, color);
            ui.painter().rect_stroke(
                chip,
                3.0,
                egui::Stroke::new(1.0, tokens::BORDER_STRONG),
                egui::StrokeKind::Outside,
            );
        }
        if let Some((icon, color)) = self.trailing {
            let slot = Rect::from_center_size(
                egui::pos2(
                    value_rect.min.x + galley.size().x.min(value_w) + 4.0 + TRAILING / 2.0,
                    rect.min.y + first_y,
                ),
                Vec2::splat(TRAILING),
            );
            icon.paint(ui, slot, 13.0, color);
        }
        // A link lights up under the pointer, so it reads as one before it is clicked.
        let lit = self.link && ui.rect_contains_pointer(text_rect);
        let ink = if self.warn {
            tokens::WARN
        } else if self.good {
            tokens::OK
        } else if lit {
            tokens::PRIMARY_TOP
        } else if self.link {
            tokens::ACCENT
        } else {
            tokens::TEXT
        };
        if let Some(wrapped) = &long {
            long_lines(ui, id, text_rect, line_h, self.value, wrapped, &font, ink);
        } else {
            put_label(
                ui,
                value_rect,
                Label::new(RichText::new(self.value).font(font).color(ink)).wrap(),
            );
        }
        // A link is a pointer target over its text only, not across the column; the keyboard
        // reaches it through the row.
        let value = if self.link {
            ui.interact(text_rect, id.with("value-text"), Sense::CLICK)
                .on_hover_cursor(egui::CursorIcon::PointingHand)
        } else {
            ui.interact(value_rect, id.with("value"), Sense::hover())
        };
        if long.is_some() {
            // The lines are painted: the column carries the value for assistive tech.
            let text = self.value;
            value.widget_info(|| WidgetInfo::labeled(Role::Label, true, text));
        }

        let copy_text = self.copy.unwrap_or(self.value);
        let around = ui.clip_rect();
        let keys = if focused { Keys::read(ui) } else { Keys::default() };
        let menu_key = keys.menu;
        let mut copied = false;
        if keys.copy_both {
            copy(ui.ctx(), id, &key_and_value(self.key, copy_text), around);
            copied = true;
        } else if keys.copy {
            copy(ui.ctx(), id, copy_text, around);
            copied = true;
        }

        // The value menu: a right-click anywhere on the row, or the keyboard.
        if row.secondary_clicked() || value.secondary_clicked() {
            let at = ui.ctx().pointer_interact_pos().unwrap_or(text_rect.left_bottom());
            open_menu(ui.ctx(), id, MenuAnchor { at, keyboard: false });
        } else if menu_key {
            let at = egui::pos2(text_rect.min.x, rect.min.y + first_y + line_h / 2.0 + 2.0);
            open_menu(ui.ctx(), id, MenuAnchor { at, keyboard: true });
        }
        if Popup::is_id_open(ui.ctx(), menu_id(id)) {
            let target = ValueTarget {
                row: row.clone(),
                key: self.key.to_owned(),
                copy: copy_text.to_owned(),
                link: self.link,
                around,
            };
            ui.data_mut(|d| d.insert_temp(context_slot(), target));
        }

        if (hovered || focused || flashing) && copy_button(ui, id, rect, first_y, self.key, flashing) {
            copy(ui.ctx(), id, copy_text, around);
            copied = true;
        }

        // Enter on the row (egui's click from the keyboard or from assistive tech), a click on
        // the link's text, or Follow link in the menu on the frame before.
        let from_keyboard = row.clicked() && !row.clicked_by(PointerButton::Primary);
        let requested = ui
            .data_mut(|d| d.remove_temp::<bool>(follow_slot(id)))
            .unwrap_or(false);
        let followed = self.link && (value.clicked() || from_keyboard || requested);
        PropOutput {
            value,
            copied,
            followed,
        }
    }
}

/// Paints the lines of a long value inside `text` (one `line_h` each, the lines after the first
/// indented under the first item), then the fold line when part of it is folded.
#[expect(clippy::too_many_arguments, reason = "the pieces of one row's value")]
fn long_lines(
    ui: &mut Ui,
    id: Id,
    text: Rect,
    line_h: f32,
    value: &str,
    wrapped: &Wrapped,
    font: &egui::FontId,
    ink: Color32,
) {
    let mut y = text.min.y;
    for (i, line) in wrapped.lines.iter().enumerate() {
        let x = text.min.x + if i == 0 { 0.0 } else { wrapped.indent };
        ui.painter().text(
            egui::pos2(x, y),
            egui::Align2::LEFT_TOP,
            &value[line.clone()],
            font.clone(),
            ink,
        );
        y += line_h;
    }
    if let Some(fold) = &wrapped.fold {
        let left = text.min.x + wrapped.indent;
        let unfold = long_value::fold_line(
            ui,
            id,
            left,
            y + long_value::FOLD_LINE_H / 2.0,
            fold,
            font.clone(),
            ink,
        );
        // Unfolded from the keyboard, the button goes: the row keeps the focus.
        if unfold.clicked() && unfold.has_focus() {
            ui.memory_mut(|m| m.request_focus(id));
        }
    }
}

/// A key with a hint: a dotted underline, and the hint after the pointer rests on the key.
fn key_hint(ui: &Ui, id: Id, key_rect: Rect, key: &str, text: &str) {
    let width = ui
        .painter()
        .layout_no_wrap(
            key.to_owned(),
            egui::TextStyle::Body.resolve(ui.style()),
            Color32::WHITE,
        )
        .size()
        .x
        .min(key_rect.width());
    let y = key_rect.center().y + 8.0;
    let mut x = key_rect.min.x + 1.0;
    while x < key_rect.min.x + width {
        ui.painter().circle_filled(egui::pos2(x, y), 0.6, tokens::TEXT_3);
        x += 3.0;
    }
    let hit = Rect::from_min_size(key_rect.min, vec2(width, key_rect.height()));
    // Over the key's own label, which is what assistive tech reads; the hint is its tooltip.
    let response = ui.interact(hit, id.with("key-hint"), Sense::hover());
    tooltip::named(response.on_hover_cursor(egui::CursorIcon::Help), key, text, None);
}

fn put_label(ui: &mut Ui, rect: Rect, label: Label) {
    ui.scope_builder(
        UiBuilder::new()
            .max_rect(rect)
            .layout(Layout::left_to_right(Align::Center)),
        |ui| {
            ui.add(label);
        },
    );
}

/// The copy icon at the right end of the row, a pointer target only: the keyboard copies from
/// the row. Returns true when it was clicked.
fn copy_button(ui: &mut Ui, id: Id, row: Rect, first_y: f32, key: &str, flashing: bool) -> bool {
    let rect = Rect::from_center_size(
        egui::pos2(row.max.x - PAD - ICON_BOX / 2.0, row.min.y + first_y),
        Vec2::splat(ICON_BOX),
    );
    let response = ui.interact(rect, id.with("copy"), Sense::CLICK);
    // The name changes with the icon, so the state can be read without seeing the pixels.
    let name = if flashing {
        format!("Copied {key}")
    } else {
        format!("Copy {key}")
    };
    response.widget_info(|| WidgetInfo::labeled(Role::Button, true, name.clone()));
    if response.hovered() && !flashing {
        ui.painter()
            .rect_filled(rect, tokens::RADIUS_SM, tokens::NEUTRAL_ACTIVE);
    }
    let (icon, ink) = if flashing {
        (Icon::Check, tokens::OK)
    } else if response.hovered() {
        (Icon::Copy, tokens::TEXT)
    } else {
        (Icon::Copy, tokens::TEXT_3)
    };
    icon.paint(ui, rect, 13.0, ink);
    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
    response.clicked()
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn a_row_is_named_by_its_key_and_value_and_a_long_value_is_cut() {
        assert_eq!(row_name("Parent", "3 0 R"), "Parent: 3 0 R");
        assert_eq!(row_name("", "op #53 S"), "op #53 S");
        let long = "9".repeat(200);
        let name = row_name("/Widths", &long);
        assert_eq!(name.chars().count(), "/Widths: ".len() + NAME_CHARS + 1);
        // Smoke: the copy is never cut.
        assert_eq!(key_and_value("/Widths", &long).len(), "/Widths: ".len() + 200);
    }
}
