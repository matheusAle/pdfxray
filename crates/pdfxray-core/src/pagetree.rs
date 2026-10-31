//! The page tree read from the raw layer: every page's object, its five boxes with where each
//! one came from (own key, inherited from an ancestor `/Pages` node, or the spec default),
//! `/Rotate`, `/UserUnit` and the source of its `/Resources`.

use std::collections::BTreeSet;

use pdfxray_raw::{Dict, Object, RawDocument};
use serde::Serialize;

use crate::{BoxKind, ObjRef, Rect};

/// Deepest `/Pages` nesting followed before giving up.
const MAX_DEPTH: usize = 64;

/// Where a page box value comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case", tag = "kind", content = "from")]
pub enum BoxSource {
    /// Set on the page object itself.
    Own,
    /// Inherited from this ancestor `/Pages` node.
    Inherited(ObjRef),
    /// Absent; ISO 32000-2 §14.11.2 default (the box it falls back to).
    Default(BoxKind),
}

/// A page box and its provenance.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct PageBox {
    /// Which box.
    pub kind: BoxKind,
    /// Its rectangle in page space.
    pub rect: Rect,
    /// Where the value came from.
    pub source: BoxSource,
}

/// A page as found in the raw page tree.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub(crate) struct RawPage {
    /// The page object.
    pub(crate) obj: ObjRef,
    /// All five boxes, Media first.
    pub(crate) boxes: Vec<PageBox>,
    /// `/Rotate` normalised to 0, 90, 180 or 270.
    pub(crate) rotate: u16,
    /// `/Rotate` exactly as written, when it is not a multiple of 90.
    pub(crate) rotate_invalid: Option<i64>,
    /// `/UserUnit` (default 1).
    pub(crate) user_unit: f64,
    /// Object whose `/Resources` applies (the page or an ancestor), if any.
    pub(crate) resources_from: Option<ObjRef>,
}

/// Problems found while walking the page tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PageTreeIssue {
    /// No `/Root` → `/Pages`.
    Missing,
    /// A `/Kids` entry is not a page or pages node, or points back up the tree.
    BadKid {
        /// The node holding the bad entry.
        parent: ObjRef,
    },
    /// Nesting exceeded [`MAX_DEPTH`].
    TooDeep,
}

#[derive(Clone, Default)]
struct Inherited {
    media: Option<(Rect, ObjRef)>,
    crop: Option<(Rect, ObjRef)>,
    rotate: Option<(i64, ObjRef)>,
    resources: Option<ObjRef>,
}

/// Walks `/Root /Pages` depth-first in document order.
pub(crate) fn walk(raw: &RawDocument) -> (Vec<RawPage>, Vec<PageTreeIssue>) {
    let mut pages = Vec::new();
    let mut issues = Vec::new();
    let root = raw
        .trailer()
        .and_then(|t| t.get(b"Root"))
        .and_then(|r| r.value.as_ref_obj())
        .and_then(|r| raw.object(r))
        .and_then(|o| {
            o.value
                .value
                .as_dict()
                .and_then(|d| d.get(b"Pages"))
                .and_then(|p| p.value.as_ref_obj())
        });
    match root {
        Some(pages_ref) => {
            let mut seen = BTreeSet::new();
            visit(
                raw,
                pages_ref,
                &Inherited::default(),
                0,
                &mut seen,
                &mut pages,
                &mut issues,
            );
        }
        None => issues.push(PageTreeIssue::Missing),
    }
    (pages, issues)
}

fn visit(
    raw: &RawDocument,
    node: ObjRef,
    inherited: &Inherited,
    depth: usize,
    seen: &mut BTreeSet<ObjRef>,
    pages: &mut Vec<RawPage>,
    issues: &mut Vec<PageTreeIssue>,
) {
    if depth > MAX_DEPTH {
        issues.push(PageTreeIssue::TooDeep);
        return;
    }
    if !seen.insert(node) {
        issues.push(PageTreeIssue::BadKid { parent: node });
        return;
    }
    let Some(obj) = raw.object(node) else { return };
    let Some(dict) = obj.value.value.as_dict() else {
        return;
    };
    let mut here = inherited.clone();
    if let Some(r) = rect(raw, dict, b"MediaBox") {
        here.media = Some((r, node));
    }
    if let Some(r) = rect(raw, dict, b"CropBox") {
        here.crop = Some((r, node));
    }
    if let Some(v) = int(raw, dict, b"Rotate") {
        here.rotate = Some((v, node));
    }
    if dict.get(b"Resources").is_some() {
        here.resources = Some(node);
    }
    let is_pages =
        dict.get_name(b"Type").is_some_and(|n| n.as_bytes() == b"Pages") || dict.get(b"Kids").is_some();
    if is_pages {
        let kids = dict.get(b"Kids").map(|k| raw.resolve(&k.value).into_owned());
        for kid in kids.as_ref().and_then(Object::as_array).unwrap_or_default() {
            match kid.value.as_ref_obj() {
                Some(k) => visit(raw, k, &here, depth + 1, seen, pages, issues),
                None => issues.push(PageTreeIssue::BadKid { parent: node }),
            }
        }
    } else {
        pages.push(page(raw, node, dict, &here));
    }
}

fn page(raw: &RawDocument, obj: ObjRef, dict: &Dict, inh: &Inherited) -> RawPage {
    let source = |from: ObjRef| {
        if from == obj {
            BoxSource::Own
        } else {
            BoxSource::Inherited(from)
        }
    };
    // ISO 32000-2 §14.11.2: MediaBox is required; Letter is the de facto fallback.
    let (media, media_src) = inh.media.map_or(
        (
            Rect::new(0.0, 0.0, 612.0, 792.0),
            BoxSource::Default(BoxKind::Media),
        ),
        |(r, f)| (r, source(f)),
    );
    let (crop, crop_src) = inh
        .crop
        .map_or((media, BoxSource::Default(BoxKind::Media)), |(r, f)| {
            (r, source(f))
        });
    let mut boxes = vec![
        PageBox {
            kind: BoxKind::Media,
            rect: media,
            source: media_src,
        },
        PageBox {
            kind: BoxKind::Crop,
            rect: crop,
            source: crop_src,
        },
    ];
    for (kind, key) in [
        (BoxKind::Bleed, &b"BleedBox"[..]),
        (BoxKind::Trim, b"TrimBox"),
        (BoxKind::Art, b"ArtBox"),
    ] {
        let b = rect(raw, dict, key).map_or(
            PageBox {
                kind,
                rect: crop,
                source: BoxSource::Default(BoxKind::Crop),
            },
            |r| PageBox {
                kind,
                rect: r,
                source: BoxSource::Own,
            },
        );
        boxes.push(b);
    }
    let raw_rotate = inh.rotate.map_or(0, |(v, _)| v);
    let rotate_invalid = (raw_rotate % 90 != 0).then_some(raw_rotate);
    let rotate = u16::try_from(raw_rotate.div_euclid(90).rem_euclid(4) * 90).unwrap_or(0);
    let user_unit = num(raw, dict, b"UserUnit").filter(|u| *u > 0.0).unwrap_or(1.0);
    RawPage {
        obj,
        boxes,
        rotate,
        rotate_invalid,
        user_unit,
        resources_from: inh.resources,
    }
}

fn rect(raw: &RawDocument, dict: &Dict, key: &[u8]) -> Option<Rect> {
    let value = raw.resolve(&dict.get(key)?.value).into_owned();
    let items = value.as_array()?;
    let nums: Vec<f64> = items
        .iter()
        .filter_map(|item| raw.resolve(&item.value).as_f64())
        .collect();
    match nums.as_slice() {
        [a, b, c, d] => Some(Rect::new(*a, *b, *c, *d)),
        _ => None,
    }
}

fn int(raw: &RawDocument, dict: &Dict, key: &[u8]) -> Option<i64> {
    raw.resolve(&dict.get(key)?.value).as_int()
}

fn num(raw: &RawDocument, dict: &Dict, key: &[u8]) -> Option<f64> {
    raw.resolve(&dict.get(key)?.value).as_f64()
}
