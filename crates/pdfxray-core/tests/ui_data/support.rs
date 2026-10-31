//! Helpers shared by the `ui_data` tests.

use super::*;

pub(crate) fn open(name: &str) -> Document {
    Document::open(pdfxray_fixtures::pdf_path(name)).unwrap()
}

pub(crate) fn third_party(relative: &str) -> PathBuf {
    fixtures_dir()
        .parent()
        .unwrap()
        .join("third_party")
        .join("verapdf")
        .join(relative)
}

pub(crate) fn near(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-3
}

pub(crate) fn r(num: u32) -> ObjRef {
    ObjRef::new(num, 0)
}

/// The glyph node of glyph `glyph` of operator `op` on page 0.
pub(crate) fn glyph(op: u32, glyph: u32) -> NodeId {
    NodeId::Glyph { page: 0, op, glyph }
}

/// Execution index of the first operator named `operator` on page 0.
pub(crate) fn first_op(doc: &Document, operator: &str) -> u32 {
    doc.captured(0)
        .unwrap()
        .ops
        .iter()
        .find(|o| o.operator == operator)
        .unwrap_or_else(|| panic!("no {operator} operator"))
        .ix
}

/// Every `.pdf` below `dir`, sorted.
pub(crate) fn pdfs_below(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            pdfs_below(&path, out);
        } else if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("pdf")) {
            out.push(path);
        }
    }
}

/// A one-page document with the given page dictionary entries and content.
pub(crate) fn single_page(
    page_extra: &str,
    content: &str,
    extra_objects: impl FnOnce(&mut PdfWriter),
) -> Document {
    let mut w = PdfWriter::new(PdfVersion::V1_7);
    w.object(1, "<< /Type /Catalog /Pages 2 0 R >>");
    w.object(2, "<< /Type /Pages /Kids [3 0 R] /Count 1 >>");
    w.object(
        3,
        format!("<< /Type /Page /Parent 2 0 R {page_extra} /Contents 4 0 R >>"),
    );
    w.stream(4, "", content.as_bytes());
    extra_objects(&mut w);
    w.finish_table(&Trailer::new(1)).unwrap();
    Document::from_bytes("synthetic.pdf", w.into_output().unwrap().bytes).unwrap()
}
