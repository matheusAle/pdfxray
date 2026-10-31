//! Facts the panels derive from the document once and then reuse every frame: per-page content
//! statistics and the structure-tree index (who owns which MCID, the role map, figures without
//! alternative text).

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use pdfxray_core::capture::{CapturedPage, PaintKind, StreamRef};
use pdfxray_core::numeric::usize_from_u32;
use pdfxray_core::{
    Coverage, Document, NodeId, ObjRef, OrderComparison, PageIx, ReadingOrder, ReadingOrderSource, Rect,
    SpeechPreset, TagBox, TreeKind, TreeNode,
};

use super::AppState;
use super::speech::SpeechData;
use super::tables::TableData;

/// What one page paints and how it is marked.
#[derive(Debug, Clone, PartialEq)]
pub struct PageStats {
    /// Tagged, artifact and untagged items.
    pub coverage: Coverage,
    /// Glyphs painted.
    pub text: u32,
    /// Images painted.
    pub image: u32,
    /// Paths (fills and strokes) painted.
    pub path: u32,
    /// Shadings painted.
    pub shading: u32,
    /// Marked-content sequences that carry an MCID.
    pub marked: u32,
    /// Operators on the page, nested form operators included.
    pub ops: u32,
    /// Distinct content streams the operators come from.
    pub streams: u32,
    /// The box of each operator that paints something untagged, with its execution index.
    pub untagged_boxes: Vec<(u32, Rect)>,
}

impl PageStats {
    fn of(doc: &Document, page: PageIx) -> Option<Self> {
        let coverage = doc.coverage(page)?;
        let captured = doc.captured(page)?;
        let mut stats = Self {
            coverage,
            text: 0,
            image: 0,
            path: 0,
            shading: 0,
            marked: u32::try_from(captured.marked.iter().filter(|m| m.mcid.is_some()).count())
                .unwrap_or(u32::MAX),
            ops: u32::try_from(captured.ops.len()).unwrap_or(u32::MAX),
            streams: distinct_streams(captured),
            untagged_boxes: Vec::new(),
        };
        for &ix in &stats.coverage.untagged_ops {
            if let Some(rect) = op_box(captured, ix) {
                stats.untagged_boxes.push((ix, rect));
            }
        }
        for item in &captured.items {
            match item.kind {
                PaintKind::Glyph { .. } => stats.text += 1,
                PaintKind::Image { .. } => stats.image += 1,
                PaintKind::Fill | PaintKind::Stroke | PaintKind::FillStroke => stats.path += 1,
                PaintKind::Shading => stats.shading += 1,
                PaintKind::Clip => {}
            }
        }
        Some(stats)
    }
}

/// The union of the boxes of the items operator `ix` painted.
fn op_box(captured: &CapturedPage, ix: u32) -> Option<Rect> {
    let op = captured.ops.get(usize_from_u32(ix)).filter(|op| op.ix == ix)?;
    let first = usize::try_from(op.first_item?).ok()?;
    captured
        .items
        .iter()
        .skip(first)
        .take(usize_from_u32(op.item_count))
        .filter(|item| !matches!(item.kind, PaintKind::Clip))
        .map(|item| item.bbox)
        .reduce(|a, b| Rect::new(a.x0.min(b.x0), a.y0.min(b.y0), a.x1.max(b.x1), a.y1.max(b.y1)))
}

fn distinct_streams(captured: &CapturedPage) -> u32 {
    let mut seen: Vec<&StreamRef> = Vec::new();
    for op in &captured.ops {
        if !seen.contains(&&op.stream) {
            seen.push(&op.stream);
        }
    }
    u32::try_from(seen.len()).unwrap_or(u32::MAX)
}

/// One `/RoleMap` entry as the structure tree shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoleMapEntry {
    /// The custom structure type.
    pub role: String,
    /// The standard type it maps to.
    pub standard: String,
    /// The namespace of the element, shortened (`pdf2 ns`), when it has one.
    pub namespace: Option<String>,
}

/// The structure tree, indexed for the other panels.
#[derive(Debug, Default)]
pub struct TagIndex {
    /// Structure elements in the tree.
    pub elements: usize,
    owners: HashMap<(PageIx, i32), NodeId>,
    missing_alt: HashSet<NodeId>,
    /// Role mappings used by at least one element, in tree order, once each.
    pub role_map: Vec<RoleMapEntry>,
}

impl TagIndex {
    fn build(doc: &Document) -> Self {
        let mut index = Self::default();
        for root in doc.tree(TreeKind::Tags) {
            index.walk(doc, &root, None);
        }
        index
    }

    fn walk(&mut self, doc: &Document, node: &TreeNode, owner: Option<&NodeId>) {
        let mut inner = owner;
        match &node.id {
            id @ NodeId::StructElem(_) => {
                self.elements += 1;
                inner = Some(id);
                let (role, standard) = split_role(&node.label);
                if standard.is_some_and(|s| s == "Figure") || (standard.is_none() && role == "Figure") {
                    let has_alt = doc
                        .node(id)
                        .is_some_and(|props| props.get("Element", "Alt").is_some());
                    if !has_alt {
                        self.missing_alt.insert(id.clone());
                    }
                }
                if let Some(standard) = standard
                    && !self
                        .role_map
                        .iter()
                        .any(|e| e.role == role && e.standard == standard)
                {
                    self.role_map.push(RoleMapEntry {
                        role: role.to_owned(),
                        standard: standard.to_owned(),
                        namespace: doc
                            .node(id)
                            .and_then(|p| p.get("Element", "Namespace").map(short_namespace)),
                    });
                }
            }
            NodeId::MarkedContent { page, mcid } => {
                if let Some(owner) = owner {
                    self.owners.entry((*page, *mcid)).or_insert_with(|| owner.clone());
                }
            }
            _ => {}
        }
        for child in &node.children {
            self.walk(doc, child, inner);
        }
    }

    /// The structure element whose `/K` holds MCID `mcid` of `page`.
    pub fn owner_of(&self, page: PageIx, mcid: i32) -> Option<&NodeId> {
        self.owners.get(&(page, mcid))
    }

    /// True for a Figure that has no `/Alt`.
    pub fn lacks_alt(&self, id: &NodeId) -> bool {
        self.missing_alt.contains(id)
    }
}

/// `Cell → TD` split into the role and the standard type it maps to.
pub fn split_role(label: &str) -> (&str, Option<&str>) {
    match label.split_once(" → ") {
        Some((role, standard)) => (role, Some(standard)),
        None => (label, None),
    }
}

fn short_namespace(uri: &str) -> String {
    if uri.ends_with("/pdf2/ssn") {
        "pdf2 ns".to_owned()
    } else if uri.ends_with("/pdf/ssn") {
        "pdf ns".to_owned()
    } else {
        uri.to_owned()
    }
}

/// The cache behind [`AppState::page_stats`], [`AppState::tag_index`] and the per-page tag
/// overlay data.
#[derive(Debug, Default)]
pub(crate) struct Derived {
    pages: HashMap<PageIx, Option<PageStats>>,
    tags: Option<TagIndex>,
    decoded: HashMap<ObjRef, Option<usize>>,
    tag_boxes: HashMap<PageIx, Arc<[TagBox]>>,
    orders: HashMap<(PageIx, ReadingOrderSource), Arc<ReadingOrder>>,
    comparisons: HashMap<PageIx, OrderComparison>,
    speech: HashMap<SpeechPreset, Arc<SpeechData>>,
    speech_pages: HashMap<(PageIx, SpeechPreset), Arc<ReadingOrder>>,
    tables: HashMap<NodeId, Option<Arc<TableData>>>,
}

impl Derived {
    pub(crate) fn clear(&mut self) {
        self.pages.clear();
        self.tags = None;
        self.decoded.clear();
        self.tag_boxes.clear();
        self.orders.clear();
        self.comparisons.clear();
        self.speech.clear();
        self.speech_pages.clear();
        self.tables.clear();
    }

    pub(crate) fn decoded_len(&mut self, doc: &Document, stream: ObjRef) -> Option<usize> {
        *self
            .decoded
            .entry(stream)
            .or_insert_with(|| doc.stream_bytes(stream)?.decoded.map(|bytes| bytes.len()))
    }

    pub(crate) fn page(&mut self, doc: &Document, page: PageIx) -> Option<&PageStats> {
        self.pages
            .entry(page)
            .or_insert_with(|| PageStats::of(doc, page))
            .as_ref()
    }

    pub(crate) fn tag_boxes(&mut self, doc: &Document, page: PageIx) -> Arc<[TagBox]> {
        Arc::clone(
            self.tag_boxes
                .entry(page)
                .or_insert_with(|| doc.tag_boxes(page).into()),
        )
    }

    pub(crate) fn reading_order(
        &mut self,
        doc: &Document,
        page: PageIx,
        source: ReadingOrderSource,
    ) -> Arc<ReadingOrder> {
        Arc::clone(
            self.orders
                .entry((page, source))
                .or_insert_with(|| Arc::new(doc.reading_order(page, source))),
        )
    }

    /// What the document announces under `preset`, read once.
    pub(crate) fn speech(&mut self, doc: &Document, preset: SpeechPreset) -> Arc<SpeechData> {
        Arc::clone(
            self.speech
                .entry(preset)
                .or_insert_with(|| Arc::new(SpeechData::new(doc.speech_order(preset)))),
        )
    }

    /// The speech data of `preset` if it was read already.
    pub(crate) fn speech_cached(&self, preset: SpeechPreset) -> Option<&Arc<SpeechData>> {
        self.speech.get(&preset)
    }

    /// The reading order of `page` as `preset` announces it, read once.
    pub(crate) fn speech_page(
        &mut self,
        doc: &Document,
        page: PageIx,
        preset: SpeechPreset,
    ) -> Arc<ReadingOrder> {
        let data = self.speech(doc, preset);
        Arc::clone(
            self.speech_pages
                .entry((page, preset))
                .or_insert_with(|| Arc::new(doc.speech_reading_order(page, &data.order))),
        )
    }

    /// The grid of the `Table` element `id` with the text of its cells, read once; `None` when
    /// `id` is not a table.
    pub(crate) fn table(&mut self, doc: &Document, id: &NodeId) -> Option<Arc<TableData>> {
        self.tables
            .entry(id.clone())
            .or_insert_with(|| TableData::read(doc, id).map(Arc::new))
            .clone()
    }

    pub(crate) fn comparison(&mut self, doc: &Document, page: PageIx) -> OrderComparison {
        self.comparisons
            .entry(page)
            .or_insert_with(|| doc.reading_order_comparison(page))
            .clone()
    }

    /// Reads the structure tree of a freshly opened document.
    pub(crate) fn index(&mut self, doc: &Document) {
        self.tags = Some(TagIndex::build(doc));
    }

    pub(crate) fn tags(&self) -> Option<&TagIndex> {
        self.tags.as_ref()
    }
}

impl AppState {
    /// What `page` paints and how it is marked; `None` without a document or captured content.
    pub fn page_stats(&mut self, page: PageIx) -> Option<&PageStats> {
        let doc = &self.document.as_ref()?.doc;
        self.derived.page(doc, page)
    }

    /// The size of the decoded data of stream `stream`, read once; `None` when it has no data.
    pub fn decoded_len(&mut self, stream: ObjRef) -> Option<usize> {
        let doc = &self.document.as_ref()?.doc;
        self.derived.decoded_len(doc, stream)
    }

    /// Every structure element that draws on `page`, with its box (see [`Document::tag_boxes`]);
    /// empty without a document. Read once per page.
    pub fn tag_boxes(&mut self, page: PageIx) -> Arc<[TagBox]> {
        match self.document.as_ref() {
            Some(loaded) => self.derived.tag_boxes(&loaded.doc, page),
            None => Arc::from([]),
        }
    }

    /// The reading order of `page` from `source`; `None` without a document. Read once per page.
    pub fn reading_order(&mut self, page: PageIx, source: ReadingOrderSource) -> Option<Arc<ReadingOrder>> {
        let doc = &self.document.as_ref()?.doc;
        Some(self.derived.reading_order(doc, page, source))
    }

    /// How far the structure order of `page` and its paint order agree; `None` without a
    /// document.
    pub fn order_comparison(&mut self, page: PageIx) -> Option<OrderComparison> {
        let doc = &self.document.as_ref()?.doc;
        Some(self.derived.comparison(doc, page))
    }

    /// The structure tree indexed; `None` without a document.
    pub fn tag_index(&self) -> Option<&TagIndex> {
        self.derived.tags()
    }
}
