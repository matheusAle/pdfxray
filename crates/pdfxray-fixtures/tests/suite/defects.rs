//! Every deliberate defect, proven from the raw bytes without going through `PdfWriter`.

use std::collections::BTreeSet;

use pdfxray_fixtures::{BoxKind, Defect, Expected, find};
use pretty_assertions::assert_eq;

use crate::common::{
    count, find as find_bytes, kids, latin1, mcids_in, name_after, numbers_after, object_offset, object_text,
    page_objects, parent_tree_nums, parse_table, ref_after, role_map, scan_objects, stream_data, text_of,
};
use crate::content::{analyze_marked_content, parse_ops};

fn built(name: &str) -> (Vec<u8>, Expected) {
    let b = find(name).unwrap().build().unwrap();
    (b.pdf, b.expected)
}

#[test]
fn every_defective_fixture_declares_the_defects_it_is_named_for() {
    let kinds: Vec<(&str, Vec<&str>)> = pdfxray_fixtures::all()
        .iter()
        .map(|f| {
            let defects = f.build().unwrap().expected.defects;
            let kinds = defects
                .iter()
                .map(|d| match d {
                    Defect::XrefOffsetWrong { .. } => "xref_offset_wrong",
                    Defect::RoleMapCycle { .. } => "role_map_cycle",
                    Defect::OrphanMcid { .. } => "orphan_mcid",
                    Defect::MissingMcid { .. } => "missing_mcid",
                    Defect::ParentTreeMismatch { .. } => "parent_tree_mismatch",
                    Defect::BoxOutsideMediaBox { .. } => "box_outside_media_box",
                    Defect::Encrypted { .. } => "encrypted",
                    Defect::StartxrefWrong { .. } => "startxref_wrong",
                    Defect::UntaggedContent { .. } => "untagged_content",
                    Defect::DuplicateMcid { .. } => "duplicate_mcid",
                    Defect::MarkedContentInsideArtifact { .. } => "marked_content_inside_artifact",
                })
                .collect();
            (f.name, kinds)
        })
        .filter(|(_, kinds): &(&str, Vec<&str>)| !kinds.is_empty())
        .collect();

    assert_eq!(
        kinds,
        vec![
            ("page-boxes", vec!["box_outside_media_box"]),
            ("rolemap-custom", vec!["role_map_cycle"]),
            (
                "broken-xref-offsets",
                vec!["xref_offset_wrong", "xref_offset_wrong"]
            ),
            (
                "orphan-mcid",
                vec!["orphan_mcid", "missing_mcid", "parent_tree_mismatch"]
            ),
            ("encrypted-stub", vec!["encrypted"]),
            (
                "untagged-content",
                vec![
                    "untagged_content",
                    "untagged_content",
                    "duplicate_mcid",
                    "marked_content_inside_artifact"
                ]
            ),
            ("startxref-broken", vec!["startxref_wrong"]),
        ]
    );
}

#[test]
fn xref_offsets_disagree_with_the_real_object_headers_by_exactly_the_declared_amount() {
    let (pdf, expected) = built("broken-xref-offsets");
    let (entries, prev) = parse_table(&pdf, crate::common::last_startxref(&pdf));

    // Ground truth by scanning for "N 0 obj" at line starts.
    let truth: Vec<(u32, usize)> = scan_objects(&pdf);
    let recorded: Vec<(u32, usize)> = entries
        .iter()
        .filter(|e| e.in_use)
        .map(|e| (e.obj, e.offset))
        .collect();
    let deltas: Vec<(u32, i64)> = recorded
        .iter()
        .zip(&truth)
        .map(|((obj, rec), (truth_obj, actual))| {
            assert_eq!(obj, truth_obj);
            (*obj, *rec as i64 - *actual as i64)
        })
        .filter(|(_, delta)| *delta != 0)
        .collect();

    assert_eq!(prev, None);
    assert_eq!(deltas, vec![(3, 7), (4, -3)]);

    let from_bytes: Vec<Defect> = deltas
        .iter()
        .map(|(obj, _)| Defect::XrefOffsetWrong {
            obj: *obj,
            recorded: recorded.iter().find(|(o, _)| o == obj).unwrap().1 as u64,
            actual: truth.iter().find(|(o, _)| o == obj).unwrap().1 as u64,
        })
        .collect();
    assert_eq!(expected.defects, from_bytes);
    // Smoke: the other three entries are correct.
    assert_eq!(recorded.len() - deltas.len(), 3);
}

#[test]
fn a_wrong_offset_no_longer_points_at_an_object_header() {
    let (pdf, expected) = built("broken-xref-offsets");

    for defect in &expected.defects {
        let Defect::XrefOffsetWrong { obj, recorded, .. } = defect else {
            panic!("unexpected defect {defect:?}");
        };
        let at = &pdf[usize::try_from(*recorded).unwrap()..];

        assert!(!at.starts_with(format!("{obj} 0 obj").as_bytes()), "object {obj}");
        assert_eq!(
            object_offset(&pdf, *obj).map(|o| o as u64),
            expected.defects.iter().find_map(|d| match d {
                Defect::XrefOffsetWrong { obj: o, actual, .. } if o == obj => Some(*actual),
                _ => None,
            })
        );
    }
}

/// Finds a cycle in `role_map` by walking from each key; returns it rotated to start at its smallest name.
fn find_cycle(map: &std::collections::BTreeMap<String, String>) -> Option<Vec<String>> {
    for start in map.keys() {
        let mut path = vec![start.clone()];
        let mut current = start;
        while let Some(next) = map.get(current) {
            if let Some(pos) = path.iter().position(|p| p == next) {
                let mut cycle = path[pos..].to_vec();
                let min = cycle
                    .iter()
                    .enumerate()
                    .min_by_key(|(_, n)| (*n).clone())
                    .unwrap()
                    .0;
                cycle.rotate_left(min);
                return Some(cycle);
            }
            path.push(next.clone());
            current = next;
        }
    }
    None
}

#[test]
fn role_map_contains_the_declared_cycle() {
    let (pdf, expected) = built("rolemap-custom");
    let tree = expected.struct_tree.as_ref().unwrap();
    let map = role_map(&text_of(&pdf, tree.root));

    let cycle = find_cycle(&map);

    assert_eq!(cycle, Some(vec!["LoopA".to_owned(), "LoopB".to_owned()]));
    assert_eq!(
        expected.defects,
        vec![Defect::RoleMapCycle {
            cycle: cycle.unwrap()
        }]
    );
    // Smoke: the non-cyclic custom roles resolve and are not reported.
    assert_eq!(map.get("Heading1").map(String::as_str), Some("H1"));
    assert_eq!(map.get("MyPara").map(String::as_str), Some("P"));
    assert_eq!(find_cycle(&role_map("/RoleMap << /A /B >>")), None);
}

#[test]
fn content_and_structure_disagree_about_mcids_as_declared() {
    let (pdf, expected) = built("orphan-mcid");
    let content = latin1(&stream_data(&pdf, 4));
    let in_content: BTreeSet<i32> = mcids_in(&content).into_iter().collect();

    // Walk every struct element from the bytes: (object, /K MCID).
    let elems: Vec<(u32, i32)> = scan_objects(&pdf)
        .into_iter()
        .filter_map(|(obj, _)| {
            let text = object_text(&pdf, obj)?;
            text.contains("/Type /StructElem").then_some(())?;
            kids(&text).1.first().map(|mcid| (obj, *mcid))
        })
        .collect();
    let referenced: BTreeSet<i32> = elems.iter().map(|(_, m)| *m).collect();

    let parent_tree = &parent_tree_nums(&text_of(&pdf, 11))[0];
    let mut computed: Vec<Defect> = Vec::new();
    for mcid in in_content.difference(&referenced) {
        computed.push(Defect::OrphanMcid { page: 0, mcid: *mcid });
    }
    for (elem, mcid) in &elems {
        if !in_content.contains(mcid) {
            computed.push(Defect::MissingMcid {
                elem: *elem,
                page: 0,
                mcid: *mcid,
            });
        }
    }
    for (elem, mcid) in &elems {
        if let Some(slot) = parent_tree.1.get(usize::try_from(*mcid).unwrap())
            && *slot != Some(*elem)
        {
            computed.push(Defect::ParentTreeMismatch {
                key: parent_tree.0,
                mcid: *mcid,
                parent_tree: *slot,
                k_parent: *elem,
            });
        }
    }

    assert_eq!(in_content, BTreeSet::from([0, 1, 2]));
    assert_eq!(referenced, BTreeSet::from([0, 2, 5]));
    assert_eq!(parent_tree.1, vec![Some(8), None, Some(8)]);
    assert_eq!(computed, expected.defects);
    assert_eq!(expected.defects.len(), 3);
}

#[test]
fn trim_box_sticks_out_of_the_media_box_on_the_declared_page() {
    let (pdf, expected) = built("page-boxes");
    let mut computed = Vec::new();
    for (index, page) in page_objects(&pdf).into_iter().enumerate() {
        let text = text_of(&pdf, page);
        let media = crate::common::inherited(&pdf, page, "/MediaBox ")
            .and_then(|(_, t)| numbers_after(&t, "/MediaBox "))
            .unwrap();
        for (key, kind) in [
            ("/CropBox ", BoxKind::Crop),
            ("/BleedBox ", BoxKind::Bleed),
            ("/TrimBox ", BoxKind::Trim),
            ("/ArtBox ", BoxKind::Art),
        ] {
            if let Some(b) = numbers_after(&text, key) {
                let outside = b[0] < media[0] || b[1] < media[1] || b[2] > media[2] || b[3] > media[3];
                if outside {
                    computed.push(Defect::BoxOutsideMediaBox {
                        page: u32::try_from(index).unwrap(),
                        box_kind: kind,
                    });
                }
            }
        }
    }

    assert_eq!(
        computed,
        vec![Defect::BoxOutsideMediaBox {
            page: 2,
            box_kind: BoxKind::Trim
        }]
    );
    assert_eq!(computed, expected.defects);
}

#[test]
fn encrypt_dictionary_is_referenced_from_the_trailer_with_the_declared_handler() {
    let (pdf, expected) = built("encrypted-stub");
    let text = latin1(&pdf);
    let trailer = &text[text.rfind("trailer").unwrap()..];
    let encrypt = ref_after(trailer, "/Encrypt ").unwrap();
    let dict = text_of(&pdf, encrypt);

    assert_eq!(encrypt, 6);
    assert_eq!(
        expected.defects,
        vec![Defect::Encrypted {
            handler: name_after(&dict, "/Filter ").unwrap(),
            v: i32::try_from(crate::common::int_after(&dict, "/V ").unwrap()).unwrap(),
            r: i32::try_from(crate::common::int_after(&dict, "/R ").unwrap()).unwrap(),
        }]
    );
    // The stub does not actually encrypt: page content is readable plain text.
    assert_eq!(
        latin1(&stream_data(&pdf, 4)),
        "BT /F1 18 Tf 20 40 Td (Not really secret) Tj ET"
    );
    assert_eq!(count(&pdf, b"/Encrypt "), 1);
    assert!(find_bytes(&pdf, b"/Filter /Standard").is_some());
}

#[test]
fn untagged_duplicate_and_nested_content_are_recomputed_from_the_operators() {
    let (pdf, expected) = built("untagged-content");
    let ops = parse_ops(&stream_data(&pdf, 4));

    let report = analyze_marked_content(&ops);

    let mut computed: Vec<Defect> = report
        .untagged
        .iter()
        .map(|range| Defect::UntaggedContent {
            page: 0,
            op_index_range: *range,
        })
        .collect();
    computed.extend(
        report
            .mcid_uses
            .iter()
            .filter(|(_, uses)| **uses > 1)
            .map(|(mcid, uses)| Defect::DuplicateMcid {
                page: 0,
                mcid: *mcid,
                count: *uses,
            }),
    );
    computed.extend(report.nested_in_artifact.iter().map(|(op_index, tag)| {
        Defect::MarkedContentInsideArtifact {
            page: 0,
            op_index: *op_index,
            tag: tag.clone(),
        }
    }));
    assert_eq!(computed, expected.defects);
    // The ranges point at the painting operators the comments in the catalog name.
    let names: Vec<&str> = report
        .untagged
        .iter()
        .map(|r| ops[r[0] as usize].name.as_str())
        .collect();
    assert_eq!(names, ["Tj", "f"]);
    assert_eq!(ops[37].name, "BDC");
    // Smoke: MCIDs 0 and 2 are used once, MCID 1 twice, and the page lists them in stream order.
    assert_eq!(
        report.mcid_uses.into_iter().collect::<Vec<_>>(),
        [(0, 1), (1, 2), (2, 1)]
    );
    assert_eq!(expected.pages[0].mcids, [0, 1, 1, 2]);
}

#[test]
fn well_tagged_fixtures_have_no_untagged_duplicate_or_nested_content() {
    for name in [
        "tagged-simple",
        "artifacts",
        "orphan-mcid",
        "rolemap-custom",
        "struct-rich",
    ] {
        let (pdf, expected) = built(name);
        let page = page_objects(&pdf)[0];
        let contents = ref_after(&text_of(&pdf, page), "/Contents ").unwrap();

        let report = analyze_marked_content(&parse_ops(&stream_data(&pdf, contents)));

        assert_eq!(report.untagged, Vec::<[u32; 2]>::new(), "{name}");
        assert_eq!(report.nested_in_artifact, Vec::<(u32, String)>::new(), "{name}");
        assert!(report.mcid_uses.values().all(|uses| *uses == 1), "{name}");
        assert!(
            expected.defects.iter().all(|d| !matches!(
                d,
                Defect::UntaggedContent { .. }
                    | Defect::DuplicateMcid { .. }
                    | Defect::MarkedContentInsideArtifact { .. }
            )),
            "{name}"
        );
    }
}

#[test]
fn startxref_points_away_from_the_xref_table_by_the_declared_amount() {
    let (pdf, expected) = built("startxref-broken");

    let recorded = crate::common::last_startxref(&pdf);
    let actual = pdf.windows(6).rposition(|w| w == b"\nxref\n").unwrap() + 1;

    assert_eq!(
        expected.defects,
        vec![Defect::StartxrefWrong {
            recorded: recorded as u64,
            actual: actual as u64
        }]
    );
    assert_eq!(recorded - actual, 12);
    assert!(pdf[actual..].starts_with(b"xref\n"));
    assert!(!pdf[recorded..].starts_with(b"xref"));
    assert_eq!(expected.revisions[0].xref_offset, actual as u64);
    // Smoke: the table itself is intact, so reading it from the true offset finds all five objects.
    let (entries, prev) = parse_table(&pdf, actual);
    assert_eq!(entries.iter().filter(|e| e.in_use).count(), 5);
    assert_eq!(prev, None);
}
