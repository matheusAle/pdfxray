//! Shared by `tests/perf.rs` and `benches/open.rs`: a large synthetic tagged document.
//!
//! Every page holds one heading, a Sect, 70 paragraphs, 8 figures (an image `XObject`), and 5
//! small tables (Table, TR, 2 x TD) with some vector art, i.e. 100 structure elements per page.
//! 200 pages therefore carry 20 000 structure elements plus the `Document` root.
//!
//! Each consumer includes this file with `mod common;` / `#[path]` and uses a different subset
//! of it, hence the blanket `dead_code` allowance.

#![allow(dead_code, clippy::unwrap_used, clippy::expect_used)]

use std::fmt::Write as _;

use pdfxray_fixtures::{PdfVersion, PdfWriter, Trailer};

/// Structure elements per page, excluding the shared `Document` element.
pub(crate) const ELEMS_PER_PAGE: u32 = 100;

const PARAGRAPHS: u32 = 70;
const FIGURES: u32 = 8;
const TABLES: u32 = 5;
/// Objects before the first page: catalog, pages, font, struct root, Document, parent tree, image.
const FIRST_PAGE_OBJ: u32 = 8;
/// Objects per page: page, content, then one per structure element.
const PER_PAGE: u32 = 2 + ELEMS_PER_PAGE;

/// Builds a tagged document of `pages` pages (`pages * 100 + 1` structure elements).
pub(crate) fn synthetic_pdf(pages: u32) -> Vec<u8> {
    let page_obj = |p: u32| FIRST_PAGE_OBJ + PER_PAGE * p;
    let mut w = PdfWriter::new(PdfVersion::V1_7);
    let kids: String = (0..pages).fold(String::new(), |mut s, p| {
        let _ = write!(s, "{} 0 R ", page_obj(p));
        s
    });
    let sects: String = (0..pages).fold(String::new(), |mut s, p| {
        let _ = write!(s, "{} 0 R ", page_obj(p) + 2);
        s
    });
    w.object(
        1,
        "<< /Type /Catalog /Pages 2 0 R /StructTreeRoot 4 0 R /MarkInfo << /Marked true >> /Lang (en-US) >>",
    );
    w.object(
        2,
        format!(
            "<< /Type /Pages /Kids [{kids}] /Count {pages} /MediaBox [0 0 612 792] \
             /Resources << /Font << /F1 3 0 R >> /XObject << /Im1 7 0 R >> >> >>"
        ),
    );
    w.object(3, "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>");
    w.object(
        4,
        format!("<< /Type /StructTreeRoot /K 5 0 R /ParentTree 6 0 R /ParentTreeNextKey {pages} >>"),
    );
    w.object(
        5,
        format!("<< /Type /StructElem /S /Document /P 4 0 R /K [{sects}] >>"),
    );
    let mut nums = String::new();
    for p in 0..pages {
        let _ = write!(nums, "{p} [");
        // MCID order is the order elements are written below: H1, paragraphs, figures, cells.
        for e in mcid_elements() {
            let _ = write!(nums, "{} 0 R ", page_obj(p) + 2 + e);
        }
        nums.push_str("] ");
    }
    w.object(6, format!("<< /Nums [{nums}] >>"));
    w.stream(
        7,
        "/Type /XObject /Subtype /Image /Width 4 /Height 4 /ColorSpace /DeviceGray /BitsPerComponent 8",
        &[0x20, 0x60, 0xa0, 0xe0].repeat(4),
    );
    for p in 0..pages {
        let base = page_obj(p);
        w.object(
            base,
            format!(
                "<< /Type /Page /Parent 2 0 R /StructParents {p} /Contents {} 0 R >>",
                base + 1
            ),
        );
        w.stream(base + 1, "", page_content(p).as_bytes());
        write_elements(&mut w, base);
    }
    w.finish_table(&Trailer::new(1)).unwrap();
    w.into_output().unwrap().bytes
}

/// `"a 0 R b 0 R "` for the given object numbers.
fn refs(objects: impl Iterator<Item = u32>) -> String {
    objects.fold(String::new(), |mut out, obj| {
        let _ = write!(out, "{obj} 0 R ");
        out
    })
}

/// Element indices (relative to the Sect, which is 0) that own an MCID, in MCID order.
fn mcid_elements() -> Vec<u32> {
    let mut out = vec![1]; // H1
    out.extend((0..PARAGRAPHS).map(|i| 2 + i));
    out.extend((0..FIGURES).map(|i| 2 + PARAGRAPHS + i));
    let tables = 2 + PARAGRAPHS + FIGURES;
    for t in 0..TABLES {
        // Table, TR, TD, TD
        out.push(tables + t * 4 + 2);
        out.push(tables + t * 4 + 3);
    }
    out
}

fn write_elements(w: &mut PdfWriter, base: u32) {
    let sect = base + 2;
    let el = |i: u32| sect + i;
    let tables = 2 + PARAGRAPHS + FIGURES;
    let kids = refs((1..tables).chain((0..TABLES).map(|t| tables + t * 4)).map(el));
    w.object(
        sect,
        format!("<< /Type /StructElem /S /Sect /P 5 0 R /Pg {base} 0 R /K [{kids}] >>"),
    );
    let mut mcid = 0_u32;
    let mut leaf = |w: &mut PdfWriter, i: u32, role: &str, parent: u32, extra: &str| {
        w.object(
            el(i),
            format!(
                "<< /Type /StructElem /S /{role} /P {} 0 R /Pg {base} 0 R {extra} /K {mcid} >>",
                el(parent)
            ),
        );
        mcid += 1;
    };
    leaf(w, 1, "H1", 0, "");
    for i in 0..PARAGRAPHS {
        leaf(w, 2 + i, "P", 0, "");
    }
    for i in 0..FIGURES {
        leaf(w, 2 + PARAGRAPHS + i, "Figure", 0, "/Alt (A small picture)");
    }
    for t in 0..TABLES {
        let at = tables + t * 4;
        w.object(
            el(at),
            format!(
                "<< /Type /StructElem /S /Table /P {} 0 R /Pg {base} 0 R /K [{} 0 R] >>",
                el(0),
                el(at + 1)
            ),
        );
        w.object(
            el(at + 1),
            format!(
                "<< /Type /StructElem /S /TR /P {} 0 R /Pg {base} 0 R /K [{} 0 R {} 0 R] >>",
                el(at),
                el(at + 2),
                el(at + 3)
            ),
        );
        leaf(w, at + 2, "TD", at + 1, "");
        leaf(w, at + 3, "TD", at + 1, "");
    }
}

/// Content of one page, MCIDs in the order `write_elements` assigns them.
fn page_content(page: u32) -> String {
    let number = page + 1;
    let mut out = String::new();
    let mut mcid = 0_u32;
    let _ = writeln!(
        out,
        "/H1 << /MCID {mcid} >> BDC BT /F1 18 Tf 36 750 Td (Chapter {number}) Tj ET EMC"
    );
    mcid += 1;
    for i in 0..PARAGRAPHS {
        let left = 36 + (i % 3) * 190;
        let base = 720 - (i / 3) * 9;
        let _ = writeln!(
            out,
            "/P << /MCID {mcid} >> BDC 0.95 g {left} {} 180 8 re f 0 g BT /F1 7 Tf {left} {base} Td \
             (Paragraph {i} of chapter {number} with some words) Tj ET EMC",
            base - 2
        );
        mcid += 1;
    }
    for i in 0..FIGURES {
        let left = 36 + i * 70;
        let _ = writeln!(
            out,
            "/Figure << /MCID {mcid} >> BDC q 60 0 0 40 {left} 400 cm /Im1 Do Q \
             {left} 380 m {} 380 l {} 395 l S EMC",
            left + 60,
            left + 60
        );
        mcid += 1;
    }
    for table in 0..TABLES {
        for cell in 0..2 {
            let left = 36 + table * 110 + cell * 50;
            let _ = writeln!(
                out,
                "/TD << /MCID {mcid} >> BDC {left} 200 48 20 re S BT /F1 8 Tf {} 208 Td (Cell {table}.{cell}) \
                 Tj ET EMC",
                left + 4
            );
            mcid += 1;
        }
    }
    let _ = writeln!(out, "/Artifact BMC 0.5 g 0 0 612 10 re f EMC");
    out
}

/// Paragraphs per row of the dense page, laid out left to right.
const DENSE_COLUMNS: u32 = 20;
/// Horizontal distance between two paragraphs of a row, in points.
const DENSE_COLUMN_W: u32 = 29;
/// Vertical distance between two rows, in points (the text is 4 pt).
const DENSE_LINE_H: u32 = 8;
/// x of the first column, in points.
const DENSE_LEFT: u32 = 10;
/// Space above the first row's baseline and below the last one's, in points.
const DENSE_MARGIN: u32 = 12;
/// Width of the dense page, in points (US Letter).
const DENSE_WIDTH: u32 = 612;
/// Largest page side a conforming reader must support (ISO 32000-2, Annex C), in points.
const MAX_PAGE_SIDE: u32 = 14_400;

// Object numbers of the dense page's file; the paragraphs follow from `DENSE_FIRST_P` on.
const DENSE_CATALOG: u32 = 1;
const DENSE_PAGES: u32 = 2;
const DENSE_FONT: u32 = 3;
const DENSE_STRUCT_ROOT: u32 = 4;
const DENSE_DOCUMENT: u32 = 5;
const DENSE_PAGE: u32 = 6;
const DENSE_PARENT_TREE: u32 = 7;
const DENSE_CONTENT: u32 = 8;
pub(crate) const DENSE_FIRST_P: u32 = 9;

/// Height of the dense page holding `n` paragraphs: tall enough for every row, so the content
/// never wraps back to the top and is read strictly top to bottom, left to right.
pub(crate) fn dense_page_height(n: u32) -> u32 {
    n.div_ceil(DENSE_COLUMNS) * DENSE_LINE_H + 2 * DENSE_MARGIN
}

/// One page holding `n` paragraphs, each with its own MCID, all children of one `Document`
/// element: the shape that exposes anything quadratic in the number of MCIDs per page or
/// children per element.
///
/// The paragraphs are laid out in reading order on a page as tall as they need (see
/// [`dense_page_height`]), so a correct document yields no diagnostics: the file is dense, not
/// broken.
///
/// # Panics
///
/// When `n` paragraphs need a page taller than [`MAX_PAGE_SIDE`] (more than 35 940).
pub(crate) fn dense_page_pdf(n: u32) -> Vec<u8> {
    let height = dense_page_height(n);
    assert!(
        height <= MAX_PAGE_SIDE,
        "{n} paragraphs need a {height} pt page, above the {MAX_PAGE_SIDE} pt limit"
    );
    let paragraphs: String = (0..n).fold(String::new(), |mut s, i| {
        let _ = write!(s, "{} 0 R ", DENSE_FIRST_P + i);
        s
    });
    let mut content = String::new();
    for i in 0..n {
        let x = DENSE_LEFT + (i % DENSE_COLUMNS) * DENSE_COLUMN_W;
        let y = height - DENSE_MARGIN - (i / DENSE_COLUMNS) * DENSE_LINE_H;
        let _ = writeln!(
            content,
            "/P << /MCID {i} >> BDC BT /F1 4 Tf {x} {y} Td (w{i}) Tj ET EMC"
        );
    }

    let mut w = PdfWriter::new(PdfVersion::V1_7);
    w.object(
        DENSE_CATALOG,
        format!(
            "<< /Type /Catalog /Pages {DENSE_PAGES} 0 R /StructTreeRoot {DENSE_STRUCT_ROOT} 0 R \
             /MarkInfo << /Marked true >> >>"
        ),
    );
    w.object(
        DENSE_PAGES,
        format!(
            "<< /Type /Pages /Kids [{DENSE_PAGE} 0 R] /Count 1 /MediaBox [0 0 {DENSE_WIDTH} {height}] \
             /Resources << /Font << /F1 {DENSE_FONT} 0 R >> >> >>"
        ),
    );
    w.object(
        DENSE_FONT,
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>",
    );
    w.object(
        DENSE_STRUCT_ROOT,
        format!(
            "<< /Type /StructTreeRoot /K {DENSE_DOCUMENT} 0 R /ParentTree {DENSE_PARENT_TREE} 0 R \
             /ParentTreeNextKey 1 >>"
        ),
    );
    w.object(
        DENSE_DOCUMENT,
        format!("<< /Type /StructElem /S /Document /P {DENSE_STRUCT_ROOT} 0 R /K [{paragraphs}] >>"),
    );
    w.object(
        DENSE_PAGE,
        format!("<< /Type /Page /Parent {DENSE_PAGES} 0 R /StructParents 0 /Contents {DENSE_CONTENT} 0 R >>"),
    );
    // The page's ParentTree entry lists the paragraphs in MCID order, like the Document's /K.
    w.object(DENSE_PARENT_TREE, format!("<< /Nums [0 [{paragraphs}]] >>"));
    w.stream(DENSE_CONTENT, "", content.as_bytes());
    for i in 0..n {
        w.object(
            DENSE_FIRST_P + i,
            format!("<< /Type /StructElem /S /P /P {DENSE_DOCUMENT} 0 R /Pg {DENSE_PAGE} 0 R /K {i} >>"),
        );
    }
    w.finish_table(&Trailer::new(DENSE_CATALOG)).unwrap();
    w.into_output().unwrap().bytes
}
