//! `box-matrix`: a media box with a non-zero origin, every box on one page, quarter-turn
//! rotations, `/UserUnit` and a crop box inherited from the `/Pages` node.
//!
//! Effective render size is defined in [`crate::expected::PageGeometryFacts`].

use super::common::{HELVETICA, built};
use crate::expected::{CatalogFacts, Expected, PageFacts, PageGeometryFacts, Sourced};
use crate::writer::{PdfVersion, PdfWriter, Trailer};
use crate::{Built, WriterError};

/// Every box in its own colour; the media box has its origin at (-50,-50).
const PAGE0_CONTENT: &str = "\
1 0 0 RG -50 -50 700 892 re S
0 0.6 0 RG 0 0 600 792 re S
0 0 1 RG 10 10 580 772 re S
1 0 1 RG 20 20 560 752 re S
0 0.6 0.6 RG 30 30 540 732 re S
BT /F1 24 Tf 100 700 Td (Box matrix) Tj ET";

const PAGE1_CONTENT: &str = "\
0 0 0 RG 25 25 350 250 re S
BT /F1 18 Tf 40 140 Td (Rotate 180) Tj ET";

const PAGE2_CONTENT: &str = "\
0 0 0 RG 15 15 220 270 re S
BT /F1 18 Tf 30 140 Td (Rotate 270) Tj ET";

const PAGE3_CONTENT: &str = "\
0 0 0 RG 15 15 220 270 re S
BT /F1 18 Tf 30 140 Td (UserUnit 3) Tj ET";

pub(super) fn build() -> Result<Built, WriterError> {
    let mut w = PdfWriter::new(PdfVersion::V1_7);
    w.object(1, "<< /Type /Catalog /Pages 2 0 R >>");
    w.object(
        2,
        "<< /Type /Pages /Kids [3 0 R 4 0 R 5 0 R 6 0 R] /Count 4 \
         /MediaBox [0 0 300 400] /CropBox [10 10 240 290] >>",
    );
    w.object(
        3,
        "<< /Type /Page /Parent 2 0 R /MediaBox [-50 -50 650 842] /CropBox [0 0 600 792] \
         /BleedBox [10 10 590 782] /TrimBox [20 20 580 772] /ArtBox [30 30 570 762] \
         /Resources << /Font << /F1 7 0 R >> >> /Contents 8 0 R >>",
    );
    w.object(
        4,
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 400 300] /CropBox [20 20 380 280] /Rotate 180 \
         /Resources << /Font << /F1 7 0 R >> >> /Contents 9 0 R >>",
    );
    w.object(
        5,
        "<< /Type /Page /Parent 2 0 R /Rotate 270 /Resources << /Font << /F1 7 0 R >> >> /Contents 10 0 R >>",
    );
    w.object(
        6,
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 250 300] /UserUnit 3.0 \
         /Resources << /Font << /F1 7 0 R >> >> /Contents 11 0 R >>",
    );
    w.object(7, HELVETICA);
    w.stream(8, "", PAGE0_CONTENT.as_bytes());
    w.stream(9, "", PAGE1_CONTENT.as_bytes());
    w.stream(10, "", PAGE2_CONTENT.as_bytes());
    w.stream(11, "", PAGE3_CONTENT.as_bytes());
    w.finish_table(&Trailer::new(1))?;
    let out = w.into_output()?;

    let geometry = |page: u32, visible_box: [f64; 4], rotate: i32, user_unit: f64| {
        let (w, h) = (visible_box[2] - visible_box[0], visible_box[3] - visible_box[1]);
        let render_size = if rotate % 180 == 0 { [w, h] } else { [h, w] };
        PageGeometryFacts {
            page,
            visible_box,
            rotate,
            user_unit,
            render_size,
            physical_size: [render_size[0] * user_unit, render_size[1] * user_unit],
        }
    };
    let expected = Expected::new(PdfVersion::V1_7, &out.revisions, CatalogFacts::new(1))
        .with_page(
            PageFacts::new(0, 3, Sourced::own([-50.0, -50.0, 650.0, 842.0], 3))
                .crop(Sourced::own([0.0, 0.0, 600.0, 792.0], 3))
                .bleed(Sourced::own([10.0, 10.0, 590.0, 782.0], 3))
                .trim(Sourced::own([20.0, 20.0, 580.0, 772.0], 3))
                .art(Sourced::own([30.0, 30.0, 570.0, 762.0], 3)),
        )
        .with_page(
            PageFacts::new(1, 4, Sourced::own([0.0, 0.0, 400.0, 300.0], 4))
                .crop(Sourced::own([20.0, 20.0, 380.0, 280.0], 4))
                .rotate(Sourced::own(180, 4)),
        )
        // Page 2 owns neither box: both come from the /Pages node, object 2.
        .with_page(
            PageFacts::new(2, 5, Sourced::inherited([0.0, 0.0, 300.0, 400.0], 2))
                .crop(Sourced::inherited([10.0, 10.0, 240.0, 290.0], 2))
                .rotate(Sourced::own(270, 5)),
        )
        // Page 3 owns its media box but inherits the crop box.
        .with_page(
            PageFacts::new(3, 6, Sourced::own([0.0, 0.0, 250.0, 300.0], 6))
                .crop(Sourced::inherited([10.0, 10.0, 240.0, 290.0], 2))
                .user_unit(Sourced::own(3.0, 6)),
        )
        // Page 0: crop 600 x 792 lies inside the media box, so that is the visible box; no turn.
        .with_page_geometry(geometry(0, [0.0, 0.0, 600.0, 792.0], 0, 1.0))
        // Page 1: crop 360 x 260, a half turn keeps the orientation.
        .with_page_geometry(geometry(1, [20.0, 20.0, 380.0, 280.0], 180, 1.0))
        // Page 2: inherited crop 230 x 280, a quarter turn swaps it to 280 x 230.
        .with_page_geometry(geometry(2, [10.0, 10.0, 240.0, 290.0], 270, 1.0))
        // Page 3: crop 230 x 280 renders at that size, but physically 3 times larger: 690 x 840.
        .with_page_geometry(geometry(3, [10.0, 10.0, 240.0, 290.0], 0, 3.0));
    Ok(built(out, expected))
}
