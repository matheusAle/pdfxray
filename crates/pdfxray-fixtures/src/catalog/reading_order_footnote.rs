//! `reading-order-footnote`: a footnote tagged right after the paragraph that cites it (the
//! placement the PDF Association Best Practice Guide recommends) but drawn at the bottom of the
//! page. A screen reader jumps down to the note and back up to the body, which is a detour and
//! not a backtrack.

use super::common::{HELVETICA, built, page_dict, struct_elem};
use crate::expected::{CatalogFacts, Expected, PageFacts, Sourced, StructElemFacts, StructFacts};
use crate::writer::{PdfVersion, PdfWriter, Trailer};
use crate::{Built, WriterError};

/// Three body paragraphs top to bottom, then the footnote (label and text) at the page bottom, as
/// a layout engine paints it.
const CONTENT: &str = "\
/P << /MCID 0 >> BDC BT /F1 12 Tf 40 340 Td (Revenue grew in every region.) Tj ET EMC
/P << /MCID 1 >> BDC BT /F1 12 Tf 40 310 Td (Costs stayed flat this year.) Tj ET EMC
/P << /MCID 2 >> BDC BT /F1 12 Tf 40 280 Td (Closing remarks follow.) Tj ET EMC
/Lbl << /MCID 3 >> BDC BT /F1 8 Tf 40 60 Td (1) Tj ET EMC
/P << /MCID 4 >> BDC BT /F1 8 Tf 50 60 Td (Source: annual report.) Tj ET EMC";

pub(super) fn build() -> Result<Built, WriterError> {
    let mut w = PdfWriter::new(PdfVersion::V1_7);
    w.object(
        1,
        "<< /Type /Catalog /Pages 2 0 R /StructTreeRoot 6 0 R /MarkInfo << /Marked true >> /Lang (en-US) >>",
    );
    w.object(2, "<< /Type /Pages /Kids [3 0 R] /Count 1 >>");
    w.object(3, page_dict("/MediaBox [0 0 300 400] /StructParents 0"));
    w.stream(4, "", CONTENT.as_bytes());
    w.object(5, HELVETICA);
    w.object(
        6,
        "<< /Type /StructTreeRoot /K 7 0 R /ParentTree 14 0 R /ParentTreeNextKey 1 >>",
    );
    // The note (9) follows the paragraph that cites it (8) in /K, before the next paragraph (12).
    w.object(
        7,
        "<< /Type /StructElem /S /Document /P 6 0 R /K [8 0 R 9 0 R 12 0 R 13 0 R] >>",
    );
    w.object(8, struct_elem("P", 7, Some(3), "0"));
    w.object(
        9,
        "<< /Type /StructElem /S /Note /P 7 0 R /Pg 3 0 R /K [10 0 R 11 0 R] >>",
    );
    w.object(10, struct_elem("Lbl", 9, Some(3), "3"));
    w.object(11, struct_elem("P", 9, Some(3), "4"));
    w.object(12, struct_elem("P", 7, Some(3), "1"));
    w.object(13, struct_elem("P", 7, Some(3), "2"));
    w.object(14, "<< /Nums [0 [8 0 R 12 0 R 13 0 R 10 0 R 11 0 R]] >>");
    w.finish_table(&Trailer::new(1))?;
    let out = w.into_output()?;

    let tree = StructFacts::new(6)
        .child(
            StructElemFacts::new(7, "Document", Some("Document"))
                .child(StructElemFacts::new(8, "P", Some("P")).page(0).mcids(&[0]))
                .child(
                    StructElemFacts::new(9, "Note", Some("Note"))
                        .page(0)
                        .child(StructElemFacts::new(10, "Lbl", Some("Lbl")).page(0).mcids(&[3]))
                        .child(StructElemFacts::new(11, "P", Some("P")).page(0).mcids(&[4])),
                )
                .child(StructElemFacts::new(12, "P", Some("P")).page(0).mcids(&[1]))
                .child(StructElemFacts::new(13, "P", Some("P")).page(0).mcids(&[2])),
        )
        .parent_tree(0, &[Some(8), Some(12), Some(13), Some(10), Some(11)]);
    let expected = Expected::new(
        PdfVersion::V1_7,
        &out.revisions,
        CatalogFacts::new(1).marked(true).lang("en-US"),
    )
    .with_page(PageFacts::new(0, 3, Sourced::own([0.0, 0.0, 300.0, 400.0], 3)).mcids(&[0, 1, 2, 3, 4]))
    .with_struct_tree(tree);
    Ok(built(out, expected))
}
