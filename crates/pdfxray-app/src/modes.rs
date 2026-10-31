//! Debug render modes of the canvas: x-ray outlines, isolate/hide, the paint-order stepper's
//! position, the clip in effect and the alpha heat map.
//!
//! [`ViewMode`] is the plain settings record the toolbar edits. [`RenderRequest`] is what one
//! bitmap needs (page, scale, which operators paint, which overlays); it is the cache key of
//! `crate::page_cache` and is turned into a [`RenderSpec`] on the render thread.

use std::collections::BTreeSet;
use std::time::Duration;

use egui::Key;
use pdfxray_core::{Document, NodeId, PageIx};
use pdfxray_render::xray::{
    ARTIFACT, HEAT_BLEND, HEAT_SOFT_MASK, INVISIBLE_TEXT, KIND_GLYPH, KIND_IMAGE, KIND_PATH, KIND_SHADING,
    OUTSIDE_CROP, TAG_PALETTE, TAGGED, UNTAGGED, UNTAGGED_TAG, WHITE_ON_WHITE,
};
use pdfxray_render::{
    ColorBy, ContentFilter, RenderSpec, XrayOptions, alpha_heatmap_overlays, clip_overlays, ops_for,
    xray_overlays,
};

use crate::components::Icon;
use crate::selection::node_page;
use crate::state::AppState;
use crate::view::ScaleKey;

/// Whether an isolation keeps only its operators or removes them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IsolateMode {
    /// Only the node paints.
    Only,
    /// Everything but the node paints.
    Hide,
}

impl IsolateMode {
    /// Word for the chip and the status bar, e.g. `Isolated`.
    pub const fn title(self) -> &'static str {
        match self {
            Self::Only => "Isolated",
            Self::Hide => "Hidden",
        }
    }
}

/// A node that is isolated or hidden on one page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Isolation {
    /// Keep or remove [`Self::ops`].
    pub mode: IsolateMode,
    /// The node the user chose.
    pub node: NodeId,
    /// Display name of [`Self::node`].
    pub title: String,
    /// Page the operators belong to; filters are per page.
    pub page: PageIx,
    /// The operators that make up the node.
    pub ops: BTreeSet<u32>,
}

/// Opacity of the x-ray outlines in the page bitmap (50 %); the hovered and the selected item
/// are drawn over it at full strength by the canvas.
pub const XRAY_ALPHA: u8 = 128;

/// What the tag overlay of the canvas shows (see `center::canvas::tag_overlay`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TagShow {
    /// A box for every structure element on the page (`⇧T`).
    pub all_boxes: bool,
    /// The tag name over each box (`L`).
    pub labels: bool,
    /// Numbered badges and the path through the elements (`R`).
    pub reading_order: bool,
    /// Draw the containers (Sect, Div) as thin neutral boxes around their children.
    pub nesting: bool,
    /// Include artifacts.
    pub artifacts: bool,
}

impl Default for TagShow {
    fn default() -> Self {
        Self {
            all_boxes: false,
            labels: true,
            reading_order: false,
            nesting: true,
            artifacts: true,
        }
    }
}

/// Where the reading order comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OrderSource {
    /// The logical order of the structure tree.
    #[default]
    Structure,
    /// The order the content is painted in.
    Paint,
    /// What a screen reader announces, for the reader the Speech tab models.
    Speech,
}

impl OrderSource {
    /// Every source, in menu order.
    pub const ALL: [Self; 3] = [Self::Structure, Self::Paint, Self::Speech];

    /// Text of the option.
    pub const fn title(self) -> &'static str {
        match self {
            Self::Structure => "Structure tree",
            Self::Paint => "Paint order",
            Self::Speech => "Speech",
        }
    }

    /// The words of the reading-order header: `from structure tree`.
    pub const fn from_text(self) -> &'static str {
        match self {
            Self::Structure => "from structure tree",
            Self::Paint => "from paint order",
            Self::Speech => "from speech order",
        }
    }
}

/// Everything the View menu sets.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ViewMode {
    /// Outline every painted item.
    pub xray: bool,
    /// What the x-ray colours encode.
    pub color_by: ColorBy,
    /// What the tag overlay of the x-ray shows.
    pub show: TagShow,
    /// Where the reading order of the tag overlay comes from.
    pub order_from: OrderSource,
    /// Also outline items that cannot be seen.
    pub invisible: bool,
    /// The current isolate or hide, if any.
    pub isolation: Option<Isolation>,
    /// Whether the paint-order stepper is on.
    pub stepper: bool,
    /// Last operator index that paints while the stepper is on.
    pub step: u32,
    /// Show the clip in effect for the selected operator.
    pub clip: bool,
    /// Tint items that are not plain opaque.
    pub heatmap: bool,
    /// The stepper moves over operators that paint nothing (graphics state, marked content ...)
    /// instead of stopping at each.
    pub skip_state_only: bool,
    /// The stepper is playing: it advances by itself.
    pub playing: bool,
    /// Time since the stepper last advanced while playing.
    pub(crate) play_carry: Duration,
}

impl ViewMode {
    /// True when `mode` is applied to a node on `page`.
    pub fn isolates(&self, mode: IsolateMode, page: PageIx) -> bool {
        self.isolation
            .as_ref()
            .is_some_and(|i| i.mode == mode && i.page == page)
    }

    /// The operators that may paint on `page`: the isolation and the stepper combined.
    pub fn content_filter(&self, doc: &Document, page: PageIx, current: PageIx) -> ContentFilter {
        let isolation = self.isolation.as_ref().filter(|i| i.page == page);
        // The stepper walks the current page; the others are painted whole.
        let step = (self.stepper && page == current).then_some(self.step);
        match (isolation, step) {
            (None, None) => ContentFilter::All,
            (None, Some(last)) => ContentFilter::UpTo(last),
            (Some(i), None) => match i.mode {
                IsolateMode::Only => ContentFilter::Only(i.ops.clone()),
                IsolateMode::Hide => ContentFilter::Hide(i.ops.clone()),
            },
            (Some(i), Some(last)) => ContentFilter::Only(match i.mode {
                IsolateMode::Only => i.ops.iter().copied().filter(|ix| *ix <= last).collect(),
                IsolateMode::Hide => doc
                    .captured(page)
                    .into_iter()
                    .flat_map(|c| c.ops.iter().map(|o| o.ix))
                    .filter(|ix| *ix <= last && !i.ops.contains(ix))
                    .collect(),
            }),
        }
    }
}

/// Text of the colour-by selector.
pub const fn color_title(by: ColorBy) -> &'static str {
    match by {
        ColorBy::Kind => "Kind",
        ColorBy::Tag => "Tag",
        ColorBy::TagRole => "Tag role",
        ColorBy::Artifact => "Artifact",
    }
}

/// The colourings, in selector order.
pub const COLOR_MODES: [ColorBy; 3] = [ColorBy::Kind, ColorBy::TagRole, ColorBy::Artifact];

/// A render mode: one toggle, one key. The page-wide ones are in the View menu; isolate and hide
/// act on the selection and sit on the toolbar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewTool {
    /// Outline every painted item.
    Xray,
    /// Paint only the selected node.
    Isolate,
    /// Paint everything but the selected node.
    Hide,
    /// Paint the page one operator at a time.
    PaintOrder,
    /// Outline the clip in effect.
    Clip,
    /// Tint items that are not plain opaque.
    Alpha,
}

impl ViewTool {
    /// Every mode, in menu order.
    pub const ALL: [Self; 6] = [
        Self::Xray,
        Self::Isolate,
        Self::Hide,
        Self::PaintOrder,
        Self::Clip,
        Self::Alpha,
    ];

    /// The modes that change how the whole page is painted, in View menu order.
    pub const PAGE: [Self; 4] = [Self::Xray, Self::PaintOrder, Self::Clip, Self::Alpha];

    /// The modes that act on the selection, in toolbar order.
    pub const SELECTION: [Self; 2] = [Self::Isolate, Self::Hide];

    /// Name in the menu and on the View button while it is on.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Xray => "X-ray",
            Self::Isolate => "Isolate",
            Self::Hide => "Hide",
            Self::PaintOrder => "Paint order",
            Self::Clip => "Clip",
            Self::Alpha => "Alpha",
        }
    }

    /// Icon in the menu.
    pub const fn icon(self) -> Icon {
        match self {
            Self::Xray => Icon::Xray,
            Self::Isolate => Icon::Isolate,
            Self::Hide => Icon::Hide,
            Self::PaintOrder => Icon::PaintOrder,
            Self::Clip => Icon::Clip,
            Self::Alpha => Icon::Alpha,
        }
    }

    /// The key that toggles it.
    pub const fn key(self) -> Key {
        match self {
            Self::Xray => Key::X,
            Self::Isolate => Key::I,
            Self::Hide => Key::H,
            Self::PaintOrder => Key::P,
            Self::Clip => Key::C,
            Self::Alpha => Key::A,
        }
    }

    /// What it does, in a sentence.
    pub const fn hint(self) -> &'static str {
        match self {
            Self::Xray => "Outline every painted item: glyphs, paths, images, shadings",
            Self::Isolate => "Paint only the selected node's content",
            Self::Hide => "Paint everything except the selected node's content",
            Self::PaintOrder => "Paint the page one operator at a time",
            Self::Clip => "Outline the clip in effect for the selected operator",
            Self::Alpha => "Tint items with a soft mask, a blend mode or alpha below 1",
        }
    }

    /// Whether the mode is on.
    pub fn active(self, state: &AppState) -> bool {
        let view = &state.canvas.view;
        match self {
            Self::Xray => view.xray,
            Self::Isolate => view.isolates(IsolateMode::Only, state.canvas.page),
            Self::Hide => view.isolates(IsolateMode::Hide, state.canvas.page),
            Self::PaintOrder => view.stepper,
            Self::Clip => view.clip,
            Self::Alpha => view.heatmap,
        }
    }

    /// Whether the mode can be turned on: isolate and hide need a selection with content, the
    /// stepper needs a page with operators. A mode that is on can always be turned off.
    pub fn available(self, state: &AppState) -> bool {
        self.active(state)
            || match self {
                Self::Isolate | Self::Hide => state.can_isolate(),
                Self::PaintOrder => state.last_op(state.canvas.page).is_some(),
                Self::Xray | Self::Clip | Self::Alpha => true,
            }
    }

    /// Turns the mode on or off.
    pub fn toggle(self, state: &mut AppState) {
        match self {
            Self::Xray => state.canvas.view.xray = !state.canvas.view.xray,
            Self::Isolate => state.toggle_isolation(IsolateMode::Only),
            Self::Hide => state.toggle_isolation(IsolateMode::Hide),
            Self::PaintOrder => state.set_stepper(!state.canvas.view.stepper),
            Self::Clip => state.canvas.view.clip = !state.canvas.view.clip,
            Self::Alpha => state.canvas.view.heatmap = !state.canvas.view.heatmap,
        }
    }
}

/// One bitmap to render. Two requests that are equal produce the same pixels, so it is the key
/// of the bitmap cache.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderRequest {
    /// Page to draw.
    pub page: PageIx,
    /// Bitmap scale.
    pub scale: ScaleKey,
    /// Which operators paint.
    pub content: ContentFilter,
    /// X-ray outlines, if on.
    pub xray: Option<XrayOptions>,
    /// Operator whose clip is outlined.
    pub clip_op: Option<u32>,
    /// Whether the alpha heat map is drawn.
    pub heatmap: bool,
}

impl RenderRequest {
    /// The render specification: overlays are x-ray first, then the heat map, then the clip.
    pub fn spec(&self, doc: &Document) -> RenderSpec {
        let mut overlays = Vec::new();
        if let Some(options) = self.xray {
            overlays.extend(xray_overlays(doc, self.page, options));
        }
        if self.heatmap {
            overlays.extend(alpha_heatmap_overlays(doc, self.page));
        }
        if let Some(op) = self.clip_op {
            overlays.extend(clip_overlays(doc, self.page, op));
        }
        RenderSpec {
            scale: self.scale.scale(),
            overlays,
            content: self.content.clone(),
        }
    }
}

/// One line of a legend: a name and the colours it is drawn in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegendEntry {
    /// What the colour means.
    pub label: &'static str,
    /// Straight RGBA colours; more than one when the meaning has a palette.
    pub colors: Vec<[u8; 4]>,
}

impl LegendEntry {
    fn one(label: &'static str, color: [u8; 4]) -> Self {
        Self {
            label,
            colors: vec![color],
        }
    }
}

/// The legend of the x-ray colouring `by`, plus the hidden-item colours when `invisible`.
pub fn legend_entries(by: ColorBy, invisible: bool) -> Vec<LegendEntry> {
    let mut entries = match by {
        ColorBy::Kind => vec![
            LegendEntry::one("Glyph", KIND_GLYPH),
            LegendEntry::one("Path", KIND_PATH),
            LegendEntry::one("Image", KIND_IMAGE),
            LegendEntry::one("Shading", KIND_SHADING),
        ],
        // The Tag roles card, in the user's role colours, is the legend of this one.
        ColorBy::TagRole => Vec::new(),
        ColorBy::Tag => vec![
            LegendEntry::one("No tag", UNTAGGED_TAG),
            LegendEntry {
                label: "Tag name",
                colors: TAG_PALETTE.to_vec(),
            },
        ],
        ColorBy::Artifact => vec![
            LegendEntry::one("Artifact", ARTIFACT),
            LegendEntry::one("Tagged", TAGGED),
            LegendEntry::one("Untagged", UNTAGGED),
        ],
    };
    if invisible {
        entries.extend([
            LegendEntry::one("Outside crop box", OUTSIDE_CROP),
            LegendEntry::one("Invisible text", INVISIBLE_TEXT),
            LegendEntry::one("White on white", WHITE_ON_WHITE),
        ]);
    }
    entries
}

/// The legend of the alpha heat map.
pub fn heatmap_legend() -> Vec<LegendEntry> {
    let opaque = |rgb: [u8; 3]| [rgb[0], rgb[1], rgb[2], 255];
    vec![
        LegendEntry::one("Soft mask", opaque(HEAT_SOFT_MASK)),
        LegendEntry::one("Blend mode", opaque(HEAT_BLEND)),
        LegendEntry {
            label: "Alpha below 1",
            colors: vec![[255, 200, 0, 255], [255, 0, 0, 255]],
        },
    ]
}

/// What the selection means for the render modes: the operators behind it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SelectedOps {
    /// The node the operators were computed for; for a validation finding, its target.
    pub(crate) node: NodeId,
    /// Page of the operators.
    pub(crate) page: PageIx,
    /// The operators; empty when the node has no content to isolate.
    pub(crate) ops: BTreeSet<u32>,
}

impl AppState {
    /// The node that isolate, hide and clip act on: the selection, or for a validation finding
    /// the node the finding points at.
    pub fn view_target(&self) -> Option<NodeId> {
        let selection = self.selection.primary.as_ref()?;
        let NodeId::Validation(index) = selection else {
            return Some(selection.clone());
        };
        let finding = self
            .document
            .as_ref()?
            .doc
            .validation()?
            .findings
            .get(usize::try_from(*index).ok()?)?;
        finding.target.clone()
    }

    /// Recomputes the operators behind [`Self::view_target`] when the selection changed. Called
    /// once per frame; cheap while nothing changed.
    pub(crate) fn refresh_selection_ops(&mut self) {
        let target = self.view_target();
        if self.canvas.selected_ops.as_ref().map(|s| &s.node) == target.as_ref() {
            return;
        }
        self.canvas.selected_ops = target.and_then(|node| {
            let doc = &self.document.as_ref()?.doc;
            let page = node_page(&node).or_else(|| doc.bbox(&node).map(|(page, _)| page))?;
            let ops = ops_for(doc, &node);
            Some(SelectedOps { node, page, ops })
        });
    }

    /// True when the selection has content to isolate or hide.
    pub fn can_isolate(&self) -> bool {
        self.canvas
            .selected_ops
            .as_ref()
            .is_some_and(|s| !s.ops.is_empty())
    }

    /// Isolates or hides the selection's content; does nothing when it has none.
    pub fn isolate_selection(&mut self, mode: IsolateMode) {
        self.refresh_selection_ops();
        let Some(selected) = self.canvas.selected_ops.clone().filter(|s| !s.ops.is_empty()) else {
            return;
        };
        let Some(loaded) = &self.document else {
            return;
        };
        let title = loaded
            .doc
            .node(&selected.node)
            .map_or_else(|| selected.node.to_string(), |props| props.title);
        self.go_to_page(selected.page);
        self.canvas.view.isolation = Some(Isolation {
            mode,
            node: selected.node,
            title,
            page: selected.page,
            ops: selected.ops,
        });
    }

    /// Toggles `mode`: turns it off when it is on for this page, otherwise applies it to the
    /// selection.
    pub fn toggle_isolation(&mut self, mode: IsolateMode) {
        if self.canvas.view.isolates(mode, self.canvas.page) {
            self.clear_isolation();
        } else {
            self.isolate_selection(mode);
        }
    }

    /// Shows all content again.
    pub fn clear_isolation(&mut self) {
        self.canvas.view.isolation = None;
    }

    /// The operator whose clip the Clip toggle shows: the selected operator, or the operator of
    /// the selected glyph, when it is on the page shown.
    pub fn clip_op(&self) -> Option<u32> {
        match self.selection.primary.as_ref()? {
            NodeId::Op { page, ix } if *page == self.canvas.page => Some(*ix),
            NodeId::Glyph { page, op, .. } if *page == self.canvas.page => Some(*op),
            _ => None,
        }
    }

    /// What the canvas needs rendered for `page` at `scale`, or `None` without a document.
    pub fn render_request(&self, page: PageIx, scale: ScaleKey) -> Option<RenderRequest> {
        let doc = &self.document.as_ref()?.doc;
        let view = &self.canvas.view;
        Some(RenderRequest {
            page,
            scale,
            content: view.content_filter(doc, page, self.canvas.page),
            xray: view.xray.then_some(XrayOptions {
                color_by: view.color_by,
                include_invisible: view.invisible,
                alpha: XRAY_ALPHA,
            }),
            // The clip belongs to the selection, which is on the current page.
            clip_op: if view.clip && page == self.canvas.page {
                self.clip_op()
            } else {
                None
            },
            heatmap: view.heatmap,
        })
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;
    use crate::test_support::fixture;

    fn isolation(mode: IsolateMode, ops: &[u32]) -> Isolation {
        Isolation {
            mode,
            node: NodeId::Op { page: 0, ix: 3 },
            title: "Op".to_owned(),
            page: 0,
            ops: ops.iter().copied().collect(),
        }
    }

    #[test]
    fn the_filter_combines_isolation_and_stepper() {
        let doc = fixture("graphics-state");
        let mut view = ViewMode::default();
        assert_eq!(view.content_filter(&doc, 0, 0), ContentFilter::All);

        view.stepper = true;
        view.step = 4;
        assert_eq!(view.content_filter(&doc, 0, 0), ContentFilter::UpTo(4));
        // The stepper walks the current page only; the page below is painted whole.
        assert_eq!(view.content_filter(&doc, 1, 0), ContentFilter::All);

        view.stepper = false;
        view.isolation = Some(isolation(IsolateMode::Hide, &[2, 3]));
        assert_eq!(
            view.content_filter(&doc, 0, 0),
            ContentFilter::Hide([2, 3].into_iter().collect())
        );
        // Filters are per page.
        assert_eq!(view.content_filter(&doc, 1, 0), ContentFilter::All);

        // Both: hide keeps everything up to the step except the hidden ones.
        view.stepper = true;
        assert_eq!(
            view.content_filter(&doc, 0, 0),
            ContentFilter::Only([0, 1, 4].into_iter().collect())
        );
        view.isolation = Some(isolation(IsolateMode::Only, &[2, 3, 9]));
        assert_eq!(
            view.content_filter(&doc, 0, 0),
            ContentFilter::Only([2, 3].into_iter().collect())
        );
    }

    #[test]
    fn legends_list_the_palette_of_their_mode() {
        let labels = |by, invisible| {
            legend_entries(by, invisible)
                .into_iter()
                .map(|e| e.label)
                .collect::<Vec<_>>()
        };

        assert_eq!(
            labels(ColorBy::Kind, false),
            ["Glyph", "Path", "Image", "Shading"]
        );
        assert_eq!(labels(ColorBy::Tag, false), ["No tag", "Tag name"]);
        assert_eq!(
            labels(ColorBy::Artifact, false),
            ["Artifact", "Tagged", "Untagged"]
        );
        assert_eq!(
            labels(ColorBy::Artifact, true),
            [
                "Artifact",
                "Tagged",
                "Untagged",
                "Outside crop box",
                "Invisible text",
                "White on white"
            ]
        );
        // Smoke: the tag legend carries the whole palette.
        assert_eq!(
            legend_entries(ColorBy::Tag, false)[1].colors.len(),
            TAG_PALETTE.len()
        );
    }

    #[test]
    fn the_request_spec_orders_overlays_and_keeps_the_filter() {
        let doc = fixture("graphics-state");
        let request = RenderRequest {
            page: 0,
            scale: crate::view::render_scale(1.0, 1.0, egui::vec2(612.0, 792.0), 8192),
            content: ContentFilter::UpTo(10),
            xray: Some(XrayOptions::default()),
            clip_op: Some(59),
            heatmap: false,
        };

        let spec = request.spec(&doc);

        assert_eq!(spec.content, ContentFilter::UpTo(10));
        assert!((spec.scale - 1.0).abs() < f32::EPSILON);
        let expected =
            xray_overlays(&doc, 0, XrayOptions::default()).len() + clip_overlays(&doc, 0, 59).len();
        assert_eq!(spec.overlays.len(), expected);
        // Smoke: the clip is the last overlay and the page really has one at op 59.
        assert_eq!(spec.overlays.last(), clip_overlays(&doc, 0, 59).first());
        assert_eq!(clip_overlays(&doc, 0, 59).len(), 1);
    }
}
