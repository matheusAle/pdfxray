//! A one-page file holding one object of every kind an object preview draws: fonts (embedded,
//! not embedded, with a broken program), images, a Form `XObject` without resources of its own,
//! colour spaces, a shading and a tiling pattern.
//!
//! It is not part of the [`crate::catalog`]: no expected facts are generated for it. Tests build
//! it with [`object_gallery`] and assert on the objects by number (the `GALLERY_*` constants).

use crate::WriterError;
use crate::writer::{PdfVersion, PdfWriter, Trailer};

/// The embedded font program: Inter Regular (SIL Open Font License, `assets/fonts`).
const INTER: &[u8] = include_bytes!("../../../assets/fonts/Inter-Regular.ttf");

/// The page.
pub const GALLERY_PAGE: u32 = 3;
/// The page's content stream.
pub const GALLERY_CONTENT: u32 = 4;
/// A TrueType font with its program embedded (Inter), shown as `Hi`.
pub const GALLERY_EMBEDDED_FONT: u32 = 5;
/// Helvetica without a program, shown as `Ho`.
pub const GALLERY_MISSING_FONT: u32 = 8;
/// A subset TrueType font whose `/FontFile2` is not a font program, shown as `x`.
pub const GALLERY_BROKEN_FONT: u32 = 9;
/// `[/Indexed /DeviceRGB 1 <FF0000 0000FF>]`: red and blue.
pub const GALLERY_INDEXED: u32 = 12;
/// An axial shading from red at x = 0 to blue at x = 100, extended both ways.
pub const GALLERY_SHADING: u32 = 13;
/// A coloured tiling pattern: a red 10 x 10 square every 20 points.
pub const GALLERY_PATTERN: u32 = 14;
/// An ICC profile stream (`/N 3`, `/Alternate /DeviceRGB`); the profile data is a stub.
pub const GALLERY_ICC: u32 = 15;
/// `[/Separation /Spot /DeviceCMYK <tint transform>]`.
pub const GALLERY_SEPARATION: u32 = 16;
/// A Form `XObject` without `/Resources`, `/BBox [0 0 50 50]` scaled by 2 and moved by (10, 10);
/// it paints `/Im1`, which only the page's resources name.
pub const GALLERY_FORM: u32 = 17;
/// A 2 x 2 RGB image: red, green / blue, yellow.
pub const GALLERY_IMAGE: u32 = 18;
/// An 8 x 1 image mask.
pub const GALLERY_MASK: u32 = 19;

const CONTENT: &str = "\
BT /F1 12 Tf 10 180 Td (Hi) Tj ET
BT /F2 12 Tf 10 160 Td (Ho) Tj ET
BT /F3 12 Tf 10 140 Td (x) Tj ET
q /Fm1 Do Q";

/// The 2 x 2 RGB pixels: red, green / blue, yellow.
const PIXELS: [u8; 12] = [0xFF, 0, 0, 0, 0xFF, 0, 0, 0, 0xFF, 0xFF, 0xFF, 0];

/// Builds the gallery.
///
/// # Errors
/// Only if the writer rejects the file, which the definition rules out.
pub fn object_gallery() -> Result<Vec<u8>, WriterError> {
    let mut w = PdfWriter::new(PdfVersion::V1_7);
    w.object(1, "<< /Type /Catalog /Pages 2 0 R >>");
    w.object(2, "<< /Type /Pages /Kids [3 0 R] /Count 1 >>");
    w.object(
        GALLERY_PAGE,
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Contents 4 0 R /Resources << \
         /Font << /F1 5 0 R /F2 8 0 R /F3 9 0 R >> \
         /XObject << /Fm1 17 0 R /Im1 18 0 R /Mk1 19 0 R >> \
         /ColorSpace << /CS1 12 0 R /CS2 [/ICCBased 15 0 R] /CS3 16 0 R >> \
         /Shading << /Sh1 13 0 R >> /Pattern << /P1 14 0 R >> >> >>",
    );
    w.stream(GALLERY_CONTENT, "", CONTENT.as_bytes());
    let widths = vec!["600"; 95].join(" ");
    w.object(
        GALLERY_EMBEDDED_FONT,
        format!(
            "<< /Type /Font /Subtype /TrueType /BaseFont /Inter-Regular /FirstChar 32 /LastChar 126 \
             /Widths [{widths}] /FontDescriptor 6 0 R /Encoding /WinAnsiEncoding >>"
        ),
    );
    w.object(
        6,
        "<< /Type /FontDescriptor /FontName /Inter-Regular /Flags 32 /FontBBox [-742 -244 2600 1200] \
         /ItalicAngle 0 /Ascent 969 /Descent -242 /CapHeight 727 /StemV 80 /FontFile2 7 0 R >>",
    );
    w.stream(7, &format!("/Length1 {}", INTER.len()), INTER);
    w.object(
        GALLERY_MISSING_FONT,
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>",
    );
    w.object(
        GALLERY_BROKEN_FONT,
        "<< /Type /Font /Subtype /TrueType /BaseFont /ABCDEF+Broken /FirstChar 120 /LastChar 120 \
         /Widths [500] /FontDescriptor 10 0 R /Encoding /WinAnsiEncoding >>",
    );
    w.object(
        10,
        "<< /Type /FontDescriptor /FontName /ABCDEF+Broken /Flags 32 /FontBBox [0 0 1000 1000] \
         /ItalicAngle 0 /Ascent 800 /Descent -200 /CapHeight 700 /StemV 80 /FontFile2 11 0 R >>",
    );
    w.stream(11, "/Length1 16", b"not a font file!");
    w.object(GALLERY_INDEXED, "[/Indexed /DeviceRGB 1 <FF00000000FF>]");
    w.object(
        GALLERY_SHADING,
        "<< /ShadingType 2 /ColorSpace /DeviceRGB /Coords [0 0 100 0] /Extend [true true] \
         /Function << /FunctionType 2 /Domain [0 1] /C0 [1 0 0] /C1 [0 0 1] /N 1 >> >>",
    );
    w.stream(
        GALLERY_PATTERN,
        "/Type /Pattern /PatternType 1 /PaintType 1 /TilingType 1 /BBox [0 0 10 10] \
         /XStep 20 /YStep 20 /Resources << >>",
        b"1 0 0 rg 0 0 10 10 re f",
    );
    w.stream(GALLERY_ICC, "/N 3 /Alternate /DeviceRGB", b"stub profile");
    w.object(
        GALLERY_SEPARATION,
        "[/Separation /Spot /DeviceCMYK \
         << /FunctionType 2 /Domain [0 1] /C0 [0 0 0 0] /C1 [0 1 1 0] /N 1 >>]",
    );
    w.stream(
        GALLERY_FORM,
        "/Type /XObject /Subtype /Form /BBox [0 0 50 50] /Matrix [2 0 0 2 10 10]",
        b"q 50 0 0 50 0 0 cm /Im1 Do Q",
    );
    w.stream(
        GALLERY_IMAGE,
        "/Type /XObject /Subtype /Image /Width 2 /Height 2 /ColorSpace /DeviceRGB /BitsPerComponent 8",
        &PIXELS,
    );
    w.stream(
        GALLERY_MASK,
        "/Type /XObject /Subtype /Image /Width 8 /Height 1 /ImageMask true /BitsPerComponent 1",
        &[0x0F],
    );
    w.finish_table(&Trailer::new(1))?;
    Ok(w.into_output()?.bytes)
}
