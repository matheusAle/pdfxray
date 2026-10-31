//! pdfxray document model and query API.
//!
//! This is the single entry point for the UI, the CLI and the tests: nothing outside this
//! crate (except `pdfxray-render`) talks to the raw syntax layer or the engine directly.

// The workspace allows these two pedantic lints; the two library crates under the query API
// document every error and panic of their public API.
#![warn(clippy::missing_errors_doc, clippy::missing_panics_doc)]

pub mod capture;
mod content;
mod crypto;
mod document;
mod encryption;
mod error;
pub mod explain;
mod geom;
mod id;
mod layer;
mod matrix;
mod objects;
mod open;
mod page;
mod pagetree;
mod parallel;
mod props;
mod resources;
mod revisions;
mod tags;
#[cfg(test)]
mod test_support;
mod tree;
pub mod validation;

pub use content::{ActualText, CharCode, Coverage, GlyphInfo, MAX_PAINT_STACK, PaintStep, UnicodeSource};
pub use document::{DiagnosticInfo, DocInfo, Document, EngineError};
pub use encryption::{EncryptionInfo, Permissions};
pub use error::CoreError;
pub use geom::{Point, Rect};
pub use id::{BoxKind, NodeId, PageIx, ParseNodeIdError, ResKind};
pub use layer::LayerMode;
pub use matrix::{Decomposed, Matrix};
pub use objects::StreamBytes;
pub use open::OpenStage;
pub use page::{PageInfo, PrintFigures, fmt_rect, num};
pub use pagetree::{BoxSource, PageBox};
pub use pdfxray_raw::ObjRef;
/// Audited numeric conversions shared by every crate above `pdfxray-raw`.
pub use pdfxray_raw::numeric;
pub use props::{PropGroup, PropRow, Props};
pub use resources::{
    ColorSpaceVisual, FontFacts, FontProgram, FontVisual, FormVisual, ImageVisual, MAX_PREVIEW_GLYPHS,
    ObjectRefs, PatternVisual, SPECIMEN, ShadingStop, ShadingVisual, UsedResource, Visual, VisualKind,
};
pub use revisions::{RevisionInfo, XrefKind};
pub use tags::{
    HeaderRef, HeaderSource, IrregularRow, LangChange, LinkFlag, LinkTarget, NoteFlag, NoteLinkSource,
    NoteRole, OrderComparison, OrderStep, PresetRule, ReadingOrder, ReadingOrderSource, RoleClass, RowGroup,
    SkipReason, SpeechCell, SpeechItem, SpeechOrder, SpeechPreset, StepKind, Substitution, TableCell,
    TableModel, TableRules, TableScope, TagBox,
};
pub use tree::{Badge, Severity, TreeKind, TreeNode};
pub use validation::{RuleText, ValidationFinding, ValidationRun};
