//! Opening with progress.

use pretty_assertions::assert_eq;

use super::*;

type Event = (OpenStage, usize, usize);

fn open_recording(name: &str) -> (Document, Vec<Event>) {
    let mut events = Vec::new();
    let caller = std::thread::current().id();
    let mut hook = |stage, done, total| {
        assert_eq!(
            std::thread::current().id(),
            caller,
            "progress is reported on the calling thread"
        );
        events.push((stage, done, total));
    };
    let doc = Document::open_with_progress(
        pdfxray_fixtures::pdf_path(name),
        &mut hook,
        &AtomicBool::new(false),
    )
    .unwrap();
    (doc, events)
}

#[test]
fn a_small_file_reports_each_stage_start_and_end_in_order() {
    let (doc, events) = open_recording("minimal-one-page");

    assert_eq!(
        events,
        [
            (OpenStage::ReadStructure, 0, 0),
            (OpenStage::ReadStructure, 1, 1),
            (OpenStage::IndexObjects, 0, 0),
            (OpenStage::IndexObjects, 5, 5),
            (OpenStage::ReadPageTree, 0, 0),
            (OpenStage::ReadPageTree, 1, 1),
            (OpenStage::CaptureContent, 0, 1),
            (OpenStage::CaptureContent, 1, 1),
            (OpenStage::BuildStructureTree, 0, 0),
            (OpenStage::BuildStructureTree, 0, 0),
        ]
    );
    assert_eq!(doc.page_count(), 1);
    assert_eq!(doc.info().object_count, 5);
}

#[test]
fn a_tagged_file_ends_with_its_structure_element_count() {
    let (doc, events) = open_recording("tagged-simple");

    let last = events.last().copied().unwrap();

    assert_eq!(last, (OpenStage::BuildStructureTree, 3, 3), "Document, H1 and P");
    assert_eq!(events.first().copied(), Some((OpenStage::ReadStructure, 0, 0)));
    assert!(doc.coverage(0).is_some());
}

#[test]
fn a_long_file_counts_pages_one_by_one_while_workers_capture_them() {
    let (doc, events) = open_recording("long-doc");
    let pages = doc.page_count();
    let captured: Vec<usize> = events
        .iter()
        .filter(|(stage, done, _)| *stage == OpenStage::CaptureContent && *done > 0)
        .map(|(_, done, _)| *done)
        .collect();
    let stages: Vec<OpenStage> = {
        let mut stages: Vec<OpenStage> = events.iter().map(|(s, ..)| *s).collect();
        stages.dedup();
        stages
    };

    assert!(
        pages >= 4,
        "long-doc is long enough to be captured in parallel: {pages} pages"
    );
    assert_eq!(
        captured,
        (1..=pages).collect::<Vec<_>>(),
        "one report per page, counting up"
    );
    assert!(events.contains(&(OpenStage::CaptureContent, 0, pages)));
    assert!(
        events
            .iter()
            .filter(|(s, ..)| *s == OpenStage::CaptureContent)
            .all(|(_, _, t)| *t == pages)
    );
    assert_eq!(
        stages,
        [
            OpenStage::ReadStructure,
            OpenStage::IndexObjects,
            OpenStage::ReadPageTree,
            OpenStage::CaptureContent,
            OpenStage::BuildStructureTree,
        ]
    );
    assert!(events.contains(&(OpenStage::ReadPageTree, pages, pages)));
}

#[test]
fn raising_the_flag_before_or_during_opening_cancels_it() {
    let path = pdfxray_fixtures::pdf_path("long-doc");
    let before = AtomicBool::new(true);
    let during = AtomicBool::new(false);
    let mut stages = Vec::new();

    let early = Document::open_with_progress(&path, &mut |_, _, _| {}, &before);
    let mid = Document::open_with_progress(
        &path,
        &mut |stage, done, _| {
            stages.push(stage);
            if stage == OpenStage::CaptureContent && done >= 1 {
                during.store(true, Ordering::Relaxed);
            }
        },
        &during,
    );
    let from_bytes = Document::from_bytes_with_progress(
        "x.pdf",
        std::fs::read(pdfxray_fixtures::pdf_path("minimal-one-page")).unwrap(),
        &mut |_, _, _| {},
        &AtomicBool::new(true),
    );

    assert!(matches!(early, Err(CoreError::Cancelled)), "{early:?}");
    assert!(matches!(mid, Err(CoreError::Cancelled)), "{mid:?}");
    assert!(matches!(from_bytes, Err(CoreError::Cancelled)));
    assert_eq!(
        stages.last(),
        Some(&OpenStage::CaptureContent),
        "it stopped while capturing"
    );
    assert!(!stages.contains(&OpenStage::BuildStructureTree));
    assert_eq!(CoreError::Cancelled.to_string(), "opening was cancelled");
}

#[test]
fn opening_without_progress_gives_the_same_document() {
    let plain = open("struct-rich");
    let (reported, _) = open_recording("struct-rich");

    assert_eq!(plain.info(), reported.info());
    assert_eq!(plain.coverage(0), reported.coverage(0));
    assert_eq!(plain.page_count(), reported.page_count());
}
