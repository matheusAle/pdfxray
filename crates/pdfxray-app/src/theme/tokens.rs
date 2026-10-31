//! Design tokens, exactly as in "Navigation & micro-interactions" §4 of the design.
//!
//! Colours are `const` so painting code never builds them per frame; sizes are in points.

use egui::Color32;
use pdfxray_core::NodeId;

/// Window background (`#0B0D10`). Chrome that has its own tone below uses that tone instead.
pub const BG: Color32 = Color32::from_rgb(0x0B, 0x0D, 0x10);
/// The classic panel surface (`#121519`). Areas now have their own tone, see [`PANEL`] and the
/// other surface tones; this one stays for cards and wells that sit one step above a panel.
pub const SURFACE: Color32 = Color32::from_rgb(0x12, 0x15, 0x19);
/// Hovered rows, input fields, chips.
pub const SURFACE_2: Color32 = Color32::from_rgb(0x18, 0x1C, 0x21);
/// Pressed or selected controls, icon wells.
pub const SURFACE_3: Color32 = Color32::from_rgb(0x20, 0x25, 0x2C);
/// Hairlines between regions.
pub const BORDER: Color32 = Color32::from_rgb(0x25, 0x2A, 0x31);
/// Outlines of controls and popovers.
pub const BORDER_STRONG: Color32 = Color32::from_rgb(0x32, 0x38, 0x42);
/// Primary text.
pub const TEXT: Color32 = Color32::from_rgb(0xE7, 0xEA, 0xEE);
/// Secondary text: labels of controls.
pub const TEXT_2: Color32 = Color32::from_rgb(0xA2, 0xAA, 0xB6);
/// Tertiary text: keys, ids, hints.
pub const TEXT_3: Color32 = Color32::from_rgb(0x6B, 0x73, 0x80);
// ---- Surface tones (premium pass): areas separate by tone, not by lines. ----

/// The canvas area, the darkest tone: pages and the grid sit on it.
pub const CANVAS: Color32 = Color32::from_rgb(0x0A, 0x0C, 0x0F);
/// Side panels: the navigator and the inspector.
pub const PANEL: Color32 = Color32::from_rgb(0x10, 0x13, 0x17);
/// The toolbar above the canvas.
pub const TOOLBAR_BG: Color32 = Color32::from_rgb(0x0C, 0x0F, 0x12);
/// The drawer under the canvas.
pub const DRAWER_BG: Color32 = Color32::from_rgb(0x0E, 0x11, 0x15);
/// The activity rail.
pub const RAIL_BG: Color32 = Color32::from_rgb(0x0B, 0x0D, 0x10);
/// The status bar.
pub const STATUS_BG: Color32 = Color32::from_rgb(0x0B, 0x0D, 0x10);
/// The title bar, `#0F1216` at 85 %. The blur behind it is not available in egui: it is a
/// translucent fill.
pub const TITLE_BG: Color32 = Color32::from_rgba_unmultiplied_const(0x0F, 0x12, 0x16, 0xD9);

/// Fill of an active tool, the active rail item and the selected segment: `#FFFFFF14`. The
/// accent is for the selection and the primary action only.
pub const NEUTRAL_ACTIVE: Color32 = Color32::from_rgba_unmultiplied_const(0xFF, 0xFF, 0xFF, 0x14);
/// Ink of an active (neutral) tool, segment or rail item: white.
pub const ACTIVE_INK: Color32 = Color32::WHITE;
/// Fill of a hovered control or row: `#FFFFFF0A`.
pub const NEUTRAL_HOVER: Color32 = Color32::from_rgba_unmultiplied_const(0xFF, 0xFF, 0xFF, 0x0A);
/// The only line left between areas, e.g. under the title bar: `#FFFFFF0A`.
pub const HAIRLINE: Color32 = Color32::from_rgba_unmultiplied_const(0xFF, 0xFF, 0xFF, 0x0A);
/// Outline of popovers, menus, tooltips and floating cards: `#FFFFFF17`.
pub const POPOVER_HAIRLINE: Color32 = Color32::from_rgba_unmultiplied_const(0xFF, 0xFF, 0xFF, 0x17);
/// Fill of popovers and menus: `#1A1E24` at 97 %. The design has 92 % over a blur;
/// egui cannot blur, and without it the page and the inspector read through the panel.
pub const POPOVER_FILL: Color32 = Color32::from_rgba_unmultiplied_const(0x1A, 0x1E, 0x24, 0xF7);
/// Fill of tooltips: `#1A1E24`, opaque. A tooltip is small and rests on top of dense rows, where
/// even 3 % of their text would read through it.
pub const TOOLTIP_FILL: Color32 = Color32::from_rgb(0x1A, 0x1E, 0x24);
/// Fill of floating canvas cards: `#12161B` at 90 %.
pub const CARD_FILL: Color32 = Color32::from_rgba_unmultiplied_const(0x12, 0x16, 0x1B, 0xE6);

/// The accent: the current selection, the primary action and links. Active tools are neutral.
pub const ACCENT: Color32 = Color32::from_rgb(0x3D, 0xD6, 0xF5);
/// Accent at 12 % opacity (`#3DD6F51F`): fill of selected rows.
pub const ACCENT_DIM: Color32 = Color32::from_rgba_unmultiplied_const(0x3D, 0xD6, 0xF5, 0x1F);
/// Top of the primary button's gradient: `#6FE3F9`.
pub const PRIMARY_TOP: Color32 = Color32::from_rgb(0x6F, 0xE3, 0xF9);
/// Bottom of the primary button's gradient: `#2CC6E8`.
pub const PRIMARY_BOTTOM: Color32 = Color32::from_rgb(0x2C, 0xC6, 0xE8);
/// Ink on the accent: label and icon of the primary button.
pub const ON_ACCENT: Color32 = Color32::from_rgb(0x05, 0x21, 0x2A);

/// Structure elements.
pub const KIND_TAG: Color32 = Color32::from_rgb(0xB4, 0x8C, 0xFF);
/// Content-stream nodes.
pub const KIND_CONTENT: Color32 = Color32::from_rgb(0x5A, 0xA9, 0xFF);
/// Artifacts.
pub const KIND_ARTIFACT: Color32 = Color32::from_rgb(0x8A, 0x93, 0xA3);
/// Annotations.
pub const KIND_ANNOT: Color32 = Color32::from_rgb(0xFF, 0xB5, 0x47);
/// Tag role: headings (`H1`..`H6`).
pub const ROLE_HEADING: Color32 = Color32::from_rgb(0xB4, 0x8C, 0xFF);
/// Tag role: paragraphs.
pub const ROLE_P: Color32 = Color32::from_rgb(0x5A, 0xA9, 0xFF);
/// Tag role: figures.
pub const ROLE_FIGURE: Color32 = Color32::from_rgb(0xFF, 0x8A, 0x4C);
/// Tag role: captions.
pub const ROLE_CAPTION: Color32 = Color32::from_rgb(0xF4, 0x72, 0xB6);
/// Tag role: tables.
pub const ROLE_TABLE: Color32 = Color32::from_rgb(0x2D, 0xD4, 0xBF);
/// Tag role: lists.
pub const ROLE_LIST: Color32 = Color32::from_rgb(0xA3, 0xE6, 0x35);
/// Tag role: links.
pub const ROLE_LINK: Color32 = Color32::from_rgb(0xFA, 0xCC, 0x15);
/// Tag role: inline elements (`Span`, `Quote`, ...).
pub const ROLE_INLINE: Color32 = Color32::from_rgb(0x94, 0xA3, 0xB8);
/// Tag role: grouping elements (`Sect`, `Div`, ...).
pub const ROLE_GROUP: Color32 = Color32::from_rgb(0xCB, 0xD5, 0xE1);
/// Tag role: a type that never reaches a standard one.
pub const ROLE_OTHER: Color32 = TEXT_3;
/// Success.
pub const OK: Color32 = Color32::from_rgb(0x4A, 0xDE, 0x80);
/// Warning.
pub const WARN: Color32 = Color32::from_rgb(0xFB, 0xBF, 0x24);
/// Error.
pub const ERR: Color32 = Color32::from_rgb(0xF8, 0x71, 0x71);
/// The second selection (shift-click) and the measure tool.
pub const PINK: Color32 = Color32::from_rgb(0xFF, 0x50, 0xC8);

// ---- Object preview cards ("2.9 Objects · Preview cards"). ----

/// Fill of a preview card: `#0E1115`, the drawer's tone.
pub const PREVIEW_CARD: Color32 = DRAWER_BG;
/// Outline of a preview card: `#FFFFFF0F`.
pub const PREVIEW_CARD_HAIRLINE: Color32 = Color32::from_rgba_unmultiplied_const(0xFF, 0xFF, 0xFF, 0x0F);
/// The canvas of pictures, swatches and shadings, and the dark squares of the checkerboard behind
/// pictures: `#101317`, the panel tone.
pub const CHECKER_DARK: Color32 = PANEL;
/// The light squares of the checkerboard behind pictures: `#151A20`.
pub const CHECKER_LIGHT: Color32 = Color32::from_rgb(0x15, 0x1A, 0x20);
/// Hairline around the pixels of an image: `#FFFFFF40`.
pub const PIXEL_BOUNDS: Color32 = Color32::from_rgba_unmultiplied_const(0xFF, 0xFF, 0xFF, 0x40);
/// The canvas of a font, without a white box: `#0B0D10`.
pub const FONT_CANVAS: Color32 = BG;
/// Ink of a font specimen, its glyphs and the stops of a gradient: `#F2F4F7`.
pub const SPECIMEN_INK: Color32 = Color32::from_rgb(0xF2, 0xF4, 0xF7);
/// Ink of a specimen drawn with a fallback font, never the file's own: text-2 at 40 %,
/// `#A2AAB666`.
pub const FALLBACK_INK: Color32 = Color32::from_rgba_unmultiplied_const(0xA2, 0xAA, 0xB6, 0x66);
/// Fill of a glyph cell: `#FFFFFF05`.
pub const GLYPH_CELL: Color32 = Color32::from_rgba_unmultiplied_const(0xFF, 0xFF, 0xFF, 0x05);
/// Outline of a glyph cell: `#FFFFFF14`.
pub const GLYPH_CELL_HAIRLINE: Color32 = NEUTRAL_ACTIVE;
/// Fill of the frame of a font that is not embedded: the warning at 4 %, `#FBBF240A`.
pub const WARN_FRAME: Color32 = Color32::from_rgba_unmultiplied_const(0xFB, 0xBF, 0x24, 0x0A);
/// Outline of that frame: the warning at 35 %, `#FBBF2459`.
pub const WARN_FRAME_HAIRLINE: Color32 = Color32::from_rgba_unmultiplied_const(0xFB, 0xBF, 0x24, 0x59);
/// Fill of a fact chip on a card: `#FFFFFF0A`.
pub const FACT_CHIP: Color32 = NEUTRAL_HOVER;
/// Outline of a fact chip: `#FFFFFF12`.
pub const FACT_CHIP_HAIRLINE: Color32 = Color32::from_rgba_unmultiplied_const(0xFF, 0xFF, 0xFF, 0x12);
/// Fill of a warning fact chip: the warning at 10 %, `#FBBF241A`.
pub const WARN_CHIP: Color32 = Color32::from_rgba_unmultiplied_const(0xFB, 0xBF, 0x24, 0x1A);
/// Outline of a warning fact chip: the warning at 25 %, `#FBBF2440`.
pub const WARN_CHIP_HAIRLINE: Color32 = Color32::from_rgba_unmultiplied_const(0xFB, 0xBF, 0x24, 0x40);
/// Outline of a colour swatch and of a gradient bar: `#FFFFFF1A`.
pub const SWATCH_HAIRLINE: Color32 = Color32::from_rgba_unmultiplied_const(0xFF, 0xFF, 0xFF, 0x1A);

/// Corner radius of chips and rows.
pub const RADIUS_SM: f32 = 4.0;
/// Corner radius of controls.
pub const RADIUS_MD: f32 = 6.0;
/// Corner radius of cards and popovers.
pub const RADIUS_LG: f32 = 8.0;
/// Corner radius of a preview card.
pub const RADIUS_XL: f32 = 10.0;

/// Height of tree rows and property rows.
pub const ROW_H: f32 = 28.0;
/// Height of toolbar controls.
pub const CONTROL_H: f32 = 26.0;

/// Width of the activity rail.
pub const RAIL_W: f32 = 52.0;
/// Default width of the navigator.
pub const NAVIGATOR_W: f32 = 300.0;
/// Smallest width the navigator can be dragged to.
pub const NAVIGATOR_MIN_W: f32 = 240.0;
/// Default width of the inspector.
pub const INSPECTOR_W: f32 = 340.0;
/// Smallest width the inspector can be dragged to: room for the key and value columns.
pub const INSPECTOR_MIN_W: f32 = 280.0;
/// Width the canvas keeps while a side panel is dragged wider.
pub const CANVAS_MIN_W: f32 = 360.0;
/// Default height of the drawer.
pub const DRAWER_H: f32 = 196.0;
/// Smallest height the drawer can be dragged to.
pub const DRAWER_MIN_H: f32 = 96.0;
/// Height of the title bar.
pub const TITLE_H: f32 = 44.0;
/// Height of the status bar.
pub const STATUS_H: f32 = 28.0;

/// Letter spacing in points (egui's `extra_letter_spacing`), see [`crate::theme::fonts::tracked`].
pub mod tracking {
    /// Titles: tightened.
    pub const TITLE: f32 = -0.25;
    /// Section headers: opened up.
    pub const SECTION: f32 = 0.9;
}

/// UI text sizes, Inter.
pub mod text {
    /// Section headers: 10 px, semibold, `+0.9` tracking, in text-3.
    pub const SECTION: f32 = 10.0;
    /// Chips in the UI font.
    pub const SMALL: f32 = 11.0;
    /// Body text.
    pub const BODY: f32 = 12.0;
    /// Emphasised body text.
    pub const LARGE: f32 = 13.0;
    /// Panel titles.
    pub const TITLE: f32 = 15.0;
    /// The inspector's title: semibold, wraps.
    pub const INSPECTOR_TITLE: f32 = 17.0;
}

/// Value, id and operator sizes, `JetBrains Mono`.
pub mod mono {
    /// The units switch, the code of a glyph cell.
    pub const TINY: f32 = 9.0;
    /// Captions on a preview canvas: an image's scale, gradient stops.
    pub const CAPTION: f32 = 9.5;
    /// Ids, counts and chips.
    pub const SMALL: f32 = 10.0;
    /// Values and operators.
    pub const BODY: f32 = 11.0;
    /// Values of the rows under a preview card.
    pub const PREVIEW_VALUE: f32 = 11.5;
    /// Large values.
    pub const LARGE: f32 = 12.0;
}

/// The colour a node's kind dot gets in trees and crumbs.
pub fn kind_color(id: &NodeId) -> Color32 {
    match id {
        NodeId::StructElem(_) => KIND_TAG,
        NodeId::MarkedContent { .. } | NodeId::Op { .. } | NodeId::Glyph { .. } => KIND_CONTENT,
        NodeId::Artifact { .. } => KIND_ARTIFACT,
        NodeId::Validation(_) => ERR,
        NodeId::Diagnostic(_) => WARN,
        NodeId::Page(_) | NodeId::PageBox { .. } => OK,
        NodeId::Document
        | NodeId::Trailer
        | NodeId::Group(_)
        | NodeId::Object(_)
        | NodeId::Revision(_)
        | NodeId::Resource { .. } => TEXT_3,
    }
}
