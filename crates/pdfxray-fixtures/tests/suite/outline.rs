//! Re-derives the rich structure outline, annotations and embedded files from the raw objects.

use std::collections::BTreeMap;

use pdfxray_fixtures::{
    AnnotationFacts, AttributeFacts, AttributeSetFacts, AttributeSource, EmbeddedFileFacts, KidFacts,
    NamespaceFacts, NodeFacts, ObjectParentFacts, OutlineFacts, PageKeyFacts, ParentTreeEntry, RoleTarget,
};

use crate::common::{dict_text, latin1, page_objects, raw_stream_data, root_object};
use crate::pdfobj::{Obj, parse_object};

/// Structure types of the PDF 1.7 standard namespace that the fixtures rely on. `Annot` is not
/// one of them: it only exists in PDF 2.0.
const STANDARD_1_7: [&str; 34] = [
    "Document",
    "Part",
    "Art",
    "Sect",
    "Div",
    "BlockQuote",
    "Caption",
    "TOC",
    "TOCI",
    "Index",
    "NonStruct",
    "Private",
    "H",
    "H1",
    "H2",
    "H3",
    "P",
    "L",
    "LI",
    "Lbl",
    "LBody",
    "Table",
    "TR",
    "TH",
    "TD",
    "THead",
    "TBody",
    "TFoot",
    "Span",
    "Note",
    "Reference",
    "Link",
    "Figure",
    "Form",
];

/// Namespaces whose element names are final.
const FINAL_NAMESPACES: [&str; 2] = ["http://iso.org/pdf2/ssn", "http://www.w3.org/1998/Math/MathML"];

struct Ctx<'a> {
    pdf: &'a [u8],
    pages: Vec<u32>,
    role_map: BTreeMap<String, String>,
    class_map: BTreeMap<String, Vec<AttributeSetFacts>>,
    namespaces: Vec<NamespaceFacts>,
    namespace_rolemaps: BTreeMap<String, BTreeMap<String, RoleTarget>>,
}

impl Ctx<'_> {
    fn obj(&self, n: u32) -> Obj {
        parse_object(self.pdf, n)
    }

    fn resolve(&self, o: &Obj) -> Obj {
        match o {
            Obj::Ref(n) => self.obj(*n),
            other => other.clone(),
        }
    }

    fn page_index(&self, page: &Obj) -> Option<u32> {
        let n = page.reference()?;
        Some(u32::try_from(self.pages.iter().position(|p| *p == n).expect("known page")).unwrap())
    }

    fn namespace_uri(&self, ns: &Obj) -> String {
        self.obj(ns.reference().expect("/NS reference"))
            .get("NS")
            .and_then(Obj::string)
            .expect("namespace URI")
            .to_owned()
    }

    fn resolve_role(&self, role: &str, namespace: Option<&str>) -> (Option<String>, Option<String>) {
        match namespace {
            None => {
                let mut seen = vec![role.to_owned()];
                let mut current = role.to_owned();
                loop {
                    if STANDARD_1_7.contains(&current.as_str()) {
                        return (Some(current), None);
                    }
                    let Some(next) = self.role_map.get(&current) else {
                        return (None, None);
                    };
                    if seen.contains(next) {
                        return (None, None);
                    }
                    seen.push(next.clone());
                    current.clone_from(next);
                }
            }
            Some(uri) => {
                let (mut role, mut uri) = (role.to_owned(), uri.to_owned());
                for _ in 0..8 {
                    if FINAL_NAMESPACES.contains(&uri.as_str()) {
                        return (Some(role), Some(uri));
                    }
                    let Some(target) = self.namespace_rolemaps.get(&uri).and_then(|m| m.get(&role)) else {
                        return (None, Some(uri));
                    };
                    role.clone_from(&target.role);
                    uri = target.namespace.clone();
                }
                (None, Some(uri))
            }
        }
    }

    fn attribute_set(dict: &Obj) -> AttributeSetFacts {
        let owner = dict.get("O").and_then(Obj::name).expect("/O").to_owned();
        let entries = match dict {
            Obj::Dict(entries) => entries
                .iter()
                .filter(|(k, _)| k != "O")
                .map(|(k, v)| (k.clone(), v.syntax()))
                .collect(),
            _ => BTreeMap::new(),
        };
        AttributeSetFacts { owner, entries }
    }

    fn attribute_sets(&self, value: &Obj) -> Vec<AttributeSetFacts> {
        match self.resolve(value) {
            dict @ Obj::Dict(_) => vec![Self::attribute_set(&dict)],
            Obj::Arr(items) => items
                .iter()
                .map(|i| self.resolve(i))
                .filter(|i| matches!(i, Obj::Dict(_)))
                .map(|d| Self::attribute_set(&d))
                .collect(),
            _ => Vec::new(),
        }
    }

    fn kid(&self, kid: &Obj, elem_page: Option<u32>) -> KidFacts {
        match kid {
            Obj::Num(mcid) => KidFacts::Mcr {
                page: elem_page.expect("bare MCID needs the element's /Pg"),
                mcid: *mcid as i32,
                stream: None,
            },
            Obj::Ref(n) => {
                let target = self.obj(*n);
                if target.get("Type").and_then(Obj::name) == Some("StructElem") {
                    KidFacts::Elem(Box::new(self.node(*n)))
                } else {
                    self.kid(&target, elem_page)
                }
            }
            dict @ Obj::Dict(_) => match dict.get("Type").and_then(Obj::name) {
                Some("MCR") => KidFacts::Mcr {
                    page: dict
                        .get("Pg")
                        .and_then(|p| self.page_index(p))
                        .or(elem_page)
                        .expect("MCR page"),
                    mcid: dict.get("MCID").and_then(Obj::int).expect("MCID") as i32,
                    stream: dict.get("Stm").and_then(Obj::reference),
                },
                Some("OBJR") => KidFacts::Objr {
                    page: dict.get("Pg").and_then(|p| self.page_index(p)),
                    object: dict.get("Obj").and_then(Obj::reference).expect("/Obj"),
                },
                other => panic!("unexpected kid type {other:?}"),
            },
            other => panic!("unexpected kid {other:?}"),
        }
    }

    fn node(&self, n: u32) -> NodeFacts {
        let dict = self.obj(n);
        let role = dict.get("S").and_then(Obj::name).expect("/S").to_owned();
        let namespace = dict.get("NS").map(|ns| self.namespace_uri(ns));
        let (standard_role, standard_namespace) = self.resolve_role(&role, namespace.as_deref());
        let page = dict.get("Pg").and_then(|p| self.page_index(p));

        let text = |key: &str| dict.get(key).and_then(Obj::string).map(str::to_owned);
        let mut attributes: Vec<AttributeFacts> = dict
            .get("A")
            .map(|a| self.attribute_sets(a))
            .unwrap_or_default()
            .into_iter()
            .map(|set| AttributeFacts {
                source: AttributeSource::Direct,
                set,
            })
            .collect();
        let classes: Vec<String> = match dict.get("C") {
            Some(Obj::Name(c)) => vec![c.clone()],
            Some(Obj::Arr(items)) => items.iter().filter_map(|i| i.name().map(str::to_owned)).collect(),
            _ => Vec::new(),
        };
        for class in &classes {
            for set in self.class_map.get(class).expect("class defined in /ClassMap") {
                attributes.push(AttributeFacts {
                    source: AttributeSource::Class { name: class.clone() },
                    set: set.clone(),
                });
            }
        }
        let refs = |key: &str| -> Vec<u32> {
            dict.get(key)
                .map(|v| self.resolve(v))
                .map(|v| v.array().iter().filter_map(Obj::reference).collect())
                .unwrap_or_default()
        };
        let kids = match dict.get("K") {
            None => Vec::new(),
            Some(Obj::Arr(items)) => items.iter().map(|k| self.kid(k, page)).collect(),
            Some(single) => vec![self.kid(single, page)],
        };

        NodeFacts {
            object: n,
            role,
            standard_role,
            namespace,
            standard_namespace,
            page,
            id: text("ID"),
            lang: text("Lang"),
            alt: text("Alt"),
            actual_text: text("ActualText"),
            phoneme: text("Phoneme"),
            phonetic_alphabet: dict
                .get("PhoneticAlphabet")
                .and_then(Obj::name)
                .map(str::to_owned),
            attributes,
            classes,
            refs: refs("Ref"),
            associated_files: refs("AF"),
            kids,
        }
    }

    /// Walks a name tree, collecting `(name, object)` leaves.
    fn name_tree(&self, node: &Obj, out: &mut BTreeMap<String, u32>) {
        for pair in node.get("Names").map(Obj::array).unwrap_or_default().chunks(2) {
            out.insert(
                pair[0].string().expect("name").to_owned(),
                pair[1].reference().expect("object"),
            );
        }
        for kid in node.get("Kids").map(Obj::array).unwrap_or_default() {
            self.name_tree(&self.resolve(kid), out);
        }
    }

    /// Walks a number tree depth first, recording visited node objects and the leaves.
    fn number_tree(&self, node_ref: u32, nodes: &mut Vec<u32>, leaves: &mut Vec<(i32, Obj)>) {
        nodes.push(node_ref);
        let node = self.obj(node_ref);
        for pair in node.get("Nums").map(Obj::array).unwrap_or_default().chunks(2) {
            leaves.push((pair[0].int().expect("key") as i32, pair[1].clone()));
        }
        for kid in node.get("Kids").map(Obj::array).unwrap_or_default() {
            self.number_tree(kid.reference().expect("kid node"), nodes, leaves);
        }
    }
}

/// The rich outline of the file, or `None` when there is no structure tree.
pub(crate) fn observe_outline(pdf: &[u8]) -> Option<OutlineFacts> {
    let catalog = parse_object(pdf, root_object(pdf));
    let root_obj = catalog.get("StructTreeRoot")?.reference()?;
    let root = parse_object(pdf, root_obj);

    let mut ctx = Ctx {
        pdf,
        pages: page_objects(pdf),
        role_map: BTreeMap::new(),
        class_map: BTreeMap::new(),
        namespaces: Vec::new(),
        namespace_rolemaps: BTreeMap::new(),
    };
    if let Some(Obj::Dict(entries)) = root.get("RoleMap") {
        ctx.role_map = entries
            .iter()
            .map(|(k, v)| (k.clone(), v.name().expect("name").to_owned()))
            .collect();
    }
    if let Some(Obj::Dict(entries)) = root.get("ClassMap") {
        ctx.class_map = entries
            .iter()
            .map(|(k, v)| (k.clone(), ctx.attribute_sets(v)))
            .collect();
    }
    // Namespaces: first the URIs, then the role maps (which refer to namespaces by reference).
    let ns_objects: Vec<u32> = root
        .get("Namespaces")
        .map(Obj::array)
        .unwrap_or_default()
        .iter()
        .filter_map(Obj::reference)
        .collect();
    for ns in &ns_objects {
        let dict = parse_object(pdf, *ns);
        let uri = dict.get("NS").and_then(Obj::string).expect("NS").to_owned();
        let mut role_map = BTreeMap::new();
        if let Some(Obj::Dict(entries)) = dict.get("RoleMapNS") {
            for (key, value) in entries {
                let [Obj::Name(role), target_ns] = value.array() else {
                    panic!("RoleMapNS value is [name ns]")
                };
                role_map.insert(
                    key.clone(),
                    RoleTarget {
                        role: role.clone(),
                        namespace: ctx.namespace_uri(target_ns),
                    },
                );
            }
        }
        ctx.namespace_rolemaps.insert(uri.clone(), role_map.clone());
        ctx.namespaces.push(NamespaceFacts {
            object: *ns,
            uri,
            role_map,
        });
    }

    let mut outline = OutlineFacts::new(root_obj);
    outline.role_map = ctx.role_map.clone();
    outline.class_map = ctx.class_map.clone();
    outline.namespaces.clone_from(&ctx.namespaces);
    if let Some(id_tree) = root.get("IDTree") {
        ctx.name_tree(&ctx.resolve(id_tree), &mut outline.id_tree);
    }
    outline.children = match root.get("K") {
        None => Vec::new(),
        Some(Obj::Arr(items)) => items
            .iter()
            .filter_map(Obj::reference)
            .map(|n| ctx.node(n))
            .collect(),
        Some(single) => vec![ctx.node(single.reference().expect("root child"))],
    };
    for (index, page) in ctx.pages.iter().enumerate() {
        if let Some(key) = parse_object(pdf, *page).get("StructParents").and_then(Obj::int) {
            outline.page_keys.push(PageKeyFacts {
                page: u32::try_from(index).unwrap(),
                key: key as i32,
            });
        }
    }
    if let Some(parent_tree) = root.get("ParentTree").and_then(Obj::reference) {
        let mut leaves = Vec::new();
        ctx.number_tree(parent_tree, &mut outline.parent_tree_nodes, &mut leaves);
        for (key, value) in leaves {
            match value {
                Obj::Arr(items) => outline.parent_tree.push(ParentTreeEntry {
                    key,
                    parents: items.iter().map(Obj::reference).collect(),
                }),
                Obj::Ref(parent) => outline
                    .parent_tree_objects
                    .push(ObjectParentFacts { key, parent }),
                other => panic!("unexpected parent tree value {other:?}"),
            }
        }
    }
    outline.parent_tree_next_key = root.get("ParentTreeNextKey").and_then(Obj::int).map(|k| k as i32);
    Some(outline)
}

/// Annotations of every page, in page and `/Annots` order.
pub(crate) fn observe_annotations(pdf: &[u8]) -> Vec<AnnotationFacts> {
    let pages = page_objects(pdf);
    let mut out = Vec::new();
    for (index, page) in pages.iter().enumerate() {
        let page_dict = parse_object(pdf, *page);
        for annot_ref in page_dict.get("Annots").map(Obj::array).unwrap_or_default() {
            let object = annot_ref.reference().expect("annotation reference");
            let a = parse_object(pdf, object);
            let rect = a.get("Rect").expect("/Rect").numbers();
            let action = a.get("A");
            let dest = action.and_then(|a| a.get("D")).or_else(|| a.get("Dest"));
            out.push(AnnotationFacts {
                page: u32::try_from(index).unwrap(),
                object,
                subtype: a.get("Subtype").and_then(Obj::name).expect("subtype").to_owned(),
                rect: [rect[0], rect[1], rect[2], rect[3]],
                flags: a.get("F").and_then(Obj::int).unwrap_or(0) as u32,
                struct_parent: a.get("StructParent").and_then(Obj::int).map(|k| k as i32),
                uri: action
                    .and_then(|a| a.get("URI"))
                    .and_then(Obj::string)
                    .map(str::to_owned),
                goto_page: dest
                    .and_then(|d| d.array().first())
                    .and_then(Obj::reference)
                    .and_then(|target| pages.iter().position(|p| *p == target))
                    .map(|i| u32::try_from(i).unwrap()),
                has_appearance: a.get("AP").and_then(|ap| ap.get("N")).is_some(),
                field_type: a.get("FT").and_then(Obj::name).map(str::to_owned),
                field_name: a.get("T").and_then(Obj::string).map(str::to_owned),
            });
        }
    }
    out
}

/// Embedded files reachable through `/AF` arrays of struct elements.
pub(crate) fn observe_embedded_files(pdf: &[u8], outline: &OutlineFacts) -> Vec<EmbeddedFileFacts> {
    fn collect(nodes: &[NodeFacts], out: &mut Vec<u32>) {
        for node in nodes {
            out.extend(&node.associated_files);
            for kid in &node.kids {
                if let KidFacts::Elem(child) = kid {
                    collect(std::slice::from_ref(child), out);
                }
            }
        }
    }
    let mut filespecs = Vec::new();
    collect(&outline.children, &mut filespecs);
    filespecs
        .into_iter()
        .map(|filespec| {
            let spec = parse_object(pdf, filespec);
            let stream = spec
                .get("EF")
                .and_then(|ef| ef.get("F"))
                .and_then(Obj::reference)
                .expect("embedded file stream");
            assert!(latin1(&dict_text(pdf, stream)).contains("/Type /EmbeddedFile"));
            EmbeddedFileFacts {
                filespec,
                name: spec
                    .get("UF")
                    .or_else(|| spec.get("F"))
                    .and_then(Obj::string)
                    .expect("file name")
                    .to_owned(),
                relationship: spec
                    .get("AFRelationship")
                    .and_then(Obj::name)
                    .expect("relationship")
                    .to_owned(),
                stream,
                size: u32::try_from(raw_stream_data(pdf, stream).len()).unwrap(),
            }
        })
        .collect()
}
