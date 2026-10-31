//! Coverage.

use pretty_assertions::assert_eq;

use super::*;

#[test]
fn coverage_counts_painted_items_by_how_they_are_marked() {
    let untagged = open("untagged-content").coverage(0).unwrap();
    let artifacts = open("artifacts").coverage(0).unwrap();
    let simple = open("tagged-simple").coverage(0).unwrap();

    // Tagged: "Tagged one" 10 + "Tagged two" 10 + "Tagged again" 12 glyphs. Artifact: "Header" 6 +
    // the "Nested" 6 that sit inside an artifact although they carry an MCID. Untagged: "Untagged
    // text" 13 glyphs and the grey rule.
    assert_eq!(
        (
            untagged.tagged,
            untagged.artifact,
            untagged.untagged,
            untagged.total
        ),
        (32, 12, 14, 58)
    );
    assert_eq!(
        untagged.untagged_ops,
        [17, 28],
        "the Tj of the untagged text, then the `f` of the rule"
    );
    // Artifacts: header 11 + footer 11 glyphs, a layout rule and a border; body text 9 glyphs.
    assert_eq!(
        (
            artifacts.tagged,
            artifacts.artifact,
            artifacts.untagged,
            artifacts.total
        ),
        (9, 24, 0, 33)
    );
    assert_eq!(artifacts.untagged_ops, Vec::<u32>::new());
    assert_eq!(
        (simple.tagged, simple.artifact, simple.untagged, simple.total),
        (21, 0, 0, 21)
    );
    for c in [&untagged, &artifacts, &simple] {
        assert_eq!(c.tagged + c.artifact + c.untagged, c.total);
    }
    assert_eq!(
        open("minimal-one-page")
            .coverage(0)
            .map(|c| (c.untagged, c.total)),
        Some((13, 13))
    );
    assert_eq!(open("minimal-one-page").coverage(1), None, "there is no page 2");
}
