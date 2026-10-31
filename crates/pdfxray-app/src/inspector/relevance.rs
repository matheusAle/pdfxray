//! Which sections of the inspector matter in which entry of the rail.
//!
//! A section the current entry does not care about starts collapsed, so the inspector leads with
//! what the user came for: the image's own numbers in Content, the structure facts in Tags. The
//! header stays, one click opens it, and what the user opened or closed is remembered.

use super::structure::{ACCESSIBILITY_SECTION, STRUCTURE_SECTION};
use super::{font, geometry, graphics, text_state};
use crate::state::RailItem;

/// The sections that start collapsed in `rail`, by title.
///
/// | Entry | Starts collapsed |
/// |---|---|
/// | Tags | Image, Font, Text state |
/// | Content | Structure, Accessibility |
/// | Artifacts | Structure, Accessibility, Font, Text state |
/// | Pages | Graphics state, Image, Font, Text state, Structure, Accessibility |
/// | Objects, Revisions | Geometry, Graphics state, Text state, Structure, Accessibility |
/// | Validation | none |
pub const fn collapsed(rail: RailItem) -> &'static [&'static str] {
    match rail {
        RailItem::Tags => &[graphics::IMAGE_TITLE, font::TITLE, text_state::TITLE],
        RailItem::Content => &[STRUCTURE_SECTION, ACCESSIBILITY_SECTION],
        RailItem::Artifacts => &[
            STRUCTURE_SECTION,
            ACCESSIBILITY_SECTION,
            font::TITLE,
            text_state::TITLE,
        ],
        RailItem::Pages => &[
            graphics::TITLE,
            graphics::IMAGE_TITLE,
            font::TITLE,
            text_state::TITLE,
            STRUCTURE_SECTION,
            ACCESSIBILITY_SECTION,
        ],
        RailItem::Objects | RailItem::Revisions => &[
            geometry::TITLE,
            graphics::TITLE,
            text_state::TITLE,
            STRUCTURE_SECTION,
            ACCESSIBILITY_SECTION,
        ],
        RailItem::Validation => &[],
    }
}

/// True when the section called `title` starts open in `rail`.
pub fn starts_open(rail: RailItem, title: &str) -> bool {
    !collapsed(rail).contains(&title)
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn image_is_quiet_in_tags_and_structure_in_content() {
        assert_eq!(starts_open(RailItem::Tags, "Image"), false);
        assert_eq!(starts_open(RailItem::Content, "Image"), true);
        assert_eq!(starts_open(RailItem::Content, "Structure"), false);
        assert_eq!(starts_open(RailItem::Tags, "Structure"), true);
        // Smoke: nothing is collapsed on the Validation entry.
        assert_eq!(collapsed(RailItem::Validation), [] as [&str; 0]);
    }
}
