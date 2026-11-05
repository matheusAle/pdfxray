//! The pairing's two corpus invariants, over the vendored veraPDF files and the fetched tier
//! (`fixtures/cache`) when present:
//!
//! * **identity**: a document paired with itself yields the empty diff;
//! * **mirror**: pairing A with B and B with A yields mirrored results (`added` ↔ `removed`,
//!   field values and deltas swapped, the offset negated).

#![allow(
    clippy::unwrap_used,
    clippy::panic,
    reason = "test helpers fail loudly on unexpected input"
)]
#![allow(
    clippy::print_stderr,
    reason = "says how small the corpus was, as the veraPDF and PDFium tests do when they skip"
)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use pdfxray_core::diff::{self, PageMatch, Pair, Pairing, Status};
use pdfxray_core::{Document, NodeId};

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

fn corpus() -> Vec<PathBuf> {
    let root = pdfxray_fixtures::fixtures_dir();
    let root = root.parent().unwrap_or(&root).to_path_buf();
    let mut files = Vec::new();
    pdfs_below(&root.join("third_party").join("verapdf"), &mut files);
    let vendored = files.len();
    pdfs_below(&root.join("cache"), &mut files);
    // The fetched tier is not committed (`cargo xtask corpus fetch`). Without it these tests
    // still pass on the vendored sample alone, which is how a mirror failure on a fetched file
    // reached a release candidate unnoticed. `PDFXRAY_CORPUS=required` makes its absence loud,
    // as `PDFXRAY_VERAPDF` and `PDFXRAY_PDFIUM` do for their tools.
    let fetched = files.len() - vendored;
    if std::env::var("PDFXRAY_CORPUS").as_deref() == Ok("required") {
        assert!(
            fetched > 0,
            "PDFXRAY_CORPUS=required, but the fetched corpus tier is missing: \
             run `cargo xtask corpus fetch`"
        );
    } else if fetched == 0 {
        eprintln!(
            "NOTE: the fetched corpus tier is absent, so only the {vendored} vendored files are \
             compared. Run `cargo xtask corpus fetch`, and set PDFXRAY_CORPUS=required to make \
             its absence fail instead of warn."
        );
    }
    files.sort();
    files
}

fn open(path: &Path) -> Option<Document> {
    Document::open(path).ok()
}

#[test]
fn every_corpus_file_paired_with_itself_yields_the_empty_diff() {
    let files = corpus();
    assert!(
        files.len() >= 38,
        "vendored sample went missing: {} files",
        files.len()
    );
    let mut elements = 0;
    for file in &files {
        // Two independent opens: nothing may be shared between the sides.
        let (Some(a), Some(b)) = (open(file), open(file)) else {
            continue;
        };
        let p = diff::pair(&a, &b);
        let name = file.display();
        let changed: Vec<&Pair> = p.pairs.iter().filter(|x| x.status != Status::Same).collect();
        assert!(
            changed.is_empty(),
            "{name}: {} changed, first {:?}",
            changed.len(),
            changed[0]
        );
        assert_eq!(p.offset, None, "{name}");
        assert!(
            p.pages.iter().all(PageMatch::is_unchanged),
            "{name}: {:?}",
            p.pages
        );
        assert_eq!(p.pages.len(), a.page_count(), "{name}");
        assert_eq!(p.pairs.len(), p.sides.a.elements, "{name}");
        assert!(
            p.pairs
                .iter()
                .all(|x| x.a == x.b && x.changed.is_empty() && x.rows.is_empty()),
            "{name}: every element pairs with itself, with nothing to report"
        );
        assert!(p.summary().is_unchanged(), "{name}");
        elements += p.pairs.len();
    }
    // Smoke: the vendored files alone carry 207 structure elements, so the invariant was
    // exercised on real elements, not only on untagged files.
    assert!(
        elements >= 200,
        "only {elements} elements paired across the corpus"
    );
}

/// What a pair says, seen from A, keyed by `(a, b)`.
type View =
    BTreeMap<(Option<NodeId>, Option<NodeId>), (Status, Vec<(String, Option<String>, Option<String>)>)>;

fn view(p: &Pairing, flip: bool) -> View {
    p.pairs
        .iter()
        .map(|x| {
            let fields = x.changed.iter().map(|c| {
                let (a, b) = if flip {
                    (c.b.clone(), c.a.clone())
                } else {
                    (c.a.clone(), c.b.clone())
                };
                (format!("{:?}", c.field), a, b)
            });
            let mut fields: Vec<_> = fields.collect();
            fields.sort();
            if flip {
                ((x.b.clone(), x.a.clone()), (x.status.mirrored(), fields))
            } else {
                ((x.a.clone(), x.b.clone()), (x.status, fields))
            }
        })
        .collect()
}

#[test]
fn pairing_b_with_a_mirrors_pairing_a_with_b() {
    let docs: Vec<Document> = corpus().iter().filter_map(|f| open(f)).collect();
    assert!(docs.len() >= 30, "only {} corpus files opened", docs.len());
    let mut compared = 0;
    let mut differing = 0;
    for w in docs.windows(2) {
        let (a, b) = (&w[0], &w[1]);
        let ab = diff::pair(a, b);
        let ba = diff::pair(b, a);
        assert_eq!(view(&ab, false), view(&ba, true), "{a:?} vs {b:?}");
        assert_eq!(
            ab.offset.map(|o| (o.dx, o.dy)),
            ba.offset
                .map(|o| (-o.dx, -o.dy))
                .map(|(x, y): (f64, f64)| (x + 0.0, y + 0.0)),
            "{a:?} vs {b:?}: the offset negates"
        );
        assert_eq!(ab.pairs.len(), ba.pairs.len());
        let summary = (ab.summary(), ba.summary());
        assert_eq!(
            (
                summary.0.added,
                summary.0.removed,
                summary.0.modified,
                summary.0.moved
            ),
            (
                summary.1.removed,
                summary.1.added,
                summary.1.modified,
                summary.1.moved
            )
        );
        compared += 1;
        differing += usize::from(!ab.summary().is_unchanged());
    }
    // Smoke: different files differ, so the mirror was checked on real changes.
    assert!(
        compared >= 30 && differing >= compared / 2,
        "{differing} of {compared} pairs differed"
    );
}
