use serde::{Deserialize, Serialize};

use crate::{NodeId, PageIx};

/// Which navigation tree to build.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TreeKind {
    /// Pages and their boxes.
    Pages,
    /// Raw objects from the trailer, plus revisions.
    Objects,
    /// Logical structure tree.
    Tags,
    /// Content operators of one page.
    Content(PageIx),
    /// Artifact sequences by page and type.
    Artifacts,
    /// Resources by owner and category.
    Resources,
    /// Diagnostics by severity.
    Diagnostics,
}

/// Severity of a [`Badge`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    /// Informational.
    Info,
    /// Suspicious but legal.
    Warning,
    /// Violates the specification.
    Error,
}

/// A marker shown next to a tree node, e.g. a diagnostic attached to it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Badge {
    /// Machine-readable code, e.g. `orphan-mcid`.
    pub code: String,
    /// Severity.
    pub severity: Severity,
}

/// A node in a navigation tree.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TreeNode {
    /// Node identity.
    pub id: NodeId,
    /// Primary label, e.g. `P` or `12 0 obj`.
    pub label: String,
    /// Secondary text, e.g. `MCID 3` or `/Type /Page`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    /// Attached markers.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub badges: Vec<Badge>,
    /// Children in document order.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<TreeNode>,
}

impl TreeNode {
    /// Creates a leaf.
    pub fn leaf(id: NodeId, label: impl Into<String>) -> Self {
        Self {
            id,
            label: label.into(),
            detail: None,
            badges: Vec::new(),
            children: Vec::new(),
        }
    }

    /// Sets the detail text.
    #[must_use]
    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    /// Sets the children.
    #[must_use]
    pub fn with_children(mut self, children: Vec<TreeNode>) -> Self {
        self.children = children;
        self
    }

    /// Depth-first search for `id`.
    pub fn find(&self, id: &NodeId) -> Option<&TreeNode> {
        if &self.id == id {
            return Some(self);
        }
        self.children.iter().find_map(|c| c.find(id))
    }

    /// Number of nodes in this subtree, including `self`.
    pub fn len(&self) -> usize {
        1 + self.children.iter().map(TreeNode::len).sum::<usize>()
    }

    /// Always false: a tree node counts itself.
    pub fn is_empty(&self) -> bool {
        false
    }
}

impl From<pdfxray_raw::Severity> for Severity {
    fn from(s: pdfxray_raw::Severity) -> Self {
        match s {
            pdfxray_raw::Severity::Info => Self::Info,
            pdfxray_raw::Severity::Warning => Self::Warning,
            pdfxray_raw::Severity::Error => Self::Error,
        }
    }
}
