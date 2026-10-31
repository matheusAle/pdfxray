//! A captured veraPDF report attached to its fixture resolves findings to pdfxray nodes.
//!
//! It lives here, not in `pdfxray-core`, because the report is parsed by this crate and this
//! crate depends on the core.

use pdfxray_core::{Document, NodeId, TreeKind};
use pretty_assertions::assert_eq;

#[expect(
    clippy::unwrap_used,
    reason = "test helper: missing fixture data must fail the test loudly"
)]
fn orphan_mcid_with_report() -> Document {
    let path = pdfxray_fixtures::pdf_path("orphan-mcid");
    let xml_path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/data/orphan-mcid-ua1.xml");
    let xml = std::fs::read_to_string(xml_path).unwrap();
    let report = pdfxray_verapdf::parse_report(&xml).unwrap();
    let mut doc = Document::open(path).unwrap();
    doc.set_validation(&report);
    doc
}

#[test]
fn findings_resolve_to_nodes() {
    let doc = orphan_mcid_with_report();
    let run = doc.validation().unwrap();

    let resolved: Vec<(&str, Option<String>)> = run
        .findings
        .iter()
        .map(|f| (f.rule.as_str(), f.target.as_ref().map(ToString::to_string)))
        .collect();
    assert_eq!(
        resolved,
        vec![
            ("ISO 14289-1:2014 7.21.4.1-1", Some("res:3:0:Font:F1".to_owned())),
            ("ISO 14289-1:2014 7.1-10", Some("doc".to_owned())),
            ("ISO 14289-1:2014 7.1-8", Some("doc".to_owned())),
            ("ISO 14289-1:2014 7.1-3", Some("mc:0:1".to_owned())),
        ]
    );
    assert!(!run.compliant);
}

#[test]
fn findings_for_node_and_props_link_back() {
    let doc = orphan_mcid_with_report();
    let run = doc.validation().unwrap();
    let mc = NodeId::MarkedContent { page: 0, mcid: 1 };

    let on_mc: Vec<u32> = run.findings_for(&mc).map(|(i, _)| i).collect();
    assert_eq!(on_mc, vec![3]);

    let props = doc.node(&NodeId::Validation(3)).unwrap();
    assert_eq!(props.title, "ISO 14289-1:2014 7.1-3");
    assert_eq!(props.get("Location", "Target"), Some("mc:0:1"));
    assert!(props.get("Rule", "Requirement").is_some_and(|r| !r.is_empty()));
}

#[test]
fn diagnostics_tree_has_validation_group() {
    let doc = orphan_mcid_with_report();

    let tree = doc.tree(TreeKind::Diagnostics);
    let group = tree.iter().find(|g| g.label.starts_with("Validation: ")).unwrap();
    assert_eq!(group.children.len(), 4);
    assert_eq!(group.children[3].id, NodeId::Validation(3));
    assert!(
        group
            .detail
            .as_deref()
            .is_some_and(|d| d.starts_with("not compliant, 4 failed"))
    );
}

#[test]
fn trees_badge_nodes_that_findings_point_at() {
    let doc = orphan_mcid_with_report();
    let badged = |tree: Vec<pdfxray_core::TreeNode>| -> Vec<String> {
        fn walk(n: &pdfxray_core::TreeNode, out: &mut Vec<String>) {
            if n.badges.iter().any(|b| b.code == "validation") {
                out.push(n.id.to_string());
            }
            n.children.iter().for_each(|c| walk(c, out));
        }
        let mut out = Vec::new();
        for n in &tree {
            walk(n, &mut out);
        }
        out
    };

    // The 7.1-3 finding targets mc:0:1; the content tree lists it as its BDC operator's
    // sequence, the resources tree lists the flagged font.
    assert_eq!(
        badged(doc.tree(TreeKind::Resources)),
        vec!["res:3:0:Font:F1".to_owned()]
    );
    assert_eq!(badged(doc.tree(TreeKind::Diagnostics)), Vec::<String>::new());
    assert_eq!(badged(doc.tree(TreeKind::Pages)), Vec::<String>::new());
}

#[test]
fn validation_badges_are_added_per_request_and_never_stick_to_the_cached_trees() {
    let path = pdfxray_fixtures::pdf_path("orphan-mcid");
    let xml_path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/data/orphan-mcid-ua1.xml");
    let report = pdfxray_verapdf::parse_report(&std::fs::read_to_string(xml_path).unwrap()).unwrap();
    let mut doc = Document::open(path).unwrap();
    let font = NodeId::Resource {
        owner: pdfxray_core::ObjRef::new(3, 0),
        kind: pdfxray_core::ResKind::Font,
        name: "F1".to_owned(),
    };
    let badges = |doc: &Document| {
        doc.tree(TreeKind::Resources)
            .iter()
            .find_map(|n| n.find(&font))
            .map(|n| n.badges.len())
    };

    let before = badges(&doc);
    doc.set_validation(&report);
    let first = badges(&doc);
    let second = badges(&doc);
    doc.set_validation(&report);
    let replaced = badges(&doc);

    assert_eq!(before, Some(0), "no run attached yet");
    assert_eq!(first, Some(1));
    assert_eq!(second, Some(1), "asking again must not stack another badge");
    assert_eq!(replaced, Some(1), "a replaced run still badges once");
    assert_eq!(
        doc.tree(TreeKind::Objects),
        doc.tree(TreeKind::Objects),
        "repeated requests agree"
    );
}
