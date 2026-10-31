//! Synthetic pages: marked content, streams, forms and failure modes.

use pretty_assertions::assert_eq;

use super::*;

#[test]
fn marked_content_properties_nest_and_resolve_resources() {
    let page = synth(
        b"/Span << /ActualText <FEFF00480069> /Alt (alt) /Lang (en-US) >> BDC \
            /P /MC0 BDC 0 0 10 10 re f /Foo /MC1 BDC 20 0 10 10 re f EMC EMC \
          EMC \
          /Figure BMC 40 0 10 10 re f",
        "/Properties << /MC0 << /MCID 7 /Lang (fr-FR) >> /MC1 6 0 R >>",
        &[(6, Obj::Plain("<< /Type /OC >>"))],
    );

    let seqs = &page.marked;

    assert_eq!(seqs.len(), 4);
    assert_eq!(seqs[0].tag, "Span");
    assert_eq!(
        seqs[0].properties,
        vec![
            ("ActualText".to_owned(), "<FEFF00480069>".to_owned()),
            ("Alt".to_owned(), "(alt)".to_owned()),
            ("Lang".to_owned(), "(en-US)".to_owned()),
        ]
    );
    assert_eq!(
        (
            seqs[0].actual_text.as_deref(),
            seqs[0].alt.as_deref(),
            seqs[0].lang.as_deref(),
            seqs[0].mcid
        ),
        (Some("Hi"), Some("alt"), Some("en-US"), None)
    );
    assert_eq!(seqs[0].property_resource, None);
    assert_eq!(
        (seqs[1].tag.as_str(), seqs[1].property_resource.as_deref()),
        ("P", Some("MC0"))
    );
    assert_eq!(
        (seqs[1].mcid, seqs[1].lang.as_deref(), seqs[1].parent),
        (Some(7), Some("fr-FR"), Some(0))
    );
    assert_eq!(seqs[2].properties, vec![("Type".to_owned(), "/OC".to_owned())]);
    assert_eq!(
        (seqs[2].property_resource.as_deref(), seqs[2].parent),
        (Some("MC1"), Some(1))
    );
    assert_eq!(
        (seqs[3].tag.as_str(), seqs[3].end_op, seqs[3].parent),
        ("Figure", None, None)
    );
    assert_eq!(
        seqs.iter().map(|s| s.end_op).collect::<Vec<_>>(),
        vec![Some(9), Some(8), Some(7), None]
    );
    // Items know every sequence around them; boxes aggregate upwards.
    assert_eq!(
        page.items.iter().map(|i| i.mc_stack.clone()).collect::<Vec<_>>(),
        vec![vec![0, 1], vec![0, 1, 2], vec![3]]
    );
    assert_rect(seqs[0].bbox.unwrap(), [0.0, 0.0, 30.0, 10.0]);
    assert_rect(seqs[2].bbox.unwrap(), [20.0, 0.0, 30.0, 10.0]);
    assert_rect(seqs[3].bbox.unwrap(), [40.0, 0.0, 50.0, 10.0]);
    assert_eq!(page.ops.iter().map(|o| o.mc_depth).max(), Some(3));
}

#[test]
fn unbalanced_emc_is_noted_and_ignored() {
    let page = synth(b"EMC 0 0 5 5 re f", "", &[]);

    assert_eq!(page.marked, Vec::new());
    assert_eq!(page.notes.len(), 1);
    assert_eq!(page.notes[0].op, Some(0));
    assert!(page.notes[0].message.contains("EMC"), "{:?}", page.notes[0]);
    assert_eq!(page.items.len(), 1);
    assert_eq!(page.ops[0].mc_depth, 0);
}

#[test]
fn contents_arrays_report_stream_index_and_stream_relative_spans() {
    let first: &[u8] = b"0 0 1 rg 10 10 20 20 re f";
    let second: &[u8] = b"  1 0 0 rg 40 40 10 10 re f";
    let page = capture_page(&synth_streams(&[first, second], "", &[]), 0).unwrap();

    let rows: Vec<_> = page
        .ops
        .iter()
        .map(|o| (o.operator.as_str(), o.stream.clone(), o.span))
        .collect();

    let s0 = StreamRef::PageContent { index: 0 };
    let s1 = StreamRef::PageContent { index: 1 };
    assert_eq!(
        rows,
        vec![
            ("rg", s0.clone(), Some((0, 8))),
            ("re", s0.clone(), Some((9, 23))),
            ("f", s0, Some((24, 25))),
            ("rg", s1.clone(), Some((2, 10))),
            ("re", s1.clone(), Some((11, 25))),
            ("f", s1, Some((26, 27))),
        ]
    );
    assert_eq!(page.items.len(), 2);
    assert_eq!(page.items[1].gstate.fill.components, vec![1.0, 0.0, 0.0]);
    assert_eq!(page.ops[3].local_ix, 3);
    assert_eq!(page.alignment, Alignment::Exact);
}

#[test]
fn forms_inherit_text_state_and_fonts_from_the_invoking_stream() {
    let page = synth(
        b"/F1 12 Tf 2 Tc /Fm Do",
        "/XObject << /Fm 6 0 R >>",
        &[(
            6,
            Obj::Stream(
                "/Type /XObject /Subtype /Form /BBox [0 0 200 200] /Resources << /ProcSet [/PDF] >>",
                b"BT 10 100 Td (AB) Tj ET",
            ),
        )],
    );

    let all = glyphs(&page);

    assert_eq!(text(&all), "AB");
    assert_eq!(all[0].item.gstate.text.font.as_deref(), Some("F1"));
    assert!(close(all[0].item.gstate.text.font_size, 12.0));
    // Character spacing 2 is inherited too.
    assert_points(&origins(&all), &[(10.0, 100.0), (20.004, 100.0)]);
    assert_eq!(all[0].item.xobject_stack, vec![0]);
    assert_eq!(page.alignment, Alignment::Exact);
}

#[test]
fn forms_showing_text_in_an_unreachable_inherited_font_degrade_the_page() {
    let resources = "/XObject << /Fm 6 0 R >>";
    let dict = "/Type /XObject /Subtype /Form /BBox [0 0 200 200] /Resources << /Font << /F9 5 0 R >> >>";
    let inherits = synth(
        b"/F1 12 Tf /Fm Do",
        resources,
        &[(6, Obj::Stream(dict, b"BT 10 100 Td (A) Tj ET"))],
    );
    let selects_its_own = synth(
        b"/F1 12 Tf /Fm Do",
        resources,
        &[(6, Obj::Stream(dict, b"BT /F9 12 Tf 10 100 Td (A) Tj ET"))],
    );

    match &inherits.alignment {
        Alignment::Degraded { reason } => assert!(reason.contains("inherited font F1"), "{reason}"),
        Alignment::Exact => panic!("expected a degraded page"),
    }
    assert_eq!(selects_its_own.alignment, Alignment::Exact);
    assert_eq!(glyphs(&selects_its_own).len(), 1);
    assert_eq!(glyphs(&inherits).len(), 1);
}

#[test]
fn self_referencing_forms_terminate() {
    let page = synth(
        b"/Fm Do",
        "/XObject << /Fm 6 0 R >>",
        &[(
            6,
            Obj::Stream(
                "/Type /XObject /Subtype /Form /BBox [0 0 200 200] /Resources << /XObject << /Fm 6 0 R >> >>",
                b"0 0 5 5 re f /Fm Do",
            ),
        )],
    );

    assert_eq!(page.xobjects.len(), 1);
    assert_eq!(fills(&page).len(), 1);
    assert!(
        page.notes
            .iter()
            .any(|n| n.message.contains("XObject Fm could not be resolved")),
        "{:?}",
        page.notes
    );
}

#[test]
fn malformed_operands_degrade_the_page_but_keep_the_operator_list() {
    let page = synth(b"0 0 10 re f 20 20 10 10 re f", "", &[]);

    assert_eq!(
        op_list(&page).iter().map(|o| o.0).collect::<Vec<_>>(),
        ["re", "f", "re", "f"]
    );
    match &page.alignment {
        Alignment::Degraded { reason } => assert!(reason.contains("stopped at operator 1"), "{reason}"),
        Alignment::Exact => panic!("expected a degraded page"),
    }
    assert_eq!(page.items, Vec::new());
}

#[test]
fn untokenizable_tails_degrade_the_page() {
    let page = synth(b"0 0 10 10 re f ] >> garbage 20 20 10 10 re f", "", &[]);

    assert_eq!(
        op_list(&page).iter().map(|o| o.0).collect::<Vec<_>>(),
        ["re", "f"]
    );
    match &page.alignment {
        Alignment::Degraded { reason } => assert!(reason.contains("byte 15"), "{reason}"),
        Alignment::Exact => panic!("expected a degraded page"),
    }
    assert_eq!(page.items.len(), 1);
}

#[test]
fn extra_operands_follow_the_last_n_rule() {
    let page = synth(b"1 2 3 4 5 6 7 cm 0 0 10 10 re f", "", &[]);

    assert_matrix(page.ops[0].gstate.ctm, [2.0, 3.0, 4.0, 5.0, 6.0, 7.0]);
    // The engine drew the same box the walker's CTM predicts.
    assert_rect(page.items[0].bbox, [6.0, 7.0, 66.0, 87.0]);
}

#[test]
fn missing_pages_and_empty_pages() {
    let pdf = synth_streams(&[b""], "", &[]);

    let page = capture_page(&pdf, 0).unwrap();

    assert!(page.ops.is_empty() && page.items.is_empty());
    assert_eq!(page.alignment, Alignment::Exact);
    assert!(capture_page(&pdf, 1).is_none());
    assert_eq!(capture_document(&pdf).len(), 1);
}
