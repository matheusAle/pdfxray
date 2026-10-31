//! Malformed and unusual structure trees, built in memory: every defect must surface as a
//! diagnostic (and a badge), and nothing may loop, overflow or panic.

#![allow(
    clippy::unwrap_used,
    clippy::panic,
    reason = "test helpers fail loudly on unexpected input"
)]

use std::fmt::Write as _;

use pdfxray_core::{Document, LayerMode, NodeId, ObjRef, Point, Severity, TreeKind, TreeNode};
use pdfxray_fixtures::{PdfVersion, PdfWriter, Trailer};
use pretty_assertions::assert_eq;

const MARKED: &str = "/MarkInfo << /Marked true >>";
const ROOT_WITH_PT: &str = "<< /Type /StructTreeRoot /K 7 0 R /ParentTree 20 0 R >>";

/// A one-page document: catalog 1, pages 2, page 3, content 4, font 5, then `objects`
/// (the structure tree root must be object 6).
fn build(catalog_extra: &str, page_extra: &str, mcids: &[i32], objects: &[(u32, String)]) -> Document {
    let mut content = String::new();
    for n in mcids {
        let y = 180 - 20 * n;
        let _ = writeln!(
            content,
            "/P << /MCID {n} >> BDC BT /F1 12 Tf 20 {y} Td (text {n}) Tj ET EMC"
        );
    }
    let mut w = PdfWriter::new(PdfVersion::V1_7);
    w.object(
        1,
        format!("<< /Type /Catalog /Pages 2 0 R /StructTreeRoot 6 0 R {catalog_extra} >>"),
    );
    w.object(2, "<< /Type /Pages /Kids [3 0 R] /Count 1 >>");
    w.object(
        3,
        format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] {page_extra} \
             /Resources << /Font << /F1 5 0 R >> >> /Contents 4 0 R >>"
        ),
    );
    w.stream(4, "", content.as_bytes());
    w.object(5, "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>");
    let mut sorted: Vec<_> = objects.iter().collect();
    sorted.sort_by_key(|(n, _)| *n);
    for (n, body) in sorted {
        w.object(*n, body);
    }
    w.finish_table(&Trailer::new(1)).unwrap();
    Document::from_bytes("edge.pdf", w.into_output().unwrap().bytes).unwrap()
}

fn s(n: u32, body: &str) -> (u32, String) {
    (n, body.to_owned())
}

fn elem(n: u32) -> NodeId {
    NodeId::StructElem(ObjRef::new(n, 0))
}

fn mc(mcid: i32) -> NodeId {
    NodeId::MarkedContent { page: 0, mcid }
}

fn codes(doc: &Document) -> Vec<(&'static str, Severity, Option<u32>)> {
    doc.info()
        .diagnostics
        .iter()
        .map(|d| (d.code, d.severity, d.object.map(|o| o.num)))
        .collect()
}

fn root(doc: &Document) -> TreeNode {
    doc.tree(TreeKind::Tags).remove(0)
}

fn outline(node: &TreeNode, depth: usize, out: &mut Vec<String>) {
    out.push(format!("{}{}", "  ".repeat(depth), node.id));
    for c in &node.children {
        outline(c, depth + 1, out);
    }
}

fn outline_of(doc: &Document) -> Vec<String> {
    let mut out = Vec::new();
    outline(&root(doc), 0, &mut out);
    out
}

fn badges(node: &TreeNode, id: &NodeId) -> Vec<String> {
    node.find(id)
        .unwrap()
        .badges
        .iter()
        .map(|b| b.code.clone())
        .collect()
}

#[test]
fn direct_struct_elements_fold_into_their_nearest_indirect_ancestor() {
    let doc = build(
        MARKED,
        "/StructParents 0",
        &[0, 1],
        &[
            s(6, ROOT_WITH_PT),
            s(
                7,
                "<< /Type /StructElem /S /Document /P 6 0 R /K [<< /S /Span /Pg 3 0 R /K 0 >> 8 0 R] >>",
            ),
            s(8, "<< /Type /StructElem /S /P /P 7 0 R /Pg 3 0 R /K 1 >>"),
            s(20, "<< /Nums [0 [7 0 R 8 0 R]] >>"),
        ],
    );

    assert_eq!(
        outline_of(&doc),
        [
            "group:100",
            "  struct:7:0",
            "    mc:0:0",
            "    struct:8:0",
            "      mc:0:1",
        ]
    );
    assert_eq!(codes(&doc), [("struct-elem-direct", Severity::Warning, Some(7))]);
    assert!(
        doc.info().diagnostics[0]
            .message
            .starts_with("structure element /Span is a direct object")
    );
    assert_eq!(badges(&root(&doc), &elem(7)), ["struct-elem-direct"]);
    // The folded element's MCID is owned by the indirect ancestor, in hits and in the ParentTree.
    let (_, folded) = doc.bbox(&mc(0)).unwrap();
    let at = Point::new(folded.x0.midpoint(folded.x1), folded.y0.midpoint(folded.y1));
    assert_eq!(
        doc.hit(0, at, LayerMode::Tags),
        vec![mc(0), elem(7), NodeId::Page(0)]
    );
}

#[test]
fn a_cycle_through_k_is_reported_once_and_cut() {
    let doc = build(
        MARKED,
        "",
        &[],
        &[
            s(6, "<< /Type /StructTreeRoot /K 7 0 R >>"),
            s(7, "<< /Type /StructElem /S /Document /P 6 0 R /K [8 0 R] >>"),
            s(8, "<< /Type /StructElem /S /Sect /P 7 0 R /K [7 0 R] >>"),
        ],
    );

    assert_eq!(codes(&doc), [("struct-elem-cycle", Severity::Error, Some(7))]);
    assert_eq!(outline_of(&doc), ["group:100", "  struct:7:0", "    struct:8:0"]);
    assert_eq!(badges(&root(&doc), &elem(7)), ["struct-elem-cycle"]);
    assert_eq!(doc.breadcrumb(&elem(8)), vec![NodeId::Document, elem(7), elem(8)]);
}

#[test]
fn an_element_with_two_parents_is_shown_once() {
    let doc = build(
        MARKED,
        "",
        &[],
        &[
            s(6, "<< /Type /StructTreeRoot /K 7 0 R >>"),
            s(
                7,
                "<< /Type /StructElem /S /Document /P 6 0 R /K [8 0 R 9 0 R] >>",
            ),
            s(8, "<< /Type /StructElem /S /Sect /P 7 0 R /K [9 0 R] >>"),
            s(9, "<< /Type /StructElem /S /Sect /P 8 0 R >>"),
        ],
    );

    assert_eq!(
        codes(&doc),
        [("struct-elem-multiple-parents", Severity::Error, Some(9))]
    );
    assert_eq!(
        outline_of(&doc),
        ["group:100", "  struct:7:0", "    struct:8:0", "      struct:9:0"]
    );
}

#[test]
fn an_mcid_referenced_by_two_elements_is_flagged_on_the_second() {
    let doc = build(
        MARKED,
        "/StructParents 0",
        &[0],
        &[
            s(6, ROOT_WITH_PT),
            s(
                7,
                "<< /Type /StructElem /S /Document /P 6 0 R /K [8 0 R 9 0 R] >>",
            ),
            s(8, "<< /Type /StructElem /S /P /P 7 0 R /Pg 3 0 R /K 0 >>"),
            s(9, "<< /Type /StructElem /S /P /P 7 0 R /Pg 3 0 R /K 0 >>"),
            s(20, "<< /Nums [0 [8 0 R]] >>"),
        ],
    );

    assert_eq!(codes(&doc), [("mcid-referenced-twice", Severity::Error, Some(9))]);
    assert_eq!(
        doc.info().diagnostics[0].message,
        "MCID 0 on page 1 is referenced by both 8 0 R and 9 0 R"
    );
    // The first element in document order owns the content.
    let (_, b) = doc.bbox(&mc(0)).unwrap();
    let at = Point::new(b.x0 + 1.0, b.y0 + 1.0);
    assert_eq!(doc.hit(0, at, LayerMode::Tags)[..2], [mc(0), elem(8)]);
}

#[test]
fn marked_content_without_any_page_is_reported_and_left_out_of_the_tree() {
    let doc = build(
        MARKED,
        "/StructParents 0",
        &[0],
        &[
            s(6, ROOT_WITH_PT),
            s(7, "<< /Type /StructElem /S /Document /P 6 0 R /K [8 0 R] >>"),
            s(8, "<< /Type /StructElem /S /P /P 7 0 R /K 0 >>"),
            s(20, "<< /Nums [0 [8 0 R]] >>"),
        ],
    );

    assert_eq!(
        codes(&doc),
        [
            ("pg-missing", Severity::Warning, Some(8)),
            ("mcid-not-in-structure", Severity::Error, Some(3)),
        ]
    );
    assert_eq!(outline_of(&doc), ["group:100", "  struct:7:0", "    struct:8:0"]);
    let p = doc.node(&elem(8)).unwrap();
    assert_eq!(p.get("Content", "MCID"), Some("0 · page unknown"));
    assert_eq!(p.get("Structure", "Children"), Some("1 MCID"));
}

#[test]
fn a_pg_that_is_not_a_page_is_reported() {
    let doc = build(
        MARKED,
        "/StructParents 0",
        &[0],
        &[
            s(6, ROOT_WITH_PT),
            s(7, "<< /Type /StructElem /S /Document /P 6 0 R /K [8 0 R] >>"),
            s(8, "<< /Type /StructElem /S /P /P 7 0 R /Pg 5 0 R /K 0 >>"),
            s(20, "<< /Nums [0 [8 0 R]] >>"),
        ],
    );

    let got = codes(&doc);

    assert_eq!(got[0], ("pg-not-a-page", Severity::Warning, Some(8)));
    assert!(got.contains(&("pg-missing", Severity::Warning, Some(8))));
    assert_eq!(got.len(), 3, "{got:?}");
}

#[test]
fn an_annotation_without_struct_parent_is_reported() {
    let doc = build(
        MARKED,
        "/StructParents 0 /Annots [21 0 R]",
        &[],
        &[
            s(6, ROOT_WITH_PT),
            s(
                7,
                "<< /Type /StructElem /S /Link /P 6 0 R /K [<< /Type /OBJR /Obj 21 0 R /Pg 3 0 R >>] >>",
            ),
            s(20, "<< /Nums [] >>"),
            s(
                21,
                "<< /Type /Annot /Subtype /Link /Rect [10 10 50 30] /P 3 0 R >>",
            ),
        ],
    );

    assert_eq!(
        codes(&doc),
        [("struct-parent-missing", Severity::Warning, Some(21))]
    );
    let r = root(&doc);
    assert_eq!(badges(&r, &elem(7)), ["struct-parent-missing"]);
    assert_eq!(
        r.find(&NodeId::Object(ObjRef::new(21, 0)))
            .unwrap()
            .detail
            .as_deref(),
        Some("Link")
    );
    // No content: the box falls back to the annotation's /Rect on the OBJR's page.
    let (page, rect) = doc.bbox(&elem(7)).unwrap();
    assert_eq!(
        (page, (rect.x0, rect.y0, rect.x1, rect.y1)),
        (0, (10.0, 10.0, 50.0, 30.0))
    );
}

#[test]
fn a_page_without_struct_parents_cannot_be_mapped() {
    let doc = build(
        MARKED,
        "",
        &[0],
        &[
            s(6, ROOT_WITH_PT),
            s(7, "<< /Type /StructElem /S /P /P 6 0 R /Pg 3 0 R /K 0 >>"),
            s(20, "<< /Nums [] >>"),
        ],
    );

    assert_eq!(
        codes(&doc),
        [("struct-parent-missing", Severity::Warning, Some(3))]
    );
}

#[test]
fn a_tree_without_parent_tree_is_reported_on_the_root() {
    let doc = build(
        MARKED,
        "/StructParents 0",
        &[0],
        &[
            s(6, "<< /Type /StructTreeRoot /K 7 0 R >>"),
            s(7, "<< /Type /StructElem /S /P /P 6 0 R /Pg 3 0 R /K 0 >>"),
        ],
    );

    assert_eq!(codes(&doc), [("parent-tree-missing", Severity::Error, Some(6))]);
    assert_eq!(root(&doc).badges[0].code, "parent-tree-missing");
}

#[test]
fn struct_parents_pointing_nowhere_and_stale_parent_tree_slots_are_mismatches() {
    let doc = build(
        MARKED,
        "/StructParents 4",
        &[0],
        &[
            s(6, ROOT_WITH_PT),
            s(7, "<< /Type /StructElem /S /P /P 6 0 R /Pg 3 0 R /K 0 >>"),
            s(20, "<< /Nums [0 [7 0 R]] >>"),
        ],
    );
    let stale = build(
        MARKED,
        "/StructParents 0",
        &[0],
        &[
            s(6, ROOT_WITH_PT),
            s(7, "<< /Type /StructElem /S /P /P 6 0 R /Pg 3 0 R /K 0 >>"),
            s(8, "<< /Type /StructElem /S /P /P 6 0 R >>"),
            s(20, "<< /Nums [0 [7 0 R 8 0 R]] >>"),
        ],
    );

    assert_eq!(
        codes(&doc),
        [("struct-parent-missing", Severity::Warning, Some(3))]
    );
    assert_eq!(
        codes(&stale),
        [("parent-tree-mismatch", Severity::Error, Some(8))]
    );
    assert_eq!(
        stale.info().diagnostics[0].message,
        "ParentTree key 0 names 8 0 R for MCID 1 on page 1, but no element references that MCID in /K"
    );
}

#[test]
fn deep_nesting_stops_at_the_limit_without_overflowing_the_stack() {
    let mut objects = vec![s(6, "<< /Type /StructTreeRoot /K 7 0 R >>")];
    for n in 7..320_u32 {
        let parent = if n == 7 { 6 } else { n - 1 };
        objects.push((
            n,
            format!(
                "<< /Type /StructElem /S /Div /P {parent} 0 R /K [{} 0 R] >>",
                n + 1
            ),
        ));
    }
    objects.push((320, "<< /Type /StructElem /S /Div /P 319 0 R >>".to_owned()));

    let doc = build(MARKED, "", &[], &objects);

    assert_eq!(
        codes(&doc),
        [("struct-tree-too-deep", Severity::Error, Some(206))]
    );
    let r = root(&doc);
    assert_eq!(r.detail.as_deref(), Some("200 elements, /Marked true"));
    assert_eq!(r.len(), 201);
    assert_eq!(doc.breadcrumb(&elem(206)).len(), 201);
}

#[test]
fn mark_info_flags_show_on_the_root() {
    let doc = build(
        "/MarkInfo << /Marked true /Suspects true /UserProperties true >>",
        "",
        &[],
        &[s(6, "<< /Type /StructTreeRoot >>")],
    );

    assert_eq!(
        root(&doc).detail.as_deref(),
        Some("0 elements, /Marked true, /Suspects true, /UserProperties true")
    );
    assert_eq!(codes(&doc), []);
}

#[test]
fn mark_info_and_struct_tree_root_must_agree() {
    let unmarked = build(
        "/MarkInfo << /Marked false >>",
        "",
        &[],
        &[s(6, "<< /Type /StructTreeRoot >>")],
    );
    let missing = build("", "", &[], &[s(6, "<< /Type /StructTreeRoot >>")]);
    let no_root = {
        let mut w = PdfWriter::new(PdfVersion::V1_7);
        w.object(
            1,
            "<< /Type /Catalog /Pages 2 0 R /MarkInfo << /Marked true >> >>",
        );
        w.object(2, "<< /Type /Pages /Kids [] /Count 0 >>");
        w.finish_table(&Trailer::new(1)).unwrap();
        Document::from_bytes("noroot.pdf", w.into_output().unwrap().bytes).unwrap()
    };

    assert_eq!(
        codes(&unmarked),
        [("marked-false-but-tagged", Severity::Warning, Some(1))]
    );
    assert!(
        unmarked.info().diagnostics[0]
            .message
            .ends_with("/Marked is false")
    );
    assert_eq!(
        codes(&missing),
        [("marked-false-but-tagged", Severity::Warning, Some(1))]
    );
    assert!(
        missing.info().diagnostics[0]
            .message
            .ends_with("/Marked is missing")
    );
    assert_eq!(
        codes(&no_root),
        [("marked-true-no-struct-tree", Severity::Warning, Some(1))]
    );
    assert_eq!(no_root.tree(TreeKind::Tags), Vec::<TreeNode>::new());
    assert_eq!(root(&unmarked).badges[0].code, "marked-false-but-tagged");
    assert_eq!(
        root(&unmarked).detail.as_deref(),
        Some("0 elements, /Marked false")
    );
}

#[test]
fn unmapped_custom_types_are_reported_once_per_type() {
    let doc = build(
        MARKED,
        "",
        &[],
        &[
            s(
                6,
                "<< /Type /StructTreeRoot /K 7 0 R /RoleMap << /Half /Weird >> >>",
            ),
            s(
                7,
                "<< /Type /StructElem /S /Document /P 6 0 R /K [8 0 R 9 0 R 10 0 R 11 0 R] >>",
            ),
            s(8, "<< /Type /StructElem /S /Weird /P 7 0 R >>"),
            s(9, "<< /Type /StructElem /S /Weird /P 7 0 R >>"),
            s(10, "<< /Type /StructElem /S /Half /P 7 0 R >>"),
            s(11, "<< /Type /StructElem /P 7 0 R >>"),
        ],
    );

    assert_eq!(
        codes(&doc),
        [
            ("struct-elem-no-type", Severity::Error, Some(11)),
            ("non-standard-type", Severity::Warning, Some(8)),
            ("non-standard-type", Severity::Warning, Some(10)),
            ("non-standard-type", Severity::Warning, Some(11)),
        ]
    );
    let info = doc.info();
    let messages: Vec<_> = info.diagnostics.iter().map(|d| d.message.as_str()).collect();
    assert_eq!(
        messages[1],
        "structure type /Weird is not a standard type and the role map does not reach one (2 elements)"
    );
    assert_eq!(
        messages[2],
        "structure type /Half maps only as far as Half → Weird, which is not a standard type and the role map does not reach one (1 element)"
    );
    let r = root(&doc);
    assert_eq!(badges(&r, &elem(8)), ["non-standard-type"]);
    assert_eq!(badges(&r, &elem(9)), ["non-standard-type"]);
    assert_eq!(r.find(&elem(11)).unwrap().label, "(no type)");
    assert_eq!(
        doc.node(&elem(10)).unwrap().get("Element", "Standard type"),
        Some("none (not mapped to a standard type)")
    );
    // The diagnostic is linked from both elements that share it.
    let links = |n: u32| {
        doc.node(&elem(n))
            .unwrap()
            .groups
            .iter()
            .find(|g| g.name == "Diagnostics")
            .map(|g| g.rows.iter().filter_map(|r| r.link.clone()).collect::<Vec<_>>())
    };
    assert_eq!(links(8), Some(vec![NodeId::Diagnostic(1)]));
    assert_eq!(links(9), links(8));
}

#[test]
fn id_tree_problems_are_reported() {
    let doc = build(
        MARKED,
        "",
        &[],
        &[
            s(
                6,
                "<< /Type /StructTreeRoot /K 7 0 R /IDTree << /Names [(a) 8 0 R (b) 8 0 R (z) 99 0 R] >> >>",
            ),
            s(
                7,
                "<< /Type /StructElem /S /Document /P 6 0 R /K [8 0 R 9 0 R] >>",
            ),
            s(8, "<< /Type /StructElem /S /Sect /P 7 0 R /ID (a) >>"),
            s(9, "<< /Type /StructElem /S /Sect /P 7 0 R /ID (a) >>"),
        ],
    );

    assert_eq!(
        codes(&doc),
        [
            ("duplicate-element-id", Severity::Error, Some(9)),
            ("id-tree-mismatch", Severity::Warning, Some(8)),
            // 99 0 R does not exist, so there is no object to point at.
            ("id-tree-mismatch", Severity::Warning, None),
        ]
    );
    assert!(
        doc.info().diagnostics[1]
            .message
            .contains("(b) to 8 0 R, whose own /ID differs")
    );
}

#[test]
fn unusable_k_entries_class_names_and_parent_links_are_diagnostics() {
    let doc = build(
        MARKED,
        "",
        &[],
        &[
            s(
                6,
                "<< /Type /StructTreeRoot /K 7 0 R /ClassMap << /Known << /O /Layout >> >> >>",
            ),
            s(
                7,
                "<< /Type /StructElem /S /Document /P 6 0 R /C [/Known /Unknown] /K [(text) 12 0 R 8 0 R 99 0 R] >>",
            ),
            s(8, "<< /Type /StructElem /S /Sect /P 6 0 R >>"),
            s(12, "[1 2 3]"),
        ],
    );

    assert_eq!(
        codes(&doc),
        [
            ("class-not-found", Severity::Warning, Some(7)),
            ("struct-kid-invalid", Severity::Warning, Some(7)),
            ("struct-kid-invalid", Severity::Warning, Some(7)),
            ("struct-elem-parent-mismatch", Severity::Warning, Some(8)),
            ("struct-kid-invalid", Severity::Warning, Some(7)),
        ]
    );
    let p = doc.node(&elem(7)).unwrap();
    assert_eq!(p.get("Attributes", "Classes"), Some("Known, Unknown"));
    assert_eq!(
        p.get("Attributes", "Layout"),
        Some("(no entries) (from class Known)")
    );
}

#[test]
fn noisy_values_never_panic_the_inspector() {
    let doc = build(
        MARKED,
        "/StructParents 0",
        &[0],
        &[
            s(
                6,
                "<< /Type /StructTreeRoot /K 7 0 R /ParentTree 20 0 R /RoleMap 5 \
                 /Namespaces [7 0 R 7 0 R null 30 0 R] /ClassMap [1] /IDTree 6 0 R >>",
            ),
            s(
                7,
                "<< /Type /StructElem /S /A /NS 30 0 R /P 6 0 R /Pg 3 0 R /Lang <FEFF00650073> \
                 /A [<< /O /Table /Span -1 /K << /N (nested) >> >> 7 << /O /Layout >> 2] /K [0 -1 99999999999] \
                 /Alt <FEFFD800> /Ref [7 0 R 1 0 R] /AF 5 >>",
            ),
            s(20, "<< /Kids [20 0 R] /Nums [0 [7 0 R] bad (x) 7 0 R] >>"),
            s(
                30,
                "<< /Type /Namespace /NS (urn:x) /RoleMapNS << /A [/B 30 0 R] /B [/A 30 0 R] /C /D >> >>",
            ),
        ],
    );

    let p = doc.node(&elem(7)).unwrap();

    assert_eq!(p.get("Element", "Lang"), Some("es"));
    assert_eq!(p.get("Element", "Alt"), Some("\u{FFFD}"));
    assert_eq!(p.get("Attributes", "Table: Span"), Some("-1"));
    assert_eq!(p.get("Attributes", "Table: revision"), Some("7"));
    assert_eq!(p.get("Attributes", "Layout: revision"), Some("2"));
    assert_eq!(p.get("Element", "Namespace"), Some("urn:x"));
    assert_eq!(p.get("Element", "Role chain"), Some("A → B → A"));
    assert_eq!(
        p.get("Element", "Standard type"),
        Some("none (role map cycle: A → B)")
    );
    assert_eq!(doc.tree(TreeKind::Tags).len(), 1);
}
