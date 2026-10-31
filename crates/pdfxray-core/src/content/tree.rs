//! Navigation trees of the content: operators (nested) and artifact sequences.

use super::{PageData, ScopeChain};
use crate::capture::{Alignment, ArtifactInfo, MarkedSeq, OpRecord};
use crate::{NodeId, PageIx, TreeNode, fmt_rect};

/// Longest detail text shown in a tree row.
const DETAIL_CHARS: usize = 80;

/// A container node still collecting children.
struct Open {
    /// Which container this is.
    frame: Frame,
    /// The Form `XObject` scope the container lives in: it stays open while the running
    /// operator is inside that scope.
    needs: Option<u32>,
    node: TreeNode,
}

#[derive(PartialEq, Eq)]
enum Frame {
    /// A marked-content sequence (index into `CapturedPage::marked`).
    Mc(u32),
    /// A Form `XObject` drawn by the container's `Do`.
    Form,
}

/// The operators of a page, nested by marked-content sequences and Form `XObject`s.
pub(super) fn content_tree(page: PageIx, data: &PageData) -> Vec<TreeNode> {
    let mut root = Vec::new();
    if let Alignment::Degraded { reason } = &data.cap.alignment {
        root.push(TreeNode::leaf(NodeId::Page(page), "Capture degraded").with_detail(reason.clone()));
    }
    let mut stack: Vec<Open> = Vec::new();
    let mut scopes = ScopeChain::default();
    for op in &data.cap.ops {
        let chain = scopes.of(&data.cap, op.xobject);
        // Containers whose form is no longer running are finished, with everything above them.
        let stale = stack
            .iter()
            .position(|o| o.needs.is_some_and(|scope| !chain.contains(&scope)));
        if let Some(at) = stale {
            close_to(&mut root, &mut stack, at);
        }
        let node = op_node(page, op, data);
        if let Some(&seq) = data.begin_of.get(&op.ix) {
            stack.push(Open {
                frame: Frame::Mc(seq),
                needs: op.xobject,
                node,
            });
        } else if let Some(&scope) = data.scope_of.get(&op.ix) {
            stack.push(Open {
                frame: Frame::Form,
                needs: Some(scope),
                node,
            });
        } else if let Some(at) = data
            .end_of
            .get(&op.ix)
            .and_then(|&seq| stack.iter().position(|o| o.frame == Frame::Mc(seq)))
        {
            close_to(&mut root, &mut stack, at + 1);
            if let Some(top) = stack.last_mut() {
                top.node.children.push(node);
            }
            close_to(&mut root, &mut stack, at);
        } else {
            attach(&mut root, &mut stack, node);
        }
    }
    close_to(&mut root, &mut stack, 0);
    root
}

/// Closes containers from the top until only `keep` remain.
fn close_to(root: &mut Vec<TreeNode>, stack: &mut Vec<Open>, keep: usize) {
    while stack.len() > keep {
        if let Some(open) = stack.pop() {
            attach(root, stack, open.node);
        }
    }
}

/// Adds `node` as the last child of the innermost open container, or to the root.
fn attach(root: &mut Vec<TreeNode>, stack: &mut [Open], node: TreeNode) {
    match stack.last_mut() {
        Some(parent) => parent.node.children.push(node),
        None => root.push(node),
    }
}

/// The tree row of one operator.
fn op_node(page: PageIx, op: &OpRecord, data: &PageData) -> TreeNode {
    let operands = op.operands.join(" ");
    let detail = match data.begin_of.get(&op.ix).and_then(|&s| data.seq(s)) {
        // A `BMC` operand is the tag, which the summary already shows.
        Some(seq) if op.operands.len() <= 1 => seq_summary(seq),
        Some(seq) => format!("{} \u{b7} {operands}", seq_summary(seq)),
        None => operands,
    };
    let node = TreeNode::leaf(NodeId::Op { page, ix: op.ix }, op.operator.clone());
    if detail.is_empty() {
        node
    } else {
        node.with_detail(truncate(&detail, DETAIL_CHARS))
    }
}

/// `P MCID 3`, `Artifact Pagination Header`, or just the tag.
pub(super) fn seq_summary(seq: &MarkedSeq) -> String {
    if let Some(info) = &seq.artifact {
        return format!("Artifact{}", artifact_suffix(info));
    }
    match seq.mcid {
        Some(mcid) => format!("{} MCID {mcid}", seq.tag),
        None => seq.tag.clone(),
    }
}

/// ` /Pagination /Header`, empty for a bare artifact.
pub(super) fn artifact_suffix(info: &ArtifactInfo) -> String {
    let names: Vec<String> = [&info.kind, &info.subtype]
        .into_iter()
        .flatten()
        .map(|n| format!(" /{n}"))
        .collect();
    names.concat()
}

/// Cuts `text` to at most `max` characters, ending with an ellipsis when it was longer.
pub(super) fn truncate(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_owned();
    }
    let mut cut: String = text.chars().take(max.saturating_sub(1)).collect();
    cut.push('\u{2026}');
    cut
}

/// The group node of one page's artifacts, `None` when it has none.
pub(super) fn artifact_group(page: PageIx, data: &PageData) -> Option<TreeNode> {
    let children: Vec<TreeNode> = data
        .artifacts
        .iter()
        .zip(0u32..)
        .filter_map(|(&s, number)| {
            let seq = data.seq(s)?;
            let info = seq.artifact.as_ref()?;
            let node = TreeNode::leaf(
                NodeId::Artifact { page, seq: number },
                format!("Artifact{}", artifact_suffix(info)),
            );
            Some(match seq.bbox.or(info.bbox) {
                Some(bbox) => node.with_detail(fmt_rect(bbox)),
                None => node,
            })
        })
        .collect();
    if children.is_empty() {
        return None;
    }
    Some(TreeNode::leaf(NodeId::Page(page), format!("Page {}", page + 1)).with_children(children))
}
