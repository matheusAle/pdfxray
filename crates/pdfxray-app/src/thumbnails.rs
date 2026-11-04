//! Page thumbnails of the Pages list: small bitmaps rendered one at a time on a thread of their
//! own, so scrolling a long strip never waits for the page on the canvas (or the other way
//! round).
//!
//! Like [`crate::page_cache`], the thread shares the UI's document. It renders the page
//! asked for last first: the ones that scrolled into view matter most.

use std::collections::{HashMap, HashSet};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

use egui::{ColorImage, Context, TextureHandle, TextureOptions};
use pdfxray_core::numeric::usize_from_u32;
use pdfxray_core::{Document, PageIx};
use pdfxray_render::{RenderSpec, page_dimensions, render_page};

/// Pixels along the longest side of a thumbnail bitmap (twice what the strip shows, for
/// high-density screens).
const LONGEST_SIDE: f32 = 120.0;

type Done = (PageIx, Option<TextureHandle>);

/// The render thread, as seen from the UI. Dropping it stops the thread once it is idle.
struct Worker {
    jobs: Sender<PageIx>,
    done: Receiver<Done>,
}

impl Worker {
    fn spawn(ctx: &Context, doc: &Arc<Document>) -> Option<Self> {
        let doc = Arc::clone(doc);
        let (jobs, jobs_rx) = mpsc::channel();
        let (done_tx, done) = mpsc::channel();
        let ctx = ctx.clone();
        thread::Builder::new()
            .name("pdfxray-thumbnails".to_owned())
            .spawn(move || work(&ctx, &doc, &jobs_rx, &done_tx))
            // A thumbnail is only a picture of the page: the strip shows the page without one,
            // and the canvas reports a page that cannot be rendered.
            .ok()?;
        Some(Self { jobs, done })
    }
}

/// Body of the thread: renders the page asked for last, until the UI hangs up.
fn work(ctx: &Context, doc: &Document, jobs: &Receiver<PageIx>, done: &Sender<Done>) {
    let mut stack: Vec<PageIx> = Vec::new();
    loop {
        if stack.is_empty() {
            let Ok(page) = jobs.recv() else {
                return;
            };
            stack.push(page);
        }
        stack.extend(jobs.try_iter());
        let Some(page) = stack.pop() else {
            continue;
        };
        let texture = catch_unwind(AssertUnwindSafe(|| render(ctx, doc, page)))
            .ok()
            .flatten();
        if done.send((page, texture)).is_err() {
            return;
        }
        ctx.request_repaint();
    }
}

/// `None` when the page cannot be rendered; the canvas shows why.
fn render(ctx: &Context, doc: &Document, page: PageIx) -> Option<TextureHandle> {
    let (width, height) = page_dimensions(doc, page).ok()?;
    let scale = LONGEST_SIDE / width.max(height).max(1.0);
    let rendered = render_page(doc, page, &RenderSpec::new(scale)).ok()?;
    let image = ColorImage::from_rgba_unmultiplied(
        [usize_from_u32(rendered.width), usize_from_u32(rendered.height)],
        &rendered.rgba,
    );
    Some(ctx.load_texture(format!("thumbnail-{page}"), image, TextureOptions::LINEAR))
}

/// The thumbnails rendered so far and the thread that renders more.
#[derive(Default)]
pub(crate) struct Thumbnails {
    /// Started on the first request of a document.
    worker: Option<Worker>,
    /// Pages the thread was asked for.
    requested: HashSet<PageIx>,
    /// Finished pages; `None` when the page cannot be rendered.
    ready: HashMap<PageIx, Option<TextureHandle>>,
}

impl Thumbnails {
    /// Drops everything, e.g. when another document is opened; the thread stops by itself.
    pub(crate) fn clear(&mut self) {
        self.worker = None;
        self.requested.clear();
        self.ready.clear();
    }

    /// True while a page that was asked for has no answer yet.
    pub(crate) fn is_pending(&self) -> bool {
        self.requested.len() > self.ready.len()
    }

    /// How many pages have a bitmap.
    pub(crate) fn rendered(&self) -> usize {
        self.ready.values().filter(|texture| texture.is_some()).count()
    }

    /// Takes in what the thread finished.
    pub(crate) fn poll(&mut self) {
        if let Some(worker) = &self.worker {
            self.ready.extend(worker.done.try_iter());
        }
    }

    /// The bitmap of `page`, once it is rendered; asks the thread for it the first time.
    pub(crate) fn get(&mut self, ctx: &Context, doc: &Arc<Document>, page: PageIx) -> Option<TextureHandle> {
        self.poll();
        if let Some(texture) = self.ready.get(&page) {
            return texture.clone();
        }
        if self.requested.insert(page) {
            if self.worker.is_none() {
                self.worker = Worker::spawn(ctx, doc);
            }
            match &self.worker {
                Some(worker) if worker.jobs.send(page).is_ok() => {}
                // No thread to render it: the page is shown without a picture.
                _ => {
                    self.ready.insert(page, None);
                }
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;
    use crate::test_support::{fixture, wait_until};

    #[test]
    fn the_thumbnail_thread_draws_from_the_uis_document_and_lets_go_of_it_when_cleared() {
        let ctx = Context::default();
        let doc = Arc::new(fixture("struct-rich"));
        let mut thumbnails = Thumbnails::default();

        let first = thumbnails.get(&ctx, &doc, 0);
        let while_live = Arc::strong_count(&doc);
        let texture = wait_until(|| thumbnails.get(&ctx, &doc, 0));
        thumbnails.clear();
        wait_until(|| (Arc::strong_count(&doc) == 1).then_some(()));

        assert!(first.is_none(), "the first ask goes to the thread");
        assert_eq!(while_live, 2, "the UI and the thumbnail thread hold one document");
        assert_eq!(Arc::strong_count(&doc), 1, "the thread let go after clear()");
        // smoke: the thumbnail is a real bitmap, longest side as configured
        let [w, h] = texture.size();
        assert_eq!(w.max(h), 120, "LONGEST_SIDE pixels along the longest side");
    }
}
