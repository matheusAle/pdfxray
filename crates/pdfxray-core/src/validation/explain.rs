//! Plain-language text for the validation rules people meet most often: what the rule is
//! called, why it matters to a reader of the document, and how to fix a file that breaks it.
//!
//! The wording is pdfxray's own; veraPDF's rule descriptions are not reproduced. Rules are keyed
//! like [`RuleId`](super::RuleId): `specification`, `clause` and test number. Where only the
//! clause is known to be about a topic, a clause-level text answers for every test of it (see
//! [`RuleText::exact`]).

/// What a validation rule means, in pdfxray's words.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuleText {
    /// A short plain title ("Figure has no alternative text").
    pub title: &'static str,
    /// Why the rule matters to the people who read the document.
    pub why: &'static str,
    /// What to change to satisfy the rule.
    pub how: &'static str,
    /// True when the text was written for this exact test; false when it is the clause-level
    /// text that answers for every test of the clause.
    pub exact: bool,
}

/// The standards the table covers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Spec {
    /// ISO 14289-1 (PDF/UA-1).
    Ua1,
    /// ISO 14289-2 (PDF/UA-2).
    Ua2,
}

impl Spec {
    fn of(specification: &str) -> Option<Self> {
        let spec = specification.trim();
        if spec.starts_with("ISO 14289-1") {
            Some(Self::Ua1)
        } else if spec.starts_with("ISO 14289-2") {
            Some(Self::Ua2)
        } else {
            None
        }
    }
}

/// One row: a rule of `test` in `clause`, or (`test` = `None`) the whole clause.
struct Row {
    spec: Spec,
    clause: &'static str,
    test: Option<u32>,
    title: &'static str,
    why: &'static str,
    how: &'static str,
}

const fn row(
    spec: Spec,
    clause: &'static str,
    test: Option<u32>,
    title: &'static str,
    why: &'static str,
    how: &'static str,
) -> Row {
    Row {
        spec,
        clause,
        test,
        title,
        why,
        how,
    }
}

use Spec::{Ua1, Ua2};

const ROWS: &[Row] = &[
    // ---- PDF/UA-1 clause 5: identification ----
    row(
        Ua1,
        "5",
        None,
        "File does not declare PDF/UA",
        "Checkers and assistive software look for a PDF/UA declaration in the document's XMP metadata to know which rules the author promised to follow.",
        "Add the PDF/UA identification (pdfuaid:part = 1) to the XMP metadata, or export again with the PDF/UA option on.",
    ),
    // ---- 7.1 general ----
    row(
        Ua1,
        "7.1",
        None,
        "Tagged-PDF basics are not met",
        "PDF/UA starts from a properly tagged document: a structure tree, content tied to it, and document-level metadata. Without them assistive technology has nothing dependable to read.",
        "Export again from the authoring tool with tagging and accessibility options on, or repair the tags in an accessibility editor.",
    ),
    row(
        Ua1,
        "7.1",
        Some(1),
        "Artifact inside tagged content",
        "Decoration that is marked as an artifact but sits inside tagged content confuses the reading order: assistive technology cannot tell whether to read it.",
        "Close the tagged content before the decoration, or tag the decoration's text as real content if it should be read.",
    ),
    row(
        Ua1,
        "7.1",
        Some(2),
        "Tagged content inside an artifact",
        "Assistive technology ignores artifacts. Real content wrapped in one is hidden from screen readers even though it is tagged.",
        "Move the tagged content out of the artifact, or drop the Artifact marking around it.",
    ),
    row(
        Ua1,
        "7.1",
        Some(3),
        "Content is neither tagged nor an artifact",
        "Everything on a page must be real content described by a structure element, or decoration marked as an Artifact. Anything else may be skipped by a screen reader or read in the wrong place.",
        "Tag the content to its structure element, or wrap it in an Artifact (/Artifact BMC … EMC) if it is decoration such as a rule, a background or a page number.",
    ),
    row(
        Ua1,
        "7.1",
        Some(7),
        "A custom tag is not mapped to a standard one",
        "Assistive technology only understands the standard structure types (P, H1, Table …). A custom tag that is not mapped to one of them is read as plain, unlabelled text.",
        "Add a RoleMap entry that sends each custom type to the closest standard type, or use the standard type directly.",
    ),
    row(
        Ua1,
        "7.1",
        Some(8),
        "Document has no XMP metadata",
        "The metadata stream carries the document's identification, such as its title and the PDF/UA declaration. Checkers and assistive software read it to know what they are dealing with.",
        "Add an XMP metadata stream to the catalog (/Metadata with /Type /Metadata and /Subtype /XML); most export tools do this when you fill in the document properties.",
    ),
    row(
        Ua1,
        "7.1",
        Some(10),
        "Window does not show the document title",
        "A screen reader announces the window title. Without DisplayDocTitle the viewer shows the file name, which is often meaningless.",
        "Set /ViewerPreferences << /DisplayDocTitle true >> in the catalog and give the document a real title.",
    ),
    // ---- 7.2 text and language ----
    row(
        Ua1,
        "7.2",
        None,
        "Language is missing or invalid",
        "Screen readers pick the voice and pronunciation from the declared language. A missing or malformed language makes text sound wrong or unreadable.",
        "Set /Lang on the catalog (for example en-US) and on any passage in a different language.",
    ),
    row(
        Ua1,
        "7.2",
        Some(29),
        "A language code is not valid",
        "A /Lang value must be a proper language tag such as en or pt-BR. An empty or malformed one is ignored or mispronounced by assistive technology.",
        "Replace the value with a valid tag, or remove the /Lang entry where the document language already applies.",
    ),
    // ---- 7.3 figures ----
    row(
        Ua1,
        "7.3",
        None,
        "Graphics need a description",
        "Screen readers announce a graphic by reading its description. Without one a blind reader hears nothing useful.",
        "Give each meaningful figure alternative text, and mark decorative ones as artifacts.",
    ),
    row(
        Ua1,
        "7.3",
        Some(1),
        "Figure has no alternative text",
        "Screen readers announce a Figure by reading its /Alt text. Without it, a blind reader only hears “image”, or nothing.",
        "Add an /Alt entry to the Figure element describing what the image shows. If it is purely decorative, mark it as an Artifact instead.",
    ),
    // ---- 7.4 headings ----
    row(
        Ua1,
        "7.4",
        None,
        "Headings are not structured properly",
        "People who cannot see the page jump between headings to navigate. Missing or out-of-order heading levels make that impossible.",
        "Use H1–H6 (or H) tags in order without skipping levels, and tag real headings as headings.",
    ),
    row(
        Ua1,
        "7.4.2",
        Some(1),
        "The first heading is not level 1",
        "Heading levels give the outline of the document. Starting at H2 leaves the top of the outline empty.",
        "Make the first heading H1, or renumber the headings so the outline starts at 1 and does not skip levels.",
    ),
    // ---- 7.5 tables ----
    row(
        Ua1,
        "7.5",
        None,
        "Table structure is incomplete",
        "A screen reader announces a cell together with its headers. A table without usable header information is read as a stream of unrelated values.",
        "Use TH cells for headers and give them a Scope (or link cells to headers with IDs).",
    ),
    row(
        Ua1,
        "7.5",
        Some(1),
        "Table header cells have no scope",
        "When a header cell does not say whether it heads a row or a column, a screen reader cannot tell which header belongs to which cell.",
        "Add a /Scope (Row, Column or Both) attribute to every TH cell, or link data cells to headers with IDs.",
    ),
    // ---- 7.9 notes ----
    row(
        Ua1,
        "7.9",
        None,
        "Notes and references are not set up",
        "Footnotes and endnotes must be reachable from the text that cites them, or a reader loses the connection.",
        "Tag notes as Note elements, give each a unique ID and link references to them.",
    ),
    row(
        Ua1,
        "7.9",
        Some(1),
        "A note has no ID",
        "A Note needs a unique ID so the reference that cites it can be linked to it. Without it the connection between text and note is lost.",
        "Give every Note element an /ID that no other element uses.",
    ),
    // ---- 7.18 annotations and links ----
    row(
        Ua1,
        "7.18",
        None,
        "Annotation is not accessible",
        "Links, comments and form widgets are only usable by keyboard and screen-reader users when they are tagged and described.",
        "Tag each annotation inside an Annot, Link or Form element and give it a description (/Contents or /Alt).",
    ),
    row(
        Ua1,
        "7.18.1",
        Some(2),
        "Annotation has no description",
        "A screen reader reads an annotation's description. With none, a comment or highlight is announced as an unnamed object.",
        "Add /Contents to the annotation or an /Alt to its structure element.",
    ),
    row(
        Ua1,
        "7.18.5",
        Some(1),
        "Link is not tagged as a Link",
        "Screen readers list the links on a page only when the link annotation sits inside a Link structure element.",
        "Put the link's annotation (and the text it covers) inside a Link element in the structure tree.",
    ),
    // ---- 7.20 XObjects ----
    row(
        Ua1,
        "7.20",
        Some(2),
        "Form content is not tied to the structure correctly",
        "Content that comes from a reusable form must be reached from the structure tree exactly once, or it is read twice or not at all.",
        "Reference the form's marked content from one structure element, and tag or hide the repeats.",
    ),
    // ---- 7.21 fonts ----
    row(
        Ua1,
        "7.21",
        None,
        "Font problem",
        "Fonts must be complete and carry enough information to reproduce and read the text on any machine.",
        "Embed the fonts and make sure every font has Unicode mapping information; re-export with font embedding on.",
    ),
    row(
        Ua1,
        "7.21.4.1",
        Some(1),
        "Font is not embedded",
        "A font that is not embedded is replaced by another one on the reader's computer, so the text can change shape, spacing or even characters.",
        "Re-export with 'embed all fonts' on, or embed the font in a PDF editor.",
    ),
    row(
        Ua1,
        "7.21.5",
        None,
        "Glyph widths disagree with the font",
        "If the widths written in the PDF do not match the embedded font, text spacing is wrong in some viewers.",
        "Re-export so the widths are generated from the embedded font.",
    ),
    row(
        Ua1,
        "7.21.7",
        None,
        "Characters cannot be turned into Unicode text",
        "Copying, searching and screen reading all need each glyph mapped to a Unicode character. Unmapped glyphs come out as gibberish or silence.",
        "Add a /ToUnicode map to the font, use a standard encoding, or re-export from a tool that writes the mapping.",
    ),
    // ---- PDF/UA-2 clause 8 ----
    row(
        Ua2,
        "8.2",
        None,
        "Logical structure problem",
        "The logical structure tells assistive technology what is on the page and in what order. Structure that breaks the rules is misread or ignored.",
        "Fix the structure tree with an accessibility editor, or re-export with tagging options on.",
    ),
    row(
        Ua2,
        "8.2.2",
        Some(1),
        "Content is neither tagged nor an artifact",
        "Everything on a page must be real content described by a structure element, or decoration marked as an Artifact. Anything else may be skipped or read in the wrong place.",
        "Tag the content to its structure element, or wrap it in an Artifact if it is decoration.",
    ),
    row(
        Ua2,
        "8.2.4",
        Some(1),
        "Structure type is not in a standard namespace",
        "Tags belong to namespaces so tools know what they mean. A type that reaches no recognised namespace cannot be interpreted.",
        "Map custom types through the RoleMap to a type in the PDF 1.7, PDF 2.0 or MathML namespace, or put the element in the right namespace.",
    ),
    row(
        Ua2,
        "8.2.5",
        None,
        "A structure type is used in a way PDF/UA-2 does not allow",
        "Each structure type has rules about where it may appear and what it must carry. Breaking them leaves assistive technology guessing.",
        "Check the element against the rules for its type and correct its attributes or its place in the tree.",
    ),
    row(
        Ua2,
        "8.2.5.12",
        Some(1),
        "Unnumbered heading (H) is used",
        "PDF 2.0 wants numbered headings so assistive technology knows each heading's level. An unnumbered H gives no outline position.",
        "Replace H with H1–H6 matching the heading's level.",
    ),
    row(
        Ua2,
        "8.2.5.20",
        Some(2),
        "One Link covers annotations with different targets",
        "A Link element is announced as one link. If its annotations go to different places the reader cannot know where activating it leads.",
        "Split the content into separate Link elements, one per destination.",
    ),
    row(
        Ua2,
        "8.2.5.25",
        None,
        "List structure is inconsistent",
        "Lists are announced with their item count and numbering. Inconsistent list attributes change what a reader hears.",
        "Make the list's numbering attribute match what is shown, and use L, LI, Lbl and LBody correctly.",
    ),
    row(
        Ua2,
        "8.2.5.26",
        None,
        "Table structure is not valid",
        "Screen readers read cells with their headers and navigate by row and column. Irregular tables break that navigation.",
        "Use a regular grid of TR, TH and TD, with spans that fit, and give header cells a Scope.",
    ),
    row(
        Ua2,
        "8.2.5.26",
        Some(5),
        "Table header cells have no scope",
        "When a header cell does not say whether it heads a row or a column, a screen reader cannot tell which header belongs to which cell.",
        "Add a /Scope attribute to every TH cell, or link data cells to headers with IDs.",
    ),
    row(
        Ua2,
        "8.2.5.28",
        None,
        "Figure needs a description",
        "Screen readers announce a Figure by reading its alternative text. Without it a blind reader hears nothing useful.",
        "Add /Alt (or /ActualText) to each meaningful Figure, and mark decorative ones as artifacts.",
    ),
    row(
        Ua2,
        "8.2.5.28.2",
        Some(1),
        "Figure has no alternative text",
        "Screen readers announce a Figure by reading its /Alt text. Without it, a blind reader only hears “image”, or nothing.",
        "Add an /Alt entry to the Figure element describing what the image shows. If it is purely decorative, mark it as an Artifact instead.",
    ),
];

/// The plain-language text for a validation rule, or `None` for a rule outside the table.
///
/// The lookup first tries the exact `(specification, clause, test)`, then the text of the clause
/// itself, then that of each enclosing clause (`7.21.4.1` falls back to `7.21.4`, `7.21`).
/// `specification` is the veraPDF specification string (`ISO 14289-1:2014`, `ISO 14289-2:2024`);
/// other specifications have no entries yet.
///
/// ```
/// let rule = pdfxray_core::validation::explain_rule("ISO 14289-1:2014", "7.3", 1);
/// assert_eq!(rule.map(|r| r.title), Some("Figure has no alternative text"));
/// ```
pub fn explain_rule(specification: &str, clause: &str, test: u32) -> Option<RuleText> {
    let spec = Spec::of(specification)?;
    let rows = || ROWS.iter().filter(move |r| r.spec == spec);
    let exact = rows().find(|r| r.clause == clause && r.test == Some(test));
    let found = exact.or_else(|| {
        let mut clause = clause;
        loop {
            if let Some(row) = rows().find(|r| r.clause == clause && r.test.is_none()) {
                break Some(row);
            }
            clause = clause.rsplit_once('.')?.0;
        }
    })?;
    Some(RuleText {
        title: found.title,
        why: found.why,
        how: found.how,
        exact: found.test.is_some(),
    })
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    const UA1: &str = "ISO 14289-1:2014";
    const UA2: &str = "ISO 14289-2:2024";

    #[test]
    fn the_design_rules_read_as_in_the_export() {
        let figure = explain_rule(UA1, "7.3", 1).unwrap();

        assert_eq!(figure.title, "Figure has no alternative text");
        assert_eq!(
            figure.why,
            "Screen readers announce a Figure by reading its /Alt text. Without it, a blind reader only hears “image”, or nothing."
        );
        assert_eq!(
            figure.how,
            "Add an /Alt entry to the Figure element describing what the image shows. If it is purely decorative, mark it as an Artifact instead."
        );
        assert!(figure.exact);
        assert_eq!(
            explain_rule(UA1, "7.1", 3).map(|r| r.title),
            Some("Content is neither tagged nor an artifact")
        );
        assert_eq!(
            explain_rule(UA1, "7.21.4.1", 1).map(|r| r.title),
            Some("Font is not embedded")
        );
    }

    #[test]
    fn every_test_of_clause_7_1_has_a_text_and_unverified_ones_are_clause_level() {
        for test in 1..=11 {
            assert!(explain_rule(UA1, "7.1", test).is_some(), "7.1-{test}");
        }
        let exact: Vec<u32> = (1..=11)
            .filter(|t| explain_rule(UA1, "7.1", *t).is_some_and(|r| r.exact))
            .collect();
        assert_eq!(exact, [1, 2, 3, 7, 8, 10]);
        assert_eq!(
            explain_rule(UA1, "7.1", 9).map(|r| r.title),
            Some("Tagged-PDF basics are not met")
        );
    }

    #[test]
    fn the_required_clauses_are_all_covered() {
        for (clause, test) in [
            ("5", 1),
            ("7.2", 34),
            ("7.2", 29),
            ("7.3", 1),
            ("7.4", 1),
            ("7.4.2", 1),
            ("7.4.4", 1),
            ("7.5", 1),
            ("7.9", 1),
            ("7.18", 1),
            ("7.18.1", 2),
            ("7.18.5", 1),
            ("7.21", 1),
            ("7.21.4.1", 1),
            ("7.21.7", 1),
        ] {
            assert!(explain_rule(UA1, clause, test).is_some(), "{clause}-{test}");
        }
    }

    #[test]
    fn deeper_clauses_fall_back_to_the_enclosing_clause() {
        assert_eq!(
            explain_rule(UA1, "7.4.4", 2).map(|r| (r.title, r.exact)),
            Some(("Headings are not structured properly", false))
        );
        assert_eq!(
            explain_rule(UA1, "7.21.4.2", 1).map(|r| (r.title, r.exact)),
            Some(("Font problem", false))
        );
        assert_eq!(
            explain_rule(UA2, "8.2.5.26", 3).map(|r| r.title),
            Some("Table structure is not valid")
        );
    }

    #[test]
    fn pdf_ua_2_rules_are_keyed_by_their_own_clauses() {
        assert_eq!(
            explain_rule(UA2, "8.2.5.28.2", 1).map(|r| r.title),
            Some("Figure has no alternative text")
        );
        assert_eq!(
            explain_rule(UA2, "8.2.4", 1).map(|r| r.title),
            Some("Structure type is not in a standard namespace")
        );
        assert_eq!(
            explain_rule(UA2, "7.3", 1),
            None,
            "a UA-1 clause is not a UA-2 clause"
        );
        assert_eq!(explain_rule(UA1, "8.2.2", 1), None);
    }

    #[test]
    fn other_specifications_and_unknown_clauses_have_no_text() {
        assert_eq!(explain_rule("ISO 19005-2:2011", "6.1", 1), None);
        assert_eq!(explain_rule(UA1, "7.99", 1), None);
        assert_eq!(explain_rule("", "7.3", 1), None);
    }

    #[test]
    fn every_row_is_written_out_and_none_repeats_a_key() {
        let mut keys = std::collections::BTreeSet::new();
        for r in ROWS {
            assert!(
                keys.insert((r.spec as u8, r.clause, r.test)),
                "{} {:?}",
                r.clause,
                r.test
            );
            assert!(
                !r.title.is_empty() && r.why.ends_with('.') && r.how.ends_with('.'),
                "{}",
                r.clause
            );
        }
        assert!(ROWS.len() >= 30, "{} rows", ROWS.len());
    }
}
