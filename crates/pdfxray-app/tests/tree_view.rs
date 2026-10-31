//! The virtualised tree widget: flattening, expansion and the per-frame cost bound.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)]

use egui::Key;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use pdfxray_app::tree_view::{Decor, TreeModel, TreeOutput};
use pdfxray_core::{NodeId, ObjRef, TreeNode};
use pretty_assertions::assert_eq;

fn id(n: u32) -> NodeId {
    NodeId::Object(ObjRef::new(n, 0))
}

fn leaf(n: u32) -> TreeNode {
    TreeNode::leaf(id(n), format!("leaf {n}"))
}

fn small_tree() -> Vec<TreeNode> {
    vec![
        TreeNode::leaf(id(1), "root A").with_children(vec![
            leaf(2),
            TreeNode::leaf(id(3), "branch").with_children(vec![leaf(4), leaf(5)]),
        ]),
        TreeNode::leaf(id(6), "root B").with_children(vec![leaf(7)]),
        leaf(8),
    ]
}

/// `roots` top-level nodes with `children` leaves each: `roots * (children + 1)` nodes.
fn synthetic(roots: u32, children: u32) -> Vec<TreeNode> {
    (0..roots)
        .map(|r| {
            let base = r * (children + 1);
            let kids = (1..=children).map(|c| leaf(base + c)).collect();
            TreeNode::leaf(id(base), format!("root {r}")).with_children(kids)
        })
        .collect()
}

#[test]
fn flattening_keeps_document_order_and_opens_only_the_top_level() {
    let model = TreeModel::new(small_tree());

    assert_eq!(model.len(), 8);
    assert_eq!(
        model.visible_labels(),
        ["root A", "  leaf 2", "  branch", "root B", "  leaf 7", "leaf 8"]
    );
    assert_eq!(model.visible_len(), 6);
}

#[test]
fn toggling_hides_and_restores_the_subtree() {
    let mut model = TreeModel::new(small_tree());

    assert!(model.toggle(&id(1)));
    assert_eq!(model.visible_labels(), ["root A", "root B", "  leaf 7", "leaf 8"]);

    assert!(model.toggle(&id(1)));
    assert!(model.toggle(&id(3)));
    assert_eq!(
        model.visible_labels(),
        [
            "root A",
            "  leaf 2",
            "  branch",
            "    leaf 4",
            "    leaf 5",
            "root B",
            "  leaf 7",
            "leaf 8"
        ]
    );
    // Leaves and unknown ids cannot be toggled.
    assert!(!model.toggle(&id(8)));
    assert!(!model.toggle(&id(99)));
    // Smoke: nothing changed by the failed toggles.
    assert_eq!(model.visible_len(), 8);
}

#[test]
fn an_empty_tree_is_empty() {
    let model = TreeModel::new(Vec::new());

    assert!(model.is_empty());
    assert_eq!(model.len(), 0);
    assert_eq!(model.visible_len(), 0);
}

#[test]
fn a_filter_keeps_the_matches_their_ancestors_and_what_is_under_them() {
    let mut model = TreeModel::new(small_tree());

    // A leaf: it and the path to it.
    model.set_filter(&["leaf 4".to_owned()]);
    assert_eq!(model.visible_labels(), ["root A", "  branch", "    leaf 4"]);
    assert!(model.is_filtered());

    // A branch: everything under it too, and the roots on the way up.
    model.set_filter(&["branch".to_owned()]);
    assert_eq!(
        model.visible_labels(),
        ["root A", "  branch", "    leaf 4", "    leaf 5"]
    );

    // Several words must all match, in any case.
    model.set_filter(&["ROOT".to_owned(), "b".to_owned()]);
    assert_eq!(model.visible_labels(), ["root B", "  leaf 7"]);

    // Nothing matches: nothing is shown, and the tree is still there.
    model.set_filter(&["zzz".to_owned()]);
    assert_eq!(model.visible_len(), 0);
    assert_eq!(model.len(), 8);

    // No words, or only blanks: every row again, as the expansion state says.
    model.set_filter(&["  ".to_owned()]);
    assert!(!model.is_filtered());
    assert_eq!(
        model.visible_labels(),
        ["root A", "  leaf 2", "  branch", "root B", "  leaf 7", "leaf 8"]
    );
}

#[test]
fn expand_all_and_collapse_all_reach_every_row() {
    let mut model = TreeModel::new(small_tree());

    model.expand_all();
    assert_eq!(model.visible_len(), 8);
    model.collapse_all();
    assert_eq!(model.visible_labels(), ["root A", "root B", "leaf 8"]);
    model.expand_all();
    assert_eq!(model.visible_labels()[3], "    leaf 4");
}

#[test]
fn the_filter_matches_the_detail_text_too() {
    let mut model = TreeModel::new(vec![
        TreeNode::leaf(id(1), "Trailer").with_detail("/Size /Root /ID"),
        TreeNode::leaf(id(2), "1 0 obj").with_detail("catalog"),
    ]);

    model.set_filter(&["/root".to_owned()]);

    assert_eq!(model.visible_labels(), ["Trailer"]);
}

struct Frame {
    model: TreeModel,
    last: TreeOutput,
    selected: Option<NodeId>,
}

fn harness(roots: Vec<TreeNode>) -> Harness<'static, Frame> {
    let state = Frame {
        model: TreeModel::new(roots),
        last: TreeOutput::default(),
        selected: None,
    };
    let mut harness = Harness::builder()
        .with_size(egui::vec2(400.0, 400.0))
        .with_context_setup(pdfxray_app::theme::install)
        .build_ui_state(
            |ui, state: &mut Frame| {
                state.last = state.model.ui(ui, state.selected.as_ref(), "test-tree");
                if let Some(clicked) = state.last.clicked.clone() {
                    state.selected = Some(clicked);
                }
            },
            state,
        );
    // A big tree needs a few more frames than the default step limit to settle its layout.
    harness.run_steps(8);
    harness
}

#[test]
fn a_hundred_thousand_nodes_build_only_the_visible_rows() {
    // 10 roots with 10 000 children each: 100 010 nodes, all roots expanded.
    let mut harness = harness(synthetic(10, 10_000));

    assert_eq!(harness.state().model.len(), 100_010);
    assert_eq!(harness.state().model.visible_len(), 100_010);
    let built = harness.state().last.rows_built;
    assert!(built > 5, "something is drawn: {built}");
    assert!(
        built < 40,
        "bounded by the 400 pt viewport, not by the tree: {built}"
    );

    // The first rows are on screen, the far end is not.
    let _first = harness.get_by_role_and_label(egui::Role::Button, "root 0");
    assert!(
        harness
            .query_by_role_and_label(egui::Role::Button, "leaf 99999")
            .is_none()
    );

    // Scrolling far down still builds a bounded number of rows.
    harness.hover_at(egui::pos2(200.0, 200.0));
    harness.run_steps(2);
    harness.input_mut().events.push(egui::Event::MouseWheel {
        unit: egui::MouseWheelUnit::Point,
        delta: egui::vec2(0.0, -400_000.0),
        phase: egui::TouchPhase::Move,
        source: egui::MouseWheelSource::Wheel,
        modifiers: egui::Modifiers::NONE,
    });
    harness.run_steps(8);
    assert!(harness.state().last.rows_built < 40);
    assert!(
        harness
            .query_by_role_and_label(egui::Role::Button, "root 0")
            .is_none()
    );
}

#[test]
fn collapsing_a_huge_root_shrinks_the_visible_list_and_clicking_selects() {
    let mut harness = harness(synthetic(3, 5_000));
    assert_eq!(harness.state().model.visible_len(), 15_003);

    harness.get_by_label("Collapse root 0").click();
    harness.run_steps(8);

    assert_eq!(harness.state().model.visible_len(), 10_003);
    assert_eq!(harness.state().model.visible_labels()[..2], ["root 0", "root 1"]);

    harness
        .get_by_role_and_label(egui::Role::Button, "root 1")
        .click();
    harness.run_steps(8);
    assert_eq!(harness.state().selected, Some(id(5_001)));
    // Smoke: only the toggle and the select changed anything; the tree size is untouched.
    assert_eq!(harness.state().model.len(), 15_003);
}

/// A tag with two references into content and a tag after them, as the Tags tree is built.
fn tags_with_references() -> TreeModel {
    let mcid = |mcid| NodeId::MarkedContent { page: 0, mcid };
    TreeModel::with_decor(
        vec![TreeNode::leaf(id(1), "Document").with_children(vec![
            TreeNode::leaf(id(2), "P").with_children(vec![
                TreeNode::leaf(mcid(0), "MCID 0"),
                TreeNode::leaf(id(9), "OBJR"),
                TreeNode::leaf(mcid(1), "MCID 1"),
            ]),
            TreeNode::leaf(id(3), "Figure"),
        ])],
        |node| Decor {
            content_ref: matches!(node.id, NodeId::MarkedContent { .. }) || node.label == "OBJR",
            ..Decor::default()
        },
    )
}

#[test]
fn content_references_are_listed_but_not_found_by_id_and_arrows_step_over_them() {
    let mut model = tags_with_references();
    model.expand_all();

    assert_eq!(
        model.visible_labels(),
        [
            "Document",
            "  P",
            "    MCID 0",
            "    OBJR",
            "    MCID 1",
            "  Figure"
        ]
    );
    assert!(model.contains(&id(2)));
    assert!(!model.contains(&id(9)));
    assert!(!model.contains(&NodeId::MarkedContent { page: 0, mcid: 0 }));
    // Down from the tag goes past its three references; Up comes back past them.
    assert_eq!(model.key_target(Some(&id(2)), Key::ArrowDown), Some(id(3)));
    assert_eq!(model.key_target(Some(&id(3)), Key::ArrowUp), Some(id(2)));
    // Right on the open tag has no child tag to go to.
    assert_eq!(model.key_target(Some(&id(2)), Key::ArrowRight), None);
    // The last tag has nowhere to go down, though nothing is below it.
    assert_eq!(model.key_target(Some(&id(3)), Key::ArrowDown), None);
    // Smoke: the references are still counted.
    assert_eq!(model.len(), 6);
}

#[test]
fn a_content_reference_row_reports_no_click_hover_or_selection() {
    let state = Frame {
        model: tags_with_references(),
        last: TreeOutput::default(),
        selected: None,
    };
    let mut harness = Harness::builder()
        .with_size(egui::vec2(400.0, 400.0))
        .with_context_setup(pdfxray_app::theme::install)
        .build_ui_state(
            |ui, state: &mut Frame| {
                state.last = state.model.ui(ui, state.selected.as_ref(), "refs");
                if let Some(clicked) = state.last.clicked.clone() {
                    state.selected = Some(clicked);
                }
            },
            state,
        );
    harness.state_mut().model.expand_all();
    harness.run_steps(8);

    let reference = harness
        .get_by_role_and_label(egui::Role::Label, "OBJR")
        .rect()
        .center();
    harness.hover_at(reference);
    harness.run_steps(3);
    assert_eq!(harness.state().last.hovered, None);
    harness.drag_at(reference);
    harness.drop_at(reference);
    harness.run_steps(3);
    assert_eq!(harness.state().last.clicked, None);
    assert_eq!(harness.state().selected, None);

    // The tag above it is still a button that selects.
    harness.get_by_role_and_label(egui::Role::Button, "P").click();
    harness.run_steps(3);
    assert_eq!(harness.state().selected, Some(id(2)));
    // Smoke: every row, references included, was built.
    assert_eq!(harness.state().last.rows_built, 6);
}

#[test]
fn a_row_selected_far_down_is_scrolled_to_the_middle_of_the_viewport() {
    // 10 roots with 40 leaves each, all open: the row of `leaf 250` is far below the 400 pt of
    // the viewport. Rows are 28 pt tall and 4 pt apart, so the 32 pt pitch decides where it is.
    let mut harness = harness(synthetic(10, 40));
    assert!(
        harness
            .query_by_role_and_label(egui::Role::Button, "leaf 250")
            .is_none()
    );

    harness.state_mut().selected = Some(id(250));
    harness.run_steps(8);

    let row = harness
        .get_by_role_and_label(egui::Role::Button, "leaf 250")
        .rect();
    // On screen, and near the middle of the 400 pt viewport rather than at its edge or beyond.
    assert!(row.min.y >= 0.0 && row.max.y <= 400.0, "{row:?}");
    assert!(
        (row.center().y - 200.0).abs() < 40.0,
        "the selected row is centred: {row:?}"
    );
    // Smoke: the rows around it are drawn too, in the same column.
    let next = harness
        .get_by_role_and_label(egui::Role::Button, "leaf 251")
        .rect();
    assert_eq!(next.min.x, row.min.x);
    assert_eq!(next.min.y - row.min.y, 32.0);
}

#[test]
fn a_row_shows_a_hand_and_its_chevron_is_a_wider_target_than_the_glyph() {
    let mut harness = harness(small_tree());
    let hover = |harness: &mut Harness<'static, Frame>, at: egui::Pos2| {
        harness.hover_at(at);
        harness.run_steps(2);
        harness.output().platform_output.cursor_icon
    };

    let row = harness.get_by_role_and_label(egui::Role::Button, "root B").rect();
    assert_eq!(hover(&mut harness, row.center()), egui::CursorIcon::PointingHand);

    // `branch` is closed: its chevron opens it.
    let chevron = harness.get_by_label("Expand branch").rect();
    // The glyph is 12 pt; the target around it is 20 pt wide and the height of the row.
    assert_eq!(chevron.width(), 20.0);
    assert_eq!(chevron.height(), 28.0);
    assert_eq!(
        hover(&mut harness, chevron.center()),
        egui::CursorIcon::PointingHand
    );
    // A click 8 pt off the centre, outside the old 14 pt slot, still opens the row.
    assert!(
        !harness
            .state()
            .model
            .visible_labels()
            .contains(&"    leaf 4".to_owned())
    );
    let aside = egui::pos2(chevron.center().x - 8.0, chevron.center().y);
    harness.hover_at(aside);
    harness.drag_at(aside);
    harness.drop_at(aside);
    harness.run_steps(4);
    assert!(
        harness
            .state()
            .model
            .visible_labels()
            .contains(&"    leaf 4".to_owned())
    );
    // Smoke: opening a row did not select it.
    assert_eq!(harness.state().selected, None);
}

#[test]
fn a_row_the_keyboard_is_on_shows_the_accent_ring_before_it_is_selected() {
    let mut harness = harness(small_tree());
    let ring = |harness: &Harness<'static, Frame>| {
        harness.output().shapes.iter().any(|clipped| {
            fn has_ring(shape: &egui::Shape) -> bool {
                match shape {
                    egui::Shape::Vec(shapes) => shapes.iter().any(has_ring),
                    egui::Shape::Rect(r) => {
                        r.stroke.color == pdfxray_app::theme::tokens::ACCENT
                            && r.fill == egui::Color32::TRANSPARENT
                    }
                    _ => false,
                }
            }
            has_ring(&clipped.shape)
        })
    };
    assert!(!ring(&harness));

    harness
        .get_by_role_and_label(egui::Role::Button, "root B")
        .focus();
    harness.run_steps(3);

    assert!(ring(&harness));
    // Smoke: focusing did not select it.
    assert_eq!(harness.state().selected, None);
}
