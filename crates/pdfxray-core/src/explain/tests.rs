use pretty_assertions::assert_eq;

use super::*;

fn ops(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| (*s).to_owned()).collect()
}

fn note(op: &str, operands: &[&str]) -> String {
    operator(op, &ops(operands))
}

#[test]
fn the_design_examples_read_as_in_the_export() {
    assert_eq!(note("q", &[]), "save graphics state");
    assert_eq!(note("Q", &[]), "restore graphics state");
    assert_eq!(note("BT", &[]), "begin text");
    assert_eq!(note("ET", &[]), "end text");
    assert_eq!(note("EMC", &[]), "end tagged content");
    assert_eq!(note("Tf", &["/F1", "26"]), "font F1 at 26 pt");
    assert_eq!(note("Td", &["40", "676"]), "move to x 40, y 676");
    assert_eq!(
        note("BDC", &["/H1", "<< /MCID 0 >>"]),
        "start tagged content: H1, MCID 0"
    );
    assert_eq!(
        operator_with_text("TJ", &ops(&["[<41>-12<6E6E75616C>]"]), Some("Annual Report 2025")),
        "draw “Annual Report 2025”"
    );
}

#[test]
fn graphics_state_and_path_operators_fill_in_their_operands() {
    assert_eq!(note("w", &["2"]), "set line width to 2");
    assert_eq!(note("J", &["1"]), "set line cap: round ends");
    assert_eq!(note("j", &["2"]), "set line join: bevelled corners");
    assert_eq!(note("M", &["10"]), "set miter limit to 10");
    assert_eq!(note("d", &["[6 3]", "2"]), "set dashed lines: 6 3");
    assert_eq!(note("d", &["[]", "0"]), "set solid lines");
    assert_eq!(note("gs", &["/GS1"]), "apply graphics-state settings GS1");
    assert_eq!(note("m", &["10", "20.5"]), "start a path at x 10, y 20.5");
    assert_eq!(
        note("re", &["36", "30", "540", "732"]),
        "rectangle 540 × 732 at x 36, y 30"
    );
    assert_eq!(note("c", &["1", "2", "3", "4", "5", "6"]), "curve to x 5, y 6");
    assert_eq!(note("f*", &[]), "fill the path (even-odd)");
    assert_eq!(note("W", &[]), "clip to the path (non-zero winding)");
    assert_eq!(note("n", &[]), "end the path without painting");
}

#[test]
fn cm_describes_what_the_matrix_does() {
    assert_eq!(
        note("cm", &["340", "0", "0", "150", "40", "222"]),
        "place: scale 340 × 150, move to 40, 222"
    );
    assert_eq!(note("cm", &["0", "1", "-1", "0", "0", "0"]), "place: rotate 90°");
    assert_eq!(note("cm", &["2", "0", "0", "2", "0", "0"]), "place: scale 2×");
    assert_eq!(
        note("cm", &["1", "0", "0", "1", "0", "0"]),
        "change the coordinate system (no change)"
    );
    assert_eq!(note("cm", &["1", "2"]), "change the coordinate system");
}

#[test]
fn text_operators_use_the_decoded_text_when_given() {
    assert_eq!(note("Tc", &["0.5"]), "set character spacing to 0.5");
    assert_eq!(note("Tz", &["80"]), "set horizontal scaling to 80%");
    assert_eq!(note("Tr", &["3"]), "text mode: invisible");
    assert_eq!(
        note("Tm", &["1", "0", "0", "1", "40", "676"]),
        "set text matrix, origin x 40, y 676"
    );
    assert_eq!(note("T*", &[]), "move to the next line");
    assert_eq!(
        note("Tj", &["(DRAFT)"]),
        "draw “DRAFT”",
        "a plain literal is shown as written"
    );
    assert_eq!(note("Tj", &["<0041>"]), "show text");
    assert_eq!(
        note("TJ", &["[(A) -20 (B)]"]),
        "show text with spacing adjustments"
    );
    assert_eq!(
        operator_with_text("Tj", &ops(&["<0041>"]), Some("Hello")),
        "draw “Hello”"
    );
    assert_eq!(note("\"", &["1", "2", "(Hi)"]), "draw “Hi”");
}

#[test]
fn long_text_is_cut_to_keep_the_note_short() {
    let long = "The quick brown fox jumps over the lazy dog again and again";
    let shown = operator_with_text("Tj", &ops(&["<00>"]), Some(long));

    assert_eq!(shown, "draw “The quick brown fox jumps ove…”");
    assert!(shown.chars().count() <= 60);
}

#[test]
fn colour_shading_xobject_and_marked_content_operators() {
    assert_eq!(note("g", &["0.5"]), "fill grey 0.5");
    assert_eq!(note("RG", &["1", "0", "0"]), "stroke colour RGB 1 0 0");
    assert_eq!(
        note("rg", &["0.118", "0.227", "0.373"]),
        "fill colour RGB 0.118 0.227 0.373"
    );
    assert_eq!(note("k", &["0", "1", "1", "0"]), "fill colour CMYK 0 1 1 0");
    assert_eq!(note("cs", &["/Cs1"]), "fill colour space: Cs1");
    assert_eq!(note("scn", &["0.2", "0.4", "/P1"]), "fill colour 0.2 0.4");
    assert_eq!(note("sh", &["/Sh1"]), "paint shading Sh1");
    assert_eq!(note("Do", &["/Im1"]), "draw image or form /Im1");
    assert_eq!(note("BMC", &["/Span"]), "start tagged content: Span");
    assert_eq!(note("BMC", &["/Artifact"]), "start artifact (not real content)");
    assert_eq!(
        note("BDC", &["/Artifact", "<< /Type /Pagination /Subtype /Header >>"]),
        "start artifact: Pagination Header"
    );
    assert_eq!(
        note("BDC", &["/Span", "/MC0"]),
        "start tagged content: Span (/MC0)"
    );
    assert_eq!(note("MP", &["/Tag"]), "marker point: Tag");
    assert_eq!(note("BX", &[]), "begin compatibility section");
}

#[test]
fn missing_operands_read_as_question_marks_and_unknown_operators_are_named() {
    assert_eq!(note("Td", &[]), "move to x ?, y ?");
    assert_eq!(note("Tf", &["/F1"]), "font F1 at ? pt");
    assert_eq!(note("zz", &[]), "unknown operator zz");
}

#[test]
fn every_iso_32000_2_operator_has_a_short_lowercase_note() {
    let all = known_operators();

    assert_eq!(all.len(), 73);
    let unique: std::collections::BTreeSet<&&str> = all.iter().collect();
    assert_eq!(unique.len(), 73, "no operator is listed twice");
    for op in all {
        let shown = note(op, &["1", "2", "3", "4", "5", "6"]);
        assert!(!shown.starts_with("unknown"), "{op} has no entry: {shown}");
        assert!(shown.chars().count() <= 60, "{op}: {shown}");
        assert!(
            shown.chars().next().is_some_and(|c| !c.is_uppercase()),
            "{op} starts in lowercase: {shown}"
        );
        assert!(is_known_operator(op));
        let bare = note(op, &[]);
        assert!(
            !bare.is_empty() && bare.chars().count() <= 60,
            "{op} without operands: {bare}"
        );
    }
    assert!(!is_known_operator("zz"));
}

#[test]
fn every_diagnostic_code_of_this_crate_and_the_raw_layer_is_explained() {
    for code in pdfxray_raw::RawDiagnosticKind::CODES {
        assert!(diagnostic(code).is_some(), "no explanation for raw code {code}");
    }
    for code in [
        "page-tree-missing",
        "page-tree-bad-kid",
        "page-tree-too-deep",
        "page-count-mismatch",
        "box-outside-mediabox",
        "duplicate-element-id",
        "id-tree-mismatch",
        "parent-tree-missing",
        "struct-parent-missing",
        "parent-tree-mismatch",
        "objr-target-missing",
        "mcid-not-in-content",
        "mcid-not-in-structure",
        "mcid-duplicate-in-content",
        "marked-content-in-artifact",
        "untagged-content",
        "marked-true-no-struct-tree",
        "marked-false-but-tagged",
        "pg-missing",
        "mcid-referenced-twice",
        "struct-kid-invalid",
        "pg-not-a-page",
        "struct-elem-direct",
        "struct-tree-too-deep",
        "struct-elem-cycle",
        "struct-elem-multiple-parents",
        "class-not-found",
        "struct-elem-no-type",
        "struct-elem-parent-mismatch",
        "non-standard-type",
        "rolemap-cycle",
        "reading-order-backtrack",
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
    ] {
        assert!(diagnostic(code).is_some(), "no explanation for {code}");
    }
    assert_eq!(diagnostic("not-a-code"), None);
    assert_eq!(diagnostic_codes().count(), 44 + 5 + 27 + 1 + 10);
}
