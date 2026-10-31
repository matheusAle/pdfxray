//! Icons.

use pretty_assertions::assert_eq;

use super::*;

#[test]
fn every_icon_renders_without_a_missing_glyph() {
    let mut harness = builder(vec2(480.0, 80.0)).build_ui(|ui| {
        ui.horizontal_wrapped(|ui| {
            for icon in Icon::ALL {
                icon.show(ui, 16.0, tokens::TEXT);
            }
        });
    });
    harness.run();

    // No icon is registered twice.
    assert_eq!(Icon::ALL.iter().collect::<HashSet<_>>().len(), Icon::ALL.len());
}
