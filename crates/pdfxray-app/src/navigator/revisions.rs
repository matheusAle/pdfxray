//! The Revisions entry: one row per revision of the file.

use egui::Ui;

use super::tree_body;
use crate::state::{AppState, NavTree};

pub(super) fn ui(ui: &mut Ui, state: &mut AppState) {
    tree_body(ui, state, NavTree::Revisions, &[], "This file has one revision.");
}
