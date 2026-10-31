//! A validation report: the rules checked, their verdicts and the failed checks.
//!
//! These are the validator's results as plain data. `pdfxray-verapdf` produces them from
//! veraPDF's machine readable report;
//! [`Document::set_validation`](crate::Document::set_validation) resolves them onto node ids.

use std::fmt;

use serde::{Deserialize, Serialize};

use super::Locator;

/// Identifies a veraPDF rule: `specification`, `clause` and `testNumber` of the rule element.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RuleId {
    /// Specification the clause belongs to, e.g. `ISO 14289-1:2014`.
    pub specification: String,
    /// Clause number, e.g. `7.1`.
    pub clause: String,
    /// Test number within the clause (1-based, as in veraPDF).
    pub test_number: u32,
}

impl fmt::Display for RuleId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}-{}", self.specification, self.clause, self.test_number)
    }
}

/// Outcome of a single check.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Status {
    /// The check failed (this is what veraPDF reports by default).
    Failed,
    /// The check passed (only present when passed checks are requested).
    Passed,
}

/// One failed check of one rule, with a locator that pdfxray can map onto its nodes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Finding {
    /// The violated rule.
    pub rule: RuleId,
    /// The rule's description (the normative requirement).
    pub description: String,
    /// Check status.
    pub status: Status,
    /// veraPDF's raw context path.
    pub context: String,
    /// Locator derived from `context`.
    pub locator: Locator,
    /// veraPDF's message for this particular check, when present.
    pub error_message: Option<String>,
    /// Message arguments. veraPDF 1.30 does not emit them in the XML report, so this is
    /// normally empty; it is filled when `<errorArguments>` elements are present.
    pub error_arguments: Vec<String>,
    /// veraPDF model object the rule applies to, e.g. `PDFont` or `SEP`.
    pub object: String,
    /// Rule tags (`structure`, `font`, `metadata`, severity tags in WCAG profiles ...).
    pub tags: Vec<String>,
}

/// A parsed validation report.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Report {
    /// Human readable profile name as reported by veraPDF.
    pub profile: String,
    /// Whether veraPDF found the document compliant.
    pub compliant: bool,
    /// Number of passed checks.
    pub checks_passed: u64,
    /// Number of failed checks (may exceed `findings.len()` when veraPDF caps the listing).
    pub checks_failed: u64,
    /// One finding per listed failed check, in report order.
    pub findings: Vec<Finding>,
    /// veraPDF release that produced the report (`core` build information), when present.
    pub verapdf_version: Option<String>,
    /// The unmodified XML the report was parsed from.
    pub raw_xml: String,
}
