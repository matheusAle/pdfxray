//! Helpers shared by the `tags_content` tests.

use super::*;

pub(crate) type App = Harness<'static, PdfxrayApp>;

pub(crate) fn find(nodes: Vec<TreeNode>, label: &str) -> Option<NodeId> {
    nodes.into_iter().find_map(|node| {
        if node.label == label {
            Some(node.id)
        } else {
            find(node.children, label)
        }
    })
}

pub(crate) fn element(name: &str, label: &str) -> NodeId {
    let doc = Document::open(common::fixture_path(name)).unwrap();
    find(doc.tree(TreeKind::Tags), label).unwrap_or_else(|| panic!("no {label} element in {name}"))
}

/// Screen position of a page-space point on the canvas, for a page of height `page_height`.
pub(crate) fn screen_of(harness: &App, point: Point, page_height: f64) -> egui::Pos2 {
    let canvas = harness.get_by_role_and_label(Role::Image, "Page 1 canvas").rect();
    let zoom = f64::from(harness.state().state.canvas.zoom);
    // Screen positions are far smaller than f32's precision limit.
    #[allow(clippy::cast_possible_truncation)]
    let (x, y) = ((point.x * zoom) as f32, ((page_height - point.y) * zoom) as f32);
    canvas.min + vec2(x, y)
}

pub(crate) fn opened(name: &str, rail: &str) -> App {
    let mut harness = app();
    open_fixture(&mut harness, name);
    click_rail(&mut harness, rail);
    wait_for_render(&mut harness);
    harness
}

/// The crumbs of the inspector: its buttons above the title's pin.
pub(crate) fn crumbs(harness: &App) -> Vec<String> {
    buttons_in(harness, Region::Inspector)
        .into_iter()
        .take_while(|name| name != "Pin")
        .collect()
}

pub(crate) fn tags_rows(harness: &mut App) -> usize {
    harness
        .state_mut()
        .state
        .tree_model(pdfxray_app::NavTree::Core(TreeKind::Tags))
        .unwrap()
        .visible_len()
}

pub(crate) fn selected_rows(harness: &App) -> Vec<String> {
    nodes_in(harness, Region::Navigator, Role::Button)
        .into_iter()
        .filter(is_selected)
        .filter_map(|n| n.accesskit_node().label())
        .collect()
}

/// The tag of page 1's MCID 5, the first label of the list.
pub(crate) const OWNER_OF_MCID_5: &str = "Lbl";

/// The label of a marked content row of the Tags tree: it is no button, only text.
pub(crate) fn mcid_row<'h>(harness: &'h App, label: &str) -> egui_kittest::Node<'h> {
    the(harness, Region::Navigator, Role::Label, label)
}

pub(crate) fn mcid_rows(harness: &App) -> Vec<String> {
    texts_in(harness, Region::Navigator, Role::Label)
        .into_iter()
        .filter(|text| text.starts_with("MCID "))
        .collect()
}

pub(crate) fn expanded_tags() -> App {
    let mut harness = opened("struct-rich", "Tags");
    the(&harness, Region::Navigator, Role::Button, "Expand all").click();
    harness.run();
    harness
}
