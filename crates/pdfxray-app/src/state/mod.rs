//! All app state and the operations on it. Plain data, no widgets: tests build and inspect it
//! directly, and the panels only read it and call these methods.
//!
//! One sub-struct per area, so work on one panel does not touch another's state:
//!
//! * [`NavState`]: rail entry, navigator filter and view ([`nav`]).
//! * [`SelectionState`]: the selected node and the second one ([`selection`]).
//! * [`ObjectHistory`]: back and forward through objects ([`history`]).
//! * [`ObjectsState`]: the object view, the revision being looked at and the Objects lists
//!   ([`objects`]).
//! * [`DrawerState`]: drawer tab, height and the hit stack ([`drawer`]); the Speech tab's settings
//!   and what it reads are in [`speech`], the table behind the selection in [`tables`].
//! * [`CanvasState`]: page, zoom, layer mode and debug tools ([`canvas`]).

pub mod canvas;
pub mod derived;
pub mod drawer;
pub mod history;
pub mod nav;
pub mod objects;
pub mod path;
pub mod pointer;
pub mod selection;
pub mod speech;
pub mod tables;

use std::collections::HashMap;
use std::error::Error;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use egui::Context;
use pdfxray_core::{DocInfo, Document, EncryptionInfo, NodeId, TreeKind};
use pdfxray_render::ColorBy;

pub use canvas::CanvasState;
pub use derived::{PageStats, RoleMapEntry, TagIndex};
pub use drawer::{DrawerState, DrawerTab, HitEntry, HitStack};
pub use history::ObjectHistory;
pub use nav::{NavState, RailItem};
pub use objects::{
    DocumentRow, ObjectMode, ObjectRow, ObjectsState, PreviewScale, ResourceGroup, ResourceRow,
};
pub use selection::SelectionState;
pub use speech::{NoteLink, SpeechData, SpeechState};
pub use tables::{TableContext, TableData};

use self::derived::Derived;
use crate::instances::{Spawner, native_spawner};
use crate::modes::RenderRequest;
use crate::open::{FilePicker, Opener, native_picker};
use crate::page_cache::{Frame, PageCache};
use crate::previews::{PreviewFrame, Previews};
use crate::recents::{Clock, Recent, Recents, RecentsError, unix_now};
use crate::thumbnails::Thumbnails;
use crate::tree_view::TreeModel;
use crate::units::{Lengths, Unit};
use crate::validation::{ValidationState, Validator, VeraPdfValidator};

/// How often the UI looks for a finished bitmap while one is being rendered.
const RENDER_POLL: Duration = Duration::from_millis(50);

/// Initial status bar text.
pub const READY: &str = "Ready";

/// An opened document and where it came from.
#[derive(Debug)]
pub struct LoadedDocument {
    /// The document.
    pub doc: Document,
    /// File it was read from.
    pub path: PathBuf,
    /// How long opening took.
    pub opened_in: Duration,
    /// Summary facts, read once.
    pub info: DocInfo,
    /// Whether the file has a structure tree.
    pub tagged: bool,
    /// What `/Encrypt` says, for an encrypted file.
    pub encryption: Option<EncryptionInfo>,
}

/// A tree the navigator can show.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NavTree {
    /// A tree built by the core.
    Core(TreeKind),
    /// One row per revision of the file.
    Revisions,
}

/// All app state.
pub struct AppState {
    /// Status bar message.
    pub status: String,
    /// The open document, if any.
    pub document: Option<LoadedDocument>,
    /// Why the last attempt to open a file failed; cleared by the next success.
    pub open_error: Option<String>,
    /// The unit every length is shown in, switched in the status bar.
    pub unit: Unit,
    /// Rail entry, navigator filter and view.
    pub nav: NavState,
    /// The selected nodes.
    pub selection: SelectionState,
    /// Back and forward through objects.
    pub history: ObjectHistory,
    /// The Objects entry: object view mode, revision and the lists it shares.
    pub objects: ObjectsState,
    /// Drawer tab, height and hit stack.
    pub drawer: DrawerState,
    /// Page, zoom, layer mode and debug tools.
    pub canvas: CanvasState,
    /// Asked for a path when "Open…" is pressed.
    pub picker: FilePicker,
    /// Starts a new window (a new process) when one is needed; see [`crate::instances`].
    pub spawner: Spawner,
    /// The operating system's menu bar has New Window, Open…, Close Window and Quit with their
    /// shortcuts, so the window leaves ⌘N, ⌘O, ⌘W and ⌘Q to it (macOS). Off: the window handles
    /// them itself.
    pub native_menu: bool,
    /// The job that is opening a file, and how its card is shown.
    pub opening: Opener,
    /// The files opened lately, shown by the empty state.
    pub recents: Recents,
    /// Why the recent files could not be saved, if they could not.
    pub recents_error: Option<RecentsError>,
    /// Seconds since 1970, for the times of the recent files.
    pub clock: Clock,
    /// An encrypted file's explainer shows instead of the workspace until a rail entry is picked.
    pub explainer: bool,
    /// Runs veraPDF; replaced by a fake in tests.
    pub validator: Arc<dyn Validator>,
    /// State of the validation run.
    pub validation: ValidationState,
    trees: HashMap<NavTree, TreeModel>,
    pages: PageCache,
    thumbnails: Thumbnails,
    previews: Previews,
    derived: Derived,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            status: READY.to_owned(),
            document: None,
            open_error: None,
            unit: Unit::default(),
            nav: NavState::default(),
            selection: SelectionState::default(),
            history: ObjectHistory::default(),
            objects: ObjectsState::default(),
            drawer: DrawerState::default(),
            canvas: CanvasState::default(),
            picker: native_picker(),
            spawner: native_spawner(),
            native_menu: false,
            opening: Opener::default(),
            recents: Recents::default(),
            recents_error: None,
            clock: unix_now,
            explainer: false,
            validator: Arc::new(VeraPdfValidator::default()),
            validation: ValidationState::default(),
            trees: HashMap::new(),
            pages: PageCache::default(),
            thumbnails: Thumbnails::default(),
            previews: Previews::default(),
            derived: Derived::default(),
        }
    }
}

impl AppState {
    /// Makes `doc` the open document. `remember` adds it to the recent files.
    pub(crate) fn install(&mut self, doc: Document, path: &Path, opened_in: Duration, remember: bool) {
        // The title bar names the file; the status bar keeps its message slot for news.
        READY.clone_into(&mut self.status);
        self.open_error = None;
        let loaded = LoadedDocument {
            info: doc.info(),
            tagged: !doc.tree(TreeKind::Tags).is_empty(),
            encryption: doc.encryption(),
            doc,
            path: path.to_path_buf(),
            opened_in,
        };
        if remember {
            let recent = Recent {
                path: std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf()),
                name: loaded.info.name.clone(),
                pages: loaded.info.page_count,
                tagged: loaded.tagged,
                opened_at: (self.clock)(),
                missing: false,
            };
            self.recents_error = self.recents.record(recent).err();
        }
        self.explainer = loaded.encryption.is_some();
        self.document = Some(loaded);
        self.invalidate_trees();
        self.derived.clear();
        if let Some(loaded) = &self.document {
            self.derived.index(&loaded.doc);
        }
        self.pages.clear();
        self.thumbnails.clear();
        self.previews.clear();
        self.validation.abandon_run();
        self.drawer.hit_stack = None;
        self.drawer.sorted_by_severity = false;
        self.history.clear();
        self.objects = ObjectsState::default();
        self.canvas.reset_for_document();
        if self
            .document
            .as_ref()
            .is_some_and(|l| crate::states::broken::damage(&l.info).is_some())
        {
            // A damaged index is what the file is about: the drawer opens on its problems.
            self.drawer.tab = DrawerTab::Diagnostics;
            self.drawer.page_boxes_only = false;
        }
        self.reset_drawer(self.nav.rail);
        self.selection.secondary = None;
        self.selection.pinned = None;
        self.select(NodeId::Document);
    }

    /// Opens a file dropped onto the window here, replacing the document it shows: the drop says
    /// which window the file is meant for. Files asked for in other ways may open in a new window
    /// ([`AppState::open_files`]).
    pub fn accept_drop(&mut self, path: &Path) {
        self.open_path(path);
    }

    /// Number of pages of the open document, 0 without one.
    pub fn page_count(&self) -> usize {
        self.document.as_ref().map_or(0, |d| d.doc.page_count())
    }

    /// Picks a rail entry: the navigator, the centre and the drawer follow it (the table of the
    /// design's navigation spec).
    pub fn set_rail(&mut self, rail: RailItem) {
        let changed = self.nav.rail != rail;
        self.nav.set_rail(rail);
        self.explainer = false;
        if let Some(tab) = rail.drawer_default() {
            self.drawer.tab = tab;
        }
        // A new entry brings the drawer to the way that entry wants it; the same one leaves it.
        if changed {
            self.reset_drawer(rail);
        } else {
            self.drawer.reset(
                self.drawer.open,
                self.canvas.view.stepper,
                self.validation_finished(),
            );
        }
        // The Diagnostics tab narrows to page boxes on the Pages entry only.
        self.drawer.page_boxes_only = rail == RailItem::Pages;
        match rail {
            RailItem::Artifacts => {
                self.canvas.view.xray = true;
                self.canvas.view.color_by = ColorBy::Artifact;
            }
            // The structure view colours by how content is marked: tagged, artifact, untagged.
            RailItem::Tags => self.canvas.view.color_by = ColorBy::Artifact,
            RailItem::Pages => self.canvas.show_boxes = true,
            RailItem::Content | RailItem::Objects | RailItem::Revisions | RailItem::Validation => {}
        }
    }

    /// When the last validation run finished, if there is one.
    pub(crate) fn validation_finished(&self) -> Option<std::time::Instant> {
        self.validation.last.as_ref().map(|stamp| stamp.finished)
    }

    /// Puts the drawer the way `rail` wants it: open where the drawer is the main content, while
    /// the stepper is on and for a damaged file, else a strip.
    fn reset_drawer(&mut self, rail: RailItem) {
        let damaged = self
            .document
            .as_ref()
            .is_some_and(|loaded| crate::states::broken::damage(&loaded.info).is_some());
        let open = rail.drawer_opens() || self.canvas.view.stepper || damaged;
        self.drawer
            .reset(open, self.canvas.view.stepper, self.validation_finished());
    }

    /// How lengths on the current page are shown: the chosen unit and the page's `/UserUnit`.
    pub fn lengths(&self) -> Lengths {
        let user_unit = self
            .document
            .as_ref()
            .and_then(|loaded| loaded.doc.page(self.canvas.page))
            .map(|page| page.user_unit)
            .filter(|unit| *unit > 0.0)
            .unwrap_or(1.0);
        Lengths::new(self.unit, user_unit)
    }

    /// The tree the navigator lists for the active rail entry, or `None` when it lists no tree.
    pub fn nav_tree(&self) -> Option<NavTree> {
        match self.nav.rail {
            RailItem::Tags => Some(NavTree::Core(TreeKind::Tags)),
            RailItem::Content | RailItem::Artifacts => {
                Some(NavTree::Core(TreeKind::Content(self.canvas.page)))
            }
            RailItem::Revisions => Some(NavTree::Revisions),
            RailItem::Pages | RailItem::Objects | RailItem::Validation => None,
        }
    }

    /// The flattened tree, built from the document on first use.
    pub fn tree_model(&mut self, tree: NavTree) -> Option<&mut TreeModel> {
        let doc = &self.document.as_ref()?.doc;
        let derived = &mut self.derived;
        Some(self.trees.entry(tree).or_insert_with(|| match tree {
            NavTree::Core(TreeKind::Tags) => crate::navigator::tags::model(doc, derived),
            NavTree::Core(TreeKind::Content(page)) => crate::navigator::content::model(doc, derived, page),
            NavTree::Core(kind) => TreeModel::new(doc.tree(kind)),
            NavTree::Revisions => TreeModel::new(revision_nodes(doc)),
        }))
    }

    /// Drops cached trees, e.g. after the document changed underneath them.
    pub(crate) fn invalidate_trees(&mut self) {
        self.trees.clear();
    }

    /// Number of render requests sent to the render thread so far; one per distinct bitmap
    /// shown (page, scale and render modes).
    pub fn render_count(&self) -> usize {
        self.pages.renders()
    }

    /// True while the render thread is still working on the latest request.
    pub fn render_pending(&self) -> bool {
        self.pages.is_pending()
    }

    /// Digest ([`crate::pixel_digest`]) of the bitmap last put on the canvas for the current
    /// page.
    pub fn shown_digest(&self) -> Option<u64> {
        self.pages.shown_digest(self.canvas.page)
    }

    /// The request the bitmap last put on the canvas for the current page answers.
    pub fn shown_request(&self) -> Option<&RenderRequest> {
        self.pages.shown_request(self.canvas.page)
    }

    /// The thumbnail of `page` for the Pages list once it is rendered; asks for it the first
    /// time. `None` without a document or while it renders.
    pub(crate) fn thumbnail(
        &mut self,
        ctx: &Context,
        page: pdfxray_core::PageIx,
    ) -> Option<egui::TextureHandle> {
        let doc = &self.document.as_ref()?.doc;
        self.thumbnails.get(ctx, doc, page)
    }

    /// How many thumbnails have a rendered bitmap.
    pub fn thumbnails_rendered(&self) -> usize {
        self.thumbnails.rendered()
    }

    /// True while a thumbnail that was asked for is still being rendered.
    pub fn thumbnails_pending(&self) -> bool {
        self.thumbnails.is_pending()
    }

    /// The preview of `visual` (an object of the open document) once it is rendered; asks the
    /// preview thread for it the first time. `None` without a document.
    pub(crate) fn object_preview(
        &mut self,
        ctx: &Context,
        visual: &pdfxray_core::Visual,
    ) -> Option<PreviewFrame> {
        let doc = &self.document.as_ref()?.doc;
        let frame = self.previews.get(ctx, doc, visual);
        if self.previews.is_pending() {
            // The thread wakes the UI when it is done; this is the safety net.
            ctx.request_repaint_after(RENDER_POLL);
        }
        Some(frame)
    }

    /// True while an object preview that was asked for is still being rendered.
    pub fn previews_pending(&self) -> bool {
        self.previews.is_pending()
    }

    /// Digest ([`crate::pixel_digest`]) of the preview of object `r`, once it is rendered.
    pub fn preview_digest(&self, r: pdfxray_core::ObjRef) -> Option<u64> {
        self.previews.digest(r)
    }

    /// Takes in the bitmaps the render thread finished. Called once per frame.
    pub(crate) fn poll_renders(&mut self) {
        self.pages.poll();
        self.thumbnails.poll();
        self.previews.poll();
    }

    /// Forgets the shown bitmaps of the pages that are not on the canvas any more.
    pub(crate) fn keep_shown_pages(&mut self, visible: &[pdfxray_core::PageIx]) {
        self.pages.keep_shown(visible);
    }

    /// What to show for `request`: the bitmap, the previous one while it renders, or nothing.
    /// `None` without a document.
    pub(crate) fn cached_page(&mut self, ctx: &Context, request: RenderRequest) -> Option<Frame> {
        let doc = &self.document.as_ref()?.doc;
        let frame = self.pages.get(ctx, doc, request);
        if self.pages.is_pending() {
            // The thread wakes the UI when it is done; this is the safety net.
            ctx.request_repaint_after(RENDER_POLL);
        }
        Some(frame)
    }
}

/// `Revision 1`, `Revision 2`, …: one leaf per revision, oldest first, numbered like the chip of
/// the title bar (`rev 2/2`); the newest says so.
fn revision_nodes(doc: &Document) -> Vec<pdfxray_core::TreeNode> {
    let count = u32::try_from(doc.info().revision_count).unwrap_or(u32::MAX);
    (0..count)
        .map(|index| {
            let leaf =
                pdfxray_core::TreeNode::leaf(NodeId::Revision(index), format!("Revision {}", index + 1));
            if index + 1 == count {
                leaf.with_detail("latest")
            } else {
                leaf
            }
        })
        .collect()
}

/// An error and its sources on one line, e.g. `cannot read /x.pdf: No such file or directory`.
pub(crate) fn describe(error: &dyn Error) -> String {
    let mut text = error.to_string();
    let mut source = error.source();
    while let Some(cause) = source {
        text.push_str(": ");
        text.push_str(&cause.to_string());
        source = cause.source();
    }
    text
}
