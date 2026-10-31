//! The Validation entry and drawer tab end to end, with a fake veraPDF: run, findings, selection, install.
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

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use common::fake::{Call, FakeValidator, struct_rich_report};
use common::{
    Region, app, click_rail, fixture_path, has_label, is_selected, named, nodes_in, open_fixture, texts_in,
    the, wait_for_render, wait_until,
};
use egui::{Key, Modifiers, Role};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use pdfxray_app::{Availability, DrawerTab, Environment, PdfxrayApp, RailItem};
use pdfxray_core::NodeId;
use pdfxray_core::validation::Locator;
use pdfxray_verapdf::Profile;

mod explanations;
mod findings;
mod install;
mod navigation;
mod runs;
mod support;

use support::*;
