//! Every generated fixture.

use pretty_assertions::assert_eq;

use super::*;

#[test]
fn every_generated_fixture_captures_consistently() {
    let mut captured = 0usize;
    let mut skipped = Vec::new();
    for entry in fs::read_dir(fixtures_dir()).unwrap().flatten() {
        let path = entry.path();
        if path.extension().is_none_or(|e| e != "pdf") {
            continue;
        }
        let name = path.file_stem().unwrap().to_string_lossy().into_owned();
        let Ok(pdf) = Pdf::new(fs::read(&path).unwrap()) else {
            skipped.push(name);
            continue;
        };
        for page in capture_document(&pdf) {
            assert_consistent(&page);
            assert_eq!(page.alignment, Alignment::Exact, "{name} page {}", page.page);
            captured += 1;
        }
    }
    // Only the deliberately unreadable files may be skipped.
    skipped.sort();
    assert!(
        skipped
            .iter()
            .all(|n| n.contains("encrypted") || n.contains("broken")),
        "{skipped:?}"
    );
    assert!(captured >= 25, "{captured} pages captured");
}
