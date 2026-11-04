//! The aligner: a pure function over two keyed sequences. It knows nothing about PDFs, so every
//! case of the algorithm is tested here without a document.
//!
//! Every item carries one key per rung of the ladder (`role + text`, `text`, `role + box`,
//! `geometry`); a missing key never matches anything. The alignment is
//!
//! 1. **patience anchoring**: the keys of the first rung that are unique in *both* sides of the
//!    range, kept in the longest chain increasing on both sides (so anchors never cross);
//! 2. **recursion** into the gaps between anchors, back at the top rung, because a key repeated
//!    across the whole document is often unique inside a narrow gap;
//! 3. **bounded full alignment** inside a gap with no anchor at any rung: a weighted longest
//!    common subsequence, refused above [`MAX_CELLS`] (the gap is then left to the move pass and
//!    counted in [`Aligned::unbounded_gaps`]);
//! 4. **the move pass**: leftover removals paired with leftover additions whose text key is
//!    unique among the leftovers of both sides. Those are the items that changed place.
//!
//! When the items form a tree, [`pair_parents`] then pairs leftover containers whose children
//! are paired with each other's: a `Document` or `Sect` whose text and box both changed because
//! something inside it did is still the same container.
//!
//! There is deliberately no trimming of the common prefix and suffix, as textual diffs do. A
//! document repeats itself from page to page (the same cell texts, figures in the same place),
//! and a page appended to B ends B exactly like A ends: trimming would pair A's last page with
//! B's new one. Equal items only pair as anchors, where they are unique, or inside a gap.
//!
//! The result is symmetric by construction: the two sides are put in a canonical order before
//! aligning and swapped back after, so aligning B with A mirrors aligning A with B exactly, ties
//! included.

use std::collections::HashMap;
use std::collections::hash_map::Entry;

/// Rungs of the key ladder, best first.
pub(crate) const RUNGS: usize = 4;

/// The keys of one item, one per rung; `None` where the item has nothing to key on.
pub(crate) type Keys = [Option<u128>; RUNGS];

/// Most cells (`|gap A| × |gap B|`) a bounded full alignment may fill.
pub(crate) const MAX_CELLS: usize = 1 << 20;

/// Rungs the move pass pairs leftovers on: the text keys, never geometry.
const MOVE_RUNGS: [usize; 2] = [0, 1];

/// How a match was made.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum Method {
    /// Equal ladders at the start or end of a range, or a unique key on both sides.
    Anchor,
    /// The weighted alignment inside a gap that had no unique key at any rung.
    Bounded,
    /// The move pass: a leftover on both sides with the same unique text key.
    Moved,
    /// [`pair_parents`]: leftovers of the same role whose paired children mostly pair with each
    /// other's. Its `rung` is [`RUNGS`]: no key matched.
    Children,
}

/// One item of A matched with one item of B.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct Match {
    /// Index into A.
    pub(crate) a: usize,
    /// Index into B.
    pub(crate) b: usize,
    /// The rung the match rests on: for an anchor, the rung its key was unique on, never better
    /// than the anchors bounding its gap (see [`Gap::floor`]). Keys equal on a better rung but
    /// repeated there are no evidence: four identical table rows told apart by their boxes are
    /// paired on the box, whatever their texts say.
    pub(crate) rung: usize,
    /// How the match was made.
    pub(crate) method: Method,
}

/// The result of [`align`].
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct Aligned {
    /// The matches, by A index.
    pub(crate) matches: Vec<Match>,
    /// Items of A matched with nothing, ascending.
    pub(crate) removed: Vec<usize>,
    /// Items of B matched with nothing, ascending.
    pub(crate) added: Vec<usize>,
    /// Gaps too large for the bounded alignment, left to the move pass.
    pub(crate) unbounded_gaps: usize,
}

/// Aligns `a` with `b`. See the module documentation.
pub(crate) fn align(a: &[Keys], b: &[Keys]) -> Aligned {
    if a > b {
        return mirror(align_ordered(b, a));
    }
    align_ordered(a, b)
}

/// One row of [`Aligned::ordered`]: an item of A, of B, or one of each.
pub(crate) type Row = (Option<usize>, Option<usize>);

/// Where a row goes in [`Aligned::ordered`].
type SortKey = (usize, u8, usize);

impl Aligned {
    /// Every item once, as `(a, b)`: A's order, each item only B has placed right before the
    /// in-order match that follows it in B (at the end when none does). An item replaced by
    /// another thus reads removed, then added.
    pub(crate) fn ordered(&self, a_len: usize) -> Vec<Row> {
        let mut in_order: Vec<(usize, usize)> = self
            .matches
            .iter()
            .filter(|m| matches!(m.method, Method::Anchor | Method::Bounded))
            .map(|m| (m.b, m.a))
            .collect();
        in_order.sort_unstable();
        // The A index of the first in-order match after B index `j`.
        let next = |j: usize| -> usize {
            let k = in_order.partition_point(|&(b, _)| b <= j);
            in_order.get(k).map_or(a_len, |&(_, i)| i)
        };
        // Sort key: (A position, 0 for B's own items / 1 for A's, B index).
        let mut rows: Vec<(SortKey, Row)> = Vec::new();
        rows.extend(self.matches.iter().map(|m| ((m.a, 1, 0), (Some(m.a), Some(m.b)))));
        rows.extend(self.removed.iter().map(|&i| ((i, 1, 0), (Some(i), None))));
        rows.extend(self.added.iter().map(|&j| ((next(j), 0, j), (None, Some(j)))));
        rows.sort_unstable_by_key(|(k, _)| *k);
        rows.into_iter().map(|(_, r)| r).collect()
    }
}

/// The tree over a sequence of items: each item's parent (an earlier item) and a key of its role.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Tree<'a> {
    pub(crate) parent: &'a [Option<usize>],
    pub(crate) role: &'a [u128],
}

/// For leftover `x` of one side, the leftover of the other side most of `x`'s paired children
/// have as parent, among those with `x`'s role; ties go to the smaller index.
fn best_parent(
    x: usize,
    children: &[Vec<usize>],
    partner: &[Option<usize>],
    mine: Tree<'_>,
    theirs: Tree<'_>,
    their_left: &[bool],
) -> Option<usize> {
    let mut votes: HashMap<usize, usize> = HashMap::new();
    for &c in &children[x] {
        if let Some(p) = partner[c].and_then(|c2| theirs.parent[c2])
            && their_left[p]
            && theirs.role[p] == mine.role[x]
        {
            *votes.entry(p).or_default() += 1;
        }
    }
    votes
        .into_iter()
        .max_by(|(p, n), (q, m)| n.cmp(m).then(q.cmp(p)))
        .map(|(p, _)| p)
}

/// Pairs leftovers whose children are paired with each other's: `x` of A and `y` of B, of the
/// same role, when `y` is where most of `x`'s paired children went and `x` is where most of
/// `y`'s came from. Repeated until nothing changes, so that a container paired this way lets
/// its own parent pair in the next round; every round decides on the state before it, so the
/// result does not depend on the order of the items and B with A mirrors A with B.
pub(crate) fn pair_parents(mut r: Aligned, ta: Tree<'_>, tb: Tree<'_>) -> Aligned {
    let (na, nb) = (ta.parent.len(), tb.parent.len());
    let children = |t: Tree<'_>, n: usize| {
        let mut out = vec![Vec::new(); n];
        for (i, p) in t.parent.iter().enumerate() {
            if let Some(p) = *p
                && p < n
            {
                out[p].push(i);
            }
        }
        out
    };
    let (kids_a, kids_b) = (children(ta, na), children(tb, nb));
    let mut a_to_b: Vec<Option<usize>> = vec![None; na];
    let mut b_to_a: Vec<Option<usize>> = vec![None; nb];
    for m in &r.matches {
        a_to_b[m.a] = Some(m.b);
        b_to_a[m.b] = Some(m.a);
    }
    loop {
        let left_a: Vec<bool> = a_to_b.iter().map(Option::is_none).collect();
        let left_b: Vec<bool> = b_to_a.iter().map(Option::is_none).collect();
        let found: Vec<(usize, usize)> = r
            .removed
            .iter()
            .filter(|&&x| left_a[x])
            .filter_map(|&x| {
                let y = best_parent(x, &kids_a, &a_to_b, ta, tb, &left_b)?;
                (best_parent(y, &kids_b, &b_to_a, tb, ta, &left_a) == Some(x)).then_some((x, y))
            })
            .collect();
        if found.is_empty() {
            break;
        }
        for (x, y) in found {
            a_to_b[x] = Some(y);
            b_to_a[y] = Some(x);
            r.matches.push(Match {
                a: x,
                b: y,
                rung: RUNGS,
                method: Method::Children,
            });
        }
    }
    r.matches.sort_unstable_by_key(|m| m.a);
    r.removed.retain(|&x| a_to_b[x].is_none());
    r.added.retain(|&y| b_to_a[y].is_none());
    r
}

/// Swaps the two sides of a result.
fn mirror(r: Aligned) -> Aligned {
    let mut matches: Vec<Match> = r
        .matches
        .into_iter()
        .map(|m| Match { a: m.b, b: m.a, ..m })
        .collect();
    matches.sort_unstable_by_key(|m| m.a);
    Aligned {
        matches,
        removed: r.added,
        added: r.removed,
        unbounded_gaps: r.unbounded_gaps,
    }
}

/// A half-open range of A and one of B still to align.
#[derive(Debug, Clone, Copy)]
struct Gap {
    a: (usize, usize),
    b: (usize, usize),
    /// The weakest rung among the anchors the gap was cut by. A key unique inside the gap is
    /// unique only because those anchors narrowed it, so a match inside is no stronger.
    floor: usize,
}

fn align_ordered(a: &[Keys], b: &[Keys]) -> Aligned {
    let mut out = Aligned::default();
    let mut work = vec![Gap {
        a: (0, a.len()),
        b: (0, b.len()),
        floor: 0,
    }];
    while let Some(gap) = work.pop() {
        align_gap(a, b, gap, &mut out, &mut work);
    }
    let mut matched_a = vec![false; a.len()];
    let mut matched_b = vec![false; b.len()];
    for m in &out.matches {
        matched_a[m.a] = true;
        matched_b[m.b] = true;
    }
    let left_a: Vec<usize> = (0..a.len()).filter(|&i| !matched_a[i]).collect();
    let left_b: Vec<usize> = (0..b.len()).filter(|&j| !matched_b[j]).collect();
    let moved = move_pass(a, b, &left_a, &left_b);
    for m in &moved {
        matched_a[m.a] = true;
        matched_b[m.b] = true;
    }
    out.matches.extend(moved);
    out.matches.sort_unstable_by_key(|m| m.a);
    out.removed = left_a.into_iter().filter(|&i| !matched_a[i]).collect();
    out.added = left_b.into_iter().filter(|&j| !matched_b[j]).collect();
    out
}

/// The best rung on which `x` and `y` carry the same key.
fn common_rung(x: &Keys, y: &Keys) -> Option<usize> {
    (0..RUNGS).find(|&r| x[r].is_some() && x[r] == y[r])
}

/// Anchors `gap` on its best rung and queues the gaps between anchors; without anchors at any
/// rung, aligns it in full.
fn align_gap(a: &[Keys], b: &[Keys], gap: Gap, out: &mut Aligned, work: &mut Vec<Gap>) {
    if gap.a.0 == gap.a.1 || gap.b.0 == gap.b.1 {
        return;
    }
    for rung in 0..RUNGS {
        let anchors = anchors(a, b, gap, rung);
        if anchors.is_empty() {
            continue;
        }
        let floor = gap.floor.max(rung);
        let (mut ai, mut bi) = (gap.a.0, gap.b.0);
        for &(i, j) in &anchors {
            out.matches.push(Match {
                a: i,
                b: j,
                rung: floor,
                method: Method::Anchor,
            });
            work.push(Gap {
                a: (ai, i),
                b: (bi, j),
                floor,
            });
            (ai, bi) = (i + 1, j + 1);
        }
        work.push(Gap {
            a: (ai, gap.a.1),
            b: (bi, gap.b.1),
            floor,
        });
        return;
    }
    bounded(a, b, gap, out);
}

/// Where a key occurs in a range: its only position, or `None` once it repeats.
fn unique_positions(keys: &[Keys], (lo, hi): (usize, usize), rung: usize) -> HashMap<u128, Option<usize>> {
    let mut seen: HashMap<u128, Option<usize>> = HashMap::new();
    for (i, k) in keys.iter().enumerate().take(hi).skip(lo) {
        if let Some(key) = k[rung] {
            match seen.entry(key) {
                Entry::Vacant(v) => {
                    v.insert(Some(i));
                }
                Entry::Occupied(mut o) => {
                    o.insert(None);
                }
            }
        }
    }
    seen
}

/// The patience anchors of `gap` on `rung`: keys unique on both sides, kept in the longest chain
/// that increases on both sides, by A position.
fn anchors(a: &[Keys], b: &[Keys], gap: Gap, rung: usize) -> Vec<(usize, usize)> {
    let in_b = unique_positions(b, gap.b, rung);
    if in_b.is_empty() {
        return Vec::new();
    }
    let in_a = unique_positions(a, gap.a, rung);
    let mut pairs: Vec<(usize, usize)> = in_a
        .iter()
        .filter_map(|(key, &i)| Some((i?, (*in_b.get(key)?)?)))
        .collect();
    pairs.sort_unstable();
    longest_increasing(&pairs)
}

/// The longest subsequence of `pairs` (sorted by first element, all distinct on both) that
/// increases in the second element too. Patience sorting, O(n log n); of several longest
/// chains, the one that ends at the smallest second elements wins.
fn longest_increasing(pairs: &[(usize, usize)]) -> Vec<(usize, usize)> {
    // tails[k]: index into `pairs` of the smallest tail of an increasing chain of length k + 1.
    let mut tails: Vec<usize> = Vec::new();
    let mut prev: Vec<Option<usize>> = vec![None; pairs.len()];
    for (n, &(_, y)) in pairs.iter().enumerate() {
        let k = tails.partition_point(|&t| pairs[t].1 < y);
        if k > 0 {
            prev[n] = Some(tails[k - 1]);
        }
        if k == tails.len() {
            tails.push(n);
        } else {
            tails[k] = n;
        }
    }
    let mut chain = Vec::with_capacity(tails.len());
    let mut cur = tails.last().copied();
    while let Some(n) = cur {
        chain.push(pairs[n]);
        cur = prev[n];
    }
    chain.reverse();
    chain
}

/// Weight of a match on `rung`: a better rung always outweighs any number of worse ones in the
/// same position, so text matches are never traded for geometry matches.
fn weight(rung: usize) -> u64 {
    match rung {
        0 => 8,
        1 => 4,
        2 => 2,
        _ => 1,
    }
}

/// Weighted longest common subsequence over a gap with no unique key at any rung. Ties prefer
/// matching, then skipping an item of A: deterministic, and mirrored by [`align`]'s ordering.
fn bounded(a: &[Keys], b: &[Keys], gap: Gap, out: &mut Aligned) {
    let (rows, cols) = (gap.a.1 - gap.a.0, gap.b.1 - gap.b.0);
    if rows.saturating_mul(cols) > MAX_CELLS {
        out.unbounded_gaps += 1;
        return;
    }
    let width = cols + 1;
    let at = |row: usize, col: usize| row * width + col;
    let rung_at = |row: usize, col: usize| common_rung(&a[gap.a.0 + row], &b[gap.b.0 + col]);
    // score[at(row, col)]: best weight aligning the suffixes from `row` of A and `col` of B.
    let mut score = vec![0u64; (rows + 1) * width];
    for row in (0..rows).rev() {
        for col in (0..cols).rev() {
            let diag = rung_at(row, col).map(|r| weight(r) + score[at(row + 1, col + 1)]);
            let skip = score[at(row + 1, col)].max(score[at(row, col + 1)]);
            score[at(row, col)] = diag.map_or(skip, |d| d.max(skip));
        }
    }
    let (mut row, mut col) = (0, 0);
    while row < rows && col < cols {
        let here = score[at(row, col)];
        if let Some(rung) = rung_at(row, col)
            && here == weight(rung) + score[at(row + 1, col + 1)]
        {
            out.matches.push(Match {
                a: gap.a.0 + row,
                b: gap.b.0 + col,
                rung: rung.max(gap.floor),
                method: Method::Bounded,
            });
            row += 1;
            col += 1;
        } else if here == score[at(row + 1, col)] {
            row += 1;
        } else {
            col += 1;
        }
    }
}

/// Pairs leftovers whose text key is unique among the leftovers of both sides.
fn move_pass(a: &[Keys], b: &[Keys], left_a: &[usize], left_b: &[usize]) -> Vec<Match> {
    let mut out = Vec::new();
    let mut taken_a = vec![false; left_a.len()];
    let mut taken_b = vec![false; left_b.len()];
    for rung in MOVE_RUNGS {
        let count = |keys: &[Keys], left: &[usize], taken: &[bool]| {
            let mut seen: HashMap<u128, Option<usize>> = HashMap::new();
            for (n, &i) in left.iter().enumerate() {
                if taken[n] {
                    continue;
                }
                if let Some(key) = keys[i][rung] {
                    seen.entry(key).and_modify(|s| *s = None).or_insert(Some(n));
                }
            }
            seen
        };
        let in_b = count(b, left_b, &taken_b);
        let mut found: Vec<(usize, usize)> = count(a, left_a, &taken_a)
            .iter()
            .filter_map(|(key, &na)| Some((na?, (*in_b.get(key)?)?)))
            .collect();
        found.sort_unstable();
        for (na, nb) in found {
            taken_a[na] = true;
            taken_b[nb] = true;
            out.push(Match {
                a: left_a[na],
                b: left_b[nb],
                rung,
                method: Method::Moved,
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    /// An item keyed only by its text: rungs 0 and 1 carry `t`, the role-box rung carries
    /// `100 + t`, no geometry.
    fn t(text: u128) -> Keys {
        [Some(text), Some(text), Some(100 + text), None]
    }

    fn seq(texts: &[u128]) -> Vec<Keys> {
        texts.iter().copied().map(t).collect()
    }

    /// `(a, b, method)` per match.
    fn pairs(r: &Aligned) -> Vec<(usize, usize, Method)> {
        r.matches.iter().map(|m| (m.a, m.b, m.method)).collect()
    }

    fn assert_mirrored(a: &[Keys], b: &[Keys]) {
        let ab = align(a, b);
        let ba = align(b, a);
        assert_eq!(mirror(ba), ab, "B→A must mirror A→B");
    }

    #[test]
    fn identical_sequences_match_in_order_with_nothing_left() {
        let a = seq(&[1, 2, 3, 2, 1]);
        let r = align(&a, &a);
        assert_eq!(
            pairs(&r),
            (0..5).map(|i| (i, i, Method::Anchor)).collect::<Vec<_>>()
        );
        assert!(r.removed.is_empty() && r.added.is_empty());
        assert_eq!(r.matches.iter().map(|m| m.rung).max(), Some(0));
    }

    #[test]
    fn empty_against_non_empty_is_all_added_or_all_removed() {
        let b = seq(&[1, 2]);
        let r = align(&[], &b);
        assert_eq!(
            (r.matches.len(), r.removed.clone(), r.added.clone()),
            (0, vec![], vec![0, 1])
        );
        let r = align(&b, &[]);
        assert_eq!((r.matches.len(), r.removed, r.added), (0, vec![0, 1], vec![]));
        assert_eq!(align(&[], &[]), Aligned::default());
    }

    #[test]
    fn an_insertion_leaves_exactly_the_new_item_added() {
        let a = seq(&[1, 2, 3, 4]);
        let b = seq(&[1, 2, 9, 3, 4]);
        let r = align(&a, &b);
        assert_eq!(r.added, vec![2]);
        assert_eq!(r.removed, Vec::<usize>::new());
        assert_eq!(
            pairs(&r).iter().map(|&(i, j, _)| (i, j)).collect::<Vec<_>>(),
            vec![(0, 0), (1, 1), (2, 3), (3, 4)]
        );
        assert_mirrored(&a, &b);
    }

    #[test]
    fn a_removal_leaves_exactly_the_old_item_removed() {
        let a = seq(&[1, 2, 3, 4]);
        let b = seq(&[1, 3, 4]);
        let r = align(&a, &b);
        assert_eq!((r.removed.clone(), r.added.clone()), (vec![1], vec![]));
        assert_eq!(r.matches.len(), 3);
        assert_mirrored(&a, &b);
    }

    #[test]
    fn a_moved_item_is_paired_by_the_move_pass() {
        let a = seq(&[1, 2, 3, 4, 5]);
        let b = seq(&[1, 3, 4, 5, 2]);
        let r = align(&a, &b);
        assert!(r.removed.is_empty() && r.added.is_empty(), "{r:?}");
        let moved: Vec<_> = r.matches.iter().filter(|m| m.method == Method::Moved).collect();
        assert_eq!(moved.len(), 1);
        assert_eq!((moved[0].a, moved[0].b, moved[0].rung), (1, 4, 0));
        assert_mirrored(&a, &b);
    }

    #[test]
    fn a_reordering_keeps_the_longest_chain_in_place_and_moves_the_rest() {
        let a = seq(&[1, 2, 3, 4, 5, 6]);
        let b = seq(&[4, 5, 6, 1, 2, 3]);
        let r = align(&a, &b);
        assert!(r.removed.is_empty() && r.added.is_empty());
        assert_eq!(r.matches.len(), 6);
        let anchored = r.matches.iter().filter(|m| m.method == Method::Anchor).count();
        let moved = r.matches.iter().filter(|m| m.method == Method::Moved).count();
        assert_eq!((anchored, moved), (3, 3));
        // Every match pairs equal keys.
        assert!(r.matches.iter().all(|m| a[m.a] == b[m.b]));
        assert_mirrored(&a, &b);
    }

    #[test]
    fn with_no_unique_key_the_gap_falls_to_bounded_alignment() {
        // Every key repeats on both sides and neither end is equal: no trimming, no anchor at
        // any rung, only the weighted alignment.
        let a = seq(&[7, 8, 7, 8]);
        let b = seq(&[8, 7, 8, 7]);
        let r = align(&a, &b);
        let by = |method| r.matches.iter().filter(|m| m.method == method).count();
        assert_eq!((by(Method::Anchor), by(Method::Bounded)), (0, 3), "{r:?}");
        // The two leftover `7`s are unique among the leftovers: the move pass pairs them.
        assert_eq!(by(Method::Moved), 1);
        assert!(r.removed.is_empty() && r.added.is_empty());
        assert!(r.matches.iter().all(|m| a[m.a] == b[m.b]));
        // The in-order matches never cross; only the move pass may.
        let in_order: Vec<_> = r.matches.iter().filter(|m| m.method != Method::Moved).collect();
        assert!(in_order.windows(2).all(|w| w[0].b < w[1].b));
        assert_mirrored(&a, &b);
    }

    #[test]
    fn when_every_key_is_unique_only_anchors_are_used() {
        let a = seq(&[1, 2, 3, 4, 5]);
        let b = seq(&[2, 1, 3, 5, 4]);
        let r = align(&a, &b);
        assert!(r.matches.iter().all(|m| m.method != Method::Bounded));
        assert_eq!(r.matches.len(), 5);
        assert!(r.removed.is_empty() && r.added.is_empty());
        assert_mirrored(&a, &b);
    }

    #[test]
    fn adjacent_duplicates_are_resolved_inside_the_narrow_gap() {
        // `5 5` repeats, but between the unique anchors 1 and 9 the gap is only those two.
        let a = seq(&[1, 5, 5, 9, 5]);
        let b = seq(&[1, 5, 5, 9, 2, 5]);
        let r = align(&a, &b);
        assert_eq!(r.added, vec![4]);
        assert_eq!(r.removed, Vec::<usize>::new());
        assert_eq!(
            pairs(&r).iter().map(|&(i, j, _)| (i, j)).collect::<Vec<_>>(),
            vec![(0, 0), (1, 1), (2, 2), (3, 3), (4, 5)]
        );
        assert_mirrored(&a, &b);
    }

    #[test]
    fn a_text_change_falls_to_the_role_box_rung() {
        // The middle item's text changed; its box (rung 2) and geometry did not.
        let mut a = seq(&[1, 2, 3]);
        let mut b = seq(&[1, 20, 3]);
        a[1] = [Some(2), Some(2), Some(500), Some(600)];
        b[1] = [Some(20), Some(20), Some(500), Some(600)];
        let r = align(&a, &b);
        assert_eq!(r.matches.len(), 3);
        let m = r.matches[1];
        assert_eq!((m.a, m.b, m.rung, m.method), (1, 1, 2, Method::Anchor));
        assert_mirrored(&a, &b);
    }

    #[test]
    fn identical_rows_told_apart_by_their_boxes_pair_on_the_box_rung() {
        // Three rows with the same text (7), each with one cell whose text repeats too (9);
        // only the rows' boxes (rung 2) differ. B lost the middle row and its cell.
        let row = |bx: u128| -> Keys { [Some(7), Some(7), Some(bx), None] };
        let cell = |bx: u128| -> Keys { [Some(9), Some(9), Some(bx), None] };
        let a = vec![t(1), row(50), cell(60), row(51), cell(61), row(52), cell(62)];
        let b = vec![t(1), row(50), cell(60), row(52), cell(62)];
        let r = align(&a, &b);
        assert_eq!((r.removed.clone(), r.added.clone()), (vec![3, 4], vec![]));
        // The rows anchor on their boxes; the cells are unique only inside the gaps those
        // anchors cut, so they are no stronger. Equal texts are claimed by nobody.
        assert_eq!(
            r.matches.iter().map(|m| (m.a, m.b, m.rung)).collect::<Vec<_>>(),
            vec![(0, 0, 0), (1, 1, 2), (2, 2, 2), (5, 3, 2), (6, 4, 2)]
        );
        assert!(r.matches.iter().all(|m| m.method == Method::Anchor));
        assert_mirrored(&a, &b);
    }

    #[test]
    fn a_retag_matches_on_text_alone() {
        // Same text, different role: rung 0 (role + text) differs, rung 1 (text) does not.
        let a = vec![t(1), [Some(10), Some(2), Some(30), None], t(3)];
        let b = vec![t(1), [Some(11), Some(2), Some(31), None], t(3)];
        let r = align(&a, &b);
        assert_eq!(r.matches[1].rung, 1);
        assert!(r.removed.is_empty() && r.added.is_empty());
    }

    #[test]
    fn look_alikes_at_the_end_are_not_paired_onto_an_appended_page() {
        // A: page 1 ends with a figure (no text, box key 50) and a cell whose text repeats on
        // every page (7). B appends page 2, which starts with the unique heading 9 and ends the
        // same way. Trimming the common suffix would pair A's last items with page 2's.
        let figure: Keys = [None, None, Some(50), Some(60)];
        let a = vec![t(1), figure, t(7)];
        let b = vec![t(1), figure, t(7), t(9), figure, t(7)];
        let r = align(&a, &b);
        assert_eq!(
            r.matches.iter().map(|m| (m.a, m.b)).collect::<Vec<_>>(),
            vec![(0, 0), (1, 1), (2, 2)]
        );
        assert_eq!(r.added, vec![3, 4, 5]);
        assert_mirrored(&a, &b);
    }

    #[test]
    fn items_with_no_key_at_all_are_never_matched() {
        let none: Keys = [None; RUNGS];
        let r = align(&[none, t(1)], &[none, t(1)]);
        assert_eq!(pairs(&r), vec![(1, 1, Method::Anchor)]);
        assert_eq!((r.removed.clone(), r.added.clone()), (vec![0], vec![0]));
    }

    #[test]
    fn an_oversized_anchorless_gap_is_counted_and_left_to_the_move_pass() {
        // Alternating keys, shifted by one: no equal prefix or suffix, no unique key, and more
        // cells than the bounded alignment may fill.
        let side = MAX_CELLS.isqrt() + 2;
        let a: Vec<Keys> = (0..side).map(|i| t(1 + (i % 2) as u128)).collect();
        let b: Vec<Keys> = (0..side).map(|i| t(2 - (i % 2) as u128)).collect();
        let r = align(&a, &b);
        assert_eq!(r.unbounded_gaps, 1);
        assert!(
            r.matches.is_empty(),
            "nothing is unique, so the move pass pairs nothing"
        );
        assert_eq!((r.removed.len(), r.added.len()), (side, side));
    }

    #[test]
    fn ordered_places_additions_after_the_match_before_them() {
        let a = seq(&[1, 2, 3]);
        let b = seq(&[0, 1, 9, 3]);
        let r = align(&a, &b);
        assert_eq!(
            r.ordered(a.len()),
            vec![
                (None, Some(0)),
                (Some(0), Some(1)),
                (Some(1), None),
                (None, Some(2)),
                (Some(2), Some(3)),
            ]
        );
        assert_eq!(align(&[], &b).ordered(0).len(), 4);
    }

    /// A container (role 1) holding three paragraphs (role 2), each item keyed by `texts`;
    /// the container is keyed on nothing it shares with the other side.
    fn tree_side(container_key: u128, texts: &[u128]) -> (Vec<Keys>, Vec<Option<usize>>, Vec<u128>) {
        let mut keys = vec![[
            Some(container_key),
            Some(container_key),
            Some(container_key),
            None,
        ]];
        let mut parent = vec![None];
        let mut role = vec![1];
        for &t in texts {
            keys.push(seq(&[t])[0]);
            parent.push(Some(0));
            role.push(2);
        }
        (keys, parent, role)
    }

    #[test]
    fn a_container_whose_children_pair_is_paired_with_theirs() {
        let (ka, pa, ra) = tree_side(1000, &[1, 2, 3]);
        let (kb, pb, rb) = tree_side(2000, &[1, 2, 4]);
        let tree = |parent, role| Tree { parent, role };
        let r = pair_parents(align(&ka, &kb), tree(&pa, &ra), tree(&pb, &rb));
        let root = r.matches.iter().find(|m| m.a == 0).copied();
        assert_eq!(
            root.map(|m| (m.b, m.method, m.rung)),
            Some((0, Method::Children, RUNGS))
        );
        assert_eq!((r.removed.clone(), r.added.clone()), (vec![3], vec![3]));
        // Mirrored.
        let back = pair_parents(align(&kb, &ka), tree(&pb, &rb), tree(&pa, &ra));
        assert_eq!(mirror(back), r);
    }

    #[test]
    fn containers_of_different_roles_are_not_paired_by_their_children() {
        let (ka, pa, ra) = tree_side(1000, &[1, 2]);
        let (kb, pb, mut rb) = tree_side(2000, &[1, 2]);
        rb[0] = 9;
        let r = pair_parents(
            align(&ka, &kb),
            Tree {
                parent: &pa,
                role: &ra,
            },
            Tree {
                parent: &pb,
                role: &rb,
            },
        );
        assert!(r.matches.iter().all(|m| m.a != 0));
        assert_eq!((r.removed.clone(), r.added.clone()), (vec![0], vec![0]));
        assert_eq!(r.matches.len(), 2);
    }

    #[test]
    fn longest_increasing_picks_a_longest_chain() {
        let pairs = [(0, 3), (1, 1), (2, 2), (3, 0), (4, 4)];
        assert_eq!(longest_increasing(&pairs), vec![(1, 1), (2, 2), (4, 4)]);
        assert_eq!(longest_increasing(&[]), vec![]);
    }

    #[test]
    fn mirror_holds_on_a_mixed_edit() {
        let a = seq(&[1, 2, 2, 3, 4, 5, 6, 6, 7]);
        let b = seq(&[2, 1, 3, 3, 9, 5, 6, 7, 4]);
        assert_mirrored(&a, &b);
        let r = align(&a, &b);
        assert_eq!(r.matches.len() + r.removed.len(), a.len());
        assert_eq!(r.matches.len() + r.added.len(), b.len());
    }
}
