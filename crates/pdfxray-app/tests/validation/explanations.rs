//! What the inspector says about a finding: the plain words, the source with an example fix, the rule docs.

use pretty_assertions::assert_eq;

use super::*;

#[test]
fn the_inspector_explains_the_finding_in_plain_words() {
    let fake = FakeValidator::ready();
    let mut harness = app_with(&fake);
    open_fixture(&mut harness, "orphan-mcid");
    title_bar_validate(&mut harness);

    the(&harness, Region::Navigator, Role::Button, "MCID 1 p1").click();
    harness.run();

    let labels = labels_in(&harness, Region::Inspector);
    for expected in [
        "Content is neither tagged nor an artifact",
        "PDF/UA-1 · rule 7.1-3 · 1 of 1",
        "Everything on a page must be real content described by a structure element, or decoration marked as an Artifact. Anything else may be skipped by a screen reader or read in the wrong place.",
        "Tag the content to its structure element, or wrap it in an Artifact (/Artifact BMC … EMC) if it is decoration such as a rule, a background or a page number.",
    ] {
        assert!(labels.contains(&expected.to_owned()), "{expected} in {labels:?}");
    }
    // The text is the rule's own, not a general note.
    assert!(!labels.iter().any(|t| t.starts_with("General guidance")));
    // Where: the content line and the end of veraPDF's path.
    assert!(
        labels
            .iter()
            .any(|t| t.starts_with("MCID 1 · ") && t.ends_with(" · page 1")),
        "{labels:?}"
    );
    assert!(
        labels.contains(&".../contentItem[0]{mcid:1}".to_owned()),
        "{labels:?}"
    );
    // veraPDF's own words stay available below, past the fold of a 900 px window.
    harness
        .get_by_role_and_label(Role::Button, "veraPDF says")
        .scroll_to_me();
    harness.run_steps(3);
    let labels = labels_in(&harness, Region::Inspector);
    assert!(labels.contains(&"Content shall be marked as Artifact or tagged as real content".to_owned()));
    // The footer has the finding buttons, not the object buttons.
    let buttons = texts_in(&harness, Region::Inspector, Role::Button);
    for expected in ["Previous", "Rule docs", "Next finding"] {
        assert!(
            buttons.contains(&expected.to_owned()),
            "{expected} in {buttons:?}"
        );
    }
    // Other entries show the node's properties again, and the finding bar is gone.
    click_rail(&mut harness, "Tags");
    let buttons = texts_in(&harness, Region::Inspector, Role::Button);
    assert!(buttons.contains(&"Pin".to_owned()), "{buttons:?}");
    assert!(!buttons.contains(&"Next finding".to_owned()));
}

#[test]
fn a_figure_without_alt_shows_its_source_with_an_example_fix_that_is_never_applied() {
    let mut fake = FakeValidator::new(Availability::Ready {
        version: "1.30.3".to_owned(),
    });
    fake.report = struct_rich_report();
    fake.run_gate.open();
    let mut harness = app_with(&Arc::new(fake));
    open_fixture(&mut harness, "struct-rich");
    title_bar_validate(&mut harness);

    // 12 0 R has no /Alt: the example adds one.
    the(&harness, Region::Navigator, Role::Button, "H1 12 0 R p1").click();
    harness.run();
    let labels = labels_in(&harness, Region::Inspector);
    assert!(labels.contains(&" 12 0 obj".to_owned()), "{labels:?}");
    assert!(
        labels.contains(&"+  /Alt (Describe what the figure shows)".to_owned()),
        "{labels:?}"
    );
    // The note under the code card sits past the fold of a 900 px window.
    harness
        .get_by_role_and_label(Role::Button, "Where")
        .scroll_to_me();
    harness.run_steps(3);
    let labels = labels_in(&harness, Region::Inspector);
    assert!(labels.contains(&"Example only. pdfxray never changes your file.".to_owned()));
    // The element of the finding links to itself in the Where section.
    assert!(labels.contains(&"/H1 12 0 R".to_owned()), "{labels:?}");

    // 14 0 R has one: the source is shown without a fix.
    the(&harness, Region::Navigator, Role::Button, "Figure 14 0 R p1").click();
    harness.run();
    let labels = labels_in(&harness, Region::Inspector);
    assert!(labels.contains(&" 14 0 obj".to_owned()), "{labels:?}");
    assert!(!labels.iter().any(|t| t.starts_with('+')), "{labels:?}");
    assert!(!labels.contains(&"Example only. pdfxray never changes your file.".to_owned()));
    // The file on disk and in memory is untouched.
    let doc = &harness.state().state.document.as_ref().unwrap().doc;
    assert_eq!(doc.bytes(), std::fs::read(fixture_path("struct-rich")).unwrap());
}

#[test]
fn a_rule_without_its_own_note_says_it_is_general_guidance_for_the_clause() {
    let mut fake = FakeValidator::new(Availability::Ready {
        version: "1.30.3".to_owned(),
    });
    // 7.1-3 becomes 7.1-4, which has no text of its own: the 7.1 text answers.
    fake.report.findings[3].rule.test_number = 4;
    fake.run_gate.open();
    let mut harness = app_with(&Arc::new(fake));
    open_fixture(&mut harness, "orphan-mcid");
    title_bar_validate(&mut harness);

    the(&harness, Region::Navigator, Role::Button, "MCID 1 p1").click();
    harness.run();

    let labels = labels_in(&harness, Region::Inspector);
    assert!(
        labels.contains(&"Tagged-PDF basics are not met".to_owned()),
        "{labels:?}"
    );
    assert!(
        labels.contains(&"General guidance for clause 7.1".to_owned()),
        "{labels:?}"
    );
}

#[test]
fn rule_docs_opens_the_rules_of_the_standard_in_the_browser() {
    let fake = FakeValidator::ready();
    let mut harness = app_with(&fake);
    open_fixture(&mut harness, "orphan-mcid");
    title_bar_validate(&mut harness);
    the(&harness, Region::Navigator, Role::Button, "MCID 1 p1").click();
    harness.run();

    the(&harness, Region::Inspector, Role::Button, "Rule docs").click();
    let mut urls = Vec::new();
    for _ in 0..4 {
        harness.step();
        urls.extend(
            harness
                .output()
                .platform_output
                .commands
                .iter()
                .filter_map(|command| match command {
                    egui::OutputCommand::OpenUrl(open) => Some(open.url.clone()),
                    _ => None,
                }),
        );
    }

    assert_eq!(
        urls,
        ["https://github.com/veraPDF/veraPDF-validation-profiles/wiki/PDFUA-Part-1-rules"]
    );
}
