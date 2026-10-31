//! The GRAPHICS STATE section (blend, alpha, soft mask, clip, fill and stroke colours with
//! swatches) and the IMAGE section of an image `Do`.

use egui::{Color32, Ui};
use pdfxray_core::capture::{CapturedPage, ColorInfo, PaintKind};
use pdfxray_core::numeric::{u8_from_f64_round, usize_from_u32};
use pdfxray_core::{NodeId, Rect, ResKind, num};

use super::Action;
use super::defaults::{self, DefaultValue};
use super::structure::Cx;
use crate::components::{PropRow, tooltip};

/// Title of the graphics state section.
pub const TITLE: &str = "Graphics state";

/// Title of the image section.
pub const IMAGE_TITLE: &str = "Image";

/// Where the current value of a colour or a clip came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SetBy {
    /// Nothing changed it: the initial value.
    Initial,
    /// The operator at this execution index set it.
    Operator(String, u32),
    /// `Q` brought the value back from an earlier state.
    Restored(u32),
}

impl SetBy {
    /// The short text of the row: `rg`, `default`, `Q (restored)`.
    pub fn text(&self) -> String {
        match self {
            Self::Initial => "default".to_owned(),
            Self::Operator(name, _) => name.clone(),
            Self::Restored(_) => "Q (restored)".to_owned(),
        }
    }
}

/// `#1E3A5F` for a colour with a device-like component count (1 gray, 3 RGB, 4 CMYK), `None`
/// for spaces with no obvious screen colour (Separation, Lab, patterns).
pub fn color_hex(color: &ColorInfo) -> Option<String> {
    let [r, g, b] = rgb(color)?;
    Some(format!("#{r:02X}{g:02X}{b:02X}"))
}

/// The screen colour of `color`, 0 to 255 per channel.
fn rgb(color: &ColorInfo) -> Option<[u8; 3]> {
    if color.pattern.is_some()
        || matches!(
            color.space.as_str(),
            "Pattern" | "Separation" | "DeviceN" | "Lab" | "Indexed"
        )
    {
        return None;
    }
    let byte = |v: f64| u8_from_f64_round(v.clamp(0.0, 1.0) * 255.0);
    match color.components.as_slice() {
        [gray] => Some([byte(*gray); 3]),
        [r, g, b] => Some([byte(*r), byte(*g), byte(*b)]),
        [c, m, y, k] => Some([
            byte((1.0 - c) * (1.0 - k)),
            byte((1.0 - m) * (1.0 - k)),
            byte((1.0 - y) * (1.0 - k)),
        ]),
        _ => None,
    }
}

/// The colour as egui paints it.
fn swatch(color: &ColorInfo) -> Option<Color32> {
    rgb(color).map(|[r, g, b]| Color32::from_rgb(r, g, b))
}

/// The operators that set a fill (`stroking == false`) or stroke colour.
fn is_color_operator(operator: &str, stroking: bool) -> bool {
    if stroking {
        matches!(operator, "G" | "RG" | "K" | "SC" | "SCN" | "CS")
    } else {
        matches!(operator, "g" | "rg" | "k" | "sc" | "scn" | "cs")
    }
}

/// Where the fill or stroke colour in effect after operator `ix` came from: the last operator
/// that changed it.
pub fn color_source(captured: &CapturedPage, ix: u32, stroking: bool) -> SetBy {
    let colour = |op: &pdfxray_core::capture::OpRecord| {
        if stroking {
            op.gstate.stroke.clone()
        } else {
            op.gstate.fill.clone()
        }
    };
    let upto = (usize_from_u32(ix) + 1).min(captured.ops.len());
    for at in (0..upto).rev() {
        let op = &captured.ops[at];
        let before = at.checked_sub(1).map(|p| colour(&captured.ops[p]));
        if before.as_ref().is_some_and(|b| *b == colour(op)) {
            continue;
        }
        if before.is_none() && colour(op) == pdfxray_core::capture::GState::default().fill {
            return SetBy::Initial;
        }
        return if is_color_operator(&op.operator, stroking) {
            SetBy::Operator(op.operator.clone(), op.ix)
        } else if op.operator == "Q" {
            SetBy::Restored(op.ix)
        } else {
            SetBy::Operator(op.operator.clone(), op.ix)
        };
    }
    SetBy::Initial
}

/// The operators that made the clip in effect after operator `ix`: `["W", "n"]`, or `[]`.
pub fn clip_operators(captured: &CapturedPage, ix: u32) -> Vec<String> {
    let upto = (usize_from_u32(ix) + 1).min(captured.ops.len());
    for at in (0..upto).rev() {
        let op = &captured.ops[at];
        let before = at.checked_sub(1).map(|p| &captured.ops[p]);
        if before.is_some_and(|b| b.gstate.clip == op.gstate.clip) {
            continue;
        }
        let mut operators = Vec::new();
        if let Some(b) = before
            && matches!(b.operator.as_str(), "W" | "W*")
        {
            operators.push(b.operator.clone());
        }
        operators.push(op.operator.clone());
        return operators;
    }
    Vec::new()
}

/// `ca 1.0`: alpha as the design writes it, always with one decimal at least.
fn alpha(value: f64) -> String {
    if (value - value.round()).abs() < f64::EPSILON {
        format!("{value:.1}")
    } else {
        num(value)
    }
}

/// The graphics state values that nobody changed: the blend mode, alpha, soft mask and clip the
/// page started with.
pub fn default_values(state: &pdfxray_core::capture::GState) -> Vec<DefaultValue> {
    let mut found = Vec::new();
    if state.blend_mode == "Normal" {
        found.push(DefaultValue::new("Blend", "Normal", "Blend Normal"));
    }
    if (state.alpha_fill - 1.0).abs() < f64::EPSILON && (state.alpha_stroke - 1.0).abs() < f64::EPSILON {
        found.push(DefaultValue::new("Alpha", "ca 1.0 · CA 1.0", "Alpha 1"));
    }
    if !state.soft_mask {
        found.push(DefaultValue::new("SMask", "None", "no SMask"));
    }
    if state.clip.is_none() {
        found.push(DefaultValue::new("Clip", "none", "no clip"));
    }
    found
}

pub(super) fn show(ui: &mut Ui, cx: &Cx<'_>) {
    let Some((_, captured, op)) = cx.op() else {
        return;
    };
    let state = &op.gstate;
    let folded = default_values(state);
    let is_folded = |key: &str| folded.iter().any(|item| item.key == key);
    cx.section(ui, TITLE, |ui| {
        if !is_folded("Blend") {
            PropRow::new("Blend", &state.blend_mode).show(ui);
        }
        if !is_folded("Alpha") {
            PropRow::new(
                "Alpha",
                &format!(
                    "ca {} · CA {}",
                    alpha(state.alpha_fill),
                    alpha(state.alpha_stroke)
                ),
            )
            .show(ui);
        }
        if !is_folded("SMask") {
            PropRow::new("SMask", "Active").show(ui);
        }
        if !is_folded("Clip") {
            clip_row(ui, cx, captured, op.ix, state.clip);
        }
        for (key, color, stroking) in [("Fill", &state.fill, false), ("Stroke", &state.stroke, true)] {
            color_row(ui, key, color, &color_source(captured, op.ix, stroking));
        }
        defaults::show(ui, TITLE, &folded);
    });
}

fn clip_row(ui: &mut Ui, cx: &Cx<'_>, captured: &CapturedPage, ix: u32, clip: Option<Rect>) {
    let Some(rect) = clip else {
        PropRow::new("Clip", "none").show(ui);
        return;
    };
    let operators = clip_operators(captured, ix);
    let lengths = &cx.lengths;
    let value = format!(
        "{} · rect {} {} {} {}",
        operators.join(" "),
        lengths.number(rect.x0),
        lengths.number(rect.y0),
        lengths.number(rect.x1),
        lengths.number(rect.y1)
    );
    // The key's hint explains W, W* and n.
    PropRow::new("Clip", &value).show(ui);
}

fn color_row(ui: &mut Ui, key: &str, color: &ColorInfo, set_by: &SetBy) {
    let space = match &color.resource {
        Some(name) => format!("{} (/{name})", color.space),
        None => color.space.clone(),
    };
    let shown = match (color_hex(color), &color.pattern) {
        (Some(hex), _) => hex,
        (None, Some(pattern)) => format!("/{pattern}"),
        (None, None) => color
            .components
            .iter()
            .map(|c| num(*c))
            .collect::<Vec<_>>()
            .join(" "),
    };
    // Non-breaking spaces around the dot keep it with its words when the value wraps.
    let plain = format!("{shown} {space} · {}", set_by.text());
    let value = format!("{shown} {space}\u{a0}·\u{a0}{}", set_by.text());
    let mut row = PropRow::new(key, &value).copy_text(&plain);
    if let Some(fill) = swatch(color) {
        row = row.swatch(fill);
    }
    let output = row.show(ui);
    let origin = match set_by {
        SetBy::Initial => "the initial colour".to_owned(),
        SetBy::Operator(name, ix) => format!("set by {name} at op #{ix}"),
        SetBy::Restored(ix) => format!("restored by Q at op #{ix}"),
    };
    tooltip::text(output.value, format!("{shown} · {space} · {origin}"));
}

/// What the inspector knows of the image an operator painted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageFacts {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// True for `BI ... EI`.
    pub inline: bool,
    /// The colour space as the resource says it (`ICCBased`), with a link to its object.
    pub color_space: Option<(String, Option<NodeId>)>,
    /// The filters of the image stream (`DCTDecode`).
    pub filters: Option<String>,
    /// The encoded length in bytes, as text.
    pub length: Option<String>,
    /// The image `XObject`.
    pub object: Option<NodeId>,
}

/// The image the node painted, `None` when it painted none.
pub(super) fn image_facts(cx: &Cx<'_>) -> Option<ImageFacts> {
    let PaintKind::Image {
        width,
        height,
        inline,
    } = cx.item().map(|item| &item.kind)?
    else {
        return None;
    };
    let mut facts = ImageFacts {
        width: *width,
        height: *height,
        inline: *inline,
        color_space: None,
        filters: None,
        length: None,
        object: None,
    };
    if *inline {
        return Some(facts);
    }
    let (_, _, op) = cx.op()?;
    let name = op.operands.first().map_or("", |n| n.trim_start_matches('/'));
    let Some(owner) = cx.resource_owner() else {
        return Some(facts);
    };
    let resource = cx.doc.node(&NodeId::Resource {
        owner,
        kind: ResKind::XObject,
        name: name.to_owned(),
    });
    let row_of = |group: &str, key: &str| {
        resource
            .as_ref()
            .and_then(|p| p.groups.iter().find(|g| g.name == group))
            .and_then(|g| g.rows.iter().find(|r| r.key == key))
    };
    facts.object = row_of("Resource", "Object").and_then(|r| r.link.clone());
    facts.color_space = row_of("Image", "ColorSpace").map(|r| (r.value.clone(), r.link.clone()));
    let stream = facts.object.as_ref().and_then(|id| cx.doc.node(id));
    facts.filters = stream
        .as_ref()
        .and_then(|p| p.get("Stream", "Filters"))
        .map(str::to_owned);
    facts.length = stream
        .as_ref()
        .and_then(|p| p.get("Stream", "Encoded bytes"))
        .map(str::to_owned);
    Some(facts)
}

/// The IMAGE section of an operator that painted an image.
pub(super) fn image(ui: &mut Ui, cx: &Cx<'_>, action: &mut Action) {
    let Some(facts) = image_facts(cx) else {
        return;
    };
    cx.section(ui, IMAGE_TITLE, |ui| {
        PropRow::new("Dimensions", &format!("{} × {} px", facts.width, facts.height)).show(ui);
        if facts.inline {
            PropRow::new("Kind", "inline image").show(ui);
            return;
        }
        if let Some((space, link)) = &facts.color_space {
            let output = PropRow::new("ColorSpace", space).link(link.is_some()).show(ui);
            if output.value.clicked() {
                action.navigate.clone_from(link);
            }
        }
        if let Some(filters) = &facts.filters {
            PropRow::new("Filter", filters).show(ui);
        }
        if let Some(length) = &facts.length {
            PropRow::new("Length", &format!("{length} bytes")).show(ui);
        }
        if let Some(id) = &facts.object {
            let output = PropRow::new("Object", &id.to_string()).link(true).show(ui);
            if output.value.clicked() {
                action.navigate = Some(id.clone());
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use pdfxray_core::capture::GState;
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn a_fresh_graphics_state_folds_blend_alpha_soft_mask_and_clip() {
        let shorts: Vec<String> = default_values(&GState::default())
            .into_iter()
            .map(|item| item.short)
            .collect();
        assert_eq!(shorts, ["Blend Normal", "Alpha 1", "no SMask", "no clip"]);
    }

    #[test]
    fn a_changed_value_is_not_folded() {
        let state = GState {
            blend_mode: "Multiply".to_owned(),
            alpha_fill: 0.5,
            soft_mask: true,
            ..GState::default()
        };
        let keys: Vec<&str> = default_values(&state).iter().map(|item| item.key).collect();
        assert_eq!(keys, ["Clip"]);
    }
}
