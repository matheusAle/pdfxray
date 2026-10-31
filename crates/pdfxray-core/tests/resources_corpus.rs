//! Resources over the vendored veraPDF files (`fixtures/third_party/verapdf`): the tree builds
//! and every entry resolves to props, whatever the file looks like. The corpus is optional; the
//! test passes trivially when it is absent.

// Helpers here are not `#[test]` functions, so clippy.toml's allow-*-in-tests does not reach them.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use pdfxray_core::{Document, NodeId, TreeKind, TreeNode};
use pdfxray_fixtures::fixtures_dir;

/// Entries resolved to props per file; keeps the run short on resource-heavy files.
const MAX_ENTRIES_PER_FILE: usize = 150;

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

fn walk<'t>(nodes: &'t [TreeNode], out: &mut Vec<&'t TreeNode>) {
    for n in nodes {
        out.push(n);
        walk(&n.children, out);
    }
}

#[test]
fn corpus_trees_build_and_entries_resolve() {
    let mut files = 0;
    let mut entries = 0;
    for path in corpus() {
        let Ok(doc) = Document::open(&path) else {
            continue;
        };
        let name = path.display().to_string();
        let tree = doc.tree(TreeKind::Resources);
        let mut nodes = Vec::new();
        walk(&tree, &mut nodes);

        // Category group numbers are unique within the tree.
        let groups: Vec<u32> = nodes
            .iter()
            .filter_map(|n| match n.id {
                NodeId::Group(g) => Some(g),
                _ => None,
            })
            .collect();
        assert_eq!(
            groups.iter().collect::<BTreeSet<_>>().len(),
            groups.len(),
            "duplicate group ids in {name}"
        );
        assert!(groups.iter().all(|g| *g >= 200), "{name}");

        // No owner appears twice (resource entries may repeat only in the shared listing).
        let owners: Vec<&NodeId> = tree
            .iter()
            .filter(|n| n.id != NodeId::Group(200))
            .map(|n| &n.id)
            .collect();
        assert_eq!(
            owners.iter().collect::<BTreeSet<_>>().len(),
            owners.len(),
            "{name}"
        );

        let resources: Vec<&TreeNode> = nodes
            .iter()
            .copied()
            .filter(|n| matches!(n.id, NodeId::Resource { .. }))
            .collect();
        for node in resources.iter().take(MAX_ENTRIES_PER_FILE) {
            let props = doc
                .node(&node.id)
                .unwrap_or_else(|| panic!("{} has no props in {name}", node.id));
            assert_eq!(props.id, node.id, "{name}");
            assert!(props.get("Resource", "Name").is_some(), "{} in {name}", node.id);
            assert!(props.get("Usage", "Count").is_some(), "{} in {name}", node.id);
            entries += 1;
        }
        files += 1;
    }
    // Smoke: if the corpus is present, the run looked at real entries.
    if !corpus().is_empty() {
        assert!(files > 0);
        assert!(entries > 0, "no resource entry in {files} files");
    }
}
