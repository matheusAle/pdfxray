//! Runs: the UA claim of the title bar, profile changes, another document, progress and cancelling.

use pretty_assertions::assert_eq;

use super::*;

#[test]
fn the_title_bar_claims_a_ua_level_only_after_a_run_proved_it() {
    let mut fake = FakeValidator::new(Availability::Ready {
        version: "1.30.3".to_owned(),
    });
    let xml = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../pdfxray-verapdf/tests/data/ua1-compliant.xml"
    ))
    .unwrap();
    fake.report = pdfxray_verapdf::parse_report(&xml).unwrap();
    fake.run_gate.open();
    let mut harness = app_with(&Arc::new(fake));
    open_fixture(&mut harness, "tagged-simple");

    // Before a run the file only says it is tagged: its own claim is not read.
    let _chip = the(
        &harness,
        Region::Chrome,
        Role::Button,
        "PDF 1.7 · Tagged · rev 1/1",
    );

    title_bar_validate(&mut harness);

    // The verdict replaces the tagged run: UA-1 is proven.
    let _chip = the(&harness, Region::Chrome, Role::Button, "PDF 1.7 · UA-1 · rev 1/1");
    assert!(
        !texts_in(&harness, Region::Chrome, Role::Button)
            .iter()
            .any(|l| l.contains("Tagged"))
    );
    the(&harness, Region::Chrome, Role::Button, "PDF 1.7 · UA-1 · rev 1/1").click();
    harness.run();
    let labels = common::all_label_texts(&harness);
    let at = labels.iter().position(|t| t == "PDF/UA").unwrap();
    assert_eq!(labels[at + 1], "UA-1 · compliant");
    harness.key_press(Key::Escape);
    harness.run();
    assert_eq!(
        harness.state().state.status,
        "Validated against PDF/UA-1: compliant"
    );
}

#[test]
fn changing_the_profile_after_a_run_runs_again_with_the_chosen_one() {
    let fake = FakeValidator::ready();
    let mut harness = app_with(&fake);
    open_fixture(&mut harness, "orphan-mcid");
    title_bar_validate(&mut harness);
    assert_eq!(fake.calls().len(), 2);

    harness.get_by_label("Profile").click();
    harness.run();
    harness.get_by_role_and_label(Role::Button, "PDF/UA-2").click();
    // The run shows a spinner, which never lets the app settle: step instead.
    harness.run_steps(3);
    wait_until(&mut harness, "the second run", |s| {
        !s.validation.is_running() && s.status == "Validated against PDF/UA-2: not compliant"
    });

    assert_eq!(
        fake.calls().last(),
        Some(&Call::Validate(fixture_path("orphan-mcid"), Profile::PdfUa2))
    );
    assert_eq!(fake.calls().len(), 3);
    assert_eq!(
        harness.get_by_label("Profile").value().as_deref(),
        Some("PDF/UA-2 · ISO 14289-2")
    );
    // The same profile again does not run twice.
    harness.get_by_label("Profile").click();
    harness.run_steps(4);
    harness.get_by_role_and_label(Role::Button, "PDF/UA-2").click();
    harness.run();
    assert_eq!(fake.calls().len(), 3);
}

#[test]
fn opening_another_document_drops_the_results_and_abandons_a_run() {
    let fake = FakeValidator::new(Availability::Ready {
        version: "1.30.3".to_owned(),
    });
    let fake = Arc::new(fake);
    let mut harness = validation_tab(&fake, "orphan-mcid");
    the(&harness, Region::Navigator, Role::Button, "Validate").click();
    wait_until(&mut harness, "the run to start", |s| s.validation.is_running());

    open_fixture(&mut harness, "page-boxes");

    assert!(!harness.state().state.validation.is_running());
    assert_eq!(harness.state().state.status, pdfxray_app::READY);
    assert_eq!(
        harness
            .state()
            .state
            .document
            .as_ref()
            .map(|d| d.info.name.as_str()),
        Some("page-boxes.pdf")
    );
    // The abandoned run was told to stop and its result never reaches the new document.
    fake.run_gate.open();
    harness.run_steps(10);
    std::thread::sleep(std::time::Duration::from_millis(30));
    harness.run_steps(3);
    assert!(!validated(&harness));
    assert!(has_label(
        &harness,
        "Choose a profile and press Validate to check this file."
    ));
}

#[test]
fn a_run_shows_progress_and_can_be_cancelled() {
    let fake = FakeValidator::new(Availability::Ready {
        version: "1.30.3".to_owned(),
    });
    fake.install_gate.open();
    let fake = Arc::new(fake);
    let mut harness = validation_tab(&fake, "orphan-mcid");

    the(&harness, Region::Navigator, Role::Button, "Validate").click();
    wait_until(&mut harness, "the run to start", |s| s.validation.is_running());

    // While it runs: a status line and Cancel instead of Validate; the profile cannot change.
    assert!(has_label(&harness, "Running PDF/UA-1…"));
    assert!(named(&harness, Region::Navigator, Role::Button, "Validate").is_empty());
    assert!(harness.get_by_label("Profile").accesskit_node().is_disabled());
    assert_eq!(harness.state().state.status, "Validating against PDF/UA-1…");

    the(&harness, Region::Navigator, Role::Button, "Cancel").click();
    wait_until(&mut harness, "the run to stop", |s| !s.validation.is_running());

    assert_eq!(harness.state().state.status, "Validation cancelled");
    assert!(!validated(&harness));
    assert!(has_label(
        &harness,
        "Choose a profile and press Validate to check this file."
    ));
    assert!(!has_label(&harness, "Running PDF/UA-1…"));
    // Smoke: the user can start again, and the second run completes once the gate opens.
    fake.run_gate.open();
    run_validation(&mut harness);
    assert!(validated(&harness));
}
