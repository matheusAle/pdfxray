//! What got fixed and what broke: the diagnostics of two documents, compared through the
//! pairing.
//!
//! A diagnostic has no identity across files any more than an element has: its message names
//! object numbers, its index is its place in a list. So findings are compared as a multiset
//! keyed on **what** (the diagnostic code and severity, or the veraPDF rule) and **where**: the
//! element pair when the finding concerns a structure element, the page pair when it concerns a
//! page or something on one, the document otherwise. A key found more often in A than in B was
//! fixed that many times; more often in B, introduced. Which of several equal-keyed findings is
//! reported is the side's own order, so the result is deterministic and B with A mirrors A with B.
//!
//! The document model's own diagnostics are always compared. veraPDF findings only exist when a
//! run is attached to both sides, and are compared only when both ran the same profile.

use std::collections::{BTreeMap, HashMap};

use serde::Serialize;

use super::{Pairing, Side};
use crate::{Document, NodeId, ObjRef, PageIx, Severity};

/// The diagnostics of A against those of B.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize)]
pub struct DiagnosticsDiff {
    /// The document model's own diagnostics (syntax, page tree, structure tree checks).
    pub core: FindingsDiff,
    /// veraPDF findings.
    pub validation: ValidationDiff,
}

impl DiagnosticsDiff {
    /// True when nothing was fixed or introduced.
    pub fn is_unchanged(&self) -> bool {
        self.core.is_unchanged()
            && match &self.validation {
                ValidationDiff::Compared(f) => f.is_unchanged(),
                _ => true,
            }
    }
}

/// veraPDF findings compared, or why they were not.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize)]
#[serde(rename_all = "snake_case", tag = "state")]
pub enum ValidationDiff {
    /// Neither side has a validation run attached.
    #[default]
    NotRun,
    /// Only one side has a run: there is nothing to compare it with.
    OneSide {
        /// The side that has one.
        side: Side,
    },
    /// The two runs used different profiles, so their findings are not comparable.
    ProfilesDiffer {
        /// A's profile.
        a: String,
        /// B's profile.
        b: String,
    },
    /// Both sides ran the same profile.
    Compared(FindingsDiff),
}

/// Findings in one side and not the other.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize)]
pub struct FindingsDiff {
    /// Findings of A that B no longer has.
    pub fixed: Vec<FindingChange>,
    /// Findings of B that A did not have.
    pub introduced: Vec<FindingChange>,
    /// Findings on both sides, over the whole document.
    pub unchanged: usize,
}

impl FindingsDiff {
    /// True when nothing was fixed or introduced.
    pub fn is_unchanged(&self) -> bool {
        self.fixed.is_empty() && self.introduced.is_empty()
    }
}

/// One finding only one side has.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FindingChange {
    /// The diagnostic code, or the veraPDF rule id.
    pub code: String,
    /// The diagnostic's severity; veraPDF findings have none.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub severity: Option<Severity>,
    /// The message, as the side that has the finding words it.
    pub message: String,
    /// What it concerns.
    pub on: Anchor,
}

/// What a finding concerns, in terms both sides share.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum Anchor {
    /// A structure element: `pair` indexes [`Pairing::pairs`].
    Element {
        /// The pair.
        pair: usize,
        /// The element in A.
        #[serde(skip_serializing_if = "Option::is_none")]
        a: Option<NodeId>,
        /// The element in B.
        #[serde(skip_serializing_if = "Option::is_none")]
        b: Option<NodeId>,
    },
    /// A page, or something on it.
    Page {
        /// The page of A.
        #[serde(skip_serializing_if = "Option::is_none")]
        a: Option<PageIx>,
        /// The page of B.
        #[serde(skip_serializing_if = "Option::is_none")]
        b: Option<PageIx>,
    },
    /// The document as a whole, or an object neither an element nor on a page.
    Document,
}

/// Where a finding is, as a key both sides compute alike. Ordered document first, then
/// elements in pairing order, then pages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
enum Where {
    Document,
    Element(usize),
    Page(usize),
}

/// The pairing seen from one side: which pair holds each element, which page pair each page.
struct Locate<'a> {
    elements: HashMap<ObjRef, usize>,
    page_objects: HashMap<ObjRef, PageIx>,
    pages: HashMap<PageIx, usize>,
    pairing: &'a Pairing,
}

impl<'a> Locate<'a> {
    fn new(doc: &Document, pairing: &'a Pairing, side: Side) -> Self {
        let pick = |a: Option<&'a NodeId>, b: Option<&'a NodeId>| if side == Side::A { a } else { b };
        let elements = pairing
            .pairs
            .iter()
            .enumerate()
            .filter_map(|(i, p)| match pick(p.a.as_ref(), p.b.as_ref()) {
                Some(NodeId::StructElem(r)) => Some((*r, i)),
                _ => None,
            })
            .collect();
        let page_objects = (0..doc.page_count())
            .filter_map(|p| PageIx::try_from(p).ok())
            .filter_map(|p| Some((doc.page(p)?.obj?, p)))
            .collect();
        let pages = pairing
            .pages
            .iter()
            .enumerate()
            .filter_map(|(i, m)| Some((if side == Side::A { m.a } else { m.b }?, i)))
            .collect();
        Self {
            elements,
            page_objects,
            pages,
            pairing,
        }
    }

    fn object(&self, r: ObjRef) -> Where {
        if let Some(&i) = self.elements.get(&r) {
            return Where::Element(i);
        }
        self.page_objects
            .get(&r)
            .map_or(Where::Document, |&p| self.page(p))
    }

    fn page(&self, p: PageIx) -> Where {
        self.pages.get(&p).map_or(Where::Document, |&i| Where::Page(i))
    }

    fn node(&self, node: Option<&NodeId>) -> Where {
        match node {
            Some(NodeId::StructElem(r) | NodeId::Object(r)) => self.object(*r),
            Some(
                NodeId::Page(page)
                | NodeId::PageBox { page, .. }
                | NodeId::MarkedContent { page, .. }
                | NodeId::Artifact { page, .. }
                | NodeId::Op { page, .. }
                | NodeId::Glyph { page, .. },
            ) => self.page(*page),
            Some(NodeId::Resource { owner, .. }) => match self.page_objects.get(owner) {
                Some(&p) => self.page(p),
                None => Where::Document,
            },
            _ => Where::Document,
        }
    }

    fn anchor(&self, w: Where) -> Anchor {
        match w {
            Where::Document => Anchor::Document,
            Where::Element(i) => {
                let p = &self.pairing.pairs[i];
                Anchor::Element {
                    pair: i,
                    a: p.a.clone(),
                    b: p.b.clone(),
                }
            }
            Where::Page(i) => {
                let m = &self.pairing.pages[i];
                Anchor::Page { a: m.a, b: m.b }
            }
        }
    }
}

/// One finding, reduced to what is compared and what is shown.
struct Item {
    code: String,
    severity: Option<Severity>,
    message: String,
    at: Where,
}

type Key = (Where, String, Option<Severity>);

/// The multiset comparison of two sides' findings.
fn compare(a: &[Item], b: &[Item], la: &Locate<'_>) -> FindingsDiff {
    let mut groups: BTreeMap<Key, (Vec<&Item>, Vec<&Item>)> = BTreeMap::new();
    for x in a {
        groups
            .entry((x.at, x.code.clone(), x.severity))
            .or_default()
            .0
            .push(x);
    }
    for x in b {
        groups
            .entry((x.at, x.code.clone(), x.severity))
            .or_default()
            .1
            .push(x);
    }
    let mut out = FindingsDiff::default();
    let change = |x: &Item| FindingChange {
        code: x.code.clone(),
        severity: x.severity,
        message: x.message.clone(),
        on: la.anchor(x.at),
    };
    for (in_a, in_b) in groups.values() {
        let kept = in_a.len().min(in_b.len());
        out.unchanged += kept;
        out.fixed.extend(in_a[kept..].iter().map(|x| change(x)));
        out.introduced.extend(in_b[kept..].iter().map(|x| change(x)));
    }
    out
}

fn core_items(doc: &Document, locate: &Locate<'_>) -> Vec<Item> {
    doc.diagnostics
        .iter()
        .map(|d| Item {
            code: d.code.to_owned(),
            severity: Some(d.severity),
            message: d.message.clone(),
            at: d.object.map_or(Where::Document, |r| locate.object(r)),
        })
        .collect()
}

fn validation(a: &Document, b: &Document, la: &Locate<'_>, lb: &Locate<'_>) -> ValidationDiff {
    let (run_a, run_b) = match (a.validation(), b.validation()) {
        (None, None) => return ValidationDiff::NotRun,
        (Some(_), None) => return ValidationDiff::OneSide { side: Side::A },
        (None, Some(_)) => return ValidationDiff::OneSide { side: Side::B },
        (Some(x), Some(y)) => (x, y),
    };
    if run_a.profile != run_b.profile {
        return ValidationDiff::ProfilesDiffer {
            a: run_a.profile.clone(),
            b: run_b.profile.clone(),
        };
    }
    let items = |run: &crate::ValidationRun, locate: &Locate<'_>| -> Vec<Item> {
        run.findings
            .iter()
            .map(|f| Item {
                code: f.rule.clone(),
                severity: None,
                message: f.message.clone().unwrap_or_else(|| f.description.clone()),
                at: locate.node(f.target.as_ref()),
            })
            .collect()
    };
    ValidationDiff::Compared(compare(&items(&run_a, la), &items(&run_b, lb), la))
}

/// Compares the diagnostics of `a` and `b` through `pairing`, which must pair `a` with `b`.
///
/// A finding is compared on its code and severity (for veraPDF, its rule) and on what it
/// concerns: the element pair, the page pair, or the document. Messages are not compared, as
/// they name object numbers. Equal-keyed findings beyond the other side's count are fixed (only
/// in A) or introduced (only in B). veraPDF findings are compared only when both documents
/// carry a run of the same profile.
pub fn diagnostics(a: &Document, b: &Document, pairing: &Pairing) -> DiagnosticsDiff {
    let (la, lb) = (Locate::new(a, pairing, Side::A), Locate::new(b, pairing, Side::B));
    DiagnosticsDiff {
        core: compare(&core_items(a, &la), &core_items(b, &lb), &la),
        validation: validation(a, b, &la, &lb),
    }
}
