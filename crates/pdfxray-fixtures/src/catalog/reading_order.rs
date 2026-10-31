//! `reading-order`: a structure order that disagrees with the page: the caption is tagged before
//! the figure it sits under, and the heading uses a custom role mapped to `H1`.

use super::common::{HELVETICA, built, page_dict, struct_elem};
use crate::expected::{CatalogFacts, Expected, PageFacts, Sourced, StructElemFacts, StructFacts};
use crate::writer::{PdfVersion, PdfWriter, Trailer};
use crate::{Built, WriterError};

/// Painted top to bottom: heading (MCID 0), paragraph (1), the figure's grey square (2), the
/// caption under it (3) and a closing paragraph (4).
const CONTENT: &str = "\
/Heading1 << /MCID 0 >> BDC BT /F1 24 Tf 40 360 Td (Quarterly report) Tj ET EMC
/P << /MCID 1 >> BDC BT /F1 12 Tf 40 330 Td (Revenue grew in every region.) Tj ET EMC
/Figure << /MCID 2 >> BDC 0.8 g 40 200 100 100 re f 0 g EMC
/Caption << /MCID 3 >> BDC BT /F1 10 Tf 40 180 Td (Figure 1: Revenue by region) Tj ET EMC
/P << /MCID 4 >> BDC BT /F1 12 Tf 40 140 Td (Closing remarks follow.) Tj ET EMC";

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
        "<< /Type /StructTreeRoot /K 7 0 R /ParentTree 13 0 R /ParentTreeNextKey 1 \
         /RoleMap << /Heading1 /H1 >> >>",
    );
    // The caption (10) comes before the figure (11) in /K, although it is painted after it and
    // sits below it.
    w.object(
        7,
        "<< /Type /StructElem /S /Document /P 6 0 R /K [8 0 R 9 0 R 10 0 R 11 0 R 12 0 R] >>",
    );
    w.object(8, struct_elem("Heading1", 7, Some(3), "0"));
    w.object(9, struct_elem("P", 7, Some(3), "1"));
    w.object(10, struct_elem("Caption", 7, Some(3), "3"));
    w.object(
        11,
        "<< /Type /StructElem /S /Figure /P 7 0 R /Pg 3 0 R /Alt (Grey square) /K 2 >>",
    );
    w.object(12, struct_elem("P", 7, Some(3), "4"));
    w.object(13, "<< /Nums [0 [8 0 R 9 0 R 11 0 R 10 0 R 12 0 R]] >>");
    w.finish_table(&Trailer::new(1))?;
    let out = w.into_output()?;

    let tree = StructFacts::new(6)
        .role_map(&[("Heading1", "H1")])
        .child(
            StructElemFacts::new(7, "Document", Some("Document"))
                .child(
                    StructElemFacts::new(8, "Heading1", Some("H1"))
                        .page(0)
                        .mcids(&[0]),
                )
                .child(StructElemFacts::new(9, "P", Some("P")).page(0).mcids(&[1]))
                .child(
                    StructElemFacts::new(10, "Caption", Some("Caption"))
                        .page(0)
                        .mcids(&[3]),
                )
                .child(
                    StructElemFacts::new(11, "Figure", Some("Figure"))
                        .page(0)
                        .mcids(&[2]),
                )
                .child(StructElemFacts::new(12, "P", Some("P")).page(0).mcids(&[4])),
        )
        .parent_tree(0, &[Some(8), Some(9), Some(11), Some(10), Some(12)]);
    let expected = Expected::new(
        PdfVersion::V1_7,
        &out.revisions,
        CatalogFacts::new(1).marked(true).lang("en-US"),
    )
    .with_page(PageFacts::new(0, 3, Sourced::own([0.0, 0.0, 300.0, 400.0], 3)).mcids(&[0, 1, 2, 3, 4]))
    .with_struct_tree(tree);
    Ok(built(out, expected))
}
