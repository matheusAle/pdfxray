//! A comparison: the open document (A) against a second one (B).
//!
//! [`AppState::diff`] holds the session while there is one; "diff mode" is `diff.is_some()`. The
//! open document stays [`AppState::document`], the A side, and every panel that is not about the
//! comparison keeps reading it. B is a second [`LoadedDocument`] here, beside the pairing of the
//! two, which [`pdfxray_core::diff::pair`] computes on a thread of its own: the app computes
//! nothing about either file, it shows the pairing.
//!
//! The pairing is invalidated whenever either side is read again (a drop, ⌘R) and recomputed in
//! the background; until it is back the canvas says so and the Changes list waits.
//!
//! Only what is about a comparison knows about sides: the selection ([`SelectionState::side`]),
//! the drawer's hit stack ([`HitStack::side`]) and the breadcrumb ([`AppState::path_on`]). A
//! [`NodeId`] has no side: nothing but `page:` and `box:` ids mean the same thing in two
//! versions of a file, so a pair always names both.
//!
//! [`SelectionState::side`]: super::SelectionState::side
//! [`HitStack::side`]: super::HitStack::side

use std::collections::HashMap;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{Receiver, TryRecvError, channel};
use std::time::{Duration, Instant};

use pdfxray_core::diff::{Method, Pair, Pairing, Rung, Side, Status};
use pdfxray_core::{Badge, Document, NodeId, PageIx, Severity, TagBox, TreeNode};
use pdfxray_render::annotate::{ChangeShow, status_rgba};
use pdfxray_render::composite::{Fit, Mode};

use super::{AppState, LoadedDocument};
use crate::components::{Annotation, Tone};
use crate::diff_frames::DiffFrames;
use crate::tree_view::{Decor, TreeModel};

/// Pages per side above which a comparison warns that it will be slow and large. It never
/// refuses: a debugger shows what it is asked to (plan decision 23).
pub const PAGE_BUDGET: usize = 300;

/// How often the UI looks at the pairing thread while it runs.
const POLL: Duration = Duration::from_millis(30);

/// The badge code of a pair that was matched on weak evidence (see [`low_confidence`]).
pub const LOW_CONFIDENCE: &str = "pair-low-confidence";

/// Where the pairing is.
enum PairingState {
    /// The thread is pairing; the receiver gets its answer.
    Running {
        started: Instant,
        done: Receiver<Result<Pairing, String>>,
    },
    /// The pairing of the two sides as they are now.
    Ready(Arc<Pairing>),
    /// The thread failed; the reason.
    Failed(String),
}

/// What a row of the Changes list stands for. Rows are `group:N` nodes, `N` an index into
/// [`DiffSession::rows`]: a row is not a node of either document.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeRow {
    /// A page pair, by its index in [`Pairing::pages`].
    Page(usize),
    /// A pair, by its index in [`Pairing::pairs`].
    Pair(usize),
    /// What changed outside the structure tree on a page pair (artifacts, untagged content),
    /// collapsed to one row.
    Residue(usize),
    /// The elements that are on no page.
    Unplaced,
}

/// A comparison of the open document (A) with `b`.
pub struct DiffSession {
    /// The B side.
    pub b: LoadedDocument,
    pairing: PairingState,
    /// How the two pages are shown; enters side by side.
    pub mode: Mode,
    /// What to do when the pages differ in size or `/Rotate`: refuse until the user opts into
    /// scaling B to A.
    pub fit: Fit,
    /// Overlaid, the side whose reading order is drawn and whose content a click picks.
    pub active: Side,
    /// The Changes list shows unchanged pairs too.
    pub show_unchanged: bool,
    /// The page pair on the canvas: an index into [`Pairing::pages`].
    pub page: usize,
    /// The selected pair, an index into [`Pairing::pairs`].
    selected: Option<usize>,
    /// The selection [`DiffSession::selected`] was read from.
    synced: Option<(Side, NodeId)>,
    /// What the canvas outlines: changed elements, and with ⇧T the unchanged ones too; `L`
    /// names them, `N`… adds the containers. Its own, not the View menu's: entering a
    /// comparison does not change how A alone is shown.
    pub show: ChangeShow,
    /// The canvas numbers the reading order (`R`): both sides side by side, the active side
    /// overlaid.
    pub order: bool,
    rows: Vec<ChangeRow>,
    tree: Option<TreeModel>,
    /// The pair of each element of either side, built on first use.
    index: Option<HashMap<(Side, NodeId), usize>>,
    /// B's tag boxes per page, read once.
    b_boxes: HashMap<PageIx, Arc<[TagBox]>>,
    /// The composite pictures of the canvas.
    pub(crate) frames: DiffFrames,
}

impl std::fmt::Debug for DiffSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DiffSession")
            .field("b", &self.b.path)
            .field("mode", &self.mode)
            .field("page", &self.page)
            .field("selected", &self.selected)
            .finish_non_exhaustive()
    }
}

impl DiffSession {
    /// A session comparing `a` with `b`, pairing in the background.
    fn new(a: &Arc<Document>, b: LoadedDocument) -> Self {
        let pairing = start_pairing(a, &b.doc);
        Self {
            b,
            pairing,
            mode: Mode::SideBySide,
            fit: Fit::Refuse,
            active: Side::B,
            show_unchanged: false,
            page: 0,
            selected: None,
            synced: None,
            show: ChangeShow {
                unchanged: false,
                labels: true,
                nesting: false,
            },
            order: false,
            rows: Vec::new(),
            tree: None,
            index: None,
            b_boxes: HashMap::new(),
            frames: DiffFrames::default(),
        }
    }

    /// Pairs the two sides again: one of them was read again.
    fn restart(&mut self, a: &Arc<Document>) {
        self.pairing = start_pairing(a, &self.b.doc);
        self.selected = None;
        self.synced = None;
        self.rows.clear();
        self.tree = None;
        self.index = None;
        self.b_boxes.clear();
        self.frames.clear();
    }

    /// Number of composite pictures asked of the drawing thread so far.
    pub fn pictures_drawn(&self) -> usize {
        self.frames.renders()
    }

    /// True while the drawing thread has not answered the latest picture.
    pub fn is_drawing(&self) -> bool {
        self.frames.is_pending()
    }

    /// Digest ([`crate::pixel_digest`]) of the composite last shown, with the page pair and mode
    /// it shows.
    pub fn shown_picture(
        &self,
    ) -> Option<(
        Option<PageIx>,
        Option<PageIx>,
        pdfxray_render::composite::Mode,
        u64,
    )> {
        self.frames
            .shown()
            .map(|(key, digest)| (key.a, key.b, key.mode, digest))
    }

    /// The tag boxes of page `page` of B.
    pub(crate) fn b_boxes(&mut self, page: PageIx) -> Arc<[TagBox]> {
        let doc = &self.b.doc;
        Arc::clone(
            self.b_boxes
                .entry(page)
                .or_insert_with(|| doc.tag_boxes(page).into()),
        )
    }

    /// The index in [`Pairing::pairs`] of the pair holding element `id` of `side`.
    pub fn pair_of(&mut self, side: Side, id: &NodeId) -> Option<usize> {
        let pairing = Arc::clone(self.pairing()?);
        let index = self.index.get_or_insert_with(|| {
            let mut index = HashMap::new();
            for (at, pair) in pairing.pairs.iter().enumerate() {
                if let Some(a) = &pair.a {
                    index.insert((Side::A, a.clone()), at);
                }
                if let Some(b) = &pair.b {
                    index.insert((Side::B, b.clone()), at);
                }
            }
            index
        });
        index.get(&(side, id.clone())).copied()
    }

    /// The pairing, once it is done.
    pub fn pairing(&self) -> Option<&Arc<Pairing>> {
        match &self.pairing {
            PairingState::Ready(pairing) => Some(pairing),
            PairingState::Running { .. } | PairingState::Failed(_) => None,
        }
    }

    /// True while the pairing thread runs.
    pub fn is_pairing(&self) -> bool {
        matches!(self.pairing, PairingState::Running { .. })
    }

    /// How long the pairing has been running, while it runs.
    pub fn pairing_for(&self) -> Option<Duration> {
        match &self.pairing {
            PairingState::Running { started, .. } => Some(started.elapsed()),
            PairingState::Ready(_) | PairingState::Failed(_) => None,
        }
    }

    /// Why the pairing failed, if it did.
    pub fn pairing_error(&self) -> Option<&str> {
        match &self.pairing {
            PairingState::Failed(why) => Some(why),
            PairingState::Running { .. } | PairingState::Ready(_) => None,
        }
    }

    /// The selected pair.
    pub fn selected(&self) -> Option<&Pair> {
        self.pairing()?.pairs.get(self.selected?)
    }

    /// Index of the selected pair in [`Pairing::pairs`].
    pub fn selected_index(&self) -> Option<usize> {
        self.selected
    }

    /// The pages of A and B on the canvas: either may be missing (a page only one side has).
    pub fn pages(&self) -> Option<(Option<PageIx>, Option<PageIx>)> {
        let m = self.pairing()?.pages.get(self.page)?;
        Some((m.a, m.b))
    }

    /// What the Changes row `id` stands for.
    pub fn row(&self, id: &NodeId) -> Option<ChangeRow> {
        match id {
            NodeId::Group(n) => self.rows.get(usize::try_from(*n).ok()?).copied(),
            _ => None,
        }
    }

    /// The row id of `target`, when the list has one.
    pub fn row_id(&self, target: ChangeRow) -> Option<NodeId> {
        let at = self.rows.iter().position(|r| *r == target)?;
        Some(NodeId::Group(u32::try_from(at).ok()?))
    }

    /// The Changes list, built from the pairing on first use and again when its filter changes.
    pub fn tree(&mut self) -> Option<&mut TreeModel> {
        let pairing = Arc::clone(self.pairing()?);
        if self.tree.is_none() {
            let (rows, roots) = change_tree(&pairing, self.show_unchanged);
            self.rows = rows;
            let rows = self.rows.clone();
            self.tree = Some(TreeModel::with_decor(roots, |node| decor(&pairing, &rows, node)));
        }
        self.tree.as_mut()
    }

    /// Shows or hides the unchanged pairs in the Changes list.
    pub fn set_show_unchanged(&mut self, show: bool) {
        if self.show_unchanged != show {
            self.show_unchanged = show;
            self.tree = None;
        }
    }

    /// The page pair holding `pair`: A's page when it has one, else B's.
    fn page_of(pairing: &Pairing, pair: &Pair) -> Option<usize> {
        pairing
            .pages
            .iter()
            .position(|m| pair.page_a.is_some() && m.a == pair.page_a)
            .or_else(|| {
                pairing
                    .pages
                    .iter()
                    .position(|m| pair.page_b.is_some() && m.b == pair.page_b)
            })
    }

    /// Selects pair `index` and brings its page pair onto the canvas.
    fn select_pair(&mut self, index: usize) {
        let Some(pairing) = self.pairing().cloned() else {
            return;
        };
        let Some(pair) = pairing.pairs.get(index) else {
            return;
        };
        self.selected = Some(index);
        if let Some(page) = Self::page_of(&pairing, pair) {
            self.page = page;
        }
    }

    /// Reads the selected pair off the selection `(side, id)`, when it changed: the canvas
    /// follows the selection wherever it was made (the Changes list, a tree of A, the canvas).
    fn sync(&mut self, side: Side, id: Option<&NodeId>) {
        // Read once the pairing is there: a selection made while it runs is read then.
        if self.pairing().is_none() {
            return;
        }
        let now = id.map(|id| (side, id.clone()));
        if now == self.synced {
            return;
        }
        self.synced.clone_from(&now);
        let Some((side, id)) = now else {
            return;
        };
        if let Some(target) = self.row(&id) {
            match target {
                ChangeRow::Pair(index) => self.select_pair(index),
                ChangeRow::Page(page) | ChangeRow::Residue(page) => {
                    self.selected = None;
                    self.page = page;
                }
                ChangeRow::Unplaced => self.selected = None,
            }
            return;
        }
        match self.pair_of(side, &id) {
            Some(index) => self.select_pair(index),
            None => self.selected = None,
        }
    }
}

/// Starts pairing `a` with `b` on a thread of its own.
fn start_pairing(a: &Arc<Document>, b: &Arc<Document>) -> PairingState {
    let (a, b) = (Arc::clone(a), Arc::clone(b));
    let (sender, done) = channel();
    let work = move || {
        let result = catch_unwind(AssertUnwindSafe(|| pdfxray_core::diff::pair(&a, &b)))
            .map_err(|_| "the pairing crashed".to_owned());
        sender.send(result).ok();
    };
    match std::thread::Builder::new()
        .name("pdfxray-pair".to_owned())
        .spawn(work)
    {
        Ok(_) => PairingState::Running {
            started: Instant::now(),
            done,
        },
        Err(error) => PairingState::Failed(format!("cannot start the pairing thread: {error}")),
    }
}

/// The page pair of the first change: a pair that is not the same, or a page whose painted
/// content outside the tags differs, whichever comes first in A's page order.
fn first_change(pairing: &Pairing) -> Option<usize> {
    let pair = pairing
        .pairs
        .iter()
        .filter(|p| p.status != Status::Same)
        .find_map(|p| DiffSession::page_of(pairing, p));
    let residue = pairing
        .pages
        .iter()
        .position(|m| m.residue.only_a + m.residue.only_b > 0 || m.a.is_none() || m.b.is_none());
    match (pair, residue) {
        (Some(p), Some(r)) => Some(p.min(r)),
        (p, r) => p.or(r),
    }
}

/// True when `pair` rests on weak evidence: matched by the bounded alignment of an anchorless
/// gap, on geometry alone, or on a degraded page. It must not look like a certain pair.
pub fn low_confidence(pair: &Pair) -> bool {
    let c = &pair.confidence;
    c.method == Some(Method::Bounded) || c.rung == Some(Rung::Geometry) || c.degraded_a || c.degraded_b
}

/// `status` in words.
pub const fn status_word(status: Status) -> &'static str {
    pdfxray_render::annotate::status_name(status)
}

/// The colour of `status`, as the canvas draws it.
pub fn status_color(status: Status) -> egui::Color32 {
    let [r, g, b, a] = status_rgba(status);
    egui::Color32::from_rgba_unmultiplied(r, g, b, a)
}

/// `Page 3`, `Page 3 → 4`, `Page 5, only in B`: how a page pair is named.
pub fn page_pair_label(a: Option<PageIx>, b: Option<PageIx>) -> String {
    match (a, b) {
        (Some(a), Some(b)) if a == b => format!("Page {}", a + 1),
        (Some(a), Some(b)) => format!("Page {} → {}", a + 1, b + 1),
        (Some(a), None) => format!("Page {}, only in A", a + 1),
        (None, Some(b)) => format!("Page {}, only in B", b + 1),
        (None, None) => "No page".to_owned(),
    }
}

/// The rows of the Changes list and their trees: a root per page pair with a change (every one
/// while unchanged pairs show), its changed pairs, and one row for what changed outside the
/// structure tree; the pairs on no page last.
fn change_tree(pairing: &Pairing, show_unchanged: bool) -> (Vec<ChangeRow>, Vec<TreeNode>) {
    let mut rows = Vec::new();
    let id = |rows: &mut Vec<ChangeRow>, row| {
        rows.push(row);
        NodeId::Group(u32::try_from(rows.len() - 1).unwrap_or(u32::MAX))
    };
    let shown = |p: &Pair| show_unchanged || p.status != Status::Same;
    let mut per_page: Vec<Vec<usize>> = vec![Vec::new(); pairing.pages.len()];
    let mut unplaced = Vec::new();
    for (index, pair) in pairing.pairs.iter().enumerate().filter(|(_, p)| shown(p)) {
        match DiffSession::page_of(pairing, pair) {
            Some(page) => per_page[page].push(index),
            None => unplaced.push(index),
        }
    }
    let mut roots = Vec::new();
    for (page, m) in pairing.pages.iter().enumerate() {
        let residue = m.residue.only_a + m.residue.only_b > 0;
        let one_sided = m.a.is_none() || m.b.is_none();
        if per_page[page].is_empty() && !residue && !one_sided && !show_unchanged {
            continue;
        }
        let mut node = TreeNode::leaf(id(&mut rows, ChangeRow::Page(page)), page_pair_label(m.a, m.b));
        if residue {
            let residue_id = id(&mut rows, ChangeRow::Residue(page));
            node.children.push(TreeNode::leaf(residue_id, "Artifacts"));
        }
        for &index in &per_page[page] {
            node.children.push(pair_node(
                id(&mut rows, ChangeRow::Pair(index)),
                &pairing.pairs[index],
            ));
        }
        roots.push(node);
    }
    if !unplaced.is_empty() {
        let mut node = TreeNode::leaf(id(&mut rows, ChangeRow::Unplaced), "Not on a page");
        for index in unplaced {
            node.children.push(pair_node(
                id(&mut rows, ChangeRow::Pair(index)),
                &pairing.pairs[index],
            ));
        }
        roots.push(node);
    }
    (rows, roots)
}

fn pair_node(id: NodeId, pair: &Pair) -> TreeNode {
    let mut node = TreeNode::leaf(id, pair.role.clone());
    if !pair.text.is_empty() {
        node = node.with_detail(pair.text.clone());
    }
    if low_confidence(pair) {
        node.badges.push(Badge {
            code: LOW_CONFIDENCE.to_owned(),
            severity: Severity::Warning,
        });
    }
    node
}

/// How a row of the Changes list is drawn: a pair's dot and tag in its status colour, a page's
/// count of changes, the residue's counts.
fn decor(pairing: &Pairing, rows: &[ChangeRow], node: &TreeNode) -> Decor {
    let row = match &node.id {
        NodeId::Group(n) => usize::try_from(*n).ok().and_then(|n| rows.get(n)),
        _ => None,
    };
    match row {
        Some(ChangeRow::Pair(index)) => {
            let pair = &pairing.pairs[*index];
            let color = status_color(pair.status);
            Decor {
                dot: Some(color),
                right: vec![Annotation::Toned(
                    status_word(pair.status).to_owned(),
                    Tone::Kind(color),
                )],
                hover_text: Some(pair_summary(pair)),
                search: status_word(pair.status).to_owned(),
                ..Decor::default()
            }
        }
        Some(ChangeRow::Page(page)) => {
            let changes = pairing
                .pairs
                .iter()
                .filter(|p| p.status != Status::Same && DiffSession::page_of(pairing, p) == Some(*page))
                .count();
            Decor {
                detail: Some(match changes {
                    0 => "no changes".to_owned(),
                    1 => "1 change".to_owned(),
                    n => format!("{n} changes"),
                }),
                ..Decor::default()
            }
        }
        Some(ChangeRow::Residue(page)) => {
            let r = pairing.pages[*page].residue;
            Decor {
                detail: Some(format!("{} only in A · {} only in B", r.only_a, r.only_b)),
                dot: Some(status_color(Status::Modified)),
                right: vec![Annotation::Toned(
                    "changed".to_owned(),
                    Tone::Kind(status_color(Status::Modified)),
                )],
                hover_text: Some(
                    "Painted content outside the structure tree differs: artifacts or untagged \
                     content. Not paired element by element yet."
                        .to_owned(),
                ),
                ..Decor::default()
            }
        }
        Some(ChangeRow::Unplaced) | None => Decor::default(),
    }
}

/// One line on what changed in `pair`: its status and the fields that changed.
pub fn pair_summary(pair: &Pair) -> String {
    let fields: Vec<String> = pair
        .changed
        .iter()
        .map(|c| format!("{:?}", c.field).to_lowercase())
        .collect();
    let mut text = format!("{} {}", status_word(pair.status), pair.role);
    if !fields.is_empty() {
        text.push_str(": ");
        text.push_str(&fields.join(", "));
    }
    if pair.reordered {
        text.push_str(" · reordered");
    }
    if low_confidence(pair) {
        text.push_str(" · low confidence");
    }
    text
}

/// Where a file to compare is going.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Target {
    /// It becomes the open document; a comparison ends.
    #[default]
    Document,
    /// It becomes (or replaces) B.
    CompareB,
    /// It replaces A and the comparison goes on.
    ReplaceA,
}

impl AppState {
    /// True while a comparison is open.
    pub fn in_diff(&self) -> bool {
        self.diff.is_some()
    }

    /// The document of `side`: A is the open document, B the comparison's.
    pub fn doc_of(&self, side: Side) -> Option<&Arc<Document>> {
        match side {
            Side::A => self.document.as_ref().map(|l| &l.doc),
            Side::B => self.diff.as_ref().map(|d| &d.b.doc),
        }
    }

    /// Compares the open document with the file at `path` (Compare With…, ⌥-drop, the
    /// `--compare` argument). Opens it in the background; while A is still opening it waits for
    /// A. Without a document there is nothing to compare with.
    pub fn compare_with(&mut self, path: &Path) {
        if self.document.is_none() {
            if self.opening.job().is_some() {
                self.pending_compare = Some(path.to_path_buf());
            } else {
                "Open a document first: Compare With… compares it with another file"
                    .clone_into(&mut self.status);
            }
            return;
        }
        self.start_open_into(path, Target::CompareB);
    }

    /// Asks for a file to compare the open document with (File › Compare With…).
    pub fn request_compare(&mut self) {
        if let Some(path) = (self.picker)() {
            self.compare_with(&path);
        }
    }

    /// Reads the side being iterated again from disk (⌘R): B in a comparison, which is paired
    /// again; the open document otherwise. Nothing watches files: this is the way to see a
    /// change made elsewhere.
    pub fn reload(&mut self) {
        if let Some(diff) = &self.diff {
            let path = diff.b.path.clone();
            self.start_open_into(&path, Target::CompareB);
        } else if let Some(loaded) = &self.document {
            let path = loaded.path.clone();
            self.open_path(&path);
        }
    }

    /// Ends the comparison: A stays open.
    pub fn end_diff(&mut self) {
        self.diff = None;
        self.drawer.hit_stack = None;
        if self.nav.rail == super::RailItem::Changes {
            self.set_rail(super::RailItem::Tags);
        }
        if self.selection.side() == Side::B {
            self.selection = super::SelectionState::default();
            self.select(NodeId::Document);
        }
    }

    /// Takes in B, read from `path`: a new comparison, or B replaced in the one open (its
    /// settings stay).
    pub(crate) fn install_b(&mut self, doc: Document, path: &Path, opened_in: Duration) {
        let Some(a) = self.document.as_ref().map(|l| Arc::clone(&l.doc)) else {
            return;
        };
        let loaded = LoadedDocument::new(doc, path, opened_in);
        let pages = loaded.info.page_count.max(a.page_count());
        if let Some(diff) = &mut self.diff {
            diff.b = loaded;
            diff.restart(&a);
        } else {
            self.diff = Some(DiffSession::new(&a, loaded));
            self.set_rail(super::RailItem::Changes);
        }
        self.drawer.hit_stack = None;
        if self.selection.side() == Side::B {
            self.selection = super::SelectionState::default();
        }
        self.status = if pages > PAGE_BUDGET {
            format!(
                "Comparing {pages}-page documents: above {PAGE_BUDGET} pages a comparison takes a \
                 lot of memory and time"
            )
        } else {
            super::READY.to_owned()
        };
    }

    /// A was read again while a comparison is open: pair it with B again.
    pub(crate) fn restart_diff(&mut self) {
        if let (Some(diff), Some(a)) = (&mut self.diff, &self.document) {
            diff.restart(&a.doc);
        }
    }

    /// Takes in the pairing when its thread is done; keeps the selected pair in line with the
    /// selection. Called once per frame.
    pub(crate) fn poll_diff(&mut self, ctx: &egui::Context) {
        let side = self.selection.side();
        let primary = self.selection.primary().cloned();
        let Some(diff) = &mut self.diff else {
            return;
        };
        if let PairingState::Running { done, .. } = &diff.pairing {
            match done.try_recv() {
                Ok(Ok(pairing)) => {
                    // The canvas opens on the first page pair with a change.
                    if let Some(page) = first_change(&pairing) {
                        diff.page = page;
                    }
                    diff.pairing = PairingState::Ready(Arc::new(pairing));
                }
                Ok(Err(why)) => diff.pairing = PairingState::Failed(why),
                Err(TryRecvError::Empty) => ctx.request_repaint_after(POLL),
                Err(TryRecvError::Disconnected) => {
                    diff.pairing = PairingState::Failed("the pairing thread stopped unexpectedly".to_owned());
                }
            }
        }
        diff.sync(side, primary.as_ref());
    }

    /// Selects `id` of `side` alone: a click on that side of the canvas, or its hit stack.
    pub fn select_in(&mut self, side: Side, id: NodeId) {
        if side == Side::A {
            self.select_on_canvas(id);
        } else {
            // B's nodes are not A's objects: they stay out of A's object history.
            self.selection.set_one_on(Side::B, id);
        }
    }

    /// Selects the row `target` of the Changes list.
    pub fn select_change(&mut self, target: ChangeRow) {
        if let Some(id) = self.diff.as_ref().and_then(|d| d.row_id(target)) {
            self.selection.set_one_on(Side::A, id);
        }
    }

    /// The path of the B or A node `id`, root first, for the breadcrumb.
    pub fn path_on(&self, side: Side, id: &NodeId) -> Vec<NodeId> {
        match (side, self.doc_of(side)) {
            (Side::A, _) => self.path_to(id),
            (Side::B, Some(doc)) => doc.breadcrumb(id),
            (Side::B, None) => vec![id.clone()],
        }
    }

    /// The file to compare with once A has opened (the `--compare` argument).
    pub(crate) fn take_pending_compare(&mut self) -> Option<PathBuf> {
        self.pending_compare.take()
    }
}

#[cfg(test)]
mod tests {
    use pdfxray_core::diff::{Confidence, PageMatch, PairKind, Residue, SideInfo, Sides};
    use pretty_assertions::assert_eq;

    use super::*;

    fn pair(status: Status, page: Option<PageIx>, method: Option<Method>) -> Pair {
        Pair {
            kind: PairKind::Element,
            status,
            a: page.map(|_| NodeId::Document),
            b: None,
            role: "P".to_owned(),
            text: "hello".to_owned(),
            page_a: page,
            page_b: page,
            bbox_a: None,
            bbox_b: None,
            delta: None,
            reordered: false,
            changed: Vec::new(),
            rows: Vec::new(),
            confidence: Confidence {
                rung: None,
                method,
                unicode_a: None,
                unicode_b: None,
                degraded_a: false,
                degraded_b: false,
            },
        }
    }

    fn pairing(pairs: Vec<Pair>, pages: Vec<PageMatch>) -> Pairing {
        let side = || SideInfo {
            name: String::new(),
            pages: 2,
            elements: 0,
            structure_only: false,
            degraded_pages: Vec::new(),
        };
        Pairing {
            pairs,
            pages,
            offset: None,
            tolerance: 1.0,
            sides: Sides { a: side(), b: side() },
            notes: Vec::new(),
            diagnostics: pdfxray_core::diff::DiagnosticsDiff::default(),
        }
    }

    fn page(a: Option<PageIx>, b: Option<PageIx>, residue: Residue) -> PageMatch {
        PageMatch {
            a,
            b,
            pairs: 1,
            residue,
        }
    }

    #[test]
    fn the_changes_list_hides_same_pairs_and_unchanged_pages_and_collapses_artifacts() {
        let changed_ink = Residue {
            a: 3,
            b: 4,
            only_a: 1,
            only_b: 2,
        };
        let p = pairing(
            vec![
                pair(Status::Same, Some(0), None),
                pair(Status::Same, Some(1), None),
                pair(Status::Modified, Some(1), Some(Method::Bounded)),
            ],
            vec![
                page(Some(0), Some(0), Residue::default()),
                page(Some(1), Some(1), changed_ink),
            ],
        );

        let (rows, roots) = change_tree(&p, false);

        let labels: Vec<&str> = roots.iter().map(|n| n.label.as_str()).collect();
        assert_eq!(labels, ["Page 2"]);
        assert_eq!(
            rows,
            [ChangeRow::Page(1), ChangeRow::Residue(1), ChangeRow::Pair(2)]
        );
        let children: Vec<&str> = roots[0].children.iter().map(|n| n.label.as_str()).collect();
        assert_eq!(children, ["Artifacts", "P"]);
        assert_eq!(roots[0].children[1].badges[0].code, LOW_CONFIDENCE);
        // smoke: showing unchanged pairs brings the first page and both same pairs back
        let (all_rows, all_roots) = change_tree(&p, true);
        assert_eq!(all_roots.len(), 2);
        assert_eq!(all_rows.len(), 6);
    }

    #[test]
    fn the_canvas_opens_on_the_first_page_pair_with_a_change() {
        let changed_ink = Residue {
            a: 1,
            b: 1,
            only_a: 1,
            only_b: 1,
        };
        let pages = || {
            vec![
                page(Some(0), Some(0), Residue::default()),
                page(Some(1), Some(1), Residue::default()),
                page(Some(2), Some(2), changed_ink),
            ]
        };
        let modified_on_2 = pairing(
            vec![
                pair(Status::Same, Some(0), None),
                pair(Status::Modified, Some(1), None),
            ],
            pages(),
        );
        let only_ink = pairing(vec![pair(Status::Same, Some(0), None)], pages());

        assert_eq!(first_change(&modified_on_2), Some(1));
        assert_eq!(first_change(&only_ink), Some(2));
        // smoke: nothing changed, nowhere to go
        let same = pairing(
            vec![pair(Status::Same, Some(0), None)],
            vec![page(Some(0), Some(0), Residue::default())],
        );
        assert_eq!(first_change(&same), None);
    }

    #[test]
    fn page_pairs_are_named_by_both_sides() {
        assert_eq!(page_pair_label(Some(2), Some(2)), "Page 3");
        assert_eq!(page_pair_label(Some(2), Some(3)), "Page 3 → 4");
        assert_eq!(page_pair_label(Some(4), None), "Page 5, only in A");
        // smoke: a page only B has
        assert_eq!(page_pair_label(None, Some(0)), "Page 1, only in B");
    }

    #[test]
    fn weak_evidence_reads_as_low_confidence() {
        assert!(low_confidence(&pair(
            Status::Same,
            Some(0),
            Some(Method::Bounded)
        )));
        let mut degraded = pair(Status::Modified, Some(0), Some(Method::Anchor));
        degraded.confidence.degraded_b = true;
        assert!(low_confidence(&degraded));
        // smoke: an anchored pair is not
        assert!(!low_confidence(&pair(
            Status::Modified,
            Some(0),
            Some(Method::Anchor)
        )));
    }
}
