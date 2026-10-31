//! `page-boxes`: every page box, rotation, inheritance, user units and one box outside the media box.

use super::common::built;
use crate::expected::{BoxKind, CatalogFacts, Defect, Expected, PageFacts, Sourced};
use crate::writer::{PdfVersion, PdfWriter, Trailer};
use crate::{Built, WriterError};

/// Page 1: strokes every box in its own colour, innermost last.
const PAGE1_CONTENT: &str = "\
1 0 0 RG 10 10 380 280 re S
0 0.6 0 RG 20 20 360 260 re S
0 0 1 RG 30 30 340 240 re S
1 0 1 RG 40 40 320 220 re S";

/// Page 2: a frame just inside its inherited media box.
const PAGE2_CONTENT: &str = "0 0 0 RG 10 10 280 380 re S";

/// Page 3: a frame in the media box and a square that sticks out of it.
const PAGE3_CONTENT: &str = "\
0 0 0 RG 10 10 180 180 re S
1 0 0 RG 150 150 100 100 re S";

pub(super) fn build() -> Result<Built, WriterError> {
    let mut w = PdfWriter::new(PdfVersion::V1_7);
    w.object(1, "<< /Type /Catalog /Pages 2 0 R >>");
    w.object(
        2,
        "<< /Type /Pages /Kids [3 0 R 4 0 R 5 0 R] /Count 3 /MediaBox [0 0 300 400] >>",
    );
    w.object(
        3,
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 400 300] /CropBox [10 10 390 290] \
         /BleedBox [20 20 380 280] /TrimBox [30 30 370 270] /ArtBox [40 40 360 260] \
         /Rotate 90 /Resources << >> /Contents 6 0 R >>",
    );
    w.object(
        4,
        "<< /Type /Page /Parent 2 0 R /Resources << >> /Contents 7 0 R >>",
    );
    w.object(
        5,
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /TrimBox [150 150 250 250] \
         /UserUnit 2.0 /Resources << >> /Contents 8 0 R >>",
    );
    w.stream(6, "", PAGE1_CONTENT.as_bytes());
    w.stream(7, "", PAGE2_CONTENT.as_bytes());
    w.stream(8, "", PAGE3_CONTENT.as_bytes());
    w.finish_table(&Trailer::new(1))?;
    let out = w.into_output()?;

    let expected = Expected::new(PdfVersion::V1_7, &out.revisions, CatalogFacts::new(1))
        .with_page(
            PageFacts::new(0, 3, Sourced::own([0.0, 0.0, 400.0, 300.0], 3))
                .crop(Sourced::own([10.0, 10.0, 390.0, 290.0], 3))
                .bleed(Sourced::own([20.0, 20.0, 380.0, 280.0], 3))
                .trim(Sourced::own([30.0, 30.0, 370.0, 270.0], 3))
                .art(Sourced::own([40.0, 40.0, 360.0, 260.0], 3))
                .rotate(Sourced::own(90, 3)),
        )
        .with_page(PageFacts::new(
            1,
            4,
            Sourced::inherited([0.0, 0.0, 300.0, 400.0], 2),
        ))
        .with_page(
            PageFacts::new(2, 5, Sourced::own([0.0, 0.0, 200.0, 200.0], 5))
                .trim(Sourced::own([150.0, 150.0, 250.0, 250.0], 5))
                .user_unit(Sourced::own(2.0, 5)),
        )
        .with_defect(Defect::BoxOutsideMediaBox {
            page: 2,
            box_kind: BoxKind::Trim,
        });
    Ok(built(out, expected))
}
