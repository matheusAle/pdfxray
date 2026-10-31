//! Page content as runs of text for the speech order: what a marked-content sequence says about
//! itself, the artifacts of a page, and the page's text in paint order.

use super::{PageData, glyph_unicode, stream_form};
use crate::capture::{MarkedSeq, PaintKind};
use crate::numeric::usize_from_u32;
use crate::{Document, ObjRef, PageIx};

/// Render mode of invisible text (`3 Tr`).
const INVISIBLE_RENDER_MODE: u8 = 3;

/// What the marked content of one MCID says beyond its glyphs.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct McFacts {
    /// `/ActualText` of the sequence.
    pub(crate) actual_text: Option<String>,
    /// `/Alt` of the sequence.
    pub(crate) alt: Option<String>,
    /// `/Lang` of the sequence.
    pub(crate) lang: Option<String>,
    /// True when the sequence sits inside an artifact.
    pub(crate) in_artifact: bool,
    /// True when it paints glyphs and every one of them is invisible (`3 Tr`).
    pub(crate) invisible: bool,
    /// Execution index of its first operator.
    pub(crate) first_op: Option<u32>,
}

/// An outermost artifact sequence of a page that paints text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ArtifactRun {
    /// Number among the page's artifact sequences (the `seq` of the artifact node).
    pub(crate) seq: u32,
    /// Execution index of its opening operator.
    pub(crate) begin_op: u32,
    /// The text it paints, whitespace collapsed.
    pub(crate) text: String,
    /// `/Subtype` (`Header`, `Footer`, ...), else `/Type`.
    pub(crate) label: Option<String>,
}

/// What a run of text in paint order belongs to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RunKind {
    /// Tagged content with this MCID, written in this stream (`None` = the page's own content).
    Tagged(i32, Option<ObjRef>),
    /// The artifact with this number.
    Artifact(u32),
    /// Text outside any tagged or artifact sequence.
    Untagged,
}

/// Consecutive glyphs of one owner, in paint order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TextRun {
    /// Who owns the glyphs.
    pub(crate) kind: RunKind,
    /// Execution index of the operator that painted the first glyph.
    pub(crate) first_op: u32,
    /// The text, whitespace collapsed.
    pub(crate) text: String,
    /// True when every glyph is invisible (`3 Tr`).
    pub(crate) invisible: bool,
}

/// Collapses runs of white space to single spaces and trims the ends.
fn squash(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

impl PageData {
    /// True when `seq` or one of its ancestors is an artifact.
    fn in_artifact(&self, seq: &MarkedSeq) -> bool {
        self.seq_chain(Some(seq.ix))
            .into_iter()
            .filter_map(|s| self.seq(s))
            .any(|s| s.artifact.is_some())
    }

    /// True when the glyphs of `seqs` exist and are all invisible.
    fn all_invisible<'a>(&self, seqs: impl Iterator<Item = &'a MarkedSeq>) -> bool {
        let mut any = false;
        for id in seqs.flat_map(|s| self.glyph_ids(s)) {
            let Some(item) = self.item(id) else { continue };
            any = true;
            if item.gstate.text.render_mode != INVISIBLE_RENDER_MODE {
                return false;
            }
        }
        any
    }
}

impl Document {
    /// The facts the marked content of `mcid` on `page` (written in `stream`) states, `None`
    /// when the page has no such sequence.
    pub(crate) fn mc_facts(&self, page: PageIx, mcid: i32, stream: Option<ObjRef>) -> Option<McFacts> {
        let data = self.content.data(page)?;
        let seqs: Vec<&MarkedSeq> = data.mc_seqs(mcid, |s| stream_form(&s.stream) == stream).collect();
        if seqs.is_empty() {
            return None;
        }
        let pick = |f: fn(&MarkedSeq) -> Option<&String>| seqs.iter().find_map(|s| f(s)).cloned();
        Some(McFacts {
            actual_text: pick(|s| s.actual_text.as_ref()),
            alt: pick(|s| s.alt.as_ref()),
            lang: pick(|s| s.lang.as_ref()),
            in_artifact: seqs.iter().any(|s| data.in_artifact(s)),
            invisible: data.all_invisible(seqs.iter().copied()),
            first_op: seqs.iter().map(|s| s.begin_op).min(),
        })
    }

    /// The outermost artifact sequences of `page` that paint text, in paint order.
    pub(crate) fn artifact_runs(&self, page: PageIx) -> Vec<ArtifactRun> {
        let Some(data) = self.content.data(page) else {
            return Vec::new();
        };
        data.artifacts
            .iter()
            .filter_map(|&ix| data.seq(ix))
            .filter(|seq| {
                !data
                    .seq_chain(seq.parent)
                    .into_iter()
                    .filter_map(|s| data.seq(s))
                    .any(|s| s.artifact.is_some())
            })
            .filter_map(|seq| {
                let text = squash(&data.seqs_text(std::iter::once(seq)));
                let info = seq.artifact.as_ref()?;
                (!text.is_empty()).then(|| ArtifactRun {
                    seq: data
                        .art_no
                        .get(usize_from_u32(seq.ix))
                        .copied()
                        .flatten()
                        .unwrap_or_default(),
                    begin_op: seq.begin_op,
                    text,
                    label: info.subtype.clone().or_else(|| info.kind.clone()),
                })
            })
            .collect()
    }

    /// The glyphs of `page` in paint order, grouped into runs of one owner: the outermost tagged
    /// or artifact sequence they sit in, or none.
    pub(crate) fn text_runs(&self, page: PageIx) -> Vec<TextRun> {
        let Some(data) = self.content.data(page) else {
            return Vec::new();
        };
        let owner_of = |stack: &[u32]| {
            stack
                .iter()
                .filter_map(|&s| data.seq(s))
                .find_map(|seq| match (&seq.artifact, seq.mcid) {
                    (Some(_), _) => Some(RunKind::Artifact(
                        data.art_no
                            .get(usize_from_u32(seq.ix))
                            .copied()
                            .flatten()
                            .unwrap_or_default(),
                    )),
                    (None, Some(mcid)) => Some(RunKind::Tagged(mcid, stream_form(&seq.stream))),
                    (None, None) => None,
                })
                .unwrap_or(RunKind::Untagged)
        };
        let mut runs: Vec<TextRun> = Vec::new();
        let mut words: Vec<String> = Vec::new();
        let flush = |runs: &mut Vec<TextRun>, words: &mut Vec<String>| {
            if let Some(last) = runs.last_mut() {
                last.text = squash(&words.concat());
            }
            words.clear();
        };
        for item in data
            .cap
            .items
            .iter()
            .filter(|i| matches!(i.kind, PaintKind::Glyph { .. }))
        {
            let kind = owner_of(&item.mc_stack);
            let invisible = item.gstate.text.render_mode == INVISIBLE_RENDER_MODE;
            if let Some(last) = runs.last_mut().filter(|l| l.kind == kind) {
                last.invisible &= invisible;
            } else {
                flush(&mut runs, &mut words);
                runs.push(TextRun {
                    kind,
                    first_op: item.op,
                    text: String::new(),
                    invisible,
                });
            }
            words.push(glyph_unicode(item).unwrap_or_default().to_owned());
        }
        flush(&mut runs, &mut words);
        runs.retain(|r| !r.text.is_empty());
        runs
    }
}
