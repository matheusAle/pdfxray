//! What got fixed and what broke: the diagnostics diff over the fixture pairs.
//!
//! The document model's own diagnostics need no tool and are always compared. veraPDF findings
//! are compared from reports attached through [`Document::set_validation`], the path a real run
//! takes; the reports here are written by hand, so the comparison is tested without an install.

#![allow(
    clippy::unwrap_used,
    clippy::panic,
    reason = "test helpers fail loudly on unexpected input"
)]

use pdfxray_core::diff::{self, Anchor, FindingChange, FindingsDiff, Pairing, Side, ValidationDiff};
use pdfxray_core::validation::{Finding, Locator, Report, RuleId, Status};
use pdfxray_core::{Document, NodeId, Severity};
use pdfxray_fixtures::pairs::{self, Mutation, PAIRS};
use pretty_assertions::assert_eq;

fn open(name: &str) -> (Document, Document) {
    let (a, b) = pairs::pair(name);
    (Document::open(&a).unwrap(), Document::open(&b).unwrap())
}

fn id(num: Option<u32>) -> Option<NodeId> {
    num.map(|n| NodeId::StructElem(pdfxray_core::ObjRef::new(n, 0)))
}

/// `(code, severity, anchor)` of each change: what is compared, without the wording.
fn shape(changes: &[FindingChange]) -> Vec<(&str, Option<Severity>, &Anchor)> {
    changes
        .iter()
        .map(|c| (c.code.as_str(), c.severity, &c.on))
        .collect()
}

fn element_anchor(p: &Pairing, a: Option<NodeId>, b: Option<NodeId>) -> Anchor {
    let pair = p.pairs.iter().position(|x| x.a == a && x.b == b).unwrap();
    Anchor::Element { pair, a, b }
}

#[test]
fn a_dropped_alt_introduces_the_missing_alt_check_on_the_paired_figure() {
    let facts = pairs::pair_mutation("drop-alt");
    let Some(Mutation::AltRemoved { element, .. }) = facts.mutations.first() else {
        panic!("drop-alt removes an /Alt");
    };
    assert_eq!(element.role, "Figure");
    let (a, b) = open("drop-alt");
    let p = diff::pair(&a, &b);
    // A Figure without /Alt (or /ActualText) fails the figure check: introduced on its pair.
    let on = element_anchor(&p, id(element.a), id(element.b));
    assert_eq!(
        shape(&p.diagnostics.core.introduced),
        vec![("figure-alt-missing", Some(Severity::Error), &on)]
    );
    assert_eq!(p.diagnostics.core.fixed, vec![]);
    // Seen from B, putting the /Alt back fixes it, on the same pair seen from the other side.
    let back = diff::pair(&b, &a);
    let on_back = element_anchor(&back, id(element.b), id(element.a));
    assert_eq!(
        shape(&back.diagnostics.core.fixed),
        vec![("figure-alt-missing", Some(Severity::Error), &on_back)]
    );
    // Smoke: what both sides share is counted, not reported.
    assert!(p.diagnostics.core.unchanged >= 1);
}

#[test]
fn swapped_siblings_with_unmoved_ink_introduce_a_reading_order_backtrack() {
    let facts = pairs::pair_mutation("reorder-tags");
    let Some(Mutation::SiblingsSwapped { first, second, .. }) = facts.mutations.first() else {
        panic!("reorder-tags swaps two siblings");
    };
    let (a, b) = open("reorder-tags");
    let p = diff::pair(&a, &b);
    let introduced = &p.diagnostics.core.introduced;
    assert_eq!(introduced.len(), 1, "{introduced:#?}");
    assert_eq!(
        (introduced[0].code.as_str(), introduced[0].severity),
        ("reading-order-backtrack", Some(Severity::Warning))
    );
    // It concerns one of the two siblings: the order is now against the layout.
    let candidates = [first, second].map(|el| element_anchor(&p, id(el.a), id(el.b)));
    assert!(candidates.contains(&introduced[0].on), "{:?}", introduced[0].on);
    // Smoke: nothing was fixed by a swap.
    assert_eq!(p.diagnostics.core.fixed, vec![]);
}

#[test]
fn the_other_pairs_change_no_diagnostic() {
    // Renumbering, re-encoding, translating, artifact text and the identical-rows removal are
    // invisible to every check, by construction. The moved paragraph (3 pt), the heading-level
    // and figure retags and the inserted page happen to break no check either: pinned here.
    let quiet = [
        "renumber-objects",
        "rewrite-content-same-ink",
        "global-shift-2pt",
        "move-paragraph-3pt",
        "change-heading-level",
        "retag-figure-as-formula",
        "insert-page",
        "artifact-page-number",
        "identical-rows",
    ];
    for name in quiet {
        let (a, b) = open(name);
        let d = diff::pair(&a, &b).diagnostics;
        assert!(d.is_unchanged(), "{name}: {d:#?}");
        assert_eq!(d.validation, ValidationDiff::NotRun, "{name}");
        // Smoke: there were findings to compare, and they paired.
        assert!(d.core.unchanged >= 1, "{name}");
    }
}

#[test]
fn fixed_and_introduced_mirror_when_the_sides_swap() {
    let mut changed = 0;
    for name in PAIRS {
        let (a, b) = open(name);
        let (ab, ba) = (diff::pair(&a, &b).diagnostics, diff::pair(&b, &a).diagnostics);
        let words = |c: &[FindingChange]| -> Vec<(String, Option<Severity>, String)> {
            c.iter()
                .map(|x| (x.code.clone(), x.severity, x.message.clone()))
                .collect()
        };
        assert_eq!(words(&ab.core.fixed), words(&ba.core.introduced), "{name}");
        assert_eq!(words(&ab.core.introduced), words(&ba.core.fixed), "{name}");
        assert_eq!(ab.core.unchanged, ba.core.unchanged, "{name}");
        changed += ab.core.fixed.len() + ab.core.introduced.len();
    }
    // Smoke: some pairs do change diagnostics, so the mirror is exercised.
    assert!(changed >= 2, "{changed}");
}

// ---- veraPDF findings, from hand-written reports -------------------------------------------

fn finding(rule: &str, locator: Locator) -> Finding {
    let (id, test) = rule.rsplit_once('-').unwrap();
    let (specification, clause) = id.rsplit_once(' ').unwrap();
    Finding {
        rule: RuleId {
            specification: specification.to_owned(),
            clause: clause.to_owned(),
            test_number: test.parse().unwrap(),
        },
        description: format!("requirement {rule}"),
        status: Status::Failed,
        context: String::new(),
        locator,
        error_message: None,
        error_arguments: Vec::new(),
        object: String::new(),
        tags: Vec::new(),
    }
}

fn report(profile: &str, findings: Vec<Finding>) -> Report {
    Report {
        profile: profile.to_owned(),
        compliant: findings.is_empty(),
        checks_passed: 10,
        checks_failed: u64::try_from(findings.len()).unwrap(),
        findings,
        verapdf_version: None,
        raw_xml: String::new(),
    }
}

const UA1: &str = "PDF/UA-1 validation profile";

fn compared(p: &Pairing) -> &FindingsDiff {
    match &p.diagnostics.validation {
        ValidationDiff::Compared(f) => f,
        other => panic!("not compared: {other:?}"),
    }
}

#[test]
fn findings_on_renumbered_objects_pair_through_their_elements_and_pages() {
    // The same three findings on both sides, located by object number, which renumbering
    // changed: on an element, on a page, and on the document.
    let facts = pairs::pair_mutation("renumber-objects");
    let Some(Mutation::Renumbered { objects, .. }) = facts.mutations.first() else {
        panic!("renumber-objects renumbers");
    };
    let chart = objects.iter().find(|o| o.label == "el.chart").unwrap();
    let (page_a, page_b) = (facts.a.page_objects[1], facts.b.page_objects[1]);
    assert!(chart.a != chart.b && page_a != page_b, "the numbers differ");
    let (a, b) = open("renumber-objects");
    let side = |chart: u32, page: u32| {
        report(
            UA1,
            vec![
                finding(
                    "ISO 14289-1:2014 7.3-1",
                    Locator::StructElem {
                        obj: Some((chart, 0)),
                        path: vec![0, 4],
                    },
                ),
                finding(
                    "ISO 14289-1:2014 7.18.1-2",
                    Locator::Object {
                        num: page,
                        generation: 0,
                    },
                ),
                finding("ISO 14289-1:2014 7.1-8", Locator::Document),
            ],
        )
    };
    a.set_validation(&side(chart.a, page_a));
    b.set_validation(&side(chart.b, page_b));
    let p = diff::pair(&a, &b);
    let f = compared(&p);
    assert!(f.is_unchanged(), "{f:#?}");
    assert_eq!(f.unchanged, 3);
    // Smoke: the element finding really is on a renumbered element.
    assert!(
        p.pairs
            .iter()
            .any(|x| x.a == id(Some(chart.a)) && x.b == id(Some(chart.b)))
    );

    // The page finding is placed by page, not lumped with the document: moved to page 3 in B,
    // it is fixed on page 2 and introduced on page 3.
    let page = |n: u32| {
        finding(
            "ISO 14289-1:2014 7.18.1-2",
            Locator::Object {
                num: n,
                generation: 0,
            },
        )
    };
    a.set_validation(&report(UA1, vec![page(page_a)]));
    b.set_validation(&report(UA1, vec![page(facts.b.page_objects[2])]));
    let moved = diff::diagnostics(&a, &b, &p);
    let ValidationDiff::Compared(f) = &moved.validation else {
        panic!("{moved:?}");
    };
    let on = |n: u32| Anchor::Page {
        a: Some(n),
        b: Some(n),
    };
    assert_eq!(shape(&f.fixed), vec![("ISO 14289-1:2014 7.18.1-2", None, &on(1))]);
    assert_eq!(
        shape(&f.introduced),
        vec![("ISO 14289-1:2014 7.18.1-2", None, &on(2))]
    );
}

#[test]
fn a_finding_gone_from_the_paired_element_is_fixed_and_a_new_one_introduced() {
    let facts = pairs::pair_mutation("drop-alt");
    let Some(Mutation::AltRemoved { element, .. }) = facts.mutations.first() else {
        panic!("drop-alt removes an /Alt");
    };
    let (a, b) = open("drop-alt");
    let on_chart = |num: Option<u32>| Locator::StructElem {
        obj: Some((num.unwrap(), 0)),
        path: vec![0, 4],
    };
    // A failed a page check that B passes; B fails the figure check A passed.
    a.set_validation(&report(
        UA1,
        vec![finding("ISO 14289-1:2014 7.18.1-2", Locator::Page { index: 0 })],
    ));
    b.set_validation(&report(
        UA1,
        vec![finding("ISO 14289-1:2014 7.3-1", on_chart(element.b))],
    ));
    let p = diff::pair(&a, &b);
    let f = compared(&p);
    let on = element_anchor(&p, id(element.a), id(element.b));
    let page0 = Anchor::Page {
        a: Some(0),
        b: Some(0),
    };
    assert_eq!(shape(&f.introduced), vec![("ISO 14289-1:2014 7.3-1", None, &on)]);
    assert_eq!(shape(&f.fixed), vec![("ISO 14289-1:2014 7.18.1-2", None, &page0)]);
    assert_eq!(f.unchanged, 0);
    // Smoke: the message falls back to the requirement when veraPDF gave none.
    assert_eq!(f.introduced[0].message, "requirement ISO 14289-1:2014 7.3-1");
}

#[test]
fn findings_are_compared_only_between_two_runs_of_one_profile() {
    let (a, b) = open("drop-alt");
    assert_eq!(diff::pair(&a, &b).diagnostics.validation, ValidationDiff::NotRun);
    a.set_validation(&report(UA1, vec![]));
    assert_eq!(
        diff::pair(&a, &b).diagnostics.validation,
        ValidationDiff::OneSide { side: Side::A }
    );
    b.set_validation(&report("WCAG 2.2", vec![]));
    let p = diff::pair(&a, &b);
    assert_eq!(
        p.diagnostics.validation,
        ValidationDiff::ProfilesDiffer {
            a: UA1.to_owned(),
            b: "WCAG 2.2".to_owned()
        }
    );
    // A run attached after pairing is compared by asking again.
    b.set_validation(&report(UA1, vec![]));
    let again = diff::diagnostics(&a, &b, &p);
    assert_eq!(
        again.validation,
        ValidationDiff::Compared(FindingsDiff::default())
    );
    // Smoke: the core diagnostics are compared whatever the runs.
    assert_eq!(again.core, p.diagnostics.core);
}
