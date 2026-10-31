//! The inspector of a failed check: what the rule means in plain words, how to fix it and where
//! it failed, with Previous and Next to walk through the findings.

use egui::{Align, Frame, Layout, Margin, OpenUrl, RichText, ScrollArea, Sense, Stroke, Ui, vec2};
use pdfxray_core::numeric::usize_from_u32;
use pdfxray_core::{Document, NodeId, ValidationFinding};

use super::{Action, header};
use crate::components::{Icon, PropRow, ToolButton, context_menu, section, tooltip};
use crate::findings;
use crate::state::{AppState, RailItem};
use crate::theme::{fonts, tokens};

/// Name of the button that selects the previous finding.
pub const PREVIOUS_LABEL: &str = "Previous";

/// Name of the button that selects the next finding.
pub const NEXT_LABEL: &str = "Next finding";

/// Name of the button that opens the rule's page in the browser.
pub const RULE_DOCS_LABEL: &str = "Rule docs";

/// Title of the section about why the rule exists.
pub const WHY_SECTION: &str = "Why it matters";

/// Title of the section about how to satisfy the rule.
pub const HOW_SECTION: &str = "How to fix";

/// Title of the section about where the check failed.
pub const WHERE_SECTION: &str = "Where";

/// Title of the section with veraPDF's own words.
pub const VERAPDF_SECTION: &str = "veraPDF says";

/// Said when pdfxray has no note of its own for the rule.
pub const NO_NOTE: &str = "pdfxray has no plain-language note for this rule yet. veraPDF's wording is below.";

/// The line under the example of a fix.
pub const EXAMPLE_NOTE: &str = "Example only. pdfxray never changes your file.";

/// What the example adds to a Figure that has no alternative text.
pub const ALT_EXAMPLE: &str = "/Alt (Describe what the figure shows)";

/// The rule of a Figure without alternative text, for which an example fix is known.
const FIGURE_ALT_RULE: &str = "7.3-1";

/// Longest source shown in the code card, in lines.
const MAX_CODE_LINES: usize = 14;

/// The finding the inspector shows for `selection`; only the Validation entry shows findings.
pub(super) fn finding_of(state: &AppState, selection: &NodeId) -> Option<u32> {
    if state.nav.rail != RailItem::Validation {
        return None;
    }
    state.finding_for(selection)
}

/// One line of the code card.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeLine {
    /// The text, indented as the object view does.
    pub text: String,
    /// True for the line the example adds.
    pub added: bool,
}

/// The source of the object a finding is about, with the example fix when one is known: a Figure
/// without `/Alt` gets an `/Alt` line. `None` when the finding is not about an indirect object.
pub fn code_sample(doc: &Document, finding: &ValidationFinding) -> Option<Vec<CodeLine>> {
    let target = finding.target.as_ref()?;
    let (NodeId::StructElem(r) | NodeId::Object(r)) = target else {
        return None;
    };
    let props = doc.node(&NodeId::Object(*r))?;
    let entries: Vec<(&str, &str)> = props
        .groups
        .iter()
        .find(|group| group.name == "Value")?
        .rows
        .iter()
        .map(|row| (row.key.as_str(), row.value.as_str()))
        .collect();
    let line = |text: String, added: bool| CodeLine { text, added };
    let mut lines = vec![
        line(format!("{} {} obj", r.num, r.generation), false),
        line("<<".to_owned(), false),
    ];
    for (key, value) in entries.iter().take(MAX_CODE_LINES) {
        lines.push(line(format!("  {key} {value}"), false));
    }
    if entries.len() > MAX_CODE_LINES {
        lines.push(line("  …".to_owned(), false));
    }
    let wants_alt = findings::short_rule(&finding.rule) == FIGURE_ALT_RULE
        && !entries.iter().any(|(key, _)| *key == "/Alt");
    if wants_alt {
        lines.push(line(format!("  {ALT_EXAMPLE}"), true));
    }
    lines.push(line(">>".to_owned(), false));
    lines.push(line("endobj".to_owned(), false));
    Some(lines)
}

/// `…/SEFigure[0]`: the end of veraPDF's path, which is the part that names the node.
pub fn short_context(context: &str) -> String {
    let mut parts: Vec<&str> = context.split('/').collect();
    if parts.len() <= 2 {
        return context.to_owned();
    }
    let last = parts.pop().unwrap_or_default();
    format!(".../{last}")
}

/// `MCID 9 · Do /Im1 · page 1`, for the finding of marked content or an operator.
pub fn content_line(doc: &Document, target: &NodeId) -> Option<String> {
    match target {
        NodeId::MarkedContent { page, mcid } => {
            let captured = doc.captured(*page)?;
            let seq = captured.marked.iter().find(|m| m.mcid == Some(*mcid))?;
            let end = seq.end_op.unwrap_or(u32::MAX);
            let painter = captured
                .ops
                .iter()
                .skip(usize_from_u32(seq.begin_op) + 1)
                .take_while(|op| op.ix < end)
                .find(|op| op.item_count > 0);
            let operator = painter.map_or_else(
                || "no painting operator".to_owned(),
                |op| match (op.operator.as_str(), op.operands.first()) {
                    ("Do", Some(name)) => format!("Do {name}"),
                    (operator, _) => operator.to_owned(),
                },
            );
            Some(format!("MCID {mcid} · {operator} · page {}", page + 1))
        }
        NodeId::Op { page, ix } => {
            let op = doc
                .captured(*page)?
                .ops
                .get(usize_from_u32(*ix))
                .filter(|op| op.ix == *ix)?;
            Some(format!("op #{ix} {} · page {}", op.operator, page + 1))
        }
        _ => None,
    }
}

/// Draws the inspector for finding `index`.
pub(super) fn ui(ui: &mut Ui, state: &mut AppState, index: u32, selection: &NodeId, action: &mut Action) {
    let Some(shown) = Shown::of(state, index) else {
        return;
    };
    header::crumbs(
        ui,
        state,
        shown.finding.target.as_ref().unwrap_or(selection),
        action,
    );
    let title = title(ui, state, &shown);
    if title.pin_pressed {
        state.selection.pinned = state.selection.pinned.is_none().then(|| selection.clone());
    }
    context_menu::attach(&title.area, state, selection);
    ScrollArea::vertical()
        .id_salt("inspector-finding")
        .auto_shrink(false)
        .show(ui, |ui| {
            let Some(loaded) = &state.document else {
                return;
            };
            let doc = &loaded.doc;
            // Rows are told apart per finding: a value flashes when it changes, not when the
            // next finding replaces the whole page.
            ui.push_id(index, |ui| {
                match shown.finding.explain() {
                    Some(text) => {
                        section::show(ui, "finding-why", WHY_SECTION, |ui| body_text(ui, text.why));
                        section::show(ui, "finding-how", HOW_SECTION, |ui| {
                            body_text(ui, text.how);
                            if !text.exact {
                                super::note(
                                    ui,
                                    RichText::new(format!(
                                        "General guidance for clause {}",
                                        findings::clause(&shown.finding.rule)
                                    ))
                                    .font(fonts::ui(tokens::text::SMALL))
                                    .color(tokens::TEXT_3),
                                );
                            }
                            if let Some(lines) = code_sample(doc, &shown.finding) {
                                code_card(ui, &lines);
                            }
                        });
                    }
                    None => {
                        section::show(ui, "finding-why", "What failed", |ui| {
                            body_text(ui, NO_NOTE);
                            if let Some(lines) = code_sample(doc, &shown.finding) {
                                code_card(ui, &lines);
                            }
                        });
                    }
                }
                section::show(ui, "finding-where", WHERE_SECTION, |ui| {
                    where_rows(ui, state, doc, &shown.finding, action);
                });
                section::show(ui, "finding-verapdf", VERAPDF_SECTION, |ui| {
                    PropRow::new("Requirement", &shown.finding.description).show(ui);
                    if let Some(message) = &shown.finding.message {
                        PropRow::new("Message", message).show(ui);
                    }
                });
            });
        });
}

/// What the inspector needs of a finding, copied so the state can change while it is drawn.
struct Shown {
    finding: ValidationFinding,
    heading: String,
    line: String,
}

impl Shown {
    fn of(state: &AppState, index: u32) -> Option<Self> {
        let run = state.document.as_ref()?.doc.validation()?;
        let finding = run.findings.get(usize_from_u32(index))?.clone();
        let same: Vec<u32> = findings::groups(run)
            .into_iter()
            .find(|group| group.rule == finding.rule)
            .map(|group| group.findings)
            .unwrap_or_default();
        let place = same.iter().position(|i| *i == index).map_or(1, |p| p + 1);
        let profile = state
            .validation
            .last
            .as_ref()
            .map_or_else(|| run.profile.clone(), |last| last.profile.label());
        Some(Self {
            heading: findings::title_of(&finding),
            line: format!(
                "{profile} · rule {} · {place} of {}",
                findings::short_rule(&finding.rule),
                same.len()
            ),
            finding,
        })
    }
}

/// The fail mark, the rule's title, the line under it and the pin.
fn title(ui: &mut Ui, state: &AppState, shown: &Shown) -> header::TitleOutput {
    header::block(
        ui,
        |ui| {
            let (rect, _) = ui.allocate_exact_size(vec2(16.0, 16.0), Sense::hover());
            findings::fail_mark(ui.painter(), rect, tokens::ERR);
        },
        &shown.heading,
        &shown.line,
        Some(state.selection.pinned.is_some()),
    )
}

fn body_text(ui: &mut Ui, text: &str) {
    // Indented like the property rows around it.
    Frame::new().inner_margin(Margin::symmetric(8, 4)).show(ui, |ui| {
        ui.label(
            RichText::new(text)
                .font(fonts::ui(tokens::text::LARGE))
                .color(tokens::TEXT_2),
        );
    });
}

/// The object's source; the line an example fix adds is green with a plus in front.
fn code_card(ui: &mut Ui, lines: &[CodeLine]) {
    ui.add_space(4.0);
    Frame::new()
        .outer_margin(Margin::symmetric(8, 0))
        .fill(tokens::BG)
        .stroke(Stroke::new(1.0, tokens::BORDER))
        .corner_radius(tokens::RADIUS_MD)
        .inner_margin(Margin::symmetric(10, 8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 1.0;
            for line in lines {
                let (text, color) = if line.added {
                    (format!("+{}", line.text), tokens::OK)
                } else {
                    (format!(" {}", line.text), tokens::TEXT_2)
                };
                let label = egui::Label::new(
                    RichText::new(text)
                        .font(fonts::mono(tokens::mono::BODY))
                        .color(color),
                )
                .truncate();
                if line.added {
                    Frame::new().fill(tokens::OK.gamma_multiply(0.12)).show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.add(label);
                    });
                } else {
                    ui.add(label);
                }
            }
        });
    if lines.iter().any(|line| line.added) {
        Frame::new().inner_margin(Margin::symmetric(8, 2)).show(ui, |ui| {
            ui.label(
                RichText::new(EXAMPLE_NOTE)
                    .font(fonts::ui(tokens::text::SMALL))
                    .color(tokens::TEXT_3),
            );
        });
    }
}

fn where_rows(
    ui: &mut Ui,
    state: &AppState,
    doc: &Document,
    finding: &ValidationFinding,
    action: &mut Action,
) {
    if let Some(target) = &finding.target {
        let element = match target {
            NodeId::StructElem(_) => Some(target.clone()),
            NodeId::MarkedContent { page, mcid } => state
                .tag_index()
                .and_then(|tags| tags.owner_of(*page, *mcid))
                .cloned(),
            _ => None,
        };
        if let Some(element) = element {
            let label = element_label(doc, &element);
            if PropRow::new("Element", &label)
                .link(true)
                .show(ui)
                .value
                .clicked()
            {
                action.navigate = Some(element);
            }
        }
        if let Some(content) = content_line(doc, target) {
            PropRow::new("Content", &content).show(ui);
        } else if let Some(page) = findings::page_of(doc, finding) {
            PropRow::new("Page", &(page + 1).to_string()).show(ui);
        }
    }
    PropRow::new("veraPDF path", &short_context(&finding.context))
        .copy_text(&finding.context)
        .show(ui);
}

/// `/Figure 22 0 R`: the role of a structure element and its object.
fn element_label(doc: &Document, element: &NodeId) -> String {
    let NodeId::StructElem(r) = element else {
        return element.to_string();
    };
    let role = doc.node(element).map_or_else(String::new, |props| props.title);
    format!("/{role} {} {} R", r.num, r.generation)
}

/// Previous, Rule docs and Next finding.
pub(super) fn footer(ui: &mut Ui, state: &mut AppState, index: u32) {
    let rule = state
        .document
        .as_ref()
        .and_then(|loaded| loaded.doc.validation())
        .and_then(|run| run.findings.get(usize_from_u32(index)))
        .map(|finding| finding.rule.clone())
        .unwrap_or_default();
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        let previous = ToolButton::new(PREVIOUS_LABEL).icon(Icon::ChevronLeft).show(ui);
        if previous.clicked() {
            state.step_finding(-1);
        }
        tooltip::with_shortcut(previous, "Previous finding", "⌘[");
        let docs = ToolButton::new(RULE_DOCS_LABEL).icon(Icon::Help).show(ui);
        if docs.clicked() {
            ui.ctx()
                .open_url(OpenUrl::new_tab(findings::rule_docs_url(&rule)));
        }
        tooltip::text(docs, "Open the rules of this standard in your browser");
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let next = ToolButton::new(NEXT_LABEL)
                .icon(Icon::ChevronRight)
                .primary()
                .show(ui);
            if next.clicked() {
                state.step_finding(1);
            }
            tooltip::with_shortcut(next, "Next finding", "⌘]");
        });
    });
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn the_context_is_cut_to_the_part_that_names_the_node() {
        assert_eq!(
            short_context("root/document[0]/StructTreeRoot[0](8 0 obj)/K[0]/K[1](14 0 obj SEFigure Figure)"),
            ".../K[1](14 0 obj SEFigure Figure)"
        );
        assert_eq!(short_context("root"), "root");
        assert_eq!(short_context("root/document[0]"), "root/document[0]");
    }
}
