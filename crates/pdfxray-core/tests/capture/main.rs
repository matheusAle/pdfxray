//! Content capture over the generated fixtures and over small synthetic pages.
//!
//! Every test asserts concrete values first and ends with a smoke assertion on a derived property
//! (counts, containment, ordering) so a wrong value cannot hide behind a right-looking shape.
//!
//! The helpers the modules share are in `support`; each other module covers one feature.

// Helpers here are not `#[test]` functions, so clippy.toml's allow-*-in-tests does not reach them;
// failing loudly is what a test helper should do. Float comparisons are against operand values
// echoed verbatim from the content stream (exactly representable), not computed results.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::float_cmp)]

use std::fs;

use hayro_syntax::Pdf;
use pdfxray_core::capture::{
    Alignment, CapturedPage, ColorInfo, MarkedSeq, PageIndex, PaintItem, PaintKind, StreamRef,
    capture_document, capture_page,
};
use pdfxray_core::{ObjRef, Point, Rect};
use pdfxray_fixtures::writer::{PdfVersion, PdfWriter, Trailer};
use pdfxray_fixtures::{Expected, fixtures_dir};

mod every_fixture;
mod forms;
mod graphics_state;
mod marked_content;
mod minimal_one_page;
mod page_boxes;
mod support;
mod tagged;
mod text;

use support::*;
