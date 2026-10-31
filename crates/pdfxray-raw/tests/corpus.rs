//! The vendored veraPDF sample and the fetched corpus: the raw layer must never panic on real
//! files, must find the same pages as hayro-syntax where hayro can open the file, and its
//! diagnostics are snapshotted so changes are reviewed.
#![allow(
    missing_docs,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::cast_possible_truncation
)]

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use pdfxray_raw::{ObjRef, Object, RawDocument, RawError};
use pretty_assertions::assert_eq;

fn pdfs_under(dir: &Path) -> Vec<PathBuf> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, out);
            } else if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("pdf")) {
                out.push(path);
            }
        }
    }
    let mut out = Vec::new();
    walk(dir, &mut out);
    out.sort();
    out
}

fn fixtures_root() -> PathBuf {
    pdfxray_fixtures::fixtures_dir()
        .parent()
        .expect("fixtures dir has a parent")
        .to_path_buf()
}

/// Leaf pages reachable from the catalog, following `/Kids` once per node.
fn reachable_pages(doc: &RawDocument) -> Option<usize> {
    fn walk(doc: &RawDocument, node: &Object, seen: &mut HashSet<ObjRef>, depth: usize) -> usize {
        if depth > 64 {
            return 0;
        }
        if let Object::Ref(r) = node
            && !seen.insert(*r)
        {
            return 0;
        }
        let node = doc.resolve(node).into_owned();
        let Some(dict) = node.as_dict() else {
            return 0;
        };
        match dict.get(b"Kids").map(|k| doc.resolve(&k.value).into_owned()) {
            Some(Object::Array(kids)) => kids.iter().map(|k| walk(doc, &k.value, seen, depth + 1)).sum(),
            _ => 1,
        }
    }
    let root = doc.resolve(&doc.trailer()?.get(b"Root")?.value).into_owned();
    let pages = root.as_dict()?.get(b"Pages")?.value.clone();
    Some(walk(doc, &pages, &mut HashSet::new(), 0))
}

/// Reads every object and decodes every stream; returns how many were read.
fn exercise(doc: &RawDocument) -> (usize, usize) {
    let (mut objects, mut streams) = (0, 0);
    for r in doc.object_refs() {
        let Some(obj) = doc.object(r) else {
            continue;
        };
        objects += 1;
        if let Object::Stream(s) = &obj.value.value {
            streams += 1;
            let _ = doc.decode_stream(s);
        }
    }
    (objects, streams)
}

struct Outcome {
    name: String,
    diagnostics: BTreeMap<&'static str, usize>,
    pages: Option<usize>,
    hayro_pages: Option<usize>,
    encrypted: bool,
}

fn survey(dir: &Path) -> Vec<Outcome> {
    let root = fixtures_root();
    pdfs_under(dir)
        .into_iter()
        .map(|path| {
            let bytes = std::fs::read(&path).unwrap();
            let name = path
                .strip_prefix(&root)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            let hayro_pages = hayro_syntax::Pdf::new(Arc::new(bytes.clone()))
                .ok()
                .map(|p| p.pages().len());
            let doc = match RawDocument::parse(Arc::new(bytes)) {
                Ok(doc) => doc,
                Err(RawError::MissingHeader) => {
                    return Outcome {
                        name,
                        diagnostics: BTreeMap::from([("missing-header", 1)]),
                        pages: None,
                        hayro_pages,
                        encrypted: false,
                    };
                }
            };
            let (objects, _) = exercise(&doc);
            assert!(objects <= doc.object_refs().len(), "{name}");
            for d in doc.diagnostics() {
                assert!(
                    d.span.start <= d.span.end && d.span.end <= doc.bytes().len(),
                    "{name}: {d}"
                );
            }
            let mut diagnostics = BTreeMap::new();
            for d in doc.diagnostics() {
                *diagnostics.entry(d.code()).or_default() += 1;
            }
            Outcome {
                pages: reachable_pages(&doc),
                encrypted: doc.encryption().is_some(),
                diagnostics,
                name,
                hayro_pages,
            }
        })
        .collect()
}

fn snapshot_of(outcomes: &[Outcome]) -> BTreeMap<String, Vec<String>> {
    outcomes
        .iter()
        .map(|o| {
            (
                o.name.clone(),
                o.diagnostics
                    .iter()
                    .map(|(code, n)| format!("{code} x{n}"))
                    .collect(),
            )
        })
        .collect()
}

/// Reachable page counts agree with hayro-syntax wherever hayro can open the file. Encrypted files
/// are skipped because pdfxray does not decrypt their object streams.
fn assert_pages_agree(outcomes: &[Outcome]) {
    let mut compared = 0;
    for o in outcomes {
        if let (Some(hayro), false) = (o.hayro_pages, o.encrypted) {
            assert_eq!(o.pages, Some(hayro), "{}: raw vs hayro-syntax pages", o.name);
            compared += 1;
        }
    }
    // Smoke: the comparison really ran for most files.
    assert!(
        compared * 2 >= outcomes.len(),
        "only {compared} of {} files were compared",
        outcomes.len()
    );
}

#[test]
fn vendored_verapdf_sample_agrees_with_hayro_and_snapshots_its_diagnostics() {
    let outcomes = survey(&fixtures_root().join("third_party"));
    if outcomes.is_empty() {
        eprintln!("no vendored PDFs found; skipping");
        return;
    }

    assert_pages_agree(&outcomes);
    insta::assert_yaml_snapshot!("verapdf_diagnostics", snapshot_of(&outcomes));
}

#[test]
fn fetched_corpus_agrees_with_hayro_and_snapshots_its_diagnostics() {
    let outcomes = survey(&fixtures_root().join("cache"));
    if outcomes.is_empty() {
        eprintln!("no fetched corpus found; skipping");
        return;
    }

    assert_pages_agree(&outcomes);
    insta::assert_yaml_snapshot!("cache_diagnostics", snapshot_of(&outcomes));
}

/// Deterministic xorshift, so failures reproduce.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, n: usize) -> usize {
        usize::try_from(self.next() % u64::try_from(n.max(1)).unwrap_or(1)).unwrap_or(0)
    }
}

#[test]
fn mutated_real_files_never_panic_and_keep_spans_inside() {
    let root = fixtures_root();
    let mut files = pdfs_under(&root.join("third_party"));
    files.extend(pdfs_under(&root.join("cache")));
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    let mut parsed = 0;
    for path in files
        .iter()
        .filter(|p| std::fs::metadata(p).is_ok_and(|m| m.len() < 400_000))
    {
        let original = std::fs::read(path).unwrap();
        for round in 0..12 {
            let mut bytes = original.clone();
            for _ in 0..=rng.below(8) {
                let at = rng.below(bytes.len());
                if bytes.is_empty() {
                    break;
                }
                match rng.below(4) {
                    0 => bytes[at] = u8::try_from(rng.below(256)).unwrap_or(0),
                    1 => bytes.truncate(at.max(16).min(bytes.len())),
                    2 => {
                        let end = (at + rng.below(200)).min(bytes.len());
                        bytes.drain(at..end);
                    }
                    _ => bytes.insert(at, b"0 <>[]/(%\n"[rng.below(10)]),
                }
            }
            if let Ok(doc) = RawDocument::parse(Arc::new(bytes)) {
                let len = doc.bytes().len();
                for d in doc.diagnostics() {
                    assert!(
                        d.span.start <= d.span.end && d.span.end <= len,
                        "{}: round {round}: {d}",
                        path.display()
                    );
                }
                let _ = exercise(&doc);
                parsed += 1;
            }
        }
    }
    // Smoke: mutated files mostly still have a header and get parsed.
    assert!(parsed > 0 || files.is_empty());
}
