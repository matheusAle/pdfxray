//! Tag boxes, role classes, reading order (structure and paint), its backtrack diagnostic and
//! text previews, through the public query API.

#![allow(
    clippy::unwrap_used,
    clippy::panic,
    reason = "test helpers fail loudly on unexpected input"
)]

use std::fmt::Write as _;

use pdfxray_core::{
    Document, NodeId, ObjRef, OrderComparison, ReadingOrderSource, Rect, RoleClass, Severity, StepKind,
    TreeKind,
};
use pdfxray_fixtures::{PdfVersion, PdfWriter, Trailer};
use pretty_assertions::assert_eq;

fn fixture(name: &str) -> Document {
    Document::open(pdfxray_fixtures::pdf_path(name)).unwrap()
}

fn elem(n: u32) -> NodeId {
    NodeId::StructElem(ObjRef::new(n, 0))
}

fn ids(order: &pdfxray_core::ReadingOrder) -> Vec<NodeId> {
    order.steps.iter().map(|s| s.id.clone()).collect()
}

fn kinds(order: &pdfxray_core::ReadingOrder) -> Vec<StepKind> {
    order.steps.iter().map(|s| s.kind).collect()
}

fn backtrack_diagnostics(doc: &Document) -> usize {
    doc.info()
        .diagnostics
        .iter()
        .filter(|d| d.code == "reading-order-backtrack")
        .count()
}

// ---- reading-order fixture: H1 (Heading1), P, Caption, Figure, P -------------------------------

#[test]
fn structure_order_follows_the_tree_and_flags_the_figure_as_the_only_backtrack() {
    let doc = fixture("reading-order");

    let order = doc.reading_order(0, ReadingOrderSource::Structure);

    assert_eq!(ids(&order), [elem(8), elem(9), elem(10), elem(11), elem(12)]);
    assert_eq!(
        order.steps.iter().map(|s| s.index).collect::<Vec<_>>(),
        [1, 2, 3, 4, 5]
    );
    assert_eq!(
        order.steps.iter().map(|s| s.role.as_str()).collect::<Vec<_>>(),
        ["Heading1", "P", "Caption", "Figure", "P"]
    );
    assert_eq!(
        order.steps.iter().map(|s| s.class).collect::<Vec<_>>(),
        [
            RoleClass::Heading,
            RoleClass::Paragraph,
            RoleClass::Caption,
            RoleClass::Figure,
            RoleClass::Paragraph
        ]
    );
    assert_eq!(
        kinds(&order),
        [
            StepKind::First,
            StepKind::Forward,
            StepKind::Forward,
            StepKind::Backtrack,
            StepKind::Forward
        ]
    );
    assert_eq!(
        order.steps.iter().map(|s| s.backtrack).collect::<Vec<_>>(),
        [false, false, false, true, false]
    );
    assert_eq!(
        order.steps.iter().map(|s| s.preview.as_str()).collect::<Vec<_>>(),
        [
            "Quarterly report",
            "Revenue grew in every region.",
            "Figure 1: Revenue by region",
            "Grey square",
            "Closing remarks follow."
        ]
    );
    assert_eq!(order.steps[3].rect, Rect::new(40.0, 200.0, 140.0, 300.0));
    // Smoke: exactly one backtrack.
    assert_eq!(order.backtracks, 1);
}

#[test]
fn paint_order_follows_the_content_stream_and_has_no_backtrack() {
    let doc = fixture("reading-order");

    let order = doc.reading_order(0, ReadingOrderSource::Paint);

    assert_eq!(ids(&order), [elem(8), elem(9), elem(11), elem(10), elem(12)]);
    assert_eq!(
        order.steps.iter().map(|s| s.class).collect::<Vec<_>>(),
        [
            RoleClass::Heading,
            RoleClass::Paragraph,
            RoleClass::Figure,
            RoleClass::Caption,
            RoleClass::Paragraph
        ]
    );
    assert_eq!(
        kinds(&order),
        [
            StepKind::First,
            StepKind::Forward,
            StepKind::Forward,
            StepKind::Forward,
            StepKind::Forward
        ]
    );
    assert_eq!(
        order.steps.iter().map(|s| s.backtrack).collect::<Vec<_>>(),
        [false; 5]
    );
    // Smoke.
    assert_eq!(order.backtracks, 0);
}

// ---- reading-order-columns: H1, two paragraphs per column, a closing paragraph -----------------

#[test]
fn two_columns_read_column_by_column_make_one_column_break_and_no_backtrack() {
    let doc = fixture("reading-order-columns");

    let order = doc.reading_order(0, ReadingOrderSource::Structure);

    assert_eq!(
        ids(&order),
        [elem(8), elem(9), elem(10), elem(11), elem(12), elem(13)]
    );
    assert_eq!(
        order.steps.iter().map(|s| s.preview.as_str()).collect::<Vec<_>>(),
        [
            "Two columns",
            "Column one starts here.",
            "and goes on below.",
            "Column two follows.",
            "and ends here.",
            "End of the page."
        ]
    );
    assert_eq!(
        kinds(&order),
        [
            StepKind::First,
            StepKind::Forward,
            StepKind::Forward,
            StepKind::ColumnBreak,
            StepKind::Forward,
            StepKind::Forward
        ]
    );
    // The second column starts higher than the end of the first and to the right of it.
    assert!(order.steps[3].rect.y1 > order.steps[2].rect.y1);
    assert!(order.steps[3].rect.x0 > order.steps[2].rect.x1);
    assert_eq!(backtrack_diagnostics(&doc), 0);
    // Smoke.
    assert_eq!(order.backtracks, 0);
}

#[test]
fn the_paint_order_of_the_columns_is_the_same_as_the_structure_order() {
    let doc = fixture("reading-order-columns");

    let comparison = doc.reading_order_comparison(0);

    assert_eq!(
        comparison,
        OrderComparison {
            same: 6,
            total: 6,
            first_difference: None
        }
    );
    // Smoke.
    assert_eq!(
        kinds(&doc.reading_order(0, ReadingOrderSource::Paint)),
        kinds(&doc.reading_order(0, ReadingOrderSource::Structure))
    );
}

// ---- reading-order-footnote: P, Note (Lbl, P) at the page bottom, P, P --------------------------

#[test]
fn a_footnote_tagged_after_its_paragraph_is_a_detour_down_and_back_and_no_backtrack() {
    let doc = fixture("reading-order-footnote");

    let order = doc.reading_order(0, ReadingOrderSource::Structure);

    assert_eq!(ids(&order), [elem(8), elem(10), elem(11), elem(12), elem(13)]);
    assert_eq!(
        order.steps.iter().map(|s| s.preview.as_str()).collect::<Vec<_>>(),
        [
            "Revenue grew in every region.",
            "1",
            "Source: annual report.",
            "Costs stayed flat this year.",
            "Closing remarks follow."
        ]
    );
    // The note's label enters the detour, the next paragraph returns from it; the note's own text
    // follows its label on the same line.
    assert_eq!(
        kinds(&order),
        [
            StepKind::First,
            StepKind::Detour,
            StepKind::Forward,
            StepKind::Detour,
            StepKind::Forward
        ]
    );
    assert!(order.steps[1].rect.y1 < order.steps[0].rect.y0);
    assert!(order.steps[3].rect.y1 > order.steps[2].rect.y1);
    assert_eq!(backtrack_diagnostics(&doc), 0);
    assert_eq!(doc.info().diagnostics.len(), 0);
    // Smoke.
    assert_eq!(order.backtracks, 0);
}

#[test]
fn painted_last_the_footnote_is_just_the_next_line_down() {
    let doc = fixture("reading-order-footnote");

    let order = doc.reading_order(0, ReadingOrderSource::Paint);

    assert_eq!(ids(&order), [elem(8), elem(12), elem(13), elem(10), elem(11)]);
    assert_eq!(
        kinds(&order),
        [
            StepKind::First,
            StepKind::Forward,
            StepKind::Forward,
            StepKind::Forward,
            StepKind::Forward
        ]
    );
    // Smoke.
    assert_eq!(order.backtracks, 0);
}

#[test]
fn the_comparison_names_the_first_position_where_the_orders_differ() {
    let doc = fixture("reading-order");

    let comparison = doc.reading_order_comparison(0);

    assert_eq!(
        comparison,
        OrderComparison {
            same: 3,
            total: 5,
            first_difference: Some((3, elem(10))),
        }
    );
    // Smoke.
    assert_eq!(comparison.total - comparison.same, 2);
}

#[test]
fn a_custom_role_takes_the_class_of_its_role_map_target() {
    let doc = fixture("reading-order");

    let classes: Vec<Option<RoleClass>> = [7, 8, 9, 10, 11, 12]
        .into_iter()
        .map(|n| doc.role_class(&elem(n)))
        .collect();

    assert_eq!(
        classes,
        [
            Some(RoleClass::Group),
            Some(RoleClass::Heading),
            Some(RoleClass::Paragraph),
            Some(RoleClass::Caption),
            Some(RoleClass::Figure),
            Some(RoleClass::Paragraph)
        ]
    );
    // Smoke: only structure elements of the tree have a class.
    assert_eq!(doc.role_class(&elem(99)), None);
    assert_eq!(doc.role_class(&NodeId::Page(0)), None);
}

#[test]
fn tag_boxes_list_parents_before_children_with_role_standard_class_and_depth() {
    let doc = fixture("reading-order");

    let boxes = doc.tag_boxes(0);

    assert_eq!(
        boxes
            .iter()
            .map(|b| (
                b.id.clone(),
                b.role.as_str(),
                b.standard.as_str(),
                b.class,
                b.depth
            ))
            .collect::<Vec<_>>(),
        [
            (elem(7), "Document", "Document", RoleClass::Group, 0),
            (elem(8), "Heading1", "H1", RoleClass::Heading, 1),
            (elem(9), "P", "P", RoleClass::Paragraph, 1),
            (elem(10), "Caption", "Caption", RoleClass::Caption, 1),
            (elem(11), "Figure", "Figure", RoleClass::Figure, 1),
            (elem(12), "P", "P", RoleClass::Paragraph, 1),
        ]
    );
    assert_eq!(boxes[4].rect, Rect::new(40.0, 200.0, 140.0, 300.0));
    // The Document box is the union of everything under it.
    let union = boxes[1..]
        .iter()
        .map(|b| b.rect)
        .reduce(|a, b| Rect::new(a.x0.min(b.x0), a.y0.min(b.y0), a.x1.max(b.x1), a.y1.max(b.y1)))
        .unwrap();
    assert_eq!(boxes[0].rect, union);
    // Smoke: a page that does not exist has no boxes.
    assert_eq!(doc.tag_boxes(7), []);
}

#[test]
fn only_backtracks_count_not_columns_or_detours() {
    let columns = fixture("reading-order-columns").reading_order(0, ReadingOrderSource::Structure);
    let footnote = fixture("reading-order-footnote").reading_order(0, ReadingOrderSource::Structure);
    let captioned = fixture("reading-order").reading_order(0, ReadingOrderSource::Structure);

    let counted =
        |o: &pdfxray_core::ReadingOrder, kind: StepKind| o.steps.iter().filter(|s| s.kind == kind).count();

    assert_eq!(counted(&columns, StepKind::ColumnBreak), 1);
    assert_eq!(counted(&footnote, StepKind::Detour), 2);
    assert_eq!(
        (columns.backtracks, footnote.backtracks, captioned.backtracks),
        (0, 0, 1)
    );
    // Smoke.
    assert_eq!(counted(&captioned, StepKind::Backtrack), captioned.backtracks);
}

#[test]
fn the_backtrack_is_a_warning_on_the_figure_in_info_and_in_the_diagnostics_tree() {
    let doc = fixture("reading-order");

    let diagnostics = doc.info().diagnostics;

    assert_eq!(diagnostics.len(), 1);
    let d = &diagnostics[0];
    assert_eq!(d.code, "reading-order-backtrack");
    assert_eq!(d.severity, Severity::Warning);
    assert_eq!(d.object, Some(ObjRef::new(11, 0)));
    assert_eq!(
        d.message,
        "Caption (step 3) is read before Figure (step 4), but Figure sits above it on page 1, \
         so a screen reader jumps back up the page"
    );
    assert!(d.explain().unwrap().contains("jump back up the page"));
    let tree = doc.tree(TreeKind::Diagnostics);
    let listed: Vec<&str> = tree
        .iter()
        .flat_map(|group| &group.children)
        .map(|n| n.label.as_str())
        .collect();
    assert_eq!(listed, ["reading-order-backtrack"]);
    let figure = doc.node(&elem(11)).unwrap();
    let rows: Vec<(&str, Option<&NodeId>)> = figure
        .groups
        .iter()
        .filter(|g| g.name == "Diagnostics")
        .flat_map(|g| &g.rows)
        .map(|r| (r.key.as_str(), r.link.as_ref()))
        .collect();
    assert_eq!(rows, [("reading-order-backtrack", Some(&NodeId::Diagnostic(0)))]);
    // Smoke: the other elements are clean.
    assert!(
        doc.node(&elem(10))
            .unwrap()
            .groups
            .iter()
            .all(|g| g.name != "Diagnostics")
    );
}

#[test]
fn text_preview_collapses_whitespace_and_cuts_with_an_ellipsis() {
    let doc = fixture("reading-order");

    assert_eq!(
        doc.text_preview(&elem(8), 60).as_deref(),
        Some("Quarterly report")
    );
    assert_eq!(
        doc.text_preview(&elem(8), 16).as_deref(),
        Some("Quarterly report")
    );
    assert_eq!(doc.text_preview(&elem(8), 10).as_deref(), Some("Quarterly…"));
    // The Document holds no text itself: its previews join the text of everything below it.
    assert_eq!(
        doc.text_preview(&elem(7), 40).as_deref(),
        Some("Quarterly report Revenue grew in every …")
    );
    // A figure without text shows its alternate text.
    assert_eq!(doc.text_preview(&elem(11), 60).as_deref(), Some("Grey square"));
    // Smoke: nothing to show.
    assert_eq!(doc.text_preview(&elem(8), 0), None);
    assert_eq!(doc.text_preview(&elem(99), 10), None);
    assert_eq!(doc.text_preview(&NodeId::Page(0), 10), None);
}

// ---- what a screen reader announces: Alt, then ActualText, then E, then the painted text -------

/// One page of `P` elements, each painting `text` at its own line and carrying the extra
/// attributes in `attrs` (written verbatim into the structure element).
fn doc_with_attributes(items: &[(&str, &str)]) -> Document {
    let mut content = String::new();
    let mut kids = String::new();
    for (mcid, (text, _)) in items.iter().enumerate() {
        let _ = writeln!(
            content,
            "/P << /MCID {mcid} >> BDC BT /F1 10 Tf 20 {} Td ({text}) Tj ET EMC",
            380 - 20 * mcid
        );
        let _ = write!(kids, "{} 0 R ", 10 + mcid);
    }
    let mut w = PdfWriter::new(PdfVersion::V1_7);
    w.object(
        1,
        "<< /Type /Catalog /Pages 2 0 R /StructTreeRoot 6 0 R /MarkInfo << /Marked true >> >>",
    );
    w.object(2, "<< /Type /Pages /Kids [3 0 R] /Count 1 >>");
    w.object(
        3,
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 300 400] /Resources << /Font << /F1 5 0 R >> >> \
         /Contents 4 0 R >>",
    );
    w.stream(4, "", content.as_bytes());
    w.object(5, "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>");
    w.object(6, "<< /Type /StructTreeRoot /K 7 0 R >>");
    w.object(
        7,
        format!("<< /Type /StructElem /S /Document /P 6 0 R /K [{kids}] >>"),
    );
    for (i, (_, attrs)) in items.iter().enumerate() {
        w.object(
            10 + u32::try_from(i).unwrap(),
            format!("<< /Type /StructElem /S /P /P 7 0 R /Pg 3 0 R {attrs} /K {i} >>"),
        );
    }
    w.finish_table(&Trailer::new(1)).unwrap();
    Document::from_bytes("precedence.pdf", w.into_output().unwrap().bytes).unwrap()
}

#[test]
fn the_preview_follows_alt_then_actual_text_then_expansion_then_the_painted_text() {
    let doc = doc_with_attributes(&[
        (
            "painted",
            "/Alt (the  alt) /ActualText (the actual) /E (the expansion)",
        ),
        ("painted", "/ActualText (the   actual) /E (the expansion)"),
        ("painted", "/E (the  expansion)"),
        ("painted", ""),
        ("painted", "/Alt (   ) /ActualText ()"),
    ]);

    let previews: Vec<Option<String>> = (10..15).map(|n| doc.text_preview(&elem(n), 60)).collect();

    assert_eq!(
        previews,
        [
            Some("the alt".to_owned()),
            Some("the actual".to_owned()),
            Some("the expansion".to_owned()),
            Some("painted".to_owned()),
            Some("painted".to_owned()),
        ]
    );
    // The steps of the reading order show the same text, and the cut still applies to it.
    let order = doc.reading_order(0, ReadingOrderSource::Structure);
    assert_eq!(
        order.steps.iter().map(|s| s.preview.as_str()).collect::<Vec<_>>(),
        ["the alt", "the actual", "the expansion", "painted", "painted"]
    );
    assert_eq!(doc.text_preview(&elem(10), 5).as_deref(), Some("the …"));
    // Smoke: the Document has no substitute of its own, so it joins what is painted below it.
    assert_eq!(
        doc.text_preview(&elem(7), 60).as_deref(),
        Some("painted painted painted painted painted")
    );
}

#[test]
fn pages_without_tags_or_out_of_range_have_an_empty_order() {
    let tagged = fixture("reading-order");
    let untagged = fixture("minimal-one-page");

    let beyond = tagged.reading_order(3, ReadingOrderSource::Structure);
    let plain = untagged.reading_order(0, ReadingOrderSource::Paint);

    assert_eq!((beyond.steps, beyond.backtracks), (Vec::new(), 0));
    assert_eq!((plain.steps, plain.backtracks), (Vec::new(), 0));
    assert_eq!(
        untagged.reading_order_comparison(0),
        OrderComparison {
            same: 0,
            total: 0,
            first_difference: None
        }
    );
    // Smoke.
    assert_eq!(untagged.tag_boxes(0), []);
}

// ---- struct-rich keeps working -----------------------------------------------------------------

#[test]
fn struct_rich_orders_each_page_by_its_own_content_and_raises_no_backtrack() {
    let doc = fixture("struct-rich");

    let first = doc.reading_order(0, ReadingOrderSource::Structure);
    let second = doc.reading_order(1, ReadingOrderSource::Structure);

    assert_eq!(
        ids(&first),
        [
            elem(12),
            elem(13),
            elem(14),
            elem(17),
            elem(18),
            elem(21),
            elem(22)
        ]
    );
    assert_eq!(
        first
            .steps
            .iter()
            .map(|s| (s.role.as_str(), s.class, s.preview.as_str()))
            .collect::<Vec<_>>(),
        [
            ("H1", RoleClass::Heading, "Rich structure"),
            (
                "P",
                RoleClass::Paragraph,
                "Un paragraphe qui continue sur la page suivante"
            ),
            ("Figure", RoleClass::Figure, "A pale blue rectangle"),
            ("TH", RoleClass::Table, "Name"),
            ("Cell", RoleClass::Table, "Value"),
            ("Lbl", RoleClass::List, "1."),
            ("LBody", RoleClass::List, "First item"),
        ]
    );
    // The paragraph continues on page 2, where the link and the callout follow it; `Callout`
    // reaches `Span` through the role map.
    assert_eq!(ids(&second), [elem(13), elem(23), elem(24)]);
    assert_eq!(
        second.steps.iter().map(|s| s.class).collect::<Vec<_>>(),
        [RoleClass::Paragraph, RoleClass::Link, RoleClass::Inline]
    );
    assert_eq!((first.backtracks, second.backtracks), (0, 0));
    assert!(
        doc.info()
            .diagnostics
            .iter()
            .all(|d| d.code != "reading-order-backtrack")
    );
    // Smoke: the paragraph spanning both pages has one box on page 2.
    assert_eq!(doc.tag_boxes(1).iter().filter(|b| b.id == elem(13)).count(), 1);
}

#[test]
fn role_class_keys_are_stable_and_complete() {
    let keys: Vec<&str> = RoleClass::ALL.iter().map(|c| c.key()).collect();

    assert_eq!(
        keys,
        [
            "heading", "p", "figure", "caption", "table", "list", "link", "inline", "group", "artifact",
            "other"
        ]
    );
    // Smoke.
    assert_eq!(RoleClass::ALL.len(), 11);
}

#[test]
fn a_marked_content_sequence_has_the_class_of_the_element_that_owns_it() {
    let doc = fixture("reading-order");

    // MCID 0 is the heading (a custom `Heading1` the role map sends to H1), 2 the figure, 3 its
    // caption; no element owns MCID 99.
    assert_eq!(doc.mcid_role_class(0, 0), Some(RoleClass::Heading));
    assert_eq!(doc.mcid_role_class(0, 2), Some(RoleClass::Figure));
    assert_eq!(doc.mcid_role_class(0, 3), Some(RoleClass::Caption));
    assert_eq!(doc.mcid_role_class(0, 99), None);
    // Smoke: it agrees with the class of the element itself.
    let heading = doc.reading_order(0, ReadingOrderSource::Structure).steps[0]
        .id
        .clone();
    assert_eq!(doc.role_class(&heading), doc.mcid_role_class(0, 0));
}
