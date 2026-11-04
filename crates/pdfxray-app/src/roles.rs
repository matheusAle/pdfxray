//! Tag role colours: one colour per [`RoleClass`], shared by the Tags tree dots, the tag boxes,
//! labels and reading-order badges on the canvas, the Tag roles legend and the x-ray's *Tag role*
//! colouring, so a role reads the same everywhere.
//!
//! They are the render crate's [`pdfxray_render::ROLE_COLORS`], the design's `role-*` tokens
//! (artifacts keep their kind colour): the x-ray and the exported tag overlay paint a role in the
//! same colour because there is one table of them.
//!
//! The Tag roles card lists a table in parts ([`TagRole`]), so its rows, header cells and data
//! cells can be focused one by one; every part keeps the table colour.

use egui::Color32;
use pdfxray_core::{RoleClass, TagBox};
use pdfxray_render::role_rgba;

/// The colour of `class`. Every place that draws a role uses this one function, which reads the
/// render crate's table.
pub const fn role_color(class: RoleClass) -> Color32 {
    let [r, g, b, _] = role_rgba(class);
    Color32::from_rgb(r, g, b)
}

/// How the legend names `class`: the tags it stands for, as in the design.
pub const fn class_name(class: RoleClass) -> &'static str {
    match class {
        RoleClass::Heading => "H1–H6",
        RoleClass::Paragraph => "P",
        RoleClass::Figure => "Figure",
        RoleClass::Caption => "Caption",
        RoleClass::Table => "Table",
        RoleClass::List => "L / LI",
        RoleClass::Link => "Link",
        RoleClass::Inline => "Span",
        RoleClass::Group => "Sect / Div",
        RoleClass::Artifact => "Artifact",
        RoleClass::Other => "Other",
    }
}

/// What a row of the Tag roles card stands for, and what focusing it picks: a role class, or one
/// part of a table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum TagRole {
    /// Every element of the class; for [`RoleClass::Table`] the table and its row groups
    /// (`THead`, `TBody` and `TFoot`), whose parts have rows of their own.
    Class(RoleClass),
    /// `TR`.
    TableRow,
    /// `TH`.
    TableHeader,
    /// `TD`.
    TableCell,
}

impl TagRole {
    /// Every role, in the card's order: the parts of a table right after it.
    pub const ALL: [Self; 14] = [
        Self::Class(RoleClass::Heading),
        Self::Class(RoleClass::Paragraph),
        Self::Class(RoleClass::Figure),
        Self::Class(RoleClass::Caption),
        Self::Class(RoleClass::Table),
        Self::TableRow,
        Self::TableHeader,
        Self::TableCell,
        Self::Class(RoleClass::List),
        Self::Class(RoleClass::Link),
        Self::Class(RoleClass::Inline),
        Self::Class(RoleClass::Group),
        Self::Class(RoleClass::Artifact),
        Self::Class(RoleClass::Other),
    ];

    /// The role of an element of `class` whose standard type is `standard`.
    pub fn of(class: RoleClass, standard: &str) -> Self {
        match (class, standard) {
            (RoleClass::Table, "TR") => Self::TableRow,
            (RoleClass::Table, "TH") => Self::TableHeader,
            (RoleClass::Table, "TD") => Self::TableCell,
            _ => Self::Class(class),
        }
    }

    /// The role of the element `tag` outlines.
    pub fn of_box(tag: &TagBox) -> Self {
        Self::of(tag.class, &tag.standard)
    }

    /// The class the role belongs to, which gives its colour.
    pub const fn class(self) -> RoleClass {
        match self {
            Self::Class(class) => class,
            Self::TableRow | Self::TableHeader | Self::TableCell => RoleClass::Table,
        }
    }

    /// True for a row, a header cell or a data cell: the card indents it under its table.
    pub const fn is_table_part(self) -> bool {
        !matches!(self, Self::Class(_))
    }

    /// How the card names the role: [`class_name`] for a class, the tag for a table part.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Class(class) => class_name(class),
            Self::TableRow => "TR",
            Self::TableHeader => "TH",
            Self::TableCell => "TD",
        }
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;
    use crate::theme::tokens;

    #[test]
    fn a_role_takes_the_design_token_of_its_class() {
        // The colours live in the render crate; the theme's role tokens must be the same.
        let tokens = [
            tokens::ROLE_HEADING,
            tokens::ROLE_P,
            tokens::ROLE_FIGURE,
            tokens::ROLE_CAPTION,
            tokens::ROLE_TABLE,
            tokens::ROLE_LIST,
            tokens::ROLE_LINK,
            tokens::ROLE_INLINE,
            tokens::ROLE_GROUP,
            tokens::KIND_ARTIFACT,
            tokens::ROLE_OTHER,
        ];
        assert_eq!(RoleClass::ALL.map(role_color), tokens);
        // Smoke: the heading is the design's violet.
        assert_eq!(
            role_color(RoleClass::Heading),
            Color32::from_rgb(0xB4, 0x8C, 0xFF)
        );
    }

    #[test]
    fn the_legend_names_tables_without_an_ellipsis() {
        // An ellipsis promises a dialog, and a screen reader reads it as a cut word.
        assert_eq!(class_name(RoleClass::Table), "Table");
        let with_ellipsis: Vec<&str> = RoleClass::ALL
            .into_iter()
            .map(class_name)
            .filter(|name| name.contains('…') || name.contains("..."))
            .collect();
        assert_eq!(with_ellipsis, Vec::<&str>::new());
        // Smoke: the other names are untouched.
        assert_eq!(class_name(RoleClass::List), "L / LI");
    }

    #[test]
    fn a_table_is_listed_in_parts_that_keep_the_table_colour() {
        assert_eq!(TagRole::of(RoleClass::Table, "TR"), TagRole::TableRow);
        assert_eq!(TagRole::of(RoleClass::Table, "TH"), TagRole::TableHeader);
        assert_eq!(TagRole::of(RoleClass::Table, "TD"), TagRole::TableCell);
        // The table and its row groups stay one role.
        assert_eq!(
            ["Table", "THead", "TBody", "TFoot"].map(|s| TagRole::of(RoleClass::Table, s)),
            [TagRole::Class(RoleClass::Table); 4]
        );
        let parts: Vec<&str> = TagRole::ALL
            .into_iter()
            .filter(|role| role.is_table_part())
            .map(TagRole::name)
            .collect();
        assert_eq!(parts, ["TR", "TH", "TD"]);
        assert!(
            TagRole::ALL
                .into_iter()
                .filter(|role| role.is_table_part())
                .all(|role| role.class() == RoleClass::Table)
        );
        // Smoke: every class has its role, once, in the order of the classes.
        let classes: Vec<RoleClass> = TagRole::ALL
            .into_iter()
            .filter_map(|role| match role {
                TagRole::Class(class) => Some(class),
                _ => None,
            })
            .collect();
        assert_eq!(classes, RoleClass::ALL);
    }
}
