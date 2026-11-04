//! Checks across the diff's modules over the vendored corpus: the bottom-up keys agree with the
//! per-element queries, the two payload layers agree, and the output does not depend on the
//! order pages are visited in.

use std::path::{Path, PathBuf};

use pretty_assertions::assert_eq;

use super::pair::{Visit, pair_with};
use super::{Field, keys, rows};
use crate::{Document, UnicodeSource};

fn pdfs_below(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for path in entries.flatten().map(|e| e.path()) {
        if path.is_dir() {
            pdfs_below(&path, out);
        } else if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("pdf")) {
            out.push(path);
        }
    }
}

/// The vendored veraPDF files that open, sorted by path.
fn vendored() -> Vec<Document> {
    let root = pdfxray_fixtures::fixtures_dir();
    let root = root.parent().unwrap_or(&root).to_path_buf();
    let mut files = Vec::new();
    pdfs_below(&root.join("third_party").join("verapdf"), &mut files);
    files.sort();
    files.iter().filter_map(|f| Document::open(f).ok()).collect()
}

#[test]
fn bottom_up_keys_agree_with_the_per_element_queries() {
    let mut checked_boxes = 0;
    let mut checked_texts = 0;
    for doc in vendored() {
        let keys = keys::build(&doc);
        assert_eq!(keys.len(), doc.tag_elem_count());
        for e in &keys {
            // The box, as `Document::bbox` computes it walking the subtree.
            assert_eq!(e.placed, doc.bbox(&e.node), "{doc:?} {}", e.node);
            checked_boxes += usize::from(e.placed.is_some());
            // The text, as the inspector's `Text` row reads it walking the subtree. Glyphs
            // without Unicode are skipped there and keyed here, so those are not compared.
            if e.text.unicode == Some(UnicodeSource::None) {
                continue;
            }
            let row = doc
                .tag_props(&e.node)
                .and_then(|p| p.get("Content", "Text").map(str::to_owned));
            let prefix = e.text.prefix.as_str();
            match row {
                Some(row) => {
                    assert!(
                        row.starts_with(prefix),
                        "{doc:?} {}: {row:?} vs {prefix:?}",
                        e.node
                    );
                    checked_texts += 1;
                }
                None => assert!(prefix.is_empty(), "{doc:?} {}: {prefix:?}", e.node),
            }
        }
    }
    // Smoke: the corpus has painted, tagged text.
    assert!(
        checked_boxes >= 100 && checked_texts >= 100,
        "{checked_boxes} boxes, {checked_texts} texts"
    );
}

#[test]
fn the_typed_layer_never_disagrees_with_the_row_layer() {
    let docs = vendored();
    let mut typed_seen = 0;
    for w in docs.windows(2) {
        let p = super::pair(&w[0], &w[1]);
        for pair in &p.pairs {
            for r in &pair.rows {
                if let Some(field) = rows::field_of(&r.group, &r.key) {
                    assert!(
                        pair.changed.iter().any(|c| c.field == field),
                        "row {}/{} changed but {field:?} is not in the typed layer: {pair:?}",
                        r.group,
                        r.key
                    );
                }
            }
            for c in &pair.changed {
                let row_backed = !matches!(c.field, Field::Level | Field::Bbox | Field::Page);
                if row_backed {
                    assert!(
                        pair.rows
                            .iter()
                            .any(|r| rows::field_of(&r.group, &r.key) == Some(c.field)),
                        "{:?} changed but no row did: {pair:?}",
                        c.field
                    );
                }
                typed_seen += 1;
            }
        }
    }
    // Smoke: different files do differ in typed fields.
    assert!(typed_seen > 0);
}

#[test]
fn output_is_byte_identical_across_runs_and_page_visit_orders() {
    let docs = vendored();
    let mut compared = 0;
    for w in docs.windows(2) {
        let (ka, kb) = (keys::build(&w[0]), keys::build(&w[1]));
        let json = |visit| serde_json::to_string(&pair_with(&w[0], &ka, &w[1], &kb, visit)).unwrap();
        let first = json(Visit::Forward);
        assert_eq!(json(Visit::Forward), first, "{:?} vs {:?}: two runs", w[0], w[1]);
        assert_eq!(
            json(Visit::Reverse),
            first,
            "{:?} vs {:?}: page order",
            w[0],
            w[1]
        );
        compared += 1;
    }
    assert!(compared >= 30);
}
