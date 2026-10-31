//! The checks on alternate text, role maps, `Private` elements and tables: what raises them, with
//! which severity, where they show up and what they explain.

#![allow(
    clippy::unwrap_used,
    clippy::panic,
    reason = "test helpers fail loudly on unexpected input"
)]

use std::fmt::Write as _;

use pdfxray_core::{Document, NodeId, ObjRef, Severity, TreeKind, TreeNode, explain};
use pdfxray_fixtures::{PdfVersion, PdfWriter, Trailer};
use pretty_assertions::assert_eq;

/// The codes this milestone adds.
const CODES: [&str; 10] = [
    "alt-hides-content",
    "figure-alt-missing",
    "alt-and-actualtext",
    "actualtext-on-structure",
    "rolemap-multi-hop",
    "private-hides-content",
    "table-header-missing",
    "table-header-unresolved",
    "table-headers-id-missing",
    "table-irregular",
];

fn fixture(name: &str) -> Document {
    Document::open(pdfxray_fixtures::pdf_path(name)).unwrap()
}

fn elem(n: u32) -> NodeId {
    NodeId::StructElem(ObjRef::new(n, 0))
}

/// A diagnostic as `(code, severity, object)`.
type Found = (&'static str, Severity, Option<u32>);

/// The diagnostics of this milestone, in report order.
fn found(doc: &Document) -> Vec<Found> {
    doc.info()
        .diagnostics
        .iter()
        .filter(|d| CODES.contains(&d.code))
        .map(|d| (d.code, d.severity, d.object.map(|o| o.num)))
        .collect()
}

/// One structure element: `S`, the parent, extra entries and the kids (`/K` value).
fn node(obj: u32, parent: u32, role: &str, extra: &str, kids: &str) -> (u32, String) {
    (
        obj,
        format!("<< /Type /StructElem /S /{role} /P {parent} 0 R /Pg 3 0 R {extra} /K {kids} >>"),
    )
}

/// A document with one defect per check. MCID `n` is painted by `owners[n]`.
fn defects_doc() -> Document {
    let elems = [
        node(
            7,
            6,
            "Document",
            "",
            "[8 0 R 9 0 R 10 0 R 11 0 R 13 0 R 16 0 R 23 0 R 31 0 R 33 0 R 34 0 R 38 0 R]",
        ),
        // A figure without alternate text.
        node(8, 7, "Figure", "", "0"),
        // ActualText on a heading.
        node(9, 7, "H1", "/ActualText (Heading)", "1"),
        // Both alternate texts on one element.
        node(10, 7, "Span", "/Alt (alt) /ActualText (actual)", "2"),
        // Alt on a Part, and on a Sect, with paragraphs below.
        node(11, 7, "Part", "/Alt (Chapter)", "[12 0 R]"),
        node(12, 11, "P", "", "3"),
        node(13, 7, "Sect", "/Alt (Section)", "[14 0 R 15 0 R]"),
        node(14, 13, "P", "", "4"),
        node(15, 13, "P", "", "5"),
        // A 2 x 2 table without a header cell.
        node(16, 7, "Table", "", "[17 0 R 20 0 R]"),
        node(17, 16, "TR", "", "[18 0 R 19 0 R]"),
        node(18, 17, "TD", "", "6"),
        node(19, 17, "TD", "", "7"),
        node(20, 16, "TR", "", "[21 0 R 22 0 R]"),
        node(21, 20, "TD", "", "8"),
        node(22, 20, "TD", "", "9"),
        // An irregular table: three headers over a row of two cells.
        node(23, 7, "Table", "", "[24 0 R 28 0 R]"),
        node(24, 23, "TR", "", "[25 0 R 26 0 R 27 0 R]"),
        node(25, 24, "TH", "", "10"),
        node(26, 24, "TH", "", "11"),
        node(27, 24, "TH", "", "12"),
        node(28, 23, "TR", "", "[29 0 R 30 0 R]"),
        node(29, 28, "TD", "", "13"),
        node(30, 28, "TD", "", "14"),
        // Content in a Private element.
        node(31, 7, "Private", "", "[32 0 R]"),
        node(32, 31, "P", "", "15"),
        // A role that needs two hops.
        node(33, 7, "Hdg", "", "16"),
        // A data cell no header reaches: its row's header is a column header.
        node(34, 7, "Table", "", "[35 0 R]"),
        node(35, 34, "TR", "", "[36 0 R 37 0 R]"),
        node(36, 35, "TH", "/A << /O /Table /Scope /Column >>", "17"),
        node(37, 35, "TD", "", "18"),
        // A /Headers list naming an ID nobody has.
        node(38, 7, "Table", "", "[39 0 R]"),
        node(39, 38, "TR", "", "[40 0 R]"),
        node(40, 39, "TD", "/A << /O /Table /Headers [(nope)] >>", "19"),
    ];
    let owners = [
        8, 9, 10, 12, 14, 15, 18, 19, 21, 22, 25, 26, 27, 29, 30, 32, 33, 36, 37, 40,
    ];
    let mut content = String::new();
    for n in 0..owners.len() {
        let _ = writeln!(
            content,
            "/P << /MCID {n} >> BDC BT /F1 10 Tf 10 {} Td (text {n}) Tj ET EMC",
            390 - 10 * n
        );
    }
    let mut w = PdfWriter::new(PdfVersion::V1_7);
    w.object(
        1,
        "<< /Type /Catalog /Pages 2 0 R /StructTreeRoot 6 0 R /MarkInfo << /Marked true >> /Lang (en-US) >>",
    );
    w.object(2, "<< /Type /Pages /Kids [3 0 R] /Count 1 >>");
    w.object(
        3,
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 300 400] /StructParents 0 \
         /Resources << /Font << /F1 5 0 R >> >> /Contents 4 0 R >>",
    );
    w.stream(4, "", content.as_bytes());
    w.object(5, "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>");
    w.object(
        6,
        "<< /Type /StructTreeRoot /K 7 0 R /ParentTree 90 0 R /ParentTreeNextKey 1 \
         /RoleMap << /Hdg /Heading1 /Heading1 /H1 >> >>",
    );
    for (obj, body) in &elems {
        w.object(*obj, body);
    }
    let nums: Vec<String> = owners.iter().map(|o| format!("{o} 0 R")).collect();
    w.object(90, format!("<< /Nums [0 [{}]] >>", nums.join(" ")));
    w.finish_table(&Trailer::new(1)).unwrap();
    Document::from_bytes("defects.pdf", w.into_output().unwrap().bytes).unwrap()
}

#[test]
fn every_check_fires_once_with_its_severity_on_the_element_it_is_about() {
    let doc = defects_doc();

    assert_eq!(
        found(&doc),
        [
            ("figure-alt-missing", Severity::Error, Some(8)),
            ("actualtext-on-structure", Severity::Warning, Some(9)),
            ("alt-and-actualtext", Severity::Warning, Some(10)),
            // On a Part the loss is an error ...
            ("alt-hides-content", Severity::Error, Some(11)),
            // ... on a Sect a warning.
            ("alt-hides-content", Severity::Warning, Some(13)),
            ("private-hides-content", Severity::Warning, Some(31)),
            ("rolemap-multi-hop", Severity::Warning, Some(33)),
            ("table-header-missing", Severity::Warning, Some(16)),
            ("table-irregular", Severity::Warning, Some(23)),
            ("table-header-unresolved", Severity::Info, Some(37)),
            ("table-headers-id-missing", Severity::Warning, Some(40)),
        ]
    );
    // Smoke: every code of the milestone is exercised.
    let codes: std::collections::BTreeSet<_> = found(&doc).iter().map(|f| f.0).collect();
    assert_eq!(codes.len(), CODES.len());
}

#[test]
fn the_messages_name_what_is_wrong_in_figures() {
    let doc = defects_doc();

    let messages: Vec<String> = doc
        .info()
        .diagnostics
        .iter()
        .filter(|d| CODES.contains(&d.code))
        .map(|d| d.message.clone())
        .collect();

    assert_eq!(
        messages,
        [
            "Figure 8 0 R has no /Alt or /ActualText, so a screen reader says only \"graphic\"",
            "H1 9 0 R has /ActualText, which replaces the whole H1 with plain text and drops its role",
            "Span 10 0 R has both /Alt and /ActualText; Acrobat-based screen readers say only /Alt",
            "Part 11 0 R has /Alt, so Acrobat-based screen readers say that and skip the 1 element(s) with content below it",
            "Sect 13 0 R has /Alt, so Acrobat-based screen readers say that and skip the 2 element(s) with content below it",
            "Private 31 0 R holds content, which a reader that follows the standard never interprets",
            "role Hdg needs 2 role map hops (Hdg → Heading1 → H1); pdf.js and PDFium follow only one, so those viewers see a generic element (1 element(s))",
            "16 0 R is a table of 2 rows and 2 columns with no header cell (TH)",
            "23 0 R is 3 columns wide, but row 2 covers 2",
            "37 0 R (row 1, column 2) has no header cell",
            "40 0 R lists /Headers ID (nope), which no cell of its table has",
        ]
    );
}

fn find<'a>(nodes: &'a [TreeNode], id: &NodeId) -> Option<&'a TreeNode> {
    nodes.iter().find_map(|n| {
        if n.id == *id {
            Some(n)
        } else {
            find(&n.children, id)
        }
    })
}

#[test]
fn the_checks_show_as_badges_in_the_tags_tree_and_as_rows_of_the_diagnostics_tree() {
    let doc = defects_doc();
    let tags = doc.tree(TreeKind::Tags);

    let badges = |n: u32| -> Vec<(String, Severity)> {
        find(&tags, &elem(n))
            .unwrap()
            .badges
            .iter()
            .map(|b| (b.code.clone(), b.severity))
            .collect()
    };

    assert_eq!(badges(8), [("figure-alt-missing".to_owned(), Severity::Error)]);
    assert_eq!(badges(11), [("alt-hides-content".to_owned(), Severity::Error)]);
    assert_eq!(
        badges(37),
        [("table-header-unresolved".to_owned(), Severity::Info)]
    );
    assert_eq!(badges(12), []);
    let rows: Vec<String> = doc
        .tree(TreeKind::Diagnostics)
        .iter()
        .flat_map(|group| &group.children)
        .map(|n| n.label.clone())
        .filter(|label| CODES.contains(&label.as_str()))
        .collect();
    assert_eq!(rows.len(), 11);
    // Smoke: the Inspector of the element lists its diagnostic.
    let props = doc.node(&elem(8)).unwrap();
    assert_eq!(props.get("Diagnostics", "figure-alt-missing").is_some(), true);
}

#[test]
fn every_check_explains_itself_with_the_standard_it_comes_from() {
    let doc = defects_doc();
    let explained: Vec<(&str, bool)> = doc
        .info()
        .diagnostics
        .iter()
        .filter(|d| CODES.contains(&d.code))
        .map(|d| {
            let text = d.explain().unwrap();
            (
                d.code,
                text.contains("Matterhorn") || text.contains("WCAG") || text.contains("ISO 32000"),
            )
        })
        .collect();

    assert_eq!(explained.len(), 11);
    assert_eq!(explained.iter().filter(|(_, cites)| !cites).count(), 0);
    for code in CODES {
        let text = explain::diagnostic(code).unwrap();
        assert!(text.ends_with('.'), "{code}");
    }
    // Smoke: the figure rule cites the Matterhorn checkpoint.
    assert!(
        explain::diagnostic("figure-alt-missing")
            .unwrap()
            .contains("Matterhorn 13-004")
    );
}

// ---- fixtures ---------------------------------------------------------------------------------

#[test]
fn the_speech_fixtures_raise_exactly_the_checks_they_were_made_for() {
    let by_fixture: Vec<(&str, Vec<Found>)> = [
        "speech-alt-hides",
        "speech-artifacts",
        "speech-footnote",
        "speech-lang",
        "speech-link-objr",
        "rolemap-two-hops",
        "table-simple-pdf17",
        "table-scope-pdf2",
        "table-headers-ids",
        "reading-order",
    ]
    .into_iter()
    .map(|name| (name, found(&fixture(name))))
    .collect();

    assert_eq!(
        by_fixture,
        [
            (
                "speech-alt-hides",
                vec![("alt-hides-content", Severity::Warning, Some(9))]
            ),
            ("speech-artifacts", vec![]),
            ("speech-footnote", vec![]),
            ("speech-lang", vec![]),
            ("speech-link-objr", vec![]),
            (
                "rolemap-two-hops",
                vec![("rolemap-multi-hop", Severity::Warning, Some(8))]
            ),
            ("table-simple-pdf17", vec![]),
            ("table-scope-pdf2", vec![]),
            (
                "table-headers-ids",
                vec![("table-headers-id-missing", Severity::Warning, Some(22))]
            ),
            ("reading-order", vec![]),
        ]
    );
}
