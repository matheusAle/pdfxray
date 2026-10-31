//! Helpers shared by the `validation` tests.

use pretty_assertions::assert_eq;

use super::*;

pub(crate) fn app_with(fake: &Arc<FakeValidator>) -> Harness<'static, PdfxrayApp> {
    let mut harness = app();
    harness.state_mut().state.validator = fake.clone();
    harness
}

/// The Validation tab of `fixture`, once the fake has been asked and answered.
pub(crate) fn validation_tab(fake: &Arc<FakeValidator>, fixture: &str) -> Harness<'static, PdfxrayApp> {
    let mut harness = app_with(fake);
    open_fixture(&mut harness, fixture);
    // The search shows a spinner, which keeps repainting: step instead of running to rest.
    the(&harness, Region::Rail, Role::Button, "Validation").click();
    wait_until(&mut harness, "the search for veraPDF", |s| {
        matches!(s.validation.environment, Environment::Known(_))
    });
    harness
}

pub(crate) fn validated(harness: &Harness<'_, PdfxrayApp>) -> bool {
    harness
        .state()
        .state
        .document
        .as_ref()
        .unwrap()
        .doc
        .validation()
        .is_some()
}

pub(crate) fn run_validation(harness: &mut Harness<'static, PdfxrayApp>) {
    the(harness, Region::Navigator, Role::Button, "Validate").click();
    wait_until(harness, "the validation result", |s| {
        s.document.as_ref().unwrap().doc.validation().is_some() && !s.validation.is_running()
    });
}

/// The names of the buttons of the navigator's list, after the header's Re-run.
pub(crate) fn findings_list(harness: &Harness<'_, PdfxrayApp>) -> Vec<String> {
    let mut buttons = texts_in(harness, Region::Navigator, Role::Button);
    assert_eq!(buttons.remove(0), "Re-run");
    buttons
}

/// The text of the labels of `region`.
pub(crate) fn labels_in(harness: &Harness<'_, PdfxrayApp>, region: Region) -> Vec<String> {
    texts_in(harness, region, Role::Label)
}

/// Presses the Validate button of the title bar and waits for the report.
pub(crate) fn title_bar_validate(harness: &mut Harness<'static, PdfxrayApp>) {
    the(harness, Region::Chrome, Role::Button, "Validate").click();
    wait_until(harness, "the validation result", |s| {
        s.document.as_ref().unwrap().doc.validation().is_some() && !s.validation.is_running()
    });
}

/// Rests the pointer on the chip of the "On this page" card, which opens it.
pub(crate) fn open_page_card(harness: &mut Harness<'_, PdfxrayApp>) {
    the(harness, Region::Center, Role::Label, "On this page").hover();
    harness.run_steps(4);
}
