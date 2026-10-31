//! The Content and Artifacts entries: the content tree of the page on the canvas, with the page's
//! coverage under it. Artifacts is the same tree narrowed to `/Artifact` sequences.
//!
//! Rows show the operator, its operands and `#index`, and at the right what matters about it:
//! `MCID 0`, `Pagination`, `18 glyphs`, `3 Tr · 0.2 ca`, `untagged`.

use std::collections::{HashMap, HashSet};

use egui::{Frame, Margin, Panel, RichText, Sense, Ui, vec2};
use pdfxray_core::capture::{CapturedPage, MarkedSeq, OpRecord, PaintKind};
use pdfxray_core::numeric::{f32_from_u32, usize_from_u32};
use pdfxray_core::{Document, NodeId, PageIx, TreeKind, TreeNode, num};

use super::tree_body;
use crate::components::{Annotation, Tone};
use crate::state::derived::Derived;
use crate::state::{AppState, NavTree, RailItem};
use crate::theme::{fonts, tokens};
use crate::tree_view::{Decor, TreeModel};

/// Shown when the page has no content operators.
pub const NO_CONTENT: &str = "This page has no content.";

/// Shown when the page has no artifacts.
pub const NO_ARTIFACTS: &str = "This page has no artifacts.";

/// Text of the annotation on painting operators outside any marked content.
pub const UNTAGGED: &str = "untagged";

/// Longest text quoted in a row.
const QUOTE_CHARS: usize = 30;

/// Longest operand list shown in a row.
const OPERAND_CHARS: usize = 28;

/// Operators that show text.
fn shows_text(operator: &str) -> bool {
    matches!(operator, "Tj" | "TJ" | "'" | "\"")
}

/// What the rows of one page's tree need to know about its operators.
struct PageFacts<'a> {
    captured: &'a CapturedPage,
    sequence_at: HashMap<u32, &'a MarkedSeq>,
    form_calls: HashSet<u32>,
    untagged: HashSet<u32>,
}

impl<'a> PageFacts<'a> {
    fn new(captured: &'a CapturedPage, untagged: &[u32]) -> Self {
        Self {
            captured,
            sequence_at: captured.marked.iter().map(|m| (m.begin_op, m)).collect(),
            form_calls: captured.xobjects.iter().map(|x| x.op).collect(),
            untagged: untagged.iter().copied().collect(),
        }
    }

    fn op(&self, ix: u32) -> Option<&'a OpRecord> {
        self.captured.ops.get(usize_from_u32(ix)).filter(|op| op.ix == ix)
    }

    /// The text a text operator painted, shortened.
    fn quoted(&self, op: &OpRecord) -> Option<String> {
        let first = usize::try_from(op.first_item?).ok()?;
        let text: String = self
            .captured
            .items
            .iter()
            .skip(first)
            .take(usize_from_u32(op.item_count))
            .filter_map(|item| match &item.kind {
                PaintKind::Glyph { unicode, .. } => unicode.as_deref(),
                _ => None,
            })
            .collect();
        (!text.is_empty()).then(|| format!("\"{}\"", shorten(&text, QUOTE_CHARS)))
    }

    fn glyphs(&self, op: &OpRecord) -> usize {
        let Some(first) = op.first_item.and_then(|f| usize::try_from(f).ok()) else {
            return 0;
        };
        self.captured
            .items
            .iter()
            .skip(first)
            .take(usize_from_u32(op.item_count))
            .filter(|item| matches!(item.kind, PaintKind::Glyph { .. }))
            .count()
    }

    fn decor(&self, op: &OpRecord) -> Decor {
        let ix = op.ix;
        let operands = op.operands.join(" ");
        let sequence = self.sequence_at.get(&ix).copied();
        let paints = op.item_count > 0;
        let untagged = paints && self.untagged.contains(&ix);
        let mut decor = Decor {
            mono_label: true,
            ..Decor::default()
        };

        let (label, detail) = match (op.operator.as_str(), sequence) {
            ("BDC" | "BMC", Some(_)) => (
                Some(format!(
                    "{} {}",
                    op.operands.first().map_or("", String::as_str),
                    op.operator
                )),
                format!("#{ix}"),
            ),
            (operator, _) if shows_text(operator) => {
                let text = self
                    .quoted(op)
                    .unwrap_or_else(|| shorten(&operands, OPERAND_CHARS));
                (None, format!("{text} #{ix}"))
            }
            _ if operands.is_empty() => (None, format!("#{ix}")),
            _ => (None, format!("{} #{ix}", shorten(&operands, OPERAND_CHARS))),
        };
        decor.label = label;
        decor.detail = Some(detail);

        if let Some(seq) = sequence {
            if let Some(artifact) = &seq.artifact {
                let name = artifact
                    .subtype
                    .clone()
                    .or_else(|| artifact.kind.clone())
                    .unwrap_or_else(|| "Artifact".to_owned());
                decor.right.push(Annotation::Text(name));
                decor.dot = Some(tokens::KIND_ARTIFACT);
            } else if let Some(mcid) = seq.mcid {
                decor.right.push(Annotation::Text(format!("MCID {mcid}")));
                decor.dot = Some(tokens::KIND_TAG);
            }
        } else if op.operator == "Do" && self.form_calls.contains(&ix) {
            decor.right.push(Annotation::Text("Form".to_owned()));
        } else if shows_text(&op.operator) {
            decor
                .right
                .push(Annotation::Text(Self::state_hints(op).unwrap_or_else(|| {
                    let n = self.glyphs(op);
                    format!("{n} glyph{}", if n == 1 { "" } else { "s" })
                })));
        }
        if untagged {
            decor
                .right
                .push(Annotation::Toned(UNTAGGED.to_owned(), Tone::Err));
            decor.dot = Some(tokens::ERR);
        } else if decor.dot.is_none() {
            decor.dot = Some(if paints {
                tokens::KIND_CONTENT
            } else {
                tokens::TEXT_3
            });
        }
        decor
    }

    /// `3 Tr · 0.2 ca`: a text state that makes the text look different from plain fill.
    fn state_hints(op: &OpRecord) -> Option<String> {
        let mut hints = Vec::new();
        if op.gstate.text.render_mode != 0 {
            hints.push(format!("{} Tr", op.gstate.text.render_mode));
        }
        if op.gstate.alpha_fill < 1.0 {
            hints.push(format!("{} ca", num(op.gstate.alpha_fill)));
        }
        (!hints.is_empty()).then(|| hints.join(" · "))
    }
}

/// `text` cut to `max` characters with an ellipsis.
fn shorten(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_owned();
    }
    let mut cut: String = text.chars().take(max.saturating_sub(1)).collect();
    cut.push('…');
    cut
}

/// The content tree of `page` as a model.
pub(crate) fn model(doc: &Document, derived: &mut Derived, page: PageIx) -> TreeModel {
    let roots = doc.tree(TreeKind::Content(page));
    let untagged = derived
        .page(doc, page)
        .map(|stats| stats.coverage.untagged_ops.clone())
        .unwrap_or_default();
    let Some(captured) = doc.captured(page) else {
        return TreeModel::new(roots);
    };
    let facts = PageFacts::new(captured, &untagged);
    TreeModel::with_decor(roots, |node: &TreeNode| match &node.id {
        NodeId::Op { ix, .. } => facts.op(*ix).map(|op| facts.decor(op)).unwrap_or_default(),
        _ => Decor::default(),
    })
}

/// The text after the header title: `377 ops · 6 streams`.
pub(super) fn stats_text(state: &mut AppState) -> Option<String> {
    let page = state.canvas.page;
    let (ops, streams) = state.page_stats(page).map(|s| (s.ops, s.streams))?;
    Some(format!(
        "{ops} op{} · {streams} stream{}",
        if ops == 1 { "" } else { "s" },
        if streams == 1 { "" } else { "s" }
    ))
}

pub(super) fn ui(ui: &mut Ui, state: &mut AppState) {
    footer(ui, state);
    let tree = NavTree::Core(TreeKind::Content(state.canvas.page));
    if state.nav.rail == RailItem::Artifacts {
        tree_body(ui, state, tree, &["/Artifact"], NO_ARTIFACTS);
    } else {
        tree_body(ui, state, tree, &[], NO_CONTENT);
    }
}

/// `Page 1 coverage`: how much of what the page paints is tagged, artifact or untagged.
fn footer(ui: &mut Ui, state: &mut AppState) {
    let page = state.canvas.page;
    let Some((tagged, artifact, untagged)) = state
        .page_stats(page)
        .map(|s| (s.coverage.tagged, s.coverage.artifact, s.coverage.untagged))
    else {
        return;
    };
    Panel::bottom("content_coverage")
        .resizable(false)
        .frame(Frame::new().inner_margin(Margin::symmetric(0, 8)))
        .show(ui, |ui| {
            ui.add_space(6.0);
            ui.label(
                RichText::new(format!("Page {} coverage", page + 1))
                    .font(fonts::semibold(tokens::text::SMALL))
                    .color(tokens::TEXT_2),
            );
            bar(ui, [tagged, artifact, untagged]);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                for (name, count, color) in [
                    ("Tagged", tagged, tokens::KIND_TAG),
                    ("Artifact", artifact, tokens::KIND_ARTIFACT),
                    ("Untagged", untagged, tokens::ERR),
                ] {
                    let (dot, _) = ui.allocate_exact_size(vec2(8.0, 8.0), Sense::hover());
                    ui.painter().rect_filled(dot, 2.0, color);
                    ui.label(
                        RichText::new(format!("{name} {count}"))
                            .font(fonts::mono(tokens::mono::SMALL))
                            .color(tokens::TEXT_2),
                    );
                    ui.add_space(4.0);
                }
            });
        });
}

/// The proportions of tagged, artifact and untagged items as a bar.
fn bar(ui: &mut Ui, counts: [u32; 3]) {
    let colors = [tokens::KIND_TAG, tokens::KIND_ARTIFACT, tokens::ERR];
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 6.0), Sense::hover());
    ui.painter().rect_filled(rect, BAR_RADIUS, tokens::SURFACE_3);
    let total: f32 = counts.iter().map(|&c| f32_from_u32(c)).sum();
    if total <= 0.0 {
        return;
    }
    // The ends of the bar are as round as its track, so the first and the last part wear them.
    let used: Vec<usize> = (0..counts.len()).filter(|&i| counts[i] > 0).collect();
    let mut x = rect.min.x;
    for (at, (count, color)) in counts.into_iter().zip(colors).enumerate() {
        let width = rect.width() * f32_from_u32(count) / total;
        if width > 0.0 {
            let part = egui::Rect::from_min_size(egui::pos2(x, rect.min.y), vec2(width, rect.height()));
            let (first, last) = (used.first() == Some(&at), used.last() == Some(&at));
            let round = |on: bool| if on { BAR_RADIUS } else { 0 };
            let corners = egui::CornerRadius {
                nw: round(first),
                sw: round(first),
                ne: round(last),
                se: round(last),
            };
            ui.painter().rect_filled(part, corners, color);
            x += width;
        }
    }
}

/// Corner radius of the coverage bar, in points.
const BAR_RADIUS: u8 = 3;
