//! Re-derives the `Expected` facts from the raw bytes with the independent scanner.

use pdfxray_fixtures::{
    ArtifactFacts, Defect, FormFacts, PageFacts, ParentTreeEntry, Rect, Sourced, StructElemFacts,
    StructFacts, all, find,
};
use pretty_assertions::assert_eq;

use crate::common::{
    count, float_after, header_version, inherited, int_after, kids, last_startxref, latin1, mcids_in,
    name_after, numbers_after, object_offset, object_text, observe_revisions, observe_revisions_from,
    page_objects, parent_tree_nums, parse_table_full, ref_after, refs_in, resolve_role, role_map,
    root_object, stream_data, text_of,
};

fn rect4(text: &str, key: &str) -> Option<Rect> {
    numbers_after(text, key).map(|n| [n[0], n[1], n[2], n[3]])
}

fn content_of_page(pdf: &[u8], page: u32) -> String {
    let contents = ref_after(&text_of(pdf, page), "/Contents ").expect("/Contents");
    latin1(&stream_data(pdf, contents))
}

fn observe_page(pdf: &[u8], index: u32, obj: u32) -> PageFacts {
    let own = text_of(pdf, obj);
    let sourced = |key: &str| {
        let (node, text) = inherited(pdf, obj, key)?;
        Some(Sourced {
            value: rect4(&text, key)?,
            node,
            inherited: node != obj,
        })
    };
    let own_rect = |key: &str| rect4(&own, key).map(|value| Sourced::own(value, obj));

    PageFacts {
        index,
        object: obj,
        media_box: sourced("/MediaBox ").expect("media box"),
        crop_box: sourced("/CropBox "),
        bleed_box: own_rect("/BleedBox "),
        trim_box: own_rect("/TrimBox "),
        art_box: own_rect("/ArtBox "),
        rotate: inherited(pdf, obj, "/Rotate ").map(|(node, text)| Sourced {
            value: i32::try_from(int_after(&text, "/Rotate ").expect("rotate")).expect("small"),
            node,
            inherited: node != obj,
        }),
        user_unit: float_after(&own, "/UserUnit ").map(|v| Sourced::own(v, obj)),
        mcids: mcids_in(&content_of_page(pdf, obj)),
    }
}

fn observe_struct_elem(
    pdf: &[u8],
    obj: u32,
    map: &std::collections::BTreeMap<String, String>,
    pages: &[u32],
) -> StructElemFacts {
    let text = text_of(pdf, obj);
    let role = name_after(&text, "/S ").expect("/S");
    let (children, mcids) = kids(&text);
    StructElemFacts {
        object: obj,
        standard_role: resolve_role(map, &role),
        role,
        page: ref_after(&text, "/Pg ").map(|pg| {
            u32::try_from(pages.iter().position(|p| *p == pg).expect("known page")).expect("small")
        }),
        stream: ref_after(&text, "/Stm "),
        mcids,
        children: children
            .into_iter()
            .map(|child| observe_struct_elem(pdf, child, map, pages))
            .collect(),
    }
}

fn observe_struct_tree(pdf: &[u8]) -> Option<StructFacts> {
    let catalog = text_of(pdf, root_object(pdf));
    let root = ref_after(&catalog, "/StructTreeRoot ")?;
    let root_text = text_of(pdf, root);
    let map = role_map(&root_text);
    let pages = page_objects(pdf);
    let parent_tree = ref_after(&root_text, "/ParentTree ")
        .map(|obj| parent_tree_nums(&text_of(pdf, obj)))
        .unwrap_or_default();

    Some(StructFacts {
        root,
        children: kids(&root_text)
            .0
            .into_iter()
            .map(|child| observe_struct_elem(pdf, child, &map, &pages))
            .collect(),
        parent_tree: parent_tree
            .into_iter()
            .map(|(key, parents)| ParentTreeEntry { key, parents })
            .collect(),
        role_map: map,
    })
}

fn observe_artifacts(pdf: &[u8]) -> Vec<ArtifactFacts> {
    let mut out = Vec::new();
    for (index, page) in page_objects(pdf).into_iter().enumerate() {
        let content = content_of_page(pdf, page);
        for (at, key) in content.match_indices("/Artifact ") {
            let rest = &content[at + key.len()..];
            let dict = rest
                .starts_with("<<")
                .then(|| &rest[..rest.find(">>").expect("closing dict")]);
            out.push(ArtifactFacts {
                page: u32::try_from(index).expect("small"),
                artifact_type: dict.and_then(|d| name_after(d, "/Type ")),
                subtype: dict.and_then(|d| name_after(d, "/Subtype ")),
            });
        }
    }
    out
}

#[test]
fn header_version_matches_the_first_line() {
    for fixture in all() {
        let built = fixture.build().unwrap();

        assert_eq!(
            header_version(&built.pdf),
            built.expected.header_version,
            "{}",
            fixture.name
        );
    }
    let versions: Vec<String> = all()
        .iter()
        .map(|f| header_version(&f.build().unwrap().pdf))
        .collect();
    assert_eq!(
        versions,
        [
            "1.4", "1.7", "1.7", "1.7", "1.7", "1.4", "1.4", "1.5", "1.7", "1.7", "1.7", "1.7", "1.7", "1.7",
            "1.7", "1.7", "2.0", "1.7", "1.7", "1.7", "1.7", "1.4", "1.5", "1.7", "1.7", "1.7", "1.7",
            // speech-alt-hides .. rolemap-two-hops: only table-scope-pdf2 is PDF 2.0.
            "1.7", "1.7", "1.7", "1.7", "2.0", "1.7", "1.7", "1.7", "1.7"
        ]
    );
}

#[test]
fn revisions_match_the_xref_chain_walked_from_startxref() {
    for fixture in all() {
        let built = fixture.build().unwrap();

        // A deliberately wrong `startxref` cannot be followed; start from the true offset.
        let observed = match built.expected.defects.iter().find_map(|d| match d {
            Defect::StartxrefWrong { actual, .. } => Some(*actual),
            _ => None,
        }) {
            Some(actual) => observe_revisions_from(&built.pdf, usize::try_from(actual).unwrap()),
            None => observe_revisions(&built.pdf),
        };

        assert_eq!(observed, built.expected.revisions, "{}", fixture.name);
        assert_eq!(
            built.expected.revision_count as usize,
            observed.len(),
            "{} revision count",
            fixture.name
        );
    }
}

#[test]
fn incremental_update_lists_only_the_objects_it_touches() {
    let built = find("incremental-update").unwrap().build().unwrap();

    let objects: Vec<Vec<u32>> = observe_revisions(&built.pdf)
        .into_iter()
        .map(|r| r.objects)
        .collect();

    assert_eq!(objects, vec![vec![1, 2, 3, 4, 5], vec![4, 6]]);
    assert_eq!(built.expected.revision_count, 2);
}

#[test]
fn xref_stream_fixture_compresses_four_objects_into_one_stream() {
    let built = find("xref-stream-objstm").unwrap().build().unwrap();

    let [revision] = observe_revisions(&built.pdf).try_into().unwrap();

    assert_eq!(revision.objects, vec![1, 2, 3, 4, 5, 6, 7]);
    let compressed: Vec<(u32, u32, u32)> = revision
        .compressed
        .iter()
        .map(|c| (c.obj, c.stream, c.index))
        .collect();
    assert_eq!(compressed, vec![(1, 6, 0), (2, 6, 1), (3, 6, 2), (5, 6, 3)]);
}

#[test]
fn page_count_matches_the_page_dictionaries_in_the_file() {
    for fixture in all() {
        let built = fixture.build().unwrap();

        assert_eq!(
            count(&built.pdf, b"/Type /Page "),
            built.expected.page_count as usize,
            "{}",
            fixture.name
        );
        assert_eq!(built.expected.pages.len(), built.expected.page_count as usize);
    }
}

#[test]
fn pages_match_boxes_rotation_user_unit_and_mcids_in_the_bytes() {
    let mut checked = Vec::new();
    for fixture in all() {
        let built = fixture.build().unwrap();
        let objects = page_objects(&built.pdf);
        if objects.is_empty() {
            // Pages that live inside an object stream are not visible to the plain scanner.
            checked.push((fixture.name, false));
            continue;
        }
        let observed: Vec<PageFacts> = objects
            .into_iter()
            .enumerate()
            .map(|(i, obj)| observe_page(&built.pdf, u32::try_from(i).unwrap(), obj))
            .collect();

        assert_eq!(observed, built.expected.pages, "{}", fixture.name);
        checked.push((fixture.name, true));
    }

    let skipped: Vec<&str> = checked.iter().filter(|(_, ok)| !ok).map(|(n, _)| *n).collect();
    assert_eq!(skipped, ["xref-stream-objstm"]);
    assert_eq!(checked.len(), 36);
}

#[test]
fn page_boxes_fixture_inherits_media_box_from_the_pages_node() {
    let built = find("page-boxes").unwrap().build().unwrap();

    let second = &built.expected.pages[1];

    assert_eq!(second.media_box, Sourced::inherited([0.0, 0.0, 300.0, 400.0], 2));
    assert_eq!(second.crop_box, None);
    assert_eq!(built.expected.pages[0].rotate, Some(Sourced::own(90, 3)));
    assert_eq!(built.expected.pages[2].user_unit, Some(Sourced::own(2.0, 5)));
}

#[test]
fn struct_tree_matches_the_objects_in_the_bytes() {
    for fixture in all() {
        let built = fixture.build().unwrap();
        if built.expected.struct_tree.is_none() {
            if built.expected.struct_outline.is_some() {
                continue; // rich outlines are checked in `structure_facts`
            }
            assert_eq!(
                count(&built.pdf, b"/StructTreeRoot"),
                0,
                "{} must be untagged",
                fixture.name
            );
            continue;
        }

        let observed = observe_struct_tree(&built.pdf);

        assert_eq!(observed, built.expected.struct_tree, "{}", fixture.name);
    }
}

#[test]
fn tagged_simple_outline_is_document_h1_p() {
    let built = find("tagged-simple").unwrap().build().unwrap();

    let tree = built.expected.struct_tree.unwrap();

    let outline: Vec<(String, Vec<(String, Vec<i32>)>)> = tree
        .children
        .iter()
        .map(|doc| {
            (
                doc.role.clone(),
                doc.children
                    .iter()
                    .map(|c| (c.role.clone(), c.mcids.clone()))
                    .collect(),
            )
        })
        .collect();
    assert_eq!(
        outline,
        vec![(
            "Document".to_owned(),
            vec![("H1".to_owned(), vec![0]), ("P".to_owned(), vec![1])]
        )]
    );
    assert_eq!(built.expected.catalog.lang.as_deref(), Some("en-US"));
    assert_eq!(built.expected.catalog.marked, Some(true));
}

#[test]
fn catalog_flags_match_the_catalog_object() {
    for fixture in all() {
        let built = fixture.build().unwrap();
        let Some(text) = object_text(&built.pdf, root_object(&built.pdf)) else {
            continue; // catalog inside an object stream
        };

        let marked = text.contains("/MarkInfo").then(|| text.contains("/Marked true"));
        let lang = text.find("/Lang (").map(|at| {
            let rest = &text[at + "/Lang (".len()..];
            rest[..rest.find(')').unwrap()].to_owned()
        });

        assert_eq!(marked, built.expected.catalog.marked, "{}", fixture.name);
        assert_eq!(lang, built.expected.catalog.lang, "{}", fixture.name);
        assert_eq!(root_object(&built.pdf), built.expected.catalog.object);
    }
}

#[test]
fn artifacts_match_the_marked_content_in_page_streams() {
    for fixture in all() {
        let built = fixture.build().unwrap();
        if page_objects(&built.pdf).is_empty() {
            continue;
        }

        assert_eq!(
            observe_artifacts(&built.pdf),
            built.expected.artifacts,
            "{}",
            fixture.name
        );
    }

    let artifacts = find("artifacts").unwrap().build().unwrap().expected.artifacts;
    let summary: Vec<(Option<&str>, Option<&str>)> = artifacts
        .iter()
        .map(|a| (a.artifact_type.as_deref(), a.subtype.as_deref()))
        .collect();
    assert_eq!(
        summary,
        vec![
            (Some("Pagination"), Some("Header")),
            (Some("Layout"), None),
            (Some("Pagination"), Some("Footer")),
            (None, None),
        ]
    );
}

#[test]
fn form_xobjects_match_their_dictionaries_content_and_painters() {
    let built = find("form-xobject-nesting").unwrap().build().unwrap();
    let pdf = &built.pdf;

    let observed: Vec<FormFacts> = built
        .expected
        .forms
        .iter()
        .map(|form| {
            let text = text_of(pdf, form.object);
            assert!(text.contains("/Subtype /Form"));
            let painter = text_of(pdf, form.painted_by);
            let painter_content = if painter.contains("/Type /Page ") {
                content_of_page(pdf, form.painted_by)
            } else {
                painter.clone()
            };
            assert!(
                painter.contains(&format!("/{} {} 0 R", form.name, form.object)),
                "resource entry"
            );
            assert!(
                painter_content.contains(&format!("/{} Do", form.name)),
                "Do operator"
            );
            FormFacts {
                object: form.object,
                name: form.name.clone(),
                painted_by: form.painted_by,
                matrix: numbers_after(&text, "/Matrix ").map(|n| [n[0], n[1], n[2], n[3], n[4], n[5]]),
                mcids: mcids_in(&latin1(&stream_data(pdf, form.object))),
            }
        })
        .collect();

    assert_eq!(observed, built.expected.forms);
    // The page paints A, A paints B.
    let chain: Vec<(u32, u32)> = observed.iter().map(|f| (f.object, f.painted_by)).collect();
    assert_eq!(chain, vec![(6, 3), (7, 6)]);
}

#[test]
fn files_are_free_of_timestamps_and_use_the_fixed_id() {
    for fixture in all() {
        let built = fixture.build().unwrap();
        let pdf = built.pdf;

        assert_eq!(count(&pdf, b"CreationDate"), 0, "{}", fixture.name);
        assert_eq!(count(&pdf, b"ModDate"), 0, "{}", fixture.name);
        let ids = count(
            &pdf,
            b"/ID [<70646678726179000000000000000001> <70646678726179000000000000000001>]",
        );
        assert_eq!(
            ids,
            built.expected.revisions.len(),
            "{} one /ID per trailer",
            fixture.name
        );
    }
}

#[test]
fn refs_helper_reads_arrays() {
    assert_eq!(refs_in("/K [8 0 R 9 0 R]"), vec![8, 9]);
}

#[test]
fn hybrid_xref_hides_compressed_objects_from_the_table_and_restores_them_in_the_stream() {
    let built = find("hybrid-xref").unwrap().build().unwrap();
    let pdf = &built.pdf;

    let at = last_startxref(pdf);
    let table = parse_table_full(pdf, at);

    let in_use: Vec<u32> = table.entries.iter().filter(|e| e.in_use).map(|e| e.obj).collect();
    let free: Vec<(u32, usize)> = table
        .entries
        .iter()
        .filter(|e| !e.in_use)
        .map(|e| (e.obj, e.offset))
        .collect();
    // A reader without /XRefStm sees objects 1-5 and the stream (8); 6 and 7 are free. The free
    // entries chain 0 -> 6 -> 7 -> 0, as the free-list head field says.
    assert_eq!(in_use, vec![1, 2, 3, 4, 5, 8]);
    assert_eq!(free, vec![(0, 6), (6, 7), (7, 0)]);
    let stm = table.xref_stm.unwrap();
    assert!(text_of(pdf, 8).starts_with("8 0 obj\n<< /Type /XRef /W [1 4 2] /Index [6 2] /Size 9"));
    assert_eq!(object_offset(pdf, 8), Some(stm));
    // The merged revision lists every object and both compressed members.
    let [revision] = observe_revisions(pdf).try_into().unwrap();
    assert_eq!(revision.objects, vec![1, 2, 3, 4, 5, 6, 7, 8]);
    assert_eq!(
        revision
            .compressed
            .iter()
            .map(|c| (c.obj, c.stream, c.index))
            .collect::<Vec<_>>(),
        vec![(6, 5, 0), (7, 5, 1)]
    );
    assert_eq!(revision.xref_stm, Some(stm as u64));
    assert_eq!(built.expected.revisions, vec![revision]);
    // Smoke: the font the page needs is one of the hidden objects.
    assert!(text_of(pdf, 3).contains("/F1 6 0 R"));
}
