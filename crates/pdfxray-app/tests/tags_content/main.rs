//! The Tags and Content screens: what is selected and shown after clicking, hovering,
//! double-clicking and typing on the canvas, the trees, the drawer and the inspector.
//!
//! The helpers the modules share are in `support`; each other module covers one feature.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::float_cmp)]

#[path = "../common/mod.rs"]
mod common;

use common::{
    Region, app, buttons_in, click_at, click_rail, has_label, is_selected, named, nodes_in, open_fixture,
    texts_in, the, wait_for_render,
};
use egui::{Event, Key, Modifiers, Role, vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use pdfxray_app::PdfxrayApp;
use pdfxray_app::components::prop_hints::hint;
use pdfxray_app::view::ZoomMode;
use pdfxray_core::{Document, NodeId, ObjRef, Point, TreeKind, TreeNode};

mod inspector;
mod marked_content;
mod selection;
mod support;
mod tags_tree;

use support::*;
