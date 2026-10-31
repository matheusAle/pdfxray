//! Debug overlays built from the captured content of a page: x-ray outlines, the clip in
//! effect, and the transparency heat map.
//!
//! # X-ray palette
//!
//! Colours are straight RGBA and stable, so tests and users can rely on them.
//!
//! [`ColorBy::Kind`]:
//!
//! | painted item                        | colour                      |
//! |-------------------------------------|-----------------------------|
//! | glyph                               | [`KIND_GLYPH`]  blue        |
//! | path (fill, stroke, both)           | [`KIND_PATH`]   green       |
//! | image                               | [`KIND_IMAGE`]  orange      |
//! | shading                             | [`KIND_SHADING`] purple     |
//!
//! [`ColorBy::Artifact`]: [`ARTIFACT`] amber inside any `Artifact` sequence, [`TAGGED`] teal inside
//! a sequence with an `/MCID`, [`UNTAGGED`] blue otherwise.
//!
//! [`ColorBy::Tag`]: the innermost marked-content tag picks one of [`TAG_PALETTE`] by a stable
//! hash of the tag name; [`UNTAGGED_TAG`] grey when the item is outside every sequence.
//!
//! [`ColorBy::TagRole`]: the role class of the structure element that owns the innermost marked
//! content with an `/MCID` picks the colour from [`ROLE_COLORS`] (indexed like
//! [`RoleClass::ALL`]); content inside an `Artifact` sequence takes the artifact class,
//! [`UNTAGGED_TAG`] grey when no element owns it.
//!
//! Items that are painted but not visible get a translucent fill in addition to the outline and
//! override the colours above (first match wins): [`OUTSIDE_CROP`] magenta for an item that does
//! not touch the crop box at all, [`INVISIBLE_TEXT`] red for text rendering mode 3 or 7,
//! [`WHITE_ON_WHITE`] gold for white paint over a white background.

use pdfxray_core::capture::{CapturedPage, ColorInfo, PaintItem, PaintKind};
use pdfxray_core::numeric::{u8_from_f64_round, u64_from_usize, usize_from_u32};
use pdfxray_core::{Document, PageIx, Rect, RoleClass};

use crate::Overlay;

/// Glyph outline for [`ColorBy::Kind`].
pub const KIND_GLYPH: [u8; 4] = [0, 90, 200, 255];
/// Path outline for [`ColorBy::Kind`].
pub const KIND_PATH: [u8; 4] = [0, 160, 70, 255];
/// Image outline for [`ColorBy::Kind`].
pub const KIND_IMAGE: [u8; 4] = [230, 120, 0, 255];
/// Shading outline for [`ColorBy::Kind`].
pub const KIND_SHADING: [u8; 4] = [150, 0, 200, 255];
/// Content inside an `Artifact` sequence for [`ColorBy::Artifact`].
pub const ARTIFACT: [u8; 4] = [230, 140, 0, 255];
/// Content inside a sequence with an `/MCID` for [`ColorBy::Artifact`].
pub const TAGGED: [u8; 4] = [0, 140, 130, 255];
/// Content outside every sequence for [`ColorBy::Artifact`].
pub const UNTAGGED: [u8; 4] = [40, 90, 220, 255];
/// Content outside every sequence for [`ColorBy::Tag`].
pub const UNTAGGED_TAG: [u8; 4] = [128, 128, 128, 255];
/// Colours [`ColorBy::Tag`] picks from, by tag hash.
pub const TAG_PALETTE: [[u8; 4]; 8] = [
    [31, 119, 180, 255],
    [255, 127, 14, 255],
    [44, 160, 44, 255],
    [148, 103, 189, 255],
    [140, 86, 75, 255],
    [227, 119, 194, 255],
    [23, 190, 207, 255],
    [188, 189, 34, 255],
];
/// How many role classes there are, i.e. the length of [`ROLE_COLORS`].
pub const ROLE_CLASSES: usize = RoleClass::ALL.len();
/// One straight RGBA colour per role class, in the order of [`RoleClass::ALL`].
pub type RolePalette = [[u8; 4]; ROLE_CLASSES];
/// The role colours of the design (`13-foundations`): heading, paragraph, figure, caption, table,
/// list, link, inline, group, artifact, other. The app's role tokens are the same colours.
pub const ROLE_COLORS: RolePalette = [
    [0xB4, 0x8C, 0xFF, 255],
    [0x5A, 0xA9, 0xFF, 255],
    [0xFF, 0x8A, 0x4C, 255],
    [0xF4, 0x72, 0xB6, 255],
    [0x2D, 0xD4, 0xBF, 255],
    [0xA3, 0xE6, 0x35, 255],
    [0xFA, 0xCC, 0x15, 255],
    [0x94, 0xA3, 0xB8, 255],
    [0xCB, 0xD5, 0xE1, 255],
    [0x8A, 0x93, 0xA3, 255],
    [0x6B, 0x73, 0x80, 255],
];
/// Items that do not touch the crop box.
pub const OUTSIDE_CROP: [u8; 4] = [230, 0, 230, 255];
/// Text drawn with rendering mode 3 or 7.
pub const INVISIBLE_TEXT: [u8; 4] = [220, 0, 0, 255];
/// White paint over a white background.
pub const WHITE_ON_WHITE: [u8; 4] = [240, 190, 0, 255];

/// Fill alpha of the highlighted (hidden) categories.
const HIDDEN_FILL_ALPHA: u8 = 48;

/// What the x-ray outline colour encodes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ColorBy {
    /// Glyph, path, image or shading.
    #[default]
    Kind,
    /// The innermost marked-content tag.
    Tag,
    /// The role class of the structure element that owns the content (heading, paragraph,
    /// figure ...), in the colours of [`ROLE_COLORS`].
    TagRole,
    /// Artifact, tagged content or untagged content.
    Artifact,
}

/// Options of [`xray_overlays`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct XrayOptions {
    /// What the outline colour encodes.
    pub color_by: ColorBy,
    /// Also outline items that cannot be seen: invisible text, items outside the crop box and
    /// white paint on white. They get their own colours and a translucent fill. When false they
    /// are left out.
    pub include_invisible: bool,
    /// Opacity of every outline and fill, 255 = as the palette says. The app draws the outlines
    /// at 128 and lets the selected and hovered item stand out at full strength.
    pub alpha: u8,
}

impl Default for XrayOptions {
    fn default() -> Self {
        Self {
            color_by: ColorBy::default(),
            include_invisible: false,
            alpha: u8::MAX,
        }
    }
}

/// Why a painted item cannot be seen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Hidden {
    OutsideCrop,
    InvisibleText,
    WhiteOnWhite,
}

impl Hidden {
    const fn color(self) -> [u8; 4] {
        match self {
            Self::OutsideCrop => OUTSIDE_CROP,
            Self::InvisibleText => INVISIBLE_TEXT,
            Self::WhiteOnWhite => WHITE_ON_WHITE,
        }
    }
}

/// One outline per painted item (glyph, path, image, shading) of `page`, in paint order.
///
/// Clip-only items are not painted and get no outline. Empty when the page was not captured.
pub fn xray_overlays(doc: &Document, page: PageIx, options: XrayOptions) -> Vec<Overlay> {
    let (Some(cap), Some(info)) = (doc.captured(page), doc.page(page)) else {
        return Vec::new();
    };
    xray_items(cap, info.crop_box, options, &|mcid| {
        doc.mcid_role_class(page, mcid)
    })
}

/// The role class that owns marked content `mcid` of the page being outlined.
type RoleOf<'a> = &'a dyn Fn(i32) -> Option<RoleClass>;

fn xray_items(cap: &CapturedPage, crop: Rect, options: XrayOptions, role_of: RoleOf<'_>) -> Vec<Overlay> {
    let alpha = options.alpha;
    cap.items
        .iter()
        .enumerate()
        .filter(|(_, item)| !matches!(item.kind, PaintKind::Clip))
        .filter_map(|(at, item)| {
            let hidden = hidden_reason(cap, at, crop);
            if hidden.is_some() && !options.include_invisible {
                return None;
            }
            Some(with_alpha(
                &match hidden {
                    Some(h) => highlight(item.bbox, h.color()),
                    None => outline(item.bbox, normal_color(cap, item, options, role_of)),
                },
                alpha,
            ))
        })
        .collect()
}

/// `overlay` with its stroke and fill opacities scaled by `alpha / 255`.
fn with_alpha(overlay: &Overlay, alpha: u8) -> Overlay {
    let scale = |color: [u8; 4]| {
        let a = u16::from(color[3]) * u16::from(alpha) + 127;
        [
            color[0],
            color[1],
            color[2],
            u8::try_from(a / 255).unwrap_or(u8::MAX),
        ]
    };
    match *overlay {
        Overlay::Rect {
            rect,
            stroke,
            fill,
            width,
        } => Overlay::Rect {
            rect,
            stroke: scale(stroke),
            fill: fill.map(scale),
            width,
        },
    }
}

fn outline(rect: Rect, color: [u8; 4]) -> Overlay {
    Overlay::Rect {
        rect,
        stroke: color,
        fill: None,
        width: 1.0,
    }
}

fn highlight(rect: Rect, color: [u8; 4]) -> Overlay {
    Overlay::Rect {
        rect,
        stroke: color,
        fill: Some([color[0], color[1], color[2], HIDDEN_FILL_ALPHA]),
        width: 1.5,
    }
}

fn normal_color(cap: &CapturedPage, item: &PaintItem, options: XrayOptions, role_of: RoleOf<'_>) -> [u8; 4] {
    let by = options.color_by;
    let innermost = || {
        item.mc_stack
            .last()
            .and_then(|ix| cap.marked.get(usize_from_u32(*ix)))
    };
    let sequences = || {
        item.mc_stack
            .iter()
            .filter_map(|ix| cap.marked.get(usize_from_u32(*ix)))
    };
    match by {
        ColorBy::Kind => match item.kind {
            PaintKind::Glyph { .. } => KIND_GLYPH,
            PaintKind::Image { .. } => KIND_IMAGE,
            PaintKind::Shading => KIND_SHADING,
            _ => KIND_PATH,
        },
        ColorBy::Tag => innermost().map_or(UNTAGGED_TAG, |m| tag_color(&m.tag)),
        ColorBy::TagRole => {
            let class = if sequences().any(|m| m.artifact.is_some()) {
                Some(RoleClass::Artifact)
            } else {
                // Innermost first: the nearest sequence that an element owns.
                item.mc_stack
                    .iter()
                    .rev()
                    .filter_map(|ix| cap.marked.get(usize_from_u32(*ix))?.mcid)
                    .find_map(role_of)
            };
            class.map_or(UNTAGGED_TAG, |class| ROLE_COLORS[role_slot(class)])
        }
        ColorBy::Artifact => {
            if sequences().any(|m| m.artifact.is_some()) {
                ARTIFACT
            } else if sequences().any(|m| m.mcid.is_some()) {
                TAGGED
            } else {
                UNTAGGED
            }
        }
    }
}

/// Position of `class` in [`RoleClass::ALL`], the index into a [`RolePalette`].
pub fn role_slot(class: RoleClass) -> usize {
    RoleClass::ALL
        .iter()
        .position(|c| *c == class)
        .unwrap_or(ROLE_CLASSES - 1)
}

/// The palette entry of `tag`, by FNV-1a so it does not change between runs or versions.
fn tag_color(tag: &str) -> [u8; 4] {
    let hash = tag.bytes().fold(0xcbf2_9ce4_8422_2325_u64, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
    });
    let len = u64_from_usize(TAG_PALETTE.len());
    TAG_PALETTE[usize::try_from(hash % len).unwrap_or(0)]
}

/// Why item `at` cannot be seen, first match of: outside the crop box, invisible text, white on
/// white.
fn hidden_reason(cap: &CapturedPage, at: usize, crop: Rect) -> Option<Hidden> {
    let item = cap.items.get(at)?;
    if !intersects(&item.bbox, &crop) {
        return Some(Hidden::OutsideCrop);
    }
    if matches!(item.kind, PaintKind::Glyph { .. }) && matches!(item.gstate.text.render_mode, 3 | 7) {
        return Some(Hidden::InvisibleText);
    }
    is_white_on_white(cap, at).then_some(Hidden::WhiteOnWhite)
}

fn intersects(a: &Rect, b: &Rect) -> bool {
    a.x0 <= b.x1 && b.x0 <= a.x1 && a.y0 <= b.y1 && b.y0 <= a.y1
}

/// True when the colours the item paints with are all white and no earlier item that paints a
/// non-white colour (or an image or shading) overlaps its box, i.e. the background under it is
/// still the white page. Boxes are coarse, so an item next to a diagonal shape may be missed.
fn is_white_on_white(cap: &CapturedPage, at: usize) -> bool {
    let Some(item) = cap.items.get(at) else {
        return false;
    };
    let g = &item.gstate;
    let mode = g.text.render_mode;
    let paints_white = match item.kind {
        PaintKind::Fill => is_white(&g.fill),
        PaintKind::Stroke => is_white(&g.stroke),
        PaintKind::FillStroke => is_white(&g.fill) && is_white(&g.stroke),
        PaintKind::Glyph { .. } => match mode {
            0 | 4 => is_white(&g.fill),
            1 | 5 => is_white(&g.stroke),
            2 | 6 => is_white(&g.fill) && is_white(&g.stroke),
            _ => false,
        },
        PaintKind::Clip | PaintKind::Image { .. } | PaintKind::Shading => false,
    };
    paints_white
        && !cap.items[..at]
            .iter()
            .any(|under| paints_color_over(under, &item.bbox))
}

/// True when `under` fills in a colour other than white, or is an image or shading, and its box
/// overlaps `bbox`.
fn paints_color_over(under: &PaintItem, bbox: &Rect) -> bool {
    if !intersects(&under.bbox, bbox) {
        return false;
    }
    let g = &under.gstate;
    match under.kind {
        PaintKind::Fill | PaintKind::FillStroke => !is_white(&g.fill),
        PaintKind::Image { .. } | PaintKind::Shading => true,
        PaintKind::Stroke | PaintKind::Glyph { .. } | PaintKind::Clip => false,
    }
}

/// True for white in a device or ICC-based gray, RGB or CMYK space (1, 3 or 4 components).
fn is_white(color: &ColorInfo) -> bool {
    if color.pattern.is_some() || color.space == "Pattern" {
        return false;
    }
    match color.components.as_slice() {
        [g] => *g >= 1.0,
        [r, g, b] => [r, g, b].iter().all(|c| **c >= 1.0),
        [c, m, y, k] => [c, m, y, k].iter().all(|c| **c <= 0.0),
        _ => false,
    }
}

/// Colour of [`clip_overlays`].
pub const CLIP: [u8; 4] = [0, 170, 190, 255];

/// The clip bounding box in effect after operator `op_ix` executed: a teal outline with a faint
/// fill. Empty when no clip is in effect, the operator does not exist or the page was not
/// captured. Clips are tracked as bounding boxes, so a rotated clip is over-approximated.
pub fn clip_overlays(doc: &Document, page: PageIx, op_ix: u32) -> Vec<Overlay> {
    let Some(cap) = doc.captured(page) else {
        return Vec::new();
    };
    cap.ops
        .iter()
        .find(|o| o.ix == op_ix)
        .and_then(|o| o.gstate.clip)
        .map(|rect| Overlay::Rect {
            rect,
            stroke: CLIP,
            fill: Some([CLIP[0], CLIP[1], CLIP[2], 32]),
            width: 2.0,
        })
        .into_iter()
        .collect()
}

/// Soft mask active.
pub const HEAT_SOFT_MASK: [u8; 3] = [30, 60, 220];
/// Non-Normal blend mode.
pub const HEAT_BLEND: [u8; 3] = [200, 0, 200];

/// A tint over every painted item that is not plain opaque: a soft mask is active (blue), a
/// blend mode other than `Normal`/`Compatible` is set (purple), or its alpha is below 1 (yellow
/// for nearly opaque to red for fully transparent, more opaque tint for lower alpha). When
/// several apply the order is soft mask, blend mode, alpha. In paint order.
///
/// The alpha of an item is `ca` for fills, images and shadings, `CA` for strokes and the lower
/// of both for fill-and-stroke items.
pub fn alpha_heatmap_overlays(doc: &Document, page: PageIx) -> Vec<Overlay> {
    let Some(cap) = doc.captured(page) else {
        return Vec::new();
    };
    cap.items
        .iter()
        .filter(|item| !matches!(item.kind, PaintKind::Clip))
        .filter_map(|item| {
            let (rgb, alpha) = heat(item)?;
            Some(Overlay::Rect {
                rect: item.bbox,
                stroke: [rgb[0], rgb[1], rgb[2], 255],
                fill: Some([rgb[0], rgb[1], rgb[2], alpha]),
                width: 1.0,
            })
        })
        .collect()
}

/// Tint colour and fill alpha for a non-plain item.
fn heat(item: &PaintItem) -> Option<([u8; 3], u8)> {
    let g = &item.gstate;
    if g.soft_mask {
        return Some((HEAT_SOFT_MASK, 90));
    }
    if !matches!(g.blend_mode.as_str(), "Normal" | "Compatible") {
        return Some((HEAT_BLEND, 90));
    }
    let alpha = match item.kind {
        PaintKind::Stroke => g.alpha_stroke,
        PaintKind::FillStroke => g.alpha_fill.min(g.alpha_stroke),
        PaintKind::Glyph { .. } => match g.text.render_mode {
            1 | 5 => g.alpha_stroke,
            2 | 6 => g.alpha_fill.min(g.alpha_stroke),
            _ => g.alpha_fill,
        },
        _ => g.alpha_fill,
    }
    .clamp(0.0, 1.0);
    if alpha >= 1.0 {
        return None;
    }
    let t = 1.0 - alpha;
    Some((
        [255, u8_from_f64_round(200.0 * alpha), 0],
        u8_from_f64_round(60.0 + 120.0 * t),
    ))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use pdfxray_core::Point;
    use pdfxray_core::capture::{Alignment, GState};
    use pretty_assertions::assert_eq;

    use super::*;

    fn color(space: &str, components: &[f64]) -> ColorInfo {
        ColorInfo {
            space: space.to_owned(),
            resource: None,
            components: components.to_vec(),
            pattern: None,
        }
    }

    #[test]
    fn white_is_recognised_per_color_space() {
        assert_eq!(
            [
                is_white(&color("DeviceGray", &[1.0])),
                is_white(&color("DeviceRGB", &[1.0, 1.0, 1.0])),
                is_white(&color("DeviceCMYK", &[0.0, 0.0, 0.0, 0.0])),
                is_white(&color("DeviceGray", &[0.5])),
                is_white(&color("DeviceRGB", &[1.0, 0.0, 1.0])),
                is_white(&color("DeviceCMYK", &[0.0, 0.0, 0.0, 1.0])),
            ],
            [true, true, true, false, false, false]
        );
    }

    #[test]
    fn tag_colours_are_stable_and_spread() {
        let tags = ["P", "H1", "Span", "Figure", "Artifact", "Table", "TD", "L"];

        let colors: Vec<[u8; 4]> = tags.iter().map(|t| tag_color(t)).collect();

        assert_eq!(tag_color("P"), tag_color("P"));
        assert!(colors.iter().all(|c| TAG_PALETTE.contains(c)));
        // Smoke: eight distinct tags do not collapse onto one colour.
        let mut distinct = colors.clone();
        distinct.sort_unstable();
        distinct.dedup();
        assert!(distinct.len() >= 4, "{distinct:?}");
    }

    fn item(kind: PaintKind, bbox: Rect, edit: impl FnOnce(&mut GState)) -> PaintItem {
        let mut g = GState::default();
        edit(&mut g);
        PaintItem {
            ix: 0,
            op: 0,
            kind,
            bbox,
            clip: None,
            gstate: Arc::new(g),
            mc_stack: Vec::new(),
            xobject_stack: Vec::new(),
        }
    }

    fn page(items: Vec<PaintItem>) -> CapturedPage {
        CapturedPage {
            page: 0,
            ops: Vec::new(),
            items,
            marked: Vec::new(),
            xobjects: Vec::new(),
            alignment: Alignment::Exact,
            notes: Vec::new(),
        }
    }

    fn glyph() -> PaintKind {
        PaintKind::Glyph {
            glyph_index: 0,
            char_code: 65,
            unicode: None,
            origin: Point::new(0.0, 0.0),
            advance: 1.0,
        }
    }

    const CROP: Rect = Rect {
        x0: 0.0,
        y0: 0.0,
        x1: 200.0,
        y1: 200.0,
    };
    const INSIDE: Rect = Rect {
        x0: 10.0,
        y0: 10.0,
        x1: 50.0,
        y1: 30.0,
    };

    fn colors(overlays: &[Overlay]) -> Vec<([u8; 4], Option<[u8; 4]>)> {
        overlays
            .iter()
            .map(|o| match o {
                Overlay::Rect { stroke, fill, .. } => (*stroke, *fill),
            })
            .collect()
    }

    fn with_hidden() -> XrayOptions {
        XrayOptions {
            include_invisible: true,
            ..XrayOptions::default()
        }
    }

    #[test]
    fn hidden_items_are_left_out_or_highlighted_by_reason() {
        let outside = Rect::new(300.0, 300.0, 310.0, 310.0);
        let cap = page(vec![
            item(PaintKind::Fill, INSIDE, |_| {}),
            item(PaintKind::Fill, outside, |_| {}),
            item(glyph(), INSIDE, |g| g.text.render_mode = 3),
            item(glyph(), INSIDE, |g| g.text.render_mode = 7),
            item(glyph(), outside, |g| g.text.render_mode = 3),
            item(PaintKind::Fill, Rect::new(100.0, 100.0, 120.0, 120.0), |g| {
                g.fill = color("DeviceGray", &[1.0]);
            }),
        ]);

        let plain = xray_items(&cap, CROP, XrayOptions::default(), &|_| None);
        let full = xray_items(&cap, CROP, with_hidden(), &|_| None);

        assert_eq!(colors(&plain), vec![(KIND_PATH, None)]);
        let tint = |c: [u8; 4]| Some([c[0], c[1], c[2], HIDDEN_FILL_ALPHA]);
        assert_eq!(
            colors(&full),
            vec![
                (KIND_PATH, None),
                (OUTSIDE_CROP, tint(OUTSIDE_CROP)),
                (INVISIBLE_TEXT, tint(INVISIBLE_TEXT)),
                (INVISIBLE_TEXT, tint(INVISIBLE_TEXT)),
                // Outside the crop box wins over invisible text.
                (OUTSIDE_CROP, tint(OUTSIDE_CROP)),
                (WHITE_ON_WHITE, tint(WHITE_ON_WHITE)),
            ]
        );
    }

    #[test]
    fn the_alpha_option_fades_every_outline_and_fill_and_full_alpha_changes_nothing() {
        let cap = page(vec![
            item(PaintKind::Fill, INSIDE, |_| {}),
            item(glyph(), INSIDE, |g| g.text.render_mode = 3),
        ]);
        let faded = |alpha| XrayOptions {
            alpha,
            ..with_hidden()
        };

        let half = xray_items(&cap, CROP, faded(128), &|_| None);

        // The outline keeps its colour and halves its opacity; the tint of a hidden item too.
        assert_eq!(
            colors(&half),
            vec![
                ([KIND_PATH[0], KIND_PATH[1], KIND_PATH[2], 128], None),
                (
                    [INVISIBLE_TEXT[0], INVISIBLE_TEXT[1], INVISIBLE_TEXT[2], 128],
                    Some([INVISIBLE_TEXT[0], INVISIBLE_TEXT[1], INVISIBLE_TEXT[2], 24])
                ),
            ]
        );
        // Smoke: the default is opaque, and 255 is the palette as it is.
        assert_eq!(XrayOptions::default().alpha, u8::MAX);
        assert_eq!(
            xray_items(&cap, CROP, faded(255), &|_| None),
            xray_items(&cap, CROP, with_hidden(), &|_| None)
        );
        assert_eq!(
            colors(&xray_items(&cap, CROP, faded(255), &|_| None))[0].0,
            KIND_PATH
        );
    }

    #[test]
    fn white_paint_over_colour_is_not_white_on_white() {
        let white = |g: &mut GState| g.fill = color("DeviceRGB", &[1.0, 1.0, 1.0]);
        let backdrop = item(PaintKind::Fill, Rect::new(0.0, 0.0, 100.0, 100.0), |g| {
            g.fill = color("DeviceGray", &[0.2]);
        });
        let apart = item(PaintKind::Fill, Rect::new(100.0, 100.0, 120.0, 120.0), |_| {});
        let cap = page(vec![
            backdrop,
            item(glyph(), INSIDE, white),
            item(PaintKind::Fill, Rect::new(90.0, 90.0, 150.0, 150.0), white),
        ]);

        let out = xray_items(&cap, CROP, with_hidden(), &|_| None);

        // The glyph sits on the dark backdrop; the rectangle overlaps it too, so both can show.
        assert_eq!(
            colors(&out).iter().map(|c| c.0).collect::<Vec<_>>(),
            vec![KIND_PATH, KIND_GLYPH, KIND_PATH]
        );
        // Smoke: a dark box elsewhere on the page does not rescue the glyph.
        let cap = page(vec![apart, item(glyph(), INSIDE, white)]);
        assert_eq!(
            colors(&xray_items(&cap, CROP, with_hidden(), &|_| None))[1].0,
            WHITE_ON_WHITE
        );
    }

    #[test]
    fn stroked_text_is_white_only_when_the_colour_it_uses_is() {
        let stroke_white = |g: &mut GState| {
            g.stroke = color("DeviceGray", &[1.0]);
            g.text.render_mode = 1;
        };
        let mixed = |g: &mut GState| {
            g.stroke = color("DeviceGray", &[1.0]);
            g.text.render_mode = 2;
        };
        let cap = page(vec![
            item(glyph(), INSIDE, stroke_white),
            item(glyph(), INSIDE, mixed),
        ]);

        let out = xray_items(&cap, CROP, with_hidden(), &|_| None);

        // Mode 1 strokes in white; mode 2 also fills, in black.
        assert_eq!(
            colors(&out).iter().map(|c| c.0).collect::<Vec<_>>(),
            vec![WHITE_ON_WHITE, KIND_GLYPH]
        );
    }

    #[test]
    fn soft_mask_and_blend_mode_win_over_alpha() {
        let masked = item(PaintKind::Fill, INSIDE, |g| {
            g.soft_mask = true;
            g.alpha_fill = 0.5;
        });
        let blended = item(PaintKind::Fill, INSIDE, |g| g.blend_mode = "Screen".to_owned());
        let compatible = item(PaintKind::Fill, INSIDE, |g| {
            g.blend_mode = "Compatible".to_owned();
        });
        let stroke_only_alpha = item(PaintKind::Stroke, INSIDE, |g| {
            g.alpha_fill = 0.1;
            g.alpha_stroke = 1.0;
        });

        assert_eq!(heat(&masked), Some((HEAT_SOFT_MASK, 90)));
        assert_eq!(heat(&blended), Some((HEAT_BLEND, 90)));
        assert_eq!(heat(&compatible), None);
        // A stroke is judged by CA, not ca.
        assert_eq!(heat(&stroke_only_alpha), None);
    }

    #[test]
    fn alpha_tint_runs_from_yellow_to_red() {
        let plain = |alpha: f64| item(PaintKind::Fill, INSIDE, |g| g.alpha_fill = alpha);

        assert_eq!(heat(&plain(1.0)), None);
        assert_eq!(heat(&plain(0.5)), Some(([255, 100, 0], 120)));
        assert_eq!(heat(&plain(0.0)), Some(([255, 0, 0], 180)));
    }
}
