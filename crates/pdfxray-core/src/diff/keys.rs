//! The keys of every structure element, built bottom-up in one pass.
//!
//! An element's text contains every descendant's, so asking for it per element re-walks the
//! same content once per tree level. Here each element's text identity is composed from its
//! children's, already computed: the elements are in pre-order, so walking them backwards visits
//! every child before its parent, and the whole pass is O(content + elements).
//!
//! The text identity is a [`TextHash`]: a pair of polynomial hashes of the text *as
//! [`squash`](crate::tags) would leave it* (words joined by single spaces). Polynomial hashes
//! concatenate (`h(xy) = h(x)·Bᴸ⁽ʸ⁾ + h(y)`), which is what makes the bottom-up composition
//! exact: the parent's hash is the hash of its full squashed text, without that text ever being
//! built. Only a short display prefix is kept as a string.

use std::fmt::Write as _;

use super::align::{Keys, RUNGS};
use crate::capture::Alignment;
use crate::content::{FontCache, GlyphFact};
use crate::numeric::usize_from_u32;
use crate::tags::KidFact;
use crate::{Document, NodeId, PageIx, Rect, UnicodeSource};

/// Characters of an element's text kept for display beside its key.
pub(crate) const PREFIX: usize = 60;

/// The modulus of both polynomial hashes, the Mersenne prime 2⁶¹ − 1.
const MODULUS: u64 = (1 << 61) - 1;
/// The two bases (fixed, so keys are equal across runs and processes).
const BASES: [u64; 2] = [0x0005_DEEC_E66D, 0x0123_4567_89AB];

fn mul_mod(x: u64, y: u64) -> u64 {
    let wide = u128::from(x) * u128::from(y);
    // Reduce modulo 2⁶¹ − 1: fold the high bits onto the low ones.
    let folded = (wide & u128::from(MODULUS)) + (wide >> 61);
    let folded = u64::try_from(folded).unwrap_or(u64::MAX);
    let r = (folded & MODULUS) + (folded >> 61);
    if r >= MODULUS { r - MODULUS } else { r }
}

fn add_mod(x: u64, y: u64) -> u64 {
    let r = x + y;
    if r >= MODULUS { r - MODULUS } else { r }
}

fn pow_mod(mut base: u64, mut exp: u64) -> u64 {
    let mut acc = 1;
    while exp > 0 {
        if exp & 1 == 1 {
            acc = mul_mod(acc, base);
        }
        base = mul_mod(base, base);
        exp >>= 1;
    }
    acc
}

/// The identity of a squashed text: two polynomial hashes and the length in characters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub(crate) struct TextHash {
    h: [u64; 2],
    len: u64,
}

impl TextHash {
    /// True for the empty text.
    pub(crate) fn is_empty(self) -> bool {
        self.len == 0
    }

    fn push(&mut self, c: char) {
        let v = u64::from(c) + 1;
        for (h, base) in self.h.iter_mut().zip(BASES) {
            *h = add_mod(mul_mod(*h, base), v);
        }
        self.len += 1;
    }

    /// `self` then `other`.
    fn concat(self, other: Self) -> Self {
        let mut h = [0; 2];
        for (n, base) in BASES.into_iter().enumerate() {
            h[n] = add_mod(mul_mod(self.h[n], pow_mod(base, other.len)), other.h[n]);
        }
        Self {
            h,
            len: self.len + other.len,
        }
    }

    /// `self` and `other` joined by one space, the way squashed texts join; an empty side adds
    /// nothing.
    fn join(self, other: Self) -> Self {
        if self.is_empty() {
            return other;
        }
        if other.is_empty() {
            return self;
        }
        let mut with_space = self;
        with_space.push(' ');
        with_space.concat(other)
    }

    /// The 128 bits of the identity.
    pub(crate) fn bits(self) -> u128 {
        (u128::from(self.h[0]) << 64) | u128::from(self.h[1] ^ self.len.rotate_left(32))
    }
}

/// The squashed text of one stretch of content as it is read: its identity, a display prefix
/// and the weakest Unicode source among its glyphs.
#[derive(Debug, Clone, Default)]
pub(crate) struct Text {
    pub(crate) hash: TextHash,
    pub(crate) prefix: String,
    prefix_chars: usize,
    /// True when `prefix` holds the whole text.
    complete: bool,
    pub(crate) unicode: Option<UnicodeSource>,
}

impl Text {
    fn empty() -> Self {
        Self {
            complete: true,
            ..Self::default()
        }
    }

    /// Appends a character of a squashed text.
    fn push(&mut self, c: char) {
        self.hash.push(c);
        if self.prefix_chars < PREFIX {
            self.prefix.push(c);
            self.prefix_chars += 1;
        } else {
            self.complete = false;
        }
    }

    /// `self` and `other` joined by one space.
    fn join(&mut self, other: &Self) {
        if other.hash.is_empty() {
            self.unicode = worst(self.unicode, other.unicode);
            return;
        }
        if !self.hash.is_empty() && self.prefix_chars < PREFIX {
            self.prefix.push(' ');
            self.prefix_chars += 1;
        }
        let room = PREFIX - self.prefix_chars;
        let taken = other.prefix.chars().take(room).count();
        self.prefix.extend(other.prefix.chars().take(room));
        self.prefix_chars += taken;
        self.complete &= self.prefix_chars <= PREFIX && taken == other.prefix_chars && other.complete;
        self.hash = self.hash.join(other.hash);
        self.unicode = worst(self.unicode, other.unicode);
    }

    /// The prefix for display, ending in `…` when the text is longer.
    pub(crate) fn display(&self) -> String {
        if self.complete {
            self.prefix.clone()
        } else {
            let mut s = self.prefix.clone();
            s.push('…');
            s
        }
    }
}

/// The weaker of two Unicode sources (`None` sorts last, so it is the weakest).
fn worst(a: Option<UnicodeSource>, b: Option<UnicodeSource>) -> Option<UnicodeSource> {
    a.max(b)
}

/// A stand-in character for a glyph the font maps to no text: its code and advance, folded into
/// a private-use character so it joins the text like any other.
fn fallback_char(code: u32, advance: f64) -> char {
    let advance = crate::numeric::i64_from_f64_round(advance * 10.0);
    let mut h = u64::from(code).wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ advance.cast_unsigned();
    h ^= h >> 29;
    // Plane 15 is private use: 0xF0000..=0xFFFFD.
    let offset = u32::try_from(h % 0xFFFE).unwrap_or(0);
    char::from_u32(0xF_0000 + offset).unwrap_or(char::REPLACEMENT_CHARACTER)
}

/// The text of marked content `(page, mcid, stream)`, words joined by single spaces.
fn content_text(
    doc: &Document,
    (page, mcid, stream): (PageIx, i32, Option<crate::ObjRef>),
    fonts: &mut FontCache,
) -> Text {
    let mut out = Text::empty();
    let mut in_word = false;
    let mut push_char = |out: &mut Text, c: char| {
        if c.is_whitespace() {
            in_word = false;
            return;
        }
        if !in_word {
            in_word = true;
            if !out.hash.is_empty() {
                out.push(' ');
            }
        }
        out.push(c);
    };
    doc.mc_glyphs_each(page, mcid, stream, fonts, &mut |g: GlyphFact<'_>| {
        out.unicode = worst(out.unicode, Some(g.source));
        match g.text {
            Some(text) => {
                for c in text.chars() {
                    push_char(&mut out, c);
                }
            }
            None => push_char(&mut out, fallback_char(g.code, g.advance)),
        }
    });
    out
}

/// What the diff knows of one structure element.
#[derive(Debug, Clone)]
pub(crate) struct ElemKey {
    /// The element.
    pub(crate) node: NodeId,
    /// Its index among the document's elements.
    pub(crate) ix: usize,
    /// Its parent's index.
    pub(crate) parent: Option<usize>,
    /// True when it holds marked content itself; false for a pure container, whose box is
    /// only the union of its children's.
    pub(crate) holds_content: bool,
    /// The rung keys, see [`super::align`].
    pub(crate) keys: Keys,
    /// The text painted by the whole subtree (what the keys are made of).
    pub(crate) text: Text,
    /// The text painted by the element's own marked content, not its children's.
    pub(crate) own_text: Text,
    /// The page and box of the element, as [`Document::bbox`] gives them.
    pub(crate) placed: Option<(PageIx, Rect)>,
    /// `/S` as written.
    pub(crate) declared: String,
    /// The standard type it resolves to.
    pub(crate) standard: Option<String>,
    /// True when content of the subtree is on a page whose capture is degraded.
    pub(crate) degraded: bool,
}

impl ElemKey {
    /// The role keyed on: the standard type, else `/S`.
    pub(crate) fn role(&self) -> &str {
        self.standard.as_deref().unwrap_or(&self.declared)
    }
}

/// Every element of `doc`, in document pre-order, with its keys.
pub(crate) fn build(doc: &Document) -> Vec<ElemKey> {
    let n = doc.tag_elem_count();
    let degraded_pages: Vec<bool> = (0..doc.page_count())
        .map(|p| {
            u32::try_from(p)
                .ok()
                .and_then(|p| doc.captured(p))
                .is_some_and(|c| matches!(c.alignment, Alignment::Degraded { .. }))
        })
        .collect();
    let is_degraded = |page: PageIx| degraded_pages.get(usize_from_u32(page)).copied().unwrap_or(false);
    let mut fonts = FontCache::default();
    // Per element, filled children first: subtree text, painted boxes by page (first-seen
    // order), degradation. A parent takes its children's boxes, so each list is moved up once.
    let mut texts: Vec<Text> = vec![Text::empty(); n];
    let mut boxes: Vec<Vec<(PageIx, Rect)>> = vec![Vec::new(); n];
    let mut degraded = vec![false; n];
    let mut out: Vec<Option<ElemKey>> = vec![None; n];
    for ei in (0..n).rev() {
        let mut text = Text::empty();
        let mut own = Text::empty();
        let mut placed: Vec<(PageIx, Rect)> = Vec::new();
        let mut bad = false;
        let mut holds_content = false;
        for kid in doc.tag_kid_facts(ei) {
            match kid {
                KidFact::Content { page, mcid, stream } => {
                    holds_content = true;
                    let part = content_text(doc, (page, mcid, stream), &mut fonts);
                    text.join(&part);
                    own.join(&part);
                    if let Some(b) = doc.mc_bbox_in(page, mcid, stream) {
                        merge_box(&mut placed, page, b);
                    }
                    bad |= is_degraded(page);
                }
                // Children come after their parent in pre-order; anything else is malformed
                // and is not followed (it could be a cycle).
                KidFact::Elem(child) if child > ei && child < n => {
                    text.join(&texts[child]);
                    for (page, b) in std::mem::take(&mut boxes[child]) {
                        merge_box(&mut placed, page, b);
                    }
                    bad |= degraded[child];
                }
                KidFact::Elem(_) => {}
            }
        }
        let facts = doc.tag_elem_facts(ei);
        let first = placed.first().copied().or_else(|| doc.tag_unpainted_bbox(ei));
        let key = ElemKey {
            node: NodeId::StructElem(facts.obj),
            ix: ei,
            parent: facts.parent,
            holds_content,
            keys: [None; RUNGS],
            text: text.clone(),
            own_text: own,
            placed: first,
            declared: facts.declared.to_owned(),
            standard: facts.standard.map(str::to_owned),
            degraded: bad,
        };
        let keys = rung_keys(&key, facts.classes);
        out[ei] = Some(ElemKey { keys, ..key });
        texts[ei] = text;
        boxes[ei] = placed;
        degraded[ei] = bad;
    }
    out.into_iter().flatten().collect()
}

/// Adds `b` to the union kept for `page`.
fn merge_box(placed: &mut Vec<(PageIx, Rect)>, page: PageIx, b: Rect) {
    match placed.iter_mut().find(|(p, _)| *p == page) {
        Some((_, r)) => *r = Rect::new(r.x0.min(b.x0), r.y0.min(b.y0), r.x1.max(b.x1), r.y1.max(b.y1)),
        None => placed.push((page, b)),
    }
}

/// FNV-1a over 128 bits: fixed, so keys are equal across runs and processes.
#[derive(Debug, Clone, Copy)]
pub(super) struct Fnv(pub(super) u128);

impl Fnv {
    const OFFSET: u128 = 0x6c62_272e_07bb_0142_62b8_2175_6295_c58d;
    const PRIME: u128 = 0x0000_0000_0100_0000_0000_0000_0000_013B;

    pub(super) fn new(tag: &str) -> Self {
        Self(Self::OFFSET).bytes(tag.as_bytes())
    }

    pub(super) fn bytes(mut self, bytes: &[u8]) -> Self {
        for &b in bytes {
            self.0 ^= u128::from(b);
            self.0 = self.0.wrapping_mul(Self::PRIME);
        }
        // A separator, so that ("ab", "c") and ("a", "bc") differ.
        self.0 ^= 0xFF;
        self.0 = self.0.wrapping_mul(Self::PRIME);
        self
    }

    pub(super) fn str(self, s: &str) -> Self {
        self.bytes(s.as_bytes())
    }

    pub(super) fn num(self, v: u128) -> Self {
        self.bytes(&v.to_le_bytes())
    }
}

/// A box quantised to `quantum` points, as bytes to hash.
pub(super) fn quantised(b: Rect, quantum: f64) -> String {
    let q = |v: f64| crate::numeric::i64_from_f64_round(v / quantum);
    let mut s = String::new();
    let _ = write!(s, "{} {} {} {}", q(b.x0), q(b.y0), q(b.x1), q(b.y1));
    s
}

/// Quantum of the role-and-box rung, points.
const BOX_QUANTUM: f64 = 2.0;
/// Quantum of the geometry rung, points: coarser, it is the last resort.
const GEOMETRY_QUANTUM: f64 = 4.0;

/// A key of a role alone, for comparing roles across documents.
pub(crate) fn role_key(role: &str) -> u128 {
    Fnv::new("role").str(role).0
}

/// The ladder: `role + text`, `text`, `role + box`, `geometry`.
fn rung_keys(e: &ElemKey, classes: &[String]) -> Keys {
    let text = e.text.hash;
    let role = e.role();
    let role_text = (!text.is_empty()).then(|| {
        let mut h = Fnv::new("role+text").str(role);
        for c in classes {
            h = h.str(c);
        }
        h.num(text.bits()).0
    });
    let text_only = (!text.is_empty()).then(|| Fnv::new("text").num(text.bits()).0);
    let bbox = e.placed.map(|(_, b)| b);
    let role_box = Some(
        Fnv::new("role+box")
            .str(role)
            .str(&bbox.map_or_else(|| "none".to_owned(), |b| quantised(b, BOX_QUANTUM)))
            .0,
    );
    let geometry = bbox.map(|b| Fnv::new("geometry").str(&quantised(b, GEOMETRY_QUANTUM)).0);
    [role_text, text_only, role_box, geometry]
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    fn hash(s: &str) -> TextHash {
        let mut h = TextHash::default();
        for c in s.chars() {
            h.push(c);
        }
        h
    }

    #[test]
    fn concatenation_equals_hashing_the_whole_text() {
        let whole = hash("hello wide world");
        let composed = hash("hello").join(hash("wide")).join(hash("world"));
        assert_eq!(composed, whole);
        assert_eq!(hash("hello wide").join(hash("world")), whole);
        assert_eq!(hash("").join(hash("x")), hash("x"));
        assert_eq!(hash("x").join(hash("")), hash("x"));
        assert_ne!(hash("ab"), hash("ba"));
        assert_ne!(hash("a b"), hash("ab"));
        assert!(TextHash::default().is_empty() && !hash("a").is_empty());
    }

    #[test]
    fn text_join_keeps_a_bounded_prefix_and_marks_truncation() {
        let mut a = Text::empty();
        for c in "short".chars() {
            a.push(c);
        }
        let mut long = Text::empty();
        for c in std::iter::repeat_n('x', PREFIX + 10) {
            long.push(c);
        }
        assert_eq!(a.display(), "short");
        a.join(&long);
        assert_eq!(a.prefix.chars().count(), PREFIX);
        assert!(a.display().ends_with('…'));
        assert!(a.display().starts_with("short x"));
        assert_eq!(a.hash.len, 5 + 1 + u64::try_from(PREFIX + 10).unwrap());
    }

    #[test]
    fn fallback_characters_are_private_use_and_depend_on_code_and_advance() {
        let a = fallback_char(65, 7.2);
        assert!(('\u{F0000}'..='\u{FFFFD}').contains(&a));
        assert_eq!(a, fallback_char(65, 7.2));
        assert_ne!(a, fallback_char(66, 7.2));
        assert_ne!(a, fallback_char(65, 9.0));
    }

    #[test]
    fn mul_mod_matches_wide_arithmetic() {
        for (x, y) in [(MODULUS - 1, MODULUS - 1), (12_345, 67_890), (1 << 60, 3)] {
            let expected = u64::try_from(u128::from(x) * u128::from(y) % u128::from(MODULUS)).unwrap();
            assert_eq!(mul_mod(x, y), expected);
        }
        assert_eq!(pow_mod(3, 0), 1);
        assert_eq!(pow_mod(3, 5), 243);
    }
}
