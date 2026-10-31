//! Parser of veraPDF's machine readable report (`--format xml`, a.k.a. MRR) into the
//! [`Report`] model of `pdfxray-core`.
//!
//! Only the parts pdfxray needs are read: the build information, the first
//! `validationReport` (profile name, verdict, check counters) and every `check` below its
//! `rule` elements. If the job did not produce a validation report (damaged or encrypted
//! input) the `taskException` message is surfaced as [`ParseError::JobFailed`].

use pdfxray_core::validation::{Finding, Report, RuleId, Status};
use quick_xml::Reader;
use quick_xml::XmlVersion;
use quick_xml::events::{BytesStart, Event};

use crate::context::parse_context;

/// Why a report could not be turned into a [`Report`].
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ParseError {
    /// The XML is not well formed.
    #[error("malformed veraPDF report XML: {0}")]
    Xml(String),
    /// The report has no `validationReport` element.
    #[error("report contains no validation report")]
    NoValidationReport,
    /// A numeric attribute did not hold a number.
    #[error("attribute `{attribute}` of <{element}> is not a valid number: `{value}`")]
    BadNumber {
        /// Element name.
        element: &'static str,
        /// Attribute name.
        attribute: &'static str,
        /// Offending value.
        value: String,
    },
    /// veraPDF could not process the file (e.g. damaged or encrypted) and said so in a
    /// `taskException` instead of a validation report.
    #[error("veraPDF could not process the document ({kind}): {message}")]
    JobFailed {
        /// Task type, e.g. `PARSE`.
        kind: String,
        /// Exception message.
        message: String,
    },
}

/// Parses a veraPDF XML report. Pure; no I/O.
///
/// Only the first job's validation report is read (pdfxray always validates one file per run).
pub fn parse_report(xml: &str) -> Result<Report, ParseError> {
    ReportParser::default().run(xml)
}

/// Which text-bearing child element is currently open.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Field {
    Description,
    Object,
    Context,
    ErrorMessage,
    ErrorArgument,
    ExceptionMessage,
}

impl Field {
    fn of(element: &str, inside_check: bool, inside_rule: bool) -> Option<Self> {
        match (element, inside_check, inside_rule) {
            ("context", true, _) => Some(Self::Context),
            ("errorMessage", true, _) => Some(Self::ErrorMessage),
            ("errorArgument" | "argument", true, _) => Some(Self::ErrorArgument),
            ("description", false, true) => Some(Self::Description),
            ("object", false, true) => Some(Self::Object),
            ("exceptionMessage", _, _) => Some(Self::ExceptionMessage),
            _ => None,
        }
    }
}

#[derive(Debug, Default)]
struct RuleState {
    id: Option<RuleId>,
    description: String,
    object: String,
    tags: Vec<String>,
}

#[derive(Debug, Default)]
struct CheckState {
    status: Option<Status>,
    context: String,
    message: Option<String>,
    arguments: Vec<String>,
}

#[derive(Debug, Default)]
struct ReportParser {
    version: Option<String>,
    profile: Option<String>,
    compliant: bool,
    passed: u64,
    failed: u64,
    findings: Vec<Finding>,
    exception: Option<(String, String)>,
    // parsing state
    in_report: bool,
    report_done: bool,
    saw_report: bool,
    in_details: bool,
    rule: Option<RuleState>,
    check: Option<CheckState>,
    exception_kind: Option<String>,
    field: Option<Field>,
    text: String,
}

impl ReportParser {
    fn run(mut self, xml: &str) -> Result<Report, ParseError> {
        let mut reader = Reader::from_str(xml);
        loop {
            let event = reader.read_event().map_err(|e| ParseError::Xml(e.to_string()))?;
            match event {
                Event::Start(e) => self.start(&e)?,
                Event::Empty(e) => {
                    self.start(&e)?;
                    self.end(e.name().as_ref());
                }
                Event::End(e) => self.end(e.name().as_ref()),
                Event::Text(t) if self.field.is_some() => self.text.push_str(&t.xml10_content()),
                Event::CData(t) if self.field.is_some() => self.text.push_str(&t.xml10_content()),
                Event::GeneralRef(r) if self.field.is_some() => {
                    if let Some(c) = r.resolve_char_ref().map_err(|e| ParseError::Xml(e.to_string()))? {
                        self.text.push(c);
                    } else if let Some(c) = predefined_entity(&r.into_inner()) {
                        self.text.push(c);
                    }
                }
                Event::Eof => break,
                _ => {}
            }
        }
        self.finish(xml)
    }

    fn start(&mut self, e: &BytesStart<'_>) -> Result<(), ParseError> {
        let name = e.name().as_ref().to_owned();
        match name.as_str() {
            "releaseDetails" if attr(e, "id").as_deref() == Some("core") => {
                self.version = self.version.take().or_else(|| attr(e, "version"));
            }
            "validationReport" if !self.report_done => {
                self.in_report = true;
                self.saw_report = true;
                self.profile = attr(e, "profileName");
                self.compliant = attr(e, "isCompliant").as_deref() == Some("true");
            }
            "details" if self.in_report && self.rule.is_none() => {
                self.in_details = true;
                self.passed = number(e, "details", "passedChecks")?;
                self.failed = number(e, "details", "failedChecks")?;
            }
            "rule" if self.in_details => {
                let test_number = number(e, "rule", "testNumber")?;
                self.rule = Some(RuleState {
                    id: Some(RuleId {
                        specification: attr(e, "specification").unwrap_or_default(),
                        clause: attr(e, "clause").unwrap_or_default(),
                        test_number: u32::try_from(test_number).map_err(|_| ParseError::BadNumber {
                            element: "rule",
                            attribute: "testNumber",
                            value: test_number.to_string(),
                        })?,
                    }),
                    tags: attr(e, "tags")
                        .map(|t| {
                            t.split(',')
                                .filter(|s| !s.is_empty())
                                .map(str::to_owned)
                                .collect()
                        })
                        .unwrap_or_default(),
                    ..RuleState::default()
                });
            }
            "check" if self.rule.is_some() => {
                self.check = Some(CheckState {
                    status: match attr(e, "status").as_deref() {
                        Some("passed") => Some(Status::Passed),
                        _ => Some(Status::Failed),
                    },
                    ..CheckState::default()
                });
            }
            "taskException" if !self.saw_report => self.exception_kind = attr(e, "type"),
            _ => {}
        }
        self.field = Field::of(&name, self.check.is_some(), self.rule.is_some());
        self.text.clear();
        Ok(())
    }

    fn end(&mut self, name: &str) {
        let value = std::mem::take(&mut self.text);
        match (self.field.take(), name) {
            (Some(Field::Description), _) => {
                if let Some(rule) = &mut self.rule {
                    value.trim().clone_into(&mut rule.description);
                }
            }
            (Some(Field::Object), _) => {
                if let Some(rule) = &mut self.rule {
                    value.trim().clone_into(&mut rule.object);
                }
            }
            (Some(Field::Context), _) => {
                if let Some(check) = &mut self.check {
                    value.trim().clone_into(&mut check.context);
                }
            }
            (Some(Field::ErrorMessage), _) => {
                if let Some(check) = &mut self.check {
                    check.message = Some(value.trim().to_owned());
                }
            }
            (Some(Field::ErrorArgument), _) => {
                if let Some(check) = &mut self.check {
                    check.arguments.push(value.trim().to_owned());
                }
            }
            (Some(Field::ExceptionMessage), _) => {
                if self.exception.is_none() && !self.saw_report {
                    let kind = self.exception_kind.clone().unwrap_or_default();
                    self.exception = Some((kind, value.trim().to_owned()));
                }
            }
            (None, _) => {}
        }
        match name {
            "check" => self.finish_check(),
            "rule" => self.rule = None,
            "details" if self.rule.is_none() => self.in_details = false,
            "validationReport" if self.in_report => {
                self.in_report = false;
                self.report_done = true;
            }
            _ => {}
        }
    }

    fn finish_check(&mut self) {
        let (Some(check), Some(rule)) = (self.check.take(), self.rule.as_ref()) else {
            return;
        };
        let Some(id) = rule.id.clone() else { return };
        self.findings.push(Finding {
            rule: id,
            description: rule.description.clone(),
            status: check.status.unwrap_or(Status::Failed),
            locator: parse_context(&check.context),
            context: check.context,
            error_message: check.message,
            error_arguments: check.arguments,
            object: rule.object.clone(),
            tags: rule.tags.clone(),
        });
    }

    fn finish(self, xml: &str) -> Result<Report, ParseError> {
        let Some(profile) = self.profile else {
            return Err(match self.exception {
                Some((kind, message)) => ParseError::JobFailed { kind, message },
                None => ParseError::NoValidationReport,
            });
        };
        Ok(Report {
            profile,
            compliant: self.compliant,
            checks_passed: self.passed,
            checks_failed: self.failed,
            findings: self.findings,
            verapdf_version: self.version,
            raw_xml: xml.to_owned(),
        })
    }
}

fn attr(e: &BytesStart<'_>, name: &str) -> Option<String> {
    e.try_get_attribute(name)
        .ok()
        .flatten()
        .and_then(|a| a.normalized_value(XmlVersion::Implicit1_0).ok())
        .map(std::borrow::Cow::into_owned)
}

fn number(e: &BytesStart<'_>, element: &'static str, attribute: &'static str) -> Result<u64, ParseError> {
    match attr(e, attribute) {
        None => Ok(0),
        Some(value) => value.trim().parse().map_err(|_| ParseError::BadNumber {
            element,
            attribute,
            value,
        }),
    }
}

fn predefined_entity(name: &str) -> Option<char> {
    match name {
        "lt" => Some('<'),
        "gt" => Some('>'),
        "amp" => Some('&'),
        "apos" => Some('\''),
        "quot" => Some('"'),
        _ => None,
    }
}
