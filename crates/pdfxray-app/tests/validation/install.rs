//! Finding and installing veraPDF: the offer to install, failures, no Java, an explicit path and the file info.

use pretty_assertions::assert_eq;

use super::*;

#[test]
fn without_verapdf_the_tab_offers_to_install_it() {
    let fake = Arc::new(FakeValidator::new(Availability::NotInstalled));
    fake.run_gate.open();
    let mut harness = validation_tab(&fake, "orphan-mcid");

    // Why it cannot run, and what to do about it.
    assert!(has_label(&harness, "veraPDF was not found on this computer."));
    let _install = the(
        &harness,
        Region::Navigator,
        Role::Button,
        "Install veraPDF (≈34 MB download)",
    );
    assert!(
        the(&harness, Region::Navigator, Role::Button, "Validate")
            .accesskit_node()
            .is_disabled()
    );
    assert!(has_label(&harness, "veraPDF path:"));

    // Download: a progress bar with the numbers, then veraPDF is ready to use.
    the(
        &harness,
        Region::Navigator,
        Role::Button,
        "Install veraPDF (≈34 MB download)",
    )
    .click();
    wait_until(&mut harness, "the download to start", |s| {
        s.validation.install_progress() == Some((10_000_000, Some(34_000_000)))
    });
    assert!(has_label(&harness, "Installing veraPDF…"));
    assert!(
        harness
            .query_by_label("Install veraPDF (≈34 MB download)")
            .is_none()
    );
    let bars = nodes_in(&harness, Region::Navigator, Role::ProgressIndicator);
    assert_eq!(bars.len(), 1);

    fake.install_gate.open();
    wait_until(&mut harness, "the install to finish", |s| s.validation.is_ready());

    assert_eq!(
        harness.state().state.validation.environment,
        Environment::Known(Availability::Ready {
            version: "1.30.3".to_owned()
        })
    );
    assert_eq!(fake.calls(), [Call::Discover(None), Call::Install]);
    assert!(has_label(&harness, "Using veraPDF 1.30.3"));
    assert!(!has_label(&harness, "veraPDF was not found on this computer."));
    assert!(
        !the(&harness, Region::Navigator, Role::Button, "Validate")
            .accesskit_node()
            .is_disabled()
    );
}

#[test]
fn a_failed_install_says_why_and_offers_it_again() {
    let mut fake = FakeValidator::new(Availability::NotInstalled);
    fake.install_error = Some("SHA-256 mismatch for verapdf-installer.zip".to_owned());
    fake.install_gate.open();
    let mut harness = validation_tab(&Arc::new(fake), "orphan-mcid");

    the(
        &harness,
        Region::Navigator,
        Role::Button,
        "Install veraPDF (≈34 MB download)",
    )
    .click();
    wait_until(&mut harness, "the install to fail", |s| {
        s.validation.error.is_some()
    });

    assert!(has_label(
        &harness,
        "Installing veraPDF failed: SHA-256 mismatch for verapdf-installer.zip"
    ));
    let _again = the(
        &harness,
        Region::Navigator,
        Role::Button,
        "Install veraPDF (≈34 MB download)",
    );
    assert!(!harness.state().state.validation.is_ready());
}

#[test]
fn without_java_the_tab_explains_and_does_not_offer_the_download() {
    let fake = Arc::new(FakeValidator::new(Availability::JavaMissing));
    let harness = validation_tab(&fake, "orphan-mcid");

    assert!(has_label(
        &harness,
        "Java 11 or newer is required to run veraPDF and was not found. Install Java, then press Re-check."
    ));
    assert!(
        harness
            .query_by_label("Install veraPDF (≈34 MB download)")
            .is_none()
    );
    assert!(
        the(&harness, Region::Navigator, Role::Button, "Validate")
            .accesskit_node()
            .is_disabled()
    );
    let _recheck = the(&harness, Region::Navigator, Role::Button, "Re-check");
}

#[test]
fn an_explicit_path_is_tried_and_used() {
    let mut fake = FakeValidator::new(Availability::NotInstalled);
    fake.good_path = Some(PathBuf::from("/opt/verapdf"));
    fake.run_gate.open();
    let fake = Arc::new(fake);
    let mut harness = validation_tab(&fake, "orphan-mcid");

    // A wrong path changes nothing.
    type_path(&mut harness, "/opt/nowhere");
    the(&harness, Region::Navigator, Role::Button, "Use").click();
    wait_until(&mut harness, "the second search", |s| {
        s.validation.environment == Environment::Known(Availability::NotInstalled) && !s.validation.is_busy()
    });
    assert!(has_label(&harness, "veraPDF was not found on this computer."));

    // The right one makes veraPDF ready.
    type_path(&mut harness, "/opt/verapdf");
    the(&harness, Region::Navigator, Role::Button, "Use").click();
    wait_until(&mut harness, "veraPDF to be found", |s| s.validation.is_ready());

    assert_eq!(
        fake.calls(),
        [
            Call::Discover(None),
            Call::Discover(Some(PathBuf::from("/opt/nowhere"))),
            Call::Discover(Some(PathBuf::from("/opt/verapdf")))
        ]
    );
    assert!(has_label(&harness, "Using veraPDF 1.30.3"));
    assert!(
        !the(&harness, Region::Navigator, Role::Button, "Validate")
            .accesskit_node()
            .is_disabled()
    );
}

/// Replaces the text of the veraPDF location field.
fn type_path(harness: &mut Harness<'static, PdfxrayApp>, path: &str) {
    // The second field of the navigator: the first one is the filter.
    nodes_in(harness, Region::Navigator, Role::TextInput)
        .remove(1)
        .focus();
    harness.run();
    harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::A);
    nodes_in(harness, Region::Navigator, Role::TextInput)
        .remove(1)
        .type_text(path);
    harness.run();
}

#[test]
fn the_file_info_hover_says_which_verapdf_was_found() {
    let fake = FakeValidator::ready();
    let mut harness = validation_tab(&fake, "orphan-mcid");

    the(&harness, Region::Chrome, Role::Button, "file info").hover();
    harness.run_steps(4);

    assert!(
        has_label(&harness, "veraPDF 1.30.3"),
        "{:?}",
        common::all_label_texts(&harness)
    );
    // Smoke: the bar itself has no veraPDF item any more.
    assert!(
        !labels_in(&harness, Region::Chrome)
            .iter()
            .any(|t| t.starts_with("veraPDF"))
    );
}

#[test]
fn the_file_info_card_sets_each_icon_beside_its_text_on_evenly_spaced_lines() {
    let fake = FakeValidator::ready();
    let mut harness = validation_tab(&fake, "orphan-mcid");

    the(&harness, Region::Chrome, Role::Button, "file info").hover();
    harness.run_steps(4);

    let line = |starts: &str| {
        harness
            .query_all_by_role(Role::Label)
            .find(|node| node.value().is_some_and(|t| t.starts_with(starts)))
            .unwrap_or_else(|| panic!("the line {starts}"))
            .rect()
    };
    let (counts, opened, vera) = (line("50 tagged"), line("opened in"), line("veraPDF"));
    // The three lines share a left edge: each has its icon in front, on the same line.
    assert_eq!(counts.min.x, opened.min.x);
    assert_eq!(opened.min.x, vera.min.x);
    // They are stacked a line apart, none of them a card's height away.
    let (first, second) = (opened.min.y - counts.min.y, vera.min.y - opened.min.y);
    assert_eq!(first, second);
    assert!((14.0..=26.0).contains(&first), "{first}");
}
