//! What a pair's mutation did, in this crate's own terms.
//!
//! These facts say which objects were renumbered, which element's role, `/Alt` or position
//! changed and to what, which page was inserted, which artifact's text changed. They deliberately
//! say nothing about how a diff should report it: deriving the expected diff from them is the
//! consumer's job, which keeps a test built on them from being circular.

use serde::{Deserialize, Serialize};

/// The declared, machine-readable description of one pair.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MutationFacts {
    /// The pair name (one of [`PAIRS`](super::PAIRS)).
    pub name: String,
    /// One sentence on the mutation.
    pub description: String,
    /// The base document.
    pub a: SideFacts,
    /// The mutated document.
    pub b: SideFacts,
    /// Every change applied to produce B from A, in no particular precedence. Anything not listed
    /// here is unchanged: same roles, texts, `/Alt`, ink and page.
    pub mutations: Vec<Mutation>,
}

/// Facts about one side of a pair.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SideFacts {
    /// Number of pages.
    pub page_count: usize,
    /// Page object numbers in page order.
    pub page_objects: Vec<u32>,
    /// Number of structure elements below the structure tree root.
    pub element_count: usize,
}

/// A structure element, identified across both files.
///
/// `label` is the generator's stable name for the element; it is never written into either file.
/// `a` and `b` are its object numbers (absent on the side where the element does not exist).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ElementRef {
    /// Generator label, stable across both sides.
    pub label: String,
    /// Object number in A, if the element exists there.
    pub a: Option<u32>,
    /// Object number in B, if the element exists there.
    pub b: Option<u32>,
    /// `/S` on the side the element is described from (A when it exists there).
    pub role: String,
    /// Text the element's marked content shows, for leaf text elements.
    pub text: Option<String>,
    /// Page index in A.
    pub page_a: Option<usize>,
    /// Page index in B.
    pub page_b: Option<usize>,
}

/// One object's number in each file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Renumbering {
    /// Generator label of the object.
    pub label: String,
    /// Object number in A.
    pub a: u32,
    /// Object number in B.
    pub b: u32,
}

/// How one page's content was rewritten.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContentRewrite {
    /// Page index (same on both sides).
    pub page: usize,
    /// Content stream objects of the page in A.
    pub a_streams: Vec<u32>,
    /// Content stream objects of the page in B, in `/Contents` order.
    pub b_streams: Vec<u32>,
    /// Form `XObject`s that now hold ink the page stream painted directly in A.
    pub b_forms: Vec<u32>,
    /// `(element label, MCID in A, MCID in B)` for every marked-content sequence on the page.
    pub mcids: Vec<(String, i32, i32)>,
}

/// A page-number artifact whose text changed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactText {
    /// Page index in A.
    pub page_a: usize,
    /// Page index in B.
    pub page_b: usize,
    /// `/Type` of the artifact.
    pub artifact_type: String,
    /// `/Subtype` of the artifact.
    pub subtype: String,
    /// Text shown in A.
    pub a_text: String,
    /// Text shown in B.
    pub b_text: String,
}

/// Untagged text that reads differently on corresponding pages.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UntaggedText {
    /// Page index in A.
    pub page_a: usize,
    /// Page index in B.
    pub page_b: usize,
    /// Text shown in A.
    pub a_text: String,
    /// Text shown in B, at the same origin and size.
    pub b_text: String,
}

/// One declared change.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Mutation {
    /// Every listed object has a different number in B; nothing else about it changed.
    Renumbered {
        /// Old and new number of every indirect object.
        objects: Vec<Renumbering>,
        /// Whether the objects were also written in a different byte order.
        reordered_in_file: bool,
    },
    /// Page content streams were rewritten; the painted result is identical.
    ContentRewritten {
        /// One entry per page.
        pages: Vec<ContentRewrite>,
    },
    /// All page content (tagged ink, artifacts and `/BBox` attributes) translated.
    Translated {
        /// Horizontal offset in points, PDF user space.
        dx: f64,
        /// Vertical offset in points, PDF user space (y grows upwards, so negative is down).
        dy: f64,
    },
    /// Two siblings swapped places in their parent's `/K`; ink unchanged.
    SiblingsSwapped {
        /// The common parent.
        parent: ElementRef,
        /// The sibling that came first in A.
        first: ElementRef,
        /// The sibling that came second in A.
        second: ElementRef,
    },
    /// One element's ink (and therefore its box) moved.
    ElementMoved {
        /// The element.
        element: ElementRef,
        /// Horizontal offset in points.
        dx: f64,
        /// Vertical offset in points (negative is down).
        dy: f64,
    },
    /// `/Alt` removed from an element.
    AltRemoved {
        /// The element.
        element: ElementRef,
        /// The `/Alt` text A had.
        alt: String,
    },
    /// An element's `/S` changed; its ink and content-stream tag did not.
    RoleChanged {
        /// The element.
        element: ElementRef,
        /// `/S` in A.
        from: String,
        /// `/S` in B.
        to: String,
    },
    /// A page inserted into B; pages from `index` on are shifted by one.
    PageInserted {
        /// Page index of the new page in B.
        index: usize,
        /// Page object of the new page in B.
        page_object: u32,
        /// Structure elements that exist only on the new page, in `/K` order.
        elements: Vec<ElementRef>,
    },
    /// Artifact texts changed.
    ArtifactTextChanged {
        /// One entry per changed artifact.
        changes: Vec<ArtifactText>,
    },
    /// Text outside any structure (the file is untagged) changed, at the same origin and size.
    UntaggedTextChanged {
        /// One entry per changed line of text.
        changes: Vec<UntaggedText>,
    },
    /// An element and its subtree removed from B; nothing moved into its place.
    ElementRemoved {
        /// The removed element.
        element: ElementRef,
        /// Its parent.
        parent: ElementRef,
        /// Its index in the parent's `/K` in A.
        index_in_parent: usize,
        /// Its descendants, also removed.
        descendants: Vec<ElementRef>,
    },
}
