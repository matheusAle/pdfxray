//! The PRINT BOXES section: the bleed, the trim size and how far the art sits inside the trim,
//! read from the page's boxes. Lengths are in the unit of the status bar and, in brackets or
//! after a dot, in millimetres (inches when the unit is millimetres) as print shops quote them.

use egui::Ui;
use pdfxray_core::{BoxKind, BoxSource, PageInfo, Rect};

use crate::components::{PropRow, section};
use crate::units::{Lengths, Unit};

/// Title of the section.
pub const TITLE: &str = "Print boxes";

/// The unit a length is quoted in next to the one of the status bar.
const fn alternative(unit: Unit) -> Unit {
    match unit {
        Unit::Mm => Unit::In,
        Unit::Pt | Unit::In | Unit::Px => Unit::Mm,
    }
}

/// The rows of the section: bleed, trim size and safe area.
pub fn rows(info: &PageInfo, lengths: &Lengths) -> [(&'static str, String); 3] {
    let figures = info.print_figures();
    let other = Lengths::new(alternative(lengths.unit), lengths.user_unit);

    let bleed = match (figures.uniform_bleed, figures.bleed_each_side) {
        (Some(each), _) => format!("{} ({}) each side", lengths.format(each), other.format(each)),
        (None, Some([left, bottom, right, top])) => format!(
            "uneven · left {} · bottom {} · right {} · top {}",
            lengths.number(left),
            lengths.number(bottom),
            lengths.number(right),
            lengths.format(top)
        ),
        (None, None) => "not set".to_owned(),
    };
    let (width, height) = figures.trim_size;
    let trim = Rect::new(0.0, 0.0, width, height);
    let trim_size = format!("{} · {}", lengths.size(trim), other.size(trim));
    let art_defined = info
        .page_box(BoxKind::Art)
        .is_some_and(|b| !matches!(b.source, BoxSource::Default(_)));
    let safe = match figures.art_inset_in_trim {
        Some(inset) => format!("Art is {} inside Trim", lengths.format(inset)),
        None if art_defined => "Art is not evenly inside Trim".to_owned(),
        None => "no ArtBox set".to_owned(),
    };
    [("Bleed", bleed), ("Trim size", trim_size), ("Safe area", safe)]
}

/// Draws the section for `info`.
pub(super) fn show(ui: &mut Ui, info: &PageInfo, lengths: &Lengths) {
    section::show(ui, TITLE, TITLE, |ui| {
        for (key, value) in rows(info, lengths) {
            PropRow::new(key, &value).show(ui);
        }
    });
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;
    use crate::test_support::fixture;

    fn page(name: &str, ix: u32) -> PageInfo {
        fixture(name).page(ix).unwrap().clone()
    }

    #[test]
    fn the_framed_page_has_a_bleed_a_trim_size_and_a_safe_area() {
        let framed = page("page-boxes", 0);

        assert_eq!(
            rows(&framed, &Lengths::new(Unit::Pt, 1.0)),
            [
                ("Bleed", "10 pt (3.53 mm) each side".to_owned()),
                ("Trim size", "340 × 240 pt · 119.94 × 84.67 mm".to_owned()),
                ("Safe area", "Art is 10 pt inside Trim".to_owned()),
            ]
        );
        // In millimetres the second figure is in inches.
        assert_eq!(
            rows(&framed, &Lengths::new(Unit::Mm, 1.0)),
            [
                ("Bleed", "3.53 mm (0.139 in) each side".to_owned()),
                ("Trim size", "119.94 × 84.67 mm · 4.722 × 3.333 in".to_owned()),
                ("Safe area", "Art is 3.53 mm inside Trim".to_owned()),
            ]
        );
    }

    #[test]
    fn boxes_the_page_does_not_set_say_so() {
        let plain = page("page-boxes", 1);

        assert_eq!(
            rows(&plain, &Lengths::new(Unit::Pt, 1.0)),
            [
                ("Bleed", "not set".to_owned()),
                ("Trim size", "300 × 400 pt · 105.83 × 141.11 mm".to_owned()),
                ("Safe area", "no ArtBox set".to_owned()),
            ]
        );
    }
}
