//! Text previews of structure elements: whitespace is squashed, the text of several marked
//! contents is joined by one space, and long text is cut at the display limit (60 characters in a
//! tree row, 300 in the inspector) without reading more of the page than that.

#![allow(
    clippy::unwrap_used,
    clippy::panic,
    reason = "test helpers fail loudly on unexpected input"
)]

use std::fmt::Write as _;

use pdfxray_core::{Document, NodeId, ObjRef, TreeKind, TreeNode};
use pdfxray_fixtures::{PdfVersion, PdfWriter, Trailer};
use pretty_assertions::assert_eq;

/// A page whose content shows `strings` (PDF literal strings, already escaped), one MCID each
/// from 0, and a structure tree with one `P` per entry of `groups` (the MCIDs it holds).
fn doc_with(strings: &[&str], groups: &[&[usize]]) -> Document {
    let mut content = String::new();
    for (mcid, text) in strings.iter().enumerate() {
        let _ = writeln!(
            content,
            "/P << /MCID {mcid} >> BDC BT /F1 6 Tf 10 {} Td ({text}) Tj ET EMC",
            780 - 8 * mcid
        );
    }
    let mut w = PdfWriter::new(PdfVersion::V1_7);
    w.object(
        1,
        "<< /Type /Catalog /Pages 2 0 R /StructTreeRoot 6 0 R /MarkInfo << /Marked true >> >>",
    );
    w.object(2, "<< /Type /Pages /Kids [3 0 R] /Count 1 >>");
    w.object(
        3,
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 5 0 R >> >> \
         /Contents 4 0 R >>",
    );
    w.stream(4, "", content.as_bytes());
    w.object(5, "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>");
    let kids = (0..groups.len()).fold(String::new(), |mut out, i| {
        let _ = write!(out, "{} 0 R ", 10 + i);
        out
    });
    w.object(6, "<< /Type /StructTreeRoot /K 7 0 R >>");
    w.object(
        7,
        format!("<< /Type /StructElem /S /Document /P 6 0 R /K [{kids}] >>"),
    );
    for (i, mcids) in groups.iter().enumerate() {
        let k = mcids.iter().fold(String::new(), |mut out, m| {
            let _ = write!(out, "{m} ");
            out
        });
        w.object(
            10 + u32::try_from(i).unwrap(),
            format!("<< /Type /StructElem /S /P /P 7 0 R /Pg 3 0 R /K [{k}] >>"),
        );
    }
    w.finish_table(&Trailer::new(1)).unwrap();
    Document::from_bytes("preview.pdf", w.into_output().unwrap().bytes).unwrap()
}

fn detail_of(doc: &Document, obj: u32) -> Option<String> {
    fn find<'a>(nodes: &'a [TreeNode], id: &NodeId) -> Option<&'a TreeNode> {
        nodes.iter().find_map(|n| n.find(id))
    }
    let tree = doc.tree(TreeKind::Tags);
    find(&tree, &NodeId::StructElem(ObjRef::new(obj, 0))).and_then(|n| n.detail.clone())
}

/// The reference behaviour: squash white space, then cut to `max` characters.
fn expected_preview(text: &str, max: usize) -> String {
    let squashed = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if squashed.chars().count() <= max {
        squashed
    } else {
        let mut cut: String = squashed.chars().take(max - 1).collect();
        cut.push('…');
        cut
    }
}

#[test]
fn short_text_is_shown_whole_with_runs_of_spaces_collapsed() {
    let doc = doc_with(&["  Hello    wide   world  "], &[&[0]]);

    assert_eq!(
        detail_of(&doc, 10).as_deref(),
        Some("\"Hello wide world\" · 1 MCID")
    );
}

#[test]
fn text_of_several_marked_contents_is_joined_by_single_spaces() {
    let doc = doc_with(&["One ", " two", "three  ", "   "], &[&[0, 1, 2, 3]]);

    assert_eq!(
        detail_of(&doc, 10).as_deref(),
        Some("\"One two three\" · 4 MCIDs")
    );
}

#[test]
fn long_text_is_cut_to_sixty_characters_with_an_ellipsis() {
    let long = "alpha  beta gamma   delta epsilon zeta eta theta iota kappa lambda mu nu xi omicron pi rho";
    let doc = doc_with(&[long], &[&[0]]);

    let shown = detail_of(&doc, 10).unwrap();
    let quoted = shown.strip_suffix(" · 1 MCID").unwrap();
    let text = quoted.strip_prefix('"').unwrap().strip_suffix('"').unwrap();

    assert_eq!(text, expected_preview(long, 60));
    assert_eq!(text.chars().count(), 60);
    assert!(text.ends_with('…'), "{text}");
    assert!(text.starts_with("alpha beta gamma delta"), "{text}");
}

#[test]
fn text_of_exactly_the_limit_is_not_cut() {
    let sixty = "x".repeat(60);
    let sixty_one = "y".repeat(61);
    let doc = doc_with(&[&sixty, &sixty_one], &[&[0], &[1]]);

    assert_eq!(detail_of(&doc, 10), Some(format!("\"{sixty}\" · 1 MCID")));
    assert_eq!(
        detail_of(&doc, 11),
        Some(format!("\"{}…\" · 1 MCID", "y".repeat(59)))
    );
}

#[test]
fn preview_stops_at_the_limit_across_marked_contents_and_matches_the_reference() {
    let parts: Vec<String> = (0..12).map(|i| format!("segment{i}  ")).collect();
    let strings: Vec<&str> = parts.iter().map(String::as_str).collect();
    let all: Vec<usize> = (0..12).collect();
    let doc = doc_with(&strings, &[&all]);

    let shown = detail_of(&doc, 10).unwrap();

    let want = expected_preview(&parts.concat().replace("  ", " "), 60);
    assert_eq!(shown, format!("\"{want}\" · 12 MCIDs"));
    assert!(want.starts_with("segment0 segment1 segment2"), "{want}");
}

#[test]
fn inspector_text_is_cut_at_three_hundred_characters() {
    let words: Vec<String> = (0..90).map(|i| format!("w{i:03}")).collect();
    let text = words.join("   ");
    let doc = doc_with(&[&text], &[&[0]]);

    let props = doc.node(&NodeId::StructElem(ObjRef::new(10, 0))).unwrap();

    let shown = props.get("Content", "Text").unwrap();
    assert_eq!(shown, expected_preview(&text, 300));
    assert_eq!(shown.chars().count(), 300);
    assert!(shown.ends_with('…'));
}
