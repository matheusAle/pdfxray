//! A file whose cross-reference table is damaged: a banner above the canvas that says so, and a
//! button to the diagnostics.

use egui::{Align, Frame, Layout, Margin, RichText, Stroke, Ui};
use pdfxray_core::{DiagnosticInfo, DocInfo, Severity};

use crate::components::{Icon, ToolButton};
use crate::state::{AppState, DrawerTab};
use crate::theme::{fonts, tokens};

/// Headline of the banner.
pub const TITLE: &str = "This file's index is damaged";

/// The diagnostic code of an object that is not where the xref says.
const MOVED: &str = "xref-offset-wrong";

/// What is wrong with the file's index.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Damage {
    /// Objects that are not where the xref says.
    pub moved: usize,
    /// Every diagnostic of the file: what "View problems" lists.
    pub problems: usize,
}

/// True for a diagnostic about the xref table or the `startxref` pointer that is a warning or an
/// error: the reader had to find objects another way.
pub fn damages_index(diagnostic: &DiagnosticInfo) -> bool {
    diagnostic.severity >= Severity::Warning && diagnostic.code.contains("xref")
}

/// The damage `info` shows, or `None` when the index is sound.
pub fn damage(info: &DocInfo) -> Option<Damage> {
    info.diagnostics.iter().any(damages_index).then(|| Damage {
        moved: info.diagnostics.iter().filter(|d| d.code == MOVED).count(),
        problems: info.diagnostics.len(),
    })
}

impl Damage {
    /// What the banner says under the headline.
    pub fn message(&self) -> String {
        match self.moved {
            0 => "The xref table or the startxref pointer is wrong. Other viewers silently rebuild it; \
                  pdfxray shows the damage."
                .to_owned(),
            1 => "1 object isn't where the xref says. Other viewers silently search for it; pdfxray \
                  shows the damage."
                .to_owned(),
            n => format!(
                "{n} objects aren't where the xref says. Other viewers silently search for them; pdfxray \
                 shows the damage."
            ),
        }
    }

    /// The text of the button: `View 12 problems`.
    pub fn button(&self) -> String {
        format!(
            "View {} problem{}",
            self.problems,
            if self.problems == 1 { "" } else { "s" }
        )
    }
}

impl AppState {
    /// Opens the Diagnostics tab with the worst problems first.
    pub fn show_problems(&mut self) {
        self.drawer.show_tab(DrawerTab::Diagnostics);
        self.drawer.page_boxes_only = false;
        self.drawer.sorted_by_severity = true;
    }
}

/// Draws the banner when the open document has a damaged index.
pub fn banner(ui: &mut Ui, state: &mut AppState) {
    let Some(damage) = state.document.as_ref().and_then(|loaded| damage(&loaded.info)) else {
        return;
    };
    let mut view = false;
    Frame::new()
        .fill(tokens::WARN.gamma_multiply(0.10))
        .stroke(Stroke::new(1.0, tokens::WARN.gamma_multiply(0.30)))
        .inner_margin(Margin::symmetric(16, 8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            let label = damage.button();
            let button = ToolButton::new(&label).secondary();
            let text_width = ui.available_width() - button.width(ui) - 16.0 - 2.0 * 12.0;
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 12.0;
                Icon::Warning.show(ui, 16.0, tokens::WARN);
                ui.vertical(|ui| {
                    ui.set_max_width(text_width);
                    ui.spacing_mut().item_spacing.y = 1.0;
                    ui.label(
                        RichText::new(TITLE)
                            .font(fonts::semibold(tokens::text::BODY))
                            .color(tokens::TEXT),
                    );
                    ui.label(
                        RichText::new(damage.message())
                            .font(fonts::ui(tokens::text::BODY))
                            .color(tokens::TEXT_2),
                    );
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    view = button.show(ui).clicked();
                });
            });
        });
    if view {
        state.show_problems();
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;
    use crate::test_support::fixture;

    fn damage_of(name: &str) -> Option<Damage> {
        let doc = fixture(name);
        damage(&doc.info())
    }

    #[test]
    fn moved_objects_and_a_wrong_startxref_are_damage_and_a_sound_file_is_not() {
        assert_eq!(
            damage_of("broken-xref-offsets"),
            Some(Damage {
                moved: 2,
                problems: 2
            })
        );
        assert_eq!(
            damage_of("startxref-broken"),
            Some(Damage {
                moved: 0,
                problems: 1
            })
        );
        assert_eq!(damage_of("minimal-one-page"), None);
        assert_eq!(damage_of("incremental-update"), None);
    }

    #[test]
    fn the_banner_counts_what_it_found() {
        let two = Damage {
            moved: 2,
            problems: 2,
        };
        let one = Damage {
            moved: 1,
            problems: 1,
        };

        assert_eq!(
            two.message(),
            "2 objects aren't where the xref says. Other viewers silently search for them; pdfxray shows the damage."
        );
        assert_eq!(two.button(), "View 2 problems");
        assert_eq!(
            one.message(),
            "1 object isn't where the xref says. Other viewers silently search for it; pdfxray shows the damage."
        );
        assert_eq!(one.button(), "View 1 problem");
    }
}
