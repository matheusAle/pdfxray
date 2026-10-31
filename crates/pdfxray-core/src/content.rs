//! Page content as nodes: operators, glyphs, marked-content sequences and artifacts, built
//! from [`crate::capture`]. Owns hit-testing of painted items and their geometry.
//!
//! Contract with the `document` module (keep these signatures):
//! - `ContentIndex::build(engine, page_count, progress)` captures every page eagerly at open.
//! - `Document::{content_tree, artifacts_tree, content_props, content_hits, content_bbox,
//!   content_ancestors}` answer the query API for `NodeId::{Op, Glyph, MarkedContent,
//!   Artifact}` and `TreeKind::{Content, Artifacts}`.
//!
//! # Node semantics
//!
//! * `Op { page, ix }`: `ix` is [`OpRecord::ix`], the execution index including operators
//!   inside Form `XObject`s. In the content tree a `BDC`/`BMC` is the parent of everything up to
//!   and including its `EMC`; a `Do` that drew a Form `XObject` is the parent of the form's
//!   operators. Every captured operator appears exactly once.
//! * `Glyph { page, op, glyph }`: `glyph` is [`PaintKind::Glyph::glyph_index`].
//! * `MarkedContent { page, mcid }`: all sequences with that MCID on the page, whichever content
//!   stream (page or Form `XObject`) they were written in. Props report how many there were.
//! * `Artifact { page, seq }`: `seq` numbers the artifact sequences of the page in paint order
//!   (nested artifacts included).
//!
//! # Helpers for the tags module
//!
//! The structure tree links elements to content by `(page, MCID)`. These `pub(crate)` methods on
//! [`Document`] answer from the captured content; all of them treat "every sequence with that
//! MCID on the page" as the content of an MCID.
//!
//! * [`Document::mcids_on_page`]: sorted, unique MCIDs present on a page.
//! * [`Document::mc_bbox`]: union of the painted boxes of the sequences with that MCID.
//! * [`Document::mc_text_each`]: one stream's glyph text for an MCID, in paint order, handed over
//!   glyph by glyph until the caller stops.
//! * [`Document::mc_bbox_in`] / [`Document::mc_text_each`]: the same restricted to the sequences
//!   written in one content stream (`None` = the page's own `/Contents`, `Some(obj)` = that Form
//!   `XObject`), for structure elements that carry `/Stm`. MCIDs are only unique per stream.

mod coverage;
mod glyph;
mod paint;
mod props;
mod runs;
mod tree;

pub use self::coverage::Coverage;
pub(crate) use self::glyph::char_code_of;
pub use self::glyph::{ActualText, CharCode, GlyphInfo, UnicodeSource};
pub use self::paint::{MAX_PAINT_STACK, PaintStep};
pub(crate) use self::runs::RunKind;

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::atomic::Ordering;
use std::sync::{OnceLock, mpsc};

use hayro_syntax::Pdf;

use crate::capture::{
    CapturedPage, MarkedSeq, OpRecord, PageIndex, PaintItem, PaintKind, StreamRef, capture_page,
};
use crate::numeric::usize_from_u32;
use crate::open::{OpenStage, Progress};
use crate::{CoreError, Document, LayerMode, NodeId, ObjRef, PageIx, Point, Props, Rect, TreeNode};

/// Documents with at least this many pages are captured on several threads; below it the thread
/// hand-off costs more than it saves.
const PARALLEL_FROM: usize = 4;

/// Captured content of every page, with a spatial index per page.
#[derive(Debug, Default)]
pub(crate) struct ContentIndex {
    pages: Vec<Option<PageData>>,
}

impl ContentIndex {
    /// Captures every page. Progress is reported on the calling thread after each page; the
    /// pages are captured on several threads for documents of [`PARALLEL_FROM`] pages or more.
    /// Fails with [`CoreError::Cancelled`] when the caller's flag is raised meanwhile.
    pub(crate) fn build(
        engine: Option<&Pdf>,
        page_count: usize,
        progress: &mut Progress<'_>,
    ) -> Result<Self, CoreError> {
        let cancel = progress.cancel_flag();
        let capture = |i: usize| {
            if cancel.load(Ordering::Relaxed) {
                return None;
            }
            let pdf = engine?;
            capture_page(pdf, u32::try_from(i).ok()?).map(PageData::new)
        };
        progress.report(OpenStage::CaptureContent, 0, page_count);
        let pages = if page_count >= PARALLEL_FROM {
            let (done_tx, done_rx) = mpsc::channel::<()>();
            let joined = std::thread::scope(|scope| {
                let worker = scope.spawn(move || {
                    crate::parallel::map_pages(page_count, |i| {
                        let page = capture(i);
                        // The receiver only goes away when this scope is unwinding.
                        let _ = done_tx.send(());
                        page
                    })
                });
                // The channel closes when the worker (and with it the only sender) is done.
                for (done, ()) in (1..).zip(done_rx) {
                    progress.report(OpenStage::CaptureContent, done, page_count);
                }
                worker.join()
            });
            joined.unwrap_or_else(|panic| std::panic::resume_unwind(panic))
        } else {
            let mut pages = Vec::with_capacity(page_count);
            for i in 0..page_count {
                pages.push(capture(i));
                progress.report(OpenStage::CaptureContent, i + 1, page_count);
            }
            pages
        };
        progress.check()?;
        if page_count == 0 {
            progress.report(OpenStage::CaptureContent, 0, 0);
        }
        Ok(Self { pages })
    }

    pub(crate) fn page(&self, page: PageIx) -> Option<&CapturedPage> {
        self.data(page).map(|d| &d.cap)
    }

    fn data(&self, page: PageIx) -> Option<&PageData> {
        self.pages.get(usize_from_u32(page))?.as_ref()
    }
}

/// One captured page plus the lookup tables the queries need.
#[derive(Debug)]
struct PageData {
    cap: CapturedPage,
    /// Spatial index for hit testing, built by the first [`PageData::index`] call: pages nobody
    /// clicks on never pay for it.
    index: OnceLock<PageIndex>,
    /// Per operator: innermost marked-content sequence it is inside (a `BDC` is outside its own
    /// sequence, an `EMC` inside).
    op_mc: Vec<Option<u32>>,
    /// Per sequence: paint-order indices of the glyph items painted inside it (nested included).
    seq_glyphs: Vec<Vec<u32>>,
    /// Per sequence: its number among the page's artifact sequences.
    art_no: Vec<Option<u32>>,
    /// Artifact number to sequence index.
    artifacts: Vec<u32>,
    /// MCID to the sequences carrying it, in order of appearance.
    by_mcid: BTreeMap<i32, Vec<u32>>,
    /// Opening operator to sequence.
    begin_of: HashMap<u32, u32>,
    /// Closing operator to sequence.
    end_of: HashMap<u32, u32>,
    /// Invoking `Do` to the Form `XObject` scope it drew.
    scope_of: HashMap<u32, u32>,
}

impl PageData {
    fn new(cap: CapturedPage) -> Self {
        let mut seq_glyphs = vec![Vec::new(); cap.marked.len()];
        for item in cap
            .items
            .iter()
            .filter(|i| matches!(i.kind, PaintKind::Glyph { .. }))
        {
            for &s in &item.mc_stack {
                if let Some(list) = seq_glyphs.get_mut(usize_from_u32(s)) {
                    list.push(item.ix);
                }
            }
        }
        let mut art_no = vec![None; cap.marked.len()];
        let mut artifacts = Vec::new();
        let mut by_mcid: BTreeMap<i32, Vec<u32>> = BTreeMap::new();
        let mut begin_of = HashMap::new();
        let mut end_of = HashMap::new();
        for seq in &cap.marked {
            if seq.artifact.is_some() {
                if let Some(slot) = art_no.get_mut(usize_from_u32(seq.ix)) {
                    *slot = u32::try_from(artifacts.len()).ok();
                }
                artifacts.push(seq.ix);
            }
            if let Some(mcid) = seq.mcid {
                by_mcid.entry(mcid).or_default().push(seq.ix);
            }
            begin_of.insert(seq.begin_op, seq.ix);
            if let Some(end) = seq.end_op {
                end_of.insert(end, seq.ix);
            }
        }
        let scope_of = cap.xobjects.iter().map(|x| (x.op, x.ix)).collect();
        let mut data = Self {
            op_mc: Vec::new(),
            cap,
            index: OnceLock::new(),
            seq_glyphs,
            art_no,
            artifacts,
            by_mcid,
            begin_of,
            end_of,
            scope_of,
        };
        data.op_mc = data.sweep_marked_content();
        data
    }

    /// Innermost enclosing sequence of every operator.
    fn sweep_marked_content(&self) -> Vec<Option<u32>> {
        let mut stack: Vec<u32> = Vec::new();
        let mut chain = ScopeChain::default();
        self.cap
            .ops
            .iter()
            .map(|op| {
                let chain = chain.of(&self.cap, op.xobject);
                let stale = stack.iter().position(|&s| {
                    self.seq(s)
                        .and_then(|q| self.op(q.begin_op))
                        .and_then(|o| o.xobject)
                        .is_some_and(|scope| !chain.contains(&scope))
                });
                if let Some(at) = stale {
                    stack.truncate(at);
                }
                if let Some(&s) = self.begin_of.get(&op.ix) {
                    let outer = stack.last().copied();
                    stack.push(s);
                    outer
                } else if let Some(&s) = self.end_of.get(&op.ix) {
                    match stack.iter().position(|&x| x == s) {
                        Some(at) => {
                            stack.truncate(at);
                            Some(s)
                        }
                        None => stack.last().copied(),
                    }
                } else {
                    stack.last().copied()
                }
            })
            .collect()
    }

    fn index(&self) -> &PageIndex {
        self.index.get_or_init(|| PageIndex::new(&self.cap))
    }

    fn op(&self, ix: u32) -> Option<&OpRecord> {
        self.cap.ops.get(usize_from_u32(ix)).filter(|o| o.ix == ix)
    }

    fn seq(&self, ix: u32) -> Option<&MarkedSeq> {
        self.cap.marked.get(usize_from_u32(ix))
    }

    fn item(&self, ix: u32) -> Option<&PaintItem> {
        self.cap.items.get(usize_from_u32(ix)).filter(|i| i.ix == ix)
    }

    /// The items an operator painted.
    fn op_items(&self, op: &OpRecord) -> &[PaintItem] {
        let Some(first) = op.first_item else {
            return &[];
        };
        let first = usize_from_u32(first);
        self.cap
            .items
            .get(first..first.saturating_add(usize_from_u32(op.item_count)))
            .unwrap_or_default()
    }

    /// The glyph items of an operator, in order.
    fn op_glyphs(&self, op: &OpRecord) -> impl Iterator<Item = &PaintItem> {
        self.op_items(op)
            .iter()
            .filter(|i| matches!(i.kind, PaintKind::Glyph { .. }))
    }

    fn glyph_item(&self, op: u32, glyph: u32) -> Option<&PaintItem> {
        self.op_glyphs(self.op(op)?)
            .find(|i| matches!(i.kind, PaintKind::Glyph { glyph_index, .. } if glyph_index == glyph))
    }

    /// `seq` and its enclosing sequences, innermost first.
    fn seq_chain(&self, start: Option<u32>) -> Vec<u32> {
        std::iter::successors(start, |&s| self.seq(s)?.parent)
            .take(self.cap.marked.len())
            .collect()
    }

    /// Sequences enclosing an operator, innermost first.
    fn enclosing(&self, op: &OpRecord) -> Vec<u32> {
        self.seq_chain(self.op_mc.get(usize_from_u32(op.ix)).copied().flatten())
    }

    /// The node a sequence is addressed by: its MCID, else its artifact number.
    fn seq_node(&self, page: PageIx, seq: &MarkedSeq) -> Option<NodeId> {
        if let Some(mcid) = seq.mcid {
            return Some(NodeId::MarkedContent { page, mcid });
        }
        let seq = self.art_no.get(usize_from_u32(seq.ix)).copied().flatten()?;
        Some(NodeId::Artifact { page, seq })
    }

    /// Nodes of the sequences in `chain` (innermost first), skipping unaddressable ones and
    /// repeats.
    fn chain_nodes(&self, page: PageIx, chain: &[u32]) -> Vec<NodeId> {
        let mut out = Vec::new();
        for node in chain
            .iter()
            .filter_map(|&s| self.seq(s))
            .filter_map(|s| self.seq_node(page, s))
        {
            if !out.contains(&node) {
                out.push(node);
            }
        }
        out
    }

    fn artifact_seq(&self, number: u32) -> Option<&MarkedSeq> {
        self.seq(*self.artifacts.get(usize_from_u32(number))?)
    }

    /// Sequences that carry `mcid` and pass `keep`.
    fn mc_seqs<'a>(
        &'a self,
        mcid: i32,
        keep: impl Fn(&MarkedSeq) -> bool + 'a,
    ) -> impl Iterator<Item = &'a MarkedSeq> {
        self.by_mcid
            .get(&mcid)
            .into_iter()
            .flatten()
            .filter_map(|&s| self.seq(s))
            .filter(move |s| keep(s))
    }

    fn seqs_bbox<'a>(seqs: impl Iterator<Item = &'a MarkedSeq>) -> Option<Rect> {
        union(seqs.filter_map(|s| s.bbox))
    }

    /// Paint-order indices of the glyph items inside a sequence.
    fn glyph_ids(&self, seq: &MarkedSeq) -> BTreeSet<u32> {
        self.seq_glyphs
            .get(usize_from_u32(seq.ix))
            .map(|v| v.iter().copied().collect())
            .unwrap_or_default()
    }

    /// Feeds the text of the glyphs painted inside `seqs`, in paint order, to `f` one glyph at a
    /// time until `f` returns `false`.
    fn seqs_text_each<'a>(&self, seqs: impl Iterator<Item = &'a MarkedSeq>, f: &mut dyn FnMut(&str) -> bool) {
        let mut lists = seqs.filter_map(|s| self.seq_glyphs.get(usize_from_u32(s.ix)));
        let (Some(first), second) = (lists.next(), lists.next()) else {
            return;
        };
        let mut feed = |items: &mut dyn Iterator<Item = u32>| {
            for text in items.filter_map(|i| self.item(i)).filter_map(glyph_unicode) {
                if !f(text) {
                    return;
                }
            }
        };
        match second {
            // One sequence: its glyph list is already in paint order and free of repeats.
            None => feed(&mut first.iter().copied()),
            // Several sequences can share glyphs (nesting); merge them into paint order.
            Some(second) => {
                let merged: BTreeSet<u32> = [first, second]
                    .into_iter()
                    .chain(lists)
                    .flat_map(|l| l.iter().copied())
                    .collect();
                feed(&mut merged.into_iter());
            }
        }
    }

    /// Text of the glyphs painted inside `seqs`, in paint order.
    fn seqs_text<'a>(&self, seqs: impl Iterator<Item = &'a MarkedSeq>) -> String {
        let mut text = String::new();
        self.seqs_text_each(seqs, &mut |t| {
            text.push_str(t);
            true
        });
        text
    }

    /// Bounding box of an operator: its painted items, else the sequence it opens or the form
    /// it draws.
    fn op_bbox(&self, op: &OpRecord) -> Option<Rect> {
        union(self.op_items(op).iter().map(|i| i.bbox)).or_else(|| {
            if let Some(&s) = self.begin_of.get(&op.ix) {
                return self.seq(s)?.bbox;
            }
            let scope = self
                .cap
                .xobjects
                .get(usize_from_u32(*self.scope_of.get(&op.ix)?))?;
            scope.page_bbox
        })
    }
}

/// Scope chains of the Form `XObject` an operator runs in, cached for consecutive operators.
#[derive(Default)]
struct ScopeChain {
    /// The scope `chain` was computed for; `None` before the first call.
    last: Option<ScopeKey>,
    chain: Vec<u32>,
}

/// A Form `XObject` scope index, or the page itself.
#[derive(Clone, Copy, PartialEq, Eq)]
enum ScopeKey {
    Page,
    Form(u32),
}

impl ScopeChain {
    /// `scope` and its ancestors.
    fn of(&mut self, cap: &CapturedPage, scope: Option<u32>) -> &[u32] {
        let key = scope.map_or(ScopeKey::Page, ScopeKey::Form);
        if self.last != Some(key) {
            self.chain = std::iter::successors(scope, |&s| cap.xobjects.get(usize_from_u32(s))?.parent)
                .take(cap.xobjects.len())
                .collect();
            self.last = Some(key);
        }
        &self.chain
    }
}

/// Unicode text of a glyph item.
fn glyph_unicode(item: &PaintItem) -> Option<&str> {
    match &item.kind {
        PaintKind::Glyph { unicode, .. } => unicode.as_deref(),
        _ => None,
    }
}

/// Smallest rectangle containing all of `rects`.
fn union(rects: impl Iterator<Item = Rect>) -> Option<Rect> {
    rects.reduce(|a, b| Rect {
        x0: a.x0.min(b.x0),
        y0: a.y0.min(b.y0),
        x1: a.x1.max(b.x1),
        y1: a.y1.max(b.y1),
    })
}

/// The innermost Form `XObject` a stream is, `None` for the page's own content.
fn stream_form(stream: &StreamRef) -> Option<ObjRef> {
    match stream {
        StreamRef::PageContent { .. } => None,
        StreamRef::FormXObject { obj, .. } => *obj,
    }
}

impl Document {
    /// Operators of one page, nested by marked content and Form `XObject`s.
    pub(crate) fn content_tree(&self, page: PageIx) -> Vec<TreeNode> {
        self.content
            .data(page)
            .map(|data| tree::content_tree(page, data))
            .unwrap_or_default()
    }

    /// Artifact sequences of all pages, grouped by page.
    pub(crate) fn artifacts_tree(&self) -> Vec<TreeNode> {
        (0..self.pages.len())
            .filter_map(|p| {
                let page = PageIx::try_from(p).ok()?;
                tree::artifact_group(page, self.content.data(page)?)
            })
            .collect()
    }

    /// Props for `Op`, `Glyph`, `MarkedContent` and `Artifact` nodes.
    pub(crate) fn content_props(&self, id: &NodeId) -> Option<Props> {
        match *id {
            NodeId::Op { page, ix } => self.op_props(page, ix),
            NodeId::Glyph { page, op, glyph } => self.glyph_props(page, op, glyph),
            NodeId::MarkedContent { page, mcid } => self.mc_props(page, mcid),
            NodeId::Artifact { page, seq } => self.artifact_props(page, seq),
            _ => None,
        }
    }

    /// Content nodes under `point`, most specific first: glyph or operator, then enclosing
    /// marked-content sequences / artifacts from innermost outwards. Filtered by `layer`. Each
    /// glyph is followed by its operator.
    pub(crate) fn content_hits(&self, page: PageIx, point: Point, layer: LayerMode) -> Vec<NodeId> {
        let Some(data) = self.content.data(page) else {
            return Vec::new();
        };
        let (content, tags, artifacts) = match layer {
            LayerMode::All => (true, true, true),
            LayerMode::Content => (true, false, false),
            LayerMode::Tags => (false, true, false),
            LayerMode::Artifacts => (false, false, true),
            LayerMode::Annotations | LayerMode::Boxes => return Vec::new(),
        };
        let items: Vec<&PaintItem> = data
            .index()
            .hit(point)
            .into_iter()
            .filter_map(|i| data.item(i))
            .collect();
        let mut hits: Vec<NodeId> = Vec::new();
        let mut add = |node: NodeId| {
            if !hits.contains(&node) {
                hits.push(node);
            }
        };
        if content {
            for item in &items {
                // A glyph is followed by the operator that painted it.
                if let PaintKind::Glyph { glyph_index, .. } = item.kind {
                    add(NodeId::Glyph {
                        page,
                        op: item.op,
                        glyph: glyph_index,
                    });
                }
                add(NodeId::Op { page, ix: item.op });
            }
        }
        if let Some(first) = items.first() {
            for node in first
                .mc_stack
                .iter()
                .rev()
                .filter_map(|&s| data.seq(s))
                .filter_map(|s| data.seq_node(page, s))
            {
                let wanted = match node {
                    NodeId::MarkedContent { .. } => tags,
                    NodeId::Artifact { .. } => artifacts,
                    _ => false,
                };
                if wanted {
                    add(node);
                }
            }
        }
        hits
    }

    /// Page and page-space bounding box of a content node.
    pub(crate) fn content_bbox(&self, id: &NodeId) -> Option<(PageIx, Rect)> {
        match *id {
            NodeId::Op { page, ix } => {
                let data = self.content.data(page)?;
                Some((page, data.op_bbox(data.op(ix)?)?))
            }
            NodeId::Glyph { page, op, glyph } => {
                let data = self.content.data(page)?;
                Some((page, data.glyph_item(op, glyph)?.bbox))
            }
            NodeId::MarkedContent { page, mcid } => Some((page, self.mc_bbox(page, mcid)?)),
            NodeId::Artifact { page, seq } => {
                let seq = self.content.data(page)?.artifact_seq(seq)?;
                let bbox = seq.bbox.or_else(|| seq.artifact.as_ref()?.bbox)?;
                Some((page, bbox))
            }
            _ => None,
        }
    }

    /// Content ancestors of `id` between its page and itself (exclusive), outermost first:
    /// enclosing marked-content sequences, then for a glyph its operator.
    pub(crate) fn content_ancestors(&self, id: &NodeId) -> Vec<NodeId> {
        let ancestors = |page: PageIx, start: Option<u32>| -> Vec<NodeId> {
            let Some(data) = self.content.data(page) else {
                return Vec::new();
            };
            let mut nodes = data.chain_nodes(page, &data.seq_chain(start));
            nodes.reverse();
            nodes
        };
        match *id {
            NodeId::Op { page, ix } => {
                let start = self
                    .content
                    .data(page)
                    .and_then(|d| d.op_mc.get(usize_from_u32(ix)).copied().flatten());
                ancestors(page, start)
            }
            NodeId::Glyph { page, op, .. } => {
                let start = self
                    .content
                    .data(page)
                    .and_then(|d| d.op_mc.get(usize_from_u32(op)).copied().flatten());
                let mut nodes = ancestors(page, start);
                nodes.push(NodeId::Op { page, ix: op });
                nodes
            }
            NodeId::MarkedContent { page, mcid } => {
                let start = self
                    .content
                    .data(page)
                    .and_then(|d| d.mc_seqs(mcid, |_| true).next()?.parent);
                ancestors(page, start)
            }
            NodeId::Artifact { page, seq } => {
                let start = self.content.data(page).and_then(|d| d.artifact_seq(seq)?.parent);
                ancestors(page, start)
            }
            _ => Vec::new(),
        }
    }

    /// MCIDs of the marked content on `page`, sorted and unique.
    pub(crate) fn mcids_on_page(&self, page: PageIx) -> Vec<i32> {
        self.content
            .data(page)
            .map(|d| d.by_mcid.keys().copied().collect())
            .unwrap_or_default()
    }

    /// Union of the painted boxes of every sequence with `mcid` on `page`.
    pub(crate) fn mc_bbox(&self, page: PageIx, mcid: i32) -> Option<Rect> {
        let data = self.content.data(page)?;
        PageData::seqs_bbox(data.mc_seqs(mcid, |_| true))
    }

    /// Like [`Document::mc_bbox`], restricted to sequences written in `stream` (`None` = the
    /// page's own content, `Some(obj)` = that Form `XObject`).
    pub(crate) fn mc_bbox_in(&self, page: PageIx, mcid: i32, stream: Option<ObjRef>) -> Option<Rect> {
        let data = self.content.data(page)?;
        PageData::seqs_bbox(data.mc_seqs(mcid, |s| stream_form(&s.stream) == stream))
    }

    /// Execution index of the first operator of the sequences with `mcid` on `page` written in
    /// `stream`: where that content starts to paint in the page's paint order.
    pub(crate) fn mc_first_op(&self, page: PageIx, mcid: i32, stream: Option<ObjRef>) -> Option<u32> {
        let data = self.content.data(page)?;
        data.mc_seqs(mcid, |s| stream_form(&s.stream) == stream)
            .map(|s| s.begin_op)
            .min()
    }

    /// The whole text painted by the sequences with `mcid` on `page` written in `stream`, in paint
    /// order. Production code reads previews through [`Document::mc_text_each`]; this is the
    /// untruncated view the content tests assert on.
    #[cfg(test)]
    pub(crate) fn mc_text_in(&self, page: PageIx, mcid: i32, stream: Option<ObjRef>) -> String {
        let mut text = String::new();
        self.mc_text_each(page, mcid, stream, &mut |t| {
            text.push_str(t);
            true
        });
        text
    }

    /// Hands the text painted by the sequences with `mcid` on `page` written in `stream` (`None` =
    /// the page's own content, `Some(obj)` = that Form `XObject`) to `f` glyph by glyph, in paint
    /// order, and stops as soon as `f` returns `false`, so a caller that needs only a preview does
    /// not read a whole page.
    pub(crate) fn mc_text_each(
        &self,
        page: PageIx,
        mcid: i32,
        stream: Option<ObjRef>,
        f: &mut dyn FnMut(&str) -> bool,
    ) {
        if let Some(d) = self.content.data(page) {
            d.seqs_text_each(d.mc_seqs(mcid, |s| stream_form(&s.stream) == stream), f);
        }
    }
}

#[cfg(test)]
mod tests;
