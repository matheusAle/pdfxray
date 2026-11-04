//! Glyph facts of a marked-content sequence for the diff's keys: the Unicode text when the font
//! provides it, else the character code and advance that stand in for it, and whether the font
//! has a `/ToUnicode` map.

use std::collections::{BTreeSet, HashMap};

use crate::capture::PaintKind;
use crate::numeric::usize_from_u32;
use crate::{Document, ObjRef, PageIx, UnicodeSource};

/// One glyph, as the diff keys it.
#[derive(Debug, Clone, Copy)]
pub(crate) struct GlyphFact<'a> {
    /// Unicode text of the glyph, when the font maps it to any.
    pub(crate) text: Option<&'a str>,
    /// Character code read from the string.
    pub(crate) code: u32,
    /// Advance in page space.
    pub(crate) advance: f64,
    /// Where `text` came from.
    pub(crate) source: UnicodeSource,
}

/// A font as the content selects it: resource name, resources owner, font object.
type FontKey = (Option<String>, Option<ObjRef>, Option<ObjRef>);

/// Whether a font has `/ToUnicode`, so that each font is looked up once per document rather than
/// once per glyph: per `(resource name, resources owner, font object)`, and in front of that per
/// graphics state (shared by every glyph of an operator), which needs no key allocation.
#[derive(Debug, Default)]
pub(crate) struct FontCache {
    by_font: HashMap<FontKey, bool>,
    by_state: HashMap<(PageIx, usize, Option<ObjRef>), bool>,
}

impl Document {
    /// Hands the glyphs painted by the sequences with `mcid` on `page` written in `stream` to
    /// `f`, in paint order, like [`Document::mc_text_each`] but with the facts the diff keys on.
    pub(crate) fn mc_glyphs_each(
        &self,
        page: PageIx,
        mcid: i32,
        stream: Option<ObjRef>,
        fonts: &mut FontCache,
        f: &mut dyn FnMut(GlyphFact<'_>),
    ) {
        let Some(d) = self.content.data(page) else {
            return;
        };
        let lists: Vec<&Vec<u32>> = d
            .mc_seqs(mcid, |s| super::stream_form(&s.stream) == stream)
            .filter_map(|s| d.seq_glyphs.get(usize_from_u32(s.ix)))
            .collect();
        // Several sequences can share glyphs (nesting); merge them into paint order.
        let merged: Vec<u32>;
        let glyphs: &[u32] = match lists.as_slice() {
            [] => return,
            [one] => one,
            many => {
                merged = many
                    .iter()
                    .flat_map(|l| l.iter().copied())
                    .collect::<BTreeSet<u32>>()
                    .into_iter()
                    .collect();
                &merged
            }
        };
        // The glyphs of one operator share its font: look it up once per operator.
        let mut last_op: Option<(u32, bool)> = None;
        for item in glyphs.iter().filter_map(|&i| d.item(i)) {
            let PaintKind::Glyph {
                char_code,
                unicode,
                advance,
                ..
            } = &item.kind
            else {
                continue;
            };
            let text = unicode.as_deref().filter(|u| !u.is_empty());
            let source = if text.is_none() {
                UnicodeSource::None
            } else if let Some((op, to_unicode)) = last_op
                && op == item.op
            {
                if to_unicode {
                    UnicodeSource::ToUnicode
                } else {
                    UnicodeSource::Encoding
                }
            } else {
                let owner = d
                    .op(item.op)
                    .and_then(|op| self.resources_owner(page, &d.cap, op));
                // The address is a cheap identity: the captured page keeps every state alive.
                let state = (page, std::sync::Arc::as_ptr(&item.gstate).addr(), owner);
                let to_unicode = if let Some(&known) = fonts.by_state.get(&state) {
                    known
                } else {
                    let ts = &item.gstate.text;
                    let known = *fonts
                        .by_font
                        .entry((ts.font.clone(), owner, ts.font_ref))
                        .or_insert_with(|| {
                            self.font_facts(ts.font.as_deref(), owner, ts.font_ref)
                                .is_some_and(|facts| facts.has_to_unicode)
                        });
                    fonts.by_state.insert(state, known);
                    known
                };
                last_op = Some((item.op, to_unicode));
                if to_unicode {
                    UnicodeSource::ToUnicode
                } else {
                    UnicodeSource::Encoding
                }
            };
            f(GlyphFact {
                text,
                code: *char_code,
                advance: *advance,
                source,
            });
        }
    }
}
