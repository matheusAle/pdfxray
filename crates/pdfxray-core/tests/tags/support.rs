//! Helpers shared by the `tags` tests.

use pretty_assertions::assert_eq;

use super::*;

pub(crate) fn fixture(name: &str) -> Document {
    Document::open(pdfxray_fixtures::pdf_path(name)).unwrap()
}

pub(crate) fn expected(name: &str) -> Expected {
    pdfxray_fixtures::find(name).unwrap().build().unwrap().expected
}

pub(crate) fn obj(n: u32) -> ObjRef {
    ObjRef::new(n, 0)
}

pub(crate) fn elem(n: u32) -> NodeId {
    NodeId::StructElem(obj(n))
}

pub(crate) fn mc(page: u32, mcid: i32) -> NodeId {
    NodeId::MarkedContent { page, mcid }
}

/// The single top-level `StructTreeRoot` node.
pub(crate) fn root(doc: &Document) -> TreeNode {
    let mut top = doc.tree(TreeKind::Tags);
    assert_eq!(top.len(), 1, "one StructTreeRoot row");
    let root = top.remove(0);
    assert_eq!(root.id, NodeId::Group(100));
    assert_eq!(root.label, "StructTreeRoot");
    root
}

/// `label [id]` per node, indented by depth.
pub(crate) fn outline(node: &TreeNode, depth: usize, out: &mut Vec<String>) {
    out.push(format!("{}{} [{}]", "  ".repeat(depth), node.label, node.id));
    for c in &node.children {
        outline(c, depth + 1, out);
    }
}

pub(crate) fn outline_of(doc: &Document) -> Vec<String> {
    let mut out = Vec::new();
    outline(&root(doc), 0, &mut out);
    out
}

pub(crate) fn diagnostics(doc: &Document) -> Vec<(&'static str, Severity, Option<u32>)> {
    doc.info()
        .diagnostics
        .iter()
        .map(|d: &DiagnosticInfo| (d.code, d.severity, d.object.map(|o| o.num)))
        .collect()
}

pub(crate) fn props(doc: &Document, id: &NodeId) -> Props {
    doc.node(id).unwrap_or_else(|| panic!("no props for {id}"))
}

pub(crate) fn rows<'a>(props: &'a Props, group: &str, key: &str) -> Vec<&'a str> {
    props
        .groups
        .iter()
        .filter(|g| g.name == group)
        .flat_map(|g| &g.rows)
        .filter(|r| r.key == key)
        .map(|r| r.value.as_str())
        .collect()
}

pub(crate) fn group<'a>(props: &'a Props, name: &str) -> Option<&'a PropGroup> {
    props.groups.iter().find(|g| g.name == name)
}

pub(crate) fn center(r: Rect) -> Point {
    Point::new(r.x0.midpoint(r.x1), r.y0.midpoint(r.y1))
}

pub(crate) fn badge_codes(node: &TreeNode) -> Vec<&str> {
    node.badges.iter().map(|b| b.code.as_str()).collect()
}

#[derive(Debug, PartialEq)]
pub(crate) enum Kid {
    Elem(Box<Shape>),
    Content { page: u32, mcid: i32 },
    Object { page: Option<u32>, object: u32 },
}

#[derive(Debug, PartialEq, Default)]
pub(crate) struct Shape {
    pub(crate) object: u32,
    pub(crate) role: String,
    pub(crate) standard: Option<String>,
    pub(crate) namespace: Option<String>,
    pub(crate) standard_namespace: Option<String>,
    pub(crate) page: Option<u32>,
    pub(crate) id: Option<String>,
    pub(crate) lang: Option<String>,
    pub(crate) alt: Option<String>,
    pub(crate) actual_text: Option<String>,
    pub(crate) phoneme: Option<String>,
    pub(crate) phonetic_alphabet: Option<String>,
    pub(crate) classes: Vec<String>,
    pub(crate) refs: Vec<u32>,
    pub(crate) files: Vec<u32>,
    /// `(owner, key, value, class)`, sorted.
    pub(crate) attrs: Vec<(String, String, String, Option<String>)>,
    pub(crate) kids: Vec<Kid>,
}

pub(crate) fn shape_of_node_facts(n: &NodeFacts) -> Shape {
    let mut attrs: Vec<_> = n
        .attributes
        .iter()
        .flat_map(|a| {
            let class = match &a.source {
                AttributeSource::Direct => None,
                AttributeSource::Class { name } => Some(name.clone()),
            };
            a.set
                .entries
                .iter()
                .map(move |(k, v)| (a.set.owner.clone(), k.clone(), v.clone(), class.clone()))
        })
        .collect();
    attrs.sort();
    Shape {
        object: n.object,
        role: n.role.clone(),
        standard: n.standard_role.clone(),
        namespace: n.namespace.clone(),
        standard_namespace: n.standard_namespace.clone(),
        page: n.page,
        id: n.id.clone(),
        lang: n.lang.clone(),
        alt: n.alt.clone(),
        actual_text: n.actual_text.clone(),
        phoneme: n.phoneme.clone(),
        phonetic_alphabet: n.phonetic_alphabet.clone(),
        classes: n.classes.clone(),
        refs: n.refs.clone(),
        files: n.associated_files.clone(),
        attrs,
        kids: n
            .kids
            .iter()
            .map(|k| match k {
                KidFacts::Elem(e) => Kid::Elem(Box::new(shape_of_node_facts(e))),
                KidFacts::Mcr { page, mcid, .. } => Kid::Content {
                    page: *page,
                    mcid: *mcid,
                },
                KidFacts::Objr { page, object } => Kid::Object {
                    page: *page,
                    object: *object,
                },
            })
            .collect(),
    }
}

/// The simple fixtures list MCIDs and children separately; their elements have one or the other.
pub(crate) fn shape_of_elem_facts(e: &StructElemFacts) -> Shape {
    let page = e.page;
    let mut kids: Vec<Kid> = e
        .mcids
        .iter()
        .map(|&mcid| Kid::Content {
            page: page.unwrap(),
            mcid,
        })
        .collect();
    kids.extend(
        e.children
            .iter()
            .map(|c| Kid::Elem(Box::new(shape_of_elem_facts(c)))),
    );
    Shape {
        object: e.object,
        role: e.role.clone(),
        standard: e.standard_role.clone(),
        page,
        kids,
        ..Shape::default()
    }
}

pub(crate) fn shape_of_node(doc: &Document, node: &TreeNode) -> Shape {
    let NodeId::StructElem(r) = &node.id else {
        panic!("not a struct element: {}", node.id);
    };
    let p = props(doc, &node.id);
    let one = |g: &str, k: &str| p.get(g, k).map(str::to_owned);
    let element = |k: &str| one("Element", k);
    let link_nums = |key: &str, pick: fn(&NodeId) -> Option<u32>| -> Vec<u32> {
        group(&p, "Element")
            .map(|g| {
                g.rows
                    .iter()
                    .filter(|r| r.key == key)
                    .filter_map(|r| r.link.as_ref().and_then(pick))
                    .collect()
            })
            .unwrap_or_default()
    };
    let standard = element("Standard type").filter(|s| !s.starts_with("none"));
    let namespace = element("Namespace");
    let mut attrs: Vec<_> = group(&p, "Attributes")
        .map(|g| {
            g.rows
                .iter()
                .filter(|r| r.key != "Classes")
                .map(|r| {
                    let (owner, key) = r.key.split_once(": ").unwrap();
                    match r.value.rsplit_once(" (from class ") {
                        Some((v, class)) => (
                            owner.to_owned(),
                            key.to_owned(),
                            v.to_owned(),
                            Some(class.trim_end_matches(')').to_owned()),
                        ),
                        None => (owner.to_owned(), key.to_owned(), r.value.clone(), None),
                    }
                })
                .collect()
        })
        .unwrap_or_default();
    attrs.sort();
    let kids = node
        .children
        .iter()
        .map(|c| match &c.id {
            NodeId::StructElem(_) => Kid::Elem(Box::new(shape_of_node(doc, c))),
            NodeId::MarkedContent { page, mcid } => Kid::Content {
                page: *page,
                mcid: *mcid,
            },
            NodeId::Object(o) => Kid::Object {
                page: group(&p, "Content")
                    .unwrap()
                    .rows
                    .iter()
                    .find(|row| row.link.as_ref() == Some(&NodeId::Object(*o)))
                    .and_then(|row| row.value.rsplit(" · ").next())
                    .and_then(|s| s.strip_prefix("Page "))
                    .and_then(|n| n.parse::<u32>().ok())
                    .map(|n| n - 1),
                object: o.num,
            },
            other => panic!("unexpected child {other}"),
        })
        .collect();
    let page = group(&p, "Object")
        .and_then(|g| g.rows.iter().find(|r| r.key == "Pg"))
        .and_then(|r| match r.link {
            Some(NodeId::Page(n)) => Some(n),
            _ => None,
        });
    Shape {
        object: r.num,
        role: element("Type").unwrap(),
        standard,
        standard_namespace: element("Standard namespace").or_else(|| namespace.clone()),
        namespace,
        page,
        id: element("ID"),
        lang: element("Lang"),
        alt: element("Alt"),
        actual_text: element("ActualText"),
        phoneme: element("Phoneme"),
        phonetic_alphabet: element("Phonetic alphabet"),
        classes: one("Attributes", "Classes")
            .map(|c| c.split(", ").map(str::to_owned).collect())
            .unwrap_or_default(),
        refs: link_nums("Ref", |id| match id {
            NodeId::StructElem(r) => Some(r.num),
            _ => None,
        }),
        files: link_nums("Associated file", |id| match id {
            NodeId::Object(r) => Some(r.num),
            _ => None,
        }),
        attrs,
        kids,
    }
}

pub(crate) fn shapes_of_root(doc: &Document) -> Vec<Shape> {
    root(doc).children.iter().map(|c| shape_of_node(doc, c)).collect()
}

pub(crate) fn count_elems(shapes: &[Shape]) -> usize {
    shapes
        .iter()
        .map(|s| {
            1 + s
                .kids
                .iter()
                .map(|k| match k {
                    Kid::Elem(e) => count_elems(std::slice::from_ref(e)),
                    _ => 0,
                })
                .sum::<usize>()
        })
        .sum()
}
