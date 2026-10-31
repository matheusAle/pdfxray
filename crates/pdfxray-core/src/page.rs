use hayro_syntax::page::{Page, Rotation};
use serde::Serialize;

use crate::pagetree::{BoxSource, PageBox, RawPage};
use crate::{BoxKind, NodeId, ObjRef, PageIx, PropGroup, Props, Rect, TreeNode};

/// Facts about one page. Boxes come from the raw page tree (exact values and where they were
/// inherited from); when the raw tree is unreadable they fall back to the engine's view.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PageInfo {
    /// Zero-based index.
    pub index: PageIx,
    /// The page object, when the raw page tree could be read.
    pub obj: Option<ObjRef>,
    /// `/MediaBox`.
    pub media_box: Rect,
    /// `/CropBox` (defaults to the media box).
    pub crop_box: Rect,
    /// All five boxes with provenance, Media first.
    pub boxes: Vec<PageBox>,
    /// `/Rotate` in degrees clockwise: 0, 90, 180 or 270.
    pub rotate: u16,
    /// `/UserUnit` (default 1).
    pub user_unit: f64,
    /// Object whose `/Resources` applies.
    pub resources_from: Option<ObjRef>,
}

/// Print-production figures read from a page's boxes (ISO 32000-2 §14.11.2), in points.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PrintFigures {
    /// How far the `BleedBox` reaches beyond the `TrimBox` on each side, `[left, bottom, right,
    /// top]`. `None` when the page defines no `BleedBox` of its own (own or inherited).
    pub bleed_each_side: Option<[f64; 4]>,
    /// The bleed when all four sides agree to within 0.01 pt.
    pub uniform_bleed: Option<f64>,
    /// Width and height of the `TrimBox` (which defaults to the `CropBox`).
    pub trim_size: (f64, f64),
    /// How far the `ArtBox` sits inside the `TrimBox` when it is inset by the same amount on all
    /// four sides (to within 0.01 pt). `None` when there is no `ArtBox`, it is not inside the
    /// trim, or the inset differs per side.
    pub art_inset_in_trim: Option<f64>,
    /// The common paper name of [`PageInfo::display_size`] (either orientation, within 1 pt):
    /// `US Letter`, `A4` ... `None` for a size without a common name.
    pub paper_name: Option<&'static str>,
}

/// Two sides are "the same" for print figures when they differ by less than this many points.
const SAME_SIDE: f64 = 0.01;
/// Slack when naming a paper size, in points.
const PAPER_SLACK: f64 = 1.0;

/// Common paper sizes in points, portrait. ISO sizes are the millimetre values converted.
const PAPERS: [(&str, f64, f64); 14] = [
    ("US Letter", 612.0, 792.0),
    ("US Legal", 612.0, 1008.0),
    ("Tabloid", 792.0, 1224.0),
    ("Executive", 522.0, 756.0),
    ("Statement", 396.0, 612.0),
    ("A0", 2383.94, 3370.39),
    ("A1", 1683.78, 2383.94),
    ("A2", 1190.55, 1683.78),
    ("A3", 841.89, 1190.55),
    ("A4", 595.28, 841.89),
    ("A5", 419.53, 595.28),
    ("A6", 297.64, 419.53),
    ("B4", 708.66, 1000.63),
    ("B5", 498.90, 708.66),
];

impl PageInfo {
    pub(crate) fn new(index: PageIx, raw: Option<&RawPage>, engine: Option<&Page<'_>>) -> Self {
        if let Some(raw) = raw {
            let get = |k: BoxKind| raw.boxes.iter().find(|b| b.kind == k).map(|b| b.rect);
            return Self {
                index,
                obj: Some(raw.obj),
                media_box: get(BoxKind::Media).unwrap_or(Rect::new(0.0, 0.0, 0.0, 0.0)),
                crop_box: get(BoxKind::Crop).unwrap_or(Rect::new(0.0, 0.0, 0.0, 0.0)),
                boxes: raw.boxes.clone(),
                rotate: raw.rotate,
                user_unit: raw.user_unit,
                resources_from: raw.resources_from,
            };
        }
        let (media, crop, rotate) = engine.map_or((Rect::new(0.0, 0.0, 612.0, 792.0), None, 0), |p| {
            let rotate = match p.rotation() {
                Rotation::None => 0,
                Rotation::Horizontal => 90,
                Rotation::Flipped => 180,
                Rotation::FlippedHorizontal => 270,
            };
            (rect(p.media_box()), Some(rect(p.crop_box())), rotate)
        });
        let crop = crop.unwrap_or(media);
        let boxes = vec![
            PageBox {
                kind: BoxKind::Media,
                rect: media,
                source: BoxSource::Own,
            },
            PageBox {
                kind: BoxKind::Crop,
                rect: crop,
                source: BoxSource::Own,
            },
        ];
        Self {
            index,
            obj: None,
            media_box: media,
            crop_box: crop,
            boxes,
            rotate,
            user_unit: 1.0,
            resources_from: None,
        }
    }

    /// Boxes that matter for hit-testing: Media and Crop always, the others only when the
    /// file actually defines them (own or inherited).
    pub fn defined_boxes(&self) -> impl Iterator<Item = (BoxKind, Rect)> + '_ {
        self.boxes
            .iter()
            .filter(|b| {
                matches!(b.kind, BoxKind::Media | BoxKind::Crop) || !matches!(b.source, BoxSource::Default(_))
            })
            .map(|b| (b.kind, b.rect))
    }

    /// A box with provenance.
    pub fn page_box(&self, kind: BoxKind) -> Option<&PageBox> {
        self.boxes.iter().find(|b| b.kind == kind)
    }

    /// One-based label used everywhere in the UI.
    pub fn label(&self) -> String {
        format!("Page {}", self.index + 1)
    }

    pub(crate) fn tree_node(&self) -> TreeNode {
        let page = self.index;
        let children = self
            .boxes
            .iter()
            .map(|b| {
                TreeNode::leaf(NodeId::PageBox { page, kind: b.kind }, b.kind.key()).with_detail(format!(
                    "{}{}",
                    fmt_rect(b.rect),
                    source_suffix(b.source)
                ))
            })
            .collect();
        TreeNode::leaf(NodeId::Page(page), self.label())
            .with_detail(format!(
                "{} × {} pt",
                num(self.crop_box.width()),
                num(self.crop_box.height())
            ))
            .with_children(children)
    }

    pub(crate) fn props(&self) -> Props {
        let mut geometry = PropGroup::new("Geometry");
        for b in &self.boxes {
            geometry = geometry.link(
                b.kind.key(),
                format!("{}{}", fmt_rect(b.rect), source_suffix(b.source)),
                NodeId::PageBox {
                    page: self.index,
                    kind: b.kind,
                },
            );
        }
        geometry = geometry.row("Rotate", format!("{}°", self.rotate));
        geometry = geometry.row("UserUnit", num(self.user_unit));
        let (w, h) = self.display_size();
        geometry = geometry.row("Display size", format!("{} × {} pt", num(w), num(h)));
        let mut groups = vec![geometry];
        let mut objects = PropGroup::new("Objects");
        if let Some(obj) = self.obj {
            objects = objects.link("Page object", obj.to_string(), NodeId::Object(obj));
        }
        if let Some(res) = self.resources_from {
            objects = objects.link("Resources from", res.to_string(), NodeId::Object(res));
        }
        if !objects.rows.is_empty() {
            groups.push(objects);
        }
        Props {
            id: NodeId::Page(self.index),
            title: self.label(),
            kind: "page",
            groups,
        }
    }

    /// Bleed, trim size, art inset and paper name of the page. See [`PrintFigures`].
    pub fn print_figures(&self) -> PrintFigures {
        let rect_of = |kind: BoxKind| self.page_box(kind).map_or(self.crop_box, |b| b.rect);
        let defined = |kind: BoxKind| {
            self.page_box(kind)
                .is_some_and(|b| !matches!(b.source, BoxSource::Default(_)))
        };
        let trim = rect_of(BoxKind::Trim);
        let bleed_each_side = defined(BoxKind::Bleed).then(|| {
            let bleed = rect_of(BoxKind::Bleed);
            [
                trim.x0 - bleed.x0,
                trim.y0 - bleed.y0,
                bleed.x1 - trim.x1,
                bleed.y1 - trim.y1,
            ]
        });
        let uniform_bleed = bleed_each_side.and_then(uniform);
        let art_inset_in_trim = defined(BoxKind::Art)
            .then(|| rect_of(BoxKind::Art))
            .filter(|art| trim.contains_rect(art))
            .and_then(|art| {
                uniform([
                    art.x0 - trim.x0,
                    art.y0 - trim.y0,
                    trim.x1 - art.x1,
                    trim.y1 - art.y1,
                ])
            });
        let (w, h) = self.display_size();
        PrintFigures {
            bleed_each_side,
            uniform_bleed,
            trim_size: (trim.width(), trim.height()),
            art_inset_in_trim,
            paper_name: paper_name(w, h),
        }
    }

    /// Crop box size after `/UserUnit` and `/Rotate`, in points.
    pub fn display_size(&self) -> (f64, f64) {
        let (w, h) = (
            self.crop_box.width() * self.user_unit,
            self.crop_box.height() * self.user_unit,
        );
        if self.rotate % 180 == 90 { (h, w) } else { (w, h) }
    }

    pub(crate) fn box_props(&self, kind: BoxKind) -> Option<Props> {
        let b = self.page_box(kind)?;
        let r = b.rect;
        let mut geometry = PropGroup::new("Geometry")
            .row("Rect", fmt_rect(r))
            .row("Width", format!("{} pt", num(r.width())))
            .row("Height", format!("{} pt", num(r.height())));
        geometry = match b.source {
            BoxSource::Own => match self.obj {
                Some(o) => geometry.link("Defined on", format!("page {o}"), NodeId::Object(o)),
                None => geometry.row("Defined on", "page"),
            },
            BoxSource::Inherited(from) => {
                geometry.link("Inherited from", from.to_string(), NodeId::Object(from))
            }
            BoxSource::Default(of) => geometry.link(
                "Default",
                format!("absent; equals {}", of.key()),
                NodeId::PageBox {
                    page: self.index,
                    kind: of,
                },
            ),
        };
        if kind != BoxKind::Media && !self.media_box.contains_rect(&r) {
            geometry = geometry.row("Warning", "extends outside the MediaBox");
        }
        geometry = geometry.link("Page", self.label(), NodeId::Page(self.index));
        Some(Props {
            id: NodeId::PageBox {
                page: self.index,
                kind,
            },
            title: kind.key().to_owned(),
            kind: "page-box",
            groups: vec![geometry],
        })
    }
}

/// The common value of four sides, when they agree.
fn uniform(sides: [f64; 4]) -> Option<f64> {
    sides
        .iter()
        .all(|s| (s - sides[0]).abs() < SAME_SIDE)
        .then_some(sides[0])
}

/// The paper whose portrait or landscape size is within [`PAPER_SLACK`] of `w` x `h`.
fn paper_name(w: f64, h: f64) -> Option<&'static str> {
    let close = |a: f64, b: f64| (a - b).abs() <= PAPER_SLACK;
    PAPERS
        .iter()
        .find(|(_, pw, ph)| (close(w, *pw) && close(h, *ph)) || (close(w, *ph) && close(h, *pw)))
        .map(|(name, ..)| *name)
}

fn source_suffix(source: BoxSource) -> String {
    match source {
        BoxSource::Own => String::new(),
        BoxSource::Inherited(from) => format!(" (inherited from {from})"),
        BoxSource::Default(of) => format!(" (default = {})", of.key()),
    }
}

fn rect(r: hayro_syntax::object::Rect) -> Rect {
    Rect::new(r.x0, r.y0, r.x1, r.y1)
}

/// Formats a rectangle as PDF array syntax, e.g. `[0 0 612 792]`.
pub fn fmt_rect(r: Rect) -> String {
    format!("[{} {} {} {}]", num(r.x0), num(r.y0), num(r.x1), num(r.y1))
}

/// Formats a number the way PDF writers usually do: no trailing zeros.
pub fn num(v: f64) -> String {
    let s = format!("{v:.4}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" { "0".to_owned() } else { s.to_owned() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_drop_trailing_zeros() {
        assert_eq!(num(200.0), "200");
        assert_eq!(num(0.5), "0.5");
        assert_eq!(num(-0.0), "0");
        assert_eq!(num(1.23456), "1.2346");
        assert_eq!(fmt_rect(Rect::new(0.0, 0.0, 612.0, 792.5)), "[0 0 612 792.5]");
    }
}
