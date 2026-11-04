//! `pdfxray diff`: the pairing of two documents as JSON, narrowed to a scope and a page.

use clap::ValueEnum;
use pdfxray_core::PageIx;
use pdfxray_core::diff::{Note, Offset, PageMatch, Pair, Pairing, Sides, Status, Summary};
use serde::Serialize;

/// `--scope` values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ScopeArg {
    /// Structure element pairs and the page residue.
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

/// Narrows `pairing` to what `view` asks for.
pub(crate) fn narrow(pairing: &Pairing, view: DiffView) -> DiffOutput<'_> {
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
    let identical = summary.is_unchanged()
        && pairing.offset.is_none()
        && !in_scope_pages.iter().any(|m| page_changed(m, view.scope));
    let pairs = with_pairs.then(|| {
        scoped_pairs
            .into_iter()
            .filter(|x| view.include_unchanged || x.status != Status::Same)
            .collect()
    });
    let pages = in_scope_pages
        .into_iter()
        .filter(|m| view.include_unchanged || page_changed(m, view.scope))
        .map(|m| PageView {
            a: m.a,
            b: m.b,
            pairs: m.pairs,
            residue: (view.scope != ScopeArg::Tags).then_some(&m.residue),
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
    }
}
