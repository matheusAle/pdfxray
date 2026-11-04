//! The pairing on generated documents whose difference is known by construction.

#![allow(clippy::unwrap_used, reason = "test helpers fail loudly on unexpected input")]

mod common;

use pdfxray_core::Document;
use pdfxray_core::diff::{self, DocKeys, PageMatch, Status, Summary};
use pretty_assertions::assert_eq;

fn synthetic(pages: u32) -> Document {
    Document::from_bytes(format!("synthetic-{pages}.pdf"), common::synthetic_pdf(pages)).unwrap()
}

fn page_pairs(pages: &[PageMatch]) -> Vec<(Option<u32>, Option<u32>)> {
    pages.iter().map(|m| (m.a, m.b)).collect()
}

#[test]
fn an_appended_page_is_added_and_every_other_element_is_unchanged() {
    let (a, b) = (synthetic(2), synthetic(3));
    let p = diff::pair(&a, &b);
    let per_page = usize::try_from(common::ELEMS_PER_PAGE).unwrap();
    assert_eq!(
        p.summary(),
        Summary {
            same: 2 * per_page + 1,
            added: per_page,
            ..Summary::default()
        }
    );
    // Every added element is on the new page, and the pages correspond by what is on them.
    assert!(
        p.pairs
            .iter()
            .filter(|x| x.status == Status::Added)
            .all(|x| x.page_b == Some(2) && x.a.is_none())
    );
    assert_eq!(
        page_pairs(&p.pages),
        vec![(Some(0), Some(0)), (Some(1), Some(1)), (None, Some(2))]
    );
    // Page 0's evidence: its own elements and the `Document` element, which is placed there.
    assert_eq!((p.pages[0].pairs, p.pages[1].pairs), (per_page + 1, per_page));
    assert_eq!(p.offset, None);
    // Seen from B, the same page is removed.
    let back = diff::pair(&b, &a);
    assert_eq!((back.summary().removed, back.summary().added), (per_page, 0));
}

#[test]
fn prebuilt_keys_give_the_same_pairing() {
    let (a, b) = (synthetic(1), synthetic(2));
    let (ka, kb) = (DocKeys::build(&a), DocKeys::build(&b));
    assert_eq!(ka.len(), usize::try_from(common::ELEMS_PER_PAGE).unwrap() + 1);
    assert!(!kb.is_empty());
    assert_eq!(diff::pair_keyed(&a, &ka, &b, &kb), diff::pair(&a, &b));
}
