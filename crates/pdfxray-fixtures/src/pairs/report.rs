//! The base document every pair mutates, and the knobs ([`Edit`]) that mutate it.
//!
//! A three-page tagged report: a title, headings, paragraphs, two figures with `/Alt` and a
//! Layout `/BBox`, a table of four visually and textually identical rows, and a page-number
//! footer artifact on every page. It is modelled once, as a tree of labelled elements over
//! integer-coordinate ink, and rendered into an [`ObjectGraph`] so that numbering, content
//! encoding and every mutation are independent choices.

use std::collections::BTreeMap;

use crate::graph::{Numbering, ObjectGraph};
use crate::writer::{PdfVersion, WriterError};

/// Page width and height in points.
pub(super) const PAGE: (i32, i32) = (300, 400);
/// Footer origin before any offset.
const FOOTER_AT: (i32, i32) = (135, 20);
/// Gray level of figure fills.
const FIGURE_GRAY: &str = "0.25";

/// A filled rectangle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Fill {
    x: i32,
    y: i32,
    w: i32,
    h: i32,
}

const fn fill(x: i32, y: i32, w: i32, h: i32) -> Fill {
    Fill { x, y, w, h }
}

/// The ink of one leaf element.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Ink {
    /// One line of Helvetica.
    Text {
        size: i32,
        x: i32,
        y: i32,
        text: &'static str,
    },
    /// Gray filled rectangles.
    Figure(Vec<Fill>),
}

/// A structure element of the model.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Node {
    label: String,
    role: &'static str,
    alt: Option<&'static str>,
    /// Page key (0, 1, 2 for the base pages; [`INSERTED_PAGE`] for the inserted one).
    page: usize,
    kids: Kids,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Kids {
    Ink(Ink),
    Elems(Vec<Node>),
}

/// Page key of the page `insert-page` adds.
pub(super) const INSERTED_PAGE: usize = 3;

fn text(
    label: &str,
    role: &'static str,
    page: usize,
    size: i32,
    (x, y): (i32, i32),
    text: &'static str,
) -> Node {
    Node {
        label: label.to_owned(),
        role,
        alt: None,
        page,
        kids: Kids::Ink(Ink::Text { size, x, y, text }),
    }
}

fn figure(label: &str, page: usize, alt: &'static str, fills: Vec<Fill>) -> Node {
    Node {
        label: label.to_owned(),
        role: "Figure",
        alt: Some(alt),
        page,
        kids: Kids::Ink(Ink::Figure(fills)),
    }
}

/// Text of every identical table row: label cell, value cell.
pub(super) const ROW_TEXT: [&str; 2] = ["Balanced", "Yes"];
/// Baselines of the four table rows.
const ROW_Y: [i32; 4] = [290, 270, 250, 230];

/// The base document's elements below `Document`, in `/K` order (which is also paint order).
fn base_tree() -> Vec<Node> {
    let rows = ROW_Y
        .iter()
        .enumerate()
        .map(|(i, &y)| Node {
            label: format!("el.row{i}"),
            role: "TR",
            alt: None,
            page: 2,
            kids: Kids::Elems(vec![
                text(&format!("el.row{i}.label"), "TD", 2, 11, (30, y), ROW_TEXT[0]),
                text(&format!("el.row{i}.value"), "TD", 2, 11, (150, y), ROW_TEXT[1]),
            ]),
        })
        .collect();
    vec![
        text("el.title", "H1", 0, 20, (30, 350), "Quarterly report"),
        text(
            "el.intro",
            "P",
            0,
            11,
            (30, 320),
            "This report covers the third quarter.",
        ),
        text("el.background", "H2", 0, 14, (30, 290), "Background"),
        text(
            "el.growth",
            "P",
            0,
            11,
            (30, 266),
            "Revenue grew in every region.",
        ),
        figure(
            "el.chart",
            0,
            "Bar chart of revenue by region",
            vec![
                fill(30, 148, 120, 1),
                fill(40, 150, 25, 60),
                fill(80, 150, 25, 80),
                fill(120, 150, 25, 45),
            ],
        ),
        text("el.method", "H2", 1, 14, (30, 350), "Method"),
        text(
            "el.compare",
            "P",
            1,
            11,
            (30, 326),
            "We compared invoices against the ledger.",
        ),
        text(
            "el.audit",
            "P",
            1,
            11,
            (30, 306),
            "Each region was audited twice.",
        ),
        figure(
            "el.flow",
            1,
            "Flow diagram of the audit",
            vec![
                fill(30, 200, 60, 40),
                fill(90, 219, 30, 2),
                fill(120, 200, 60, 40),
            ],
        ),
        text("el.results", "H2", 2, 14, (30, 350), "Results"),
        text("el.balanced", "P", 2, 11, (30, 326), "All regions balanced."),
        Node {
            label: "el.table".to_owned(),
            role: "Table",
            alt: None,
            page: 2,
            kids: Kids::Elems(rows),
        },
    ]
}

/// The elements of the page `insert-page` adds.
fn inserted_tree() -> Vec<Node> {
    vec![
        text(
            "el.note-heading",
            "H2",
            INSERTED_PAGE,
            14,
            (30, 350),
            "Interim note",
        ),
        text(
            "el.note",
            "P",
            INSERTED_PAGE,
            11,
            (30, 326),
            "This page was added after review.",
        ),
    ]
}

/// How the page-number footer reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(super) enum Footer {
    /// `Page 3`.
    #[default]
    Word,
    /// `- 3 -`.
    Dashes,
}

impl Footer {
    pub(super) fn text(self, page_index: usize) -> String {
        let n = page_index + 1;
        match self {
            Self::Word => format!("Page {n}"),
            Self::Dashes => format!("- {n} -"),
        }
    }
}

/// How objects are numbered and ordered in the file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(super) enum Numbers<'a> {
    /// `1, 2, 3...` in declaration order, written in declaration order.
    #[default]
    Sequential,
    /// The numbers of the given side for every label it has; new labels get fresh numbers after
    /// the largest one. Written in declaration order.
    Stable(&'a Numbering),
    /// Every object gets `SCRAMBLE_BASE - 3 * i` (declaration index `i`): reversed, sparse, none
    /// equal to its sequential number. Written in label order, unlike either numbering.
    Scrambled,
}

/// First number of [`Numbers::Scrambled`].
const SCRAMBLE_BASE: u32 = 200;

/// The mutation knobs. [`Edit::default`] is the base document.
#[derive(Debug, Clone, Copy, Default)]
pub(super) struct Edit<'a> {
    /// Translation applied to every coordinate of the page content.
    pub(super) offset: (i32, i32),
    /// One element's ink moved by `(dx, dy)`.
    pub(super) moved: Option<(&'a str, (i32, i32))>,
    /// Two children of `Document` swapped in `/K`.
    pub(super) swap: Option<(&'a str, &'a str)>,
    /// `/Alt` dropped from this element.
    pub(super) drop_alt: Option<&'a str>,
    /// `/S` of this element replaced.
    pub(super) retag: Option<(&'a str, &'static str)>,
    /// The note page inserted at this page index.
    pub(super) insert_page_at: Option<usize>,
    /// Footer wording.
    pub(super) footer: Footer,
    /// This table row removed (no reflow).
    pub(super) remove_row: Option<usize>,
    /// Content streams rewritten into an equivalent but different encoding.
    pub(super) rewrite_content: bool,
    /// Numbering and byte order.
    pub(super) numbers: Numbers<'a>,
}

/// What the generator knows about one element of a built side.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ElemInfo {
    pub(super) role: String,
    pub(super) text: Option<String>,
    pub(super) alt: Option<String>,
    /// Page index (display order), for leaves.
    pub(super) page: Option<usize>,
    pub(super) parent: String,
    /// Child labels in `/K` order.
    pub(super) kids: Vec<String>,
}

/// What the generator knows about one page of a built side.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct PageInfo {
    pub(super) key: usize,
    pub(super) label: String,
    pub(super) footer: String,
    pub(super) streams: Vec<String>,
    pub(super) forms: Vec<String>,
    /// `(element label, MCID)` in paint order.
    pub(super) mcids: Vec<(String, i32)>,
}

/// A built document plus the generator's view of it.
#[derive(Debug, Clone)]
pub(super) struct Side {
    pub(super) pdf: Vec<u8>,
    pub(super) numbers: Numbering,
    /// Labels in declaration order.
    pub(super) labels: Vec<String>,
    /// Elements by label; `el.document` included.
    pub(super) elements: BTreeMap<String, ElemInfo>,
    /// Pages in display order.
    pub(super) pages: Vec<PageInfo>,
}

impl Side {
    pub(super) fn number(&self, label: &str) -> Option<u32> {
        self.numbers.get(label).copied()
    }
}

/// A leaf on a page, in paint order.
struct Leaf<'n> {
    label: &'n str,
    /// Content-stream tag: always the *base* role, retagging does not touch content.
    tag: &'static str,
    ink: Ink,
}

/// Builds one side.
pub(super) fn build(edit: &Edit<'_>) -> Result<Side, WriterError> {
    let mut tree = base_tree();
    let mut order: Vec<usize> = vec![0, 1, 2];
    if let Some(at) = edit.insert_page_at {
        order.insert(at, INSERTED_PAGE);
        let index_of = |key: usize| order.iter().position(|&k| k == key).unwrap_or(usize::MAX);
        let before = tree
            .iter()
            .position(|n| index_of(n.page) > at)
            .unwrap_or(tree.len());
        tree.splice(before..before, inserted_tree());
    }
    if let Some(row) = edit.remove_row {
        for node in &mut tree {
            if let Kids::Elems(kids) = &mut node.kids {
                kids.retain(|k| k.label != format!("el.row{row}"));
            }
        }
    }
    let page_index = |key: usize| order.iter().position(|&k| k == key);

    // Paint order is the base tree order: swapping `/K` must not move ink.
    let mut leaves: BTreeMap<usize, Vec<Leaf<'_>>> = BTreeMap::new();
    collect_leaves(&tree, &mut leaves);
    for leaf in leaves.values_mut().flatten() {
        let (mx, my) = match edit.moved {
            Some((label, delta)) if label == leaf.label => delta,
            _ => (0, 0),
        };
        leaf.ink = translate(&leaf.ink, (edit.offset.0 + mx, edit.offset.1 + my));
    }

    let mut g = ObjectGraph::new();
    g.object(
        "catalog",
        "<< /Type /Catalog /Pages @pages /StructTreeRoot @struct.root /MarkInfo << /Marked true >> /Lang (en-US) >>",
    );
    let kids: Vec<String> = order.iter().map(|k| format!("@page.{k}")).collect();
    g.object(
        "pages",
        format!(
            "<< /Type /Pages /Kids [{}] /Count {} >>",
            kids.join(" "),
            order.len()
        ),
    );
    g.object(
        "font.helv",
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>",
    );

    let mut pages = Vec::new();
    let mut mcid_of: BTreeMap<String, (usize, i32)> = BTreeMap::new();
    for (index, &key) in order.iter().enumerate() {
        let page_leaves = leaves.remove(&key).unwrap_or_default();
        let count = i32::try_from(page_leaves.len()).unwrap_or(i32::MAX);
        let mcids: Vec<(String, i32)> = page_leaves
            .iter()
            .zip(0..)
            .map(|(leaf, i)| {
                let m = if edit.rewrite_content { count - 1 - i } else { i };
                (leaf.label.to_owned(), m)
            })
            .collect();
        for (label, m) in &mcids {
            mcid_of.insert(label.clone(), (key, *m));
        }
        let footer = edit.footer.text(index);
        let at = (FOOTER_AT.0 + edit.offset.0, FOOTER_AT.1 + edit.offset.1);
        let (streams, forms) = if edit.rewrite_content {
            rewritten_content(&mut g, key, &page_leaves, &mcids, &footer, at)
        } else {
            let data = plain_content(&page_leaves, &mcids, &footer, at);
            let label = format!("content.{key}");
            g.stream(label.clone(), "", data.into_bytes());
            (vec![label], Vec::new())
        };
        let contents = match streams.as_slice() {
            [one] => format!("@{one}"),
            many => format!(
                "[{}]",
                many.iter().map(|s| format!("@{s}")).collect::<Vec<_>>().join(" ")
            ),
        };
        let xobjects = if forms.is_empty() {
            String::new()
        } else {
            let entries: Vec<String> = forms
                .iter()
                .enumerate()
                .map(|(i, f)| format!("/Fx{i} @{f}"))
                .collect();
            format!(" /XObject << {} >>", entries.join(" "))
        };
        let label = format!("page.{key}");
        g.object(
            label.clone(),
            format!(
                "<< /Type /Page /Parent @pages /MediaBox [0 0 {} {}] /StructParents {key} /Tabs /S /Resources << /Font << /F1 @font.helv >>{xobjects} >> /Contents {contents} >>",
                PAGE.0, PAGE.1
            ),
        );
        pages.push(PageInfo {
            key,
            label,
            footer,
            streams,
            forms,
            mcids,
        });
    }

    // Structure tree.
    let mut elements = BTreeMap::new();
    let mut top: Vec<String> = tree.iter().map(|n| n.label.clone()).collect();
    if let Some((x, y)) = edit.swap {
        let (i, j) = (top.iter().position(|l| l == x), top.iter().position(|l| l == y));
        if let (Some(i), Some(j)) = (i, j) {
            top.swap(i, j);
        }
    }
    g.object(
        "struct.root",
        format!(
            "<< /Type /StructTreeRoot /K @el.document /ParentTree @parent-tree /ParentTreeNextKey {} >>",
            order.iter().max().map_or(0, |m| m + 1)
        ),
    );
    g.object(
        "el.document",
        format!(
            "<< /Type /StructElem /S /Document /P @struct.root /K [{}] >>",
            refs(&top)
        ),
    );
    elements.insert(
        "el.document".to_owned(),
        ElemInfo {
            role: "Document".to_owned(),
            text: None,
            alt: None,
            page: None,
            parent: "struct.root".to_owned(),
            kids: top.clone(),
        },
    );
    let by_label: BTreeMap<&str, &Node> = tree.iter().map(|n| (n.label.as_str(), n)).collect();
    for label in &top {
        if let Some(node) = by_label.get(label.as_str()) {
            emit_element(&mut g, &mut elements, node, "el.document", edit, &mcid_of, &|k| {
                page_index(k)
            });
        }
    }

    // Parent tree: per page key, owners indexed by MCID.
    let mut nums = Vec::new();
    let mut keys: Vec<&PageInfo> = pages.iter().collect();
    keys.sort_by_key(|p| p.key);
    for page in keys {
        let mut owners = page.mcids.clone();
        owners.sort_by_key(|(_, m)| *m);
        let arr: Vec<String> = owners.iter().map(|(l, _)| format!("@{l}")).collect();
        nums.push(format!("{} [{}]", page.key, arr.join(" ")));
    }
    g.object("parent-tree", format!("<< /Nums [{}] >>", nums.join(" ")));

    let labels: Vec<String> = g.labels().map(str::to_owned).collect();
    let (numbers, write_order) = numbering(&labels, edit.numbers);
    let order_refs: Vec<&str> = write_order.iter().map(String::as_str).collect();
    let out = g.write(PdfVersion::V1_7, &numbers, &order_refs, "catalog")?;
    Ok(Side {
        pdf: out.bytes,
        numbers,
        labels,
        elements,
        pages,
    })
}

fn refs(labels: &[String]) -> String {
    labels
        .iter()
        .map(|l| format!("@{l}"))
        .collect::<Vec<_>>()
        .join(" ")
}

fn numbering(labels: &[String], numbers: Numbers<'_>) -> (Numbering, Vec<String>) {
    match numbers {
        Numbers::Sequential => (labels.iter().cloned().zip(1..).collect(), labels.to_vec()),
        Numbers::Stable(base) => {
            let mut next = base.values().max().copied().unwrap_or(0) + 1;
            let mut map = Numbering::new();
            for label in labels {
                let n = base.get(label).copied().unwrap_or_else(|| {
                    next += 1;
                    next - 1
                });
                map.insert(label.clone(), n);
            }
            (map, labels.to_vec())
        }
        Numbers::Scrambled => {
            let map = labels
                .iter()
                .cloned()
                .zip((0..).map(|i: u32| SCRAMBLE_BASE - 3 * i))
                .collect();
            let mut order = labels.to_vec();
            order.sort();
            (map, order)
        }
    }
}

fn collect_leaves<'n>(nodes: &'n [Node], out: &mut BTreeMap<usize, Vec<Leaf<'n>>>) {
    for node in nodes {
        match &node.kids {
            Kids::Ink(ink) => out.entry(node.page).or_default().push(Leaf {
                label: &node.label,
                tag: node.role,
                ink: ink.clone(),
            }),
            Kids::Elems(kids) => collect_leaves(kids, out),
        }
    }
}

fn translate(ink: &Ink, (dx, dy): (i32, i32)) -> Ink {
    match ink {
        Ink::Text { size, x, y, text } => Ink::Text {
            size: *size,
            x: x + dx,
            y: y + dy,
            text,
        },
        Ink::Figure(fills) => Ink::Figure(fills.iter().map(|f| fill(f.x + dx, f.y + dy, f.w, f.h)).collect()),
    }
}

/// Union of the fills: `[x0 y0 x1 y1]`.
fn bounds(fills: &[Fill]) -> [i32; 4] {
    let x0 = fills.iter().map(|f| f.x).min().unwrap_or(0);
    let y0 = fills.iter().map(|f| f.y).min().unwrap_or(0);
    let x1 = fills.iter().map(|f| f.x + f.w).max().unwrap_or(0);
    let y1 = fills.iter().map(|f| f.y + f.h).max().unwrap_or(0);
    [x0, y0, x1, y1]
}

fn emit_element(
    g: &mut ObjectGraph,
    elements: &mut BTreeMap<String, ElemInfo>,
    node: &Node,
    parent: &str,
    edit: &Edit<'_>,
    mcid_of: &BTreeMap<String, (usize, i32)>,
    page_index: &dyn Fn(usize) -> Option<usize>,
) {
    let role = match edit.retag {
        Some((label, to)) if label == node.label => to,
        _ => node.role,
    };
    let alt = node.alt.filter(|_| edit.drop_alt != Some(node.label.as_str()));
    let mut dict = format!("<< /Type /StructElem /S /{role} /P @{parent}");
    let (kids, text, page) = match &node.kids {
        Kids::Ink(ink) => {
            let (key, mcid) = mcid_of.get(&node.label).copied().unwrap_or((node.page, -1));
            dict.push_str(" /Pg @page.");
            dict.push_str(&key.to_string());
            dict.push_str(" /K ");
            dict.push_str(&mcid.to_string());
            let text = match ink {
                Ink::Text { text, .. } => Some((*text).to_owned()),
                Ink::Figure(_) => None,
            };
            (Vec::new(), text, page_index(key))
        }
        Kids::Elems(kids) => {
            let labels: Vec<String> = kids.iter().map(|k| k.label.clone()).collect();
            dict.push_str(" /K [");
            dict.push_str(&refs(&labels));
            dict.push(']');
            (labels, None, None)
        }
    };
    if let Some(alt) = alt {
        dict.push_str(" /Alt (");
        dict.push_str(alt);
        dict.push(')');
    }
    if let Kids::Ink(Ink::Figure(fills)) = &node.kids {
        let moved = match edit.moved {
            Some((label, d)) if label == node.label => d,
            _ => (0, 0),
        };
        let shifted: Vec<Fill> = fills
            .iter()
            .map(|f| {
                fill(
                    f.x + edit.offset.0 + moved.0,
                    f.y + edit.offset.1 + moved.1,
                    f.w,
                    f.h,
                )
            })
            .collect();
        let [x0, y0, x1, y1] = bounds(&shifted);
        let bbox = format!(" /A << /O /Layout /BBox [{x0} {y0} {x1} {y1}] >>");
        dict.push_str(&bbox);
    }
    dict.push_str(" >>");
    g.object(node.label.clone(), dict);
    elements.insert(
        node.label.clone(),
        ElemInfo {
            role: role.to_owned(),
            text,
            alt: alt.map(str::to_owned),
            page,
            parent: parent.to_owned(),
            kids,
        },
    );
    if let Kids::Elems(kids) = &node.kids {
        for kid in kids {
            emit_element(g, elements, kid, &node.label, edit, mcid_of, page_index);
        }
    }
}

const FOOTER_BDC: &str = "/Artifact << /Type /Pagination /Subtype /Footer >> BDC";

/// One stream, one `BT`/`ET` or `q`/`Q` per element, absolute `Td`, `re f` fills.
fn plain_content(leaves: &[Leaf<'_>], mcids: &[(String, i32)], footer: &str, (fx, fy): (i32, i32)) -> String {
    let mut lines = Vec::new();
    for (leaf, (_, mcid)) in leaves.iter().zip(mcids) {
        let tag = leaf.tag;
        lines.push(match &leaf.ink {
            Ink::Text { size, x, y, text } => {
                format!("/{tag} << /MCID {mcid} >> BDC BT /F1 {size} Tf {x} {y} Td ({text}) Tj ET EMC")
            }
            Ink::Figure(fills) => {
                let rects: Vec<String> = fills
                    .iter()
                    .map(|f| format!("{} {} {} {} re f", f.x, f.y, f.w, f.h))
                    .collect();
                format!(
                    "/{tag} << /MCID {mcid} >> BDC q {FIGURE_GRAY} g {} Q EMC",
                    rects.join(" ")
                )
            }
        });
    }
    lines.push(format!(
        "{FOOTER_BDC} BT /F1 9 Tf {fx} {fy} Td ({footer}) Tj ET EMC"
    ));
    lines.join("\n")
}

/// Origin shift of the `q ... cm` wrapping every run of text in [`rewritten_content`].
const TEXT_RUN_SHIFT: (i32, i32) = (10, 5);
/// Number of content streams a rewritten page is split into.
const SPLIT_INTO: usize = 3;

/// Equivalent content, encoded differently: consecutive text elements share one `BT`/`ET` under a
/// shifted `cm` and move with relative `Td`; every other string is split across a `TJ` array;
/// figures are painted by form `XObject`s (one placed with `cm`, the next with `/Matrix`) built from
/// `m`/`l`/`h` paths; the footer is reached with two `Td`; the result is split into
/// [`SPLIT_INTO`] streams at token boundaries inside operations, the later ones Flate-compressed.
/// Returns the stream and form labels it added to `g`.
fn rewritten_content(
    graph: &mut ObjectGraph,
    key: usize,
    leaves: &[Leaf<'_>],
    mcids: &[(String, i32)],
    footer: &str,
    (fx, fy): (i32, i32),
) -> (Vec<String>, Vec<String>) {
    let mut ops: Vec<String> = Vec::new();
    let mut forms = Vec::new();
    let mut in_text: Option<(i32, i32)> = None; // current line start, in shifted space
    let (sx, sy) = TEXT_RUN_SHIFT;
    for (i, (leaf, (_, mcid))) in leaves.iter().zip(mcids).enumerate() {
        let tag = leaf.tag;
        match &leaf.ink {
            Ink::Text { size, x, y, text } => {
                let (tx, ty) = (x - sx, y - sy);
                let (dx, dy) = if let Some((lx, ly)) = in_text {
                    (tx - lx, ty - ly)
                } else {
                    ops.push(format!("q 1 0 0 1 {sx} {sy} cm BT"));
                    (tx, ty)
                };
                in_text = Some((tx, ty));
                let show = if i % 2 == 0 {
                    let mid = text.len() / 2;
                    format!("[({}) ({})] TJ", &text[..mid], &text[mid..])
                } else {
                    format!("({text}) Tj")
                };
                ops.push(format!(
                    "/{tag} << /MCID {mcid} >> BDC /F1 {size} Tf {dx} {dy} Td {show} EMC"
                ));
            }
            Ink::Figure(fills) => {
                if in_text.take().is_some() {
                    ops.push("ET Q".to_owned());
                }
                let [x0, y0, x1, y1] = bounds(fills);
                let paths: Vec<String> = fills
                    .iter()
                    .map(|f| {
                        let (left, bottom) = (f.x - x0, f.y - y0);
                        let (right, top) = (left + f.w, bottom + f.h);
                        format!("{left} {bottom} m {right} {bottom} l {right} {top} l {left} {top} l h f")
                    })
                    .collect();
                let name = format!("Fx{}", forms.len());
                let label = format!("form.{key}.{}", forms.len());
                let by_matrix = (key + forms.len()) % 2 == 1;
                let matrix = if by_matrix {
                    format!(" /Matrix [1 0 0 1 {x0} {y0}]")
                } else {
                    String::new()
                };
                graph.stream(
                    label.clone(),
                    format!(
                        "/Type /XObject /Subtype /Form /BBox [-1 -1 {} {}]{matrix} /Resources << >>",
                        x1 - x0 + 1,
                        y1 - y0 + 1
                    ),
                    format!("{FIGURE_GRAY} g\n{}", paths.join("\n")).into_bytes(),
                );
                forms.push(label);
                ops.push(if by_matrix {
                    format!("/{tag} << /MCID {mcid} >> BDC /{name} Do EMC")
                } else {
                    format!("/{tag} << /MCID {mcid} >> BDC q 1 0 0 1 {x0} {y0} cm /{name} Do Q EMC")
                });
            }
        }
    }
    if in_text.take().is_some() {
        ops.push("ET Q".to_owned());
    }
    ops.push(format!(
        "{FOOTER_BDC} BT /F1 9 Tf {fx} {} Td 0 -11 Td ({footer}) Tj ET EMC",
        fy + 11
    ));
    let content = ops.join("\n");

    let parts = split_at_tokens(&content, SPLIT_INTO);
    let mut streams = Vec::new();
    for (i, part) in parts.iter().enumerate() {
        let label = format!("content.{key}.{i}");
        if i == 0 {
            graph.stream(label.clone(), "", part.as_bytes().to_vec());
        } else {
            graph.stream(
                label.clone(),
                "/Filter /FlateDecode",
                crate::deflate::zlib(part.as_bytes()),
            );
        }
        streams.push(label);
    }
    (streams, forms)
}

/// Splits `content` into `n` parts at spaces outside string literals, near equal lengths. The
/// separating space is dropped: consecutive `/Contents` streams are separated by whitespace anyway.
pub(super) fn split_at_tokens(content: &str, n: usize) -> Vec<String> {
    let mut boundaries = Vec::new();
    let mut depth = 0u32;
    for (i, c) in content.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => depth = depth.saturating_sub(1),
            ' ' if depth == 0 => boundaries.push(i),
            _ => {}
        }
    }
    let mut cuts = Vec::new();
    for k in 1..n {
        let target = content.len() * k / n;
        if let Some(&cut) = boundaries
            .iter()
            .find(|&&b| b >= target && cuts.last().is_none_or(|&l| b > l))
        {
            cuts.push(cut);
        }
    }
    let mut parts = Vec::new();
    let mut start = 0;
    for cut in cuts {
        parts.push(content[start..cut].to_owned());
        start = cut + 1;
    }
    parts.push(content[start..].to_owned());
    parts
}
