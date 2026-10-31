//! How often each resource is selected by content operators, from the captured pages.
//!
//! A use is one operator of the owner's content that names the resource (`Tf`, `Do`, `gs`,
//! `cs`/`CS`, `scn`/`SCN` with a name, `sh`, `BDC`/`DP` with a name). An operator counts once
//! however many times a Form `XObject` containing it is painted. Operators inside a Form
//! `XObject` without `/Resources` resolve names in the resources of whatever painted it, so
//! they count for that painter.

use std::collections::{BTreeMap, BTreeSet, HashSet};

use crate::capture::{OpRecord, StreamRef};
use crate::{Document, NodeId, ObjRef, PageIx, ResKind};

/// Operator links shown per resource.
pub(super) const MAX_OP_LINKS: usize = 50;

/// Identifies one content stream inside an owner: `(form object, index in /Contents)`.
type StreamKey = (Option<ObjRef>, u32);

/// Operators that use resources, per `(owner, category, name)`.
#[derive(Debug, Default)]
pub(super) struct Usage {
    /// Owners whose content was captured completely, so a count of zero is meaningful.
    known: BTreeSet<ObjRef>,
    ops: BTreeMap<(ObjRef, ResKind, String), Vec<NodeId>>,
}

impl Usage {
    /// Collects uses from page `only`, or from every page when `None`.
    ///
    /// `own_forms` are the Form `XObjects` that carry `/Resources` of their own.
    pub(super) fn collect(doc: &Document, only: Option<PageIx>, own_forms: &BTreeSet<ObjRef>) -> Self {
        let mut usage = Self::default();
        let mut seen: HashSet<(ObjRef, ResKind, String, StreamKey, u32)> = HashSet::new();
        for page in &doc.pages {
            if only.is_some_and(|p| p != page.index) {
                continue;
            }
            let (Some(owner), Some(captured)) = (page.obj, doc.captured(page.index)) else {
                continue;
            };
            usage.known.insert(owner);
            for op in &captured.ops {
                let Some((kind, name)) = used_resource(op) else {
                    continue;
                };
                let scope = scope_owner(&op.stream, owner, own_forms);
                let key = (scope, kind, name, stream_key(&op.stream), op.local_ix);
                if !seen.insert(key.clone()) {
                    continue;
                }
                let (scope, kind, name, ..) = key;
                usage
                    .ops
                    .entry((scope, kind, name))
                    .or_default()
                    .push(NodeId::Op {
                        page: page.index,
                        ix: op.ix,
                    });
            }
        }
        let everything = only.is_none() && doc.pages.iter().all(|p| doc.captured(p.index).is_some());
        if everything {
            usage.known.extend(own_forms.iter().copied());
        }
        usage
    }

    /// The operators that use `name`, or `None` when the owner's content was not analysed.
    pub(super) fn ops(&self, owner: ObjRef, kind: ResKind, name: &str) -> Option<&[NodeId]> {
        if !self.known.contains(&owner) {
            return None;
        }
        Some(
            self.ops
                .get(&(owner, kind, name.to_owned()))
                .map_or(&[], Vec::as_slice),
        )
    }
}

/// Which resource category and name an operator selects, if any.
pub(super) fn used_resource(op: &OpRecord) -> Option<(ResKind, String)> {
    let (kind, operand) = match op.operator.as_str() {
        "Tf" => (ResKind::Font, op.operands.first()?),
        "Do" => (ResKind::XObject, op.operands.first()?),
        "gs" => (ResKind::ExtGState, op.operands.first()?),
        "cs" | "CS" => (ResKind::ColorSpace, op.operands.first()?),
        "sh" => (ResKind::Shading, op.operands.first()?),
        "scn" | "SCN" => (ResKind::Pattern, op.operands.last()?),
        "BDC" | "DP" => (ResKind::Properties, op.operands.get(1)?),
        _ => return None,
    };
    Some((kind, decode_name(operand)?))
}

/// `/F#201` becomes `F 1`; anything that is not a name operand gives `None`.
fn decode_name(operand: &str) -> Option<String> {
    let body = operand.strip_prefix('/')?.as_bytes();
    let mut out = Vec::with_capacity(body.len());
    let mut i = 0;
    while let Some(&b) = body.get(i) {
        let hex = (b == b'#')
            .then(|| body.get(i + 1..i + 3))
            .flatten()
            .and_then(|h| std::str::from_utf8(h).ok())
            .and_then(|h| u8::from_str_radix(h, 16).ok());
        if let Some(v) = hex {
            out.push(v);
            i += 3;
        } else {
            out.push(b);
            i += 1;
        }
    }
    Some(String::from_utf8_lossy(&out).into_owned())
}

/// The owner whose `/Resources` the stream's names resolve in: the innermost enclosing form
/// that has its own, else the page.
pub(super) fn scope_owner(stream: &StreamRef, page: ObjRef, own_forms: &BTreeSet<ObjRef>) -> ObjRef {
    let mut current = stream;
    loop {
        match current {
            StreamRef::PageContent { .. } => return page,
            StreamRef::FormXObject { obj, parent, .. } => {
                if let Some(form) = obj.filter(|o| own_forms.contains(o)) {
                    return form;
                }
                current = parent;
            }
        }
    }
}

fn stream_key(stream: &StreamRef) -> StreamKey {
    match stream {
        StreamRef::PageContent { index } => (None, *index),
        StreamRef::FormXObject { obj, .. } => (*obj, 0),
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn names_are_unescaped() {
        assert_eq!(decode_name("/F1").as_deref(), Some("F1"));
        assert_eq!(decode_name("/F#201").as_deref(), Some("F 1"));
        assert_eq!(decode_name("/A#2").as_deref(), Some("A#2"));
        assert_eq!(decode_name("12"), None);
    }
}
