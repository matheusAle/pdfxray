//! Page geometry and resource inheritance, re-derived from the page tree.

use std::collections::BTreeMap;

use pdfxray_fixtures::{
    Expected, PageGeometryFacts, ResourceEntryFacts, ResourceScopeFacts, SharedResourceFacts, all, find,
};
use pretty_assertions::assert_eq;

use crate::common::{
    inherited, int_after, latin1, numbers_after, object_text, page_objects, ref_after, scan_objects,
    stream_data,
};
use crate::content::Operand;
use crate::content::parse_ops;
use crate::pdfobj::{Obj, parse_object};

fn built(name: &str) -> (Vec<u8>, Expected) {
    let b = find(name).unwrap().build().unwrap();
    (b.pdf, b.expected)
}

fn observe_geometry(pdf: &[u8]) -> Vec<PageGeometryFacts> {
    page_objects(pdf)
        .into_iter()
        .enumerate()
        .map(|(index, page)| {
            let rect = |key: &str| {
                let (_, text) = inherited(pdf, page, key)?;
                numbers_after(&text, key)
            };
            let media = rect("/MediaBox ").expect("media box");
            let visible = match rect("/CropBox ") {
                Some(crop) => [
                    crop[0].max(media[0]),
                    crop[1].max(media[1]),
                    crop[2].min(media[2]),
                    crop[3].min(media[3]),
                ],
                None => [media[0], media[1], media[2], media[3]],
            };
            let rotate = inherited(pdf, page, "/Rotate ")
                .and_then(|(_, text)| int_after(&text, "/Rotate "))
                .map_or(0, |r| i32::try_from(r.rem_euclid(360)).unwrap());
            let user_unit = object_text(pdf, page)
                .and_then(|t| crate::common::float_after(&t, "/UserUnit "))
                .unwrap_or(1.0);
            let (w, h) = (visible[2] - visible[0], visible[3] - visible[1]);
            let render_size = if rotate % 180 == 0 { [w, h] } else { [h, w] };
            PageGeometryFacts {
                page: u32::try_from(index).unwrap(),
                visible_box: [visible[0], visible[1], visible[2], visible[3]],
                rotate,
                user_unit,
                render_size,
                physical_size: [render_size[0] * user_unit, render_size[1] * user_unit],
            }
        })
        .collect()
}

#[test]
fn page_geometry_equals_the_visible_box_rotation_and_user_unit_in_the_bytes() {
    let mut checked = Vec::new();
    for fixture in all() {
        let built = fixture.build().unwrap();
        if built.expected.page_geometry.is_empty() {
            continue;
        }

        assert_eq!(
            observe_geometry(&built.pdf),
            built.expected.page_geometry,
            "{}",
            fixture.name
        );
        checked.push(fixture.name);
    }

    assert_eq!(checked, ["box-matrix"]);
}

#[test]
fn box_matrix_sizes_are_pinned() {
    let (pdf, expected) = built("box-matrix");

    let sizes: Vec<([f64; 2], [f64; 2], i32)> = expected
        .page_geometry
        .iter()
        .map(|g| (g.render_size, g.physical_size, g.rotate))
        .collect();

    assert_eq!(
        sizes,
        vec![
            // Crop 0..600 x 0..792 sits inside media -50..650 x -50..842.
            ([600.0, 792.0], [600.0, 792.0], 0),
            // Crop 20..380 x 20..280 = 360 x 260; 180 degrees keeps it.
            ([360.0, 260.0], [360.0, 260.0], 180),
            // Inherited crop 10..240 x 10..290 = 230 x 280; 270 degrees swaps to 280 x 230.
            ([280.0, 230.0], [280.0, 230.0], 270),
            // Same crop, UserUnit 3: 230 * 3 = 690 and 280 * 3 = 840 points.
            ([230.0, 280.0], [690.0, 840.0], 0),
        ]
    );
    // Where the inherited values come from: pages 2 and 3 take the crop box from /Pages (2).
    let crops: Vec<(u32, bool)> = expected
        .pages
        .iter()
        .filter_map(|p| p.crop_box.as_ref().map(|c| (c.node, c.inherited)))
        .collect();
    assert_eq!(crops, [(3, false), (4, false), (2, true), (2, true)]);
    assert_eq!(expected.pages[0].media_box.value, [-50.0, -50.0, 650.0, 842.0]);
    assert_eq!(expected.pages[2].media_box.node, 2);
    // The crop box is inside the media box on every page, so the visible box is the crop box.
    for (page, geometry) in expected.pages.iter().zip(&expected.page_geometry) {
        assert_eq!(
            Some(geometry.visible_box),
            page.crop_box.as_ref().map(|c| c.value)
        );
    }
    assert!(crate::common::text_of(&pdf, 6).contains("/UserUnit 3.0"));
}

// ---------------------------------------------------------------------------------------------
// Resources

type Uses = BTreeMap<(String, String), u32>;

/// Counts `Tf`, `Do`, `gs` and `sh` operands per resource category.
fn count_uses(content: &[u8]) -> Uses {
    let mut uses = Uses::new();
    for op in parse_ops(content) {
        let category = match op.name.as_str() {
            "Tf" => "Font",
            "Do" => "XObject",
            "gs" => "ExtGState",
            "sh" => "Shading",
            _ => continue,
        };
        let Some(Operand::Name(name)) = op.operands.first() else {
            continue;
        };
        *uses.entry((category.to_owned(), name.clone())).or_default() += 1;
    }
    uses
}

/// The categories of `resources` as `(category, name, target)` in file order.
fn entries_of(resources: &Obj, uses: &Uses) -> Vec<ResourceEntryFacts> {
    let Obj::Dict(categories) = resources else {
        panic!("resources must be a dictionary")
    };
    let mut out = Vec::new();
    for (category, value) in categories {
        let Obj::Dict(names) = value else { continue };
        for (name, target) in names {
            out.push(ResourceEntryFacts {
                category: category.clone(),
                name: name.clone(),
                target: target.reference().expect("indirect resource"),
                uses: uses.get(&(category.clone(), name.clone())).copied().unwrap_or(0),
            });
        }
    }
    out
}

fn observe_resources(pdf: &[u8]) -> (Vec<ResourceScopeFacts>, Vec<SharedResourceFacts>) {
    let pages = page_objects(pdf);
    let mut scopes: Vec<ResourceScopeFacts> = Vec::new();
    // For forms: (object, own resources dictionary, content).
    let forms: Vec<(u32, Option<Obj>, Vec<u8>)> = scan_objects(pdf)
        .into_iter()
        .map(|(n, _)| n)
        .filter(|n| object_text(pdf, *n).is_some_and(|t| t.contains("/Subtype /Form")))
        .map(|n| {
            (
                n,
                parse_object(pdf, n).get("Resources").cloned(),
                stream_data(pdf, n),
            )
        })
        .collect();

    for page in &pages {
        let (node, _) = inherited(pdf, *page, "/Resources ").expect("resources");
        let resources = parse_object(pdf, node)
            .get("Resources")
            .cloned()
            .expect("dictionary");
        let content = stream_data(
            pdf,
            ref_after(&object_text(pdf, *page).unwrap(), "/Contents ").unwrap(),
        );
        scopes.push(ResourceScopeFacts {
            owner: *page,
            node,
            inherited: node != *page,
            entries: entries_of(&resources, &count_uses(&content)),
        });
    }
    // A form is painted by the first page scope whose XObject entry for a `Do` names it.
    for (form, own, content) in &forms {
        let uses = count_uses(content);
        let (node, resources) = if let Some(resources) = own {
            (*form, resources.clone())
        } else {
            let painter = scopes
                .iter()
                .find(|s| s.entries.iter().any(|e| e.target == *form && e.uses > 0))
                .expect("a page paints the form");
            let holder = parse_object(pdf, painter.node);
            (
                painter.node,
                holder.get("Resources").cloned().expect("dictionary"),
            )
        };
        scopes.push(ResourceScopeFacts {
            owner: *form,
            node,
            inherited: node != *form,
            entries: entries_of(&resources, &uses),
        });
    }

    let mut by_target: BTreeMap<u32, (Vec<u32>, u32)> = BTreeMap::new();
    for scope in &scopes {
        for entry in &scope.entries {
            let slot = by_target.entry(entry.target).or_default();
            if !slot.0.contains(&scope.owner) {
                slot.0.push(scope.owner);
            }
            slot.1 += entry.uses;
        }
    }
    let shared = by_target
        .into_iter()
        .filter(|(_, (owners, _))| owners.len() > 1)
        .map(|(object, (mut owners, total_uses))| {
            owners.sort_unstable();
            SharedResourceFacts {
                object,
                owners,
                total_uses,
            }
        })
        .collect();
    (scopes, shared)
}

#[test]
fn resource_scopes_and_sharing_equal_what_the_page_tree_says() {
    let mut checked = Vec::new();
    for fixture in all() {
        let built = fixture.build().unwrap();
        if built.expected.resources.is_empty() {
            continue;
        }

        let (scopes, shared) = observe_resources(&built.pdf);

        assert_eq!(scopes, built.expected.resources, "{}", fixture.name);
        assert_eq!(shared, built.expected.shared_resources, "{}", fixture.name);
        checked.push(fixture.name);
    }

    assert_eq!(checked, ["resources-inheritance"]);
}

#[test]
fn resources_inheritance_sources_names_and_counts_are_pinned() {
    let (pdf, expected) = built("resources-inheritance");

    let scopes: Vec<(u32, u32, bool, usize)> = expected
        .resources
        .iter()
        .map(|s| (s.owner, s.node, s.inherited, s.entries.len()))
        .collect();
    let used: Vec<(u32, Vec<String>)> = expected
        .resources
        .iter()
        .map(|s| {
            (
                s.owner,
                s.entries
                    .iter()
                    .filter(|e| e.uses > 0)
                    .map(|e| format!("{}/{}x{}", e.category, e.name, e.uses))
                    .collect(),
            )
        })
        .collect();

    assert_eq!(
        scopes,
        [
            // Pages A and C inherit all seven names from /Pages (object 2).
            (3, 2, true, 7),
            // Page B replaces the dictionary: only its own F1.
            (4, 4, false, 1),
            (5, 2, true, 7),
            // Form 1 has its own dictionary; form 2 uses its painter's (page A -> /Pages).
            (12, 12, false, 1),
            (13, 2, true, 7),
        ]
    );
    assert_eq!(
        used,
        [
            (
                3,
                vec![
                    "Font/F1x2",
                    "Font/F2x1",
                    "ExtGState/GS1x1",
                    "XObject/Fm1x1",
                    "XObject/Fm2x1"
                ]
            ),
            (4, vec!["Font/F1x1"]),
            (5, vec!["Font/F1x1"]),
            (12, vec!["Font/F1x1"]),
            (13, vec!["Font/F2x1"]),
        ]
        .map(|(o, u)| (o, u.into_iter().map(str::to_owned).collect()))
    );
    // Unused names: F3, GS9 on every inheriting scope.
    let unused: Vec<&str> = expected.resources[0]
        .entries
        .iter()
        .filter(|e| e.uses == 0)
        .map(|e| e.name.as_str())
        .collect();
    assert_eq!(unused, ["F3", "GS9"]);
    // Helvetica (9) is shared by A, C, form 1 and form 2: 2 + 1 + 1 + 0 uses. Page B's Times-Bold is not shared.
    let helvetica = expected.shared_resources.iter().find(|s| s.object == 9).unwrap();
    assert_eq!(
        (helvetica.owners.clone(), helvetica.total_uses),
        (vec![3, 5, 12, 13], 4)
    );
    assert!(expected.shared_resources.iter().all(|s| s.object != 14));
    // The text of the file agrees: only /Pages carries F3.
    assert_eq!(latin1(&pdf).matches("/F3 11 0 R").count(), 1);
}

#[test]
fn hayro_resolves_the_same_effective_resources() {
    use hayro::hayro_syntax::Pdf;
    use hayro::hayro_syntax::object::Name;

    let (pdf_bytes, expected) = built("resources-inheritance");
    let pdf = Pdf::new(pdf_bytes).unwrap();

    // Page A inherits F2 from /Pages; page B's own dictionary hides it.
    let sees_f2: Vec<bool> = pdf
        .pages()
        .iter()
        .map(|p| p.resources().get_font(&Name::new(b"F2").unwrap()).is_some())
        .collect();
    let sees_gs9: Vec<bool> = pdf
        .pages()
        .iter()
        .map(|p| {
            p.resources()
                .get_ext_g_state(&Name::new(b"GS9").unwrap())
                .is_some()
        })
        .collect();

    assert_eq!(sees_f2, [true, false, true]);
    assert_eq!(sees_gs9, [true, false, true]);
    assert_eq!(expected.resources.iter().filter(|s| s.owner <= 5).count(), 3);
}
