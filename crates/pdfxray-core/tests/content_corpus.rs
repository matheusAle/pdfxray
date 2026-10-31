//! Content queries over the vendored veraPDF corpus: opening a file, building the content tree of
//! page 0 and probing it must never panic, whatever the file looks like.
//!
//! The corpus is optional: when `fixtures/third_party/verapdf` is absent the tests pass trivially.

// Helpers here are not `#[test]` functions, so clippy.toml's allow-*-in-tests does not reach them;
// failing loudly is what a test helper should do.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};

use pdfxray_core::capture::PaintKind;
use pdfxray_core::{Document, LayerMode, NodeId, Point, TreeKind, TreeNode};
use pdfxray_fixtures::fixtures_dir;
use pretty_assertions::assert_eq;

/// Most nodes probed per file, so one huge page does not dominate the run.
const MAX_PROBED_NODES: usize = 400;

fn pdfs_below(dir: &Path, out: &mut Vec<PathBuf>) {
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

fn corpus() -> Vec<PathBuf> {
    let root = fixtures_dir();
    let root = root.parent().unwrap_or(&root).to_path_buf();
    let mut files = Vec::new();
    pdfs_below(&root.join("third_party").join("verapdf"), &mut files);
    files.sort();
    files
}

fn flatten<'t>(nodes: &'t [TreeNode], out: &mut Vec<&'t TreeNode>) {
    for node in nodes {
        out.push(node);
        flatten(&node.children, out);
    }
}

/// What probing one file found, for the smoke assertions.
#[derive(Default)]
struct Probe {
    opened: usize,
    with_content: usize,
    nodes_resolved: usize,
    hit_content: usize,
    glyphs_resolved: usize,
}

fn probe(path: &Path, totals: &mut Probe) {
    let Ok(doc) = Document::open(path) else {
        return;
    };
    totals.opened += 1;
    if doc.page_count() == 0 {
        return;
    }
    let tree = doc.tree(TreeKind::Content(0));
    let mut flat = Vec::new();
    flatten(&tree, &mut flat);
    if !flat.is_empty() {
        totals.with_content += 1;
    }
    for node in flat.iter().take(MAX_PROBED_NODES) {
        // Every shown operator resolves to props and, when it has geometry, a box on its page.
        if matches!(node.id, NodeId::Op { .. }) {
            assert!(
                doc.node(&node.id).is_some(),
                "{}: no props for {}",
                path.display(),
                node.id
            );
            let _ = doc.bbox(&node.id);
            let _ = doc.breadcrumb(&node.id);
            totals.nodes_resolved += 1;
        }
    }
    let _ = doc.tree(TreeKind::Artifacts);
    probe_glyphs(&doc, totals);

    let crop = doc.page(0).expect("page 0").crop_box;
    let centre = Point::new(f64::midpoint(crop.x0, crop.x1), f64::midpoint(crop.y0, crop.y1));
    for layer in LayerMode::ALL {
        let hits = doc.hit(0, centre, layer);
        for hit in &hits {
            let _ = doc.node(hit);
            let _ = doc.bbox(hit);
            let _ = doc.breadcrumb(hit);
        }
        if layer == LayerMode::Content && hits.iter().any(|h| matches!(h, NodeId::Op { .. })) {
            totals.hit_content += 1;
        }
    }
    // The page corners and a point far outside must be safe too.
    for p in [
        Point::new(crop.x0, crop.y0),
        Point::new(crop.x1, crop.y1),
        Point::new(-1.0e9, 1.0e9),
        Point::new(f64::NAN, 0.0),
    ] {
        let _ = doc.hit(0, p, LayerMode::All);
    }
}

/// Resolves the first glyphs of the page and hits the centre of each box.
fn probe_glyphs(doc: &Document, totals: &mut Probe) {
    let Some(page) = doc.captured(0) else {
        return;
    };
    let glyphs = page
        .items
        .iter()
        .filter_map(|item| match item.kind {
            PaintKind::Glyph { glyph_index, .. } => Some((item.op, glyph_index, item.bbox)),
            _ => None,
        })
        .take(MAX_PROBED_NODES);
    for (op, glyph, bbox) in glyphs {
        let id = NodeId::Glyph { page: 0, op, glyph };
        assert!(doc.node(&id).is_some(), "no props for {id}");
        assert_eq!(doc.bbox(&id).map(|b| b.1), Some(bbox), "{id}");
        assert_eq!(
            doc.breadcrumb(&id).last(),
            Some(&id),
            "{id}: the breadcrumb ends at the node"
        );
        let centre = Point::new(f64::midpoint(bbox.x0, bbox.x1), f64::midpoint(bbox.y0, bbox.y1));
        for layer in [
            LayerMode::All,
            LayerMode::Content,
            LayerMode::Tags,
            LayerMode::Artifacts,
        ] {
            let _ = doc.hit(0, centre, layer);
        }
        totals.glyphs_resolved += 1;
    }
}

#[test]
fn verapdf_corpus_page_zero_never_panics() {
    let files = corpus();
    let mut totals = Probe::default();

    for file in &files {
        probe(file, &mut totals);
    }

    if files.is_empty() {
        return;
    }
    assert!(totals.opened > 0, "no corpus file opened out of {}", files.len());
    assert!(totals.with_content > 0, "no corpus page had content operators");
    assert!(totals.nodes_resolved > 0);
    assert!(totals.glyphs_resolved > 0, "no corpus page showed a glyph");
    assert!(totals.hit_content <= totals.with_content);
}
