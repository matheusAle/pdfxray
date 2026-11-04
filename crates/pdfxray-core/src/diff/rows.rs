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
//!
//! A row that holds a page-space box (the Layout `/BBox` attribute) is compared the way the
//! element's own box is: after the global offset is removed, to [`TOLERANCE`]. Every file
//! translated as a whole would otherwise report each figure's `/BBox` as changed.

use std::collections::BTreeMap;

use super::keys::ElemKey;
use super::{Field, FieldChange, RowChange, TOLERANCE};
use crate::{Document, NodeId, Rect};

/// One comparable row.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Row {
    group: String,
    key: String,
    value: String,
    /// When set, rows are equal when these are, whatever their (shortened) display values.
    identity: Option<u128>,
    /// For a row holding a page-space box: the box and the rest of the value after it.
    place: Option<(Rect, String)>,
}

/// Attribute rows whose value is a box in page space.
const PAGE_BOXES: [&str; 1] = ["Layout: BBox"];

/// `[x0 y0 x1 y1]` at the start of `value`, and what follows it.
fn page_box(value: &str) -> Option<(Rect, String)> {
    let (inside, rest) = value.strip_prefix('[')?.split_once(']')?;
    let v: Vec<f64> = inside
        .split_whitespace()
        .map(str::parse)
        .collect::<Result<_, _>>()
        .ok()?;
    let &[x0, y0, x1, y1] = v.as_slice() else {
        return None;
    };
    Some((Rect::new(x0, y0, x1, y1), rest.to_owned()))
}

/// True when `b`, with the offset removed, is within [`TOLERANCE`] of `a` on every edge.
fn same_place(a: &(Rect, String), b: &(Rect, String), (dx, dy): (f64, f64)) -> bool {
    let (ra, rb) = (a.0, b.0);
    let edges = [
        rb.x0 - dx - ra.x0,
        rb.y0 - dy - ra.y0,
        rb.x1 - dx - ra.x1,
        rb.y1 - dy - ra.y1,
    ];
    a.1 == b.1 && edges.iter().all(|e| e.abs() <= TOLERANCE)
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
            let place = (g.name == "Attributes" && PAGE_BOXES.contains(&r.key.as_str()))
                .then(|| page_box(&r.value))
                .flatten();
            out.push(Row {
                group: g.name.clone(),
                key: r.key.clone(),
                value: r.value.clone(),
                identity: None,
                place,
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
            place: None,
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
/// `offset` is the global translation of `new` relative to `old`.
pub(super) fn diff(old: &[Row], new: &[Row], offset: (f64, f64)) -> Vec<RowChange> {
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
                (Some(before), Some(after)) => {
                    match (&before.identity, &after.identity, &before.place, &after.place) {
                        (Some(left), Some(right), _, _) => left == right,
                        (_, _, Some(left), Some(right)) => same_place(left, right, offset),
                        _ => before.value == after.value,
                    }
                }
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

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    fn bbox_row(value: &str) -> Row {
        Row {
            group: "Attributes".to_owned(),
            key: PAGE_BOXES[0].to_owned(),
            value: value.to_owned(),
            identity: None,
            place: page_box(value),
        }
    }

    #[test]
    fn a_page_box_row_is_compared_after_the_offset_is_removed() {
        let (a, b) = (bbox_row("[30 148 150 230]"), bbox_row("[32 146 152 228]"));
        assert!(a.place.is_some() && b.place.is_some(), "both parse");
        assert_eq!(
            diff(std::slice::from_ref(&a), std::slice::from_ref(&b), (2.0, -2.0)),
            vec![]
        );
        // Without the offset, the same rows differ, and the change carries both values.
        let changed = diff(&[a], &[b], (0.0, 0.0));
        assert_eq!(changed.len(), 1);
        assert_eq!(
            (changed[0].a.as_deref(), changed[0].b.as_deref()),
            (Some("[30 148 150 230]"), Some("[32 146 152 228]"))
        );
    }

    #[test]
    fn a_page_box_from_another_class_is_a_change_even_in_place() {
        let a = bbox_row("[0 0 10 10] (from class A)");
        let b = bbox_row("[0 0 10 10] (from class B)");
        assert_eq!(diff(&[a], &[b], (0.0, 0.0)).len(), 1);
        assert_eq!(page_box("[1 2 3]"), None);
        assert_eq!(page_box("not a box"), None);
        assert_eq!(
            page_box("[1 2 3 4]").map(|(r, rest)| (r.x1, rest)),
            Some((3.0, String::new()))
        );
    }
}
