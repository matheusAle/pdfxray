//! `speech-alt-hides`: `/Alt` on a `Sect` that holds two paragraphs, which Acrobat-based readers
//! replace by that text, next to two figures, only one of which pdf.js replaces by its `/Alt`.

use super::common::{HELVETICA, built, page_dict, struct_elem, tagged_text};
use crate::expected::{CatalogFacts, Expected, PageFacts, Sourced, StructElemFacts, StructFacts};
use crate::writer::{PdfVersion, PdfWriter, Trailer};
use crate::{Built, WriterError};

pub(super) fn build() -> Result<Built, WriterError> {
    let content = [
        tagged_text("H1", 0, 18, (40, 360), "Quarterly report"),
        tagged_text("P", 1, 12, (40, 330), "Revenue grew in every region."),
        tagged_text("P", 2, 12, (40, 310), "Costs stayed flat this year."),
        "/Figure << /MCID 3 >> BDC 0.8 0.8 0.9 rg 40 200 100 80 re f EMC".to_owned(),
        "/Figure << /MCID 4 >> BDC 0.9 0.8 0.8 rg 160 200 100 80 re f EMC".to_owned(),
        tagged_text("P", 5, 12, (40, 160), "Closing remarks follow."),
    ]
    .join("\n");
    let mut w = PdfWriter::new(PdfVersion::V1_7);
    w.object(
        1,
        "<< /Type /Catalog /Pages 2 0 R /StructTreeRoot 6 0 R /MarkInfo << /Marked true >> /Lang (en-US) >>",
    );
    w.object(2, "<< /Type /Pages /Kids [3 0 R] /Count 1 >>");
    w.object(3, page_dict("/MediaBox [0 0 300 400] /StructParents 0"));
    w.stream(4, "", content.as_bytes());
    w.object(5, HELVETICA);
    w.object(
        6,
        "<< /Type /StructTreeRoot /K 7 0 R /ParentTree 15 0 R /ParentTreeNextKey 1 >>",
    );
    w.object(
        7,
        "<< /Type /StructElem /S /Document /P 6 0 R /K [8 0 R 9 0 R 12 0 R 13 0 R 14 0 R] >>",
    );
    w.object(8, struct_elem("H1", 7, Some(3), "0"));
    // The section's /Alt stands for both paragraphs below it in Acrobat.
    w.object(
        9,
        "<< /Type /StructElem /S /Sect /P 7 0 R /Alt (Summary of results) /K [10 0 R 11 0 R] >>",
    );
    w.object(10, struct_elem("P", 9, Some(3), "1"));
    w.object(11, struct_elem("P", 9, Some(3), "2"));
    // pdf.js replaces a figure only when it also has a /BBox and starts with content.
    w.object(
        12,
        "<< /Type /StructElem /S /Figure /P 7 0 R /Pg 3 0 R /Alt (Revenue chart) \
         /A << /O /Layout /BBox [40 200 140 280] >> /K 3 >>",
    );
    w.object(
        13,
        "<< /Type /StructElem /S /Figure /P 7 0 R /Pg 3 0 R /Alt (Cost chart) /K 4 >>",
    );
    w.object(14, struct_elem("P", 7, Some(3), "5"));
    w.object(15, "<< /Nums [0 [8 0 R 10 0 R 11 0 R 12 0 R 13 0 R 14 0 R]] >>");
    w.finish_table(&Trailer::new(1))?;
    let out = w.into_output()?;

    let leaf = |object: u32, role: &str, mcid: i32| {
        StructElemFacts::new(object, role, Some(role))
            .page(0)
            .mcids(&[mcid])
    };
    let tree = StructFacts::new(6)
        .child(
            StructElemFacts::new(7, "Document", Some("Document"))
                .child(leaf(8, "H1", 0))
                .child(
                    StructElemFacts::new(9, "Sect", Some("Sect"))
                        .child(leaf(10, "P", 1))
                        .child(leaf(11, "P", 2)),
                )
                .child(leaf(12, "Figure", 3))
                .child(leaf(13, "Figure", 4))
                .child(leaf(14, "P", 5)),
        )
        .parent_tree(0, &[Some(8), Some(10), Some(11), Some(12), Some(13), Some(14)]);
    let expected = Expected::new(
        PdfVersion::V1_7,
        &out.revisions,
        CatalogFacts::new(1).marked(true).lang("en-US"),
    )
    .with_page(PageFacts::new(0, 3, Sourced::own([0.0, 0.0, 300.0, 400.0], 3)).mcids(&[0, 1, 2, 3, 4, 5]))
    .with_struct_tree(tree);
    Ok(built(out, expected))
}
