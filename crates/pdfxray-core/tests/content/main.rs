//! Content nodes (operators, glyphs, marked content, artifacts) through the public `Document`
//! API, checked against the generated fixtures and the facts their generator recorded.
//!
//! Every test asserts concrete values first and ends with a smoke assertion on a derived property
//! (counts, containment, ordering) so a wrong value cannot hide behind a right-looking shape.
//!
//! The helpers the modules share are in `support`; each other module covers one feature.

// Helpers here are not `#[test]` functions, so clippy.toml's allow-*-in-tests does not reach them;
// failing loudly is what a test helper should do.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;

use pdfxray_core::capture::StreamRef;
use pdfxray_core::{
    Document, LayerMode, NodeId, ObjRef, Point, Props, Rect, TreeKind, TreeNode, fmt_rect, num,
};
use pdfxray_fixtures::writer::{PdfVersion, PdfWriter, Trailer};
use pdfxray_fixtures::{Expected, fixtures_dir};

mod artifacts;
mod degraded;
mod forms;
mod graphics_state;
mod images;
mod minimal_one_page;
mod missing_nodes;
mod support;
mod tagged_simple;
mod text_state;
mod type3_font;
mod untagged_content;

use support::*;
