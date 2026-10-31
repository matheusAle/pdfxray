//! `parse_report` against reports captured from veraPDF 1.30.3 (`tests/data/*.xml`, paths in
//! `<name>` sanitised), plus synthetic edge cases for escaping and failure modes.

// Helper functions here are test code even though they are not `#[test]` functions themselves.
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used, clippy::print_stderr)]

use pdfxray_core::validation::{Finding, Locator, Report, RuleId, Status};
use pdfxray_verapdf::{ParseError, parse_report};
use pretty_assertions::assert_eq;

fn data(name: &str) -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/data")
        .join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("reading {}: {e}", path.display()))
}

fn rule(specification: &str, clause: &str, test_number: u32) -> RuleId {
    RuleId {
        specification: specification.to_owned(),
        clause: clause.to_owned(),
        test_number,
    }
}

/// `(rule, locator, error message)` per finding: compact but complete enough to catch drift.
fn summary(report: &Report) -> Vec<(String, Locator, String)> {
    report
        .findings
        .iter()
        .map(|f| {
            (
                f.rule.to_string(),
                f.locator.clone(),
                f.error_message.clone().unwrap_or_default(),
            )
        })
        .collect()
}

const UA1: &str = "ISO 14289-1:2014";
const NO_METADATA: &str = "The document catalog dictionary doesn't contain metadata key or metadata stream dictionary does not contain either entry Type with value /Metadata or entry Subtype with value /XML";
const NO_VIEWER_PREFS: &str = "ViewerPreferences dictionary is not present in the document Catalog, or DisplayDocTitle key is set to false or is not present in the ViewerPreferences dictionary (ViewerPreferences = null, DisplayDocTitle = null)";

#[test]
fn tagged_simple_against_ua1() {
    let xml = data("tagged-simple-ua1.xml");
    let report = parse_report(&xml).expect("parses");
    assert_eq!(
        (
            report.profile.as_str(),
            report.compliant,
            report.checks_passed,
            report.checks_failed,
            report.verapdf_version.as_deref(),
            report.raw_xml == xml,
        ),
        ("PDF/UA-1 validation profile", false, 129, 3, Some("1.30.3"), true)
    );
    assert_eq!(
        report.findings,
        vec![
            Finding {
                rule: rule(UA1, "7.1", 10),
                description: "The document catalog dictionary shall include a ViewerPreferences dictionary containing a DisplayDocTitle key, whose value shall be true".to_owned(),
                status: Status::Failed,
                context: "root".to_owned(),
                locator: Locator::Document,
                error_message: Some(NO_VIEWER_PREFS.to_owned()),
                error_arguments: vec![],
                object: "CosDocument".to_owned(),
                tags: vec!["syntax".to_owned()],
            },
            Finding {
                rule: rule(UA1, "7.21.4.1", 1),
                description: "The font programs for all fonts used for rendering within a conforming file shall be embedded within that file, as defined in ISO 32000-1:2008, 9.9".to_owned(),
                status: Status::Failed,
                context: "root/document[0]/pages[0](3 0 obj PDPage)/contentStream[0](4 0 obj PDSemanticContentStream)/operators[4]/font[0](Helvetica)".to_owned(),
                locator: Locator::Font { page: Some(0), name: "Helvetica".to_owned() },
                error_message: Some("The font program is not embedded".to_owned()),
                error_arguments: vec![],
                object: "PDFont".to_owned(),
                tags: vec!["font".to_owned()],
            },
            Finding {
                rule: rule(UA1, "7.1", 8),
                description: "The Catalog dictionary of a conforming file shall contain the Metadata key whose value is a metadata stream as defined in ISO 32000-1:2008, 14.3.2. The metadata stream dictionary shall contain entry Type with value /Metadata and entry Subtype with value /XML".to_owned(),
                status: Status::Failed,
                context: "root/document[0]".to_owned(),
                locator: Locator::Document,
                error_message: Some(NO_METADATA.to_owned()),
                error_arguments: vec![],
                object: "PDDocument".to_owned(),
                tags: vec!["metadata".to_owned()],
            },
        ]
    );
}

#[test]
fn orphan_mcid_points_at_the_marked_content() {
    let report = parse_report(&data("orphan-mcid-ua1.xml")).expect("parses");
    assert_eq!(
        (report.compliant, report.checks_passed, report.checks_failed),
        (false, 225, 4)
    );
    assert_eq!(
        summary(&report),
        vec![
            (
                format!("{UA1} 7.21.4.1-1"),
                Locator::Font {
                    page: Some(0),
                    name: "Helvetica".to_owned()
                },
                "The font program is not embedded".to_owned()
            ),
            (
                format!("{UA1} 7.1-10"),
                Locator::Document,
                NO_VIEWER_PREFS.to_owned()
            ),
            (format!("{UA1} 7.1-8"), Locator::Document, NO_METADATA.to_owned()),
            (
                format!("{UA1} 7.1-3"),
                Locator::MarkedContent { page: 0, mcid: 1 },
                "Content is neither marked as Artifact nor tagged as real content".to_owned()
            ),
        ]
    );
    let orphan = &report.findings[3];
    assert_eq!(
        (
            orphan.description.as_str(),
            orphan.object.as_str(),
            orphan.tags.as_slice(),
            orphan.context.as_str()
        ),
        (
            "Content shall be marked as Artifact or tagged as real content",
            "SESimpleContentItem",
            ["artifact".to_owned()].as_slice(),
            "root/document[0]/pages[0](3 0 obj PDPage)/contentStream[0](4 0 obj PDSemanticContentStream)/content[1]{mcid:1}/contentItem[0]{mcid:1}"
        )
    );
}

#[test]
fn wcag_profile_keeps_severity_tags() {
    let report = parse_report(&data("orphan-mcid-wcag22.xml")).expect("parses");
    assert_eq!(report.profile, "WCAG 2.2 machine validation profile");
    let tags: Vec<(String, Vec<String>)> = report
        .findings
        .iter()
        .map(|f| (f.rule.to_string(), f.tags.clone()))
        .collect();
    let owned = |tags: &[&str]| tags.iter().map(|t| (*t).to_owned()).collect::<Vec<_>>();
    assert_eq!(
        tags,
        vec![
            (format!("{UA1} 7.1-10"), owned(&["minor", "machine", "syntax"])),
            (format!("{UA1} 7.21.4.1-1"), owned(&["major", "machine", "font"])),
            (
                format!("{UA1} 7.1-8"),
                owned(&["critical", "machine", "metadata"])
            ),
            (
                format!("{UA1} 7.1-3"),
                owned(&["critical", "machine", "artifact"])
            ),
        ]
    );
}

#[test]
fn compliant_report_has_no_findings() {
    let report = parse_report(&data("ua1-compliant.xml")).expect("parses");
    assert_eq!(
        (
            report.compliant,
            report.checks_passed,
            report.checks_failed,
            report.findings
        ),
        (true, 365, 0, vec![])
    );
}

#[test]
fn structure_table_cell_is_addressed_by_path_and_object() {
    let report = parse_report(&data("ua1-table.xml")).expect("parses");
    assert_eq!(
        summary(&report),
        vec![(
            format!("{UA1} 7.5-1"),
            Locator::StructElem { obj: Some((33, 0)), path: vec![0, 1, 1, 1] },
            "TD does not contain Headers attribute, and Headers for this table cell cannot be determined algorithmically"
                .to_owned()
        )]
    );
    assert_eq!(
        report.findings[0].tags,
        vec!["structure".to_owned(), "table".to_owned()]
    );
}

#[test]
fn link_annotation_and_form_xobject_locators() {
    let link = parse_report(&data("ua1-link-annotation.xml")).expect("parses");
    assert_eq!(
        summary(&link),
        vec![(
            format!("{UA1} 7.18.5-1"),
            Locator::Annotation { page: 0, index: 0 },
            "A Link annotation is an Artifact or is nested within P tag (standard type = P) instead of Link"
                .to_owned()
        )]
    );

    let form = parse_report(&data("ua1-xobject.xml")).expect("parses");
    let message = "Form XObject contains MCIDs and is referenced more than once".to_owned();
    assert_eq!(
        (form.checks_failed, summary(&form)),
        (
            2,
            vec![
                (
                    format!("{UA1} 7.20-2"),
                    Locator::ContentOperator {
                        page: 0,
                        operator_index: 23
                    },
                    message.clone()
                ),
                (
                    format!("{UA1} 7.20-2"),
                    Locator::ContentOperator {
                        page: 1,
                        operator_index: 2
                    },
                    message
                ),
            ]
        )
    );
}

#[test]
fn ua2_report_lists_every_failed_check_of_a_rule() {
    let report = parse_report(&data("ua2-structure.xml")).expect("parses");
    assert_eq!(report.profile, "PDF/UA-2 + Tagged PDF validation profile");
    assert_eq!(
        summary(&report),
        vec![
            (
                "ISO 14289-2:2024 8.2.4-1".to_owned(),
                Locator::StructElem { obj: Some((16, 0)), path: vec![0, 1] },
                "Non-standard structure type http://iso.org/pdf/ssn:/Standard is not mapped to a standard type".to_owned()
            ),
            (
                "ISO 14289-2:2024 8.2.4-1".to_owned(),
                Locator::StructElem { obj: Some((17, 0)), path: vec![0, 2] },
                "Non-standard structure type http://iso.org/pdf/ssn:/Text#20body is not mapped to a standard type".to_owned()
            ),
        ]
    );
}

#[test]
fn capped_listing_keeps_the_true_failure_count() {
    // veraPDF was run with `--maxfailuresdisplayed 2`: three checks of 6.2.3.3 failed, two listed.
    let report = parse_report(&data("pdfa1b-capped.xml")).expect("parses");
    assert_eq!((report.checks_failed, report.findings.len()), (5, 4));
    assert_eq!(
        summary(&report)
            .into_iter()
            .map(|(rule, locator, _)| (rule, locator))
            .collect::<Vec<_>>(),
        vec![
            (
                "ISO 19005-1:2005 6.2.3.3-3".to_owned(),
                Locator::ContentOperator {
                    page: 0,
                    operator_index: 4
                }
            ),
            (
                "ISO 19005-1:2005 6.2.3.3-3".to_owned(),
                Locator::ContentOperator {
                    page: 0,
                    operator_index: 11
                }
            ),
            (
                "ISO 19005-1:2005 6.3.4-1".to_owned(),
                Locator::Font {
                    page: Some(0),
                    name: "Helvetica".to_owned()
                }
            ),
            ("ISO 19005-1:2005 6.7.2-1".to_owned(), Locator::Document),
        ]
    );
}

#[test]
fn tagged_pdf_profile_points_at_the_structure_tree_root() {
    let report = parse_report(&data("iso32000-1-tagged.xml")).expect("parses");
    assert_eq!(report.profile, "Tagged pdf profile for PDF 1.7 specification");
    assert_eq!(
        summary(&report),
        vec![(
            "ISO 19005-2:2011 Annex_L-16".to_owned(),
            Locator::Object {
                num: 6,
                generation: 0
            },
            "StructTreeRoot elem contains Span kid".to_owned()
        )]
    );
}

#[test]
fn encrypted_and_damaged_inputs_surface_the_task_exception() {
    assert_eq!(
        parse_report(&data("job-failed-encrypted.xml")),
        Err(ParseError::JobFailed {
            kind: "PARSE".to_owned(),
            message: "Exception: The PDF stream appears to be encrypted. caused by exception: encrypted PDF with unknown or wrong password".to_owned(),
        })
    );
    assert_eq!(
        parse_report(&data("job-failed-damaged.xml")),
        Err(ParseError::JobFailed {
            kind: "PARSE".to_owned(),
            message: "Exception: Couldn't parse stream caused by exception: End of file is reached"
                .to_owned(),
        })
    );
}

const WRAPPER: &str = r#"<?xml version="1.0" encoding="utf-8"?><report><jobs><job>
<validationReport profileName="P" isCompliant="false"><details passedChecks="1" failedChecks="1">BODY</details></validationReport>
</job></jobs></report>"#;

fn synthetic(body: &str) -> String {
    WRAPPER.replace("BODY", body)
}

#[test]
fn entities_cdata_and_arguments_are_decoded() {
    let xml = synthetic(
        r#"<rule specification="S &amp; T" clause="1.2" testNumber="3" status="failed" failedChecks="1" tags="a,,b">
<description>Fonts &lt;must&gt; be &quot;embedded&quot; &#x26; &#65;</description><object>PDFont</object>
<check status="failed"><context>root/document[0]/pages[1](5 0 obj PDPage)</context>
<errorMessage><![CDATA[Font <Foo&Bar> missing]]></errorMessage>
<errorArgument>Foo</errorArgument><errorArgument> 12 </errorArgument></check></rule>"#,
    );
    let report = parse_report(&xml).expect("parses");
    assert_eq!(
        report.findings,
        vec![Finding {
            rule: rule("S & T", "1.2", 3),
            description: "Fonts <must> be \"embedded\" & A".to_owned(),
            status: Status::Failed,
            context: "root/document[0]/pages[1](5 0 obj PDPage)".to_owned(),
            locator: Locator::Page { index: 1 },
            error_message: Some("Font <Foo&Bar> missing".to_owned()),
            error_arguments: vec!["Foo".to_owned(), "12".to_owned()],
            object: "PDFont".to_owned(),
            tags: vec!["a".to_owned(), "b".to_owned()],
        }]
    );
}

#[test]
fn passed_checks_keep_their_status() {
    let xml = synthetic(
        r#"<rule specification="S" clause="1" testNumber="1" status="passed">
<description>d</description><object>O</object>
<check status="passed"><context>root</context></check>
<check status="failed"><context>root/document[0]</context></check></rule>"#,
    );
    let report = parse_report(&xml).expect("parses");
    assert_eq!(
        report
            .findings
            .iter()
            .map(|f| (f.status, f.locator.clone(), f.error_message.clone()))
            .collect::<Vec<_>>(),
        vec![
            (Status::Passed, Locator::Document, None),
            (Status::Failed, Locator::Document, None)
        ]
    );
}

#[test]
fn only_the_first_job_is_read() {
    let job = |description: &str| {
        format!(
            r#"<job><validationReport profileName="P-{description}" isCompliant="false"><details passedChecks="1" failedChecks="1">
<rule specification="S" clause="1" testNumber="1"><description>{description}</description><object>O</object>
<check status="failed"><context>root</context></check></rule></details></validationReport></job>"#
        )
    };
    let xml = format!("<report><jobs>{}{}</jobs></report>", job("first"), job("second"));
    let report = parse_report(&xml).expect("parses");
    assert_eq!(
        (
            report.profile.as_str(),
            report
                .findings
                .iter()
                .map(|f| f.description.as_str())
                .collect::<Vec<_>>()
        ),
        ("P-first", vec!["first"])
    );
}

#[test]
fn bad_input_is_a_typed_error() {
    assert!(matches!(
        parse_report("<report><jobs>"),
        Err(ParseError::Xml(_) | ParseError::NoValidationReport)
    ));
    assert!(matches!(parse_report("<a><b></a>"), Err(ParseError::Xml(_))));
    assert_eq!(
        parse_report("<report></report>"),
        Err(ParseError::NoValidationReport)
    );
    assert_eq!(parse_report(""), Err(ParseError::NoValidationReport));
    let bad_number = WRAPPER
        .replace("passedChecks=\"1\"", "passedChecks=\"many\"")
        .replace("BODY", "");
    assert_eq!(
        parse_report(&bad_number),
        Err(ParseError::BadNumber {
            element: "details",
            attribute: "passedChecks",
            value: "many".to_owned()
        })
    );
}
