//! `form-xobject-nesting`: a page paints Form A which paints Form B, with tagging in all three.

use super::common::{HELVETICA, built, struct_elem};
use crate::expected::{CatalogFacts, Expected, FormFacts, PageFacts, Sourced, StructElemFacts, StructFacts};
use crate::writer::{PdfVersion, PdfWriter, Trailer};
use crate::{Built, WriterError};

const PAGE_CONTENT: &str = "\
/P << /MCID 0 >> BDC BT /F1 14 Tf 10 280 Td (Page text) Tj ET EMC
/FmA Do";

const FORM_A_CONTENT: &str = "\
/P << /MCID 0 >> BDC BT /F1 12 Tf 10 220 Td (Form A) Tj ET EMC
/FmB Do";

const FORM_B_CONTENT: &str = "\
/P << /MCID 0 >> BDC 0.8 1 0.8 rg 0 0 100 40 re f BT 0 g /F1 10 Tf 5 15 Td (Form B) Tj ET EMC";

pub(super) fn build() -> Result<Built, WriterError> {
    let mut w = PdfWriter::new(PdfVersion::V1_7);
    w.object(
        1,
        "<< /Type /Catalog /Pages 2 0 R /StructTreeRoot 8 0 R /MarkInfo << /Marked true >> /Lang (en-US) >>",
    );
    w.object(2, "<< /Type /Pages /Kids [3 0 R] /Count 1 >>");
    w.object(
        3,
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 300 300] /StructParents 0 \
         /Resources << /Font << /F1 5 0 R >> /XObject << /FmA 6 0 R >> >> /Contents 4 0 R >>",
    );
    w.stream(4, "", PAGE_CONTENT.as_bytes());
    w.object(5, HELVETICA);
    w.stream(
        6,
        "/Type /XObject /Subtype /Form /BBox [0 0 250 250] /Matrix [0.8 0 0 0.8 20 20] /StructParents 1 \
         /Resources << /Font << /F1 5 0 R >> /XObject << /FmB 7 0 R >> >>",
        FORM_A_CONTENT.as_bytes(),
    );
    w.stream(
        7,
        "/Type /XObject /Subtype /Form /BBox [0 0 100 100] /Matrix [1 0 0 1 10 10] /StructParents 2 \
         /Resources << /Font << /F1 5 0 R >> >>",
        FORM_B_CONTENT.as_bytes(),
    );
    w.object(
        8,
        "<< /Type /StructTreeRoot /K 9 0 R /ParentTree 13 0 R /ParentTreeNextKey 3 >>",
    );
    w.object(
        9,
        "<< /Type /StructElem /S /Document /P 8 0 R /K [10 0 R 11 0 R 12 0 R] >>",
    );
    w.object(10, struct_elem("P", 9, Some(3), "0"));
    w.object(
        11,
        struct_elem("P", 9, Some(3), "<< /Type /MCR /Pg 3 0 R /Stm 6 0 R /MCID 0 >>"),
    );
    w.object(
        12,
        struct_elem("P", 9, Some(3), "<< /Type /MCR /Pg 3 0 R /Stm 7 0 R /MCID 0 >>"),
    );
    w.object(13, "<< /Nums [0 [10 0 R] 1 [11 0 R] 2 [12 0 R]] >>");
    w.finish_table(&Trailer::new(1))?;
    let out = w.into_output()?;

    let tree = StructFacts::new(8)
        .child(
            StructElemFacts::new(9, "Document", Some("Document"))
                .child(StructElemFacts::new(10, "P", Some("P")).page(0).mcids(&[0]))
                .child(
                    StructElemFacts::new(11, "P", Some("P"))
                        .page(0)
                        .stream(6)
                        .mcids(&[0]),
                )
                .child(
                    StructElemFacts::new(12, "P", Some("P"))
                        .page(0)
                        .stream(7)
                        .mcids(&[0]),
                ),
        )
        .parent_tree(0, &[Some(10)])
        .parent_tree(1, &[Some(11)])
        .parent_tree(2, &[Some(12)]);
    let expected = Expected::new(
        PdfVersion::V1_7,
        &out.revisions,
        CatalogFacts::new(1).marked(true).lang("en-US"),
    )
    .with_page(PageFacts::new(0, 3, Sourced::own([0.0, 0.0, 300.0, 300.0], 3)).mcids(&[0]))
    .with_struct_tree(tree)
    .with_form(FormFacts {
        object: 6,
        name: "FmA".to_owned(),
        painted_by: 3,
        matrix: Some([0.8, 0.0, 0.0, 0.8, 20.0, 20.0]),
        mcids: vec![0],
    })
    .with_form(FormFacts {
        object: 7,
        name: "FmB".to_owned(),
        painted_by: 6,
        matrix: Some([1.0, 0.0, 0.0, 1.0, 10.0, 10.0]),
        mcids: vec![0],
    });
    Ok(built(out, expected))
}
