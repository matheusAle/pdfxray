//! `pdfxray diff`: the pairing of two documents as JSON, narrowed to a scope and a page.

use std::borrow::Cow;

use clap::ValueEnum;
use pdfxray_core::PageIx;
use pdfxray_core::diff::{
    Anchor, DiagnosticsDiff, FindingChange, FindingsDiff, Note, Offset, PageMatch, Pair, Pairing, Sides,
    Status, Summary, ValidationDiff,
};
use pdfxray_render::score::PageScore;
use serde::Serialize;

use crate::visual::{ScoreView, Settings};

/// The visual layer of a pairing (`--visual`): how it was measured, and the score of the page
/// pairs in scope, each naming its pages.
#[derive(Debug, Clone)]
pub(crate) struct Scores {
    /// How.
    pub(crate) settings: Settings,
    /// The scores; a page pair without one here was not scored.
    pub(crate) pages: Vec<PageScore>,
}

impl Scores {
    fn of(&self, m: &PageMatch) -> Option<&PageScore> {
        self.pages.iter().find(|s| (s.a, s.b) == (m.a, m.b))
    }
}

/// The visual score of a page pair as JSON: the score, or why there is none.
#[derive(Debug, Serialize)]
#[serde(untagged)]
pub(crate) enum PageVisual {
    /// Both pages drawn and compared.
    Scored(ScoreView),
    /// They could not be drawn on one grid.
    Refused {
        /// Why.
        refused: String,
    },
}

/// `--scope` values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ScopeArg {
    /// Structure element pairs, the page residue and the diagnostics fixed and introduced.
    All,
    /// Structure element pairs and the page correspondence.
    Tags,
    /// The residue of each page pair: painted content outside the structure tree.
    Ink,
}

/// What to show of a pairing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DiffView {
    /// Which part of the comparison.
    pub(crate) scope: ScopeArg,
    /// Only page `N` of A (and what it corresponds to in B).
    pub(crate) page: Option<PageIx>,
    /// Keep the pairs and pages that did not change.
    pub(crate) include_unchanged: bool,
}

/// The JSON `pdfxray diff` prints.
#[derive(Debug, Serialize)]
pub(crate) struct DiffOutput<'a> {
    /// True when nothing in scope changed: the exit status is then 0.
    pub(crate) identical: bool,
    /// The scope shown.
    pub(crate) scope: ScopeArg,
    /// The page of A shown, when narrowed to one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) page: Option<PageIx>,
    /// Pairs by status, in scope.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) summary: Option<Summary>,
    /// The two documents.
    pub(crate) sides: &'a Sides,
    /// The global offset of B relative to A.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) offset: Option<&'a Offset>,
    /// Residual translation that still counts as not moved, points.
    pub(crate) tolerance: f64,
    /// What limited the comparison.
    #[serde(skip_serializing_if = "<[Note]>::is_empty")]
    pub(crate) notes: &'a [Note],
    /// Element pairs (`all`, `tags`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) pairs: Option<Vec<&'a Pair>>,
    /// Page pairs.
    pub(crate) pages: Vec<PageView<'a>>,
    /// Diagnostics fixed and introduced (`all`); narrowed to the page with `--page`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) diagnostics: Option<Cow<'a, DiagnosticsDiff>>,
    /// How the page pairs were scored visually (`--visual`); each page pair carries its score.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) visual: Option<Settings>,
}

/// A page pair as shown under a scope: the residue only where it is in scope.
#[derive(Debug, Serialize)]
pub(crate) struct PageView<'a> {
    /// Page of A.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) a: Option<PageIx>,
    /// Page of B.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) b: Option<PageIx>,
    /// Element pairs between the two pages.
    pub(crate) pairs: usize,
    /// Content outside the structure tree (`all`, `ink`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) residue: Option<&'a pdfxray_core::diff::Residue>,
    /// The visual score (`--visual`), beside the residue: they answer different questions. Absent
    /// for a page only one side has.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) visual: Option<PageVisual>,
}

/// The visual score of `m` in `scores`, as JSON.
fn page_visual(scores: Option<&Scores>, m: &PageMatch) -> Option<PageVisual> {
    match scores?.of(m)?.visual.as_ref()? {
        Ok(v) => Some(PageVisual::Scored(ScoreView::from(v))),
        Err(why) => Some(PageVisual::Refused {
            refused: why.to_string(),
        }),
    }
}

/// True when a page pair changed in what `scope` looks at.
fn page_changed(m: &PageMatch, scope: ScopeArg) -> bool {
    let unpaired = m.a.is_none() || m.b.is_none();
    let renumbered = m.a != m.b;
    let residue = m.residue.only_a > 0 || m.residue.only_b > 0;
    match scope {
        ScopeArg::All => unpaired || renumbered || residue,
        ScopeArg::Tags => unpaired || renumbered,
        ScopeArg::Ink => unpaired || residue,
    }
}

/// Narrows `pairing` to what `view` asks for, with the visual `scores` of its page pairs when
/// there are any. A page pair whose ink differs at all is listed even when nothing else
/// changed on it; it does not make the documents differ (`identical`, and the exit status, come
/// from the pairing).
pub(crate) fn narrow<'a>(pairing: &'a Pairing, view: DiffView, scores: Option<&Scores>) -> DiffOutput<'a> {
    let partner = view
        .page
        .and_then(|n| pairing.pages.iter().find(|m| m.a == Some(n)).and_then(|m| m.b));
    let on_page = |x: &Pair| match view.page {
        None => true,
        Some(n) => x.page_a == Some(n) || (x.a.is_none() && partner.is_some() && x.page_b == partner),
    };
    let in_scope_pages: Vec<&PageMatch> = pairing
        .pages
        .iter()
        .filter(|m| view.page.is_none_or(|n| m.a == Some(n)))
        .collect();
    let with_pairs = view.scope != ScopeArg::Ink;
    let scoped_pairs: Vec<&Pair> = if with_pairs {
        pairing.pairs.iter().filter(|x| on_page(x)).collect()
    } else {
        Vec::new()
    };
    let mut summary = Summary::default();
    for x in &scoped_pairs {
        *match x.status {
            Status::Same => &mut summary.same,
            Status::Modified => &mut summary.modified,
            Status::Moved => &mut summary.moved,
            Status::Added => &mut summary.added,
            Status::Removed => &mut summary.removed,
        } += 1;
    }
    let diagnostics = (view.scope == ScopeArg::All).then(|| match view.page {
        None => Cow::Borrowed(&pairing.diagnostics),
        Some(_) => Cow::Owned(on_page_only(pairing, &on_page, |a, b| {
            view.page
                .is_some_and(|n| a == Some(n) || (a.is_none() && partner.is_some() && b == partner))
        })),
    });
    let identical = summary.is_unchanged()
        && pairing.offset.is_none()
        && !in_scope_pages.iter().any(|m| page_changed(m, view.scope))
        && diagnostics.as_deref().is_none_or(DiagnosticsDiff::is_unchanged);
    let pairs = with_pairs.then(|| {
        scoped_pairs
            .into_iter()
            .filter(|x| view.include_unchanged || x.status != Status::Same)
            .collect()
    });
    let looks_changed = |m: &PageMatch| {
        scores
            .and_then(|s| s.of(m))
            .and_then(|s| s.visual.as_ref())
            .is_some_and(|v| v.as_ref().is_ok_and(pdfxray_render::score::VisualScore::differs))
    };
    let pages = in_scope_pages
        .into_iter()
        .filter(|m| view.include_unchanged || page_changed(m, view.scope) || looks_changed(m))
        .map(|m| PageView {
            a: m.a,
            b: m.b,
            pairs: m.pairs,
            residue: (view.scope != ScopeArg::Tags).then_some(&m.residue),
            visual: page_visual(scores, m),
        })
        .collect();
    DiffOutput {
        identical,
        scope: view.scope,
        page: view.page,
        summary: with_pairs.then_some(summary),
        sides: &pairing.sides,
        offset: pairing.offset.as_ref(),
        tolerance: pairing.tolerance,
        notes: &pairing.notes,
        pairs,
        pages,
        diagnostics,
        visual: scores.map(|s| s.settings),
    }
}

/// The diagnostics of `pairing` whose anchor is on the page asked for: an element pair that
/// `on_page` keeps, or a page pair `page_pair` keeps. Document-wide findings are on no page.
fn on_page_only(
    pairing: &Pairing,
    on_page: &dyn Fn(&Pair) -> bool,
    page_pair: impl Fn(Option<PageIx>, Option<PageIx>) -> bool,
) -> DiagnosticsDiff {
    let keep = |c: &&FindingChange| match &c.on {
        Anchor::Element { pair, .. } => pairing.pairs.get(*pair).is_some_and(on_page),
        Anchor::Page { a, b } => page_pair(*a, *b),
        Anchor::Document => false,
    };
    let narrow = |f: &FindingsDiff| FindingsDiff {
        fixed: f.fixed.iter().filter(keep).cloned().collect(),
        introduced: f.introduced.iter().filter(keep).cloned().collect(),
        unchanged: f.unchanged,
    };
    DiagnosticsDiff {
        core: narrow(&pairing.diagnostics.core),
        validation: match &pairing.diagnostics.validation {
            ValidationDiff::Compared(f) => ValidationDiff::Compared(narrow(f)),
            other => other.clone(),
        },
    }
}

#[cfg(test)]
mod tests {
    use pdfxray_core::Document;
    use pretty_assertions::assert_eq;

    use super::*;

    fn drop_alt() -> Pairing {
        let (a, b) = pdfxray_fixtures::pairs::pair("drop-alt");
        let open = |p: &std::path::Path| Document::open(p).unwrap_or_else(|e| panic!("{e}"));
        pdfxray_core::diff::pair(&open(&a), &open(&b))
    }

    fn view(scope: ScopeArg, page: Option<PageIx>) -> DiffView {
        DiffView {
            scope,
            page,
            include_unchanged: false,
        }
    }

    fn codes(out: &DiffOutput<'_>) -> Option<Vec<String>> {
        out.diagnostics
            .as_deref()
            .map(|d| d.core.introduced.iter().map(|c| c.code.clone()).collect())
    }

    #[test]
    fn diagnostics_follow_the_scope_and_the_page() {
        let p = drop_alt();
        // The figure that lost its /Alt is on page 0.
        let whole = narrow(&p, view(ScopeArg::All, None), None);
        assert_eq!(codes(&whole), Some(vec!["figure-alt-missing".to_owned()]));
        assert!(!whole.identical);
        assert_eq!(
            codes(&narrow(&p, view(ScopeArg::All, Some(0)), None)),
            Some(vec!["figure-alt-missing".to_owned()])
        );
        // Page 1 has neither a changed pair nor a changed diagnostic: identical.
        let page1 = narrow(&p, view(ScopeArg::All, Some(1)), None);
        assert_eq!(codes(&page1), Some(vec![]));
        assert!(page1.identical);
        // Only `all` carries diagnostics.
        assert_eq!(codes(&narrow(&p, view(ScopeArg::Tags, None), None)), None);
        assert_eq!(codes(&narrow(&p, view(ScopeArg::Ink, None), None)), None);
        // Smoke: the typed change is there too, so the finding rides a real pair.
        assert_eq!(whole.summary.map(|s| s.modified), Some(1));
    }
}
