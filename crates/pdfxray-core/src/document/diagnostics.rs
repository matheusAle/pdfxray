//! [`DiagnosticInfo`], the serialisable diagnostic, and the checks only visible above the syntax
//! layer (page tree shape, page boxes, the engine's view).

use std::ops::Range;

use pdfxray_raw::{RawDiagnostic, Span};
use serde::Serialize;

use super::Document;
use crate::numeric::usize_from_u32;
use crate::objects::ObjectIndex;
use crate::page::PageInfo;
use crate::pagetree::PageTreeIssue;
use crate::{BoxKind, NodeId, ObjRef, PropGroup, Props, Severity, TreeNode, fmt_rect};

/// A diagnostic in serialisable form.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DiagnosticInfo {
    /// Stable machine-readable code.
    pub code: &'static str,
    /// How serious it is.
    pub severity: Severity,
    /// Human-readable explanation.
    pub message: String,
    /// Byte range `[start, end)` in the file.
    pub span: [usize; 2],
    /// The object the problem sits in, when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub object: Option<ObjRef>,
}

impl DiagnosticInfo {
    /// What this diagnostic means for someone reading the file, in plain language, with the
    /// figures from [`DiagnosticInfo::message`] written in where the code has any ("The stream says
    /// it is 1,200 bytes long but really is 1,204"). `None` for a code pdfxray has no text for.
    ///
    /// The same text without figures is [`crate::explain::diagnostic`].
    pub fn explain(&self) -> Option<String> {
        crate::explain::diagnostic_with_message(self.code, &self.message)
    }

    /// The bytes of the file the diagnostic is about, `None` when it has no position (a core
    /// diagnostic about the whole document).
    pub fn byte_range(&self) -> Option<Range<usize>> {
        let [start, end] = self.span;
        (end >= start && end > 0).then_some(start..end)
    }

    pub(super) fn with_object(mut self, object: Option<ObjRef>) -> Self {
        self.object = object;
        self
    }

    pub(crate) fn core(
        code: &'static str,
        severity: Severity,
        message: String,
        object: Option<(ObjRef, Span)>,
    ) -> Self {
        let span = object.map_or([0, 0], |(_, s)| [s.start, s.end]);
        Self {
            code,
            severity,
            message,
            span,
            object: object.map(|(r, _)| r),
        }
    }
}

/// Problems only visible above the syntax layer: page tree shape, page boxes, engine view.
pub(super) fn structural_diagnostics(
    objects: &ObjectIndex,
    pages: &[PageInfo],
    raw_page_count: usize,
    tree_issues: &[PageTreeIssue],
    engine_ok: bool,
) -> Vec<DiagnosticInfo> {
    let at = |r: ObjRef| objects.get(r).map(|o| (r, o.span));
    let mut out = Vec::new();
    for issue in tree_issues {
        out.push(match issue {
            PageTreeIssue::Missing => DiagnosticInfo::core(
                "page-tree-missing",
                Severity::Error,
                "the catalog has no /Pages tree".into(),
                None,
            ),
            PageTreeIssue::BadKid { parent } => DiagnosticInfo::core(
                "page-tree-bad-kid",
                Severity::Error,
                format!("/Kids of {parent} has an entry that is not a page node or loops back"),
                at(*parent),
            ),
            PageTreeIssue::TooDeep => DiagnosticInfo::core(
                "page-tree-too-deep",
                Severity::Error,
                "the page tree is nested too deeply to follow".into(),
                None,
            ),
        });
    }
    if engine_ok && raw_page_count != pages.len() {
        out.push(DiagnosticInfo::core(
            "page-count-mismatch",
            Severity::Error,
            format!(
                "the page tree lists {raw_page_count} pages but the renderer found {}",
                pages.len()
            ),
            None,
        ));
    }
    for page in pages {
        let obj = page.obj.and_then(at);
        for b in &page.boxes {
            if b.kind != BoxKind::Media && !page.media_box.contains_rect(&b.rect) {
                out.push(DiagnosticInfo::core(
                    "box-outside-mediabox",
                    Severity::Warning,
                    format!(
                        "{} of {} {} extends outside its MediaBox {}",
                        b.kind.key(),
                        page.label(),
                        fmt_rect(b.rect),
                        fmt_rect(page.media_box)
                    ),
                    obj,
                ));
            }
        }
    }
    out
}

impl From<&RawDiagnostic> for DiagnosticInfo {
    fn from(d: &RawDiagnostic) -> Self {
        Self {
            code: d.kind.code(),
            severity: d.kind.severity().into(),
            message: d.kind.to_string(),
            span: [d.span.start, d.span.end],
            object: None,
        }
    }
}

impl Document {
    pub(super) fn diagnostics_tree(&self) -> Vec<TreeNode> {
        let mut groups = Vec::new();
        if !self.diagnostics.is_empty() {
            let children = self
                .diagnostics
                .iter()
                .zip(0..)
                .map(|(d, i)| TreeNode::leaf(NodeId::Diagnostic(i), d.code).with_detail(&d.message))
                .collect();
            groups.push(
                TreeNode::leaf(NodeId::Group(300), "File structure")
                    .with_detail(format!("{} issues", self.diagnostics.len()))
                    .with_children(children),
            );
        }
        if let Some(run) = &self.validation {
            let children = run
                .findings
                .iter()
                .zip(0..)
                .map(|(f, i)| TreeNode::leaf(NodeId::Validation(i), &f.rule).with_detail(&f.description))
                .collect();
            let verdict = if run.compliant {
                "compliant"
            } else {
                "not compliant"
            };
            groups.push(
                TreeNode::leaf(NodeId::Group(301), format!("Validation: {}", run.profile))
                    .with_detail(format!("{verdict}, {} failed checks", run.findings.len()))
                    .with_children(children),
            );
        }
        groups
    }

    pub(super) fn diagnostic_props(&self, index: u32) -> Option<Props> {
        let d = self.diagnostics.get(usize_from_u32(index))?;
        let group = PropGroup::new("Diagnostic")
            .row("Code", d.code)
            .row("Severity", format!("{:?}", d.severity))
            .row("Message", &d.message)
            .row("Bytes", format!("{}..{}", d.span[0], d.span[1]));
        Some(Props {
            id: NodeId::Diagnostic(index),
            title: d.code.to_owned(),
            kind: "diagnostic",
            groups: vec![group],
        })
    }
}
