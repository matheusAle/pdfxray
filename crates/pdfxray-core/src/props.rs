use serde::Serialize;

use crate::NodeId;

/// Everything the inspector shows for one node, in display order.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Props {
    /// The node described.
    pub id: NodeId,
    /// Short title, e.g. `P` or `Page 1`.
    pub title: String,
    /// Node category, e.g. `struct-elem`, `object`, `page`.
    pub kind: &'static str,
    /// Grouped key/value rows.
    pub groups: Vec<PropGroup>,
}

impl Props {
    /// Looks up a row value by group and key; handy in tests.
    pub fn get(&self, group: &str, key: &str) -> Option<&str> {
        self.groups
            .iter()
            .find(|g| g.name == group)?
            .rows
            .iter()
            .find(|r| r.key == key)
            .map(|r| r.value.as_str())
    }
}

/// A titled section of [`Props`].
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PropGroup {
    /// Section title, e.g. `Geometry`.
    pub name: String,
    /// Rows in display order.
    pub rows: Vec<PropRow>,
}

impl PropGroup {
    /// Creates an empty group.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            rows: Vec::new(),
        }
    }

    /// Appends a plain row.
    #[must_use]
    pub fn row(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.rows.push(PropRow {
            key: key.into(),
            value: value.into(),
            link: None,
        });
        self
    }

    /// Appends a row whose value navigates to another node.
    #[must_use]
    pub fn link(mut self, key: impl Into<String>, value: impl Into<String>, target: NodeId) -> Self {
        self.rows.push(PropRow {
            key: key.into(),
            value: value.into(),
            link: Some(target),
        });
        self
    }
}

/// One key/value line of [`Props`].
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PropRow {
    /// Label.
    pub key: String,
    /// Display value.
    pub value: String,
    /// Node the value refers to, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub link: Option<NodeId>,
}
