//! Page correspondence, derived from the element pairs, and the residue: the painted content of
//! each page that no structure element owns.
//!
//! A page of A and a page of B correspond when most of the pairs on each lead to the other. The
//! pages without such evidence (untagged pages, pages of artifacts only) are aligned by their
//! residue, through the same aligner as the elements: two pages that paint the same ink outside
//! the structure tree correspond. The untagged pages that are left — edited, so their residue is
//! no longer identical — are paired by how much of their residue they share: mutual bests at or
//! above [`SIMILAR_MIN`], never crossing a pair already made. Nothing is matched by index.

use std::collections::{BTreeMap, BTreeSet};

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

/// Least similarity (see [`similarity`]) at which two untagged pages correspond, `1/2`: of all
/// the items on the two pages together, at least half are on both. Below it a page is unpaired,
/// however close its best candidate: similarity is evidence, and too little of it is none.
///
/// Measured on the fixtures and the vendored corpus: two *different* untagged pages of one
/// document share at most 0.098 of their ink (repeated rules and page furniture), while an edited
/// page keeps 0.66 after a word change that shifts the rest of its line, and 0.99 after a
/// page-number change. Half sits well clear of both; what falls below it is a page whose ink
/// mostly moved (a reflow), where saying "not found" is honest.
const SIMILAR_MIN: Similarity = Similarity { shared: 1, total: 2 };

/// How much of two residues is shared: the Dice coefficient `shared / total`, the items on both
/// pages (`2·common`) over all items of the two (`|a| + |b|`), from 0 (nothing in common) to 1
/// (the same multiset). Compared as an exact fraction, so ties are ties.
#[derive(Debug, Clone, Copy)]
struct Similarity {
    shared: usize,
    total: usize,
}

impl PartialEq for Similarity {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == std::cmp::Ordering::Equal
    }
}

impl Eq for Similarity {}

impl PartialOrd for Similarity {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Similarity {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // usize is at most 64 bits: the product of two fits in u128.
        let cross = |x: usize, y: usize| x as u128 * y as u128;
        cross(self.shared, other.total).cmp(&cross(other.shared, self.total))
    }
}

/// The [`Similarity`] of two residues. Two empty residues (blank pages) are the same.
fn similarity(a: &Residues, b: &Residues) -> Similarity {
    let r = compare(a, b);
    let total = r.a + r.b;
    if total == 0 {
        return Similarity { shared: 1, total: 1 };
    }
    Similarity {
        shared: 2 * (r.a - r.only_a),
        total,
    }
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
/// B has placed where they fall. `votes` counts the element pairs per `(page of A, page of B)`;
/// `tagged_a` and `tagged_b` are the pages some structure element of that side is placed on.
pub(super) fn correspond(
    votes: &BTreeMap<(PageIx, PageIx), usize>,
    residue_a: &[Residues],
    residue_b: &[Residues],
    tagged: [&BTreeSet<PageIx>; 2],
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
    let mut aligned = align::align(&keys_a, &keys_b);
    pair_similar(&mut aligned, residue_a, residue_b, tagged);
    aligned
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

/// Pairs the untagged pages `aligned` left on both sides by residue [`similarity`]. A page
/// with a structure element on it (`tagged`, per side) is never a candidate: its residue is only
/// what lies outside the tree — on a tagged page, mostly the running header and footer, which
/// every page shares (two different pages of the tagged fixtures are 0.83 similar on it) — so
/// its similarity is no evidence. Its correspondence rests on its elements.
///
/// In rounds: each page's best candidate on the other side, at or above [`SIMILAR_MIN`], ties
/// to the smaller index; the mutual bests are paired. Two versions of one document keep their
/// page order, so a candidate that would cross a pair already made is no candidate, and of two
/// mutual bests that cross each other only the strictly more similar one is paired this round
/// (on a tie, neither: the evidence does not say which). Every rule is symmetric in A and B, so
/// pairing B with A gives the mirror image.
fn pair_similar(
    aligned: &mut align::Aligned,
    residue_a: &[Residues],
    residue_b: &[Residues],
    [tagged_a, tagged_b]: [&BTreeSet<PageIx>; 2],
) {
    let free = |left: &[usize], tagged: &BTreeSet<PageIx>| -> Vec<usize> {
        left.iter()
            .copied()
            .filter(|&p| PageIx::try_from(p).is_ok_and(|p| !tagged.contains(&p)))
            .collect()
    };
    let (mut free_a, mut free_b) = (free(&aligned.removed, tagged_a), free(&aligned.added, tagged_b));
    if free_a.is_empty() || free_b.is_empty() {
        return;
    }
    let empty = Residues::new();
    let (ra, rb) = (
        |p: usize| residue_a.get(p).unwrap_or(&empty),
        |p: usize| residue_b.get(p).unwrap_or(&empty),
    );
    let score: BTreeMap<(usize, usize), Similarity> = free_a
        .iter()
        .flat_map(|&i| free_b.iter().map(move |&j| (i, j)))
        .filter_map(|(i, j)| {
            let s = similarity(ra(i), rb(j));
            (s >= SIMILAR_MIN).then_some(((i, j), s))
        })
        .collect();
    let mut fixed: Vec<(usize, usize)> = aligned.matches.iter().map(|m| (m.a, m.b)).collect();
    let crosses = |(i, j): (usize, usize), (x, y): (usize, usize)| (i < x && j > y) || (i > x && j < y);
    loop {
        let open = |&(i, j): &(usize, usize)| {
            free_a.contains(&i) && free_b.contains(&j) && !fixed.iter().any(|&f| crosses((i, j), f))
        };
        let candidates: Vec<((usize, usize), Similarity)> = score
            .iter()
            .filter(|(k, _)| open(k))
            .map(|(&k, &s)| (k, s))
            .collect();
        // Best partner per page, ties to the smaller index (the map iterates in index order).
        let best = |flip: bool| -> BTreeMap<usize, (usize, Similarity)> {
            let mut out: BTreeMap<usize, (usize, Similarity)> = BTreeMap::new();
            for &((i, j), s) in &candidates {
                let (from, to) = if flip { (j, i) } else { (i, j) };
                let slot = out.entry(from).or_insert((to, s));
                if s > slot.1 || (s == slot.1 && to < slot.0) {
                    *slot = (to, s);
                }
            }
            out
        };
        let (from_a, from_b) = (best(false), best(true));
        let mutual: Vec<((usize, usize), Similarity)> = from_a
            .iter()
            .filter(|&(&i, &(j, _))| from_b.get(&j).is_some_and(|&(back, _)| back == i))
            .map(|(&i, &(j, s))| ((i, j), s))
            .collect();
        let taken: Vec<(usize, usize)> = mutual
            .iter()
            .filter(|&&(k, s)| !mutual.iter().any(|&(o, t)| crosses(k, o) && t >= s))
            .map(|&(k, _)| k)
            .collect();
        if taken.is_empty() {
            break;
        }
        for &(i, j) in &taken {
            free_a.retain(|&p| p != i);
            free_b.retain(|&p| p != j);
            fixed.push((i, j));
            aligned.matches.push(align::Match {
                a: i,
                b: j,
                rung: 1,
                method: align::Method::Bounded,
            });
        }
    }
    aligned.matches.sort_unstable_by_key(|m| m.a);
    let paired_a: BTreeSet<usize> = aligned.matches.iter().map(|m| m.a).collect();
    let paired_b: BTreeSet<usize> = aligned.matches.iter().map(|m| m.b).collect();
    aligned.removed.retain(|i| !paired_a.contains(i));
    aligned.added.retain(|j| !paired_b.contains(j));
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
        let pages = correspond(&votes, &ra, &rb, [&set(&[0, 1]), &set(&[0, 1, 2])]);
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
        let pages = correspond(&BTreeMap::new(), &r, &r, UNTAGGED);
        assert_eq!(pages, (0..4).map(|p| (Some(p), Some(p), 0)).collect::<Vec<_>>());
    }

    const UNTAGGED: [&BTreeSet<PageIx>; 2] = [&BTreeSet::new(), &BTreeSet::new()];

    fn set(pages: &[PageIx]) -> BTreeSet<PageIx> {
        pages.iter().copied().collect()
    }

    /// A page of `n` distinct items starting at `from`.
    fn ink(from: u128, n: u128) -> Residues {
        (from..from + n).collect()
    }

    /// `page` with its first `k` items replaced by fresh ones.
    fn edited(page: &Residues, k: usize, fresh: u128) -> Residues {
        let mut out: Residues = page[k..].to_vec();
        out.extend((0..k).map(|i| fresh + i as u128));
        out.sort_unstable();
        out
    }

    /// `correspond` B with A, mirrored back into A's terms and order.
    fn mirrored(ra: &[Residues], rb: &[Residues]) -> BTreeSet<(Option<PageIx>, Option<PageIx>)> {
        correspond(&BTreeMap::new(), rb, ra, UNTAGGED)
            .into_iter()
            .map(|(b, a, _)| (a, b))
            .collect()
    }

    fn pairs_of(pages: &[(Option<PageIx>, Option<PageIx>, usize)]) -> Vec<(Option<PageIx>, Option<PageIx>)> {
        pages.iter().map(|&(a, b, _)| (a, b)).collect()
    }

    #[test]
    fn edited_untagged_pages_correspond_and_an_inserted_one_does_not() {
        // A: three untagged pages. B: a new page inserted at 1, A's page 1 with 30 of its 100
        // items changed (similarity 0.7), A's page 2 with one changed.
        let ra = vec![ink(0, 100), ink(1000, 100), ink(2000, 100)];
        let rb = vec![
            ra[0].clone(),
            ink(5000, 60),
            edited(&ra[1], 30, 9000),
            edited(&ra[2], 1, 9500),
        ];
        let pages = correspond(&BTreeMap::new(), &ra, &rb, UNTAGGED);
        assert_eq!(
            pairs_of(&pages),
            vec![
                (Some(0), Some(0)),
                (None, Some(1)),
                (Some(1), Some(2)),
                (Some(2), Some(3)),
            ]
        );
        assert_eq!(mirrored(&ra, &rb), pairs_of(&pages).into_iter().collect());
        // Smoke: no page of A is left without a partner, and the new page has none.
        assert_eq!(pages.iter().filter(|p| p.0.is_some() && p.1.is_none()).count(), 0);
        assert_eq!(pages.iter().filter(|p| p.0.is_none()).count(), 1);
    }

    #[test]
    fn a_page_sharing_less_than_half_its_ink_stays_unpaired() {
        // 51 of 100 items replaced: similarity 0.49, below the threshold.
        let ra = vec![ink(0, 100)];
        let rb = vec![edited(&ra[0], 51, 7000)];
        let pages = correspond(&BTreeMap::new(), &ra, &rb, UNTAGGED);
        assert_eq!(pairs_of(&pages), vec![(Some(0), None), (None, Some(0))]);
        // Smoke: at 50 replaced (exactly half shared) it pairs.
        let rb = vec![edited(&ra[0], 50, 7000)];
        let pages = correspond(&BTreeMap::new(), &ra, &rb, UNTAGGED);
        assert_eq!(pairs_of(&pages), vec![(Some(0), Some(0))]);
    }

    #[test]
    fn similarity_never_crosses_a_pair_already_made() {
        // A0 and B1 are identical (an anchor); A1 resembles B0, which lies before that anchor in
        // B while A1 lies after it in A. Pairing them would swap page order: they stay apart.
        let ra = vec![ink(0, 50), ink(1000, 100)];
        let rb = vec![edited(&ra[1], 10, 9000), ra[0].clone()];
        let pages = correspond(&BTreeMap::new(), &ra, &rb, UNTAGGED);
        assert_eq!(
            pairs_of(&pages),
            vec![(None, Some(0)), (Some(0), Some(1)), (Some(1), None)]
        );
        // Smoke: without the anchor the same two pages do pair.
        let pages = correspond(&BTreeMap::new(), &ra[1..], &rb[..1], UNTAGGED);
        assert_eq!(pairs_of(&pages), vec![(Some(0), Some(0))]);
    }

    #[test]
    fn of_two_crossing_candidates_the_more_similar_wins_and_a_tie_pairs_neither() {
        // A: [x, y]; B: [y', x'] — a genuine swap. x' keeps 90 of 100, y' keeps 70.
        let (x, y) = (ink(0, 100), ink(1000, 100));
        let ra = vec![x.clone(), y.clone()];
        let rb = vec![edited(&y, 30, 8000), edited(&x, 10, 9000)];
        let pages = correspond(&BTreeMap::new(), &ra, &rb, UNTAGGED);
        assert_eq!(
            pairs_of(&pages),
            vec![(None, Some(0)), (Some(0), Some(1)), (Some(1), None)]
        );
        assert_eq!(mirrored(&ra, &rb), pairs_of(&pages).into_iter().collect());
        // Equally similar: the evidence cannot say which order is right, so neither pairs.
        let rb = vec![edited(&y, 10, 8000), edited(&x, 10, 9000)];
        let pages = correspond(&BTreeMap::new(), &ra, &rb, UNTAGGED);
        assert!(pages.iter().all(|p| p.0.is_none() || p.1.is_none()), "{pages:?}");
        // Smoke: four rows, every page once.
        assert_eq!(pages.len(), 4);
    }

    #[test]
    fn tagged_pages_are_never_paired_by_similarity() {
        // Two tagged pages whose residue (a shared footer) is nearly identical but whose
        // elements did not pair: no evidence they correspond.
        let ra = vec![ink(0, 6)];
        let rb = vec![edited(&ra[0], 1, 900)];
        let tagged = set(&[0]);
        let pages = correspond(&BTreeMap::new(), &ra, &rb, [&tagged, &tagged]);
        assert_eq!(pairs_of(&pages), vec![(Some(0), None), (None, Some(0))]);
        // Smoke: untagged, the same residues pair.
        let pages = correspond(&BTreeMap::new(), &ra, &rb, UNTAGGED);
        assert_eq!(pairs_of(&pages), vec![(Some(0), Some(0))]);
    }

    #[test]
    fn similarity_is_dice_and_blank_pages_are_the_same() {
        let ratio = |shared, total| Similarity { shared, total };
        assert_eq!(similarity(&vec![], &vec![]), ratio(1, 1));
        assert_eq!(similarity(&vec![1], &vec![]), ratio(0, 1));
        assert_eq!(similarity(&vec![1, 2, 3, 4], &vec![1, 2, 5, 6]), SIMILAR_MIN);
        assert!(similarity(&vec![1, 2, 3, 4], &vec![1, 5, 6, 7]) < SIMILAR_MIN);
        // Smoke: symmetric.
        assert_eq!(
            similarity(&vec![1, 1, 2], &vec![1, 3]),
            similarity(&vec![1, 3], &vec![1, 1, 2])
        );
    }
}
