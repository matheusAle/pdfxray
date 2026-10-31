//! Print figures.

use pretty_assertions::assert_eq;

use super::*;

#[test]
fn print_figures_of_the_page_box_fixtures_are_exact() {
    let doc = open("page-boxes");
    let all = open("box-matrix");

    let framed = doc.page(0).unwrap().print_figures();
    let plain = doc.page(1).unwrap().print_figures();
    let small = doc.page(2).unwrap().print_figures();

    assert_eq!(
        (
            framed.bleed_each_side,
            framed.uniform_bleed,
            framed.trim_size,
            framed.art_inset_in_trim
        ),
        (
            Some([10.0, 10.0, 10.0, 10.0]),
            Some(10.0),
            (340.0, 240.0),
            Some(10.0)
        )
    );
    assert_eq!(
        framed.paper_name, None,
        "280 x 380 after the 90 degree rotation is no named paper"
    );
    assert_eq!(
        (
            plain.bleed_each_side,
            plain.uniform_bleed,
            plain.trim_size,
            plain.art_inset_in_trim
        ),
        (None, None, (300.0, 400.0), None),
        "no BleedBox of its own, and the TrimBox defaults to the crop"
    );
    assert_eq!(small.trim_size, (100.0, 100.0));
    let matrix0 = all.page(0).unwrap().print_figures();
    assert_eq!(
        (
            matrix0.bleed_each_side,
            matrix0.trim_size,
            matrix0.art_inset_in_trim
        ),
        (Some([10.0; 4]), (560.0, 752.0), Some(10.0))
    );
    assert_eq!(all.page(3).unwrap().print_figures().trim_size, (230.0, 280.0));
    assert!((0..all.page_count()).all(|p| {
        all.page(u32::try_from(p).unwrap())
            .unwrap()
            .print_figures()
            .paper_name
            .is_none()
    }));
}

#[test]
fn print_figures_of_the_design_page_name_the_paper_and_the_insets() {
    let doc = single_page(
        "/MediaBox [0 0 612 792] /BleedBox [9 9 603 783] /TrimBox [18 18 594 774] /ArtBox [36 36 576 756]",
        "",
        |_| {},
    );

    let figures = doc.page(0).unwrap().print_figures();

    assert_eq!(figures.bleed_each_side, Some([9.0; 4]));
    assert_eq!(figures.uniform_bleed, Some(9.0));
    assert_eq!(figures.trim_size, (576.0, 756.0));
    assert_eq!(figures.art_inset_in_trim, Some(18.0));
    assert_eq!(figures.paper_name, Some("US Letter"));
}

#[test]
fn uneven_bleed_and_insets_have_no_single_figure_and_paper_names_ignore_orientation() {
    let uneven = single_page(
        "/MediaBox [0 0 595 842] /BleedBox [5 5 590 842] /TrimBox [10 10 585 832] /ArtBox [20 15 575 822]",
        "",
        |_| {},
    );
    let landscape = single_page("/MediaBox [0 0 842 595]", "", |_| {});
    let odd = single_page("/MediaBox [0 0 600 800]", "", |_| {});

    let figures = uneven.page(0).unwrap().print_figures();

    assert_eq!(figures.bleed_each_side, Some([5.0, 5.0, 5.0, 10.0]));
    assert_eq!(figures.uniform_bleed, None);
    assert_eq!(figures.art_inset_in_trim, None, "10 on the left, 5 at the bottom");
    assert_eq!(figures.paper_name, Some("A4"));
    assert_eq!(landscape.page(0).unwrap().print_figures().paper_name, Some("A4"));
    assert_eq!(odd.page(0).unwrap().print_figures().paper_name, None);
}
