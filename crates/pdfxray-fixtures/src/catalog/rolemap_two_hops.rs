//! `rolemap-two-hops`: a heading role that reaches `H1` only through two `/RoleMap` steps
//! (`Headline` to `Heading1` to `H1`) beside a paragraph role that needs one. Acrobat follows the
//! whole chain; pdf.js and `PDFium` follow one step.

use super::common::{HELVETICA, built, page_dict, struct_elem, tagged_text};
use crate::expected::{CatalogFacts, Expected, PageFacts, Sourced, StructElemFacts, StructFacts};
use crate::writer::{PdfVersion, PdfWriter, Trailer};
use crate::{Built, WriterError};

pub(super) fn build() -> Result<Built, WriterError> {
    let content = [
        tagged_text("Headline", 0, 18, (40, 360), "Annual results"),
        tagged_text("Para", 1, 12, (40, 330), "Revenue grew in every region."),
        tagged_text("P", 2, 12, (40, 300), "Closing remarks follow."),
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
        "<< /Type /StructTreeRoot /K 7 0 R /ParentTree 11 0 R /ParentTreeNextKey 1 \
         /RoleMap << /Headline /Heading1 /Heading1 /H1 /Para /P >> >>",
    );
    w.object(
        7,
        "<< /Type /StructElem /S /Document /P 6 0 R /K [8 0 R 9 0 R 10 0 R] >>",
    );
    w.object(8, struct_elem("Headline", 7, Some(3), "0"));
    w.object(9, struct_elem("Para", 7, Some(3), "1"));
    w.object(10, struct_elem("P", 7, Some(3), "2"));
    w.object(11, "<< /Nums [0 [8 0 R 9 0 R 10 0 R]] >>");
    w.finish_table(&Trailer::new(1))?;
    let out = w.into_output()?;

    let tree = StructFacts::new(6)
        .role_map(&[("Headline", "Heading1"), ("Heading1", "H1"), ("Para", "P")])
        .child(
            StructElemFacts::new(7, "Document", Some("Document"))
                .child(
                    StructElemFacts::new(8, "Headline", Some("H1"))
                        .page(0)
                        .mcids(&[0]),
                )
                .child(StructElemFacts::new(9, "Para", Some("P")).page(0).mcids(&[1]))
                .child(StructElemFacts::new(10, "P", Some("P")).page(0).mcids(&[2])),
        )
        .parent_tree(0, &[Some(8), Some(9), Some(10)]);
    let expected = Expected::new(
        PdfVersion::V1_7,
        &out.revisions,
        CatalogFacts::new(1).marked(true).lang("en-US"),
    )
    .with_page(PageFacts::new(0, 3, Sourced::own([0.0, 0.0, 300.0, 400.0], 3)).mcids(&[0, 1, 2]))
    .with_struct_tree(tree);
    Ok(built(out, expected))
}
