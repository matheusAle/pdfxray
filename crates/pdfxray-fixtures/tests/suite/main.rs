//! Integration tests for the fixture generator.
//!
//! * `drift`: committed files equal regenerated output; building is deterministic.
//! * `facts`: the `Expected` facts re-derived from the raw bytes by an independent scanner.
//! * `defects`: every deliberate defect is present in the bytes, checked without the writer.
//! * `hayro`: what `hayro-syntax` and `hayro` make of each fixture.
//! * `content_facts`: text, paint, image and Type 3 facts re-derived by a content-stream interpreter.
//! * `structure_facts`: the rich outline, annotations and namespaces re-derived from the objects.
//! * `geometry_facts`: page geometry and resource inheritance re-derived from the page tree.

// Helpers in this crate are not `#[test]` functions, so clippy.toml's allow-*-in-tests does not
// reach them; failing loudly is exactly what a test helper should do. The numeric lints are noise
// here: every number is a small, exactly representable PDF coordinate or byte offset.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::type_complexity
)]

mod common;
mod content;
mod content_facts;
mod defects;
mod drift;
mod facts;
mod geometry_facts;
mod hayro;
mod outline;
mod pdfobj;
mod structure_facts;
