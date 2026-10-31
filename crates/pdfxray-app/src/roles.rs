//! Tag role colours: one colour per [`RoleClass`], shared by the Tags tree dots, the tag boxes,
//! labels and reading-order badges on the canvas, the Tag roles legend and the x-ray's *Tag role*
//! colouring, so a role reads the same everywhere.
//!
//! They are the design's `role-*` tokens (artifacts keep their kind colour), the same colours as
//! the render crate's [`pdfxray_render::ROLE_COLORS`].

use egui::Color32;
use pdfxray_core::RoleClass;

use crate::theme::tokens;

/// The colour of `class`. Every place that draws a role uses this one function.
pub const fn role_color(class: RoleClass) -> Color32 {
    match class {
        RoleClass::Heading => tokens::ROLE_HEADING,
        RoleClass::Paragraph => tokens::ROLE_P,
        RoleClass::Figure => tokens::ROLE_FIGURE,
        RoleClass::Caption => tokens::ROLE_CAPTION,
        RoleClass::Table => tokens::ROLE_TABLE,
        RoleClass::List => tokens::ROLE_LIST,
        RoleClass::Link => tokens::ROLE_LINK,
        RoleClass::Inline => tokens::ROLE_INLINE,
        RoleClass::Group => tokens::ROLE_GROUP,
        RoleClass::Artifact => tokens::KIND_ARTIFACT,
        RoleClass::Other => tokens::ROLE_OTHER,
    }
}

/// How the legend names `class`: the tags it stands for, as in the design.
pub const fn class_name(class: RoleClass) -> &'static str {
    match class {
        RoleClass::Heading => "H1–H6",
        RoleClass::Paragraph => "P",
        RoleClass::Figure => "Figure",
        RoleClass::Caption => "Caption",
        RoleClass::Table => "Table…",
        RoleClass::List => "L / LI",
        RoleClass::Link => "Link",
        RoleClass::Inline => "Span",
        RoleClass::Group => "Sect / Div",
        RoleClass::Artifact => "Artifact",
        RoleClass::Other => "Other",
    }
}

#[cfg(test)]
mod tests {
    use pdfxray_render::{ROLE_COLORS, role_slot};
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn the_canvas_and_the_x_ray_paint_a_role_in_the_same_colour() {
        // The x-ray is rendered by the render crate; the boxes, dots and badges by the app.
        for class in RoleClass::ALL {
            assert_eq!(
                role_color(class).to_srgba_unmultiplied(),
                ROLE_COLORS[role_slot(class)],
                "{class:?}"
            );
        }
        // Smoke: the heading is the design's violet.
        assert_eq!(
            role_color(RoleClass::Heading),
            Color32::from_rgb(0xB4, 0x8C, 0xFF)
        );
    }
}
