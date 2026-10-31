//! Shared helpers of the app's end-to-end tests.

// Each test binary uses a different subset.
#![allow(dead_code, clippy::unwrap_used, clippy::expect_used, clippy::panic)]

pub(crate) mod fake;

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use egui::{Rect, Role, Vec2, vec2};
use egui_kittest::kittest::{NodeT, Queryable};
use egui_kittest::{Harness, HarnessBuilder, Node};
use pdfxray_app::state::drawer::STRIP_H;
use pdfxray_app::theme::tokens;
use pdfxray_app::{PdfxrayApp, Spawner};

static GPU: Mutex<()> = Mutex::new(());

/// Held for the whole of a test that renders with wgpu: the renderer panics when several
/// snapshot tests of one test binary create devices and render at the same time. Declare it
/// before the harness so it is released after the harness (and its snapshot results) are gone.
pub(crate) fn gpu_lock() -> MutexGuard<'static, ()> {
    GPU.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Window size of every test: the size of the design frames.
pub(crate) const WINDOW: Vec2 = vec2(1440.0, 900.0);

/// A host of the shell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Region {
    /// Title bar and status bar.
    Chrome,
    /// The activity rail.
    Rail,
    /// The navigator: header, filter and list.
    Navigator,
    /// The centre: toolbar, canvas and its cards.
    Center,
    /// The drawer under the canvas.
    Drawer,
    /// The inspector.
    Inspector,
}

/// A harness for the whole app. Animations are off (as if "Reduce motion" were on) so no
/// snapshot or assertion can catch a frame in the middle of one; a test of the motion itself
/// turns them on again with `pdfxray_app::theme::motion::set_reduce_motion(false)`.
pub(crate) fn builder() -> HarnessBuilder<PdfxrayApp> {
    pdfxray_app::theme::motion::set_reduce_motion(true);
    Harness::builder().with_size(WINDOW)
}

/// The app in its initial state, after the first frames.
pub(crate) fn app() -> Harness<'static, PdfxrayApp> {
    let mut harness = builder().build_eframe(|cc| PdfxrayApp::new(cc));
    harness.run();
    harness
}

pub(crate) fn fixture_path(name: &str) -> PathBuf {
    pdfxray_fixtures::pdf_path(name)
}

/// Opens a generated fixture the way a user does: press the open button and choose the file. The
/// native dialog is replaced by a picker that answers with the fixture's path.
pub(crate) fn open_fixture(harness: &mut Harness<'_, PdfxrayApp>, name: &str) {
    open_path(harness, fixture_path(name));
}

/// Opens the file at `path` in this window the way [`open_fixture`] does.
pub(crate) fn open_path(harness: &mut Harness<'_, PdfxrayApp>, path: PathBuf) {
    // The drop zone has the button until a file is open. Then Open… would start a new window, so
    // the file is dropped onto this one, which replaces its document.
    if harness.query_by_label("Open file…").is_some() {
        harness.state_mut().state.picker = Box::new(move || Some(path.clone()));
        harness.get_by_label("Open file…").click();
    } else {
        drop_file(harness, &path);
    }
    wait_for_open(harness);
    // The status bar prints how long opening took; a fixed time keeps snapshots the same on
    // every run (the number of the design's frames).
    if let Some(loaded) = harness.state_mut().state.document.as_mut() {
        loaded.opened_in = Duration::from_millis(310);
    }
    harness.run();
    wait_for_render(harness);
}

/// A file dropped onto the window, as the platform reports it.
#[derive(Debug)]
pub(crate) struct DroppedPath(pub(crate) PathBuf);

impl egui::DroppedFile for DroppedPath {
    fn path(&self) -> &Path {
        &self.0
    }

    fn bytes(&self) -> Result<Vec<u8>, String> {
        Err("not readable in tests".to_owned())
    }
}

/// Drops the file at `path` onto the window and steps one frame (a spinner may be turning, so
/// the app is not run to rest).
pub(crate) fn drop_file(harness: &mut Harness<'_, PdfxrayApp>, path: &Path) {
    harness
        .input_mut()
        .dropped_files
        .push(Arc::new(DroppedPath(path.to_path_buf())));
    harness.step();
}

/// What a recording spawner was asked to start: one entry per new window, with its file.
pub(crate) type Started = Rc<RefCell<Vec<Option<PathBuf>>>>;

/// A spawner that starts nothing and records every request.
pub(crate) fn recording_spawner() -> (Spawner, Started) {
    let started: Started = Rc::default();
    let log = Rc::clone(&started);
    let spawner: Spawner = Box::new(move |path| {
        log.borrow_mut().push(path.map(Path::to_path_buf));
        Ok(())
    });
    (spawner, started)
}

/// Steps the app until the file that is opening (on its own thread) is open, or has failed.
pub(crate) fn wait_for_open(harness: &mut Harness<'_, PdfxrayApp>) {
    wait_until(harness, "the file to open", |state| state.opening.job().is_none());
}

/// Steps the app until the render thread has answered the latest request, so the page bitmap
/// (and the image to click on) is there.
pub(crate) fn wait_for_render(harness: &mut Harness<'_, PdfxrayApp>) {
    wait_until(harness, "the page render", |state| !state.render_pending());
}

/// Steps the app until every thumbnail of the Pages list that was asked for is rendered.
pub(crate) fn wait_for_thumbnails(harness: &mut Harness<'_, PdfxrayApp>) {
    wait_until(harness, "the page thumbnails", |state| {
        !state.thumbnails_pending()
    });
}

/// Steps the app until every object preview that was asked for is rendered.
pub(crate) fn wait_for_previews(harness: &mut Harness<'_, PdfxrayApp>) {
    wait_until(harness, "the object previews", |state| !state.previews_pending());
}

/// Clicks the rail entry named `name` (`Tags`, `Content`, `Artifacts`, `Pages`, `Objects`,
/// `Revisions`, `Validation`).
pub(crate) fn click_rail(harness: &mut Harness<'_, PdfxrayApp>, name: &str) {
    the(harness, Region::Rail, Role::Button, name).click();
    harness.run_steps(3);
}

/// Clicks the drawer tab named `name`.
pub(crate) fn click_drawer_tab(harness: &mut Harness<'_, PdfxrayApp>, name: &str) {
    the(harness, Region::Drawer, Role::Button, name).click();
    harness.run_steps(3);
}

/// Height of the drawer panel as it is now: the strip while collapsed, else its own height.
fn drawer_height(harness: &Harness<'_, PdfxrayApp>) -> f32 {
    let drawer = &harness.state().state.drawer;
    if drawer.open { drawer.height } else { STRIP_H }
}

/// The label of the title bar's one chip: it is the button that starts with the PDF version.
pub(crate) fn title_chip(harness: &Harness<'_, PdfxrayApp>) -> String {
    texts_in(harness, Region::Chrome, Role::Button)
        .into_iter()
        .find(|label| label.starts_with("PDF "))
        .expect("the title bar has a chip")
}

/// Clicks the title chip, which opens the file summary and the revisions.
pub(crate) fn click_title_chip(harness: &Harness<'_, PdfxrayApp>) {
    the(harness, Region::Chrome, Role::Button, &title_chip(harness)).click();
}

/// The region of the shell that contains `rect`'s centre.
pub(crate) fn region_of(harness: &Harness<'_, PdfxrayApp>, rect: Rect) -> Region {
    let centre = rect.center();
    if centre.y < tokens::TITLE_H || centre.y > WINDOW.y - tokens::STATUS_H {
        return Region::Chrome;
    }
    if centre.x < tokens::RAIL_W {
        Region::Rail
    } else if centre.x < tokens::RAIL_W + tokens::NAVIGATOR_W {
        Region::Navigator
    } else if centre.x > WINDOW.x - tokens::INSPECTOR_W {
        Region::Inspector
    } else if centre.y > WINDOW.y - tokens::STATUS_H - drawer_height(harness) {
        Region::Drawer
    } else {
        Region::Center
    }
}

/// Nodes with `role` inside `region`, in layout order.
pub(crate) fn nodes_in<'h>(
    harness: &'h Harness<'_, PdfxrayApp>,
    region: Region,
    role: Role,
) -> Vec<Node<'h>> {
    harness
        .query_all_by(move |node| node.role() == role)
        .filter(|node| region_of(harness, node.rect()) == region)
        .collect()
}

/// Accessible names (buttons, links) or texts (labels) of the nodes with `role` in `region`.
pub(crate) fn texts_in(harness: &Harness<'_, PdfxrayApp>, region: Region, role: Role) -> Vec<String> {
    nodes_in(harness, region, role)
        .into_iter()
        .filter_map(|node| {
            if role == Role::Label {
                node.value()
            } else {
                node.accesskit_node().label()
            }
        })
        .collect()
}

/// The single node with `role` and exactly this name/text in `region`.
pub(crate) fn the<'h>(
    harness: &'h Harness<'_, PdfxrayApp>,
    region: Region,
    role: Role,
    text: &str,
) -> Node<'h> {
    let mut found: Vec<Node<'h>> = nodes_in(harness, region, role)
        .into_iter()
        .filter(|node| {
            let own = if role == Role::Label {
                node.value()
            } else {
                node.accesskit_node().label()
            };
            own.as_deref() == Some(text)
        })
        .collect();
    assert_eq!(
        found.len(),
        1,
        "expected one {role:?} `{text}` in {region:?}, found {}",
        found.len()
    );
    found.remove(0)
}

/// The text of every `Label` in the whole window.
pub(crate) fn all_label_texts(harness: &Harness<'_, PdfxrayApp>) -> Vec<String> {
    harness
        .query_all_by(|node| node.role() == Role::Label)
        .filter_map(|node| node.value())
        .collect()
}

pub(crate) fn has_label(harness: &Harness<'_, PdfxrayApp>, text: &str) -> bool {
    all_label_texts(harness).iter().any(|t| t == text)
}

/// Presses and releases the primary button at `pos` (screen coordinates), moving the pointer
/// there first.
pub(crate) fn click_at(harness: &mut Harness<'_, PdfxrayApp>, pos: egui::Pos2) {
    harness.hover_at(pos);
    harness.drag_at(pos);
    harness.drop_at(pos);
    harness.run();
}

/// Steps the app, giving background threads time, until `done` holds for the state.
pub(crate) fn wait_until(
    harness: &mut Harness<'_, PdfxrayApp>,
    what: &str,
    done: impl Fn(&pdfxray_app::AppState) -> bool,
) {
    for _ in 0..600 {
        harness.step();
        if done(&harness.state().state) {
            // A spinner may still be on screen, which `run` would never see settle.
            harness.run_steps(3);
            return;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    panic!("timed out waiting for {what}");
}

/// Whether a widget drawn as selectable (rail entry, tab, segment, active tool) is selected.
pub(crate) fn is_selected(node: &Node<'_>) -> bool {
    node.accesskit_node().toggled() == Some(egui::accesskit::Toggled::True)
}

/// Accessible names of the buttons in `region`, in layout order.
pub(crate) fn buttons_in(harness: &Harness<'_, PdfxrayApp>, region: Region) -> Vec<String> {
    texts_in(harness, region, Role::Button)
}

/// The names of the rows of the navigator's tree, in order: its buttons after the `header`
/// ones (expand all and collapse all, and the Objects/Resources switch when it is there).
pub(crate) fn nav_rows(harness: &Harness<'_, PdfxrayApp>, header: usize) -> Vec<String> {
    buttons_in(harness, Region::Navigator).split_off(header)
}

/// The nodes of `region` with `role` whose name is `name`, in layout order.
pub(crate) fn named<'h>(
    harness: &'h Harness<'_, PdfxrayApp>,
    region: Region,
    role: Role,
    name: &str,
) -> Vec<Node<'h>> {
    nodes_in(harness, region, role)
        .into_iter()
        .filter(|node| {
            let own = if role == Role::Label {
                node.value()
            } else {
                node.accesskit_node().label()
            };
            own.as_deref() == Some(name)
        })
        .collect()
}

/// The texts of the labels of `region` after the label `from` and before the label `to`, in
/// layout order.
pub(crate) fn labels_between(
    harness: &Harness<'_, PdfxrayApp>,
    region: Region,
    from: &str,
    to: &str,
) -> Vec<String> {
    let labels = texts_in(harness, region, Role::Label);
    let start = labels
        .iter()
        .position(|t| t == from)
        .unwrap_or_else(|| panic!("no label `{from}` in {labels:?}"));
    let end = labels[start..]
        .iter()
        .position(|t| t == to)
        .map_or(labels.len(), |at| start + at);
    labels[start + 1..end].to_vec()
}

/// The digest of the bitmap on the canvas.
pub(crate) fn digest(harness: &Harness<'_, PdfxrayApp>) -> u64 {
    harness.state().state.shown_digest().unwrap()
}

/// The digest of what `render_page` makes of page 0 of `fixture` at the scale on the canvas.
pub(crate) fn expected_digest(
    harness: &Harness<'_, PdfxrayApp>,
    fixture: &str,
    content: pdfxray_render::ContentFilter,
    overlays: impl FnOnce(&pdfxray_core::Document) -> Vec<pdfxray_render::Overlay>,
) -> u64 {
    let doc = pdfxray_core::Document::open(fixture_path(fixture)).unwrap();
    let scale = harness.state().state.shown_request().unwrap().scale.scale();
    let spec = pdfxray_render::RenderSpec {
        scale,
        overlays: overlays(&doc),
        content,
    };
    pdfxray_app::pixel_digest(&pdfxray_render::render_page(&doc, 0, &spec).unwrap().rgba)
}
