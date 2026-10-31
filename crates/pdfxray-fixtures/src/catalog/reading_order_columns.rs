//! `reading-order-columns`: a two-column page tagged column by column. Going from the bottom of
//! the first column to the top of the second climbs the page, but it is the normal way to read
//! columns, so it is a column break and not a backtrack.

use super::common::{HELVETICA, built, page_dict, struct_elem};
use crate::expected::{CatalogFacts, Expected, PageFacts, Sourced, StructElemFacts, StructFacts};
use crate::writer::{PdfVersion, PdfWriter, Trailer};
use crate::{Built, WriterError};

/// A heading, two lines in each of two columns (x 40 and x 170), and a closing line under both.
const CONTENT: &str = "\
/H1 << /MCID 0 >> BDC BT /F1 18 Tf 40 360 Td (Two columns) Tj ET EMC
/P << /MCID 1 >> BDC BT /F1 10 Tf 40 320 Td (Column one starts here.) Tj ET EMC
/P << /MCID 2 >> BDC BT /F1 10 Tf 40 300 Td (and goes on below.) Tj ET EMC
/P << /MCID 3 >> BDC BT /F1 10 Tf 170 320 Td (Column two follows.) Tj ET EMC
/P << /MCID 4 >> BDC BT /F1 10 Tf 170 300 Td (and ends here.) Tj ET EMC
/P << /MCID 5 >> BDC BT /F1 10 Tf 40 200 Td (End of the page.) Tj ET EMC";

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
    w.object(
        7,
        "<< /Type /StructElem /S /Document /P 6 0 R /K [8 0 R 9 0 R 10 0 R 11 0 R 12 0 R 13 0 R] >>",
    );
    w.object(8, struct_elem("H1", 7, Some(3), "0"));
    for (obj, mcid) in (9..=13).zip(1..) {
        w.object(obj, struct_elem("P", 7, Some(3), &mcid.to_string()));
    }
    w.object(14, "<< /Nums [0 [8 0 R 9 0 R 10 0 R 11 0 R 12 0 R 13 0 R]] >>");
    w.finish_table(&Trailer::new(1))?;
    let out = w.into_output()?;

    let mut document = StructElemFacts::new(7, "Document", Some("Document"))
        .child(StructElemFacts::new(8, "H1", Some("H1")).page(0).mcids(&[0]));
    for (obj, mcid) in (9..=13).zip(1..) {
        document = document.child(StructElemFacts::new(obj, "P", Some("P")).page(0).mcids(&[mcid]));
    }
    let tree = StructFacts::new(6)
        .child(document)
        .parent_tree(0, &[Some(8), Some(9), Some(10), Some(11), Some(12), Some(13)]);
    let expected = Expected::new(
        PdfVersion::V1_7,
        &out.revisions,
        CatalogFacts::new(1).marked(true).lang("en-US"),
    )
    .with_page(PageFacts::new(0, 3, Sourced::own([0.0, 0.0, 300.0, 400.0], 3)).mcids(&[0, 1, 2, 3, 4, 5]))
    .with_struct_tree(tree);
    Ok(built(out, expected))
}
