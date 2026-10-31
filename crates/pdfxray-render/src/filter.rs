//! Which operators of a page are allowed to paint.

use std::collections::BTreeSet;

use pdfxray_core::capture::CapturedPage;
use pdfxray_core::{Document, NodeId};

/// Which content operators paint in [`render_page`](crate::render_page).
///
/// Operator indices are [`OpRecord::ix`](pdfxray_core::capture::OpRecord::ix): the execution
/// index per page, including operators inside Form `XObject`s. Only *painting* operators are
/// ever suppressed (path painting, text showing, `sh`, image `Do`, inline images). Everything
/// that sets state, builds paths, moves the text cursor or nests content keeps running, so what
/// still paints looks exactly as it does in the full render.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum ContentFilter {
    /// Every operator paints: the unmodified document is rendered.
    #[default]
    All,
    /// Isolate: only these operators paint.
    Only(BTreeSet<u32>),
    /// Hide: every operator paints except these.
    Hide(BTreeSet<u32>),
    /// Paint-order stepper: operators with an index up to and including this one paint.
    UpTo(u32),
}

impl ContentFilter {
    /// True when the painting operator `ix` is allowed to paint.
    pub fn allows(&self, ix: u32) -> bool {
        match self {
            Self::All => true,
            Self::Only(set) => set.contains(&ix),
            Self::Hide(set) => !set.contains(&ix),
            Self::UpTo(last) => ix <= *last,
        }
    }
}

/// The operators that make up `id`, to build [`ContentFilter::Only`] and
/// [`ContentFilter::Hide`] sets from selected nodes.
///
/// * `Op`: the operator itself; for a `Do` that drew a Form `XObject`, also every operator the
///   form executed (including nested forms).
/// * `Glyph`: its text-showing operator. A glyph cannot be drawn alone, the whole run is.
/// * `MarkedContent`, `Artifact`: every operator from the `BDC`/`BMC` to its `EMC` (to the end
///   of the page when it is never closed) in every sequence with that MCID, or the numbered
///   artifact sequence, including nested sequences and forms invoked inside.
/// * `StructElem`: the operators of its marked content (recursively) on the first page the
///   element touches, i.e. the page [`Document::bbox`] reports; filters are per page.
/// * Anything else: empty.
///
/// The set is also empty when the page was not captured or the node does not exist.
pub fn ops_for(doc: &Document, id: &NodeId) -> BTreeSet<u32> {
    match id {
        NodeId::Op { page, ix } => doc
            .captured(*page)
            .map(|c| op_with_form(c, *ix))
            .unwrap_or_default(),
        NodeId::Glyph { page, op, .. } => doc.captured(*page).map(|c| single(c, *op)).unwrap_or_default(),
        NodeId::StructElem(_) => {
            let Some((first_page, _)) = doc.bbox(id) else {
                return BTreeSet::new();
            };
            doc.marked_content(id)
                .iter()
                .filter(|mc| matches!(mc, NodeId::MarkedContent { page, .. } if *page == first_page))
                .flat_map(|mc| ops_for(doc, mc))
                .collect()
        }
        NodeId::MarkedContent { page, mcid } => doc
            .captured(*page)
            .map(|c| {
                c.marked
                    .iter()
                    .filter(|m| m.mcid == Some(*mcid))
                    .flat_map(|m| span_of(c, m.begin_op, m.end_op))
                    .collect()
            })
            .unwrap_or_default(),
        NodeId::Artifact { page, seq } => doc
            .captured(*page)
            .and_then(|c| {
                let m = c
                    .marked
                    .iter()
                    .filter(|m| m.artifact.is_some())
                    .nth(usize::try_from(*seq).ok()?)?;
                Some(span_of(c, m.begin_op, m.end_op).collect())
            })
            .unwrap_or_default(),
        _ => BTreeSet::new(),
    }
}

fn single(c: &CapturedPage, ix: u32) -> BTreeSet<u32> {
    c.ops.iter().filter(|o| o.ix == ix).map(|o| o.ix).collect()
}

/// `ix`, plus the operators of the form it invokes when it is a `Do` that drew one.
fn op_with_form(c: &CapturedPage, ix: u32) -> BTreeSet<u32> {
    let mut set = single(c, ix);
    if let Some(scope) = c.xobjects.iter().find(|x| x.op == ix)
        && let (Some(first), Some(last)) = (scope.first_op, scope.last_op)
    {
        set.extend(first..=last);
    }
    set
}

/// Operators `begin..=end` (to the last operator when `end` is `None`) that exist on the page.
fn span_of(c: &CapturedPage, begin: u32, end: Option<u32>) -> impl Iterator<Item = u32> + '_ {
    c.ops
        .iter()
        .map(|o| o.ix)
        .filter(move |ix| *ix >= begin && end.is_none_or(|e| *ix <= e))
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    fn set(items: &[u32]) -> BTreeSet<u32> {
        items.iter().copied().collect()
    }

    #[test]
    fn each_variant_decides_by_operator_index() {
        let only = ContentFilter::Only(set(&[3, 5]));
        let hide = ContentFilter::Hide(set(&[3, 5]));
        let up_to = ContentFilter::UpTo(4);

        let verdicts = |f: &ContentFilter| (0..7).map(|ix| f.allows(ix)).collect::<Vec<_>>();

        assert_eq!(verdicts(&only), [false, false, false, true, false, true, false]);
        assert_eq!(verdicts(&hide), [true, true, true, false, true, false, true]);
        assert_eq!(verdicts(&up_to), [true, true, true, true, true, false, false]);
        // Smoke: the default allows everything.
        assert!((0..7).all(|ix| ContentFilter::default().allows(ix)));
    }
}
