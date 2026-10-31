//! `rolemap-custom`: custom structure roles, resolved through `/RoleMap`, and a cycle.

use super::common::{HELVETICA, built, page_dict, struct_elem};
use crate::expected::{CatalogFacts, Defect, Expected, PageFacts, Sourced, StructElemFacts, StructFacts};
use crate::writer::{PdfVersion, PdfWriter, Trailer};
use crate::{Built, WriterError};

const CONTENT: &str = "\
/Heading1 << /MCID 0 >> BDC BT /F1 24 Tf 20 150 Td (Custom heading) Tj ET EMC
/MyPara << /MCID 1 >> BDC BT /F1 12 Tf 20 120 Td (Custom paragraph) Tj ET EMC
/LoopA << /MCID 2 >> BDC BT /F1 12 Tf 20 90 Td (Role in a cycle) Tj ET EMC";

pub(super) fn build() -> Result<Built, WriterError> {
    let mut w = PdfWriter::new(PdfVersion::V1_7);
    w.object(
        1,
        "<< /Type /Catalog /Pages 2 0 R /StructTreeRoot 6 0 R /MarkInfo << /Marked true >> /Lang (en-US) >>",
    );
    w.object(2, "<< /Type /Pages /Kids [3 0 R] /Count 1 >>");
    w.object(3, page_dict("/MediaBox [0 0 200 200] /StructParents 0"));
    w.stream(4, "", CONTENT.as_bytes());
    w.object(5, HELVETICA);
    w.object(
        6,
        "<< /Type /StructTreeRoot /K 7 0 R /ParentTree 11 0 R /ParentTreeNextKey 1 \
         /RoleMap << /Heading1 /H1 /MyPara /P /LoopA /LoopB /LoopB /LoopA >> >>",
    );
    w.object(
        7,
        "<< /Type /StructElem /S /Document /P 6 0 R /K [8 0 R 9 0 R 10 0 R] >>",
    );
    w.object(8, struct_elem("Heading1", 7, Some(3), "0"));
    w.object(9, struct_elem("MyPara", 7, Some(3), "1"));
    w.object(10, struct_elem("LoopA", 7, Some(3), "2"));
    w.object(11, "<< /Nums [0 [8 0 R 9 0 R 10 0 R]] >>");
    w.finish_table(&Trailer::new(1))?;
    let out = w.into_output()?;

    let tree = StructFacts::new(6)
        .role_map(&[
            ("Heading1", "H1"),
            ("MyPara", "P"),
            ("LoopA", "LoopB"),
            ("LoopB", "LoopA"),
        ])
        .child(
            StructElemFacts::new(7, "Document", Some("Document"))
                .child(
                    StructElemFacts::new(8, "Heading1", Some("H1"))
                        .page(0)
                        .mcids(&[0]),
                )
                .child(StructElemFacts::new(9, "MyPara", Some("P")).page(0).mcids(&[1]))
                .child(StructElemFacts::new(10, "LoopA", None).page(0).mcids(&[2])),
        )
        .parent_tree(0, &[Some(8), Some(9), Some(10)]);
    let expected = Expected::new(
        PdfVersion::V1_7,
        &out.revisions,
        CatalogFacts::new(1).marked(true).lang("en-US"),
    )
    .with_page(PageFacts::new(0, 3, Sourced::own([0.0, 0.0, 200.0, 200.0], 3)).mcids(&[0, 1, 2]))
    .with_struct_tree(tree)
    .with_defect(Defect::RoleMapCycle {
        cycle: vec!["LoopA".to_owned(), "LoopB".to_owned()],
    });
    Ok(built(out, expected))
}
