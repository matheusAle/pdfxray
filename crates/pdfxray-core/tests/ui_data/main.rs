//! The data the revamped UI reads, checked against the generated fixtures, the vendored veraPDF
//! corpus and a few documents built on the spot: operator notes, matrices, glyph facts, coverage,
//! object references, revisions, paint order, print figures, opening progress, encryption and
//! diagnostic explanations.
//!
//! Every test asserts concrete values first and ends with a smoke assertion on a derived property
//! so a wrong value cannot hide behind a right-looking shape.
//!
//! The helpers the modules share are in `support`; each other module covers one feature.

// Helpers here are not `#[test]` functions, so clippy.toml's allow-*-in-tests does not reach them;
// failing loudly is what a test helper should do. Float comparisons are against operands echoed
// verbatim from the files or sums of exactly representable values.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::float_cmp)]

use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use pdfxray_core::capture::PaintKind;
use pdfxray_core::{
    CoreError, Document, FontFacts, Matrix, NodeId, ObjRef, OpenStage, Point, ResKind, UnicodeSource, explain,
};
use pdfxray_fixtures::fixtures_dir;
use pdfxray_fixtures::writer::{FIXED_ID_HEX, PdfVersion, PdfWriter, Trailer};

mod coverage;
mod encryption;
mod explanations;
mod forms;
mod glyphs;
mod operators;
mod paint_order;
mod print_figures;
mod progress;
mod references;
mod revisions;
mod support;

use support::*;
