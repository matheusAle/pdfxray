//! The inspector on content: hints on its keys, colours, glyphs, coverage, operator rows and the pin.

use pretty_assertions::assert_eq;

use super::*;

/// Rests the pointer on the key `key` of an inspector row until its hint shows (the tooltip
/// delay is 400 ms; one step is 250 ms). Returns whether it was hidden after one step.
fn rest_on_key(harness: &mut egui_kittest::Harness<'_, pdfxray_app::PdfxrayApp>, key: &str) -> bool {
    let text = hint(key).expect("a hinted key");
    let at = the(harness, Region::Inspector, Role::Label, key).rect().center();
    harness.hover_at(at);
    harness.step();
    harness.remove_cursor();
    harness.hover_at(at);
    harness.step();
    let early = !has_label(harness, text);
    harness.run_steps(3);
    early
}

#[test]
fn the_ctm_label_explains_the_matrix_after_400_ms_and_the_value_opens_nothing() {
    let mut harness = opened("images", "Content");
    harness.state_mut().state.select(NodeId::Op { page: 0, ix: 2 });
    harness.run_steps(3);

    let hidden_at_first = rest_on_key(&mut harness, "CTM");

    assert!(hidden_at_first, "not before 400 ms");
    let text = hint("CTM").unwrap();
    assert!(has_label(&harness, text));
    assert!(text.contains("[a b c d e f] = scale x, skew y, skew x, scale y, translate x, translate y"));

    // The value is plain text now: clicking it opens nothing and keeps the selection.
    harness.remove_cursor();
    harness.run();
    assert!(
        harness
            .query_by_role_and_label(Role::Link, "[100 0 0 100 50 150]")
            .is_none()
    );
    let value = the(&harness, Region::Inspector, Role::Label, "[100 0 0 100 50 150]");
    value.click();
    harness.run();
    assert!(!has_label(&harness, "scale x"));
    assert!(!has_label(&harness, "Rotation 0° · no skew · uniform: yes"));
    // Smoke: the selection did not move and the value can still be copied.
    assert_eq!(
        harness.state().state.selection.primary,
        Some(NodeId::Op { page: 0, ix: 2 })
    );
    let _copy = the(&harness, Region::Inspector, Role::Button, "Copy CTM");
}

#[test]
fn the_ctm_stays_as_written_while_lengths_follow_the_units_switch() {
    let mut harness = opened("images", "Content");
    harness.state_mut().state.select(NodeId::Op { page: 0, ix: 2 });
    harness.state_mut().state.unit = pdfxray_app::Unit::Mm;
    harness.run_steps(3);

    // Lengths follow the switch: the box is 100 pt = 35.28 mm, the matrix itself stays as written.
    assert!(!has_label(&harness, "[50 150 150 250]"));
    assert!(has_label(&harness, "[17.64 52.92 52.92 88.19]"));
    assert!(has_label(&harness, "35.28 × 35.28 mm"));
    assert!(has_label(&harness, "[100 0 0 100 50 150]"));
}

#[test]
fn resting_on_the_clip_label_explains_w_w_star_and_n_after_400_ms() {
    let mut harness = opened("graphics-state", "Content");
    harness.state_mut().state.select(NodeId::Op { page: 0, ix: 59 });
    harness.run_steps(3);
    assert!(
        common::all_label_texts(&harness)
            .iter()
            .any(|t| t.starts_with("W n · rect ")),
        "a clip row"
    );

    let hidden_at_first = rest_on_key(&mut harness, "Clip");

    assert!(hidden_at_first, "not before 400 ms");
    let text = hint("Clip").unwrap();
    assert!(has_label(&harness, text));
    assert!(text.contains("W*") && text.contains("even-odd") && text.contains("n ends the path"));
    // Smoke: the hint is named after the key it explains.
    assert!(has_label(&harness, "Clip"));
}

#[test]
fn colour_swatches_show_hex_space_and_the_operator_that_set_them() {
    let mut harness = opened("graphics-state", "Content");
    harness.state_mut().state.select(NodeId::Op { page: 0, ix: 37 });
    harness.run_steps(3);

    let labels = texts_in(&harness, Region::Inspector, Role::Label);
    let fill = labels.iter().position(|t| t == "Fill").expect("a Fill row");
    assert_eq!(labels[fill + 1], "#008000 DeviceRGB\u{a0}·\u{a0}rg");
    let stroke = labels.iter().position(|t| t == "Stroke").expect("a Stroke row");
    // A `q` … `Q` earlier changed the stroke colour and `Q` brought the black back.
    assert_eq!(labels[stroke + 1], "#000000 DeviceGray\u{a0}·\u{a0}Q (restored)");
}

#[test]
fn the_glyph_inspector_shows_code_unicode_width_and_the_font() {
    let mut harness = opened("minimal-one-page", "Content");
    harness.state_mut().state.select(NodeId::Glyph {
        page: 0,
        op: 3,
        glyph: 0,
    });
    harness.run_steps(3);
    // Fold the sections above, so the ones under them are on screen. One at a time: the
    // sections have 14 px of padding now, so each fold moves the next header.
    for section in ["Geometry", "Graphics state"] {
        the(&harness, Region::Inspector, Role::Button, section).click();
        harness.run_steps(2);
    }

    let labels = texts_in(&harness, Region::Inspector, Role::Label);
    let value_of = |key: &str| {
        let at = labels
            .iter()
            .position(|t| t == key)
            .unwrap_or_else(|| panic!("{key} in {labels:?}"));
        labels[at + 1].clone()
    };
    assert_eq!(value_of("Char code"), "<48>  1 byte");
    assert_eq!(value_of("Unicode"), "U+0048 'H'  via encoding");
    assert_eq!(value_of("Width"), "722 /1000 · 13 pt");
    assert_eq!(value_of("ActualText"), "—  (inherits none)");
    assert_eq!(value_of("Subtype"), "/Type1");
    assert_eq!(value_of("Embedded"), "not embedded");
    assert!(
        labels.contains(&"glyph:0:3:0 · in Tj op #3".to_owned()),
        "{labels:?}"
    );
    assert!(labels.contains(&"Glyph 'H'".to_owned()));
    // Smoke: the text state of the operator.
    assert_eq!(value_of("Tf"), "18 pt");
}

#[test]
fn the_coverage_bar_and_the_file_info_hover_count_tagged_artifact_and_untagged_items() {
    let mut harness = opened("untagged-content", "Content");
    let doc = Document::open(common::fixture_path("untagged-content")).unwrap();
    let coverage = doc.coverage(0).unwrap();
    assert!(coverage.untagged > 0 && coverage.tagged > 0 && coverage.artifact > 0);

    for expected in [
        format!("Tagged {}", coverage.tagged),
        format!("Artifact {}", coverage.artifact),
        format!("Untagged {}", coverage.untagged),
        "Page 1 coverage".to_owned(),
    ] {
        assert!(has_label(&harness, &expected), "{expected}");
    }
    // The status bar keeps the same counts in the hover of its file info.
    let counts = format!(
        "{} tagged · {} artifacts · {} untagged",
        coverage.tagged, coverage.artifact, coverage.untagged
    );
    assert!(!has_label(&harness, &counts));
    the(&harness, Region::Chrome, Role::Button, "file info").hover();
    harness.run_steps(12);
    assert!(
        has_label(&harness, &counts),
        "{:?}",
        common::all_label_texts(&harness)
    );
    // The untagged operators are marked red in the tree.
    let untagged_rows = texts_in(&harness, Region::Navigator, Role::Label)
        .iter()
        .filter(|t| *t == "untagged")
        .count();
    assert!(untagged_rows >= 1, "{untagged_rows}");
}

#[test]
fn an_untagged_operator_is_called_out_on_the_canvas_label() {
    let mut harness = opened("untagged-content", "Content");
    harness.state_mut().state.select(NodeId::Op { page: 0, ix: 17 });
    harness.run_steps(2);

    let label = harness.state_mut().state.selection_label().unwrap();
    assert!(label.warn);
    assert_eq!(label.text, "⚠ untagged · op #17 · not in an artifact");
    // A tagged one is named by its structure: the owner, the MCID and the operator.
    harness.state_mut().state.select(NodeId::Op { page: 0, ix: 11 });
    assert_eq!(
        harness.state_mut().state.selection_label().unwrap().text,
        "P · MCID 0 · Tj #11"
    );
}

#[test]
fn clicking_an_operator_row_selects_it_and_the_current_one_is_highlighted() {
    let mut harness = opened("text-state", "Content");

    the(&harness, Region::Drawer, Role::Button, "Tf").click();
    harness.run();

    assert_eq!(
        harness.state().state.selection.primary,
        Some(NodeId::Op { page: 0, ix: 1 })
    );
    assert!(is_selected(&the(&harness, Region::Drawer, Role::Button, "Tf")));
    assert!(!is_selected(&the(&harness, Region::Drawer, Role::Button, "Td")));
    // Smoke: what the state column says about the rows of the page.
    let state = &harness.state().state;
    let captured = state.document.as_ref().unwrap().doc.captured(0).unwrap();
    let said = |ix: usize| pdfxray_app::drawer::operators::state_text(captured, &captured.ops[ix]);
    assert_eq!(said(0), "text object");
    assert_eq!(said(1), "font F1 12pt");
    assert_eq!(said(2), "Tlm [1 0 0 1 20 270]");
    assert_eq!(said(3), "8 glyphs · 40.0 pt");
    // The table says where the stream is and how big it decoded.
}

#[test]
fn the_pin_keeps_the_inspector_on_its_node_while_the_selection_moves() {
    let mut harness = opened("struct-rich", "Content");
    harness.state_mut().state.select(NodeId::Op { page: 0, ix: 4 });
    harness.run_steps(2);
    the(&harness, Region::Inspector, Role::Button, "Pin").click();
    harness.run();

    harness.state_mut().state.select(NodeId::Op { page: 0, ix: 11 });
    harness.run_steps(2);
    assert!(has_label(&harness, "op:0:4 · op #4"));
    assert!(!has_label(&harness, "op:0:11 · op #11"));
    // Unpinning follows the selection again.
    the(&harness, Region::Inspector, Role::Button, "Pin").click();
    harness.run_steps(2);
    assert!(has_label(&harness, "op:0:11 · op #11"));
}
