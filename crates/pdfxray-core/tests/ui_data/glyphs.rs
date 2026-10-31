//! Glyphs.

use pretty_assertions::assert_eq;

use super::*;

#[test]
fn the_h_of_the_minimal_page_is_a_helvetica_glyph_without_embedding() {
    let doc = open("minimal-one-page");
    let tj = first_op(&doc, "Tj");

    let info = doc.glyph_info(&glyph(tj, 0)).unwrap();

    assert_eq!(info.op, NodeId::Op { page: 0, ix: tj });
    assert_eq!(
        (
            info.char_code.value,
            info.char_code.hex.as_str(),
            info.char_code.bytes,
            info.char_code.known
        ),
        (72, "48", 1, true)
    );
    assert_eq!(
        (
            info.unicode.as_deref(),
            info.code_points.as_slice(),
            info.unicode_source
        ),
        (Some("H"), [72_u32].as_slice(), UnicodeSource::Encoding)
    );
    assert_eq!(info.actual_text, None);
    assert_eq!((info.width_units, info.width_from_font), (722.0, false));
    assert!(near(info.width_pt, 12.996), "{}", info.width_pt);
    assert!(
        near(info.advance[0], 12.996) && info.advance[1].abs() < 1e-9,
        "{:?}",
        info.advance
    );
    assert_eq!((info.origin, info.font_size), (Point::new(20.0, 40.0), 18.0));
    let font = info.font.unwrap();
    assert_eq!(
        (
            font.resource.as_deref(),
            font.obj,
            font.base_font.as_deref(),
            font.subtype.as_deref(),
            font.embedded,
            font.font_file.as_deref(),
            font.subset_prefix.as_deref(),
            font.has_to_unicode,
        ),
        (
            Some("F1"),
            Some(r(5)),
            Some("Helvetica"),
            Some("Type1"),
            false,
            None,
            None,
            false
        )
    );
    assert_eq!((font.ascent_em, font.descent_em), (Some(0.718), Some(-0.207)));
    assert_eq!(font.encoding, None);
    assert_eq!(
        doc.glyph_info(&glyph(tj, 13)),
        None,
        "the string has 13 glyphs, numbered 0 to 12"
    );
    assert_eq!(
        doc.glyph_info(&NodeId::Op { page: 0, ix: tj }),
        None,
        "only glyph nodes have glyph facts"
    );
}

#[test]
fn horizontal_scaling_changes_the_advance_but_not_the_font_width() {
    let doc = open("text-state");
    let tj = doc
        .captured(0)
        .unwrap()
        .ops
        .iter()
        .find(|o| o.operands.first().is_some_and(|s| s == "(Spaced out words)"))
        .unwrap()
        .ix;

    let s = doc.glyph_info(&glyph(tj, 0)).unwrap();
    let p = doc.glyph_info(&glyph(tj, 1)).unwrap();

    // Times-Roman 18 pt at Tz 150: 556/1000 * 18 * 1.5 = 15.012.
    assert_eq!((s.unicode.as_deref(), s.width_units), (Some("S"), 556.0));
    assert!(
        near(s.width_pt, 15.012) && near(s.advance[0], 15.012),
        "{} {:?}",
        s.width_pt,
        s.advance
    );
    assert_eq!((p.unicode.as_deref(), p.width_units), (Some("p"), 500.0));
    assert!(near(p.width_pt, 13.5), "{}", p.width_pt);
    assert_eq!(
        s.font.as_ref().and_then(|f| f.base_font.as_deref()),
        Some("Times-Roman")
    );
    assert!(
        s.advance[0] > s.width_units / 1000.0 * s.font_size,
        "Tz 150 widens the advance"
    );
}

#[test]
fn a_type_3_glyph_takes_its_width_from_the_widths_array() {
    let doc = open("type3-font");
    let tj = first_op(&doc, "Tj");

    let a = doc.glyph_info(&glyph(tj, 0)).unwrap();
    let b = doc.glyph_info(&glyph(tj, 1)).unwrap();

    assert_eq!(
        (a.char_code.value, a.width_units, a.width_from_font),
        (97, 1000.0, true)
    );
    assert_eq!(
        (b.char_code.value, b.width_units, b.width_from_font),
        (98, 800.0, true)
    );
    assert!(near(a.width_pt, 20.0) && near(b.width_pt, 16.0));
    assert_eq!((a.unicode.clone(), a.unicode_source), (None, UnicodeSource::None));
    let font = a.font.unwrap();
    assert_eq!(
        (
            font.subtype.as_deref(),
            font.embedded,
            font.base_font.as_deref(),
            font.resource.as_deref()
        ),
        (Some("Type3"), false, None, Some("T3"))
    );
    assert_eq!(font.encoding.as_deref(), Some("custom (2 differences)"));
    assert_eq!((font.ascent_em, font.descent_em), (Some(1.0), Some(0.0)));
    assert_eq!(font.font_bbox, Some([0.0, 0.0, 1000.0, 1000.0]));
    assert!(
        near(b.advance[0], b.width_pt),
        "{:?} vs {}",
        b.advance,
        b.width_pt
    );
}

#[test]
fn glyph_facts_distinguish_embedded_fonts_with_and_without_a_tounicode_map() {
    let with = Document::open(third_party("PDF_UA-1/7.3 Graphics/7.3-t01-pass-a.pdf")).unwrap();
    let without = Document::open(third_party(
        "ISO 32000-1/veraPDF test suite 6-8-3-3-t01-fail-a.pdf",
    ))
    .unwrap();
    let first_glyph = |doc: &Document| {
        let it = doc
            .captured(0)
            .unwrap()
            .items
            .iter()
            .find_map(|i| match i.kind {
                PaintKind::Glyph { glyph_index, .. } => Some((i.op, glyph_index)),
                _ => None,
            })
            .unwrap();
        doc.glyph_info(&glyph(it.0, it.1)).unwrap()
    };

    let mapped = first_glyph(&with);
    let encoded = first_glyph(&without);

    assert_eq!(
        (
            mapped.char_code.hex.as_str(),
            mapped.unicode.as_deref(),
            mapped.unicode_source
        ),
        ("01", Some("A"), UnicodeSource::ToUnicode),
        "the code 1 means nothing by itself; the ToUnicode map says it is an A"
    );
    let font = mapped.font.as_ref().unwrap();
    assert_eq!(
        (
            font.base_font.as_deref(),
            font.subset_prefix.as_deref(),
            font.font_file.as_deref(),
            font.font_file_obj,
            font.has_to_unicode,
            font.resource.as_deref(),
        ),
        (
            Some("BAAAAA+LiberationSans-Bold"),
            Some("BAAAAA"),
            Some("FontFile2"),
            Some(r(33)),
            true,
            Some("TT0"),
        )
    );
    assert_eq!((mapped.width_units, mapped.width_from_font), (722.0, true));
    assert_eq!((font.ascent_em, font.descent_em), (Some(0.905), Some(-0.211)));
    assert_eq!(font.font_bbox, Some([-481.0, -376.0, 1304.0, 1034.0]));

    let plain = encoded.font.as_ref().unwrap();
    assert_eq!(
        (
            encoded.unicode.as_deref(),
            encoded.unicode_source,
            encoded.width_units
        ),
        (Some("H"), UnicodeSource::Encoding, 600.0)
    );
    assert_eq!(
        (
            plain.base_font.as_deref(),
            plain.embedded,
            plain.has_to_unicode,
            plain.encoding.as_deref()
        ),
        (Some("TMJTIB+FreeMonoBold"), true, false, Some("/WinAnsiEncoding"))
    );
    assert_ne!(mapped.unicode_source, encoded.unicode_source);
}

#[test]
fn a_composite_font_reads_two_byte_codes_widths_and_actual_text_from_its_context() {
    let doc = single_page(
        "/MediaBox [0 0 200 100] /Resources << /Font << /F1 5 0 R >> >>",
        concat!(
            "/Span << /ActualText (Outer) >> BDC /Span << /MCID 1 >> BDC BT /F1 20 Tf 10 40 Td <00410042> Tj ET EMC EMC ",
            "/Span << /ActualText (Own) >> BDC BT /F1 20 Tf 10 10 Td <0041> Tj ET EMC",
        ),
        |w| {
            w.object(5, "<< /Type /Font /Subtype /Type0 /BaseFont /ABCDEF+Demo-Bold /Encoding /Identity-H /DescendantFonts [6 0 R] >>");
            w.object(6, "<< /Type /Font /Subtype /CIDFontType2 /BaseFont /ABCDEF+Demo-Bold /FontDescriptor 7 0 R /DW 500 /W [65 [722 700]] >>");
            w.object(7, "<< /Type /FontDescriptor /FontName /ABCDEF+Demo-Bold /Flags 4 /FontBBox [-12 -270 734 890] /Ascent 890 /Descent -270 /FontFile2 8 0 R >>");
            w.stream(8, "/Length1 4", b"abcd");
        },
    );
    let tj: Vec<u32> = doc
        .captured(0)
        .unwrap()
        .ops
        .iter()
        .filter(|o| o.operator == "Tj")
        .map(|o| o.ix)
        .collect();

    let inherited = doc.glyph_info(&glyph(tj[0], 1)).unwrap();
    let own = doc.glyph_info(&glyph(tj[1], 0)).unwrap();

    assert_eq!(
        (
            inherited.char_code.hex.as_str(),
            inherited.char_code.bytes,
            inherited.char_code.value
        ),
        ("0042", 2, 66)
    );
    assert_eq!(
        (inherited.width_units, inherited.width_from_font),
        (700.0, true),
        "/W gives CID 66 a width of 700"
    );
    assert_eq!(
        inherited
            .actual_text
            .as_ref()
            .map(|a| (a.text.as_str(), a.inherited)),
        Some(("Outer", true)),
        "the MCID span in between carries no ActualText, the outer one applies"
    );
    assert_eq!(
        own.actual_text.as_ref().map(|a| (a.text.as_str(), a.inherited)),
        Some(("Own", false))
    );
    assert_eq!(
        (own.unicode.clone(), own.unicode_source),
        (None, UnicodeSource::None)
    );
    let font: FontFacts = own.font.unwrap();
    assert_eq!(
        (
            font.subtype.as_deref(),
            font.cid_subtype.as_deref(),
            font.encoding.as_deref(),
            font.embedded
        ),
        (Some("Type0"), Some("CIDFontType2"), Some("/Identity-H"), true)
    );
    assert_eq!(
        (font.font_file.as_deref(), font.subset_prefix.as_deref()),
        (Some("FontFile2"), Some("ABCDEF"))
    );
    assert_eq!((font.ascent_em, font.descent_em), (Some(0.89), Some(-0.27)));
    assert_eq!(font.width_units(65), Some(722.0));
    assert_eq!(font.width_units(900), Some(500.0), "codes outside /W use /DW");
}
