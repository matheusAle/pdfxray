//! The speech order: what each reader preset announces for the generated speech fixtures and for
//! small documents that exercise one rule each.

#![allow(
    clippy::unwrap_used,
    clippy::panic,
    reason = "test helpers fail loudly on unexpected input"
)]

use pdfxray_core::{
    Document, LangChange, LinkTarget, NodeId, NoteLinkSource, NoteRole, ObjRef, RoleClass, SkipReason,
    SpeechItem, SpeechOrder, SpeechPreset, Substitution,
};
use pdfxray_fixtures::{PdfVersion, PdfWriter, Trailer};
use pretty_assertions::assert_eq;

fn fixture(name: &str) -> Document {
    Document::open(pdfxray_fixtures::pdf_path(name)).unwrap()
}

fn elem(n: u32) -> NodeId {
    NodeId::StructElem(ObjRef::new(n, 0))
}

/// Text, language and `(from, to)` of the language change of an item.
type LangRow<'a> = (
    &'a str,
    Option<&'a str>,
    Option<(Option<&'a str>, Option<&'a str>)>,
);

fn reason(skipped: &SkipReason) -> String {
    match skipped {
        SkipReason::Artifact => "artifact".to_owned(),
        SkipReason::Private => "private".to_owned(),
        SkipReason::Empty => "empty".to_owned(),
        SkipReason::CoveredBy(by) => format!("covered by {by}"),
    }
}

/// `role · text`, with `-` for no role and the skip reason in front of an item that is not spoken.
fn line(item: &SpeechItem) -> String {
    let text = format!("{} · {}", item.role.as_deref().unwrap_or("-"), item.text);
    match &item.skipped {
        Some(skipped) => format!("({}) {text}", reason(skipped)),
        None => text,
    }
}

fn lines(order: &SpeechOrder) -> Vec<String> {
    order.items.iter().map(line).collect()
}

fn spoken_lines(order: &SpeechOrder) -> Vec<String> {
    order.spoken().map(line).collect()
}

// ---- presets ----------------------------------------------------------------------------------

#[test]
fn presets_are_labelled_and_only_the_spec_one_is_not_an_approximation() {
    let labels: Vec<&str> = SpeechPreset::ALL.iter().map(|p| p.label()).collect();
    assert_eq!(
        labels,
        [
            "Structure (spec)",
            "Acrobat on Windows (NVDA, JAWS)",
            "Firefox (pdf.js)",
            "Chrome and Edge (default)"
        ]
    );
    assert_eq!(
        SpeechPreset::ALL.map(SpeechPreset::is_approximation),
        [false, true, true, true]
    );
    assert_eq!(
        SpeechPreset::ALL.map(|p| p.rules().len()),
        [0, 1, 3, 4],
        "rule differences listed per preset"
    );
    let doc = fixture("speech-artifacts");
    let order = doc.speech_order(SpeechPreset::FirefoxPdfJs);
    assert_eq!(order.preset, SpeechPreset::FirefoxPdfJs);
    // Smoke: the order says it is an approximation.
    assert!(order.approximation);
    assert!(!doc.speech_order(SpeechPreset::Structure).approximation);
}

#[test]
fn every_rule_difference_names_its_evidence() {
    for preset in SpeechPreset::ALL {
        for rule in preset.rules() {
            assert!(
                rule.evidence.contains('['),
                "{:?}: {} cites no source",
                preset,
                rule.topic
            );
            assert!(rule.behaviour.len() > 10, "{}", rule.topic);
        }
    }
    let topics: Vec<&str> = SpeechPreset::ChromiumDefault
        .rules()
        .iter()
        .map(|r| r.topic)
        .collect();
    assert_eq!(topics, ["Order", "Artifacts", "Roles", "Untagged content"]);
}

// ---- speech-alt-hides -------------------------------------------------------------------------

#[test]
fn an_alt_on_a_sect_replaces_its_paragraphs_in_structure_and_acrobat() {
    let doc = fixture("speech-alt-hides");
    let want = [
        "H1 · Quarterly report",
        "Sect · Summary of results",
        "(covered by struct:9:0) P · Revenue grew in every region.",
        "(covered by struct:9:0) P · Costs stayed flat this year.",
        "Figure · Revenue chart",
        "Figure · Cost chart",
        "P · Closing remarks follow.",
    ];

    for preset in [SpeechPreset::Structure, SpeechPreset::AcrobatWindows] {
        assert_eq!(lines(&doc.speech_order(preset)), want, "{preset:?}");
    }

    let order = doc.speech_order(SpeechPreset::Structure);
    let sect = &order.items[1];
    assert_eq!(sect.node, elem(9));
    assert_eq!(sect.substituted_by, Some(Substitution::Alt));
    assert_eq!(
        sect.painted.as_deref(),
        Some("Revenue grew in every region. Costs stayed flat this year.")
    );
    assert_eq!(sect.class, RoleClass::Group);
    assert_eq!(
        order.items.iter().map(|i| i.index).collect::<Vec<_>>(),
        [1, 2, 3, 4, 5, 6, 7]
    );
    // Smoke: five items are spoken, two are hidden under the Sect.
    assert_eq!(order.spoken().count(), 5);
}

#[test]
fn pdf_js_reads_the_paragraphs_under_an_alt_and_replaces_only_a_figure_with_a_box() {
    let doc = fixture("speech-alt-hides");

    let order = doc.speech_order(SpeechPreset::FirefoxPdfJs);

    assert_eq!(
        lines(&order),
        [
            "H1 · Quarterly report",
            "P · Revenue grew in every region.",
            "P · Costs stayed flat this year.",
            "Figure · Revenue chart",
            // No /BBox: the figure is not replaced, so it has no text of its own.
            "Figure · ",
            "P · Closing remarks follow.",
        ]
    );
    assert_eq!(
        order.items.iter().map(|i| i.substituted_by).collect::<Vec<_>>(),
        [None, None, None, Some(Substitution::Alt), None, None]
    );
    // Smoke: nothing is skipped.
    assert_eq!(order.spoken().count(), order.items.len());
}

#[test]
fn chromium_ignores_the_tags_and_reads_the_text_in_paint_order() {
    let doc = fixture("speech-alt-hides");

    let order = doc.speech_order(SpeechPreset::ChromiumDefault);

    assert_eq!(
        lines(&order),
        [
            "- · Quarterly report",
            "- · Revenue grew in every region.",
            "- · Costs stayed flat this year.",
            "- · Closing remarks follow.",
        ]
    );
    // The items still point at the elements, so a front end can draw them.
    assert_eq!(
        order.items.iter().map(|i| i.node.clone()).collect::<Vec<_>>(),
        [elem(8), elem(10), elem(11), elem(14)]
    );
    assert_eq!(order.items[0].class, RoleClass::Other);
    // Smoke.
    assert_eq!(order.spoken().count(), 4);
}

// ---- speech-artifacts -------------------------------------------------------------------------

#[test]
fn artifacts_are_skipped_beside_the_content_they_were_painted_next_to() {
    let doc = fixture("speech-artifacts");

    for preset in [
        SpeechPreset::Structure,
        SpeechPreset::AcrobatWindows,
        SpeechPreset::FirefoxPdfJs,
    ] {
        let order = doc.speech_order(preset);
        assert_eq!(
            lines(&order),
            [
                "(artifact) - · Annual Report 2025",
                "H1 · Results",
                "P · Revenue grew in every region.",
                "(artifact) - · Page 1 of 9",
            ],
            "{preset:?}"
        );
        assert_eq!(
            spoken_lines(&order),
            ["H1 · Results", "P · Revenue grew in every region."],
            "{preset:?}"
        );
    }

    let order = doc.speech_order(SpeechPreset::Structure);
    assert_eq!(
        order.items.iter().map(|i| i.node.clone()).collect::<Vec<_>>(),
        [
            NodeId::Artifact { page: 0, seq: 0 },
            elem(8),
            elem(9),
            NodeId::Artifact { page: 0, seq: 2 },
        ]
    );
    // Smoke: the layout rule paints no text and is not listed.
    assert_eq!(order.items.len(), 4);
}

#[test]
fn chromium_reads_the_artifacts_where_they_are_painted() {
    let doc = fixture("speech-artifacts");

    let order = doc.speech_order(SpeechPreset::ChromiumDefault);

    assert_eq!(
        lines(&order),
        [
            "- · Annual Report 2025",
            "- · Results",
            "- · Revenue grew in every region.",
            "- · Page 1 of 9",
        ]
    );
    assert_eq!(order.items[0].node, NodeId::Artifact { page: 0, seq: 0 });
    // Smoke: all four are spoken.
    assert_eq!(order.spoken().count(), 4);
}

// ---- speech-footnote --------------------------------------------------------------------------

#[test]
fn a_note_is_read_where_it_sits_and_linked_to_its_reference_by_its_label() {
    let doc = fixture("speech-footnote");

    let order = doc.speech_order(SpeechPreset::Structure);

    assert_eq!(
        lines(&order),
        [
            "P · Revenue grew in every region.",
            "Reference · 1",
            "P · Closing remarks follow.",
            "Lbl · 1",
            "P · Source: annual report.",
        ]
    );
    let flags: Vec<_> = order
        .items
        .iter()
        .map(|i| {
            i.note
                .as_ref()
                .map(|n| (n.role, n.other.clone(), n.via, n.items_apart))
        })
        .collect();
    assert_eq!(
        flags,
        [
            None,
            Some((NoteRole::Reference, elem(11), NoteLinkSource::LabelMatch, 1)),
            None,
            Some((NoteRole::Note, elem(9), NoteLinkSource::LabelMatch, 1)),
            None,
        ]
    );
    // Smoke: the reference and its note are one paragraph apart.
    assert_eq!(order.items[1].note.as_ref().map(|n| n.items_apart), Some(1));
}

#[test]
fn chromium_flags_no_notes_and_keeps_the_paint_order() {
    let doc = fixture("speech-footnote");

    let order = doc.speech_order(SpeechPreset::ChromiumDefault);

    assert_eq!(
        order.items.iter().map(|i| i.text.as_str()).collect::<Vec<_>>(),
        [
            "Revenue grew in every region.",
            "1",
            "Closing remarks follow.",
            "1",
            "Source: annual report."
        ]
    );
    assert_eq!(order.items.iter().filter(|i| i.note.is_some()).count(), 0);
}

// ---- speech-lang ------------------------------------------------------------------------------

#[test]
fn the_language_changes_with_a_span_element_and_with_a_marked_content_property() {
    let doc = fixture("speech-lang");

    let order = doc.speech_order(SpeechPreset::Structure);

    let seen: Vec<LangRow> = order
        .items
        .iter()
        .map(|i| {
            (
                i.text.as_str(),
                i.lang.as_deref(),
                i.lang_change
                    .as_ref()
                    .map(|c: &LangChange| (c.from.as_deref(), c.to.as_deref())),
            )
        })
        .collect();
    assert_eq!(
        seen,
        [
            ("The German word", Some("en-US"), None),
            (
                "Gesamtumsatz",
                Some("de-DE"),
                Some((Some("en-US"), Some("de-DE")))
            ),
            (
                "means total revenue.",
                Some("en-US"),
                Some((Some("de-DE"), Some("en-US")))
            ),
            (
                "Bonjour tout le monde.",
                Some("fr-FR"),
                Some((Some("en-US"), Some("fr-FR")))
            ),
            (
                "Back in English.",
                Some("en-US"),
                Some((Some("fr-FR"), Some("en-US")))
            ),
        ]
    );
    // Smoke: four changes in five items.
    assert_eq!(order.items.iter().filter(|i| i.lang_change.is_some()).count(), 4);
}

#[test]
fn chromium_speaks_the_document_language_throughout() {
    let doc = fixture("speech-lang");

    let order = doc.speech_order(SpeechPreset::ChromiumDefault);

    assert_eq!(
        order.items.iter().map(|i| i.lang.as_deref()).collect::<Vec<_>>(),
        [Some("en-US"); 5]
    );
    assert_eq!(order.items.iter().filter(|i| i.lang_change.is_some()).count(), 0);
}

// ---- speech-link-objr -------------------------------------------------------------------------

#[test]
fn object_references_are_announced_at_their_tree_position_with_their_target() {
    let doc = fixture("speech-link-objr");

    let order = doc.speech_order(SpeechPreset::Structure);

    assert_eq!(
        lines(&order),
        [
            "P · Please read the following.",
            "Link · Annual report",
            // A Link is named by its text; /Contents is not its name.
            "Link · ",
            // A widget is named by /TU.
            "Form · Your name",
            "P · Thank you.",
        ]
    );
    assert_eq!(
        order.items.iter().map(|i| i.node.clone()).collect::<Vec<_>>(),
        [
            elem(8),
            NodeId::Object(ObjRef::new(14, 0)),
            NodeId::Object(ObjRef::new(15, 0)),
            NodeId::Object(ObjRef::new(16, 0)),
            elem(12),
        ]
    );
    let links: Vec<_> = order
        .items
        .iter()
        .map(|i| {
            i.link
                .as_ref()
                .map(|l| (l.subtype.as_str(), l.target.clone(), l.contents.as_deref()))
        })
        .collect();
    assert_eq!(
        links,
        [
            None,
            Some((
                "Link",
                Some(LinkTarget::Uri("https://example.com/report".to_owned())),
                None
            )),
            Some(("Link", Some(LinkTarget::Page(0)), Some("Downloads page"))),
            Some(("Widget", None, None)),
            None,
        ]
    );
    assert_eq!(
        order.items[1].marked_content,
        [NodeId::MarkedContent { page: 0, mcid: 1 }]
    );
    // Smoke: the three annotations are in the order.
    assert_eq!(order.items.iter().filter(|i| i.link.is_some()).count(), 3);
}

#[test]
fn chromium_places_no_annotation_and_reads_the_link_text_where_it_is_painted() {
    let doc = fixture("speech-link-objr");

    let order = doc.speech_order(SpeechPreset::ChromiumDefault);

    assert_eq!(
        order.items.iter().map(|i| i.text.as_str()).collect::<Vec<_>>(),
        ["Please read the following.", "Annual report", "Thank you."]
    );
    assert_eq!(order.items.iter().filter(|i| i.link.is_some()).count(), 0);
}

// ---- rolemap-two-hops -------------------------------------------------------------------------

#[test]
fn pdf_js_follows_one_role_map_hop_and_the_others_follow_the_chain() {
    let doc = fixture("rolemap-two-hops");

    let roles = |preset| {
        doc.speech_order(preset)
            .items
            .iter()
            .map(|i| (i.declared_role.clone(), i.role.clone(), i.level, i.class))
            .collect::<Vec<_>>()
    };
    let headline =
        |role: Option<&str>, level, class| ("Headline".to_owned(), role.map(str::to_owned), level, class);
    let para = (
        "Para".to_owned(),
        Some("P".to_owned()),
        None,
        RoleClass::Paragraph,
    );
    let plain = ("P".to_owned(), Some("P".to_owned()), None, RoleClass::Paragraph);

    for preset in [SpeechPreset::Structure, SpeechPreset::AcrobatWindows] {
        assert_eq!(
            roles(preset),
            [
                headline(Some("H1"), Some(1), RoleClass::Heading),
                para.clone(),
                plain.clone()
            ],
            "{preset:?}"
        );
    }
    assert_eq!(
        roles(SpeechPreset::FirefoxPdfJs),
        [headline(None, None, RoleClass::Other), para, plain]
    );
    // Smoke: the text is the same in every tag-following preset.
    assert_eq!(
        spoken_lines(&doc.speech_order(SpeechPreset::FirefoxPdfJs)),
        [
            "- · Annual results",
            "P · Revenue grew in every region.",
            "P · Closing remarks follow."
        ]
    );
}

// ---- small documents --------------------------------------------------------------------------

/// A tagged page whose `content` marked-content sequences are owned by the elements of `owners`
/// (MCID n by `owners[n]`), under a `Document` (object 7) that holds `elems` (object, body).
fn inline_doc(
    version: PdfVersion,
    content: &str,
    root_extra: &str,
    owners: &[u32],
    elems: &[(u32, String)],
) -> Document {
    let mut w = PdfWriter::new(version);
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
        format!("<< /Type /StructTreeRoot /K 7 0 R /ParentTree 90 0 R /ParentTreeNextKey 1 {root_extra} >>"),
    );
    for (obj, body) in elems {
        w.object(*obj, body);
    }
    let nums: Vec<String> = owners.iter().map(|o| format!("{o} 0 R")).collect();
    w.object(90, format!("<< /Nums [0 [{}]] >>", nums.join(" ")));
    w.finish_table(&Trailer::new(1)).unwrap();
    Document::from_bytes("inline.pdf", w.into_output().unwrap().bytes).unwrap()
}

/// Five kinds of substitution and skip in one document.
fn mixed_doc() -> Document {
    let content = "\
/Span << /MCID 0 >> BDC BT /F1 12 Tf 20 380 Td (painted one) Tj ET EMC
/Span << /MCID 1 >> BDC BT /F1 12 Tf 20 360 Td (W3C) Tj ET EMC
/P << /MCID 2 >> BDC BT /F1 12 Tf 20 340 Td (secret) Tj ET EMC
/P << /MCID 3 /ActualText (fi) >> BDC BT /F1 12 Tf 20 320 Td (x) Tj ET EMC
/P << /MCID 4 >> BDC BT /F1 12 Tf 3 Tr 20 300 Td (ocr text) Tj ET EMC";
    let elem_body = |s: &str| format!("<< /Type /StructElem /P 7 0 R /Pg 3 0 R {s} >>");
    inline_doc(
        PdfVersion::V1_7,
        content,
        "",
        &[8, 9, 14, 11, 12],
        &[
            (
                7,
                "<< /Type /StructElem /S /Document /P 6 0 R /K [8 0 R 9 0 R 10 0 R 11 0 R 12 0 R 13 0 R] >>"
                    .to_owned(),
            ),
            (
                8,
                elem_body("/S /Span /Alt (alt text) /ActualText (actual text) /K 0"),
            ),
            (9, elem_body("/S /Span /E (World Wide Web Consortium) /K 1")),
            (10, elem_body("/S /Private /K [14 0 R]")),
            (11, elem_body("/S /P /K 3")),
            (12, elem_body("/S /P /K 4")),
            (13, elem_body("/S /P")),
            (
                14,
                "<< /Type /StructElem /S /P /P 10 0 R /Pg 3 0 R /K 2 >>".to_owned(),
            ),
        ],
    )
}

#[test]
fn alt_wins_over_actual_text_and_only_the_spec_keeps_the_other_beside_it() {
    let doc = mixed_doc();

    let alt = |preset| {
        let order = doc.speech_order(preset);
        let first = order.items[0].clone();
        (
            first.text,
            first.substituted_by,
            first.also_actual_text,
            first.painted,
        )
    };

    let spec = (
        "alt text".to_owned(),
        Some(Substitution::Alt),
        Some("actual text".to_owned()),
        Some("painted one".to_owned()),
    );
    assert_eq!(alt(SpeechPreset::Structure), spec);
    assert_eq!(
        alt(SpeechPreset::AcrobatWindows),
        ("alt text".to_owned(), Some(Substitution::Alt), None, spec.3)
    );
    // pdf.js replaces only figures, so a Span keeps its painted text.
    assert_eq!(
        alt(SpeechPreset::FirefoxPdfJs),
        ("painted one".to_owned(), None, None, None)
    );
}

#[test]
fn expansion_actual_text_private_invisible_and_empty_elements_in_the_structure_preset() {
    let doc = mixed_doc();

    let order = doc.speech_order(SpeechPreset::Structure);

    assert_eq!(
        lines(&order),
        [
            "Span · alt text",
            "Span · World Wide Web Consortium",
            "(private) P · secret",
            "P · fi",
            "P · ocr text",
            "(empty) P · ",
        ]
    );
    let flags: Vec<_> = order
        .items
        .iter()
        .map(|i| (i.substituted_by, i.invisible, i.painted.as_deref()))
        .collect();
    assert_eq!(
        flags,
        [
            (Some(Substitution::Alt), false, Some("painted one")),
            (Some(Substitution::Expansion), false, Some("W3C")),
            (None, false, None),
            // The marked-content /ActualText replaces the painted "x".
            (Some(Substitution::ActualText), false, Some("x")),
            // Invisible text is flagged and still spoken.
            (None, true, None),
            (None, false, None),
        ]
    );
    // Smoke: four of the six are spoken.
    assert_eq!(order.spoken().count(), 4);
}

#[test]
fn the_expansion_is_not_applied_by_pdf_js_which_replaces_nothing_but_figures() {
    let doc = mixed_doc();

    let order = doc.speech_order(SpeechPreset::FirefoxPdfJs);

    assert_eq!(
        spoken_lines(&order),
        ["Span · painted one", "Span · W3C", "P · fi", "P · ocr text"]
    );
}

#[test]
fn chromium_reads_the_untagged_text_of_a_page_where_it_was_painted() {
    let doc = fixture("untagged-content");

    let order = doc.speech_order(SpeechPreset::ChromiumDefault);

    assert_eq!(
        order.items.iter().map(|i| i.text.as_str()).collect::<Vec<_>>(),
        [
            "Header",
            "Tagged one",
            "Untagged text",
            "Tagged twoTagged again",
            "Nested"
        ]
    );
    assert_eq!(
        order.items.iter().map(|i| i.node.clone()).collect::<Vec<_>>(),
        [
            NodeId::Artifact { page: 0, seq: 0 },
            elem(8),
            // Text outside any marked content is addressed by the operator that painted it.
            NodeId::Op { page: 0, ix: 17 },
            elem(9),
            NodeId::Artifact { page: 0, seq: 1 },
        ]
    );
    // Smoke: nothing is skipped, and the tag-following presets leave the untagged text out.
    assert_eq!(order.spoken().count(), 5);
    assert_eq!(
        spoken_lines(&doc.speech_order(SpeechPreset::Structure)),
        ["P · Tagged one", "P · Tagged twoTagged again"]
    );
}

// ---- the speech order as a page's reading order ------------------------------------------------

#[test]
fn a_page_reads_in_speech_order_with_the_covered_content_left_out() {
    let doc = fixture("speech-alt-hides");

    let structure = doc.speech_reading_order(0, &doc.speech_order(SpeechPreset::Structure));

    // The Sect that /Alt replaces is one step; the two paragraphs it covers take none.
    assert_eq!(
        structure
            .steps
            .iter()
            .map(|s| (s.index, s.role.as_str(), s.preview.as_str()))
            .collect::<Vec<_>>(),
        [
            (1, "H1", "Quarterly report"),
            (2, "Sect", "Summary of results"),
            (3, "Figure", "Revenue chart"),
            (4, "Figure", "Cost chart"),
            (5, "P", "Closing remarks follow."),
        ]
    );
    assert_eq!(
        structure.steps.iter().map(|s| s.id.clone()).collect::<Vec<_>>(),
        [elem(8), elem(9), elem(12), elem(13), elem(14)]
    );
    // Smoke: it is a plain top-to-bottom page, so nothing goes back up.
    assert_eq!(structure.backtracks, 0);

    // Chrome reads the paragraphs and not the Alt text, and has no figure steps.
    let chromium = doc.speech_reading_order(0, &doc.speech_order(SpeechPreset::ChromiumDefault));
    assert_eq!(
        chromium.steps.iter().map(|s| s.id.clone()).collect::<Vec<_>>(),
        [elem(8), elem(10), elem(11), elem(14)]
    );
}

#[test]
fn a_page_without_speech_has_no_steps() {
    let doc = fixture("speech-alt-hides");

    let order = doc.speech_reading_order(5, &doc.speech_order(SpeechPreset::Structure));

    assert_eq!(order.steps, []);
    assert_eq!(order.backtracks, 0);
}
