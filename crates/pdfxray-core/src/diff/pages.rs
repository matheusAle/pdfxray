//! Page correspondence, derived from the element pairs, and the residue: the painted content of
//! each page that no structure element owns.
//!
//! A page of A and a page of B correspond when most of the pairs on each lead to the other. The
//! pages without such evidence (untagged pages, pages of artifacts only) are aligned by their
//! residue, through the same aligner as the elements: two pages that paint the same ink outside
//! the structure tree correspond. Nothing is matched by index.

use std::collections::BTreeMap;

use super::Residue;
use super::align::{self, Keys, RUNGS};
use super::keys::{Fnv, quantised};
use crate::capture::{CapturedPage, PaintKind};
use crate::numeric::usize_from_u32;
use crate::{Document, PageIx, Rect};

/// Quantum of the residue items' boxes, points.
const RESIDUE_QUANTUM: f64 = 1.0;

/// The residue of one page: one identity per painted item outside the structure tree, sorted
/// (a multiset: paint order does not matter).
pub(super) type Residues = Vec<u128>;

/// The residue of every page of `doc`, visiting the pages in `order` (every page once) and
/// shifting B's boxes back by `offset`.
pub(super) fn residues(doc: &Document, order: &[PageIx], offset: (f64, f64)) -> Vec<Residues> {
    let mut out = vec![Residues::new(); doc.page_count()];
    for &page in order {
        if let (Some(slot), Some(cap)) = (out.get_mut(usize_from_u32(page)), doc.captured(page)) {
            *slot = page_residue(cap, offset);
        }
    }
    out
}

fn page_residue(cap: &CapturedPage, (dx, dy): (f64, f64)) -> Residues {
    let tagged = |s: &u32| {
        cap.marked
            .get(usize_from_u32(*s))
            .is_some_and(|m| m.mcid.is_some())
    };
    let mut out: Residues = cap
        .items
        .iter()
        .filter(|item| !matches!(item.kind, PaintKind::Clip) && !item.mc_stack.iter().any(tagged))
        .map(|item| {
            let b = item.bbox;
            let shifted = Rect::new(b.x0 - dx, b.y0 - dy, b.x1 - dx, b.y1 - dy);
            let h = Fnv::new("residue").str(&quantised(shifted, RESIDUE_QUANTUM));
            match &item.kind {
                PaintKind::Fill => h.str("fill"),
                PaintKind::Stroke => h.str("stroke"),
                PaintKind::FillStroke => h.str("fill+stroke"),
                PaintKind::Clip => h.str("clip"),
                PaintKind::Glyph {
                    unicode, char_code, ..
                } => h
                    .str("glyph")
                    .str(unicode.as_deref().unwrap_or(""))
                    .num(u128::from(*char_code)),
                PaintKind::Image { width, height, .. } => {
                    h.str("image").num(u128::from(*width)).num(u128::from(*height))
                }
                PaintKind::Shading => h.str("shading"),
            }
            .0
        })
        .collect();
    out.sort_unstable();
    out
}

/// Compares two residues as multisets.
pub(super) fn compare(a: &Residues, b: &Residues) -> Residue {
    let (mut i, mut j, mut common) = (0, 0, 0);
    while i < a.len() && j < b.len() {
        match a[i].cmp(&b[j]) {
            std::cmp::Ordering::Less => i += 1,
            std::cmp::Ordering::Greater => j += 1,
            std::cmp::Ordering::Equal => {
                common += 1;
                i += 1;
                j += 1;
            }
        }
    }
    Residue {
        a: a.len(),
        b: b.len(),
        only_a: a.len() - common,
        only_b: b.len() - common,
    }
}

/// The page of the other side most pairs lead to, per page; ties go to the smaller index.
fn best(votes: &BTreeMap<(PageIx, PageIx), usize>, flip: bool) -> BTreeMap<PageIx, (PageIx, usize)> {
    let mut out: BTreeMap<PageIx, (PageIx, usize)> = BTreeMap::new();
    for (&(pa, pb), &n) in votes {
        let (from, to) = if flip { (pb, pa) } else { (pa, pb) };
        let slot = out.entry(from).or_insert((to, n));
        if n > slot.1 || (n == slot.1 && to < slot.0) {
            *slot = (to, n);
        }
    }
    out
}

/// Which pages correspond: `(page of A, page of B, pairs between them)`, A's order, pages only
/// B has placed where they fall. `votes` counts the element pairs per `(page of A, page of B)`.
pub(super) fn correspond(
    votes: &BTreeMap<(PageIx, PageIx), usize>,
    residue_a: &[Residues],
    residue_b: &[Residues],
) -> Vec<(Option<PageIx>, Option<PageIx>, usize)> {
    let from_a = best(votes, false);
    let from_b = best(votes, true);
    let mutual = |pa: PageIx| {
        let &(pb, _) = from_a.get(&pa)?;
        (from_b.get(&pb)?.0 == pa).then_some(pb)
    };
    let residue_key = |r: &Residues| {
        let mut h = Fnv::new("page-residue");
        for &item in r {
            h = h.num(item);
        }
        h.0
    };
    // Rung 0: the element evidence, a key both pages of a mutual pair share. Rung 1: the residue.
    let mut keys_a: Vec<Keys> = residue_a
        .iter()
        .map(|r| {
            let mut k = [None; RUNGS];
            k[1] = Some(residue_key(r));
            k
        })
        .collect();
    let mut keys_b: Vec<Keys> = residue_b
        .iter()
        .map(|r| {
            let mut k = [None; RUNGS];
            k[1] = Some(residue_key(r));
            k
        })
        .collect();
    for pa in 0..keys_a.len() {
        let Ok(pa) = PageIx::try_from(pa) else { break };
        if let Some(pb) = mutual(pa)
            && let Some(kb) = keys_b.get_mut(usize_from_u32(pb))
        {
            let evidence = Some(Fnv::new("page-pair").num(u128::from(pa)).0);
            kb[0] = evidence;
            keys_a[usize_from_u32(pa)][0] = evidence;
        }
    }
    let to_page = |i: usize| PageIx::try_from(i).ok();
    align::align(&keys_a, &keys_b)
        .ordered(keys_a.len())
        .into_iter()
        .map(|(i, j)| {
            let (pa, pb) = (i.and_then(to_page), j.and_then(to_page));
            let n = match (pa, pb) {
                (Some(pa), Some(pb)) => votes.get(&(pa, pb)).copied().unwrap_or(0),
                _ => 0,
            };
            (pa, pb, n)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn residues_compare_as_multisets() {
        let r = compare(&vec![1, 1, 2, 5], &vec![1, 2, 2, 7]);
        assert_eq!(
            r,
            Residue {
                a: 4,
                b: 4,
                only_a: 2,
                only_b: 2
            }
        );
        assert_eq!(compare(&vec![], &vec![]), Residue::default());
    }

    #[test]
    fn pages_correspond_by_element_evidence_then_by_residue() {
        // A: 3 pages; B: a blank page inserted at 1. Elements vote 0→0 and 1→2; page 2 of A and
        // page 3 of B are untagged with the same residue.
        let votes = BTreeMap::from([((0, 0), 5), ((1, 2), 4), ((1, 1), 1)]);
        let ra = vec![vec![], vec![], vec![9]];
        let rb = vec![vec![], vec![3], vec![], vec![9]];
        let pages = correspond(&votes, &ra, &rb);
        assert_eq!(
            pages,
            vec![
                (Some(0), Some(0), 5),
                (None, Some(1), 0),
                (Some(1), Some(2), 4),
                (Some(2), Some(3), 0),
            ]
        );
        assert_eq!(pages.len(), 4);
    }

    #[test]
    fn identical_untagged_pages_correspond_one_to_one() {
        let r = vec![vec![1], vec![], vec![], vec![2]];
        let pages = correspond(&BTreeMap::new(), &r, &r);
        assert_eq!(pages, (0..4).map(|p| (Some(p), Some(p), 0)).collect::<Vec<_>>());
    }
}
