//! The summary strip under the inspector's title: three or four facts about the selection as
//! chips, so the answer to "what is this?" does not need a section opened.
//!
//! An image says `1360×600 · DCT · ICC · 82% visible`, a text operator its glyphs, font and size,
//! a structure element its type, page and whether it has alternative text, and so on. The facts
//! come from what the sections below already read; nothing here is computed twice for show.

use egui::{Frame, Margin, Ui};
use pdfxray_core::capture::PaintKind;
use pdfxray_core::numeric::{u32_from_f64_round, usize_from_u32};
use pdfxray_core::{NodeId, PageIx, Props};

use super::graphics;
use super::structure::Cx;
use crate::components::{Chip, Tone};
use crate::object_facts::{ObjectFacts, human_bytes};
use crate::state::AppState;

/// Most chips in the strip.
const MAX_CHIPS: usize = 4;

/// One chip of the strip.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fact {
    /// What the chip says.
    pub text: String,
    /// How it says it: neutral, or a warning for something to look at.
    pub tone: Tone,
}

impl Fact {
    fn plain(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            tone: Tone::Neutral,
        }
    }

    fn warn(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            tone: Tone::Warn,
        }
    }
}

/// `DCTDecode` as `DCT`, `ICCBased` as `ICC`, `DeviceRGB` as `RGB`: the short names of the strip.
pub fn short_name(name: &str) -> String {
    let name = name.trim_start_matches('/');
    let name = name.strip_prefix("Device").unwrap_or(name);
    let name = name.strip_suffix("Decode").unwrap_or(name);
    name.strip_suffix("Based").unwrap_or(name).to_owned()
}

/// The first word of a value, short: the colour space of `ICCBased (/CS0)` is `ICC`.
fn first_short(value: &str) -> String {
    short_name(
        value
            .split([' ', '[', ','])
            .find(|part| !part.is_empty())
            .unwrap_or(value),
    )
}

/// `82% visible`: how much of what the operator painted nothing later covers.
fn visible_text(fraction: f64) -> String {
    let percent = u32_from_f64_round(fraction.clamp(0.0, 1.0) * 100.0);
    format!("{percent}% visible")
}

/// The facts of the selection, at most four.
pub fn of(state: &AppState, id: &NodeId, props: Option<&Props>) -> Vec<Fact> {
    let mut facts = match id {
        NodeId::Op { .. } => operator(state, id),
        NodeId::Glyph { .. } => glyph(state, id),
        NodeId::MarkedContent { page, mcid } => marked_content(state, *page, *mcid),
        NodeId::Artifact { page, .. } => vec![Fact::plain("artifact"), Fact::plain(page_text(*page))],
        NodeId::StructElem(_) => element(state, id, props),
        NodeId::Page(page) => page_facts(state, *page),
        NodeId::PageBox { page, kind } => {
            let mut facts = vec![Fact::plain(kind.key())];
            if let Some(page_box) = state
                .document
                .as_ref()
                .and_then(|loaded| loaded.doc.page(*page))
                .and_then(|info| info.boxes.iter().find(|b| b.kind == *kind))
            {
                facts.push(Fact::plain(state.lengths().size(page_box.rect)));
                facts.push(Fact::plain(crate::navigator::pages::provenance(
                    *kind,
                    page_box.source,
                )));
            }
            facts
        }
        _ => Vec::new(),
    };
    facts.truncate(MAX_CHIPS);
    facts
}

fn page_text(page: PageIx) -> String {
    format!("page {}", page + 1)
}

fn operator(state: &AppState, id: &NodeId) -> Vec<Fact> {
    let Some(cx) = Cx::new(state, id) else {
        return Vec::new();
    };
    let Some((page, captured, op)) = cx.op() else {
        return Vec::new();
    };
    let visible = || {
        (op.item_count > 0)
            .then(|| cx.doc.paint_step(page, op.ix))
            .flatten()
            .map(|step| {
                let text = visible_text(step.visible_fraction);
                if step.visible_fraction < 1.0 {
                    Fact::warn(text)
                } else {
                    Fact::plain(text)
                }
            })
    };
    if let Some(image) = graphics::image_facts(&cx) {
        let mut facts = vec![Fact::plain(format!("{}×{}", image.width, image.height))];
        let filter = image
            .filters
            .as_deref()
            .filter(|filters| *filters != "none")
            .map(first_short);
        let space = image.color_space.as_ref().map(|(value, _)| first_short(value));
        match (filter, space) {
            (Some(filter), Some(space)) => facts.push(Fact::plain(format!("{filter} · {space}"))),
            (Some(one), None) | (None, Some(one)) => facts.push(Fact::plain(one)),
            (None, None) if image.inline => facts.push(Fact::plain("inline")),
            (None, None) => {}
        }
        facts.extend(visible());
        return facts;
    }
    let glyphs = op
        .first_item
        .and_then(|first| usize::try_from(first).ok())
        .map_or(0, |first| {
            captured
                .items
                .iter()
                .skip(first)
                .take(usize_from_u32(op.item_count))
                .filter(|item| matches!(item.kind, PaintKind::Glyph { .. }))
                .count()
        });
    if glyphs > 0 {
        let text = &op.gstate.text;
        let mut facts = vec![Fact::plain(format!(
            "{glyphs} glyph{}",
            if glyphs == 1 { "" } else { "s" }
        ))];
        if let Some(font) = &text.font {
            facts.push(Fact::plain(format!(
                "/{} · {}",
                font.trim_start_matches('/'),
                cx.lengths.format(text.font_size)
            )));
        }
        facts.extend(visible());
        return facts;
    }
    let mut facts = vec![Fact::plain(op.operator.clone()), Fact::plain(page_text(page))];
    facts.extend(visible());
    facts
}

fn glyph(state: &AppState, id: &NodeId) -> Vec<Fact> {
    let Some(cx) = Cx::new(state, id) else {
        return Vec::new();
    };
    let mut facts = Vec::new();
    if let Some(info) = &cx.glyph {
        if let Some(point) = info.code_points.first() {
            facts.push(Fact::plain(format!("U+{point:04X}")));
        }
        if let Some(font) = &info.font {
            let name = font.resource.as_deref().map_or_else(
                || font.base_font.clone().unwrap_or_default(),
                |resource| format!("/{resource}"),
            );
            if !name.is_empty() {
                facts.push(Fact::plain(name));
            }
        }
        facts.push(Fact::plain(format!("w {}", pdfxray_core::num(info.width_units))));
    }
    facts
}

fn marked_content(state: &AppState, page: PageIx, mcid: i32) -> Vec<Fact> {
    let mut facts = vec![Fact::plain(format!("MCID {mcid}")), Fact::plain(page_text(page))];
    let owner = state.tag_index().and_then(|tags| tags.owner_of(page, mcid));
    match owner {
        Some(owner) => {
            let role = state
                .document
                .as_ref()
                .and_then(|loaded| loaded.doc.node(owner))
                .and_then(|props| props.get("Element", "Type").map(str::to_owned));
            if let Some(role) = role {
                facts.push(Fact::plain(format!("/{role}")));
            }
        }
        None => facts.push(Fact::warn("untagged")),
    }
    facts
}

fn element(state: &AppState, id: &NodeId, props: Option<&Props>) -> Vec<Fact> {
    let Some(loaded) = &state.document else {
        return Vec::new();
    };
    let mut facts = Vec::new();
    if let Some(role) = props.and_then(|p| p.get("Element", "Type")) {
        facts.push(Fact::plain(format!("/{role}")));
    }
    if let Some((page, _)) = loaded.doc.bbox(id) {
        facts.push(Fact::plain(page_text(page)));
    }
    if props.is_some_and(|p| p.get("Element", "Alt").is_some()) {
        facts.push(Fact::plain("Alt"));
    } else if state.tag_index().is_some_and(|tags| tags.lacks_alt(id)) {
        facts.push(Fact::warn("no Alt"));
    }
    if let Some(lang) = props.and_then(|p| p.get("Element", "Lang")) {
        facts.push(Fact::plain(lang));
    }
    facts
}

fn page_facts(state: &AppState, page: PageIx) -> Vec<Fact> {
    let Some(info) = state.document.as_ref().and_then(|loaded| loaded.doc.page(page)) else {
        return Vec::new();
    };
    let mut facts = vec![Fact::plain(crate::navigator::pages::display_size_text(
        info, state.unit,
    ))];
    if info.rotate != 0 {
        facts.push(Fact::plain(format!("rotated {}°", info.rotate)));
    }
    facts.push(Fact::plain(format!("{} boxes", info.boxes.len())));
    facts
}

/// The facts of a raw object: its filters and size, or its type, and the revision that wrote it.
pub fn of_object(facts: &ObjectFacts) -> Vec<Fact> {
    let mut found = Vec::new();
    match &facts.stream {
        Some(stream) => {
            found.push(Fact::plain(if stream.filters.is_empty() {
                "no filter".to_owned()
            } else {
                stream
                    .filters
                    .iter()
                    .map(|f| short_name(f))
                    .collect::<Vec<_>>()
                    .join(" + ")
            }));
            let size = match stream.decoded_len {
                Some(decoded) if decoded != stream.encoded_len => {
                    format!("{} → {}", human_bytes(stream.encoded_len), human_bytes(decoded))
                }
                _ => human_bytes(stream.encoded_len),
            };
            found.push(Fact::plain(size));
        }
        None => found.push(Fact::plain(facts.type_line.clone())),
    }
    found.push(Fact::plain(format!("rev {}", facts.defined_in + 1)));
    if facts.changed_later {
        found.push(Fact::warn("changed later"));
    }
    found
}

/// Draws the strip: nothing when there are no facts.
pub fn show(ui: &mut Ui, facts: &[Fact]) {
    if facts.is_empty() {
        return;
    }
    Frame::new()
        .inner_margin(Margin::symmetric(12, 2))
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing = egui::vec2(4.0, 4.0);
            ui.horizontal_wrapped(|ui| {
                for fact in facts {
                    Chip::new(&fact.text).tone(fact.tone).show(ui);
                }
            });
        });
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn long_pdf_names_are_shortened_for_chips() {
        assert_eq!(short_name("DCTDecode"), "DCT");
        assert_eq!(short_name("/FlateDecode"), "Flate");
        assert_eq!(short_name("ICCBased"), "ICC");
        assert_eq!(short_name("DeviceRGB"), "RGB");
        assert_eq!(first_short("ICCBased (/CS0)"), "ICC");
        // Smoke: a name with nothing to cut stays as it is.
        assert_eq!(short_name("Separation"), "Separation");
    }

    #[test]
    fn visibility_is_a_whole_percent() {
        assert_eq!(visible_text(0.82), "82% visible");
        assert_eq!(visible_text(1.0), "100% visible");
    }
}
