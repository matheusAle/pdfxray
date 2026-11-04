//! The generic payload layer: an element's inspector rows, compared row by row. The typed fields
//! that have a row are read off this comparison.
//!
//! The rows are those of the inspector groups that describe the element itself (`Element`,
//! `Attributes`; see `Document::tag_own_groups`), plus the text of its own marked content. Left
//! out: the groups about its content's identity (MCIDs, object references), its place in the
//! tree (parent, children: each a pair of its own) and its object; rows that link to an object;
//! and the inherited effective language, which shows on the element that carries it. The
//! subtree's text is replaced by the element's own, so a change deep in the tree belongs to the
//! element whose content changed, not to every ancestor.

use std::collections::BTreeMap;

use super::keys::ElemKey;
use super::{Field, FieldChange, RowChange};
use crate::{Document, NodeId};

/// One comparable row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Row {
    group: String,
    key: String,
    value: String,
    /// When set, rows are equal when these are, whatever their (shortened) display values.
    identity: Option<u128>,
}

/// Rows of the `Element` group that are inherited, so a change shows on the element that
/// carries it, not on every descendant.
const SKIPPED_ELEMENT: [&str; 1] = ["Effective lang"];

/// True when a row's link names a node that is not stable across rewritten files.
fn names_an_object(link: Option<&NodeId>) -> bool {
    !matches!(link, None | Some(NodeId::StructElem(_) | NodeId::Page(_)))
}

/// The comparable rows of the element's inspector groups, plus its own text.
pub(super) fn rows(doc: &Document, e: &ElemKey) -> Vec<Row> {
    let mut out = Vec::new();
    for g in &doc.tag_own_groups(e.ix) {
        for r in &g.rows {
            let skipped = g.name == "Element" && SKIPPED_ELEMENT.contains(&r.key.as_str());
            if skipped || names_an_object(r.link.as_ref()) {
                continue;
            }
            out.push(Row {
                group: g.name.clone(),
                key: r.key.clone(),
                value: r.value.clone(),
                identity: None,
            });
        }
    }
    if !e.own_text.hash.is_empty() {
        // The display value is a prefix; the comparison is on the whole text's identity.
        out.push(Row {
            group: "Content".to_owned(),
            key: "Text".to_owned(),
            value: e.own_text.display(),
            identity: Some(e.own_text.hash.bits()),
        });
    }
    out
}

/// The rows of both sides that share a group and a label.
#[derive(Default)]
struct Same<'a> {
    old: Vec<&'a Row>,
    new: Vec<&'a Row>,
}

/// The rows that differ, by group and label; rows that repeat a label are compared in order.
pub(super) fn diff(old: &[Row], new: &[Row]) -> Vec<RowChange> {
    let mut by_key: BTreeMap<(&str, &str), Same<'_>> = BTreeMap::new();
    for r in old {
        by_key.entry((&r.group, &r.key)).or_default().old.push(r);
    }
    for r in new {
        by_key.entry((&r.group, &r.key)).or_default().new.push(r);
    }
    let mut out = Vec::new();
    for ((group, key), rows) in by_key {
        for n in 0..rows.old.len().max(rows.new.len()) {
            let (before, after) = (rows.old.get(n).copied(), rows.new.get(n).copied());
            let equal = match (before, after) {
                (Some(before), Some(after)) => match (before.identity, after.identity) {
                    (Some(left), Some(right)) => left == right,
                    _ => before.value == after.value,
                },
                _ => false,
            };
            if !equal {
                out.push(RowChange {
                    group: group.to_owned(),
                    key: key.to_owned(),
                    a: before.map(|r| r.value.clone()),
                    b: after.map(|r| r.value.clone()),
                });
            }
        }
    }
    out
}

/// The typed field a row stands for, if any.
pub(super) fn field_of(group: &str, key: &str) -> Option<Field> {
    Some(match (group, key) {
        ("Element", "Type") => Field::Role,
        ("Element", "Standard type") => Field::Standard,
        ("Element", "Alt") => Field::Alt,
        ("Element", "ActualText") => Field::ActualText,
        ("Element", "Lang") => Field::Lang,
        ("Content", "Text") => Field::Text,
        _ => return None,
    })
}

/// The typed changes the row changes stand for. Role and standard type take their values from
/// the keys (the rows spell an absent one out in words); the others are the row values.
pub(super) fn typed(rows: &[RowChange], a: &ElemKey, b: &ElemKey) -> Vec<FieldChange> {
    let mut out: Vec<FieldChange> = Vec::new();
    for r in rows {
        let Some(field) = field_of(&r.group, &r.key) else {
            continue;
        };
        if out.iter().any(|c| c.field == field) {
            continue;
        }
        let (va, vb) = match field {
            Field::Role => (Some(a.declared.clone()), Some(b.declared.clone())),
            Field::Standard => (a.standard.clone(), b.standard.clone()),
            _ => (r.a.clone(), r.b.clone()),
        };
        out.push(FieldChange { field, a: va, b: vb });
    }
    out
}
