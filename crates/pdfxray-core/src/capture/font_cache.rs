//! The font cache: one [`FontInfo`] per indirect font dictionary of the page, shared by every
//! `Tf` that selects it.

use std::collections::HashMap;
use std::rc::Rc;

use hayro_interpret::InterpreterSettings;
use hayro_syntax::object::Dict;
use pdfxray_raw::ObjRef;

use super::font::FontInfo;

/// Fonts already read, by the reference of their dictionary.
#[derive(Default)]
pub(super) struct FontCache {
    by_ref: HashMap<ObjRef, Rc<FontInfo>>,
}

impl FontCache {
    /// The font a `Tf` selects: the font dictionary `dict` found under reference `font_ref`.
    /// Indirect fonts are read once; a direct one is read every time; a missing one is the
    /// engine's fallback.
    pub(super) fn resolve(
        &mut self,
        font_ref: Option<ObjRef>,
        dict: Option<Dict<'_>>,
        settings: &InterpreterSettings,
    ) -> Rc<FontInfo> {
        match (font_ref, dict) {
            (Some(r), Some(dict)) => Rc::clone(
                self.by_ref
                    .entry(r)
                    .or_insert_with(|| Rc::new(FontInfo::from_dict(&dict, settings))),
            ),
            (None, Some(dict)) => Rc::new(FontInfo::from_dict(&dict, settings)),
            (_, None) => Rc::new(FontInfo::fallback()),
        }
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::super::font::standard_metrics;
    use super::*;

    #[test]
    fn a_missing_font_is_the_fallback_and_is_not_cached() {
        let mut cache = FontCache::default();
        let settings = InterpreterSettings::default();

        let a = cache.resolve(Some(ObjRef::new(5, 0)), None, &settings);
        let b = cache.resolve(Some(ObjRef::new(5, 0)), None, &settings);

        assert!(!Rc::ptr_eq(&a, &b), "fallbacks are not shared");
        assert_eq!(cache.by_ref.len(), 0);
        // smoke: the fallback has Helvetica's vertical metrics
        assert_eq!(Some((a.ascent, a.descent)), standard_metrics("Helvetica"));
    }
}
