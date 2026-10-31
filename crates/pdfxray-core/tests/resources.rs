//! Resources panel data: owners, inheritance, names to objects, usage counts and per-category
//! inspector props, checked against the generated fixtures' expected facts and against a
//! synthetic document that exercises every category.

// Helpers here are not `#[test]` functions, so clippy.toml's allow-*-in-tests does not reach them;
// failing loudly is what a test helper should do.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeSet;
use std::fs;

use pdfxray_core::capture::StreamRef;
use pdfxray_core::{Document, NodeId, ObjRef, Props, ResKind, Severity, TreeKind, TreeNode};
use pdfxray_fixtures::{Expected, PdfVersion, PdfWriter, Trailer, fixtures_dir};
use pretty_assertions::assert_eq;

fn fixture(name: &str) -> Document {
    Document::open(pdfxray_fixtures::pdf_path(name)).unwrap()
}

fn facts(name: &str) -> Expected {
    serde_json::from_slice(&fs::read(fixtures_dir().join(format!("{name}.expected.json"))).unwrap()).unwrap()
}

fn obj(n: u32) -> ObjRef {
    ObjRef::new(n, 0)
}

fn res(owner: u32, kind: ResKind, name: &str) -> NodeId {
    NodeId::Resource {
        owner: obj(owner),
        kind,
        name: name.to_owned(),
    }
}

fn find<'t>(tree: &'t [TreeNode], id: &NodeId) -> &'t TreeNode {
    tree.iter()
        .find_map(|n| n.find(id))
        .unwrap_or_else(|| panic!("{id} is not in the tree"))
}

fn props(doc: &Document, id: &NodeId) -> Props {
    doc.node(id).unwrap_or_else(|| panic!("no props for {id}"))
}

fn is_unused(node: &TreeNode) -> bool {
    node.badges
        .iter()
        .any(|b| b.code == "unused-resource" && b.severity == Severity::Info)
}

/// Every value of `key` in `group`, in order.
fn all<'p>(p: &'p Props, group: &str, key: &str) -> Vec<&'p str> {
    p.groups
        .iter()
        .filter(|g| g.name == group)
        .flat_map(|g| g.rows.iter())
        .filter(|r| r.key == key)
        .map(|r| r.value.as_str())
        .collect()
}

/// The link of the first `key` row in `group`.
fn link<'p>(p: &'p Props, group: &str, key: &str) -> Option<&'p NodeId> {
    p.groups
        .iter()
        .find(|g| g.name == group)?
        .rows
        .iter()
        .find(|r| r.key == key)?
        .link
        .as_ref()
}

/// Labels of the children of `node`.
fn labels(node: &TreeNode) -> Vec<&str> {
    node.children.iter().map(|c| c.label.as_str()).collect()
}

/// Names of the entries of `category` under owner node `owner` that carry the unused badge.
fn unused_names(owner: &TreeNode, category: &str) -> Vec<String> {
    owner
        .children
        .iter()
        .filter(|g| g.label == category)
        .flat_map(|g| g.children.iter())
        .filter(|e| is_unused(e))
        .map(|e| e.label.clone())
        .collect()
}

// ---- resources-inheritance ---------------------------------------------------------------

#[test]
fn inheritance_tree_lists_owners_in_document_order() {
    let doc = fixture("resources-inheritance");

    let tree = doc.tree(TreeKind::Resources);

    let top: Vec<(&NodeId, &str, Option<&str>)> = tree
        .iter()
        .map(|n| (&n.id, n.label.as_str(), n.detail.as_deref()))
        .collect();
    assert_eq!(
        top,
        vec![
            (&NodeId::Page(0), "Page 1 resources", Some("inherited from 2 0 R")),
            (&NodeId::Page(1), "Page 2 resources", None),
            (&NodeId::Page(2), "Page 3 resources", Some("inherited from 2 0 R")),
            (&NodeId::Object(obj(12)), "Form /Fm1 (12 0 R)", None),
            (&NodeId::Group(200), "Shared", Some("7")),
        ]
    );
    // Smoke: the tree is not trivially empty and every owner has category groups.
    assert!(tree.len() > 3);
    assert_eq!(labels(&tree[0]), ["Font", "XObject", "ExtGState"]);
    assert_eq!(labels(&tree[1]), ["Font"]);
}

#[test]
fn category_groups_have_unique_numbers_from_201() {
    let doc = fixture("resources-inheritance");
    let tree = doc.tree(TreeKind::Resources);

    let groups: Vec<u32> = tree
        .iter()
        .filter(|owner| owner.id != NodeId::Group(200))
        .flat_map(|owner| owner.children.iter())
        .map(|g| match g.id {
            NodeId::Group(n) => n,
            ref other => panic!("category group has id {other}"),
        })
        .collect();

    // Pages 1 and 3: Font, XObject, ExtGState each; page 2: Font; form 1: Font.
    assert_eq!(groups, (201..209).collect::<Vec<_>>());
    assert_eq!(groups.iter().collect::<BTreeSet<_>>().len(), groups.len());
}

#[test]
fn page_entries_show_names_targets_and_summaries() {
    let doc = fixture("resources-inheritance");
    let tree = doc.tree(TreeKind::Resources);
    let page1 = find(&tree, &NodeId::Page(0));

    let entries: Vec<(&str, Option<&str>)> = page1
        .children
        .iter()
        .flat_map(|g| g.children.iter())
        .map(|e| (e.label.as_str(), e.detail.as_deref()))
        .collect();

    assert_eq!(
        entries,
        vec![
            ("/F1", Some("Type1 Helvetica, not embedded · 2 uses")),
            // Page 1 selects /F2 itself and form 13 does too (it resolves /F2 in the page's resources).
            ("/F2", Some("Type1 Times-Roman, not embedded · 2 uses")),
            ("/F3", Some("Type1 Courier, not embedded")),
            ("/Fm1", Some("Form, BBox [0 0 100 40] · 1 use")),
            ("/Fm2", Some("Form, BBox [0 0 100 40] · 1 use")),
            ("/GS1", Some("ca · 1 use")),
            ("/GS9", Some("CA")),
        ]
    );
    // Smoke: the group detail counts its entries.
    assert_eq!(page1.children[0].detail.as_deref(), Some("3"));
}

#[test]
fn entries_match_the_fixture_facts_including_usage() {
    let doc = fixture("resources-inheritance");
    let expected = facts("resources-inheritance");
    let tree = doc.tree(TreeKind::Resources);

    // Forms without /Resources are not owners: what they use counts for their painter.
    let folded: Vec<(u32, &_)> = expected
        .forms
        .iter()
        .filter_map(|f| {
            let scope = expected
                .resources
                .iter()
                .find(|s| s.owner == f.object && s.inherited)?;
            Some((f.painted_by, scope))
        })
        .collect();
    assert_eq!(folded.len(), 1, "only form 13 lacks its own resources");

    let mut checked = 0;
    for scope in &expected.resources {
        // Owners with a node in the tree: pages and forms with their own dictionary.
        let owner_id = match expected.pages.iter().position(|p| p.object == scope.owner) {
            Some(i) => NodeId::Page(u32::try_from(i).unwrap()),
            None if !scope.inherited => NodeId::Object(obj(scope.owner)),
            None => continue,
        };
        let node = find(&tree, &owner_id);
        assert_eq!(
            node.detail.is_some(),
            scope.inherited,
            "inheritance of owner {}",
            scope.owner
        );
        for entry in &scope.entries {
            let kind = ResKind::ALL
                .into_iter()
                .find(|k| k.key() == entry.category)
                .unwrap();
            let id = res(scope.owner, kind, &entry.name);
            let extra: u32 = folded
                .iter()
                .filter(|(painter, _)| *painter == scope.owner)
                .flat_map(|(_, s)| s.entries.iter())
                .filter(|e| e.category == entry.category && e.name == entry.name)
                .map(|e| e.uses)
                .sum();
            let want_uses = entry.uses + extra;

            let p = props(&doc, &id);
            assert_eq!(
                p.get("Resource", "Object"),
                Some(format!("{} 0 R", entry.target).as_str()),
                "{id}"
            );
            assert_eq!(
                p.get("Usage", "Count"),
                Some(want_uses.to_string().as_str()),
                "{id}"
            );
            assert_eq!(is_unused(find(&tree, &id)), want_uses == 0, "badge of {id}");
            checked += 1;
        }
    }
    // 7 + 1 + 7 + 1 entries.
    assert_eq!(checked, 16);
}

#[test]
fn unused_resources_carry_the_badge() {
    let doc = fixture("resources-inheritance");
    let tree = doc.tree(TreeKind::Resources);

    assert_eq!(unused_names(find(&tree, &NodeId::Page(0)), "Font"), ["/F3"]);
    assert_eq!(unused_names(find(&tree, &NodeId::Page(0)), "ExtGState"), ["/GS9"]);
    assert_eq!(
        unused_names(find(&tree, &NodeId::Page(0)), "XObject"),
        Vec::<String>::new()
    );
    assert_eq!(
        unused_names(find(&tree, &NodeId::Page(1)), "Font"),
        Vec::<String>::new()
    );
    let page3 = find(&tree, &NodeId::Page(2));
    assert_eq!(unused_names(page3, "Font"), ["/F2", "/F3"]);
    assert_eq!(unused_names(page3, "ExtGState"), ["/GS1", "/GS9"]);
    assert_eq!(unused_names(page3, "XObject"), ["/Fm1", "/Fm2"]);
    // Smoke: the badge is the only marker used on entries.
    let badge = &find(&tree, &res(3, ResKind::Font, "F3")).badges;
    assert_eq!(badge.len(), 1);
}

#[test]
fn inherited_resource_names_its_source() {
    let doc = fixture("resources-inheritance");

    let inherited = props(&doc, &res(3, ResKind::Font, "F1"));
    assert_eq!(inherited.title, "Font /F1");
    assert_eq!(inherited.get("Resource", "Name"), Some("/F1"));
    assert_eq!(inherited.get("Resource", "Category"), Some("Font"));
    assert_eq!(inherited.get("Resource", "Owner"), Some("Page 1"));
    assert_eq!(link(&inherited, "Resource", "Owner"), Some(&NodeId::Page(0)));
    assert_eq!(inherited.get("Resource", "Defined"), Some("inherited from 2 0 R"));
    assert_eq!(
        link(&inherited, "Resource", "Defined"),
        Some(&NodeId::Object(obj(2)))
    );
    assert_eq!(inherited.get("Resource", "Object"), Some("9 0 R"));
    assert_eq!(
        link(&inherited, "Resource", "Object"),
        Some(&NodeId::Object(obj(9)))
    );

    let own = props(&doc, &res(4, ResKind::Font, "F1"));
    assert_eq!(own.get("Resource", "Owner"), Some("Page 2"));
    assert_eq!(own.get("Resource", "Defined"), Some("directly"));
    assert_eq!(own.get("Resource", "Object"), Some("14 0 R"));
    assert_eq!(own.get("Font", "BaseFont"), Some("Times-Bold"));
    // Same name, different object: the override replaced the whole dictionary.
    assert_eq!(inherited.get("Font", "BaseFont"), Some("Helvetica"));
}

#[test]
fn form_with_own_resources_is_an_owner_and_form_without_falls_back_to_the_painter() {
    let doc = fixture("resources-inheritance");
    let tree = doc.tree(TreeKind::Resources);

    // Form 12 has its own dictionary; form 13 does not and is not an owner.
    let owner_ids: Vec<&NodeId> = tree.iter().map(|n| &n.id).collect();
    assert!(owner_ids.contains(&&NodeId::Object(obj(12))));
    assert!(!owner_ids.contains(&&NodeId::Object(obj(13))));
    let form1 = props(&doc, &res(12, ResKind::Font, "F1"));
    assert_eq!(form1.get("Resource", "Owner"), Some("Form 12 0 R"));
    assert_eq!(link(&form1, "Resource", "Owner"), Some(&NodeId::Object(obj(12))));
    assert_eq!(form1.get("Resource", "Defined"), Some("directly"));
    assert_eq!(form1.get("Usage", "Count"), Some("1"));

    // Page 1 shows F2 twice: its own Tf and the Tf inside form 13, which resolves /F2 in the
    // page's resources.
    let f2 = props(&doc, &res(3, ResKind::Font, "F2"));
    assert_eq!(f2.get("Usage", "Count"), Some("2"));
    let captured = doc.captured(0).unwrap();
    let inside_form13 = f2
        .groups
        .iter()
        .find(|g| g.name == "Usage")
        .unwrap()
        .rows
        .iter()
        .filter_map(|r| match r.link.as_ref()? {
            NodeId::Op { page: 0, ix } => captured.ops.get(*ix as usize),
            _ => None,
        })
        .filter(|op| matches!(&op.stream, StreamRef::FormXObject { obj: Some(o), .. } if *o == obj(13)))
        .count();
    assert_eq!(inside_form13, 1);
}

#[test]
fn usage_links_point_at_the_operators() {
    let doc = fixture("resources-inheritance");
    let p = props(&doc, &res(3, ResKind::Font, "F1"));

    let usage = p.groups.iter().find(|g| g.name == "Usage").unwrap();

    assert_eq!(usage.rows[0].key, "Count");
    assert_eq!(usage.rows[0].value, "2");
    let links: Vec<(&str, &NodeId)> = usage
        .rows
        .iter()
        .skip(1)
        .map(|r| (r.key.as_str(), r.link.as_ref().unwrap()))
        .collect();
    // Page A's content: BT /F1 14 Tf is operator 1, BT /F1 10 Tf is operator 11.
    assert_eq!(
        links,
        vec![
            ("Tf", &NodeId::Op { page: 0, ix: 1 }),
            ("Tf", &NodeId::Op { page: 0, ix: 11 })
        ]
    );
    assert_eq!(usage.rows[1].value, "/F1 14 (page 1, op 1)");
    assert_eq!(usage.rows[2].value, "/F1 10 (page 1, op 11)");
}

#[test]
fn shared_group_lists_objects_used_by_several_owners() {
    let doc = fixture("resources-inheritance");
    let expected = facts("resources-inheritance");
    let tree = doc.tree(TreeKind::Resources);
    let shared = find(&tree, &NodeId::Group(200));

    let got: Vec<(&NodeId, &str, Option<&str>)> = shared
        .children
        .iter()
        .map(|c| (&c.id, c.label.as_str(), c.detail.as_deref()))
        .collect();

    assert_eq!(
        got[0],
        (
            &NodeId::Object(obj(9)),
            "Font 9 0 R",
            Some("Type1 Helvetica, not embedded; used by 3: Page 1, Page 3, Form /Fm1 · 4 uses")
        )
    );
    assert_eq!(
        got[6],
        (
            &NodeId::Object(obj(16)),
            "ExtGState 16 0 R",
            Some("CA; used by 2: Page 1, Page 3 · 0 uses")
        )
    );
    // The same objects, with the same total uses, as the generator recorded.
    let want: Vec<(u32, u32)> = expected
        .shared_resources
        .iter()
        .map(|s| (s.object, s.total_uses))
        .collect();
    let have: Vec<(u32, u32)> = shared
        .children
        .iter()
        .map(|c| {
            let NodeId::Object(o) = c.id else {
                panic!("{}", c.id)
            };
            let uses = c
                .detail
                .as_deref()
                .and_then(|d| d.rsplit_once(" · "))
                .and_then(|(_, u)| u.strip_suffix(" uses"))
                .and_then(|n| n.parse().ok())
                .unwrap();
            (o.num, uses)
        })
        .collect();
    assert_eq!(have, want);
    // Font 14 belongs to page 2 alone.
    assert!(shared.children.iter().all(|c| c.id != NodeId::Object(obj(14))));
}

// ---- other fixtures ----------------------------------------------------------------------

#[test]
fn image_props_come_from_the_image_dictionary() {
    let doc = fixture("images");
    let expected = facts("images");
    let image = expected
        .images
        .iter()
        .find(|i| i.name.as_deref() == Some("Im1"))
        .unwrap();

    let p = props(&doc, &res(3, ResKind::XObject, "Im1"));

    assert_eq!(p.title, "XObject /Im1");
    assert_eq!(p.get("Resource", "Object"), Some("5 0 R"));
    assert_eq!(p.get("Image", "Width"), Some(image.width.to_string().as_str()));
    assert_eq!(p.get("Image", "Height"), Some(image.height.to_string().as_str()));
    assert_eq!(
        p.get("Image", "BitsPerComponent"),
        Some(image.bits_per_component.to_string().as_str())
    );
    assert_eq!(p.get("Image", "ColorSpace"), Some(image.color_space.as_str()));
    assert_eq!(p.get("Image", "Components"), Some("3"));
    assert_eq!(p.get("Image", "ImageMask"), Some("no"));
    assert_eq!(p.get("Image", "SMask"), Some("no"));
    assert_eq!(p.get("Image", "Filter"), None);
    assert_eq!(p.get("Image", "Encoded size"), Some("12 bytes"));
    assert_eq!(p.get("Image", "Decoded size"), Some("12 bytes"));
    // The page paints the image once; form 6 paints it again from its own resources.
    assert_eq!(p.get("Usage", "Count"), Some("1"));
    assert_eq!(
        props(&doc, &res(6, ResKind::XObject, "Im1")).get("Usage", "Count"),
        Some("1")
    );
}

#[test]
fn image_and_form_entries_are_summarised_in_the_tree() {
    let doc = fixture("images");
    let tree = doc.tree(TreeKind::Resources);

    let page = find(&tree, &NodeId::Page(0));
    let xobjects = &page.children[0];
    assert_eq!(xobjects.label, "XObject");
    let rows: Vec<(&str, Option<&str>)> = xobjects
        .children
        .iter()
        .map(|e| (e.label.as_str(), e.detail.as_deref()))
        .collect();
    assert_eq!(
        rows,
        vec![
            ("/Im1", Some("2×2, 8 bpc, DeviceRGB · 1 use")),
            ("/Fm1", Some("Form, BBox [0 0 100 100] · 1 use")),
        ]
    );
    assert_eq!(find(&tree, &NodeId::Object(obj(6))).label, "Form /Fm1 (6 0 R)");
    // The form's own copy of the name is a separate resource with its own count.
    assert!(!is_unused(find(&tree, &res(6, ResKind::XObject, "Im1"))));
}

#[test]
fn form_props_describe_bbox_matrix_and_resources() {
    let doc = fixture("images");

    let p = props(&doc, &res(3, ResKind::XObject, "Fm1"));

    assert_eq!(p.get("Form", "BBox"), Some("[0 0 100 100]"));
    assert_eq!(p.get("Form", "Matrix"), Some("[1 0 0 1 200 20]"));
    assert_eq!(p.get("Form", "Resources"), Some("own (XObject)"));
    assert_eq!(p.get("Form", "Group"), None);
    assert_eq!(p.get("Usage", "Count"), Some("1"));
}

#[test]
fn type3_font_props_and_owner() {
    let doc = fixture("type3-font");
    let tree = doc.tree(TreeKind::Resources);

    // The font carries an (empty) /Resources: it is an owner without categories.
    let owner = find(&tree, &NodeId::Object(obj(5)));
    assert_eq!(owner.label, "Type 3 font /T3 (5 0 R)");
    assert_eq!(owner.children, vec![]);

    let p = props(&doc, &res(3, ResKind::Font, "T3"));

    assert_eq!(p.get("Font", "Subtype"), Some("Type3"));
    assert_eq!(p.get("Font", "Embedded"), Some("n/a (Type 3 glyph procedures)"));
    assert_eq!(p.get("Font", "Encoding"), Some("dictionary (2 differences)"));
    assert_eq!(p.get("Font", "FontMatrix"), Some("[0.001 0 0 0.001 0 0]"));
    assert_eq!(p.get("Font", "FontBBox"), Some("[0 0 1000 1000]"));
    assert_eq!(p.get("Font", "CharProcs"), Some("2 glyph procedures"));
    assert_eq!(
        p.get("Font", "Widths"),
        Some("2 entries, FirstChar 97, LastChar 98")
    );
    assert_eq!(p.get("Font", "Resources"), Some("own (0 categories)"));
    // Two text operators select /T3; Type 3 glyph procedures are not walked.
    assert_eq!(p.get("Usage", "Count"), Some("2"));
    assert_eq!(
        all(&p, "Font", "Warning"),
        ["no /ToUnicode map: text extraction depends on the encoding alone"]
    );
    assert_eq!(
        find(&tree, &res(3, ResKind::Font, "T3")).detail.as_deref(),
        Some("Type3, 2 glyphs · 2 uses")
    );
    // Smoke: the font's own (empty) dictionary has nothing to claim unused.
    assert_eq!(owner.badges, vec![]);
}

#[test]
fn text_state_lists_both_fonts_with_their_base_fonts() {
    let doc = fixture("text-state");
    let expected = facts("text-state");
    let tree = doc.tree(TreeKind::Resources);

    let page = find(&tree, &NodeId::Page(0));
    let fonts = &page.children[0];

    let rows: Vec<&str> = fonts.children.iter().map(|e| e.label.as_str()).collect();
    assert_eq!(rows, ["/F1", "/F2"]);
    let bases: BTreeSet<&str> = expected
        .text_ops
        .iter()
        .filter_map(|t| t.base_font.as_deref())
        .collect();
    assert_eq!(bases, BTreeSet::from(["Helvetica", "Times-Roman"]));
    assert_eq!(
        props(&doc, &res(3, ResKind::Font, "F1")).get("Font", "BaseFont"),
        Some("Helvetica")
    );
    assert_eq!(
        props(&doc, &res(3, ResKind::Font, "F2")).get("Font", "BaseFont"),
        Some("Times-Roman")
    );
    assert!(fonts.children.iter().all(|e| !is_unused(e)));
}

#[test]
fn graphics_state_lists_every_extgstate_key() {
    let doc = fixture("graphics-state");

    let p = props(&doc, &res(3, ResKind::ExtGState, "GS1"));

    // The dictionary is written inline in the page's /Resources.
    assert_eq!(p.get("Resource", "Object"), Some("direct (inline)"));
    assert_eq!(p.get("ExtGState", "/Type"), Some("/ExtGState"));
    assert_eq!(p.get("ExtGState", "/CA"), Some("0.5"));
    assert_eq!(p.get("ExtGState", "/ca"), Some("0.5"));
    assert_eq!(p.get("ExtGState", "/BM"), Some("/Multiply"));
    let keys: Vec<&str> = p
        .groups
        .iter()
        .find(|g| g.name == "ExtGState")
        .unwrap()
        .rows
        .iter()
        .map(|r| r.key.as_str())
        .collect();
    assert_eq!(keys, ["/Type", "/CA", "/ca", "/BM"]);
    assert_eq!(p.get("Usage", "Count"), Some("1"));

    let tree = doc.tree(TreeKind::Resources);
    assert_eq!(
        find(&tree, &res(3, ResKind::ExtGState, "GS1")).detail.as_deref(),
        Some("CA ca BM · 1 use")
    );
    let shading = props(&doc, &res(3, ResKind::Shading, "Sh1"));
    assert_eq!(shading.get("Shading", "ShadingType"), Some("2 (axial)"));
    assert_eq!(shading.get("Shading", "ColorSpace"), Some("DeviceRGB"));
    assert_eq!(shading.get("Shading", "Coords"), Some("[200 270 350 270]"));
    assert_eq!(shading.get("Shading", "Extend"), Some("[true true]"));
    assert_eq!(shading.get("Shading", "Function"), Some("type 2"));
    assert_eq!(shading.get("Usage", "Count"), Some("1"));
}

#[test]
fn unknown_nodes_have_no_props() {
    let doc = fixture("resources-inheritance");

    assert!(doc.node(&res(3, ResKind::Font, "Nope")).is_none());
    assert!(doc.node(&res(3, ResKind::Pattern, "F1")).is_none());
    assert!(doc.node(&res(999, ResKind::Font, "F1")).is_none());
    // Smoke: the real one exists.
    assert!(doc.node(&res(3, ResKind::Font, "F1")).is_some());
}

#[test]
fn documents_without_resources_give_an_empty_tree() {
    let doc = Document::from_bytes("x.pdf", b"%PDF-1.7\n".to_vec()).unwrap();

    assert_eq!(doc.tree(TreeKind::Resources), Vec::<TreeNode>::new());
}

// ---- synthetic document: every category ---------------------------------------------------

/// Zlib stream holding the stored bytes `01 02 03 04`.
const ZLIB_1234: [u8; 15] = [
    0x78, 0x01, 0x01, 0x04, 0x00, 0xFB, 0xFF, 0x01, 0x02, 0x03, 0x04, 0x00, 0x18, 0x00, 0x0B,
];

const RICH_CONTENT: &str = "\
BT /F1 12 Tf 10 10 Td (hi) Tj ET
BT /F2 12 Tf 10 30 Td <0001> Tj ET
BT /F3 12 Tf 10 50 Td (x) Tj ET
/CS1 cs 1 0 0 sc
/Pattern cs /P1 scn 0 0 20 20 re f
/Sh1 sh
/GS1 gs
/OC /MC0 BDC EMC
q 10 0 0 10 0 0 cm /Im1 Do Q";

fn rich() -> Document {
    let mut w = PdfWriter::new(PdfVersion::V1_7);
    w.object(1, "<< /Type /Catalog /Pages 2 0 R >>");
    w.object(2, "<< /Type /Pages /Kids [3 0 R] /Count 1 >>");
    w.object(
        3,
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Contents 4 0 R /Resources << \
         /Font << /F1 5 0 R /F2 6 0 R /F3 7 0 R /F9 7 0 R >> \
         /ColorSpace << /CS1 [/ICCBased 14 0 R] /CS2 [/Indexed /DeviceRGB 1 <FF000000FF00>] \
         /CS3 [/Separation /Spot /DeviceCMYK << /FunctionType 2 /Domain [0 1] /C0 [0 0 0 0] /C1 [0 1 1 0] /N 1 >>] >> \
         /Pattern << /P1 15 0 R /P2 16 0 R >> /Shading << /Sh1 17 0 R >> /ExtGState << /GS1 18 0 R >> \
         /Properties << /MC0 19 0 R >> /XObject << /Im1 20 0 R /Im2 21 0 R >> >> >>",
    );
    w.stream(4, "", RICH_CONTENT.as_bytes());
    w.object(
        5,
        "<< /Type /Font /Subtype /TrueType /BaseFont /ABCDEF+Arial-Bold /FirstChar 32 /LastChar 34 \
         /Widths [278 333] /FontDescriptor 8 0 R /ToUnicode 9 0 R /Encoding /WinAnsiEncoding >>",
    );
    w.object(
        6,
        "<< /Type /Font /Subtype /Type0 /BaseFont /XYZABC+Noto /Encoding /Identity-H /DescendantFonts [11 0 R] >>",
    );
    w.object(7, "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>");
    w.object(
        8,
        "<< /Type /FontDescriptor /FontName /ABCDEF+Arial-Bold /Flags 32 /FontBBox [-628 -376 2000 1018] \
         /ItalicAngle 0 /Ascent 905 /Descent -212 /CapHeight 716 /StemV 80 /FontFile2 10 0 R >>",
    );
    w.stream(9, "", b"/CIDInit /ProcSet findresource begin");
    w.stream(10, "/Length1 4", b"abcd");
    w.object(
        11,
        "<< /Type /Font /Subtype /CIDFontType2 /BaseFont /XYZABC+Noto \
         /CIDSystemInfo << /Registry (Adobe) /Ordering (Identity) /Supplement 0 >> \
         /FontDescriptor 12 0 R /CIDToGIDMap /Identity /DW 1000 >>",
    );
    w.object(
        12,
        "<< /Type /FontDescriptor /FontName /XYZABC+Noto /Flags 4 /FontBBox [0 0 1000 1000] \
         /ItalicAngle 0 /Ascent 900 /Descent -200 /CapHeight 700 /StemV 80 /FontFile3 13 0 R >>",
    );
    w.stream(13, "/Subtype /CIDFontType0C", b"abcd");
    w.stream(14, "/N 3 /Alternate /DeviceRGB", b"icc");
    w.stream(
        15,
        "/Type /Pattern /PatternType 1 /PaintType 1 /TilingType 1 /BBox [0 0 10 10] /XStep 10 /YStep 10 \
         /Resources << /ExtGState << /GP 18 0 R >> >>",
        b"/GP gs 0 0 5 5 re f",
    );
    w.object(16, "<< /Type /Pattern /PatternType 2 /Shading 17 0 R >>");
    w.object(
        17,
        "<< /ShadingType 2 /ColorSpace /DeviceRGB /Coords [0 0 100 0] \
         /Function << /FunctionType 2 /Domain [0 1] /C0 [1 0 0] /C1 [0 0 1] /N 1 >> >>",
    );
    w.object(18, "<< /Type /ExtGState /CA 0.5 /ca 0.25 /BM /Multiply >>");
    w.object(19, "<< /Type /OCG /Name (Layer 1) >>");
    w.stream(
        20,
        "/Type /XObject /Subtype /Image /Width 2 /Height 2 /ColorSpace /DeviceGray /BitsPerComponent 8 \
         /Filter /FlateDecode /SMask 22 0 R /Interpolate true",
        &ZLIB_1234,
    );
    w.stream(
        21,
        "/Type /XObject /Subtype /Image /Width 8 /Height 8 /ImageMask true /BitsPerComponent 1",
        &[0xAA; 8],
    );
    w.stream(
        22,
        "/Type /XObject /Subtype /Image /Width 2 /Height 2 /ColorSpace /DeviceGray /BitsPerComponent 8",
        &[0xFF; 4],
    );
    w.finish_table(&Trailer::new(1)).unwrap();
    Document::from_bytes("rich.pdf", w.into_output().unwrap().bytes).unwrap()
}

#[test]
fn rich_page_summaries_cover_every_category() {
    let doc = rich();
    let tree = doc.tree(TreeKind::Resources);
    let page = find(&tree, &NodeId::Page(0));

    let summary = |group: &str| -> Vec<(String, String)> {
        page.children
            .iter()
            .find(|g| g.label == group)
            .unwrap_or_else(|| panic!("no {group} group"))
            .children
            .iter()
            .map(|e| (e.label.clone(), e.detail.clone().unwrap_or_default()))
            .collect()
    };
    let pair = |a: &str, b: &str| (a.to_owned(), b.to_owned());

    assert_eq!(
        labels(page),
        [
            "Font",
            "XObject",
            "ColorSpace",
            "Pattern",
            "Shading",
            "ExtGState",
            "Properties"
        ]
    );
    assert_eq!(
        summary("Font"),
        vec![
            pair("/F1", "TrueType ABCDEF+Arial-Bold, embedded · 1 use"),
            pair("/F2", "Type0 XYZABC+Noto, embedded · 1 use"),
            pair("/F3", "Type1 Helvetica, not embedded · 1 use"),
            pair("/F9", "Type1 Helvetica, not embedded"),
        ]
    );
    assert_eq!(
        summary("XObject"),
        vec![
            pair("/Im1", "2×2, 8 bpc, DeviceGray, FlateDecode · 1 use"),
            pair("/Im2", "8×8, image mask, 1 bpc"),
        ]
    );
    assert_eq!(
        summary("ColorSpace"),
        vec![
            pair("/CS1", "ICCBased (N=3, Alternate=DeviceRGB) · 1 use"),
            pair("/CS2", "Indexed (Base=DeviceRGB, HiVal=1)"),
            pair("/CS3", "Separation (Colorant=/Spot, Alternate=DeviceCMYK)"),
        ]
    );
    assert_eq!(
        summary("Pattern"),
        vec![
            pair("/P1", "Tiling, coloured · 1 use"),
            pair("/P2", "Shading pattern, axial (type 2), DeviceRGB"),
        ]
    );
    assert_eq!(
        summary("Shading"),
        vec![pair("/Sh1", "axial (type 2), DeviceRGB · 1 use")]
    );
    assert_eq!(summary("ExtGState"), vec![pair("/GS1", "CA ca BM · 1 use")]);
    assert_eq!(summary("Properties"), vec![pair("/MC0", "OCG (Layer 1) · 1 use")]);
    // The tiling pattern owns resources of its own.
    assert_eq!(
        find(&tree, &NodeId::Object(obj(15))).label,
        "Pattern /P1 (15 0 R)"
    );
    // Smoke: unused ones.
    assert_eq!(unused_names(page, "Font"), ["/F9"]);
    assert_eq!(unused_names(page, "ColorSpace"), ["/CS2", "/CS3"]);
    assert_eq!(unused_names(page, "Pattern"), ["/P2"]);
    assert_eq!(unused_names(page, "XObject"), ["/Im2"]);
}

#[test]
fn embedded_subset_truetype_font_props_and_widths_warning() {
    let doc = rich();

    let p = props(&doc, &res(3, ResKind::Font, "F1"));

    assert_eq!(p.get("Font", "Subtype"), Some("TrueType"));
    assert_eq!(p.get("Font", "BaseFont"), Some("ABCDEF+Arial-Bold"));
    assert_eq!(p.get("Font", "Encoding"), Some("/WinAnsiEncoding"));
    assert_eq!(p.get("Font", "Embedded"), Some("yes"));
    assert_eq!(p.get("Font", "Font program"), Some("FontFile2 (10 0 R)"));
    assert_eq!(link(&p, "Font", "Font program"), Some(&NodeId::Object(obj(10))));
    assert_eq!(p.get("Font", "Subset"), Some("ABCDEF"));
    assert_eq!(p.get("Font", "ToUnicode"), Some("yes (9 0 R)"));
    assert_eq!(link(&p, "Font", "ToUnicode"), Some(&NodeId::Object(obj(9))));
    assert_eq!(
        p.get("Font", "Widths"),
        Some("2 entries, FirstChar 32, LastChar 34")
    );
    assert_eq!(
        all(&p, "Font", "Warning"),
        ["/Widths has 2 entries but FirstChar 32..LastChar 34 spans 3"]
    );
    assert_eq!(p.get("Font descriptor", "Object"), Some("8 0 R"));
    assert_eq!(p.get("Font descriptor", "FontName"), Some("ABCDEF+Arial-Bold"));
    assert_eq!(p.get("Font descriptor", "Flags"), Some("32 (Nonsymbolic)"));
    assert_eq!(p.get("Font descriptor", "Ascent"), Some("905"));
    assert_eq!(p.get("Font descriptor", "Descent"), Some("-212"));
    assert_eq!(p.get("Font descriptor", "CapHeight"), Some("716"));
    assert_eq!(
        p.get("Font descriptor", "FontBBox"),
        Some("[-628 -376 2000 1018]")
    );
    assert_eq!(p.get("Font descriptor", "ItalicAngle"), Some("0"));
    assert_eq!(p.get("Usage", "Count"), Some("1"));
}

#[test]
fn type0_font_reads_its_descendant() {
    let doc = rich();

    let p = props(&doc, &res(3, ResKind::Font, "F2"));

    assert_eq!(p.get("Font", "Subtype"), Some("Type0"));
    assert_eq!(p.get("Font", "Encoding"), Some("/Identity-H"));
    assert_eq!(p.get("Font", "Embedded"), Some("yes"));
    assert_eq!(
        p.get("Font", "Font program"),
        Some("FontFile3 /CIDFontType0C (13 0 R)")
    );
    assert_eq!(p.get("Font", "Subset"), Some("XYZABC"));
    assert_eq!(p.get("Font", "Widths"), None);
    assert_eq!(p.get("Descendant font", "Object"), Some("11 0 R"));
    assert_eq!(p.get("Descendant font", "Subtype"), Some("CIDFontType2"));
    assert_eq!(p.get("Descendant font", "BaseFont"), Some("XYZABC+Noto"));
    assert_eq!(
        p.get("Descendant font", "CIDSystemInfo"),
        Some("Adobe-Identity-0")
    );
    assert_eq!(p.get("Descendant font", "CIDToGIDMap"), Some("/Identity"));
    assert_eq!(p.get("Descendant font", "DW"), Some("1000"));
    assert_eq!(p.get("Font descriptor", "Flags"), Some("4 (Symbolic)"));
    assert_eq!(
        all(&p, "Font", "Warning"),
        ["no /ToUnicode map: text extraction depends on the encoding alone"]
    );
}

#[test]
fn unembedded_font_without_tounicode_is_flagged() {
    let doc = rich();

    let p = props(&doc, &res(3, ResKind::Font, "F3"));

    assert_eq!(p.get("Font", "Embedded"), Some("no"));
    assert_eq!(p.get("Font", "Font program"), None);
    assert_eq!(p.get("Font", "Subset"), Some("no"));
    assert_eq!(p.get("Font", "ToUnicode"), Some("no"));
    assert_eq!(
        all(&p, "Font", "Warning"),
        [
            "font program is not embedded",
            "no /ToUnicode map: text extraction depends on the encoding alone"
        ]
    );
    assert!(p.groups.iter().all(|g| g.name != "Font descriptor"));
    // /F9 names the same object but is never selected.
    let alias = props(&doc, &res(3, ResKind::Font, "F9"));
    assert_eq!(alias.get("Resource", "Object"), Some("7 0 R"));
    assert_eq!(alias.get("Usage", "Count"), Some("0"));
}

#[test]
fn image_props_cover_filters_masks_and_decoded_size() {
    let doc = rich();

    let p = props(&doc, &res(3, ResKind::XObject, "Im1"));
    assert_eq!(p.get("Image", "Width"), Some("2"));
    assert_eq!(p.get("Image", "Height"), Some("2"));
    assert_eq!(p.get("Image", "BitsPerComponent"), Some("8"));
    assert_eq!(p.get("Image", "ColorSpace"), Some("DeviceGray"));
    assert_eq!(p.get("Image", "Filter"), Some("FlateDecode"));
    assert_eq!(p.get("Image", "ImageMask"), Some("no"));
    assert_eq!(p.get("Image", "SMask"), Some("yes (22 0 R)"));
    assert_eq!(link(&p, "Image", "SMask"), Some(&NodeId::Object(obj(22))));
    assert_eq!(p.get("Image", "Interpolate"), Some("true"));
    assert_eq!(p.get("Image", "Encoded size"), Some("15 bytes"));
    assert_eq!(p.get("Image", "Decoded size"), Some("4 bytes"));

    let mask = props(&doc, &res(3, ResKind::XObject, "Im2"));
    assert_eq!(mask.get("Image", "ImageMask"), Some("yes"));
    assert_eq!(mask.get("Image", "BitsPerComponent"), Some("1"));
    assert_eq!(mask.get("Image", "ColorSpace"), None);
    assert_eq!(mask.get("Image", "Decoded size"), Some("8 bytes"));
    assert_eq!(mask.get("Usage", "Count"), Some("0"));
}

#[test]
fn colour_space_pattern_shading_and_property_props() {
    let doc = rich();

    let icc = props(&doc, &res(3, ResKind::ColorSpace, "CS1"));
    assert_eq!(icc.get("Color space", "Family"), Some("ICCBased"));
    assert_eq!(icc.get("Color space", "Components"), Some("3"));
    assert_eq!(icc.get("Color space", "N"), Some("3"));
    assert_eq!(icc.get("Color space", "Alternate"), Some("DeviceRGB"));
    let indexed = props(&doc, &res(3, ResKind::ColorSpace, "CS2"));
    assert_eq!(indexed.get("Color space", "Family"), Some("Indexed"));
    assert_eq!(indexed.get("Color space", "Base"), Some("DeviceRGB"));
    assert_eq!(indexed.get("Color space", "HiVal"), Some("1"));
    let sep = props(&doc, &res(3, ResKind::ColorSpace, "CS3"));
    assert_eq!(sep.get("Color space", "Family"), Some("Separation"));
    assert_eq!(sep.get("Color space", "Colorant"), Some("/Spot"));
    assert_eq!(sep.get("Color space", "Alternate"), Some("DeviceCMYK"));
    assert_eq!(sep.get("Color space", "Components"), Some("1"));

    let tiling = props(&doc, &res(3, ResKind::Pattern, "P1"));
    assert_eq!(tiling.get("Pattern", "PatternType"), Some("1 (tiling)"));
    assert_eq!(tiling.get("Pattern", "PaintType"), Some("1 (coloured)"));
    assert_eq!(tiling.get("Pattern", "TilingType"), Some("1"));
    assert_eq!(tiling.get("Pattern", "BBox"), Some("[0 0 10 10]"));
    assert_eq!(tiling.get("Pattern", "XStep"), Some("10"));
    assert_eq!(tiling.get("Pattern", "Resources"), Some("own (ExtGState)"));
    let shading_pattern = props(&doc, &res(3, ResKind::Pattern, "P2"));
    assert_eq!(shading_pattern.get("Pattern", "PatternType"), Some("2 (shading)"));
    assert_eq!(
        shading_pattern.get("Pattern", "Shading"),
        Some("axial (type 2), DeviceRGB (17 0 R)")
    );
    assert_eq!(
        link(&shading_pattern, "Pattern", "Shading"),
        Some(&NodeId::Object(obj(17)))
    );

    let property = props(&doc, &res(3, ResKind::Properties, "MC0"));
    assert_eq!(property.get("Properties", "/Type"), Some("/OCG"));
    assert_eq!(property.get("Properties", "/Name"), Some("(Layer 1)"));
    assert_eq!(property.get("Usage", "Count"), Some("1"));
}

#[test]
fn pattern_owner_has_resources_but_unknown_usage() {
    let doc = rich();
    let tree = doc.tree(TreeKind::Resources);

    let owner = find(&tree, &NodeId::Object(obj(15)));
    assert_eq!(labels(owner), ["ExtGState"]);
    let entry = find(&tree, &res(15, ResKind::ExtGState, "GP"));
    // Pattern cells are not interpreted: no count, and no claim that the entry is unused.
    assert_eq!(entry.detail.as_deref(), Some("CA ca BM"));
    assert_eq!(entry.badges, vec![]);
    let p = props(&doc, &res(15, ResKind::ExtGState, "GP"));
    assert_eq!(p.get("Resource", "Owner"), Some("Pattern 15 0 R"));
    assert_eq!(
        p.get("Usage", "Count"),
        Some("unknown (the content of this owner was not interpreted)")
    );
    assert_eq!(p.get("ExtGState", "/ca"), Some("0.25"));
}

#[test]
fn every_tree_entry_resolves_to_props() {
    let doc = rich();
    let tree = doc.tree(TreeKind::Resources);

    let mut seen = 0;
    for owner in &tree {
        for group in &owner.children {
            for entry in &group.children {
                assert!(doc.node(&entry.id).is_some(), "{}", entry.id);
                seen += 1;
            }
        }
    }

    // 4 fonts, 2 images, 3 colour spaces, 2 patterns, 1 shading, 1 ExtGState, 1 property list
    // on the page plus the pattern's own ExtGState.
    assert_eq!(seen, 15);
    assert_eq!(
        doc.tree(TreeKind::Resources),
        tree,
        "building the tree is deterministic"
    );
}
