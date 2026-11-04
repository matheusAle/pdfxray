//! The pairing proved on the fixture pairs.
//!
//! Each pair's expected diff is *derived* here from its declared [`MutationFacts`], which speak
//! only in fixture terms (objects renumbered, the element whose role or `/Alt` changed, the
//! offset in points, the page inserted). The derivation is this file's; the facts know nothing
//! of statuses, so the comparison is not circular.

#![allow(
    clippy::unwrap_used,
    clippy::panic,
    clippy::float_cmp,
    reason = "test helpers fail loudly; offsets and deltas are exact multiples of a point"
)]

use std::collections::BTreeSet;

use pdfxray_core::diff::{self, Field, Method, Pair, Pairing, Rung, Status, Summary};
use pdfxray_core::{Document, PageIx, UnicodeSource};
use pdfxray_fixtures::pairs::{self, ElementRef, Mutation, MutationFacts, PAIRS};
use pretty_assertions::assert_eq;

fn open(name: &str) -> (Document, Document) {
    let (a, b) = pairs::pair(name);
    (Document::open(&a).unwrap(), Document::open(&b).unwrap())
}

fn pairing(name: &str) -> Pairing {
    let (a, b) = open(name);
    diff::pair(&a, &b)
}

fn page(p: usize) -> PageIx {
    PageIx::try_from(p).unwrap()
}

/// The id an element of the generated files gets: they write every object at generation 0.
fn id(num: Option<u32>) -> Option<String> {
    num.map(|n| format!("struct:{n}:0"))
}

/// One changed pair, as compared here. A `bbox` field's values are elided: the facts state the
/// translation, which [`Change::delta`] carries.
#[derive(Debug, Clone, PartialEq)]
struct Change {
    status: Status,
    a: Option<String>,
    b: Option<String>,
    page_a: Option<PageIx>,
    page_b: Option<PageIx>,
    fields: Vec<(Field, Option<String>, Option<String>)>,
    delta: Option<(f64, f64)>,
    reordered: bool,
    rung: Option<Rung>,
}

fn observed(p: &Pair) -> Change {
    Change {
        status: p.status,
        a: p.a.as_ref().map(ToString::to_string),
        b: p.b.as_ref().map(ToString::to_string),
        page_a: p.page_a,
        page_b: p.page_b,
        fields: p
            .changed
            .iter()
            .map(|c| match c.field {
                Field::Bbox => (Field::Bbox, None, None),
                f => (f, c.a.clone(), c.b.clone()),
            })
            .collect(),
        delta: p.delta,
        reordered: p.reordered,
        rung: p.confidence.rung,
    }
}

/// The rung an element present on both sides must be paired on, from what the mutation kept:
/// text and role, the text alone, the role and its box, or only its box.
fn rung_for(has_text: bool, role_kept: bool) -> Rung {
    match (has_text, role_kept) {
        (true, true) => Rung::RoleText,
        (true, false) => Rung::Text,
        (false, true) => Rung::RoleBox,
        (false, false) => Rung::Geometry,
    }
}

/// The standard structure types the generator uses, so a role is its own standard type (the
/// files carry no role map).
const STANDARD: [&str; 14] = [
    "Document", "Sect", "Div", "P", "H1", "H2", "H3", "H4", "Figure", "Formula", "Table", "TR", "TD", "TH",
];

fn standard(role: &str) -> String {
    assert!(
        STANDARD.contains(&role),
        "`{role}` is not a standard type the derivation knows"
    );
    role.to_owned()
}

fn heading_level(role: &str) -> Option<String> {
    role.strip_prefix('H')?.parse::<u8>().ok().map(|l| l.to_string())
}

/// An element present on both sides, unchanged unless the caller says otherwise.
fn kept(el: &ElementRef, status: Status) -> Change {
    Change {
        status,
        a: id(el.a),
        b: id(el.b),
        page_a: el.page_a.map(page),
        page_b: el.page_b.map(page),
        fields: Vec::new(),
        delta: Some((0.0, 0.0)),
        reordered: false,
        rung: Some(rung_for(el.text.is_some(), true)),
    }
}

/// An element of one side only. A container's page is its children's, when they share one.
fn lone(el: &ElementRef, status: Status, children: &[ElementRef]) -> Change {
    let shared = |pages: Vec<Option<usize>>| {
        let set: BTreeSet<_> = pages.into_iter().collect();
        (set.len() == 1)
            .then(|| set.into_iter().next().flatten())
            .flatten()
    };
    let page_a = el
        .page_a
        .or_else(|| shared(children.iter().map(|c| c.page_a).collect()));
    let page_b = el
        .page_b
        .or_else(|| shared(children.iter().map(|c| c.page_b).collect()));
    Change {
        status,
        a: id(el.a),
        b: id(el.b),
        page_a: page_a.filter(|_| status == Status::Removed).map(page),
        page_b: page_b.filter(|_| status == Status::Added).map(page),
        fields: Vec::new(),
        delta: None,
        reordered: false,
        rung: None,
    }
}

/// The diff a pair's facts predict.
#[derive(Debug)]
struct Expected {
    /// Acceptable lists of changed pairs, in pairing order; usually one.
    changes: Vec<Vec<Change>>,
    offset: Option<(f64, f64)>,
    /// `(page of A, page of B)` of every page pair, in order.
    pages: Vec<(Option<PageIx>, Option<PageIx>)>,
    /// The page pairs whose content outside the structure tree differs.
    residue: BTreeSet<(Option<PageIx>, Option<PageIx>)>,
}

impl Expected {
    /// Appends `alternatives` to every list of changes so far.
    fn then(&mut self, alternatives: &[Vec<Change>]) {
        self.changes = self
            .changes
            .iter()
            .flat_map(|so_far| {
                alternatives.iter().map(move |alt| {
                    let mut v = so_far.clone();
                    v.extend(alt.iter().cloned());
                    v
                })
            })
            .collect();
    }
}

/// Derives the expected diff from the facts alone.
fn expected(facts: &MutationFacts) -> Expected {
    let mut e = Expected {
        changes: vec![Vec::new()],
        offset: None,
        pages: (0..facts.a.page_count)
            .map(|p| (Some(page(p)), Some(page(p))))
            .collect(),
        residue: BTreeSet::new(),
    };
    for m in &facts.mutations {
        match m {
            // Object identity and content-stream encoding are invisible to a pairing.
            Mutation::Renumbered { .. } | Mutation::ContentRewritten { .. } => {}
            Mutation::Translated { dx, dy } => e.offset = Some((*dx, *dy)),
            // Two siblings swapped: one of them changed place in the reading order, the other
            // kept it; which one is the aligner's choice. Their ink did not move.
            Mutation::SiblingsSwapped { first, second, .. } => e.then(&[first, second].map(|el| {
                vec![Change {
                    reordered: true,
                    ..kept(el, Status::Moved)
                }]
            })),
            Mutation::ElementMoved { element, dx, dy } => e.then(&[vec![Change {
                fields: vec![(Field::Bbox, None, None)],
                delta: Some((*dx, *dy)),
                ..kept(element, Status::Moved)
            }]]),
            Mutation::AltRemoved { element, alt } => e.then(&[vec![Change {
                fields: vec![(Field::Alt, Some(alt.clone()), None)],
                ..kept(element, Status::Modified)
            }]]),
            Mutation::RoleChanged { element, from, to } => {
                let mut fields = vec![
                    (Field::Role, Some(from.clone()), Some(to.clone())),
                    (Field::Standard, Some(standard(from)), Some(standard(to))),
                ];
                let (la, lb) = (heading_level(from), heading_level(to));
                if la != lb {
                    fields.push((Field::Level, la, lb));
                }
                fields.sort_by_key(|f| f.0);
                e.then(&[vec![Change {
                    fields,
                    rung: Some(rung_for(element.text.is_some(), false)),
                    ..kept(element, Status::Modified)
                }]]);
            }
            Mutation::PageInserted { index, elements, .. } => {
                e.then(&[elements.iter().map(|el| lone(el, Status::Added, &[])).collect()]);
                // Pages from `index` on correspond one later; the new page has no partner and
                // everything painted on it is new.
                for (_, b) in &mut e.pages {
                    if let Some(p) = b.as_mut()
                        && usize::try_from(*p).unwrap() >= *index
                    {
                        *p += 1;
                    }
                }
                e.pages.insert(*index, (None, Some(page(*index))));
                e.residue.insert((None, Some(page(*index))));
            }
            Mutation::ArtifactTextChanged { changes } => {
                for c in changes {
                    e.residue.insert((Some(page(c.page_a)), Some(page(c.page_b))));
                }
            }
            Mutation::ElementRemoved {
                element, descendants, ..
            } => {
                let mut gone = vec![lone(element, Status::Removed, descendants)];
                gone.extend(descendants.iter().map(|d| lone(d, Status::Removed, &[])));
                e.then(&[gone]);
            }
        }
    }
    e
}

/// Pairs `name` and checks it against what its facts predict; returns the pairing for the
/// caller's own claims.
fn check(name: &str) -> Pairing {
    let facts = pairs::pair_mutation(name);
    let want = expected(facts);
    let p = pairing(name);

    // Every element of either side is in exactly one pair.
    assert_eq!(
        (p.sides.a.pages, p.sides.a.elements),
        (facts.a.page_count, facts.a.element_count),
        "{name}: side A"
    );
    assert_eq!(
        (p.sides.b.pages, p.sides.b.elements),
        (facts.b.page_count, facts.b.element_count),
        "{name}: side B"
    );
    assert_eq!(
        p.pairs.iter().filter(|x| x.a.is_some()).count(),
        facts.a.element_count
    );
    assert_eq!(
        p.pairs.iter().filter(|x| x.b.is_some()).count(),
        facts.b.element_count
    );

    let changes: Vec<Change> = p
        .pairs
        .iter()
        .filter(|x| x.status != Status::Same)
        .map(observed)
        .collect();
    if !want.changes.contains(&changes) {
        if let [only] = want.changes.as_slice() {
            assert_eq!(&changes, only, "{name}: changed pairs");
        }
        panic!(
            "{name}: changed pairs {changes:#?}\nmatch none of {:#?}",
            want.changes
        );
    }

    assert_eq!(p.offset.map(|o| (o.dx, o.dy)), want.offset, "{name}: offset");
    assert_eq!(
        p.pages.iter().map(|m| (m.a, m.b)).collect::<Vec<_>>(),
        want.pages,
        "{name}: page correspondence"
    );
    let residue: BTreeSet<_> = p
        .pages
        .iter()
        .filter(|m| m.residue.only_a > 0 || m.residue.only_b > 0)
        .map(|m| (m.a, m.b))
        .collect();
    assert_eq!(residue, want.residue, "{name}: pages whose residue changed");
    // Nothing limited the comparison: none of the pairs is encrypted or anchorless at scale.
    assert_eq!(p.notes, vec![], "{name}");
    p
}

fn assert_empty(p: &Pairing) {
    assert_eq!(
        p.summary(),
        Summary {
            same: p.sides.a.elements,
            ..Summary::default()
        }
    );
    assert!(p.pages.iter().all(diff::PageMatch::is_unchanged), "{:?}", p.pages);
}

#[test]
fn renumbered_objects_pair_as_identical() {
    let p = check("renumber-objects");
    assert_empty(&p);
    assert_eq!(p.offset, None);
    // Smoke: object numbers really differ, so the ids the pairs name differ by side.
    assert!(
        p.pairs.iter().any(|x| x.a != x.b),
        "the pair renumbers its elements"
    );
}

#[test]
fn rewritten_content_with_the_same_ink_pairs_as_identical() {
    let p = check("rewrite-content-same-ink");
    assert_empty(&p);
    assert_eq!(p.offset, None);
    // Smoke: every element pairs on its strongest rung, text matches included.
    assert!(p.pairs.iter().any(|x| x.confidence.rung == Some(Rung::RoleText)));
}

#[test]
fn a_global_shift_is_one_offset_and_no_difference() {
    let p = check("global-shift-2pt");
    assert_empty(&p);
    let offset = p.offset.unwrap();
    assert_eq!((offset.dx, offset.dy), (2.0, -2.0));
    // Smoke: every pair is registered, figures without text (paired on their boxes) included,
    // and their Layout `/BBox` attributes do not show as changed.
    let figures: Vec<&Pair> = p.pairs.iter().filter(|x| x.role == "Figure").collect();
    assert_eq!(figures.len(), 2);
    assert!(
        figures.iter().all(|x| x.delta == Some((0.0, 0.0))
            && x.rows.is_empty()
            && x.confidence.rung == Some(Rung::RoleBox)),
        "{figures:#?}"
    );
    assert!(offset.pairs >= 3, "estimated from {} pairs", offset.pairs);
}

#[test]
fn swapped_siblings_are_one_reordered_element_and_no_ink_change() {
    let p = check("reorder-tags");
    let moved: Vec<&Pair> = p.pairs.iter().filter(|x| x.status == Status::Moved).collect();
    assert_eq!(moved.len(), 1);
    assert!(moved[0].reordered && moved[0].changed.is_empty());
    assert_eq!(moved[0].confidence.method, Some(Method::Moved));
    // Smoke: no page residue changed — order is not ink.
    assert!(p.pages.iter().all(diff::PageMatch::is_unchanged));
}

#[test]
fn a_paragraph_moved_3pt_is_moved_by_its_residual() {
    let p = check("move-paragraph-3pt");
    let moved = p.pairs.iter().find(|x| x.status == Status::Moved).unwrap();
    assert_eq!(moved.delta, Some((0.0, -3.0)));
    assert!(!moved.reordered);
    // Smoke: one moving paragraph is not a global offset.
    assert_eq!(p.offset, None);
}

#[test]
fn a_dropped_alt_is_a_typed_alt_change() {
    let p = check("drop-alt");
    let x = p.pairs.iter().find(|x| x.status == Status::Modified).unwrap();
    assert_eq!(x.role, "Figure");
    // Smoke: the generic layer says the same as the typed one.
    assert_eq!(
        x.rows
            .iter()
            .map(|r| (r.group.as_str(), r.key.as_str()))
            .collect::<Vec<_>>(),
        vec![("Element", "Alt")]
    );
}

#[test]
fn a_changed_heading_level_is_a_typed_level_change() {
    let p = check("change-heading-level");
    let x = p.pairs.iter().find(|x| x.status == Status::Modified).unwrap();
    let level = x.changed.iter().find(|c| c.field == Field::Level).unwrap();
    assert_eq!((level.a.as_deref(), level.b.as_deref()), (Some("2"), Some("3")));
    // Smoke: it paired on its text, the role being what changed.
    assert_eq!(x.confidence.rung, Some(Rung::Text));
}

#[test]
fn a_figure_retagged_as_formula_is_a_typed_role_change() {
    let p = check("retag-figure-as-formula");
    let x = p.pairs.iter().find(|x| x.status == Status::Modified).unwrap();
    assert!(x.changed.iter().all(|c| c.field != Field::Level));
    // Smoke: no text and a new role leave only the box to pair on — the bottom rung.
    assert_eq!(x.confidence.rung, Some(Rung::Geometry));
}

#[test]
fn an_inserted_page_is_derived_from_the_pairs_not_matched() {
    let p = check("insert-page");
    assert_eq!(p.summary().added, 2);
    // Smoke: later pages correspond one on, and that rests on element evidence.
    let later: Vec<_> = p.pages.iter().filter(|m| m.a >= Some(1)).collect();
    assert!(
        later.iter().all(|m| m.b == m.a.map(|a| a + 1) && m.pairs > 0),
        "{later:?}"
    );
}

#[test]
fn a_changed_page_number_artifact_is_page_residue_only() {
    let p = check("artifact-page-number");
    assert_eq!(p.summary().same, p.sides.a.elements);
    // Smoke: every page's residue changed, and nothing else did.
    assert!(
        p.pages
            .iter()
            .all(|m| m.a == m.b && m.residue.only_a > 0 && m.residue.only_b > 0)
    );
}

#[test]
fn a_removed_identical_row_is_removed_and_its_look_alikes_pair_on_their_boxes() {
    let p = check("identical-rows");
    let Some(Mutation::ElementRemoved {
        element, descendants, ..
    }) = pairs::pair_mutation("identical-rows").mutations.first()
    else {
        panic!("identical-rows removes an element");
    };
    // What the removed row and its cells read: the cells' texts, and the row's is theirs.
    let cells: Vec<&str> = descendants.iter().filter_map(|d| d.text.as_deref()).collect();
    let row_text = element.text.clone().unwrap_or_else(|| cells.join(" "));
    let mut look_alikes = vec![(element.role.as_str(), row_text.as_str())];
    look_alikes.extend(
        descendants
            .iter()
            .filter_map(|d| Some((d.role.as_str(), d.text.as_deref()?))),
    );
    // Their text repeats in A, so it is no evidence: every surviving look-alike is paired on a
    // box rung, never claimed as a text match.
    let survivors: Vec<&Pair> = p
        .pairs
        .iter()
        .filter(|x| x.status == Status::Same && look_alikes.contains(&(x.role.as_str(), x.text.as_str())))
        .collect();
    assert!(
        survivors.iter().all(|x| x.confidence.rung >= Some(Rung::RoleBox)),
        "{survivors:#?}"
    );
    // Smoke: three rows of a row and two cells survive.
    assert_eq!(survivors.len(), 9);
}

#[test]
fn every_pair_is_checked() {
    // The per-pair tests above, by name: a pair added to the catalog must get one.
    let tested = [
        "renumber-objects",
        "rewrite-content-same-ink",
        "global-shift-2pt",
        "reorder-tags",
        "move-paragraph-3pt",
        "drop-alt",
        "change-heading-level",
        "retag-figure-as-formula",
        "insert-page",
        "artifact-page-number",
        "identical-rows",
    ];
    assert_eq!(tested, PAIRS);
}

#[test]
fn text_matched_without_to_unicode_says_so_on_every_pair() {
    // The generator writes standard-14 fonts with an encoding and no `/ToUnicode`.
    let mut with_text = 0;
    for name in PAIRS {
        let p = pairing(name);
        for x in &p.pairs {
            let painted_text = !x.text.is_empty();
            for (present, source) in [
                (x.a.is_some(), x.confidence.unicode_a),
                (x.b.is_some(), x.confidence.unicode_b),
            ] {
                let want = (present && painted_text).then_some(UnicodeSource::Encoding);
                assert_eq!(source, want, "{name}: {x:?}");
            }
            assert!(
                !x.confidence.degraded_a && !x.confidence.degraded_b,
                "{name}: {x:?}"
            );
            with_text += usize::from(painted_text);
        }
        assert!(p.sides.a.degraded_pages.is_empty() && p.sides.b.degraded_pages.is_empty());
    }
    // Smoke: most pairs carry text.
    assert!(with_text > 100, "{with_text}");
}

#[test]
fn every_pair_mirrors_when_the_sides_swap() {
    for name in PAIRS {
        let (a, b) = open(name);
        let (ab, ba) = (diff::pair(&a, &b), diff::pair(&b, &a));
        let s = ab.summary();
        assert_eq!(
            ba.summary(),
            Summary {
                added: s.removed,
                removed: s.added,
                ..s
            },
            "{name}"
        );
        assert_eq!(
            ba.offset.map(|o| (o.dx, o.dy)),
            ab.offset.map(|o| (-o.dx, -o.dy)),
            "{name}"
        );
        let statuses = |p: &Pairing, flip: bool| -> BTreeSet<(Option<String>, Option<String>, Status)> {
            p.pairs
                .iter()
                .map(|x| {
                    let (a, b) = (
                        x.a.as_ref().map(ToString::to_string),
                        x.b.as_ref().map(ToString::to_string),
                    );
                    if flip {
                        (b, a, x.status.mirrored())
                    } else {
                        (a, b, x.status)
                    }
                })
                .collect()
        };
        assert_eq!(statuses(&ba, true), statuses(&ab, false), "{name}");
    }
    // Smoke: the pairs include some that differ, so mirroring is exercised.
    assert!(!pairing("insert-page").summary().is_unchanged());
}
