//! Plain-language hints for property names. A [`super::PropRow`] whose key is in [`HINTS`]
//! underlines the key with dots and shows the hint after the pointer rests on it for 400 ms:
//! the name first, then what it means. Add a line here to give another property a hint.

/// Property name → what it means. The names are the keys the inspector shows.
pub const HINTS: &[(&str, &str)] = &[
    (
        "CTM",
        "Current transformation matrix: maps the image's unit square to page space. \
         [a b c d e f] = scale x, skew y, skew x, scale y, translate x, translate y.",
    ),
    (
        "Clip",
        "The clipping path in effect: W clips to the current path with the non-zero winding \
         rule, W* with the even-odd rule, and n ends the path without painting it.",
    ),
    (
        "Tc",
        "Character spacing: extra space added after every glyph, in unscaled text units.",
    ),
    (
        "Tw",
        "Word spacing: extra space added after each single-byte space character (code 32).",
    ),
    (
        "Tz",
        "Horizontal scaling: stretches or squeezes glyphs horizontally, as a percentage.",
    ),
    (
        "TL",
        "Leading: the distance between baselines that T*, ' and \" move down by.",
    ),
    (
        "Ts",
        "Text rise: moves the baseline up (or down when negative), e.g. for superscripts.",
    ),
    (
        "Tr",
        "Text rendering mode: fill, stroke, both, invisible (3), or add to the clip. Mode 3 text \
         is not painted but is still read.",
    ),
    (
        "Tf",
        "Font and size: the font resource and the size the text is set in.",
    ),
    (
        "Tm",
        "Text matrix: places and orients the text in user space; it moves as glyphs are shown.",
    ),
    (
        "SMask",
        "Soft mask: a greyscale or alpha image that makes parts of what is painted transparent.",
    ),
    (
        "Blend",
        "Blend mode: how painted colours combine with what is already on the page.",
    ),
    (
        "Alpha",
        "Constant alpha: ca for fills and text, CA for strokes. 1 is opaque, 0 is invisible.",
    ),
    (
        "ParentTree",
        "The structure tree's index from content back to its tag: how a marked-content \
         sequence finds the element that owns it.",
    ),
    (
        "ActualText",
        "Replacement text read instead of the painted glyphs, e.g. for ligatures or drop \
         capitals.",
    ),
    (
        "UserUnit",
        "The size of one unit of page space, in multiples of 1/72 inch. Lengths shown here \
         already include it.",
    ),
];

/// The hint of `key`, if it has one.
pub fn hint(key: &str) -> Option<&'static str> {
    HINTS.iter().find(|(name, _)| *name == key).map(|(_, text)| *text)
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn the_requested_properties_have_hints_and_others_do_not() {
        for key in [
            "CTM",
            "Clip",
            "Tc",
            "Tw",
            "Tz",
            "TL",
            "Ts",
            "Tr",
            "Tf",
            "Tm",
            "SMask",
            "Blend",
            "Alpha",
            "ParentTree",
            "ActualText",
            "UserUnit",
        ] {
            assert!(hint(key).is_some_and(|text| !text.is_empty()), "{key}");
        }
        assert!(hint("CTM").is_some_and(|t| t.contains("[a b c d e f]")));
        // Smoke: an ordinary key has none, and every key is listed once.
        assert_eq!(hint("Page bbox"), None);
        let mut keys: Vec<&str> = HINTS.iter().map(|(k, _)| *k).collect();
        keys.sort_unstable();
        keys.dedup();
        assert_eq!(keys.len(), HINTS.len());
    }
}
