//! Comparing two documents: which structure elements correspond, and what changed between them.
//!
//! A diff is a **pairing**, not a picture. Every structure element of either side is in exactly
//! one [`Pair`] with a [`Status`]; geometry is one attribute of a pair. Object numbers play no
//! part: elements are matched by what they say and where they are, so a file whose objects were
//! all renumbered pairs with its original as identical.
//!
//! # How elements are paired
//!
//! Both documents' elements are read in document order (depth-first over `/K`, the order
//! [`SpeechPreset::Structure`](crate::SpeechPreset::Structure) walks). Each element gets a
//! ladder of keys, built bottom-up once per document: `role + text`, `text`, `role + box`,
//! `geometry`. The two sequences are aligned by patience anchoring on keys unique on both sides,
//! recursing into the gaps, with a bounded full alignment inside gaps that have no anchor at any
//! rung, and a move pass over what is left. Page correspondence is *derived* from where paired
//! elements live; the global offset is estimated from the pairs and every per-pair delta is the
//! residual after removing it.
//!
//! # Two payload layers
//!
//! Each pair's changes are reported twice from one comparison: as typed [`FieldChange`]s (role,
//! standard type, level, alt, actual text, language, text, box, page) and as the exhaustive
//! [`RowChange`]s of the element's inspector rows. The typed fields backed by a row are read off
//! the row comparison, so the two layers cannot disagree.
//!
//! # What is refused rather than guessed
//!
//! An encrypted side is paired on structure only (its content is not interpreted), and the
//! output says so. Pairs whose content sits on an [`Alignment::Degraded`](crate::capture::Alignment)
//! page, whose text was matched without `/ToUnicode`, or that were matched inside an anchorless
//! gap say so in their [`Confidence`]. Content streams are never compared across files.

mod align;
mod keys;
mod pages;
mod pair;
mod rows;

use serde::Serialize;

pub use self::pair::{DocKeys, pair, pair_keyed};
use crate::{NodeId, PageIx, Rect, UnicodeSource};

/// Largest residual translation, in points, that still counts as "not moved".
pub const TOLERANCE: f64 = 1.0;

/// The result of comparing document A with document B. See the [module](self) documentation.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Pairing {
    /// Every element of either side, in one pair each: A's document order, with the elements
    /// only B has placed after the pair they follow in B.
    pub pairs: Vec<Pair>,
    /// Which page of A corresponds to which page of B, derived from the pairs.
    pub pages: Vec<PageMatch>,
    /// The translation of B relative to A, estimated from the pairs; `None` when there is none.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub offset: Option<Offset>,
    /// Largest residual translation that still counts as not moved, points.
    pub tolerance: f64,
    /// Facts about each side.
    pub sides: Sides,
    /// What limited the comparison.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<Note>,
}

impl Pairing {
    /// Number of pairs with each status.
    pub fn summary(&self) -> Summary {
        let mut s = Summary::default();
        for p in &self.pairs {
            *match p.status {
                Status::Same => &mut s.same,
                Status::Modified => &mut s.modified,
                Status::Moved => &mut s.moved,
                Status::Added => &mut s.added,
                Status::Removed => &mut s.removed,
            } += 1;
        }
        s
    }
}

/// Pair counts by status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
pub struct Summary {
    /// [`Status::Same`].
    pub same: usize,
    /// [`Status::Modified`].
    pub modified: usize,
    /// [`Status::Moved`].
    pub moved: usize,
    /// [`Status::Added`].
    pub added: usize,
    /// [`Status::Removed`].
    pub removed: usize,
}

impl Summary {
    /// True when nothing was modified, moved, added or removed.
    pub fn is_unchanged(&self) -> bool {
        self.modified + self.moved + self.added + self.removed == 0
    }
}

/// The two documents compared.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Sides {
    /// Document A, the one compared against.
    pub a: SideInfo,
    /// Document B.
    pub b: SideInfo,
}

/// Facts about one side of a comparison.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SideInfo {
    /// The document's name.
    pub name: String,
    /// Pages.
    pub pages: usize,
    /// Structure elements.
    pub elements: usize,
    /// True when the file is encrypted: its content is not interpreted, so it is compared on
    /// structure only (no text, no geometry).
    pub structure_only: bool,
    /// Pages whose capture is [`Alignment::Degraded`](crate::capture::Alignment::Degraded).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub degraded_pages: Vec<PageIx>,
}

/// Something that limited a comparison.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum Note {
    /// A side is encrypted and was compared on structure only.
    StructureOnly {
        /// `a` or `b`.
        side: Side,
    },
    /// Gaps with no anchor at any rung were too large for the bounded alignment; their
    /// elements were left to the move pass and may show as removed and added.
    UnboundedGaps {
        /// How many.
        count: usize,
    },
}

/// One side of a comparison.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Side {
    /// Document A.
    A,
    /// Document B.
    B,
}

/// What a pair pairs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PairKind {
    /// Structure elements.
    Element,
    /// Painted content outside the structure tree (artifacts, untagged content). Not produced
    /// yet; reserved so that adding it does not change the format.
    Ink,
}

/// How a pair differs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    /// Nothing changed beyond the global offset.
    Same,
    /// Fields changed; it is in the same place.
    Modified,
    /// It changed place: reordered, moved to another page, or moved on the page beyond
    /// [`TOLERANCE`] after the global offset is removed. Fields may have changed too. A
    /// container that holds no content itself is never moved by geometry alone: its box is the
    /// union of its children's, which carry the change.
    Moved,
    /// Only B has it.
    Added,
    /// Only A has it.
    Removed,
}

impl Status {
    /// The status of the same pair seen from B: `Added` ↔ `Removed`.
    #[must_use]
    pub fn mirrored(self) -> Self {
        match self {
            Self::Added => Self::Removed,
            Self::Removed => Self::Added,
            other => other,
        }
    }
}

/// One element of A, one of B, or one of each that correspond.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Pair {
    /// What is paired.
    pub kind: PairKind,
    /// How the two differ.
    pub status: Status,
    /// The element of A.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub a: Option<NodeId>,
    /// The element of B.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub b: Option<NodeId>,
    /// The role of the element (B's when both exist and differ is in [`Pair::changed`]).
    pub role: String,
    /// The start of its text, for display.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub text: String,
    /// Page of the element in A.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page_a: Option<PageIx>,
    /// Page of the element in B.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page_b: Option<PageIx>,
    /// Box of the element in A, page space.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bbox_a: Option<Rect>,
    /// Box of the element in B, page space.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bbox_b: Option<Rect>,
    /// The translation from A's box to B's after the global offset is removed, points, when
    /// both boxes are on corresponding pages.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delta: Option<(f64, f64)>,
    /// True when the element changed place in the reading order.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub reordered: bool,
    /// The typed changes.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub changed: Vec<FieldChange>,
    /// Every inspector row that changed.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub rows: Vec<RowChange>,
    /// How far the pairing can be trusted.
    pub confidence: Confidence,
}

/// A typed field of an element.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Field {
    /// `/S` as written.
    Role,
    /// The standard type `/S` resolves to.
    Standard,
    /// The heading level (`H1`..`Hn`).
    Level,
    /// `/Alt`.
    Alt,
    /// `/ActualText`.
    ActualText,
    /// `/Lang` (the element's own).
    Lang,
    /// The text its own marked content paints.
    Text,
    /// Its box, beyond [`TOLERANCE`] after the global offset is removed. Not reported for a
    /// container that holds no content itself (see [`Status::Moved`]).
    Bbox,
    /// Its page, when it is not the page corresponding to A's.
    Page,
}

/// One typed field that differs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FieldChange {
    /// The field.
    pub field: Field,
    /// Its value in A; `None` when A does not set it.
    pub a: Option<String>,
    /// Its value in B; `None` when B does not set it.
    pub b: Option<String>,
}

/// One inspector row that differs. Rows that name objects (object numbers, MCIDs, diagnostics)
/// are left out: they change whenever a file is rewritten and say nothing about the element.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RowChange {
    /// The group, e.g. `Element`.
    pub group: String,
    /// The row label, e.g. `Alt`.
    pub key: String,
    /// The value in A; `None` when A has no such row.
    pub a: Option<String>,
    /// The value in B; `None` when B has no such row.
    pub b: Option<String>,
}

/// Which rung of the key ladder paired two elements.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Rung {
    /// Same role and the same text.
    RoleText,
    /// The same text.
    Text,
    /// Same role and the same box (to 2 pt).
    RoleBox,
    /// The same box (to 4 pt).
    Geometry,
}

/// How a pair was found.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Method {
    /// A key unique on both sides, or equal keys at the edge of a range.
    Anchor,
    /// The bounded full alignment of a gap with no unique key: the weakest evidence.
    Bounded,
    /// The move pass: the same unique text, out of order.
    Moved,
    /// Neither text nor box matched, but most of their children are paired with each other's:
    /// a container whose content changed.
    Children,
}

/// How far a pair can be trusted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Confidence {
    /// The rung the two elements were paired on; `None` for an element of one side only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rung: Option<Rung>,
    /// How they were paired.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub method: Option<Method>,
    /// Where A's text came from: the weakest source among its glyphs; `None` when it paints no
    /// text.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unicode_a: Option<UnicodeSource>,
    /// Where B's text came from.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unicode_b: Option<UnicodeSource>,
    /// True when A's content is on a degraded page.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub degraded_a: bool,
    /// True when B's content is on a degraded page.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub degraded_b: bool,
}

/// The pages of A and B that correspond.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PageMatch {
    /// The page of A; `None` for a page only B has.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub a: Option<PageIx>,
    /// The page of B; `None` for a page only A has.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub b: Option<PageIx>,
    /// Pairs of elements between the two pages: the evidence the correspondence rests on. A
    /// correspondence with none rests on identical residue (see [`Residue`]).
    pub pairs: usize,
    /// The painted content outside every structure element.
    pub residue: Residue,
}

impl PageMatch {
    /// True when the pages correspond under the same index with nothing outside the structure
    /// tree changed.
    pub fn is_unchanged(&self) -> bool {
        self.a.is_some() && self.a == self.b && self.residue.only_a == 0 && self.residue.only_b == 0
    }
}

/// The painted content of a page outside every structure element (artifacts and untagged
/// content), compared as a multiset of painted items by kind and box (to 1 pt, after the
/// global offset is removed). Paint order is ignored: the same ink written by different
/// operators is the same residue.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
pub struct Residue {
    /// Items on A's page.
    pub a: usize,
    /// Items on B's page.
    pub b: usize,
    /// Items only A's page has.
    pub only_a: usize,
    /// Items only B's page has.
    pub only_b: usize,
}

/// The translation of B relative to A, reported once.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Offset {
    /// Horizontal, points.
    pub dx: f64,
    /// Vertical, points.
    pub dy: f64,
    /// Pairs it was estimated from.
    pub pairs: usize,
}

#[cfg(test)]
mod tests;
