//! The failed checks of a validation run, arranged for the screens that show them: grouped by
//! rule (navigator), in that same order as a table (drawer), as badges on the page (canvas) and
//! one by one (inspector, Previous and Next).
//!
//! Everything here is derived from [`ValidationRun`] and the document; nothing is stored.

use egui::{Color32, Painter, Pos2, Rect as UiRect, Stroke, vec2};
use pdfxray_core::numeric::usize_from_u32;
use pdfxray_core::{Document, NodeId, PageIx, Rect, ValidationFinding, ValidationRun};

use crate::selection::{node_page, selection_rect};
use crate::state::AppState;
use crate::theme::tokens;

/// Findings of one rule, in report order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleGroup {
    /// The full rule id, e.g. `ISO 14289-1:2014 7.3-1`.
    pub rule: String,
    /// What follows the standard: `7.3-1`.
    pub short: String,
    /// The plain title, or veraPDF's requirement when pdfxray has no note for the rule.
    pub title: String,
    /// Indices into [`ValidationRun::findings`].
    pub findings: Vec<u32>,
}

/// `7.3-1` of `ISO 14289-1:2014 7.3-1`.
pub fn short_rule(rule: &str) -> &str {
    rule.rsplit_once(' ').map_or(rule, |(_, id)| id)
}

/// `7.3` of `ISO 14289-1:2014 7.3-1`: the clause the rule belongs to.
pub fn clause(rule: &str) -> &str {
    let id = short_rule(rule);
    id.rsplit_once('-').map_or(id, |(clause, _)| clause)
}

/// The plain-language title of a finding's rule, or veraPDF's requirement without one.
pub fn title_of(finding: &ValidationFinding) -> String {
    finding
        .explain()
        .map_or_else(|| finding.description.clone(), |text| text.title.to_owned())
}

/// The findings grouped by rule, in the order the rules first fail.
pub fn groups(run: &ValidationRun) -> Vec<RuleGroup> {
    let mut groups: Vec<RuleGroup> = Vec::new();
    for (index, finding) in (0u32..).zip(&run.findings) {
        match groups.iter_mut().find(|g| g.rule == finding.rule) {
            Some(group) => group.findings.push(index),
            None => groups.push(RuleGroup {
                rule: finding.rule.clone(),
                short: short_rule(&finding.rule).to_owned(),
                title: title_of(finding),
                findings: vec![index],
            }),
        }
    }
    groups
}

/// All finding indices in the order the navigator lists them: by rule, then report order. Previous
/// and Next walk this order, and the drawer table uses it.
pub fn order(run: &ValidationRun) -> Vec<u32> {
    groups(run).into_iter().flat_map(|g| g.findings).collect()
}

/// The page a finding is on, when its target is on one.
pub fn page_of(doc: &Document, finding: &ValidationFinding) -> Option<PageIx> {
    let target = finding.target.as_ref()?;
    node_page(target).or_else(|| doc.bbox(target).map(|(page, _)| page))
}

/// Short name of a finding's node as the lists show it: `Figure 22 0 R`, `op #93 Tj`, `MCID 1`.
pub fn node_label(doc: &Document, finding: &ValidationFinding) -> String {
    let Some(target) = &finding.target else {
        return "not resolved".to_owned();
    };
    match target {
        NodeId::StructElem(r) => {
            let role = doc.node(target).map_or_else(String::new, |props| props.title);
            format!("{role} {r}").trim_start().to_owned()
        }
        NodeId::Op { page, ix } => doc
            .captured(*page)
            .and_then(|captured| captured.ops.get(usize_from_u32(*ix)))
            .filter(|op| op.ix == *ix)
            .map_or_else(|| target.to_string(), |op| format!("op #{ix} {}", op.operator)),
        NodeId::MarkedContent { mcid, .. } => format!("MCID {mcid}"),
        NodeId::Page(page) => format!("Page {}", page + 1),
        NodeId::Document => "Document".to_owned(),
        NodeId::Object(r) => format!("obj {r}"),
        NodeId::Resource { kind, name, .. } => format!("{} /{name}", kind.key()),
        other => other.to_string(),
    }
}

/// Where the veraPDF rule of `rule` is documented: the page of the rules of its standard, or the
/// index of veraPDF's validation documentation when pdfxray does not know the standard.
pub fn rule_docs_url(rule: &str) -> &'static str {
    let standard = rule.rsplit_once(' ').map_or(rule, |(standard, _)| standard);
    let part = |prefix: &str| standard.starts_with(prefix);
    if part("ISO 14289-1") {
        "https://github.com/veraPDF/veraPDF-validation-profiles/wiki/PDFUA-Part-1-rules"
    } else if part("ISO 14289-2") {
        "https://github.com/veraPDF/veraPDF-validation-profiles/wiki/PDFUA-Part-2-rules"
    } else if part("ISO 19005-1") {
        "https://github.com/veraPDF/veraPDF-validation-profiles/wiki/PDFA-Part-1-rules"
    } else if part("ISO 19005-2") || part("ISO 19005-3") {
        "https://github.com/veraPDF/veraPDF-validation-profiles/wiki/PDFA-Parts-2-and-3-rules"
    } else if part("ISO 19005-4") {
        "https://github.com/veraPDF/veraPDF-validation-profiles/wiki/PDFA-Part-4-rules"
    } else {
        // The rules of the standards pdfxray does not know are listed from here.
        "https://docs.verapdf.org/validation/"
    }
}

/// A failed-check mark: a red ring with a cross, drawn inside `rect`.
pub fn fail_mark(painter: &Painter, rect: UiRect, color: Color32) {
    let centre = rect.center();
    let radius = rect.width().min(rect.height()) / 2.0 - 1.0;
    painter.circle_stroke(centre, radius, Stroke::new(1.4, color));
    let arm = radius * 0.4;
    for (a, b) in [
        (vec2(-arm, -arm), vec2(arm, arm)),
        (vec2(-arm, arm), vec2(arm, -arm)),
    ] {
        painter.line_segment([centre + a, centre + b], Stroke::new(1.4, color));
    }
}

/// A box on the page for the findings of one node.
#[derive(Debug, Clone, PartialEq)]
pub struct PageBadge {
    /// The finding a click on the badge selects: the current one when the node has several,
    /// otherwise the first.
    pub finding: u32,
    /// The node's box in page space.
    pub rect: Rect,
    /// What the pill says, e.g. `× 7.3-1 · Figure has no alternative text`.
    pub text: String,
    /// True for the node the inspector is showing a finding of.
    pub current: bool,
}

impl AppState {
    /// The finding the inspector shows for `node`: the finding itself when `node` is one, else the
    /// finding of the node that was last chosen, else its first.
    pub fn finding_for(&self, node: &NodeId) -> Option<u32> {
        let run = self.document.as_ref()?.doc.validation()?;
        if let NodeId::Validation(index) = node {
            return run.findings.get(usize_from_u32(*index)).map(|_| *index);
        }
        let chosen = self.validation.selected_finding;
        let mut hits = run.findings_for(node).map(|(index, _)| index).peekable();
        let first = hits.peek().copied();
        hits.find(|index| Some(*index) == chosen).or(first)
    }

    /// The finding of the selected node, if it has one.
    pub fn current_finding(&self) -> Option<u32> {
        self.finding_for(self.selection.primary.as_ref()?)
    }

    /// Selects the finding after (`delta` 1) or before (`delta` -1) the current one in the
    /// navigator's order, wrapping around at the ends. Without a current finding, the first or
    /// the last.
    pub fn step_finding(&mut self, delta: i32) {
        let Some(run) = self.document.as_ref().and_then(|l| l.doc.validation()) else {
            return;
        };
        let order = order(run);
        let Some(last) = order.len().checked_sub(1) else {
            return;
        };
        let at = self
            .current_finding()
            .and_then(|current| order.iter().position(|index| *index == current));
        let next = match (at, delta >= 0) {
            (None, true) => 0,
            (Some(at), true) => (at + 1) % order.len(),
            (None | Some(0), false) => last,
            (Some(at), false) => at - 1,
        };
        self.select_finding(order[next]);
    }

    /// The badges to draw on `page`: one per node with findings on it.
    pub fn page_badges(&self, page: PageIx) -> Vec<PageBadge> {
        let Some(loaded) = &self.document else {
            return Vec::new();
        };
        let Some(run) = loaded.doc.validation() else {
            return Vec::new();
        };
        let current = self.current_finding();
        let mut badges: Vec<(NodeId, PageBadge, usize)> = Vec::new();
        for (index, finding) in (0u32..).zip(&run.findings) {
            let Some(target) = &finding.target else {
                continue;
            };
            if let Some((_, badge, count)) = badges.iter_mut().find(|(node, ..)| node == target) {
                *count += 1;
                if Some(index) == current {
                    badge.finding = index;
                    badge.current = true;
                }
                continue;
            }
            let Some(shown) = selection_rect(&loaded.doc, target).filter(|s| s.page == page) else {
                continue;
            };
            badges.push((
                target.clone(),
                PageBadge {
                    finding: index,
                    rect: shown.rect,
                    text: format!("× {} · {}", short_rule(&finding.rule), title_of(finding)),
                    current: Some(index) == current,
                },
                1,
            ));
        }
        badges
            .into_iter()
            .map(|(_, mut badge, count)| {
                if count > 1 {
                    badge.text = format!("{} +{}", badge.text, count - 1);
                }
                badge
            })
            .collect()
    }

    /// How many failed checks sit on `page`, counting every finding whose node is drawn there.
    pub fn failed_on_page(&self, page: PageIx) -> usize {
        let Some(loaded) = &self.document else {
            return 0;
        };
        loaded.doc.validation().map_or(0, |run| {
            run.findings
                .iter()
                .filter(|finding| page_of(&loaded.doc, finding) == Some(page))
                .count()
        })
    }
}

/// Where `pos` puts a pill so that it sits on the box's top-left corner without covering a pill
/// that is already there.
pub fn pill_position(corner: Pos2, top: f32, placed: &[UiRect], size: egui::Vec2) -> UiRect {
    let mut rect = UiRect::from_min_size(Pos2::new(corner.x, (corner.y - size.y - 2.0).max(top)), size);
    while placed.iter().any(|other| other.intersects(rect)) && rect.min.y > top {
        rect = rect.translate(vec2(0.0, -(size.y + 2.0)));
    }
    rect
}

/// The red of badges and outlines.
pub const FAIL: Color32 = tokens::ERR;

#[cfg(test)]
#[allow(clippy::float_cmp)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    fn finding(rule: &str, target: Option<NodeId>) -> ValidationFinding {
        ValidationFinding {
            rule: rule.to_owned(),
            description: "A requirement".to_owned(),
            message: None,
            context: String::new(),
            target,
            tags: Vec::new(),
        }
    }

    fn run(findings: Vec<ValidationFinding>) -> ValidationRun {
        ValidationRun {
            profile: "PDF/UA-1 validation profile".to_owned(),
            compliant: false,
            verapdf_version: None,
            checks_passed: 0,
            checks_failed: u64::try_from(findings.len()).unwrap(),
            findings,
        }
    }

    #[test]
    fn rule_ids_split_into_standard_clause_and_test() {
        assert_eq!(short_rule("ISO 14289-1:2014 7.21.4.1-1"), "7.21.4.1-1");
        assert_eq!(clause("ISO 14289-1:2014 7.21.4.1-1"), "7.21.4.1");
        assert_eq!(short_rule("garbage"), "garbage");
    }

    #[test]
    fn findings_group_by_rule_in_first_failure_order() {
        let run = run(vec![
            finding("ISO 14289-1:2014 7.3-1", None),
            finding("ISO 14289-1:2014 7.1-3", None),
            finding("ISO 14289-1:2014 7.3-1", None),
        ]);

        let groups = groups(&run);

        assert_eq!(
            groups
                .iter()
                .map(|g| (g.short.as_str(), g.findings.clone()))
                .collect::<Vec<_>>(),
            [("7.3-1", vec![0, 2]), ("7.1-3", vec![1])]
        );
        assert_eq!(groups[0].title, "Figure has no alternative text");
        assert_eq!(order(&run), [0, 2, 1]);
    }

    #[test]
    fn a_rule_without_a_note_is_titled_by_its_requirement() {
        let run = run(vec![finding("ISO 19005-1:2005 6.7.2-1", None)]);

        assert_eq!(groups(&run)[0].title, "A requirement");
    }

    #[test]
    fn rule_docs_follow_the_standard() {
        assert!(rule_docs_url("ISO 14289-1:2014 7.3-1").ends_with("PDFUA-Part-1-rules"));
        assert!(rule_docs_url("ISO 14289-2:2024 8.2.4-1").ends_with("PDFUA-Part-2-rules"));
        assert!(rule_docs_url("ISO 19005-3:2012 6.1-1").ends_with("PDFA-Parts-2-and-3-rules"));
        assert_eq!(rule_docs_url("WCAG 1-1"), "https://docs.verapdf.org/validation/");
    }

    #[test]
    fn pills_do_not_cover_each_other() {
        let size = vec2(100.0, 18.0);
        let first = pill_position(Pos2::new(10.0, 100.0), 0.0, &[], size);
        let second = pill_position(Pos2::new(10.0, 100.0), 0.0, &[first], size);

        assert!(!first.intersects(second));
        assert!(second.min.y < first.min.y);
        // Never above the top of the page.
        let top = pill_position(Pos2::new(10.0, 5.0), 0.0, &[], size);
        assert_eq!(top.min.y, 0.0);
    }
}
