//! `resources-inheritance`: a `/Resources` dictionary inherited from `/Pages`, one page that
//! overrides it, a font shared by several scopes, forms with and without their own resources and
//! resources nobody uses.

use super::common::{HELVETICA, built, type1_font};
use crate::expected::{
    CatalogFacts, Expected, FormFacts, PageFacts, ResourceEntryFacts, ResourceScopeFacts,
    SharedResourceFacts, Sourced,
};
use crate::writer::{PdfVersion, PdfWriter, Trailer};
use crate::{Built, WriterError};

/// Page A uses `F1` twice, `F2` once, `GS1` once and paints both forms.
const PAGE_A: &str = "\
BT /F1 14 Tf 20 160 Td (Page A Helvetica) Tj ET
BT /F2 14 Tf 20 140 Td (Page A Times) Tj ET
BT /F1 10 Tf 20 120 Td (second Helvetica) Tj ET
/GS1 gs
0.9 0.9 1 rg 150 100 40 40 re f
/Fm1 Do
/Fm2 Do";

const PAGE_B: &str = "BT /F1 14 Tf 20 100 Td (Page B overrides) Tj ET";
const PAGE_C: &str = "BT /F1 14 Tf 20 100 Td (Page C inherits) Tj ET";
const FORM_1: &str = "BT /F1 10 Tf 5 15 Td (Form one) Tj ET";
const FORM_2: &str = "BT /F2 10 Tf 5 15 Td (Form two) Tj ET";

pub(super) fn build() -> Result<Built, WriterError> {
    let mut w = PdfWriter::new(PdfVersion::V1_7);
    w.object(1, "<< /Type /Catalog /Pages 2 0 R >>");
    w.object(
        2,
        "<< /Type /Pages /Kids [3 0 R 4 0 R 5 0 R] /Count 3 /MediaBox [0 0 200 200] \
         /Resources << /Font << /F1 9 0 R /F2 10 0 R /F3 11 0 R >> \
         /ExtGState << /GS1 15 0 R /GS9 16 0 R >> /XObject << /Fm1 12 0 R /Fm2 13 0 R >> >> >>",
    );
    w.object(3, "<< /Type /Page /Parent 2 0 R /Contents 6 0 R >>");
    w.object(
        4,
        "<< /Type /Page /Parent 2 0 R /Resources << /Font << /F1 14 0 R >> >> /Contents 7 0 R >>",
    );
    w.object(5, "<< /Type /Page /Parent 2 0 R /Contents 8 0 R >>");
    w.stream(6, "", PAGE_A.as_bytes());
    w.stream(7, "", PAGE_B.as_bytes());
    w.stream(8, "", PAGE_C.as_bytes());
    w.object(9, HELVETICA);
    w.object(10, type1_font("Times-Roman"));
    w.object(11, type1_font("Courier"));
    w.stream(
        12,
        "/Type /XObject /Subtype /Form /BBox [0 0 100 40] /Matrix [1 0 0 1 10 10] \
         /Resources << /Font << /F1 9 0 R >> >>",
        FORM_1.as_bytes(),
    );
    w.stream(
        13,
        "/Type /XObject /Subtype /Form /BBox [0 0 100 40] /Matrix [1 0 0 1 10 50]",
        FORM_2.as_bytes(),
    );
    w.object(14, type1_font("Times-Bold"));
    w.object(15, "<< /Type /ExtGState /ca 0.5 >>");
    w.object(16, "<< /Type /ExtGState /CA 0.25 >>");
    w.finish_table(&Trailer::new(1))?;
    let out = w.into_output()?;

    let entry = |category: &str, name: &str, target: u32, uses: u32| ResourceEntryFacts {
        category: category.to_owned(),
        name: name.to_owned(),
        target,
        uses,
    };
    // The entries of the /Pages dictionary with the given usage counts, in file order:
    // F1, F2, F3, GS1, GS9, Fm1, Fm2.
    let pages_entries = |uses: [u32; 7]| {
        vec![
            entry("Font", "F1", 9, uses[0]),
            entry("Font", "F2", 10, uses[1]),
            entry("Font", "F3", 11, uses[2]),
            entry("ExtGState", "GS1", 15, uses[3]),
            entry("ExtGState", "GS9", 16, uses[4]),
            entry("XObject", "Fm1", 12, uses[5]),
            entry("XObject", "Fm2", 13, uses[6]),
        ]
    };
    let scope = |owner: u32, node: u32, entries: Vec<ResourceEntryFacts>| ResourceScopeFacts {
        owner,
        node,
        inherited: owner != node,
        entries,
    };
    let shared = |object: u32, owners: &[u32], total_uses: u32| SharedResourceFacts {
        object,
        owners: owners.to_vec(),
        total_uses,
    };
    let form = |object: u32, name: &str, matrix: [f64; 6]| FormFacts {
        object,
        name: name.to_owned(),
        painted_by: 3,
        matrix: Some(matrix),
        mcids: Vec::new(),
    };

    let media = Sourced::inherited([0.0, 0.0, 200.0, 200.0], 2);
    let expected = Expected::new(PdfVersion::V1_7, &out.revisions, CatalogFacts::new(1))
        .with_page(PageFacts::new(0, 3, media.clone()))
        .with_page(PageFacts::new(1, 4, media.clone()))
        .with_page(PageFacts::new(2, 5, media))
        .with_form(form(12, "Fm1", [1.0, 0.0, 0.0, 1.0, 10.0, 10.0]))
        .with_form(form(13, "Fm2", [1.0, 0.0, 0.0, 1.0, 10.0, 50.0]))
        // Page A has no /Resources: it inherits the /Pages dictionary (object 2).
        .with_resource_scope(scope(3, 2, pages_entries([2, 1, 0, 1, 0, 1, 1])))
        // Page B replaces the whole dictionary: F2 and everything else are gone, F1 is another font.
        .with_resource_scope(scope(4, 4, vec![entry("Font", "F1", 14, 1)]))
        .with_resource_scope(scope(5, 2, pages_entries([1, 0, 0, 0, 0, 0, 0])))
        // Form 1 has its own /Resources and reaches the shared Helvetica again.
        .with_resource_scope(scope(12, 12, vec![entry("Font", "F1", 9, 1)]))
        // Form 2 has none, so it falls back to what its painter (page A) sees.
        .with_resource_scope(scope(13, 2, pages_entries([0, 1, 0, 0, 0, 0, 0])))
        // Helvetica: page A twice, page C once, form 1 once (own dictionary), form 2 not at all.
        .with_shared_resource(shared(9, &[3, 5, 12, 13], 4))
        .with_shared_resource(shared(10, &[3, 5, 13], 2))
        .with_shared_resource(shared(11, &[3, 5, 13], 0))
        .with_shared_resource(shared(12, &[3, 5, 13], 1))
        .with_shared_resource(shared(13, &[3, 5, 13], 1))
        .with_shared_resource(shared(15, &[3, 5, 13], 1))
        .with_shared_resource(shared(16, &[3, 5, 13], 0));
    Ok(built(out, expected))
}
