//! Robustness: every vendored veraPDF file must open and build its structure tree without a
//! panic, and everything the tags tree shows must be reachable through the query API.
//!
//! `insta` is not a dev-dependency of this crate, so the per-file results are printed
//! (`cargo test -p pdfxray-core --test tags_corpus -- --nocapture`) and checked by invariants.

#![allow(
    clippy::unwrap_used,
    clippy::panic,
    reason = "test helpers fail loudly on unexpected input"
)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use pdfxray_core::{Document, NodeId, TreeKind, TreeNode};

fn pdfs_under(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = entries.filter_map(Result::ok).map(|e| e.path()).collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            pdfs_under(&path, out);
        } else if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("pdf")) {
            out.push(path);
        }
    }
}

fn walk(node: &TreeNode, f: &mut impl FnMut(&TreeNode)) {
    f(node);
    for c in &node.children {
        walk(c, f);
    }
}

#[test]
fn every_verapdf_file_builds_a_consistent_structure_tree() {
    let fixtures = pdfxray_fixtures::fixtures_dir();
    let dir = fixtures.parent().unwrap().join("third_party").join("verapdf");
    let mut files = Vec::new();
    pdfs_under(&dir, &mut files);
    assert!(
        files.len() >= 10,
        "vendored sample went missing: {} files",
        files.len()
    );

    let mut opened = 0;
    let mut tagged = 0;
    let mut codes: BTreeMap<&'static str, usize> = BTreeMap::new();
    for path in &files {
        let name = path.strip_prefix(&dir).unwrap().display().to_string();
        let Ok(doc) = Document::open(path) else {
            println!("{name}: not opened");
            continue;
        };
        opened += 1;
        let top = doc.tree(TreeKind::Tags);
        let mut elems = 0;
        for root in &top {
            assert_eq!(root.id, NodeId::Group(100), "{name}");
            walk(root, &mut |n| {
                if matches!(n.id, NodeId::StructElem(_)) {
                    elems += 1;
                    let props = doc
                        .node(&n.id)
                        .unwrap_or_else(|| panic!("{name}: no props for {}", n.id));
                    assert_eq!(props.id, n.id, "{name}");
                    assert_eq!(doc.breadcrumb(&n.id).last(), Some(&n.id), "{name}");
                    let _ = doc.bbox(&n.id);
                }
            });
        }
        if !top.is_empty() {
            tagged += 1;
        }
        let mut tag_codes = Vec::new();
        let diagnostics = doc.info().diagnostics;
        for (i, d) in diagnostics.iter().enumerate() {
            let index = u32::try_from(i).unwrap();
            assert!(doc.node(&NodeId::Diagnostic(index)).is_some(), "{name}");
            if is_tag_code(d.code) {
                tag_codes.push(d.code);
                *codes.entry(d.code).or_default() += 1;
            }
        }
        tag_codes.sort_unstable();
        tag_codes.dedup();
        println!("{name}: {elems} elements, tag diagnostics {tag_codes:?}");
    }

    println!(
        "opened {opened}/{} files, {tagged} with a structure tree; tag codes {codes:?}",
        files.len()
    );
    assert!(
        opened >= files.len() - 3,
        "too many files failed to open: {opened}/{}",
        files.len()
    );
    assert!(tagged >= 5, "tagged files: {tagged}");
}

fn is_tag_code(code: &str) -> bool {
    matches!(
        code,
        "rolemap-cycle"
            | "non-standard-type"
            | "parent-tree-mismatch"
            | "parent-tree-missing"
            | "mcid-not-in-content"
            | "mcid-not-in-structure"
            | "mcid-referenced-twice"
            | "mcid-duplicate-in-content"
            | "struct-parent-missing"
            | "struct-elem-cycle"
            | "struct-elem-direct"
            | "struct-elem-no-type"
            | "struct-elem-multiple-parents"
            | "struct-elem-parent-mismatch"
            | "struct-kid-invalid"
            | "struct-tree-too-deep"
            | "pg-missing"
            | "pg-not-a-page"
            | "class-not-found"
            | "id-tree-mismatch"
            | "duplicate-element-id"
            | "objr-target-missing"
            | "marked-false-but-tagged"
            | "marked-true-no-struct-tree"
            | "marked-content-in-artifact"
            | "untagged-content"
    )
}
