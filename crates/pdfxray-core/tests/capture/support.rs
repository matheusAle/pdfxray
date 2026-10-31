//! Helpers shared by the `capture` tests.

use pretty_assertions::assert_eq;

use super::*;

pub(crate) const EPS: f64 = 1e-3;

pub(crate) fn load(name: &str) -> Pdf {
    let path = pdfxray_fixtures::pdf_path(name);
    Pdf::new(fs::read(&path).unwrap()).unwrap()
}

pub(crate) fn expected(name: &str) -> Expected {
    let path = fixtures_dir().join(format!("{name}.expected.json"));
    serde_json::from_slice(&fs::read(&path).unwrap()).unwrap()
}

pub(crate) fn fixture_page(name: &str, page: u32) -> CapturedPage {
    capture_page(&load(name), page).unwrap()
}

pub(crate) enum Obj<'a> {
    Plain(&'a str),
    Stream(&'a str, &'a [u8]),
}

/// A 200x200 page with Helvetica as `/F1` (object 5). `resources` is spliced into the page's
/// resource dictionary; `extra` objects are numbered from 6 up by the caller.
pub(crate) fn synth_streams(streams: &[&[u8]], resources: &str, extra: &[(u32, Obj<'_>)]) -> Pdf {
    let mut w = PdfWriter::new(PdfVersion::V1_7);
    w.object(1, "<< /Type /Catalog /Pages 2 0 R >>");
    w.object(2, "<< /Type /Pages /Kids [3 0 R] /Count 1 >>");
    let contents = if streams.len() == 1 {
        "4 0 R".to_owned()
    } else {
        let refs: Vec<String> = (0..streams.len()).map(|i| format!("{} 0 R", 40 + i)).collect();
        format!("[{}]", refs.join(" "))
    };
    w.object(
        3,
        format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] \
             /Resources << /Font << /F1 5 0 R >> {resources} >> /Contents {contents} >>"
        ),
    );
    w.object(5, "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>");
    if let [only] = streams {
        w.stream(4, "", only);
    } else {
        for (i, data) in streams.iter().enumerate() {
            w.stream(40 + u32::try_from(i).unwrap(), "", data);
        }
    }
    for (num, obj) in extra {
        match obj {
            Obj::Plain(body) => {
                w.object(*num, body);
            }
            Obj::Stream(dict, data) => {
                w.stream(*num, dict, data);
            }
        }
    }
    w.finish_table(&Trailer::new(1)).unwrap();
    Pdf::new(w.into_output().unwrap().bytes).unwrap()
}

pub(crate) fn synth(content: &[u8], resources: &str, extra: &[(u32, Obj<'_>)]) -> CapturedPage {
    capture_page(&synth_streams(&[content], resources, extra), 0).unwrap()
}

pub(crate) fn close(actual: f64, want: f64) -> bool {
    (actual - want).abs() < EPS
}

#[track_caller]
pub(crate) fn assert_rect(actual: Rect, want: [f64; 4]) {
    let got = [actual.x0, actual.y0, actual.x1, actual.y1];
    assert!(
        got.iter().zip(want).all(|(a, b)| close(*a, b)),
        "rect {got:?} != {want:?}"
    );
}

#[track_caller]
pub(crate) fn assert_matrix(actual: [f64; 6], want: [f64; 6]) {
    assert!(
        actual.iter().zip(want).all(|(a, b)| close(*a, b)),
        "matrix {actual:?} != {want:?}"
    );
}

#[track_caller]
pub(crate) fn assert_points(actual: &[Point], want: &[(f64, f64)]) {
    let got: Vec<(f64, f64)> = actual.iter().map(|p| (p.x, p.y)).collect();
    assert_eq!(got.len(), want.len(), "point count: {got:?}");
    assert!(
        got.iter()
            .zip(want)
            .all(|(g, w)| close(g.0, w.0) && close(g.1, w.1)),
        "points {got:?} != {want:?}"
    );
}

/// One glyph item unpacked.
pub(crate) struct Glyph<'p> {
    pub(crate) item: &'p PaintItem,
    pub(crate) index: u32,
    pub(crate) code: u32,
    pub(crate) unicode: &'p str,
    pub(crate) origin: Point,
    pub(crate) advance: f64,
}

pub(crate) fn glyphs(page: &CapturedPage) -> Vec<Glyph<'_>> {
    page.items
        .iter()
        .filter_map(|item| match &item.kind {
            PaintKind::Glyph {
                glyph_index,
                char_code,
                unicode,
                origin,
                advance,
            } => Some(Glyph {
                item,
                index: *glyph_index,
                code: *char_code,
                unicode: unicode.as_deref().unwrap_or(""),
                origin: *origin,
                advance: *advance,
            }),
            _ => None,
        })
        .collect()
}

pub(crate) fn text(glyphs: &[Glyph<'_>]) -> String {
    glyphs.iter().map(|g| g.unicode).collect()
}

pub(crate) fn origins(glyphs: &[Glyph<'_>]) -> Vec<Point> {
    glyphs.iter().map(|g| g.origin).collect()
}

pub(crate) fn kinds_of(page: &CapturedPage, wanted: fn(&PaintKind) -> bool) -> Vec<&PaintItem> {
    page.items.iter().filter(|i| wanted(&i.kind)).collect()
}

pub(crate) fn fills(page: &CapturedPage) -> Vec<&PaintItem> {
    kinds_of(page, |k| matches!(k, PaintKind::Fill))
}

pub(crate) fn union(items: &[&PaintItem]) -> Rect {
    items.iter().skip(1).fold(items[0].bbox, |acc, i| Rect {
        x0: acc.x0.min(i.bbox.x0),
        y0: acc.y0.min(i.bbox.y0),
        x1: acc.x1.max(i.bbox.x1),
        y1: acc.y1.max(i.bbox.y1),
    })
}

/// `(operator, operands)` pairs of the page's operators.
pub(crate) fn op_list(page: &CapturedPage) -> Vec<(&str, Vec<&str>)> {
    page.ops
        .iter()
        .map(|o| {
            (
                o.operator.as_str(),
                o.operands.iter().map(String::as_str).collect(),
            )
        })
        .collect()
}

pub(crate) fn gray(v: f64) -> ColorInfo {
    ColorInfo {
        space: "DeviceGray".to_owned(),
        resource: None,
        components: vec![v],
        pattern: None,
    }
}

/// Structural invariants every captured page must satisfy, exact or degraded.
#[track_caller]
pub(crate) fn assert_consistent(page: &CapturedPage) {
    for (i, op) in page.ops.iter().enumerate() {
        assert_eq!(op.ix as usize, i, "op ix");
    }
    for (i, item) in page.items.iter().enumerate() {
        assert_eq!(item.ix as usize, i, "item ix");
        let op = &page.ops[item.op as usize];
        let first = op.first_item.unwrap() as usize;
        assert!(
            (first..first + op.item_count as usize).contains(&i),
            "item {i} outside the item range of op {}",
            op.ix
        );
        assert!(item.bbox.x0 <= item.bbox.x1 && item.bbox.y0 <= item.bbox.y1);
        assert!(item.mc_stack.iter().all(|m| (*m as usize) < page.marked.len()));
        assert!(
            item.xobject_stack
                .iter()
                .all(|x| (*x as usize) < page.xobjects.len())
        );
    }
    assert_eq!(
        page.ops.iter().map(|o| o.item_count as usize).sum::<usize>(),
        page.items.len()
    );
    for (i, seq) in page.marked.iter().enumerate() {
        assert_eq!(seq.ix as usize, i, "marked ix");
        assert!(matches!(
            page.ops[seq.begin_op as usize].operator.as_str(),
            "BMC" | "BDC"
        ));
        if let Some(end) = seq.end_op {
            assert_eq!(page.ops[end as usize].operator, "EMC");
        }
    }
}

pub(crate) fn seq_summary(seq: &MarkedSeq) -> (&str, Option<i32>, u32, Option<u32>, Option<u32>) {
    (seq.tag.as_str(), seq.mcid, seq.begin_op, seq.end_op, seq.parent)
}
