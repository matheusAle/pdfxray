//! One property: a dim key, a mono value and a copy icon that only shows while the row is
//! hovered. Copying flips the icon to a check for [`COPIED_FOR`] seconds.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use egui::{
    Align, Color32, Id, Label, Layout, Rect, Response, RichText, Role, Sense, Ui, UiBuilder, Vec2,
    WidgetInfo, vec2,
};

use super::icon::Icon;
use super::{prop_hints, tooltip};
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

/// Where a row leaves its value while it is right-clicked or its menu is open.
fn context_slot() -> Id {
    Id::unique("pdfxray-prop-row-context")
}

/// The value of the row that was right-clicked this frame, or whose context menu is open. The
/// panel that owns the rows attaches the node's menu to it (see `context_menu::attach`): the
/// rows themselves know nothing of the app state. Taking it clears it.
pub fn take_context(ctx: &egui::Context) -> Option<Response> {
    ctx.data_mut(|d| {
        let response = d.get_temp::<Response>(context_slot());
        d.remove::<Response>(context_slot());
        response
    })
}

/// The text of a value as a flash key: it changes when the value does.
fn flash_key(value: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    value.hash(&mut hasher);
    hasher.finish()
}

/// What the user did with a [`PropRow`].
#[derive(Debug)]
pub struct PropOutput {
    /// The response of the value. It senses clicks when the row is a link or `clickable`.
    pub value: Response,
    /// The copy icon was clicked this frame.
    pub copied: bool,
}

/// A property row.
#[derive(Debug, Clone)]
#[must_use = "call `show`"]
pub struct PropRow<'a> {
    key: &'a str,
    value: &'a str,
    copy: Option<&'a str>,
    link: bool,
    clickable: bool,
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
            clickable: false,
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

    /// Styles the value as a link: accent colour, clickable.
    pub const fn link(mut self, link: bool) -> Self {
        self.link = link;
        self
    }

    /// Makes the value respond to clicks without the link styling, e.g. to open a popover.
    pub const fn clickable(mut self, clickable: bool) -> Self {
        self.clickable = clickable;
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
    pub fn show(self, ui: &mut Ui) -> PropOutput {
        let width = ui.available_width();
        let lead = if self.swatch.is_some() { SWATCH + 6.0 } else { 0.0 };
        let tail = if self.trailing.is_some() { TRAILING } else { 0.0 };
        let value_w = (width - KEY_W - ICON_BOX - PAD * 2.0 - lead - tail).max(40.0);
        let font = fonts::mono(self.value_size);
        // Wrapped height, measured up front so the hover rect is known before painting.
        let galley = ui
            .painter()
            .layout(self.value.to_owned(), font.clone(), Color32::WHITE, value_w);
        // The first line of the value is what the key, the swatch and the copy icon line up
        // with. It is centred in the first MIN_H band, wrapped or not, so it sits at the same
        // height in every row; the lines that wrap go below it with the same room at the foot.
        let line_h = galley.rows.first().map_or(MIN_H, |row| row.height());
        let top = ((MIN_H - line_h) / 2.0).max(0.0);
        let height = (galley.size().y + 2.0 * top).max(MIN_H);
        let first_y = top + line_h / 2.0;

        let (rect, row) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
        let hovered = row.hovered() || ui.rect_contains_pointer(rect);
        let id = row.id;
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
        if hovered {
            ui.painter()
                .rect_filled(rect, tokens::RADIUS_SM, tokens::NEUTRAL_HOVER);
        }

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
        let text_rect = Rect::from_min_size(
            value_rect.min + vec2(0.0, top),
            vec2(galley.size().x.min(value_w), galley.size().y),
        );
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
        // The last thing placed in the row: placing a label moves the cursor of `ui` to the foot
        // of its rectangle, so this is the whole row, and the next row starts right below it.
        put_label(
            ui,
            value_rect,
            Label::new(RichText::new(self.value).font(font).color(ink)).wrap(),
        );
        // Every value senses clicks across its column, so a right-click reaches it. Only links
        // and `clickable` values are announced as controls and get the hand, and only over
        // their text: the rest of the column is not a target.
        let activatable = self.link || self.clickable;
        let wide = ui.interact(value_rect, id.with("value"), Sense::CLICK);
        let value = if activatable {
            ui.interact(text_rect, id.with("value-text"), Sense::CLICK)
        } else {
            wide.clone()
        };
        if self.link {
            let text = self.value;
            value.widget_info(|| WidgetInfo::labeled(Role::Link, true, text));
        } else if self.clickable {
            let text = self.value;
            value.widget_info(|| WidgetInfo::labeled(Role::Button, true, text));
        }
        let value = if activatable {
            value.on_hover_cursor(egui::CursorIcon::PointingHand)
        } else {
            value
        };
        if let Some(menu) = [&value, &wide]
            .into_iter()
            .find(|r| r.secondary_clicked() || r.context_menu_opened())
        {
            ui.data_mut(|d| d.insert_temp(context_slot(), menu.clone()));
        }

        let copied = (hovered || flashing)
            && copy_button(
                ui,
                id,
                rect,
                first_y,
                self.key,
                self.copy.unwrap_or(self.value),
                flashing,
            );
        if copied {
            ui.data_mut(|d| d.insert_temp(id.with("copied"), now + COPIED_FOR));
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_secs_f64(COPIED_FOR));
        }
        PropOutput { value, copied }
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

/// The copy icon at the right end of the row. Returns true when it was clicked.
fn copy_button(ui: &mut Ui, id: Id, row: Rect, first_y: f32, key: &str, text: &str, flashing: bool) -> bool {
    let rect = Rect::from_center_size(
        egui::pos2(row.max.x - PAD - ICON_BOX / 2.0, row.min.y + first_y),
        Vec2::splat(ICON_BOX),
    );
    let response = ui.interact(rect, id.with("copy"), Sense::click());
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
    let clicked = response.clicked();
    if clicked {
        ui.ctx().copy_text(text.to_owned());
    }
    response.on_hover_cursor(egui::CursorIcon::PointingHand);
    clicked
}
