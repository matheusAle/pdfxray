//! Logical structure through the public query API: the generated fixtures' structure trees,
//! role maps, attributes, namespaces, `ParentTree` cross-checks and the links between tags,
//! content and pages.
//!
//! The helpers the modules share are in `support`; each other module covers one feature.

#![allow(
    clippy::unwrap_used,
    clippy::panic,
    reason = "test helpers fail loudly on unexpected input"
)]

use std::time::Instant;

use pdfxray_core::{
    DiagnosticInfo, Document, LayerMode, NodeId, ObjRef, Point, PropGroup, Props, Rect, Severity, TreeKind,
    TreeNode,
};
use pdfxray_fixtures::{AttributeSource, Defect, Expected, KidFacts, NodeFacts, StructElemFacts};

mod hit_and_breadcrumb;
mod long_doc;
mod no_structure_tree;
mod orphan_mcid;
mod pdf2_namespaces;
mod rolemap_custom;
mod struct_rich;
mod support;
mod tagged_simple;
mod untagged_content;

use support::*;
