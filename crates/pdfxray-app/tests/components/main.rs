//! The shared components: behaviour of each one, and the "components" snapshot that mirrors
//! the "Components" frame of the design.
//!
//! The helpers the modules share are in `support`; each other module covers one feature.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)]

#[path = "../common/mod.rs"]
mod common;

use std::collections::HashSet;

use egui::{Id, Key, Role, vec2};
use egui_kittest::kittest::{NodeT, Queryable};
use egui_kittest::{Harness, HarnessBuilder};
use pdfxray_app::components::{
    Annotation, Badge, Chip, Crumb, Icon, Keycap, PropRow, Segment, Tone, ToolButton, TreeRow, crumb,
    popover, section, segmented, tooltip,
};
use pdfxray_app::theme::{fonts, tokens};

mod active_states;
mod badges;
mod chips;
mod crumbs;
mod design_tokens;
mod icons;
mod keycaps;
mod popovers;
mod premium_theme;
mod prop_rows;
mod sections;
mod segmented_controls;
mod snapshot;
mod support;
mod tool_buttons;
mod tooltips;
mod tree_rows;

use support::*;
