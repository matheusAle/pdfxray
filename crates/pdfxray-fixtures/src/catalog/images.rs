//! `images`: an image `XObject`, an inline image and an image painted from inside a Form `XObject`.

use super::common::{built, page_dict_with};
use crate::expected::{CatalogFacts, Expected, FormFacts, ImageFacts, ImageKind, PageFacts, Sourced};
use crate::writer::{PdfVersion, PdfWriter, Trailer};
use crate::{Built, WriterError};

/// The 2 x 2 RGB pixels: red, green / blue, yellow. Raw 8-bit samples, no filter.
const PIXELS: [u8; 12] = [0xFF, 0, 0, 0, 0xFF, 0, 0, 0, 0xFF, 0xFF, 0xFF, 0];

/// Operators 0-3 place the `XObject`; 4-7 the rotated inline image; 8-11 paint the form. The
/// inline image is a single operator (`BI`) however many lines it spans.
const PAGE_CONTENT: &str = "\
q
100 0 0 100 50 150 cm
/Im1 Do
Q
q
0 40 -40 0 120 20 cm
BI /W 2 /H 2 /CS /RGB /BPC 8 /F /AHx ID
FF0000 00FF00 0000FF FFFF00>
EI
Q
q
1 0 0 1 10 0 cm
/Fm1 Do
Q";

const FORM_CONTENT: &str = "\
q
60 0 0 60 0 0 cm
/Im1 Do
Q";

pub(super) fn build() -> Result<Built, WriterError> {
    let mut w = PdfWriter::new(PdfVersion::V1_7);
    w.object(1, "<< /Type /Catalog /Pages 2 0 R >>");
    w.object(2, "<< /Type /Pages /Kids [3 0 R] /Count 1 >>");
    w.object(
        3,
        page_dict_with("/MediaBox [0 0 300 300]", "/XObject << /Im1 5 0 R /Fm1 6 0 R >>"),
    );
    w.stream(4, "", PAGE_CONTENT.as_bytes());
    w.stream(
        5,
        "/Type /XObject /Subtype /Image /Width 2 /Height 2 /ColorSpace /DeviceRGB /BitsPerComponent 8",
        &PIXELS,
    );
    w.stream(
        6,
        "/Type /XObject /Subtype /Form /BBox [0 0 100 100] /Matrix [1 0 0 1 200 20] \
         /Resources << /XObject << /Im1 5 0 R >> >>",
        FORM_CONTENT.as_bytes(),
    );
    w.finish_table(&Trailer::new(1))?;
    let out = w.into_output()?;

    let image = |stream: Option<u32>, op_index: u32, kind: ImageKind, bbox: [f64; 4]| ImageFacts {
        page: 0,
        stream,
        op_index,
        kind,
        name: matches!(kind, ImageKind::Xobject).then(|| "Im1".to_owned()),
        object: matches!(kind, ImageKind::Xobject).then_some(5),
        width: 2,
        height: 2,
        bits_per_component: 8,
        color_space: "DeviceRGB".to_owned(),
        bbox,
    };
    let expected = Expected::new(PdfVersion::V1_7, &out.revisions, CatalogFacts::new(1))
        .with_page(PageFacts::new(0, 3, Sourced::own([0.0, 0.0, 300.0, 300.0], 3)))
        // Operator 2: the unit square scaled by 100 and moved to (50,150).
        .with_image(image(None, 2, ImageKind::Xobject, [50.0, 150.0, 150.0, 250.0]))
        // Operator 6: cm [0 40 -40 0 120 20] maps (x,y) to (120 - 40y, 20 + 40x): a quarter turn,
        // x in 80..120 and y in 20..60.
        .with_image(image(None, 6, ImageKind::Inline, [80.0, 20.0, 120.0, 60.0]))
        // Form operator 2: scale 60 inside the form, then the form matrix (+200,+20) and the page
        // CTM (+10,+0): x in 210..270, y in 20..80.
        .with_image(image(Some(6), 2, ImageKind::Xobject, [210.0, 20.0, 270.0, 80.0]))
        .with_form(FormFacts {
            object: 6,
            name: "Fm1".to_owned(),
            painted_by: 3,
            matrix: Some([1.0, 0.0, 0.0, 1.0, 200.0, 20.0]),
            mcids: Vec::new(),
        });
    Ok(built(out, expected))
}
