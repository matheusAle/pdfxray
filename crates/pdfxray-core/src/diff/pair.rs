//! Putting a pairing together: keys, alignment, page correspondence, offset, payloads.

use std::collections::{BTreeMap, BTreeSet};

use rayon::prelude::*;

use super::align::{self, Keys, Match};
use super::keys::{self, ElemKey};
use super::pages::{self, Residues};
use super::{
    Confidence, DiagnosticsDiff, Field, FieldChange, FindingsDiff, Method, Note, Offset, PageMatch, Pair,
    PairKind, Pairing, Rung, Side, SideInfo, Sides, Status, TOLERANCE, ValidationDiff, rows,
};
use crate::capture::Alignment;
use crate::numeric::usize_from_u32;
use crate::{Document, EngineError, PageIx, Rect, fmt_rect};

/// Smallest global offset reported, points: below it the two sides are registered.
const OFFSET_MIN: f64 = 0.01;

/// The order pages are visited in while their residue is read. The result never depends on it;
/// the tests check that.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Visit {
    Forward,
    #[cfg(test)]
    Reverse,
}

/// The keys of one document's structure elements: the per-document half of a pairing, built
/// bottom-up in one pass. Build it once per document and reuse it while the document does not
/// change; a pairing only needs both sides' keys and the documents.
#[derive(Debug, Clone)]
pub struct DocKeys(Vec<ElemKey>);

impl DocKeys {
    /// Builds the keys of every structure element of `doc`.
    pub fn build(doc: &Document) -> Self {
        Self(keys::build(doc))
    }

    /// Number of structure elements keyed.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// True when the document has no structure elements.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// Pairs the structure elements of `a` with those of `b`. See the [module](super)
/// documentation.
pub fn pair(a: &Document, b: &Document) -> Pairing {
    let (ka, kb) = crate::parallel::install(|| rayon::join(|| DocKeys::build(a), || DocKeys::build(b)));
    pair_keyed(a, &ka, b, &kb)
}

/// [`pair`], with keys built beforehand. `ka` must have been built from `a` and `kb` from `b`.
pub fn pair_keyed(a: &Document, ka: &DocKeys, b: &Document, kb: &DocKeys) -> Pairing {
    pair_with(a, &ka.0, b, &kb.0, Visit::Forward)
}

pub(super) fn pair_with(a: &Document, ka: &[ElemKey], b: &Document, kb: &[ElemKey], visit: Visit) -> Pairing {
    let seq = |k: &[ElemKey]| k.iter().map(|e| e.keys).collect::<Vec<Keys>>();
    let parents = |k: &[ElemKey]| k.iter().map(|e| e.parent).collect::<Vec<_>>();
    let roles = |k: &[ElemKey]| k.iter().map(|e| keys::role_key(e.role())).collect::<Vec<_>>();
    let (pa, pb, ra, rb) = (parents(ka), parents(kb), roles(ka), roles(kb));
    let align_keys = |sa: &[Keys], sb: &[Keys]| {
        align::pair_parents(
            align::align(sa, sb),
            align::Tree {
                parent: &pa,
                role: &ra,
            },
            align::Tree {
                parent: &pb,
                role: &rb,
            },
        )
    };
    let order = |doc: &Document| page_order(doc, visit);
    let residue_a = pages::residues(a, &order(a), (0.0, 0.0));
    let residue_b = pages::residues(b, &order(b), (0.0, 0.0));

    let first = align_keys(&seq(ka), &seq(kb));
    let (correspondence, a_to_b) = correspond(&first.matches, ka, kb, &residue_a, &residue_b, a.page_count());
    let offset = estimate_offset(&first.matches, ka, kb, &a_to_b);
    let shift = offset.map_or((0.0, 0.0), |o| (o.dx, o.dy));
    // The offset comes from text matches, which no translation affects; the box rungs were
    // keyed before it was known, so a shifted figure keys differently on each side. With an
    // offset, both sides are re-keyed halfway towards each other (so B with A mirrors A with B
    // exactly) and aligned again.
    let (aligned, correspondence, a_to_b, residue_b) = if offset.is_some() {
        let half = (shift.0 / 2.0, shift.1 / 2.0);
        let aligned = align_keys(
            &keys::registered(ka, half),
            &keys::registered(kb, (-half.0, -half.1)),
        );
        let residue_b = pages::residues(b, &order(b), shift);
        let (correspondence, a_to_b) =
            correspond(&aligned.matches, ka, kb, &residue_a, &residue_b, a.page_count());
        (aligned, correspondence, a_to_b, residue_b)
    } else {
        (first, correspondence, a_to_b, residue_b)
    };
    let empty = Residues::new();
    let page_matches = correspondence
        .iter()
        .map(|&(pa, pb, n)| {
            let ra = pa
                .and_then(|p| residue_a.get(usize_from_u32(p)))
                .unwrap_or(&empty);
            let rb = pb
                .and_then(|p| residue_b.get(usize_from_u32(p)))
                .unwrap_or(&empty);
            PageMatch {
                a: pa,
                b: pb,
                pairs: n,
                residue: pages::compare(ra, rb),
            }
        })
        .collect();

    let by_a: BTreeMap<usize, &Match> = aligned.matches.iter().map(|m| (m.a, m)).collect();
    let rows = aligned.ordered(ka.len());
    // Each pair compares its own two elements: independent, so spread over the pool. The
    // result keeps `rows` order.
    let pairs = crate::parallel::install(|| {
        rows.into_par_iter()
            .filter_map(|row| match row {
                (Some(i), Some(_)) => {
                    let m = by_a.get(&i)?;
                    Some(matched(a, b, &ka[m.a], &kb[m.b], m, shift, &a_to_b))
                }
                (Some(i), None) => Some(unpaired(&ka[i], Side::A)),
                (None, Some(j)) => Some(unpaired(&kb[j], Side::B)),
                (None, None) => None,
            })
            .collect()
    });

    let mut notes = Vec::new();
    for (side, doc) in [(Side::A, a), (Side::B, b)] {
        if structure_only(doc) {
            notes.push(Note::StructureOnly { side });
        }
    }
    if aligned.unbounded_gaps > 0 {
        notes.push(Note::UnboundedGaps {
            count: aligned.unbounded_gaps,
        });
    }
    let mut pairing = Pairing {
        pairs,
        pages: page_matches,
        offset,
        tolerance: TOLERANCE,
        sides: Sides {
            a: side_info(a, ka.len()),
            b: side_info(b, kb.len()),
        },
        notes,
        diagnostics: DiagnosticsDiff {
            core: FindingsDiff::default(),
            validation: ValidationDiff::NotRun,
        },
    };
    pairing.diagnostics = super::diagnostics(a, b, &pairing);
    pairing
}

/// `(page of A, page of B, element pairs between them)`.
type PagePair = (Option<PageIx>, Option<PageIx>, usize);

/// The page correspondence `matches` give: the page pairs, and for each page of A its partner.
fn correspond(
    matches: &[Match],
    ka: &[ElemKey],
    kb: &[ElemKey],
    residue_a: &[Residues],
    residue_b: &[Residues],
    a_pages: usize,
) -> (Vec<PagePair>, Vec<Option<PageIx>>) {
    let mut votes: BTreeMap<(PageIx, PageIx), usize> = BTreeMap::new();
    for m in matches {
        if let (Some((pa, _)), Some((pb, _))) = (ka[m.a].placed, kb[m.b].placed) {
            *votes.entry((pa, pb)).or_default() += 1;
        }
    }
    let tagged =
        |k: &[ElemKey]| -> BTreeSet<PageIx> { k.iter().filter_map(|e| e.placed.map(|(p, _)| p)).collect() };
    let correspondence = pages::correspond(&votes, residue_a, residue_b, [&tagged(ka), &tagged(kb)]);
    let mut a_to_b: Vec<Option<PageIx>> = vec![None; a_pages];
    for &(pa, pb, _) in &correspondence {
        if let (Some(pa), Some(pb)) = (pa, pb)
            && let Some(slot) = a_to_b.get_mut(usize_from_u32(pa))
        {
            *slot = Some(pb);
        }
    }
    (correspondence, a_to_b)
}

/// Every page of `doc`, in the order `visit` asks for.
fn page_order(doc: &Document, visit: Visit) -> Vec<PageIx> {
    let pages = (0..doc.page_count()).filter_map(|p| PageIx::try_from(p).ok());
    match visit {
        Visit::Forward => pages.collect(),
        #[cfg(test)]
        Visit::Reverse => pages.rev().collect(),
    }
}

fn structure_only(doc: &Document) -> bool {
    doc.engine_error() == Some(EngineError::Encrypted)
}

fn side_info(doc: &Document, elements: usize) -> SideInfo {
    SideInfo {
        name: doc.name.clone(),
        pages: doc.page_count(),
        elements,
        structure_only: structure_only(doc),
        degraded_pages: (0..doc.page_count())
            .filter_map(|p| PageIx::try_from(p).ok())
            .filter(|&p| {
                doc.captured(p)
                    .is_some_and(|c| matches!(c.alignment, Alignment::Degraded { .. }))
            })
            .collect(),
    }
}

/// Rounds to a thousandth of a point, without a negative zero, so output is stable.
fn round3(v: f64) -> f64 {
    let r = (v * 1000.0).round() / 1000.0;
    if r == 0.0 { 0.0 } else { r }
}

fn centre(r: Rect) -> (f64, f64) {
    (f64::midpoint(r.x0, r.x1), f64::midpoint(r.y0, r.y1))
}

fn median(mut v: Vec<f64>) -> f64 {
    v.sort_unstable_by(f64::total_cmp);
    let n = v.len();
    if n % 2 == 1 {
        v[n / 2]
    } else {
        f64::midpoint(v[n / 2 - 1], v[n / 2])
    }
}

/// Fewest pairs a global offset is estimated from.
const OFFSET_MIN_PAIRS: usize = 3;

/// The translation most pairs share: the per-axis median, reported only when a strict majority
/// of the deltas lies within [`TOLERANCE`] of it. Two paragraphs moved by different amounts have
/// a median, but no common offset; reporting it would turn both moves into residuals.
fn consensus_offset(deltas: &[(f64, f64)]) -> Option<(f64, f64)> {
    if deltas.len() < OFFSET_MIN_PAIRS {
        return None;
    }
    let x = median(deltas.iter().map(|d| d.0).collect());
    let y = median(deltas.iter().map(|d| d.1).collect());
    let agreeing = deltas
        .iter()
        .filter(|d| (d.0 - x).hypot(d.1 - y) <= TOLERANCE)
        .count();
    (agreeing * 2 > deltas.len()).then_some((round3(x), round3(y)))
}

/// The common translation of the pairs matched on their text between corresponding pages.
/// Geometry-matched pairs are left out: they matched *because* they did not move.
fn estimate_offset(
    matches: &[Match],
    ka: &[ElemKey],
    kb: &[ElemKey],
    a_to_b: &[Option<PageIx>],
) -> Option<Offset> {
    let mut deltas = Vec::new();
    for m in matches.iter().filter(|m| m.rung <= 1) {
        if let (Some((pa, ra)), Some((pb, rb))) = (ka[m.a].placed, kb[m.b].placed)
            && a_to_b.get(usize_from_u32(pa)).copied().flatten() == Some(pb)
        {
            let (ca, cb) = (centre(ra), centre(rb));
            deltas.push((cb.0 - ca.0, cb.1 - ca.1));
        }
    }
    let (dx, dy) = consensus_offset(&deltas)?;
    (dx.hypot(dy) >= OFFSET_MIN).then_some(Offset {
        dx,
        dy,
        pairs: deltas.len(),
    })
}

fn rung(r: usize) -> Option<Rung> {
    Some(match r {
        0 => Rung::RoleText,
        1 => Rung::Text,
        2 => Rung::RoleBox,
        3 => Rung::Geometry,
        _ => return None,
    })
}

fn method(m: align::Method) -> Method {
    match m {
        align::Method::Anchor => Method::Anchor,
        align::Method::Bounded => Method::Bounded,
        align::Method::Moved => Method::Moved,
        align::Method::Children => Method::Children,
    }
}

/// The level of `H1`..`Hn`.
fn heading_level(standard: Option<&str>) -> Option<u8> {
    standard?.strip_prefix('H')?.parse().ok()
}

fn matched(
    a: &Document,
    b: &Document,
    ea: &ElemKey,
    eb: &ElemKey,
    m: &Match,
    (ox, oy): (f64, f64),
    a_to_b: &[Option<PageIx>],
) -> Pair {
    let row_changes = rows::diff(&rows::rows(a, ea), &rows::rows(b, eb), (ox, oy));
    let mut changed = rows::typed(&row_changes, ea, eb);

    let (la, lb) = (
        heading_level(ea.standard.as_deref()),
        heading_level(eb.standard.as_deref()),
    );
    if la != lb {
        changed.push(FieldChange {
            field: Field::Level,
            a: la.map(|l| l.to_string()),
            b: lb.map(|l| l.to_string()),
        });
    }

    let mut delta = None;
    let mut moved_on_page = false;
    let mut page_changed = false;
    if let (Some((pa, ra)), Some((pb, rb))) = (ea.placed, eb.placed) {
        let expected = a_to_b.get(usize_from_u32(pa)).copied().flatten();
        if expected == Some(pb) {
            let (ca, cb) = (centre(ra), centre(rb));
            let d = (round3(cb.0 - ca.0 - ox), round3(cb.1 - ca.1 - oy));
            // A pure container's box is its children's union: they carry the change.
            let own_geometry = ea.holds_content || eb.holds_content;
            moved_on_page = own_geometry && d.0.hypot(d.1) > TOLERANCE;
            let edges = [
                rb.x0 - ox - ra.x0,
                rb.y0 - oy - ra.y0,
                rb.x1 - ox - ra.x1,
                rb.y1 - oy - ra.y1,
            ];
            if own_geometry && edges.iter().any(|e| e.abs() > TOLERANCE) {
                changed.push(FieldChange {
                    field: Field::Bbox,
                    a: Some(fmt_rect(ra)),
                    b: Some(fmt_rect(rb)),
                });
            }
            delta = Some(d);
        } else {
            // The two pages do not correspond. That holds whether A's page has a counterpart
            // elsewhere or none at all, and it has to: `a_to_b[pa] == Some(pb)` is mutual, so
            // asking `expected.is_some()` instead made the answer depend on which side was
            // called A — an element whose A page was dropped read as unmoved one way round and
            // moved the other.
            page_changed = true;
            changed.push(FieldChange {
                field: Field::Page,
                a: Some(pa.to_string()),
                b: Some(pb.to_string()),
            });
        }
    }
    changed.sort_by_key(|c| c.field);

    let reordered = m.method == align::Method::Moved;
    let status = if reordered || moved_on_page || page_changed {
        Status::Moved
    } else if !changed.is_empty() || !row_changes.is_empty() {
        Status::Modified
    } else {
        Status::Same
    };
    let (text_a, text_b) = (ea.text.display(), eb.text.display());
    Pair {
        kind: PairKind::Element,
        status,
        a: Some(ea.node.clone()),
        b: Some(eb.node.clone()),
        role: ea.role().to_owned(),
        text: if text_a.is_empty() { text_b } else { text_a },
        page_a: ea.placed.map(|(p, _)| p),
        page_b: eb.placed.map(|(p, _)| p),
        bbox_a: ea.placed.map(|(_, r)| r),
        bbox_b: eb.placed.map(|(_, r)| r),
        delta,
        reordered,
        changed,
        rows: row_changes,
        confidence: Confidence {
            rung: rung(m.rung),
            method: Some(method(m.method)),
            unicode_a: ea.text.unicode,
            unicode_b: eb.text.unicode,
            degraded_a: ea.degraded,
            degraded_b: eb.degraded,
        },
    }
}

fn unpaired(e: &ElemKey, side: Side) -> Pair {
    let (page, bbox) = (e.placed.map(|(p, _)| p), e.placed.map(|(_, r)| r));
    let on_a = side == Side::A;
    Pair {
        kind: PairKind::Element,
        status: if on_a { Status::Removed } else { Status::Added },
        a: on_a.then(|| e.node.clone()),
        b: (!on_a).then(|| e.node.clone()),
        role: e.role().to_owned(),
        text: e.text.display(),
        page_a: page.filter(|_| on_a),
        page_b: page.filter(|_| !on_a),
        bbox_a: bbox.filter(|_| on_a),
        bbox_b: bbox.filter(|_| !on_a),
        delta: None,
        reordered: false,
        changed: Vec::new(),
        rows: Vec::new(),
        confidence: Confidence {
            rung: None,
            method: None,
            unicode_a: e.text.unicode.filter(|_| on_a),
            unicode_b: e.text.unicode.filter(|_| !on_a),
            degraded_a: e.degraded && on_a,
            degraded_b: e.degraded && !on_a,
        },
    }
}

#[cfg(test)]
#[allow(
    clippy::float_cmp,
    reason = "the expected values are exact: medians and rounding of exactly representable inputs"
)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn median_and_rounding_are_symmetric_under_negation() {
        let v = vec![1.0, 2.0, 4.0, 10.0];
        assert_eq!(median(v.clone()), 3.0);
        assert_eq!(median(v.iter().map(|x| -x).collect()), -3.0);
        assert_eq!(round3(-0.0001), 0.0);
        assert!(round3(-0.0001).is_sign_positive());
        assert_eq!(round3(1.23456), 1.235);
        assert_eq!(round3(-1.23456), -1.235);
    }

    #[test]
    fn an_offset_needs_a_majority_of_pairs_that_agree() {
        // Two paragraphs moved by different amounts: no common offset.
        assert_eq!(consensus_offset(&[(0.0, 10.0), (0.0, 140.0)]), None);
        assert_eq!(consensus_offset(&[(0.0, 10.0), (0.0, 140.0), (0.0, 75.0)]), None);
        // A shifted page with one element that also moved on its own.
        let shifted = [(2.0, -2.0), (2.0, -2.0), (1.9, -2.0), (2.1, -2.0), (30.0, 5.0)];
        assert_eq!(consensus_offset(&shifted), Some((2.0, -2.0)));
        // Negating every delta negates the offset.
        let negated: Vec<_> = shifted.iter().map(|d| (-d.0, -d.1)).collect();
        assert_eq!(consensus_offset(&negated), Some((-2.0, 2.0)));
        assert_eq!(consensus_offset(&[]), None);
    }

    #[test]
    fn heading_levels_come_from_the_standard_type() {
        assert_eq!(heading_level(Some("H3")), Some(3));
        assert_eq!(heading_level(Some("H")), None);
        assert_eq!(heading_level(Some("P")), None);
        assert_eq!(heading_level(None), None);
    }
}
