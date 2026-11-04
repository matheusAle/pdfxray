//! The diff pairs, checked from the bytes without the writer or the report model.
//!
//! Every check here reads the files the way a reader would: the catalog, the page tree, the
//! structure tree and the `ParentTree` are followed by reference, content streams are interpreted
//! into device-space ink attributed to structure elements, and `hayro` renders both sides. The
//! declared [`MutationFacts`] are then held against what the bytes show.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;

use hayro::hayro_interpret::InterpreterSettings;
use hayro::hayro_syntax::Pdf;
use hayro::vello_cpu::color::palette::css::WHITE;
use hayro::{PixmapSettings, RenderCache, RenderSettings, render};
use pdfxray_fixtures::pairs::{self, BuiltPair, Mutation, MutationFacts, PAIRS};
use pretty_assertions::assert_eq;

use crate::common::{latin1, scan_objects, stream_data, text_of};
use crate::content::{Op, Operand, parse_ops};
use crate::pdfobj::{Obj, parse_object};

fn built(name: &str) -> BuiltPair {
    pairs::build(name)
        .unwrap_or_else(|| panic!("no pair {name}"))
        .unwrap()
}

// ---------------------------------------------------------------------------------------------
// Reading a file

/// The catalog, page tree and structure tree of one file, read by reference.
struct Doc<'a> {
    pdf: &'a [u8],
    pages: Vec<u32>,
    elements: BTreeMap<u32, Elem>,
    /// Children of `Document` (or of the root, if there is no single `Document`) in `/K` order.
    top: Vec<u32>,
    /// `(StructParents key, MCID)` → owning element.
    parent_tree: BTreeMap<(i64, i64), u32>,
    parent_tree_obj: u32,
}

#[derive(Debug, Clone, PartialEq)]
struct Elem {
    role: String,
    parent: u32,
    kids: Vec<u32>,
    mcid: Option<i64>,
    page: Option<u32>,
    alt: Option<String>,
    bbox: Option<Vec<f64>>,
}

fn trailer_root(pdf: &[u8]) -> u32 {
    let text = latin1(pdf);
    let at = text.rfind("trailer\n").expect("classic trailer");
    let rest = &text[at..];
    let root = &rest[rest.find("/Root ").expect("/Root") + "/Root ".len()..];
    root.split_whitespace().next().unwrap().parse().unwrap()
}

impl<'a> Doc<'a> {
    fn read(pdf: &'a [u8]) -> Self {
        let root = trailer_root(pdf);
        let catalog = parse_object(pdf, root);
        let pages_obj = parse_object(pdf, catalog.get("Pages").and_then(Obj::reference).unwrap());
        let pages: Vec<u32> = pages_obj
            .get("Kids")
            .unwrap()
            .array()
            .iter()
            .map(|k| k.reference().unwrap())
            .collect();
        assert_eq!(
            pages_obj.get("Count").and_then(Obj::int),
            Some(pages.len() as i64)
        );

        let Some(tree_root) = catalog.get("StructTreeRoot").and_then(Obj::reference) else {
            // Untagged: no tree, no ParentTree.
            return Self {
                pdf,
                pages,
                elements: BTreeMap::new(),
                top: Vec::new(),
                parent_tree: BTreeMap::new(),
                parent_tree_obj: 0,
            };
        };
        let root_dict = parse_object(pdf, tree_root);
        let mut elements = BTreeMap::new();
        let mut queue = vec![(root_dict.get("K").and_then(Obj::reference).unwrap(), tree_root)];
        while let Some((obj, parent)) = queue.pop() {
            let d = parse_object(pdf, obj);
            assert_eq!(d.get("Type").and_then(Obj::name), Some("StructElem"), "{obj}");
            assert_eq!(d.get("P").and_then(Obj::reference), Some(parent), "/P of {obj}");
            let (kids, mcid) = match d.get("K") {
                Some(Obj::Num(n)) => (Vec::new(), Some(*n as i64)),
                Some(Obj::Arr(items)) => (items.iter().map(|k| k.reference().unwrap()).collect(), None),
                other => panic!("unexpected /K {other:?} in {obj}"),
            };
            for &kid in kids.iter().rev() {
                queue.push((kid, obj));
            }
            let elem = Elem {
                role: d.get("S").and_then(Obj::name).unwrap().to_owned(),
                parent,
                kids,
                mcid,
                page: d.get("Pg").and_then(Obj::reference),
                alt: d.get("Alt").and_then(Obj::string).map(str::to_owned),
                bbox: d.get("A").and_then(|a| a.get("BBox")).map(Obj::numbers),
            };
            assert!(elements.insert(obj, elem).is_none(), "{obj} reached twice");
        }
        let doc_elem = root_dict.get("K").and_then(Obj::reference).unwrap();
        let top = elements[&doc_elem].kids.clone();

        let parent_tree_obj = root_dict.get("ParentTree").and_then(Obj::reference).unwrap();
        let pt = parse_object(pdf, parent_tree_obj);
        let nums = pt.get("Nums").unwrap().array();
        let mut parent_tree = BTreeMap::new();
        for pair in nums.chunks(2) {
            let key = pair[0].int().unwrap();
            for (mcid, owner) in pair[1].array().iter().enumerate() {
                parent_tree.insert((key, mcid as i64), owner.reference().unwrap());
            }
        }
        Self {
            pdf,
            pages,
            elements,
            top,
            parent_tree,
            parent_tree_obj,
        }
    }

    fn page_dict(&self, index: usize) -> Obj {
        parse_object(self.pdf, self.pages[index])
    }

    fn page_index(&self, obj: u32) -> Option<usize> {
        self.pages.iter().position(|&p| p == obj)
    }

    /// Content stream objects of page `index`, in `/Contents` order.
    fn content_streams(&self, index: usize) -> Vec<u32> {
        match self.page_dict(index).get("Contents").unwrap() {
            Obj::Ref(n) => vec![*n],
            Obj::Arr(items) => items.iter().map(|i| i.reference().unwrap()).collect(),
            other => panic!("/Contents {other:?}"),
        }
    }

    /// Concatenated content of page `index`, streams joined by a newline.
    fn content(&self, index: usize) -> Vec<u8> {
        let streams: Vec<Vec<u8>> = self
            .content_streams(index)
            .into_iter()
            .map(|s| stream_data(self.pdf, s))
            .collect();
        streams.join(&b'\n')
    }
}

// ---------------------------------------------------------------------------------------------
// Ink

/// Who a piece of ink belongs to.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
enum Owner {
    Elem(u32),
    /// A `/Artifact` sequence on page `n`.
    Artifact(usize),
    Untagged(usize),
}

/// One piece of ink in device space (PDF user space of the page), coordinates in thousandths.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
enum Ink {
    /// A shown string: text, origin, effective font size.
    Glyphs { text: String, x: i64, y: i64, size: i64 },
    /// A filled path: bounding box and gray level.
    Fill { bbox: [i64; 4], gray: i64 },
}

impl Ink {
    fn shifted(&self, dx: f64, dy: f64) -> Self {
        let (dx, dy) = (milli(dx), milli(dy));
        match self {
            Ink::Glyphs { text, x, y, size } => Ink::Glyphs {
                text: text.clone(),
                x: x + dx,
                y: y + dy,
                size: *size,
            },
            Ink::Fill { bbox, gray } => Ink::Fill {
                bbox: [bbox[0] + dx, bbox[1] + dy, bbox[2] + dx, bbox[3] + dy],
                gray: *gray,
            },
        }
    }
}

fn milli(v: f64) -> i64 {
    (v * 1000.0).round() as i64
}

type Matrix = [f64; 6];
const IDENTITY: Matrix = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];

fn mul(a: Matrix, b: Matrix) -> Matrix {
    [
        a[0] * b[0] + a[1] * b[2],
        a[0] * b[1] + a[1] * b[3],
        a[2] * b[0] + a[3] * b[2],
        a[2] * b[1] + a[3] * b[3],
        a[4] * b[0] + a[5] * b[2] + b[4],
        a[4] * b[1] + a[5] * b[3] + b[5],
    ]
}

fn apply(m: Matrix, x: f64, y: f64) -> (f64, f64) {
    (m[0] * x + m[2] * y + m[4], m[1] * x + m[3] * y + m[5])
}

fn num(o: &Operand) -> f64 {
    match o {
        Operand::Num(n) => *n,
        other => panic!("number expected, got {other:?}"),
    }
}

fn nums(op: &Op) -> Vec<f64> {
    op.operands.iter().map(num).collect()
}

#[derive(Clone)]
struct State {
    ctm: Matrix,
    gray: f64,
}

struct Tracer<'d, 'a> {
    doc: &'d Doc<'a>,
    page: usize,
    key: i64,
    out: Vec<(Owner, Ink)>,
    owners: Vec<Owner>,
}

impl Tracer<'_, '_> {
    fn owner(&self) -> Owner {
        self.owners.last().cloned().unwrap_or(Owner::Untagged(self.page))
    }

    fn run(&mut self, ops: &[Op], ctm: Matrix, xobjects: &Obj) {
        let mut gs = State { ctm, gray: 0.0 };
        let mut stack: Vec<State> = Vec::new();
        let (mut tm, mut tlm) = (IDENTITY, IDENTITY);
        let mut size = 0.0;
        let mut path: Vec<(f64, f64)> = Vec::new();
        for op in ops {
            match op.name.as_str() {
                "q" => stack.push(gs.clone()),
                "Q" => gs = stack.pop().expect("balanced q/Q"),
                "cm" => {
                    let n = nums(op);
                    gs.ctm = mul([n[0], n[1], n[2], n[3], n[4], n[5]], gs.ctm);
                }
                "g" => gs.gray = num(&op.operands[0]),
                "BT" => (tm, tlm) = (IDENTITY, IDENTITY),
                "Tf" => size = num(&op.operands[1]),
                "Td" => {
                    let n = nums(op);
                    tlm = mul([1.0, 0.0, 0.0, 1.0, n[0], n[1]], tlm);
                    tm = tlm;
                }
                "Tm" => {
                    let n = nums(op);
                    tlm = [n[0], n[1], n[2], n[3], n[4], n[5]];
                    tm = tlm;
                }
                "Tj" | "TJ" => {
                    let items = match &op.operands[0] {
                        Operand::Arr(items) => items.clone(),
                        other => vec![other.clone()],
                    };
                    let mut text = Vec::new();
                    for item in items {
                        match item {
                            Operand::Str(s) => text.extend(s),
                            other => panic!("the pairs never kern in TJ: {other:?}"),
                        }
                    }
                    let trm = mul(tm, gs.ctm);
                    let (x, y) = apply(trm, 0.0, 0.0);
                    let ink = Ink::Glyphs {
                        text: latin1(&text),
                        x: milli(x),
                        y: milli(y),
                        size: milli(size * trm[1].hypot(trm[3])),
                    };
                    self.out.push((self.owner(), ink));
                }
                "re" => {
                    let n = nums(op);
                    for (x, y) in [(n[0], n[1]), (n[0] + n[2], n[1] + n[3])] {
                        path.push(apply(gs.ctm, x, y));
                    }
                }
                "m" | "l" => {
                    let n = nums(op);
                    path.push(apply(gs.ctm, n[0], n[1]));
                }
                "ET" | "h" => {}
                "f" => {
                    let xs = path.iter().map(|p| milli(p.0));
                    let ys = path.iter().map(|p| milli(p.1));
                    let bbox = [
                        xs.clone().min().unwrap(),
                        ys.clone().min().unwrap(),
                        xs.max().unwrap(),
                        ys.max().unwrap(),
                    ];
                    self.out.push((
                        self.owner(),
                        Ink::Fill {
                            bbox,
                            gray: milli(gs.gray),
                        },
                    ));
                    path.clear();
                }
                "BDC" | "BMC" => {
                    let tag = match &op.operands[0] {
                        Operand::Name(n) => n.clone(),
                        other => panic!("tag {other:?}"),
                    };
                    let mcid = op.operands.get(1).and_then(|d| match d {
                        Operand::Dict(entries) => {
                            entries.iter().find(|(k, _)| k == "MCID").map(|(_, v)| num(v))
                        }
                        _ => None,
                    });
                    let owner = if tag == "Artifact" {
                        Owner::Artifact(self.page)
                    } else if let Some(mcid) = mcid {
                        let owner = self.doc.parent_tree[&(self.key, mcid as i64)];
                        Owner::Elem(owner)
                    } else {
                        self.owner()
                    };
                    self.owners.push(owner);
                }
                "EMC" => {
                    self.owners.pop().expect("balanced marked content");
                }
                "Do" => {
                    let name = match &op.operands[0] {
                        Operand::Name(n) => n.clone(),
                        other => panic!("{other:?}"),
                    };
                    let form = xobjects.get(&name).and_then(Obj::reference).unwrap();
                    let dict = parse_object(self.doc.pdf, form);
                    assert_eq!(dict.get("Subtype").and_then(Obj::name), Some("Form"));
                    let m = dict.get("Matrix").map_or(IDENTITY.to_vec(), Obj::numbers);
                    let ops = parse_ops(&stream_data(self.doc.pdf, form));
                    self.run(
                        &ops,
                        mul([m[0], m[1], m[2], m[3], m[4], m[5]], gs.ctm),
                        &Obj::Dict(Vec::new()),
                    );
                }
                other => panic!("operator {other} is not used by the pairs"),
            }
        }
        assert!(stack.is_empty(), "unbalanced q/Q");
    }
}

/// All ink of the file in page order, attributed to its owner.
fn ink(doc: &Doc<'_>) -> Vec<(Owner, Ink)> {
    let mut out = Vec::new();
    for page in 0..doc.pages.len() {
        let dict = doc.page_dict(page);
        // An untagged page has no key; it is never looked up there (nothing has an MCID).
        let key = dict.get("StructParents").and_then(Obj::int).unwrap_or(-1);
        let xobjects = dict
            .get("Resources")
            .and_then(|r| r.get("XObject"))
            .cloned()
            .unwrap_or(Obj::Dict(Vec::new()));
        let mut tracer = Tracer {
            doc,
            page,
            key,
            out: Vec::new(),
            owners: Vec::new(),
        };
        tracer.run(&parse_ops(&doc.content(page)), IDENTITY, &xobjects);
        assert!(tracer.owners.is_empty(), "unbalanced BDC/EMC on page {page}");
        out.extend(tracer.out);
    }
    out
}

/// Ink grouped by owner, each group in paint order.
fn ink_by_owner(doc: &Doc<'_>) -> BTreeMap<Owner, Vec<Ink>> {
    let mut map: BTreeMap<Owner, Vec<Ink>> = BTreeMap::new();
    for (owner, ink) in ink(doc) {
        map.entry(owner).or_default().push(ink);
    }
    map
}

// ---------------------------------------------------------------------------------------------
// Validity

/// Asserts what a strict reader expects of a never-updated file: one xref subsection from 0 to
/// `/Size`, every in-use entry pointing at its object header, a well-formed free list, every
/// reference resolving, the `ParentTree` agreeing with every element's `/K` and `/Pg` and with the
/// MCIDs the content uses, and `hayro` loading the page tree.
fn assert_strictly_valid(pdf: &[u8], what: &str) {
    // Byte offsets: slice the bytes, not a Latin-1 `String` (the binary marker is multi-byte there).
    let at = pdf.windows(10).rposition(|w| w == b"startxref\n").unwrap();
    let startxref: usize = latin1(&pdf[at + 10..]).lines().next().unwrap().parse().unwrap();
    assert_eq!(crate::common::count(pdf, b"\nxref\n"), 1, "{what}: one revision");
    let xref = latin1(&pdf[startxref..]);
    let mut lines = xref.lines();
    assert_eq!(lines.next(), Some("xref"), "{what}");
    let header: Vec<usize> = lines
        .next()
        .unwrap()
        .split(' ')
        .map(|n| n.parse().unwrap())
        .collect();
    let size = usize::try_from(crate::common::int_after(&xref, "/Size ").unwrap()).unwrap();
    assert_eq!(header, [0, size], "{what}: a single subsection covering /Size");
    let entries: Vec<&str> = lines.by_ref().take(size).collect();
    assert_eq!(lines.next(), Some("trailer"), "{what}: no second subsection");

    let mut in_use = BTreeSet::new();
    let mut free_next = BTreeMap::new();
    for (obj, entry) in entries.iter().enumerate() {
        let field: usize = entry[..10].parse().unwrap();
        match &entry[17..18] {
            "n" => {
                assert!(
                    pdf[field..].starts_with(format!("{obj} 0 obj\n").as_bytes()),
                    "{what}: entry {obj} points at {field}"
                );
                in_use.insert(obj as u32);
            }
            "f" => {
                free_next.insert(obj, field);
            }
            other => panic!("{what}: entry type {other}"),
        }
    }
    let mut at = 0;
    let mut visited = BTreeSet::new();
    loop {
        assert!(visited.insert(at), "{what}: free list loops at {at}");
        at = free_next[&at];
        if at == 0 {
            break;
        }
    }
    assert_eq!(
        visited.len(),
        free_next.len(),
        "{what}: every free entry is on the free list"
    );
    let headers: BTreeSet<u32> = scan_objects(pdf).into_iter().map(|(n, _)| n).collect();
    assert_eq!(headers, in_use, "{what}: xref and object headers agree");

    for &obj in &in_use {
        let body = text_of(pdf, obj);
        let dict_end = body.find("stream\n").unwrap_or(body.len());
        for r in refs_in(&body[..dict_end]) {
            assert!(in_use.contains(&r), "{what}: {obj} references missing {r}");
        }
    }

    let doc = Doc::read(pdf);
    for (&(key, mcid), &owner) in &doc.parent_tree {
        let elem = &doc.elements[&owner];
        assert_eq!(elem.mcid, Some(mcid), "{what}: ParentTree {key}/{mcid}");
        let page = doc.page_index(elem.page.unwrap()).unwrap();
        assert_eq!(
            doc.page_dict(page).get("StructParents").and_then(Obj::int),
            Some(key),
            "{what}: /Pg of {owner}"
        );
    }
    let leaves = doc.elements.values().filter(|e| e.mcid.is_some()).count();
    assert_eq!(
        leaves,
        doc.parent_tree.len(),
        "{what}: every MCID owner is in the ParentTree"
    );
    let owners: BTreeSet<Owner> = ink(&doc).into_iter().map(|(o, _)| o).collect();
    if doc.elements.is_empty() {
        assert!(
            owners.iter().all(|o| matches!(o, Owner::Untagged(_))),
            "{what}: an untagged file marks nothing"
        );
        let catalog = parse_object(pdf, trailer_root(pdf));
        assert!(catalog.get("MarkInfo").is_none(), "{what}: not claimed as tagged");
    } else {
        assert!(
            !owners.iter().any(|o| matches!(o, Owner::Untagged(_))),
            "{what}: no untagged ink"
        );
    }

    let parsed = Pdf::new(pdf.to_vec()).unwrap_or_else(|e| panic!("{what}: {e:?}"));
    assert_eq!(parsed.pages().len(), doc.pages.len(), "{what}: hayro page count");
}

/// `N 0 R` targets in `text`.
fn refs_in(text: &str) -> Vec<u32> {
    let mut out = Vec::new();
    let bytes = text.as_bytes();
    for (at, _) in text.match_indices(" 0 R") {
        let end_ok = bytes.get(at + 4).is_none_or(|b| !b.is_ascii_alphanumeric());
        let start = text[..at]
            .rfind(|c: char| !c.is_ascii_digit())
            .map_or(0, |s| s + 1);
        if end_ok && start < at {
            out.push(text[start..at].parse().unwrap());
        }
    }
    out
}

/// `text` with every `N 0 R` renumbered through `map` (numbers not in the map are kept).
fn remap_refs(text: &str, map: &BTreeMap<u32, u32>) -> String {
    let mut out = String::new();
    let mut last = 0;
    let bytes = text.as_bytes();
    for (at, _) in text.match_indices(" 0 R") {
        let end_ok = bytes.get(at + 4).is_none_or(|b| !b.is_ascii_alphanumeric());
        let start = text[..at]
            .rfind(|c: char| !c.is_ascii_digit())
            .map_or(0, |s| s + 1);
        if !end_ok || start >= at || start < last {
            continue;
        }
        let n: u32 = text[start..at].parse().unwrap();
        out.push_str(&text[last..start]);
        out.push_str(&map.get(&n).copied().unwrap_or(n).to_string());
        last = at;
    }
    out.push_str(&text[last..]);
    out
}

// ---------------------------------------------------------------------------------------------
// Rendering

/// Every page rendered at 1 px per point on white: `(width, height, RGBA)`.
fn render_all(pdf: &[u8]) -> Vec<(u16, u16, Vec<u8>)> {
    let parsed = Pdf::new(pdf.to_vec()).unwrap();
    parsed
        .pages()
        .iter()
        .map(|page| {
            let pixmap = render(
                page,
                &RenderCache::new(),
                &InterpreterSettings::default(),
                &RenderSettings::default(),
                &PixmapSettings {
                    bg_color: WHITE,
                    ..PixmapSettings::default()
                },
            );
            (
                pixmap.width(),
                pixmap.height(),
                pixmap.data_as_u8_slice().to_vec(),
            )
        })
        .collect()
}

fn painted_pixels(rgba: &[u8]) -> usize {
    rgba.chunks(4).filter(|p| *p != [255, 255, 255, 255]).count()
}

// ---------------------------------------------------------------------------------------------
// Helpers over the facts

fn mutation<'m>(facts: &'m MutationFacts, kind: &str) -> &'m Mutation {
    let found: Vec<&Mutation> = facts
        .mutations
        .iter()
        .filter(|m| serde_json::to_value(m).unwrap()["kind"] == kind)
        .collect();
    assert_eq!(found.len(), 1, "{}: exactly one {kind}", facts.name);
    found[0]
}

/// Same elements (by object number), same roles, texts, `/Alt`, boxes, pages and `/K`.
fn assert_same_tagging(a: &Doc<'_>, b: &Doc<'_>, what: &str) {
    assert_eq!(a.elements.len(), b.elements.len(), "{what}: element count");
    for (obj, ea) in &a.elements {
        let eb = &b.elements[obj];
        assert_eq!(ea.role, eb.role, "{what}: role of {obj}");
        assert_eq!(ea.kids, eb.kids, "{what}: /K of {obj}");
        assert_eq!(ea.alt, eb.alt, "{what}: /Alt of {obj}");
        assert_eq!(
            ea.page.and_then(|p| a.page_index(p)),
            eb.page.and_then(|p| b.page_index(p)),
            "{what}: page of {obj}"
        );
    }
}

// ---------------------------------------------------------------------------------------------
// Catalog and committed files

#[test]
fn pairs_are_the_agreed_twelve_in_order() {
    assert_eq!(
        PAIRS,
        [
            "renumber-objects",
            "rewrite-content-same-ink",
            "global-shift-2pt",
            "reorder-tags",
            "move-paragraph-3pt",
            "drop-alt",
            "change-heading-level",
            "retag-figure-as-formula",
            "insert-page",
            "artifact-page-number",
            "identical-rows",
            "untagged-edits",
        ]
    );
    for name in PAIRS {
        let facts = pairs::pair_mutation(name);
        assert_eq!(facts.name, name);
        assert!(facts.description.ends_with('.'), "{name} needs a sentence");
        assert!(!facts.mutations.is_empty(), "{name} declares a mutation");
    }
    // Smoke: an unknown name builds nothing.
    assert!(pairs::build("no-such-pair").is_none());
}

#[test]
fn pair_paths_name_the_committed_files() {
    for name in PAIRS {
        let (a, b) = pairs::pair(name);
        let built = built(name);

        assert_eq!(
            latin1(&fs::read(&a).unwrap()),
            latin1(&built.a),
            "{name} a drifted"
        );
        assert_eq!(
            latin1(&fs::read(&b).unwrap()),
            latin1(&built.b),
            "{name} b drifted"
        );
        assert_eq!(
            fs::read_to_string(pairs::mutation_path(name)).unwrap(),
            built.mutation_json().unwrap(),
            "{name} facts drifted"
        );
        let parsed: MutationFacts =
            serde_json::from_str(&fs::read_to_string(pairs::mutation_path(name)).unwrap()).unwrap();
        assert_eq!(&parsed, pairs::pair_mutation(name));
        // Smoke: both sides are PDFs.
        assert!(built.a.starts_with(b"%PDF-1.7\n") && built.b.starts_with(b"%PDF-1.7\n"));
    }
}

#[test]
fn pairs_dir_holds_exactly_the_pair_files() {
    let expected: BTreeSet<String> = PAIRS
        .iter()
        .flat_map(|n| {
            [
                format!("{n}.a.pdf"),
                format!("{n}.b.pdf"),
                format!("{n}.mutation.json"),
            ]
        })
        .collect();
    let actual: BTreeSet<String> = fs::read_dir(pairs::pairs_dir())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();

    assert_eq!(actual, expected);
    assert_eq!(actual.len(), 36);
}

#[test]
fn building_a_pair_twice_is_byte_identical() {
    for name in PAIRS {
        let (first, second) = (built(name), built(name));

        assert_eq!(latin1(&first.a), latin1(&second.a), "{name} a");
        assert_eq!(latin1(&first.b), latin1(&second.b), "{name} b");
        assert_eq!(first.mutation_json().unwrap(), second.mutation_json().unwrap());
        assert!(first.a.len() > 1000);
    }
}

#[test]
fn generate_writes_what_build_returns() {
    let dir = std::env::temp_dir().join(format!("pdfxray-pairs-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);

    let written = pairs::generate(&dir).unwrap();

    assert_eq!(written.len(), PAIRS.len() * 3);
    for name in PAIRS {
        let built = built(name);
        assert_eq!(
            latin1(&fs::read(dir.join(format!("{name}.a.pdf"))).unwrap()),
            latin1(&built.a)
        );
        assert_eq!(
            latin1(&fs::read(dir.join(format!("{name}.b.pdf"))).unwrap()),
            latin1(&built.b)
        );
    }
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn every_side_of_every_pair_is_strictly_valid() {
    for name in PAIRS {
        let built = built(name);
        assert_strictly_valid(&built.a, &format!("{name} a"));
        assert_strictly_valid(&built.b, &format!("{name} b"));
    }
}

/// The pairs whose base is the tagged report; the rest strip its tagging.
const UNTAGGED: [&str; 1] = ["untagged-edits"];

#[test]
fn every_base_is_the_same_document() {
    let base = built(PAIRS[0]).a;
    for name in PAIRS.iter().filter(|n| !UNTAGGED.contains(n)) {
        assert_eq!(latin1(&built(name).a), latin1(&base), "{name} base differs");
    }
    // The untagged base paints the same ink, page for page, owned by nothing.
    let (tagged, ink_of_base) = (Doc::read(&base), ink(&Doc::read(&base)));
    for name in UNTAGGED {
        let untagged = built(name).a;
        let untagged_ink = ink(&Doc::read(&untagged));
        let page_of = |o: &Owner| match o {
            Owner::Untagged(p) | Owner::Artifact(p) => *p,
            Owner::Elem(e) => tagged.page_index(tagged.elements[e].page.unwrap()).unwrap(),
        };
        let by_page = |v: &[(Owner, Ink)]| -> BTreeSet<(usize, Ink)> {
            v.iter().map(|(o, i)| (page_of(o), i.clone())).collect()
        };
        assert_eq!(by_page(&untagged_ink), by_page(&ink_of_base), "{name}");
        assert_eq!(Doc::read(&untagged).elements.len(), 0, "{name}");
    }
    // Smoke: the base is the three-page report.
    let doc = Doc::read(&base);
    assert_eq!(doc.pages.len(), 3);
    assert_eq!(doc.elements.len(), 25);
}

#[test]
fn side_facts_match_the_files() {
    for name in PAIRS {
        let built = built(name);
        let facts = &built.mutation;
        for (side, pdf) in [(&facts.a, &built.a), (&facts.b, &built.b)] {
            let doc = Doc::read(pdf);
            assert_eq!(side.page_count, doc.pages.len(), "{name}");
            assert_eq!(side.page_objects, doc.pages, "{name}");
            assert_eq!(side.element_count, doc.elements.len(), "{name}");
        }
    }
}

// ---------------------------------------------------------------------------------------------
// renumber-objects

#[test]
fn renumber_objects_renumbers_every_object_and_every_reference() {
    let built = built("renumber-objects");
    let Mutation::Renumbered {
        objects,
        reordered_in_file,
    } = mutation(&built.mutation, "renumbered")
    else {
        unreachable!()
    };
    let map: BTreeMap<u32, u32> = objects.iter().map(|r| (r.a, r.b)).collect();

    // Every object of A is listed, every number changed, and B has exactly the images.
    let a_objs: BTreeSet<u32> = scan_objects(&built.a).into_iter().map(|(n, _)| n).collect();
    let b_objs: BTreeSet<u32> = scan_objects(&built.b).into_iter().map(|(n, _)| n).collect();
    assert_eq!(map.keys().copied().collect::<BTreeSet<_>>(), a_objs);
    assert_eq!(map.values().copied().collect::<BTreeSet<_>>(), b_objs);
    assert!(objects.iter().all(|r| r.a != r.b), "some object kept its number");
    // Each body is the A body with every reference renumbered; stream data is byte-identical.
    for (&a, &b) in &map {
        assert_eq!(
            remap_refs(&text_of(&built.a, a), &map).replacen(&format!("{a} 0 obj"), &format!("{b} 0 obj"), 1),
            text_of(&built.b, b),
            "object {a} -> {b}"
        );
    }
    assert_eq!(map[&trailer_root(&built.a)], trailer_root(&built.b));
    // The byte order differs too.
    let a_order: Vec<u32> = scan_objects(&built.a).into_iter().map(|(n, _)| map[&n]).collect();
    let b_order: Vec<u32> = scan_objects(&built.b).into_iter().map(|(n, _)| n).collect();
    assert!(*reordered_in_file);
    assert_ne!(a_order, b_order);
    // The xref really is different: B is sparse.
    assert!(latin1(&built.b).contains(&format!("xref\n0 {}\n", b_objs.last().unwrap() + 1)));
    // Smoke: there are objects, and the single declared mutation is the renumbering.
    assert_eq!(objects.len(), 36);
    assert_eq!(built.mutation.mutations.len(), 1);
}

#[test]
fn renumber_objects_keeps_tagging_and_ink() {
    let built = built("renumber-objects");
    let Mutation::Renumbered { objects, .. } = mutation(&built.mutation, "renumbered") else {
        unreachable!()
    };
    let map: BTreeMap<u32, u32> = objects.iter().map(|r| (r.a, r.b)).collect();
    let (a, b) = (Doc::read(&built.a), Doc::read(&built.b));

    let a_ink: Vec<(Owner, Ink)> = ink(&a)
        .into_iter()
        .map(|(o, i)| match o {
            Owner::Elem(e) => (Owner::Elem(map[&e]), i),
            other => (other, i),
        })
        .collect();
    assert_eq!(a_ink, ink(&b));
    let mapped_top: Vec<u32> = a.top.iter().map(|e| map[e]).collect();
    assert_eq!(mapped_top, b.top);
    for (obj, ea) in &a.elements {
        let eb = &b.elements[&map[obj]];
        assert_eq!(
            (&ea.role, &ea.alt, &ea.bbox, ea.mcid),
            (&eb.role, &eb.alt, &eb.bbox, eb.mcid)
        );
    }
    assert_eq!(render_all(&built.a), render_all(&built.b), "pixels differ");
    // Smoke: there is ink to compare.
    assert!(painted_pixels(&render_all(&built.a)[0].2) > 1000);
}

// ---------------------------------------------------------------------------------------------
// rewrite-content-same-ink

#[test]
fn rewrite_content_same_ink_paints_identical_ink_from_different_streams() {
    let built = built("rewrite-content-same-ink");
    let (a, b) = (Doc::read(&built.a), Doc::read(&built.b));

    assert_eq!(
        ink(&a),
        ink(&b),
        "ink, its owners and its paint order are unchanged"
    );
    assert_same_tagging(&a, &b, "rewrite-content-same-ink");
    let (ra, rb) = (render_all(&built.a), render_all(&built.b));
    assert_eq!(ra.len(), rb.len());
    for (page, (pa, pb)) in ra.iter().zip(&rb).enumerate() {
        assert_eq!((pa.0, pa.1), (pb.0, pb.1));
        let differing = pa.2.chunks(4).zip(pb.2.chunks(4)).filter(|(x, y)| x != y).count();
        assert_eq!(differing, 0, "page {page}: pixels differ");
    }
    // Smoke: there is ink on every page.
    assert!(ra.iter().all(|p| painted_pixels(&p.2) > 500));
}

#[test]
fn rewrite_content_same_ink_really_rewrote_the_content() {
    let built = built("rewrite-content-same-ink");
    let Mutation::ContentRewritten { pages } = mutation(&built.mutation, "content-rewritten") else {
        unreachable!()
    };
    let (a, b) = (Doc::read(&built.a), Doc::read(&built.b));

    assert_eq!(pages.len(), 3);
    let mut split_inside_text_object = 0;
    let mut split_between_operands_and_operator = 0;
    for (index, page) in pages.iter().enumerate() {
        assert_eq!(page.page, index);
        assert_eq!(a.content_streams(index), page.a_streams);
        assert_eq!(b.content_streams(index), page.b_streams);
        assert_eq!(page.a_streams.len(), 1);
        assert_eq!(page.b_streams.len(), 3);
        assert!(
            page.b_streams
                .iter()
                .any(|&s| text_of(&built.b, s).contains("/FlateDecode")),
            "page {index}: some stream is compressed"
        );
        assert_ne!(
            a.content(index),
            b.content(index),
            "page {index}: content bytes differ"
        );
        for &s in &page.b_streams[..2] {
            let ops = parse_ops(&stream_data(&built.b, s));
            let opened = ops.iter().filter(|o| o.name == "BT").count();
            let closed = ops.iter().filter(|o| o.name == "ET").count();
            if opened != closed {
                split_inside_text_object += 1;
            }
            // A dangling operand: the stream ends with operands its operator is in the next stream.
            let data = latin1(&stream_data(&built.b, s));
            let last = data.split_whitespace().last().unwrap();
            if last.ends_with(')') || last.parse::<f64>().is_ok() {
                split_between_operands_and_operator += 1;
            }
        }

        // Figures moved into forms, drawn with paths; the page streams draw no rectangles.
        let xobjects = b
            .page_dict(index)
            .get("Resources")
            .unwrap()
            .get("XObject")
            .cloned();
        let forms: Vec<u32> = xobjects
            .map(|x| match x {
                Obj::Dict(entries) => entries.iter().map(|(_, v)| v.reference().unwrap()).collect(),
                _ => Vec::new(),
            })
            .unwrap_or_default();
        assert_eq!(forms, page.b_forms, "page {index}");
        for &form in &forms {
            let ops = parse_ops(&stream_data(&built.b, form));
            assert!(ops.iter().any(|o| o.name == "l") && !ops.iter().any(|o| o.name == "re"));
        }
        let b_ops = parse_ops(&b.content(index));
        assert!(
            !b_ops.iter().any(|o| o.name == "re"),
            "page {index}: no re left in page content"
        );
        assert_eq!(b_ops.iter().filter(|o| o.name == "Do").count(), forms.len());

        // MCIDs were reassigned, and the files agree with the declared mapping.
        assert!(
            page.mcids.iter().any(|(_, ma, mb)| ma != mb),
            "page {index}: MCIDs changed"
        );
        let key = a
            .page_dict(index)
            .get("StructParents")
            .and_then(Obj::int)
            .unwrap();
        for (label, ma, mb) in &page.mcids {
            assert_eq!(
                a.parent_tree.get(&(key, i64::from(*ma))),
                b.parent_tree.get(&(key, i64::from(*mb))),
                "{label}: same owner under the old and the new MCID"
            );
        }
    }
    assert!(split_inside_text_object > 0, "some split falls inside BT/ET");
    assert!(
        split_between_operands_and_operator > 0,
        "some split separates operands from their operator"
    );
    // One form is placed by cm, another by /Matrix.
    let all_forms: Vec<u32> = pages.iter().flat_map(|p| p.b_forms.clone()).collect();
    let with_matrix = all_forms
        .iter()
        .filter(|&&f| text_of(&built.b, f).contains("/Matrix"))
        .count();
    assert_eq!((all_forms.len(), with_matrix), (2, 1));
    // Smoke: the element objects themselves did not move.
    assert_eq!(
        a.elements.keys().collect::<Vec<_>>(),
        b.elements.keys().collect::<Vec<_>>()
    );
}

/// Every object both files have is byte-identical except `changed`; returns the objects only one
/// side has.
fn assert_only_objects_differ(built: &BuiltPair, changed: &[u32]) -> BTreeSet<u32> {
    let a: BTreeSet<u32> = scan_objects(&built.a).into_iter().map(|(n, _)| n).collect();
    let b: BTreeSet<u32> = scan_objects(&built.b).into_iter().map(|(n, _)| n).collect();
    for obj in a.intersection(&b) {
        let same = text_of(&built.a, *obj) == text_of(&built.b, *obj);
        assert_eq!(
            !same,
            changed.contains(obj),
            "{}: object {obj}",
            built.mutation.name
        );
    }
    a.symmetric_difference(&b).copied().collect()
}

/// Text of the glyphs `owner` paints, in paint order.
fn texts_of(ink: &BTreeMap<Owner, Vec<Ink>>, owner: &Owner) -> Vec<String> {
    ink.get(owner)
        .map(|items| {
            items
                .iter()
                .filter_map(|i| match i {
                    Ink::Glyphs { text, .. } => Some(text.clone()),
                    Ink::Fill { .. } => None,
                })
                .collect()
        })
        .unwrap_or_default()
}

// ---------------------------------------------------------------------------------------------
// global-shift-2pt

#[test]
fn global_shift_2pt_translates_every_piece_of_ink_and_every_box() {
    let built = built("global-shift-2pt");
    let Mutation::Translated { dx, dy } = *mutation(&built.mutation, "translated") else {
        unreachable!()
    };
    let (a, b) = (Doc::read(&built.a), Doc::read(&built.b));

    assert_eq!((dx, dy), (2.0, -2.0));
    let shifted: Vec<(Owner, Ink)> = ink(&a).into_iter().map(|(o, i)| (o, i.shifted(dx, dy))).collect();
    assert_eq!(
        shifted,
        ink(&b),
        "every glyph run, fill and artifact moved by exactly the offset"
    );
    assert!(
        ink(&b).iter().any(|(o, _)| matches!(o, Owner::Artifact(_))),
        "artifacts included"
    );
    for (obj, ea) in &a.elements {
        let want = ea
            .bbox
            .as_ref()
            .map(|r| vec![r[0] + dx, r[1] + dy, r[2] + dx, r[3] + dy]);
        assert_eq!(b.elements[obj].bbox, want, "BBox of {obj}");
    }
    assert_same_tagging(&a, &b, "global-shift-2pt");
    for page in 0..3 {
        assert_eq!(
            a.page_dict(page).get("MediaBox"),
            b.page_dict(page).get("MediaBox")
        );
    }

    // The rendering is the same picture, 2 px right and 2 px down at 1 px per point. The
    // rasteriser may round a glyph edge differently at another pixel phase, so allow 2/255.
    let (ra, rb) = (render_all(&built.a), render_all(&built.b));
    for (page, (pa, pb)) in ra.iter().zip(&rb).enumerate() {
        let (width, height) = (usize::from(pa.0), usize::from(pa.1));
        let px = |img: &[u8], x: usize, y: usize| -> [u8; 4] {
            img[(y * width + x) * 4..][..4].try_into().unwrap()
        };
        let mut worst = 0;
        let mut exact = 0;
        for y in 0..height - 2 {
            for x in 0..width - 2 {
                let (p, q) = (px(&pa.2, x, y), px(&pb.2, x + 2, y + 2));
                let delta = p.iter().zip(q).map(|(c, d)| c.abs_diff(d)).max().unwrap();
                worst = worst.max(delta);
                exact += usize::from(delta == 0);
            }
        }
        assert!(
            worst <= 2,
            "page {page}: shifted pixels differ by up to {worst}/255"
        );
        assert!(
            exact >= (width - 2) * (height - 2) - 4,
            "page {page}: only a few pixels may round differently"
        );
        assert_ne!(pa.2, pb.2, "page {page}: the shift is visible");
    }
    // Smoke: one declared mutation, and the shift is not a cm prepended to unchanged streams.
    assert_eq!(built.mutation.mutations.len(), 1);
    assert!(!parse_ops(&b.content(0)).iter().any(|o| o.name == "cm"));
}

// ---------------------------------------------------------------------------------------------
// reorder-tags

#[test]
fn reorder_tags_swaps_two_siblings_and_nothing_else() {
    let built = built("reorder-tags");
    let Mutation::SiblingsSwapped {
        parent,
        first,
        second,
    } = mutation(&built.mutation, "siblings-swapped")
    else {
        unreachable!()
    };
    let (a, b) = (Doc::read(&built.a), Doc::read(&built.b));
    let (pa, fa, sa) = (parent.a.unwrap(), first.a.unwrap(), second.a.unwrap());

    let mut want = a.elements[&pa].kids.clone();
    let (i, j) = (
        want.iter().position(|&k| k == fa).unwrap(),
        want.iter().position(|&k| k == sa).unwrap(),
    );
    assert_eq!(j, i + 1, "adjacent siblings, first before second in A");
    want.swap(i, j);
    assert_eq!(b.elements[&pa].kids, want);
    assert_eq!((first.b, second.b, parent.b), (first.a, second.a, parent.a));
    assert_eq!(
        (first.role.as_str(), second.role.as_str(), parent.role.as_str()),
        ("P", "P", "Document")
    );
    assert_eq!(ink(&a), ink(&b), "ink unchanged, including paint order");
    assert_eq!(render_all(&built.a), render_all(&built.b));
    let only = assert_only_objects_differ(&built, &[pa]);
    // Smoke: no object was added or removed.
    assert!(only.is_empty());
}

// ---------------------------------------------------------------------------------------------
// move-paragraph-3pt

#[test]
fn move_paragraph_3pt_moves_one_paragraphs_ink_only() {
    let built = built("move-paragraph-3pt");
    let Mutation::ElementMoved { element, dx, dy } = mutation(&built.mutation, "element-moved") else {
        unreachable!()
    };
    let (a, b) = (Doc::read(&built.a), Doc::read(&built.b));
    let moved = Owner::Elem(element.a.unwrap());

    assert_eq!((*dx, *dy), (0.0, -3.0));
    assert_eq!(
        element.text.as_deref(),
        Some("We compared invoices against the ledger.")
    );
    let (ia, ib) = (ink_by_owner(&a), ink_by_owner(&b));
    assert_eq!(ia.keys().collect::<Vec<_>>(), ib.keys().collect::<Vec<_>>());
    for (owner, items) in &ia {
        let want: Vec<Ink> = if *owner == moved {
            items.iter().map(|i| i.shifted(*dx, *dy)).collect()
        } else {
            items.clone()
        };
        assert_eq!(ib[owner], want, "{owner:?}");
    }
    assert_same_tagging(&a, &b, "move-paragraph-3pt");
    let page = element.page_a.unwrap();
    let content_a = a.content_streams(page);
    assert_eq!(assert_only_objects_differ(&built, &content_a), BTreeSet::new());
    // Smoke: the pixels changed on that page only.
    let (ra, rb) = (render_all(&built.a), render_all(&built.b));
    let changed: Vec<usize> = (0..3).filter(|&p| ra[p] != rb[p]).collect();
    assert_eq!(changed, [page]);
}

// ---------------------------------------------------------------------------------------------
// drop-alt, change-heading-level, retag-figure-as-formula

#[test]
fn drop_alt_removes_exactly_one_alt() {
    let built = built("drop-alt");
    let Mutation::AltRemoved { element, alt } = mutation(&built.mutation, "alt-removed") else {
        unreachable!()
    };
    let (a, b) = (Doc::read(&built.a), Doc::read(&built.b));
    let obj = element.a.unwrap();

    assert_eq!(a.elements[&obj].alt.as_deref(), Some(alt.as_str()));
    assert_eq!(alt, "Bar chart of revenue by region");
    assert_eq!(b.elements[&obj].alt, None);
    assert_eq!(element.role, "Figure");
    assert!(!text_of(&built.b, obj).contains("/Alt"));
    assert_eq!(ink(&a), ink(&b));
    assert!(assert_only_objects_differ(&built, &[obj]).is_empty());
    // Smoke: the other figure keeps its /Alt.
    let others = b
        .elements
        .values()
        .filter(|e| e.role == "Figure" && e.alt.is_some())
        .count();
    assert_eq!(others, 1);
}

fn assert_role_changed(name: &str, from: &str, to: &str, text: Option<&str>) {
    let built = built(name);
    let Mutation::RoleChanged {
        element,
        from: f,
        to: t,
    } = mutation(&built.mutation, "role-changed")
    else {
        unreachable!()
    };
    let (a, b) = (Doc::read(&built.a), Doc::read(&built.b));
    let obj = element.a.unwrap();

    assert_eq!((f.as_str(), t.as_str()), (from, to), "{name}");
    assert_eq!(a.elements[&obj].role, from);
    assert_eq!(b.elements[&obj].role, to);
    assert_eq!(element.text.as_deref(), text);
    assert_eq!(a.elements[&obj].alt, b.elements[&obj].alt);
    assert_eq!(a.elements[&obj].bbox, b.elements[&obj].bbox);
    // The content stream, and so the marked-content tag, is untouched.
    assert!(assert_only_objects_differ(&built, &[obj]).is_empty());
    let page = element.page_a.unwrap();
    assert!(latin1(&a.content(page)).contains(&format!("/{from} << /MCID")));
    assert_eq!(ink(&a), ink(&b));
    // Smoke: exactly one element's role differs.
    let changed = a
        .elements
        .iter()
        .filter(|(o, e)| b.elements[*o].role != e.role)
        .count();
    assert_eq!(changed, 1, "{name}");
}

#[test]
fn change_heading_level_turns_one_h2_into_h3() {
    assert_role_changed("change-heading-level", "H2", "H3", Some("Method"));
}

#[test]
fn retag_figure_as_formula_changes_only_the_structure_type() {
    assert_role_changed("retag-figure-as-formula", "Figure", "Formula", None);
    let built = built("retag-figure-as-formula");
    let b = Doc::read(&built.b);
    // Smoke: the retagged element keeps its /Alt.
    let formula = b.elements.values().find(|e| e.role == "Formula").unwrap();
    assert_eq!(formula.alt.as_deref(), Some("Flow diagram of the audit"));
}

// ---------------------------------------------------------------------------------------------
// insert-page

fn artifact_texts(doc: &Doc<'_>) -> Vec<String> {
    let by_owner = ink_by_owner(doc);
    (0..doc.pages.len())
        .map(|p| texts_of(&by_owner, &Owner::Artifact(p)).concat())
        .collect()
}

#[test]
fn insert_page_inserts_one_page_and_shifts_the_rest() {
    let built = built("insert-page");
    let Mutation::PageInserted {
        index,
        page_object,
        elements,
    } = mutation(&built.mutation, "page-inserted")
    else {
        unreachable!()
    };
    let Mutation::ArtifactTextChanged { changes } = mutation(&built.mutation, "artifact-text-changed") else {
        unreachable!()
    };
    let (a, b) = (Doc::read(&built.a), Doc::read(&built.b));

    assert_eq!(*index, 1);
    let mut want_pages = a.pages.clone();
    want_pages.insert(*index, *page_object);
    assert_eq!(
        b.pages, want_pages,
        "old page objects kept, new one in the middle"
    );
    // The new page holds exactly the declared elements, with the declared texts.
    let (ia, ib) = (ink_by_owner(&a), ink_by_owner(&b));
    let on_new_page: Vec<u32> = b
        .elements
        .iter()
        .filter(|(_, e)| e.page == Some(*page_object))
        .map(|(o, _)| *o)
        .collect();
    assert_eq!(
        on_new_page,
        elements.iter().map(|e| e.b.unwrap()).collect::<Vec<_>>()
    );
    for e in elements {
        assert_eq!(e.a, None);
        assert_eq!(e.page_b, Some(*index));
        assert_eq!(
            texts_of(&ib, &Owner::Elem(e.b.unwrap())),
            [e.text.clone().unwrap()]
        );
    }
    // Everything else: same element, same ink, page index shifted after the insertion.
    for (obj, ea) in &a.elements {
        let eb = &b.elements[obj];
        assert_eq!(
            (&ea.role, &ea.alt, ea.page),
            (&eb.role, &eb.alt, eb.page),
            "{obj}"
        );
        assert_eq!(
            ia.get(&Owner::Elem(*obj)),
            ib.get(&Owner::Elem(*obj)),
            "ink of {obj}"
        );
    }
    let mut want_top = a.top.clone();
    let after_chart = want_top
        .iter()
        .position(|&e| a.elements[&e].role == "Figure")
        .unwrap()
        + 1;
    want_top.splice(after_chart..after_chart, elements.iter().map(|e| e.b.unwrap()));
    assert_eq!(b.top, want_top);
    // Footers renumbered on the shifted pages, as declared.
    let (fa, fb) = (artifact_texts(&a), artifact_texts(&b));
    assert_eq!(fa, ["Page 1", "Page 2", "Page 3"]);
    assert_eq!(fb, ["Page 1", "Page 2", "Page 3", "Page 4"]);
    for c in changes {
        assert_eq!(
            (fa[c.page_a].as_str(), fb[c.page_b].as_str()),
            (c.a_text.as_str(), c.b_text.as_str())
        );
    }
    assert_eq!(
        changes.iter().map(|c| (c.page_a, c.page_b)).collect::<Vec<_>>(),
        [(1, 2), (2, 3)]
    );
    // Smoke: the inserted page renders with ink.
    assert!(painted_pixels(&render_all(&built.b)[*index].2) > 100);
}

// ---------------------------------------------------------------------------------------------
// artifact-page-number

#[test]
fn artifact_page_number_changes_every_footer_and_nothing_tagged() {
    let built = built("artifact-page-number");
    let Mutation::ArtifactTextChanged { changes } = mutation(&built.mutation, "artifact-text-changed") else {
        unreachable!()
    };
    let (a, b) = (Doc::read(&built.a), Doc::read(&built.b));
    let (ia, ib) = (ink_by_owner(&a), ink_by_owner(&b));

    assert_eq!(artifact_texts(&a), ["Page 1", "Page 2", "Page 3"]);
    assert_eq!(artifact_texts(&b), ["- 1 -", "- 2 -", "- 3 -"]);
    assert_eq!(changes.len(), 3);
    for (i, c) in changes.iter().enumerate() {
        assert_eq!((c.page_a, c.page_b), (i, i));
        assert_eq!(
            (c.artifact_type.as_str(), c.subtype.as_str()),
            ("Pagination", "Footer")
        );
        assert_eq!(c.a_text, artifact_texts(&a)[i]);
        assert_eq!(c.b_text, artifact_texts(&b)[i]);
        // Same origin, different text.
        let origin = |ink: &BTreeMap<Owner, Vec<Ink>>| match &ink[&Owner::Artifact(i)][0] {
            Ink::Glyphs { x, y, .. } => (*x, *y),
            Ink::Fill { .. } => panic!("footer is text"),
        };
        assert_eq!(origin(&ia), origin(&ib));
    }
    for owner in ia.keys().filter(|o| matches!(o, Owner::Elem(_))) {
        assert_eq!(ia[owner], ib[owner], "{owner:?}");
    }
    assert_same_tagging(&a, &b, "artifact-page-number");
    let streams: Vec<u32> = (0..3).flat_map(|p| a.content_streams(p)).collect();
    // Smoke: only the three content streams changed.
    assert!(assert_only_objects_differ(&built, &streams).is_empty());
}

// ---------------------------------------------------------------------------------------------
// identical-rows

#[test]
fn identical_rows_has_no_distinguishing_row_and_loses_one() {
    let built = built("identical-rows");
    let Mutation::ElementRemoved {
        element,
        parent,
        index_in_parent,
        descendants,
    } = mutation(&built.mutation, "element-removed")
    else {
        unreachable!()
    };
    let (a, b) = (Doc::read(&built.a), Doc::read(&built.b));
    let (ia, ib) = (ink_by_owner(&a), ink_by_owner(&b));
    let table = parent.a.unwrap();

    // In A every row is the same: role, cell roles, cell texts, glyph size and x.
    let rows = &a.elements[&table].kids;
    assert_eq!(rows.len(), 4);
    let signature = |row: u32| -> Vec<(String, Vec<String>, Vec<(i64, i64)>)> {
        a.elements[&row]
            .kids
            .iter()
            .map(|cell| {
                let glyphs = ia[&Owner::Elem(*cell)]
                    .iter()
                    .map(|i| match i {
                        Ink::Glyphs { x, size, .. } => (*x, *size),
                        Ink::Fill { .. } => panic!("text cells"),
                    })
                    .collect();
                (
                    a.elements[cell].role.clone(),
                    texts_of(&ia, &Owner::Elem(*cell)),
                    glyphs,
                )
            })
            .collect()
    };
    for &row in rows {
        assert_eq!(a.elements[&row].role, "TR");
        assert_eq!(signature(row), signature(rows[0]), "row {row} is a look-alike");
    }
    assert_eq!(
        signature(rows[0])
            .iter()
            .map(|c| c.1.concat())
            .collect::<Vec<_>>(),
        ["Balanced", "Yes"]
    );

    // B: the declared row and its cells are gone, nothing reflowed.
    assert_eq!((parent.role.as_str(), element.role.as_str()), ("Table", "TR"));
    assert_eq!(rows[*index_in_parent], element.a.unwrap());
    assert_eq!(element.b, None);
    let mut want = rows.clone();
    want.remove(*index_in_parent);
    assert_eq!(b.elements[&table].kids, want);
    assert_eq!(descendants.len(), 2);
    let removed: BTreeSet<u32> = std::iter::once(element.a.unwrap())
        .chain(descendants.iter().map(|d| d.a.unwrap()))
        .collect();
    assert!(descendants.iter().all(|d| d.b.is_none() && d.role == "TD"));
    for (owner, items) in &ia {
        match owner {
            Owner::Elem(e) if removed.contains(e) => assert!(!ib.contains_key(owner)),
            _ => assert_eq!(ib.get(owner), Some(items), "{owner:?} unchanged"),
        }
    }
    // The cells painted after the removed row get the next dense MCIDs, so their /K and the
    // ParentTree change too; nothing else does.
    let renumbered: Vec<u32> = b
        .elements
        .iter()
        .filter(|(o, e)| a.elements[*o].mcid != e.mcid)
        .map(|(o, _)| *o)
        .collect();
    let last_row = &a.elements[rows.last().unwrap()].kids;
    assert_eq!(&renumbered, last_row, "only the cells after the removed row");
    let page = descendants[0].page_a.unwrap();
    let mut changed = a.content_streams(page);
    changed.push(table);
    changed.push(a.parent_tree_obj);
    changed.extend(&renumbered);
    let only = assert_only_objects_differ(&built, &changed);
    // Smoke: the only objects that differ are gone, apart from the parent and the page content.
    assert!(removed.is_subset(&only));
    assert_eq!(b.elements.len(), a.elements.len() - 3);
}

// ---------------------------------------------------------------------------------------------
// untagged-edits

/// Ink of page `p`, owner dropped (an untagged file owns nothing).
fn page_ink(ink: &[(Owner, Ink)], p: usize) -> Vec<Ink> {
    let mut out: Vec<Ink> = ink
        .iter()
        .filter(|(o, _)| *o == Owner::Untagged(p))
        .map(|(_, i)| i.clone())
        .collect();
    out.sort();
    out
}

#[test]
fn untagged_edits_inserts_a_page_and_changes_only_the_declared_texts() {
    let built = built("untagged-edits");
    let Mutation::PageInserted {
        index,
        page_object,
        elements,
    } = mutation(&built.mutation, "page-inserted")
    else {
        unreachable!()
    };
    let Mutation::UntaggedTextChanged { changes } = mutation(&built.mutation, "untagged-text-changed") else {
        unreachable!()
    };
    let (a, b) = (Doc::read(&built.a), Doc::read(&built.b));
    let (ia, ib) = (ink(&a), ink(&b));

    assert!(a.elements.is_empty() && b.elements.is_empty() && elements.is_empty());
    let mut want_pages = a.pages.clone();
    want_pages.insert(*index, *page_object);
    assert_eq!(b.pages, want_pages, "old page objects kept, new one at {index}");
    for page_a in 0..a.pages.len() {
        let page_b = if page_a < *index { page_a } else { page_a + 1 };
        let declared: Vec<_> = changes.iter().filter(|c| c.page_a == page_a).collect();
        assert!(declared.iter().all(|c| c.page_b == page_b), "{declared:?}");
        // Replace each declared text of A's page by B's, at the same origin and size: that is
        // exactly B's page.
        let mut want = page_ink(&ia, page_a);
        for c in &declared {
            let line = want
                .iter_mut()
                .find(|i| matches!(i, Ink::Glyphs { text, .. } if *text == c.a_text))
                .unwrap_or_else(|| panic!("{c:?} not on A's page"));
            if let Ink::Glyphs { text, .. } = line {
                text.clone_from(&c.b_text);
            }
        }
        want.sort();
        assert_eq!(page_ink(&ib, page_b), want, "page {page_a} of A, {page_b} of B");
    }
    assert_eq!(
        changes.iter().map(|c| (c.page_a, c.page_b)).collect::<Vec<_>>(),
        [(1, 2), (1, 2), (2, 3)],
        "the Method page's paragraph and footer, then the last footer"
    );
    // Smoke: the first page is untouched, and the inserted page renders with ink.
    assert_eq!(page_ink(&ia, 0), page_ink(&ib, 0));
    assert!(painted_pixels(&render_all(&built.b)[*index].2) > 100);
}
