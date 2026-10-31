//! Content capture: every operator and every painted item of a page, with its graphics state,
//! marked-content context, Form `XObject` nesting and exact bounding boxes.
//!
//! # How it works
//!
//! The engine (`hayro-interpret`) reports what is painted but not *which operator* painted it,
//! nor marked-content property lists, Form `XObject` boundaries, the font resource name or most
//! of the text state. Guessing the operator from the order of callbacks is fragile, so capture
//! does not align two streams of events after the fact. Instead it makes the engine tell us:
//!
//! 1. The stream is parsed with the engine's own tokenizer, giving every operator, its
//!    operands and its byte span (`ops`).
//! 2. An *instrumented copy* of the stream is built in which every operator is preceded by an
//!    empty marked-content pair `/PdfxrayOp << /MCID n >> BDC EMC`. The engine reports
//!    `begin_marked_content` with the tag and `n`, so every draw callback that follows is
//!    attributed to operator `n` exactly. The pair has no effect on painting.
//! 3. The instrumented stream is executed with a recording `Device` (`recorder`).
//! 4. The *walker* (`walker`) goes over the original operators in order, tracks graphics
//!    state, text state, clip, marked-content and Form `XObject` nesting itself, and turns the
//!    events of each operator into [`PaintItem`]s.
//!
//! The engine reaches Form `XObjects` through `Do` and gives no way to instrument their streams.
//! So every drawn form is executed **separately**: the walker enters the form itself and runs
//! the engine over the form's instrumented stream, with the CTM at the `Do` (including `/Matrix`)
//! as the initial transform and the inherited text state restored by a short prologue. The
//! events the page-level run reports for the `Do` are discarded; they only tell the walker that
//! the engine did draw the form (it did not when the form is hidden by optional content, lacks
//! `/BBox`, or nests too deep). Type 3 glyph procedures and pattern cells are not walked:
//! they stay opaque to the capture.
//!
//! The initial transform is the identity, so all geometry the engine reports is already in page
//! space (PDF default user space, points, origin bottom-left, before `/Rotate`).
//!
//! # Details worth knowing
//!
//! * Invisible text (`Tr 3`) is reported by the engine and captured. Clip-only text (`Tr 7`) is
//!   rewritten to `Tr 3` in the instrumented copy so the glyph run is still reported; the clip
//!   effect is tracked by the walker.
//! * Glyph boxes are `[0, advance] x [descent, ascent]` in glyph space from the font's
//!   `/FontDescriptor` (standard-14 families fall back to their core AFM values), united with
//!   the outline's ink box so accents are covered. A Type 3 glyph's box is the `d1` box of its
//!   procedure (the metric box when it declares none or only `d0`); its advance comes from
//!   `/Widths` and `/FontMatrix`. Stroked text modes grow the box by half the line width.
//! * Character codes are read by the walker (the engine does not expose them) using the same
//!   `CMap` logic as the engine. If the number of codes does not match the number of glyphs the
//!   engine drew, codes are reported as `0` and the page is [`Alignment::Degraded`].
//! * Clip boxes are bounding boxes of the transformed clip paths, intersected; a rotated clip
//!   is therefore over-approximated.
//! * Annotation appearances are not drawn or captured.
//! * `OpRecord::gstate` and `PaintItem::gstate` are shared (`Arc`): consecutive operators that
//!   do not change the state point at the same allocation.
//! * The text matrix after a text-showing operator is derived from the glyph advances the
//!   engine reported and is exact when every character code produced a glyph.
//!
//! * Numeric operands follow the engine's rule: when an operator receives more operands than it
//!   takes, the last ones are used.
//!
//! # Degraded pages
//!
//! A page is [`Alignment::Degraded`] when attribution cannot be trusted:
//!
//! * the engine stopped before a painting operator (a malformed operator such as `0 0 10 re`);
//! * the content stream has bytes the tokenizer cannot read, so later operators are neither run
//!   nor captured;
//! * the engine panicked or reported content outside any operator;
//! * a text operator drew a different number of glyphs than the string has character codes;
//! * the engine's first glyph of a text operator is not where the tracked text matrix says
//!   (the walker lost track of the text state, or the font writes vertically);
//! * a Form `XObject` shows text in a font it inherited from the invoking stream but its own
//!   resources do not define.
//!
//! The operator list stays complete up to the point the tokenizer reaches; items after the
//! failure point may be missing or carry uncertain geometry.

mod color;
mod degrade;
mod engine;
mod font;
mod font_cache;
mod geom;
mod gstate;
mod index;
mod marked;
mod object;
mod ops;
mod path;
mod recorder;
mod scopes;
mod text;
mod types;
mod walker;

use hayro_syntax::Pdf;

pub(crate) use font::standard_metrics;
pub use index::PageIndex;
pub use types::{
    Alignment, ArtifactInfo, CaptureNote, CapturedPage, ColorInfo, GState, MarkedSeq, OpRecord, PaintItem,
    PaintKind, StreamRef, TextState, XObjectScope,
};

/// Captures page `page` (zero-based). Returns `None` when the page does not exist.
pub fn capture_page(pdf: &Pdf, page: u32) -> Option<CapturedPage> {
    let pages = pdf.pages();
    let target = pages.get(usize::try_from(page).ok()?)?;
    Some(walker::capture(pdf, page, target))
}

/// Captures every page of `pdf`, in page order.
pub fn capture_document(pdf: &Pdf) -> Vec<CapturedPage> {
    (0..pdf.pages().len())
        .filter_map(|i| capture_page(pdf, u32::try_from(i).ok()?))
        .collect()
}
