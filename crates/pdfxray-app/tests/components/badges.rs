//! Badges.

use super::*;

#[test]
fn a_badge_shows_its_count() {
    let harness = builder(vec2(200.0, 50.0)).build_ui(|ui| {
        on_surface(ui, |ui| {
            ui.horizontal(|ui| {
                Badge::new(5).show(ui);
                Badge::new(377).tone(Tone::Warn).show(ui);
            });
        });
    });

    let _five = harness.get_by_role_and_label(Role::Label, "5");
    let _many = harness.get_by_role_and_label(Role::Label, "377");
}
