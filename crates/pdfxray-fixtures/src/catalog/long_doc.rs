//! `long-doc`: 40 pages, each with a heading and three paragraphs, 201 structure elements.

use super::common::{HELVETICA, built};
use crate::expected::{CatalogFacts, Expected, PageFacts, Sourced, StructElemFacts, StructFacts};
use crate::writer::{PdfVersion, PdfWriter, Trailer};
use crate::{Built, WriterError};

const PAGES: u32 = 40;
/// Objects per page: page, content, Sect, H1, P, P, P.
const PER_PAGE: u32 = 7;
/// First object number of page 0; objects 1-6 are catalog, pages, font, tree root, Document, parents.
const FIRST: u32 = 7;

const fn page_obj(i: u32) -> u32 {
    FIRST + PER_PAGE * i
}

fn content(i: u32) -> String {
    let n = i + 1;
    format!(
        "/H1 << /MCID 0 >> BDC BT /F1 18 Tf 36 740 Td (Chapter {n}) Tj ET EMC\n\
         /P << /MCID 1 >> BDC BT /F1 11 Tf 36 700 Td (First paragraph of chapter {n}) Tj ET EMC\n\
         /P << /MCID 2 >> BDC BT /F1 11 Tf 36 680 Td (Second paragraph of chapter {n}) Tj ET EMC\n\
         /P << /MCID 3 >> BDC BT /F1 11 Tf 36 660 Td (Third paragraph of chapter {n}) Tj ET EMC"
    )
}

pub(super) fn build() -> Result<Built, WriterError> {
    let kids = (0..PAGES)
        .map(|i| format!("{} 0 R", page_obj(i)))
        .collect::<Vec<_>>()
        .join(" ");
    let sects = (0..PAGES)
        .map(|i| format!("{} 0 R", page_obj(i) + 2))
        .collect::<Vec<_>>()
        .join(" ");

    let mut w = PdfWriter::new(PdfVersion::V1_7);
    w.object(
        1,
        "<< /Type /Catalog /Pages 2 0 R /StructTreeRoot 4 0 R /MarkInfo << /Marked true >> /Lang (en-US) >>",
    );
    w.object(
        2,
        format!(
            "<< /Type /Pages /Kids [{kids}] /Count {PAGES} /MediaBox [0 0 612 792] \
             /Resources << /Font << /F1 3 0 R >> >> >>"
        ),
    );
    w.object(3, HELVETICA);
    w.object(
        4,
        format!("<< /Type /StructTreeRoot /K 5 0 R /ParentTree 6 0 R /ParentTreeNextKey {PAGES} >>"),
    );
    w.object(
        5,
        format!("<< /Type /StructElem /S /Document /P 4 0 R /K [{sects}] >>"),
    );
    let nums = (0..PAGES)
        .map(|i| {
            let base = page_obj(i);
            format!(
                "{i} [{} 0 R {} 0 R {} 0 R {} 0 R]",
                base + 3,
                base + 4,
                base + 5,
                base + 6
            )
        })
        .collect::<Vec<_>>()
        .join(" ");
    w.object(6, format!("<< /Nums [{nums}] >>"));

    let mut sect_facts = Vec::new();
    for i in 0..PAGES {
        let base = page_obj(i);
        w.object(
            base,
            format!(
                "<< /Type /Page /Parent 2 0 R /StructParents {i} /Contents {} 0 R >>",
                base + 1
            ),
        );
        w.stream(base + 1, "", content(i).as_bytes());
        w.object(
            base + 2,
            format!(
                "<< /Type /StructElem /S /Sect /P 5 0 R /K [{} 0 R {} 0 R {} 0 R {} 0 R] >>",
                base + 3,
                base + 4,
                base + 5,
                base + 6
            ),
        );
        let mut sect = StructElemFacts::new(base + 2, "Sect", Some("Sect"));
        for (offset, (role, mcid)) in [("H1", 0), ("P", 1), ("P", 2), ("P", 3)].into_iter().enumerate() {
            let obj = base + 3 + u32::try_from(offset).unwrap_or_default();
            w.object(
                obj,
                format!(
                    "<< /Type /StructElem /S /{role} /P {} 0 R /Pg {base} 0 R /K {mcid} >>",
                    base + 2
                ),
            );
            sect = sect.child(StructElemFacts::new(obj, role, Some(role)).page(i).mcids(&[mcid]));
        }
        sect_facts.push(sect);
    }
    w.finish_table(&Trailer::new(1))?;
    let out = w.into_output()?;

    let mut document = StructElemFacts::new(5, "Document", Some("Document"));
    let mut tree = StructFacts::new(4);
    let mut expected = Expected::new(
        PdfVersion::V1_7,
        &out.revisions,
        CatalogFacts::new(1).marked(true).lang("en-US"),
    );
    for (i, sect) in (0..PAGES).zip(sect_facts) {
        let base = page_obj(i);
        document = document.child(sect);
        tree = tree.parent_tree(
            i32::try_from(i).unwrap_or_default(),
            &[Some(base + 3), Some(base + 4), Some(base + 5), Some(base + 6)],
        );
        expected = expected.with_page(
            PageFacts::new(i, base, Sourced::inherited([0.0, 0.0, 612.0, 792.0], 2)).mcids(&[0, 1, 2, 3]),
        );
    }
    expected = expected.with_struct_tree(tree.child(document));
    Ok(built(out, expected))
}
