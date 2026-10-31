//! Validation results: the validator's [`Report`] as plain data, and the run attached to the
//! document, where each failed check is resolved to the [`NodeId`] it concerns so the UI can
//! badge tree nodes and jump to them.
//!
//! The types are owned here, not by the validator adapter: `pdfxray-verapdf` depends on this
//! crate and produces a [`Report`], so the document model pulls in no XML, HTTP or hashing code.

mod explain;
mod locator;
mod report;

pub use explain::{RuleText, explain_rule};
pub use locator::Locator;
pub use report::{Finding, Report, RuleId, Status};
use serde::Serialize;

use crate::numeric::usize_from_u32;
use crate::{NodeId, ObjRef, PropGroup, Props};

/// The most recent validation run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ValidationRun {
    /// Profile name as reported by veraPDF.
    pub profile: String,
    /// Overall verdict.
    pub compliant: bool,
    /// veraPDF version, if reported.
    pub verapdf_version: Option<String>,
    /// Number of passed checks.
    pub checks_passed: u64,
    /// Number of failed checks.
    pub checks_failed: u64,
    /// Failed checks, in report order; index = [`NodeId::Validation`] number.
    pub findings: Vec<ValidationFinding>,
}

/// One failed check.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ValidationFinding {
    /// Rule id, e.g. `ISO 14289-1:2014 7.1-3`.
    pub rule: String,
    /// The normative requirement.
    pub description: String,
    /// veraPDF's message for this occurrence.
    pub message: Option<String>,
    /// veraPDF's raw context path.
    pub context: String,
    /// The pdfxray node this finding concerns, when it could be resolved.
    pub target: Option<NodeId>,
    /// Rule tags.
    pub tags: Vec<String>,
}

impl ValidationRun {
    /// Builds a run from a report, resolving locators with `locate`.
    pub(crate) fn from_report(report: &Report, locate: impl Fn(&Locator) -> Option<NodeId>) -> Self {
        let findings = report
            .findings
            .iter()
            .filter(|f| f.status == Status::Failed)
            .map(|f| ValidationFinding::new(f, locate(&f.locator)))
            .collect();
        Self {
            profile: report.profile.clone(),
            compliant: report.compliant,
            verapdf_version: report.verapdf_version.clone(),
            checks_passed: report.checks_passed,
            checks_failed: report.checks_failed,
            findings,
        }
    }

    /// Findings attached to `id`, with their indices.
    pub fn findings_for<'a>(&'a self, id: &'a NodeId) -> impl Iterator<Item = (u32, &'a ValidationFinding)> {
        self.findings
            .iter()
            .zip(0..)
            .filter(move |(f, _)| f.target.as_ref() == Some(id))
            .map(|(f, i)| (i, f))
    }

    pub(crate) fn props(&self, index: u32) -> Option<Props> {
        let f = self.findings.get(usize_from_u32(index))?;
        let mut rule = PropGroup::new("Rule")
            .row("Rule", &f.rule)
            .row("Profile", &self.profile)
            .row("Requirement", &f.description);
        if let Some(m) = &f.message {
            rule = rule.row("Message", m);
        }
        if !f.tags.is_empty() {
            rule = rule.row("Tags", f.tags.join(", "));
        }
        let mut location = PropGroup::new("Location").row("Context", &f.context);
        location = match &f.target {
            Some(t) => location.link("Target", t.to_string(), t.clone()),
            None => location.row("Target", "not resolved"),
        };
        Some(Props {
            id: NodeId::Validation(index),
            title: f.rule.clone(),
            kind: "validation",
            groups: vec![rule, location],
        })
    }
}

impl ValidationFinding {
    /// The plain-language title, why-it-matters and how-to-fix text for this finding's rule, when
    /// pdfxray has one (PDF/UA-1 and PDF/UA-2 rules, see [`explain_rule`]).
    pub fn explain(&self) -> Option<RuleText> {
        let (specification, id) = self.rule.rsplit_once(' ')?;
        let (clause, test) = id.rsplit_once('-')?;
        explain_rule(specification, clause, test.parse().ok()?)
    }

    fn new(f: &Finding, target: Option<NodeId>) -> Self {
        Self {
            rule: format!(
                "{} {}-{}",
                f.rule.specification, f.rule.clause, f.rule.test_number
            ),
            description: f.description.clone(),
            message: f.error_message.clone(),
            context: f.context.clone(),
            target,
            tags: f.tags.clone(),
        }
    }
}

/// Resolves the parts of a locator that need no document knowledge. Locators that depend on
/// page content (operators, fonts, annotations) are resolved by the document.
pub(crate) fn locate_static(locator: &Locator) -> Option<NodeId> {
    match locator {
        Locator::Document => Some(NodeId::Document),
        Locator::Page { index } => Some(NodeId::Page(*index)),
        Locator::Object { num, generation } => Some(NodeId::Object(ObjRef::new(*num, *generation))),
        Locator::StructElem {
            obj: Some((num, generation)),
            ..
        } => Some(NodeId::StructElem(ObjRef::new(*num, *generation))),
        Locator::MarkedContent { page, mcid } => Some(NodeId::MarkedContent {
            page: *page,
            mcid: *mcid,
        }),
        Locator::StructElem { obj: None, .. }
        | Locator::ContentOperator { .. }
        | Locator::Annotation { .. }
        | Locator::Font { .. }
        | Locator::Unknown => None,
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    fn finding(rule: &str) -> ValidationFinding {
        ValidationFinding {
            rule: rule.to_owned(),
            description: String::new(),
            message: None,
            context: String::new(),
            target: None,
            tags: Vec::new(),
        }
    }

    #[test]
    fn findings_explain_themselves_through_their_rule_id() {
        let figure = finding("ISO 14289-1:2014 7.3-1").explain().unwrap();

        assert_eq!(figure.title, "Figure has no alternative text");
        assert!(figure.how.starts_with("Add an /Alt entry"));
        assert_eq!(
            finding("ISO 14289-1:2014 7.21.4.1-1").explain().map(|r| r.title),
            Some("Font is not embedded")
        );
        assert_eq!(
            finding("ISO 14289-2:2024 8.2.4-1").explain().map(|r| r.exact),
            Some(true)
        );
        assert_eq!(finding("ISO 19005-1:2005 6.7.2-1").explain(), None);
        assert_eq!(finding("garbage").explain(), None);
        assert_eq!(finding("ISO 14289-1:2014 7.3-x").explain(), None);
    }

    #[test]
    fn static_locators_map_to_node_ids() {
        let cases = [
            (Locator::Document, Some(NodeId::Document)),
            (Locator::Page { index: 2 }, Some(NodeId::Page(2))),
            (
                Locator::Object {
                    num: 12,
                    generation: 0,
                },
                Some(NodeId::Object(ObjRef::new(12, 0))),
            ),
            (
                Locator::StructElem {
                    obj: Some((21, 0)),
                    path: vec![0, 2],
                },
                Some(NodeId::StructElem(ObjRef::new(21, 0))),
            ),
            (
                Locator::MarkedContent { page: 0, mcid: 1 },
                Some(NodeId::MarkedContent { page: 0, mcid: 1 }),
            ),
            (
                Locator::StructElem {
                    obj: None,
                    path: vec![0],
                },
                None,
            ),
            (Locator::Unknown, None),
        ];

        for (locator, expected) in cases {
            assert_eq!(locate_static(&locator), expected, "{locator:?}");
        }
    }
}
