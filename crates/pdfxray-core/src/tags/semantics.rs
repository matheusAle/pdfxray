//! Checks on what a screen reader will say: alternate text that hides content, figures without
//! it, substitutions that fight each other, role mappings some viewers do not follow and
//! `Private` elements.
//!
//! The effects come from `docs/research/screen-reader-reading-order.md` §2.2, §2.5 and §4.5:
//! Acrobat, and so NVDA and JAWS in it, replace an element and its whole subtree by its `/Alt` or
//! `/ActualText`; pdf.js and `PDFium` follow one `RoleMap` hop; the `Private` element and its
//! descendants are not interpreted (ISO 32000-1 Table 333).

use std::collections::BTreeMap;

use super::parse::Builder;
use super::{Elem, Kid, TagTree};
use crate::Severity;

/// Standard types whose `/ActualText` replaces a heading, cell, list or figure that the viewer
/// would otherwise announce by its role.
const ROLE_BEARING: [&str; 7] = ["H", "Title", "TH", "TD", "Figure", "L", "LI"];

/// True when the text is present and not blank.
fn present(text: Option<&str>) -> bool {
    text.is_some_and(|t| !t.trim().is_empty())
}

impl Elem {
    fn has_alt(&self) -> bool {
        present(self.alt.as_deref())
    }

    fn has_actual_text(&self) -> bool {
        present(self.actual_text.as_deref())
    }
}

impl TagTree {
    /// True when `ei` directly holds marked content or an object.
    fn owns_content(&self, ei: usize) -> bool {
        self.elems[ei]
            .kids
            .iter()
            .any(|k| matches!(k, Kid::Content { .. } | Kid::Object { obj: Some(_), .. }))
    }

    /// Number of descendants of `ei` (not counting `ei`) that directly hold content.
    fn content_below(&self, ei: usize) -> usize {
        self.elems[ei]
            .kids
            .iter()
            .filter_map(|k| match k {
                Kid::Elem(c) => Some(*c),
                _ => None,
            })
            .map(|c| usize::from(self.owns_content(c)) + self.content_below(c))
            .sum()
    }

    fn has_child_elems(&self, ei: usize) -> bool {
        self.elems[ei].kids.iter().any(|k| matches!(k, Kid::Elem(_)))
    }

    /// True when `ei`, or an ancestor, has alternate or actual text.
    fn substituted_at_or_above(&self, ei: usize) -> bool {
        let mut cur = Some(ei);
        while let Some(i) = cur {
            if self.elems[i].has_alt() || self.elems[i].has_actual_text() {
                return true;
            }
            cur = self.elems[i].parent;
        }
        false
    }

    /// True when an ancestor of `ei` is a `Private` element.
    fn inside_private(&self, ei: usize) -> bool {
        std::iter::successors(self.elems[ei].parent, |&p| self.elems[p].parent)
            .any(|p| self.standard_of(p) == Some("Private"))
    }

    fn standard_of(&self, ei: usize) -> Option<&str> {
        self.elems[ei].resolved.standard.as_deref()
    }
}

impl Builder<'_> {
    /// Runs the alternate-text, role-map and `Private` checks over every element.
    pub(super) fn check_semantics(&mut self) {
        /// A finding about element `ei`.
        type Found = (&'static str, Severity, String, usize);
        let mut found: Vec<Found> = Vec::new();
        // Custom roles that need several hops: role -> (first element, elements, chain).
        let mut multi_hop: BTreeMap<String, (usize, usize, Vec<String>)> = BTreeMap::new();
        let tree = &self.tree;
        for (ei, e) in tree.elems.iter().enumerate() {
            let name = if e.role.is_empty() { "(no type)" } else { &e.role };
            let standard = tree.standard_of(ei);
            if (e.has_alt() || e.has_actual_text()) && tree.has_child_elems(ei) {
                let hidden = tree.content_below(ei);
                if hidden > 0 {
                    let by = if e.has_alt() { "/Alt" } else { "/ActualText" };
                    let severity = if e.parent.is_none() || matches!(standard, Some("Document" | "Part")) {
                        Severity::Error
                    } else {
                        Severity::Warning
                    };
                    found.push((
                        "alt-hides-content",
                        severity,
                        format!(
                            "{name} {} has {by}, so Acrobat-based screen readers say that and skip the \
                             {hidden} element(s) with content below it",
                            e.obj
                        ),
                        ei,
                    ));
                }
            }
            if e.has_alt() && e.has_actual_text() {
                found.push((
                    "alt-and-actualtext",
                    Severity::Warning,
                    format!(
                        "{name} {} has both /Alt and /ActualText; Acrobat-based screen readers say only /Alt",
                        e.obj
                    ),
                    ei,
                ));
            }
            if e.has_actual_text()
                && standard.is_some_and(|s| {
                    ROLE_BEARING.contains(&s)
                        || is_numbered_heading(s)
                        || tree.class(ei) == super::RoleClass::Group
                })
            {
                found.push((
                    "actualtext-on-structure",
                    Severity::Warning,
                    format!(
                        "{name} {} has /ActualText, which replaces the whole {} with plain text and \
                         drops its role",
                        e.obj,
                        standard.unwrap_or_default()
                    ),
                    ei,
                ));
            }
            if standard == Some("Figure") && !tree.substituted_at_or_above(ei) {
                found.push((
                    "figure-alt-missing",
                    Severity::Error,
                    format!(
                        "Figure {} has no /Alt or /ActualText, so a screen reader says only \"graphic\"",
                        e.obj
                    ),
                    ei,
                ));
            }
            if standard == Some("Private")
                && !tree.inside_private(ei)
                && (tree.owns_content(ei) || tree.content_below(ei) > 0)
            {
                found.push((
                    "private-hides-content",
                    Severity::Warning,
                    format!(
                        "Private {} holds content, which a reader that follows the standard never \
                         interprets",
                        e.obj
                    ),
                    ei,
                ));
            }
            if standard.is_some() && e.resolved.chain.len() > 2 {
                multi_hop
                    .entry(e.role.clone())
                    .and_modify(|(_, count, _)| *count += 1)
                    .or_insert((ei, 1, e.resolved.chain.clone()));
            }
        }
        for (role, (ei, count, chain)) in multi_hop {
            found.push((
                "rolemap-multi-hop",
                Severity::Warning,
                format!(
                    "role {role} needs {} role map hops ({}); pdf.js and PDFium follow only one, so \
                     those viewers see a generic element ({count} element(s))",
                    chain.len() - 1,
                    chain.join(" → ")
                ),
                ei,
            ));
        }
        for (code, severity, message, ei) in found {
            let obj = self.tree.elems[ei].obj;
            self.find_on(code, severity, message, Some(obj), Some(ei));
        }
    }
}

/// True for `H1`..`Hn`.
fn is_numbered_heading(standard: &str) -> bool {
    standard.len() > 1 && standard.starts_with('H') && standard[1..].bytes().all(|b| b.is_ascii_digit())
}
