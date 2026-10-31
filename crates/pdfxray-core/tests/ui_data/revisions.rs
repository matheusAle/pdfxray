//! Revisions.

use pretty_assertions::assert_eq;

use super::*;

#[test]
fn revisions_match_what_the_generator_recorded_for_every_well_formed_fixture() {
    let mut checked = 0;
    for name in [
        "minimal-one-page",
        "incremental-update",
        "hybrid-xref",
        "xref-stream-objstm",
        "tagged-simple",
        "struct-rich",
        "images",
        "page-boxes",
    ] {
        let doc = open(name);
        let expected: serde_json::Value = serde_json::from_slice(
            &std::fs::read(fixtures_dir().join(format!("{name}.expected.json"))).unwrap(),
        )
        .unwrap();
        let want: Vec<(u64, u64, Vec<u64>)> = expected["revisions"]
            .as_array()
            .unwrap()
            .iter()
            .map(|rev| {
                (
                    rev["number"].as_u64().unwrap() - 1,
                    rev["xref_offset"].as_u64().unwrap(),
                    rev["objects"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|o| o.as_u64().unwrap())
                        .collect(),
                )
            })
            .collect();

        let got: Vec<(u64, u64, Vec<u64>)> = doc
            .revisions()
            .iter()
            .map(|rev| {
                (
                    u64::from(rev.index),
                    u64::try_from(rev.xref_offset).unwrap(),
                    rev.objects_defined.iter().map(|o| u64::from(o.num)).collect(),
                )
            })
            .collect();

        assert_eq!(got, want, "{name}");
        checked += 1;
    }
    assert_eq!(checked, 8);
}

#[test]
fn an_incremental_update_redefines_an_object_and_adds_another() {
    let doc = open("incremental-update");

    let revisions = doc.revisions();

    assert_eq!(revisions.len(), 2);
    assert_eq!(
        (
            revisions[0].index,
            revisions[0].xref_offset,
            revisions[0].is_latest
        ),
        (0, 410, false)
    );
    assert_eq!(
        (
            revisions[1].index,
            revisions[1].xref_offset,
            revisions[1].is_latest
        ),
        (1, 826, true)
    );
    assert_eq!(revisions[1].objects_defined, vec![r(4), r(6)]);
    assert!(matches!(revisions[1].xref_kind, pdfxray_core::XrefKind::Table));
    // Object 4 (the content stream) was rewritten in revision 1; 6 (the Info) appeared there.
    assert_eq!(doc.object_last_changed(r(4)), Some(1));
    assert_eq!(doc.object_last_changed(r(6)), Some(1));
    assert_eq!(doc.object_last_changed(r(1)), Some(0));
    assert_eq!(doc.object_last_changed(r(99)), None);
    assert!(doc.changed_in_later_revision(r(4)));
    assert!(!doc.changed_in_later_revision(r(6)), "added, not changed");
    assert!(!doc.changed_in_later_revision(r(1)));
    assert!(!doc.changed_in_later_revision(r(99)));
    let changed: Vec<u32> = (1..=6).filter(|n| doc.changed_in_later_revision(r(*n))).collect();
    assert_eq!(changed, [4]);
}

#[test]
fn a_hybrid_file_has_one_revision_that_lists_the_compressed_objects_too() {
    let doc = open("hybrid-xref");

    let revisions = doc.revisions();

    assert_eq!(revisions.len(), 1);
    assert_eq!(
        (
            revisions[0].index,
            revisions[0].xref_offset,
            revisions[0].is_latest
        ),
        (0, 615, true)
    );
    let nums: Vec<u32> = revisions[0].objects_defined.iter().map(|o| o.num).collect();
    assert_eq!(
        nums,
        [1, 2, 3, 4, 5, 6, 7, 8],
        "6 and 7 are compressed into object stream 5"
    );
    assert_eq!(doc.object_last_changed(r(6)), Some(0));
    assert!(!doc.changed_in_later_revision(r(6)));
}
