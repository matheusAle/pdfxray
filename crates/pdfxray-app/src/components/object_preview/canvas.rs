//! The canvas of a preview card, by kind: an image pixel for pixel on a checkerboard, a font in
//! its own program on a dark canvas with its glyph cells, the frame of a font that is not
//! embedded, a shading's bar with its stops, the swatches of a colour space, or another picture.

use egui::epaint::{PathShape, RectShape};
use egui::{
    Align, Color32, CornerRadius, Frame, Id, Label, Layout, Margin, Pos2, Rect, RichText, Role, ScrollArea,
    Sense, Stroke, StrokeKind, TextureHandle, TextureOptions, Ui, UiBuilder, Vec2, WidgetInfo, pos2, vec2,
};
use pdfxray_core::numeric::{f32_from_f64, f32_from_u32, f32_from_usize, usize_from_f32_trunc};
use pdfxray_core::{CharCode, ColorSpaceVisual, ImageVisual, ObjRef, ShadingVisual, Visual, VisualKind};

use super::facts::{stop_text, swatch_value, swatches_title};
use super::{
    FALLBACK_LABEL, GLYPHS_LABEL, NO_PREVIEW, NOT_EMBEDDED, NOT_EMBEDDED_WHY, Painted, RENDERING, Shown,
    Variant, caption, glyph_label, image_label, swatch_label, text_at,
};
use crate::components::Icon;
use crate::previews::{Bitmap, Mask, Picture};
use crate::state::PreviewScale;
use crate::theme::{fonts, tokens};

/// Space between the canvas's sides and what it holds.
const SIDE: f32 = 24.0;
/// Space above what the canvas holds.
const TOP: f32 = 18.0;
/// Space between a picture and the canvas's top and bottom edges.
const PICTURE_PAD: f32 = 18.0;
/// Space between an image and its caption.
const CAPTION_GAP: f32 = 8.0;
/// Height of a caption line.
const CAPTION_H: f32 = 13.0;
/// The top corners of the inspector's canvas: the card's radius inside its hairline.
const CANVAS_CORNER: u8 = 9;
/// Side of a checkerboard square.
const CHECKER: f32 = 10.0;

/// Size of the specimen on the centre card, and on the inspector card.
const SPECIMEN_PT: f32 = 56.0;
const SPECIMEN_PT_SMALL: f32 = 30.0;
/// A glyph cell: its size, the gap between two, the glyph's size and where its ascent sits.
const CELL: Vec2 = vec2(44.0, 56.0);
const CELL_GAP: f32 = 8.0;
const CELL_GLYPH_PT: f32 = 24.0;
const CELL_ASCENT_Y: f32 = 7.0;
const CELL_CODE_Y: f32 = 38.0;
/// Space between a block of the font canvas and the caption under it, and under a caption.
const BLOCK_GAP: f32 = 18.0;

/// The fallback specimen on the centre card, and on the inspector card.
const FALLBACK_PT: f32 = 48.0;
const FALLBACK_PT_SMALL: f32 = 30.0;
/// The not-embedded frame: inset from the canvas, its padding and where its text starts.
const FRAME_INSET: f32 = 12.0;
const FRAME_PAD: f32 = 16.0;
const FRAME_TEXT_X: f32 = 40.0;
/// The size of the line that says why a missing program matters.
const WHY_PT: f32 = 12.5;

/// The bar of an axial shading: where it starts and how deep it is, centre and inspector.
const BAR_TOP: f32 = 36.0;
const BAR_DEPTH: f32 = 56.0;
const BAR_TOP_SMALL: f32 = 24.0;
const BAR_DEPTH_SMALL: f32 = 36.0;
const BAR_RADIUS: f32 = 6.0;
/// Half the diagonal of a stop marker, and how far below the bar's edge its centre is.
const STOP_HALF: f32 = 6.0;
const STOP_DROP: f32 = 4.0;

/// Swatches: their size on the centre and inspector card, the smallest they shrink to, the gap,
/// and the room of the value under each.
const SWATCH: f32 = 40.0;
const SWATCH_SMALL: f32 = 28.0;
const SWATCH_MIN: f32 = 10.0;
const SWATCH_GAP: f32 = 8.0;
const SWATCH_VALUE_H: f32 = 16.0;
/// Swatches smaller than this have no value under them.
const SWATCH_VALUE_MIN: f32 = 24.0;

/// Draws the canvas of `visual`, `size` points, and what it holds.
pub(super) fn show(ui: &mut Ui, shown: &Shown, visual: &Visual, size: Vec2, variant: Variant) -> Painted {
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    // The inspector's canvas is the top of the card: it follows the card's corners there.
    let corners = match variant {
        Variant::Inspector => CornerRadius {
            nw: CANVAS_CORNER,
            ne: CANVAS_CORNER,
            sw: 0,
            se: 0,
        },
        Variant::Centre(_) => CornerRadius::ZERO,
    };
    let fill = if matches!(visual.kind, VisualKind::Font(_)) {
        tokens::FONT_CANVAS
    } else {
        tokens::CHECKER_DARK
    };
    ui.painter().rect_filled(rect, corners, fill);
    if let Shown::Ready(Picture::Bitmap(_)) = shown
        && !is_bar(visual)
    {
        checkerboard(ui, rect, corners);
    }
    let mut child = ui.new_child(
        UiBuilder::new()
            .max_rect(rect)
            .layout(Layout::top_down(Align::Min)),
    );
    child.set_clip_rect(rect.intersect(ui.clip_rect()));
    child.spacing_mut().item_spacing = Vec2::ZERO;
    let body = |ui: &mut Ui| contents(ui, shown, visual, size, variant);
    match variant {
        Variant::Centre(_) => {
            ScrollArea::both()
                .id_salt("object-preview-canvas")
                .auto_shrink(false)
                .show(&mut child, body)
                .inner
        }
        Variant::Inspector => body(&mut child),
    }
}

fn contents(ui: &mut Ui, shown: &Shown, visual: &Visual, viewport: Vec2, variant: Variant) -> Painted {
    match (shown, &visual.kind) {
        (Shown::Ready(Picture::Bitmap(bitmap)), VisualKind::Image(image)) => Painted {
            factor: Some(self::image(ui, bitmap, image, visual.obj, viewport, variant)),
        },
        (Shown::Ready(Picture::Bitmap(bitmap)), VisualKind::Shading(shading)) if is_bar(visual) => {
            bar(ui, bitmap, shading, visual.obj, viewport, variant);
            Painted::default()
        }
        (Shown::Ready(Picture::Bitmap(bitmap)), _) => {
            picture(ui, bitmap, visual.obj, viewport, variant);
            Painted::default()
        }
        (Shown::Ready(Picture::Font { specimen, glyphs }), _) => {
            font(ui, specimen.as_ref(), glyphs, visual.obj, viewport, variant);
            Painted::default()
        }
        (Shown::Ready(Picture::Swatches(colors)), VisualKind::ColorSpace(space)) => {
            swatches(ui, colors, space, visual.obj, viewport, variant);
            Painted::default()
        }
        (Shown::NotEmbedded, _) => {
            not_embedded(ui, viewport, variant);
            Painted::default()
        }
        (Shown::Unavailable(reason), _) => {
            unavailable(ui, NO_PREVIEW, reason);
            Painted::default()
        }
        // Swatches only come for a colour space; anything else is still on its way.
        (Shown::Rendering | Shown::Ready(Picture::Swatches(_)), _) => {
            let (rect, _) = ui.allocate_exact_size(viewport, Sense::hover());
            ui.put(
                rect,
                Label::new(
                    RichText::new(RENDERING)
                        .font(fonts::ui(tokens::text::BODY))
                        .color(tokens::TEXT_3),
                ),
            );
            Painted::default()
        }
    }
}

/// How many items `item` wide, `gap` apart, fit in `width`: at least one.
fn per_row(width: f32, item: f32, gap: f32) -> usize {
    usize_from_f32_trunc(((width + gap) / (item + gap)).floor().max(1.0))
}

/// An axial shading with an axis: its picture is a bar along it.
fn is_bar(visual: &Visual) -> bool {
    matches!(&visual.kind, VisualKind::Shading(shading) if shading.axis.is_some())
}

/// The two-tone checkerboard behind pictures, one quad with a repeating 2 x 2 texture.
fn checkerboard(ui: &Ui, rect: Rect, corners: CornerRadius) {
    let id = Id::unique("pdfxray-preview-checkerboard");
    let texture = ui
        .ctx()
        .data_mut(|d| d.get_temp::<TextureHandle>(id))
        .unwrap_or_else(|| {
            let (dark, light) = (tokens::CHECKER_DARK, tokens::CHECKER_LIGHT);
            let image = egui::ColorImage::new([2, 2], vec![dark, light, light, dark]);
            let texture =
                ui.ctx()
                    .load_texture("preview-checkerboard", image, TextureOptions::NEAREST_REPEAT);
            ui.ctx().data_mut(|d| d.insert_temp(id, texture.clone()));
            texture
        });
    let uv = Rect::from_min_max(Pos2::ZERO, (rect.size() / (2.0 * CHECKER)).to_pos2());
    ui.painter()
        .add(RectShape::filled(rect, corners, Color32::WHITE).with_texture(texture.id(), uv));
}

/// The whole texture.
fn full_uv() -> Rect {
    Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0))
}

/// `p` on the grid of physical pixels, so a picture shown pixel for pixel stays sharp.
fn snap(ui: &Ui, p: Pos2) -> Pos2 {
    let ppp = ui.ctx().pixels_per_point();
    pos2((p.x * ppp).round() / ppp, (p.y * ppp).round() / ppp)
}

/// Announces `rect` as the picture `label` for assistive technology.
fn announce(ui: &Ui, rect: Rect, id: impl egui::AsIdSalt, label: &str) {
    let response = ui.interact(rect, ui.make_persistent_id(id), Sense::hover());
    response.widget_info(|| WidgetInfo::labeled(Role::Image, true, label));
}

/// An image, nearest-neighbour: fitted by a whole factor (a fraction only when it must shrink),
/// or one preview pixel per screen pixel. The caption under it states the factor in screen
/// pixels per image pixel. Returns that factor.
fn image(
    ui: &mut Ui,
    bitmap: &Bitmap,
    image: &ImageVisual,
    r: ObjRef,
    viewport: Vec2,
    variant: Variant,
) -> f32 {
    let ppp = ui.ctx().pixels_per_point();
    let (w, h) = (f32_from_u32(image.width), f32_from_u32(image.height));
    let room = viewport - vec2(2.0 * SIDE, 2.0 * PICTURE_PAD + CAPTION_GAP + CAPTION_H);
    let fitted = (room.x / w).min(room.y / h).max(f32::MIN_POSITIVE) * ppp;
    let preview_px = f32_from_u32(bitmap.size.0) / w;
    let factor = match variant {
        Variant::Centre(PreviewScale::Actual) => preview_px,
        _ if fitted >= 1.0 => fitted.floor(),
        _ => fitted,
    };
    let size = vec2(w, h) * factor / ppp;
    let text = caption(image.width, image.height, factor);
    let caption_w = ui
        .painter()
        .layout_no_wrap(text.clone(), fonts::mono(tokens::mono::CAPTION), Color32::WHITE)
        .size()
        .x;
    let content = vec2(
        size.x.max(caption_w) + 2.0 * SIDE,
        size.y + CAPTION_GAP + CAPTION_H + 2.0 * PICTURE_PAD,
    );
    let (area, _) = ui.allocate_exact_size(content.max(viewport), Sense::hover());
    let top = area.min.y + ((area.height() - size.y - CAPTION_GAP - CAPTION_H) / 2.0).max(PICTURE_PAD);
    let left = (area.center().x - size.x / 2.0).max(area.min.x + SIDE);
    let rect = Rect::from_min_size(snap(ui, pos2(left, top)), size);
    announce(ui, rect, ("object-preview", r), &image_label(r));
    ui.painter()
        .image(bitmap.texture.id(), rect, full_uv(), Color32::WHITE);
    ui.painter().rect_stroke(
        rect,
        0.0,
        Stroke::new(1.0, tokens::PIXEL_BOUNDS),
        StrokeKind::Outside,
    );
    text_at(
        ui,
        pos2(rect.min.x, rect.max.y + CAPTION_GAP),
        caption_w + 4.0,
        RichText::new(text)
            .font(fonts::mono(tokens::mono::CAPTION))
            .color(tokens::TEXT_3),
        false,
    );
    factor
}

/// A drawing (a form, a pattern, a page, a shading over its area), smoothed: fitted, never
/// larger than a point per pixel, or one preview pixel per screen pixel.
fn picture(ui: &mut Ui, bitmap: &Bitmap, r: ObjRef, viewport: Vec2, variant: Variant) {
    let ppp = ui.ctx().pixels_per_point();
    let natural = vec2(f32_from_u32(bitmap.size.0), f32_from_u32(bitmap.size.1));
    let room = viewport - Vec2::splat(2.0 * PICTURE_PAD);
    let scale = match variant {
        Variant::Centre(PreviewScale::Actual) => 1.0 / ppp,
        _ => (room.x / natural.x).min(room.y / natural.y).min(1.0),
    };
    let size = natural * scale;
    let (area, _) = ui.allocate_exact_size(
        (size + Vec2::splat(2.0 * PICTURE_PAD)).max(viewport),
        Sense::hover(),
    );
    let rect = Rect::from_center_size(area.center(), size);
    let rect = Rect::from_min_size(snap(ui, rect.min), size);
    announce(ui, rect, ("object-preview", r), &image_label(r));
    ui.painter()
        .image(bitmap.texture.id(), rect, full_uv(), Color32::WHITE);
    ui.painter().rect_stroke(
        rect,
        0.0,
        Stroke::new(1.0, tokens::PIXEL_BOUNDS),
        StrokeKind::Outside,
    );
}

/// Where `mask` lands when its pen starts at `pen` (the left end of the baseline is not known,
/// the top of the glyph box is: `pen.y`) and its em is `em` points.
fn mask_rect(mask: &Mask, pen: Pos2, em: f32) -> Rect {
    let s = em / mask.em;
    let size = vec2(f32_from_u32(mask.bitmap.size.0), f32_from_u32(mask.bitmap.size.1)) * s;
    Rect::from_min_size(pen - Vec2::splat(mask.pad * s), size)
}

/// The height of the glyph box of `mask` (ascent to descent) at `em` points.
fn glyph_box_h(mask: &Mask, em: f32) -> f32 {
    let h = f32_from_u32(mask.bitmap.size.1);
    (h - 2.0 * mask.pad) * em / mask.em
}

/// An embedded font: the specimen in its own program, in the text colour, then a cell per glyph
/// the file shows with it, its code under it.
fn font(
    ui: &mut Ui,
    specimen: Option<&Mask>,
    glyphs: &[(CharCode, Mask)],
    r: ObjRef,
    viewport: Vec2,
    variant: Variant,
) {
    let ppp = ui.ctx().pixels_per_point();
    let width = viewport.x - 2.0 * SIDE;
    // The specimen's em: its design size, smaller when it would not fit, or one mask pixel per
    // screen pixel.
    let specimen_em = specimen.map(|mask| {
        let advance = (f32_from_u32(mask.bitmap.size.0) - 2.0 * mask.pad) / mask.em;
        match variant {
            Variant::Centre(PreviewScale::Actual) => mask.em / ppp,
            Variant::Centre(PreviewScale::Fit) => SPECIMEN_PT.min(width / advance.max(0.1)),
            Variant::Inspector => SPECIMEN_PT_SMALL.min(width / advance.max(0.1)),
        }
    });
    let mut y = TOP;
    let specimen_at = y;
    if let (Some(mask), Some(em)) = (specimen, specimen_em) {
        y += glyph_box_h(mask, em) + BLOCK_GAP;
    }
    let label_y = y;
    y += CAPTION_H + 5.0;
    let per_row = per_row(width, CELL.x, CELL_GAP);
    let rows = glyphs.len().div_ceil(per_row);
    let cells_h = f32_from_usize(rows) * (CELL.y + CELL_GAP) - CELL_GAP;
    let cells_y = y;
    let specimen_w = match (specimen, specimen_em) {
        (Some(mask), Some(em)) => mask_rect(mask, Pos2::ZERO, em).width(),
        _ => 0.0,
    };
    let content = vec2(specimen_w + 2.0 * SIDE, cells_y + cells_h.max(0.0) + BLOCK_GAP);
    let (area, _) = ui.allocate_exact_size(content.max(viewport), Sense::hover());
    let origin = area.min;

    if let (Some(mask), Some(em)) = (specimen, specimen_em) {
        let rect = mask_rect(mask, origin + vec2(SIDE, specimen_at), em);
        let rect = Rect::from_min_size(snap(ui, rect.min), rect.size());
        announce(ui, rect, ("object-preview", r), &image_label(r));
        ui.painter()
            .image(mask.bitmap.texture.id(), rect, full_uv(), tokens::SPECIMEN_INK);
    }
    if glyphs.is_empty() {
        return;
    }
    text_at(
        ui,
        origin + vec2(SIDE, label_y),
        width,
        RichText::new(GLYPHS_LABEL)
            .font(fonts::mono(tokens::mono::CAPTION))
            .color(tokens::TEXT_3),
        false,
    );
    let mut block = Rect::NOTHING;
    for (i, (code, mask)) in glyphs.iter().enumerate() {
        let (column, row) = (f32_from_usize(i % per_row), f32_from_usize(i / per_row));
        let cell = Rect::from_min_size(
            origin
                + vec2(
                    SIDE + column * (CELL.x + CELL_GAP),
                    cells_y + row * (CELL.y + CELL_GAP),
                ),
            CELL,
        );
        block = block.union(cell);
        cell_ui(ui, cell, code, mask);
    }
    if specimen.is_none() {
        announce(ui, block, ("object-preview", r), &image_label(r));
    }
}

/// One glyph cell: the glyph in the font's program, centred, and its code under it.
fn cell_ui(ui: &mut Ui, cell: Rect, code: &CharCode, mask: &Mask) {
    ui.painter().rect(
        cell,
        tokens::RADIUS_MD,
        tokens::GLYPH_CELL,
        Stroke::new(1.0, tokens::GLYPH_CELL_HAIRLINE),
        StrokeKind::Inside,
    );
    // The mask is placed with its pen at the origin: its top-left corner is above and left of
    // it by the mask's padding, so the ascent lands `CELL_ASCENT_Y` into the cell.
    let glyph = mask_rect(mask, Pos2::ZERO, CELL_GLYPH_PT);
    let at = pos2(
        cell.center().x - glyph.width() / 2.0,
        cell.min.y + CELL_ASCENT_Y + glyph.min.y,
    );
    let rect = Rect::from_min_size(snap(ui, at), glyph.size());
    ui.painter()
        .with_clip_rect(cell)
        .image(mask.bitmap.texture.id(), rect, full_uv(), tokens::SPECIMEN_INK);
    announce(ui, cell, ("glyph-cell", &code.hex), &glyph_label(code));
    let font = fonts::mono(tokens::mono::TINY);
    let code_w = ui
        .painter()
        .layout_no_wrap(code.hex.clone(), font.clone(), Color32::WHITE)
        .size()
        .x;
    text_at(
        ui,
        pos2(cell.center().x - code_w / 2.0, cell.min.y + CELL_CODE_Y),
        code_w + 2.0,
        RichText::new(&code.hex).font(font).color(tokens::TEXT_3),
        false,
    );
}

/// A font without its program: the amber frame that says so, and a dimmed specimen in the app's
/// own font, labelled as not the file's.
fn not_embedded(ui: &mut Ui, viewport: Vec2, variant: Variant) {
    let (area, _) = ui.allocate_exact_size(viewport, Sense::hover());
    let frame = area.shrink(FRAME_INSET);
    ui.painter().rect(
        frame,
        tokens::RADIUS_LG,
        tokens::WARN_FRAME,
        Stroke::new(1.0, tokens::WARN_FRAME_HAIRLINE),
        StrokeKind::Inside,
    );
    let inspector = variant == Variant::Inspector;
    let icon = Rect::from_min_size(frame.min + Vec2::splat(FRAME_PAD), Vec2::splat(16.0));
    Icon::Warning.paint(ui, icon, 16.0, tokens::WARN);
    let title_size = if inspector { 13.0 } else { 14.0 };
    let text_x = frame.min.x + FRAME_TEXT_X;
    let text_w = frame.max.x - FRAME_PAD - text_x;
    let title = text_at(
        ui,
        pos2(text_x, icon.center().y - title_size * 0.62),
        text_w,
        RichText::new(NOT_EMBEDDED)
            .font(fonts::semibold(title_size))
            .color(tokens::WARN),
        false,
    );
    let mut y = title.max.y + 6.0;
    if inspector {
        // No room for why: the title says enough.
        y += 6.0;
    } else {
        let why = text_at(
            ui,
            pos2(text_x, y),
            text_w,
            RichText::new(NOT_EMBEDDED_WHY)
                .font(fonts::ui(WHY_PT))
                .color(tokens::TEXT_2),
            true,
        );
        y = why.max.y + BLOCK_GAP;
    }
    let size = if inspector { FALLBACK_PT_SMALL } else { FALLBACK_PT };
    let specimen = text_at(
        ui,
        pos2(frame.min.x + FRAME_PAD, y),
        frame.width() - 2.0 * FRAME_PAD,
        RichText::new(pdfxray_core::SPECIMEN)
            .font(fonts::ui(size))
            .color(tokens::FALLBACK_INK),
        false,
    );
    text_at(
        ui,
        pos2(frame.min.x + FRAME_PAD, specimen.max.y + 6.0),
        frame.width() - 2.0 * FRAME_PAD,
        RichText::new(FALLBACK_LABEL)
            .font(fonts::mono(tokens::mono::CAPTION))
            .color(tokens::TEXT_3),
        false,
    );
}

/// An axial shading: its bar, the stops on its lower edge, and (on the centre card) their values.
fn bar(ui: &mut Ui, bitmap: &Bitmap, shading: &ShadingVisual, r: ObjRef, viewport: Vec2, variant: Variant) {
    let (area, _) = ui.allocate_exact_size(viewport, Sense::hover());
    let small = variant == Variant::Inspector;
    let (top, depth) = if small {
        (BAR_TOP_SMALL, BAR_DEPTH_SMALL)
    } else {
        (BAR_TOP, BAR_DEPTH)
    };
    let rect = Rect::from_min_size(
        area.min + vec2(SIDE, top),
        vec2((area.width() - 2.0 * SIDE).max(16.0), depth),
    );
    announce(ui, rect, ("object-preview", r), &image_label(r));
    ui.painter().add(
        RectShape::filled(rect, BAR_RADIUS, Color32::WHITE).with_texture(bitmap.texture.id(), full_uv()),
    );
    ui.painter().rect_stroke(
        rect,
        BAR_RADIUS,
        Stroke::new(1.0, tokens::SWATCH_HAIRLINE),
        StrokeKind::Inside,
    );
    let last = shading.stops.len().saturating_sub(1);
    let font = fonts::mono(tokens::mono::CAPTION);
    for (i, stop) in shading.stops.iter().enumerate() {
        let x = rect.min.x + rect.width() * f32_from_f64(stop.position.clamp(0.0, 1.0));
        let centre = pos2(x, rect.max.y + STOP_DROP);
        let diamond = vec![
            centre - vec2(0.0, STOP_HALF),
            centre + vec2(STOP_HALF, 0.0),
            centre + vec2(0.0, STOP_HALF),
            centre - vec2(STOP_HALF, 0.0),
        ];
        ui.painter().add(PathShape::convex_polygon(
            diamond,
            tokens::SPECIMEN_INK,
            Stroke::new(1.0, tokens::BG),
        ));
        if small {
            continue;
        }
        let text = stop_text(stop);
        let w = ui
            .painter()
            .layout_no_wrap(text.clone(), font.clone(), Color32::WHITE)
            .size()
            .x;
        let left = match i {
            0 => x - STOP_HALF,
            i if i == last => x + STOP_HALF - w,
            _ => x - w / 2.0,
        };
        text_at(
            ui,
            pos2(left, centre.y + STOP_HALF + 8.0),
            w + 2.0,
            RichText::new(text).font(font.clone()).color(tokens::TEXT_3),
            false,
        );
    }
}

/// The swatches of a colour space, in the colours the engine painted: a palette row with the
/// index under each (the components for other spaces). They shrink to fit unless shown 1:1.
fn swatches(
    ui: &mut Ui,
    colors: &[Color32],
    space: &ColorSpaceVisual,
    r: ObjRef,
    viewport: Vec2,
    variant: Variant,
) {
    let centre = matches!(variant, Variant::Centre(_));
    let width = viewport.x - 2.0 * SIDE;
    let start = if centre { TOP + CAPTION_H + 5.0 } else { TOP };
    let base = if centre { SWATCH } else { SWATCH_SMALL };
    let value_h = |size: f32| {
        if centre && size >= SWATCH_VALUE_MIN {
            SWATCH_VALUE_H
        } else {
            0.0
        }
    };
    let layout = |size: f32| {
        let per_row = per_row(width, size, SWATCH_GAP);
        let rows = colors.len().div_ceil(per_row);
        let height = f32_from_usize(rows) * (size + value_h(size) + SWATCH_GAP) - SWATCH_GAP;
        (per_row, height)
    };
    let room = viewport.y - start - TOP;
    let mut size = base;
    if variant != Variant::Centre(PreviewScale::Actual) {
        while size > SWATCH_MIN && layout(size).1 > room {
            size -= 1.0;
        }
    }
    let (per_row, height) = layout(size);
    let (area, _) = ui.allocate_exact_size(
        vec2(viewport.x, start + height + TOP).max(viewport),
        Sense::hover(),
    );
    if centre {
        text_at(
            ui,
            area.min + vec2(SIDE, TOP),
            width,
            RichText::new(swatches_title(space))
                .font(fonts::mono(tokens::mono::CAPTION))
                .color(tokens::TEXT_3),
            false,
        );
    }
    let radius = (5.0 * size / SWATCH).max(2.0);
    let step = vec2(size + SWATCH_GAP, size + value_h(size) + SWATCH_GAP);
    let mut block = Rect::NOTHING;
    for (i, color) in colors.iter().enumerate() {
        let (column, row) = (f32_from_usize(i % per_row), f32_from_usize(i / per_row));
        let swatch = Rect::from_min_size(
            area.min + vec2(SIDE + column * step.x, start + row * step.y),
            Vec2::splat(size),
        );
        block = block.union(swatch);
        ui.painter().rect(
            swatch,
            radius,
            *color,
            Stroke::new(1.0, tokens::SWATCH_HAIRLINE),
            StrokeKind::Inside,
        );
        announce(ui, swatch, ("swatch", i), &swatch_label(i, *color));
        if value_h(size) > 0.0 {
            text_at(
                ui,
                swatch.left_bottom() + vec2(2.0, 4.0),
                size + SWATCH_GAP,
                RichText::new(swatch_value(space, i))
                    .font(fonts::mono(tokens::mono::TINY))
                    .color(tokens::TEXT_3),
                false,
            );
        }
    }
    if block.is_positive() {
        announce(ui, block, ("object-preview", r), &image_label(r));
    }
}

/// The warning icon, `title` and why, at the top of the canvas.
fn unavailable(ui: &mut Ui, title: &str, reason: &str) {
    Frame::new().inner_margin(Margin::same(16)).show(ui, |ui| {
        ui.spacing_mut().item_spacing = vec2(6.0, 4.0);
        ui.horizontal(|ui| {
            Icon::Warning.show(ui, 13.0, tokens::WARN);
            ui.label(
                RichText::new(title)
                    .font(fonts::semibold(tokens::text::BODY))
                    .color(tokens::TEXT),
            );
        });
        ui.add(
            Label::new(
                RichText::new(reason)
                    .font(fonts::ui(tokens::text::BODY))
                    .color(tokens::TEXT_2),
            )
            .wrap(),
        );
    });
}
