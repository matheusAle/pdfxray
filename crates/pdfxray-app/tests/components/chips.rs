//! Chips.

use pretty_assertions::assert_eq;

use super::*;

#[test]
fn chips_carry_their_text_and_tone_and_are_no_buttons() {
    let mut harness = builder(vec2(420.0, 60.0)).build_ui(|ui| {
        on_surface(ui, |ui| {
            ui.horizontal(|ui| {
                Chip::new("/P").show(ui);
                Chip::new("glyph").dot(tokens::KIND_CONTENT).show(ui);
                Chip::new("Tagged").tone(Tone::Ok).show(ui);
                Chip::new("rev 3 of 3").tone(Tone::Warn).show(ui);
            });
        });
    });
    harness.run();

    for text in ["/P", "glyph", "Tagged", "rev 3 of 3"] {
        let _chip = harness.get_by_role_and_label(Role::Label, text);
        assert!(
            harness.query_by_role_and_label(Role::Button, text).is_none(),
            "{text}"
        );
    }
    // Smoke: a chip is a label, so a panel that wants a control builds one around it.
    assert!(harness.query_all_by_role(Role::Button).next().is_none());

    // Tones are tints of their token: the text is the colour, the fill and border are fainter.
    let (fill, border, ink) = Tone::Kind(tokens::KIND_TAG).colors();
    assert_eq!(ink, tokens::KIND_TAG);
    assert!(fill.a() < border.a() && border.a() < ink.a());
    assert_eq!(Tone::Ok.colors().2, tokens::OK);
    assert_eq!(Tone::Warn.colors().2, tokens::WARN);
    assert_eq!(Tone::Err.colors().2, tokens::ERR);
    assert_eq!(Tone::Neutral.colors().0, tokens::SURFACE_2);
}
