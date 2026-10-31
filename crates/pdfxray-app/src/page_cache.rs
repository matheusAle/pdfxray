//! Rendered pages: a background render thread and a small cache of the bitmaps it produced.
//!
//! The UI never rasterises. [`PageCache::get`] answers from the cache or sends the
//! [`RenderRequest`] to the render thread, which owns its own copy of the document (a
//! [`Document`] cannot be shared while validation results are attached to the UI's copy). The
//! canvas shows several pages at once, so the thread renders the newest request of every page
//! that asked while it was busy, in the order they asked. Until a page's new bitmap is there its
//! previous one keeps being shown.

use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::io;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::Duration;

use egui::{ColorImage, Context, TextureHandle, TextureOptions};
use pdfxray_core::numeric::usize_from_u32;
use pdfxray_core::{CoreError, Document, PageIx};
use pdfxray_render::{RenderError, RenderedPage, Transform, render_page};

use crate::modes::RenderRequest;

/// How soon the UI is woken after the thread finished a bitmap.
const WAKE_DELAY: Duration = Duration::from_millis(1);

/// How many bitmaps are kept; the least recently used one is dropped first.
const CAPACITY: usize = 12;

/// A page bitmap on the GPU together with the transform that produced it.
#[derive(Clone)]
pub(crate) struct CachedPage {
    /// The uploaded bitmap.
    pub(crate) texture: TextureHandle,
    /// Bitmap size in pixels.
    pub(crate) size: (u32, u32),
    /// Maps page space to bitmap pixels.
    pub(crate) page_to_pixel: Transform,
    /// [`pixel_digest`] of the bitmap.
    pub(crate) digest: u64,
}

/// Why a page has no bitmap. Failures are cached like bitmaps, so it is cheap to clone.
#[derive(Debug, Clone, thiserror::Error)]
pub(crate) enum RenderFailure {
    /// The system did not start the render thread.
    #[error("cannot start the render thread")]
    Spawn(#[source] Arc<io::Error>),
    /// The render thread is gone, so it takes no more requests.
    #[error("the render thread stopped")]
    Stopped,
    /// The renderer panicked on this page.
    #[error("the renderer crashed on this page")]
    Crashed,
    /// The render thread could not open its copy of the document.
    #[error(transparent)]
    Open(Arc<CoreError>),
    /// The renderer refused the page.
    #[error(transparent)]
    Render(#[from] RenderError),
}

/// A render result; failures are cached too so a broken page is not retried every frame.
pub(crate) type CacheResult = Result<CachedPage, RenderFailure>;

/// What to put on the canvas this frame.
pub(crate) enum Frame {
    /// The requested bitmap.
    Ready(CachedPage),
    /// The requested bitmap is being rendered; this is the previous one of the same page.
    Stale(CachedPage),
    /// The requested bitmap is being rendered and there is nothing to show meanwhile.
    Rendering,
    /// The page cannot be rendered.
    Failed(RenderFailure),
}

/// Hash of RGBA pixels, stable between runs; lets tests compare bitmaps without keeping them.
pub fn pixel_digest(rgba: &[u8]) -> u64 {
    let mut hasher = DefaultHasher::new();
    rgba.hash(&mut hasher);
    hasher.finish()
}

type Done = (RenderRequest, CacheResult);

/// The render thread, as seen from the UI. Dropping it stops the thread once it is idle.
struct Worker {
    jobs: Sender<RenderRequest>,
    done: Receiver<Done>,
}

impl Worker {
    fn spawn(ctx: &Context, doc: &Document) -> Result<Self, RenderFailure> {
        let bytes = doc.bytes().to_vec();
        let (jobs, jobs_rx) = mpsc::channel();
        let (done_tx, done) = mpsc::channel();
        let ctx = ctx.clone();
        thread::Builder::new()
            .name("pdfxray-render".to_owned())
            .spawn(move || work(&ctx, bytes, &jobs_rx, &done_tx))
            .map_err(|e| RenderFailure::Spawn(Arc::new(e)))?;
        Ok(Self { jobs, done })
    }
}

/// Body of the render thread: renders the newest request of each page that asked, until the UI
/// hangs up.
fn work(ctx: &Context, bytes: Vec<u8>, jobs: &Receiver<RenderRequest>, done: &Sender<Done>) {
    let doc = Document::from_bytes("render", bytes).map_err(|e| RenderFailure::Open(Arc::new(e)));
    while let Ok(first) = jobs.recv() {
        let mut batch = vec![first];
        batch.extend(jobs.try_iter());
        for request in newest_per_page(batch) {
            let result = match &doc {
                Ok(doc) => catch_unwind(AssertUnwindSafe(|| render(ctx, doc, &request)))
                    .unwrap_or(Err(RenderFailure::Crashed)),
                Err(failure) => Err(failure.clone()),
            };
            if done.send((request, result)).is_err() {
                return;
            }
            // A delay keeps tests that run frames until rest from chasing the thread.
            ctx.request_repaint_after(WAKE_DELAY);
        }
    }
}

/// The last request of each page in `batch`, in the order the pages first asked.
fn newest_per_page(batch: Vec<RenderRequest>) -> Vec<RenderRequest> {
    let mut newest: Vec<RenderRequest> = Vec::new();
    for request in batch {
        match newest.iter_mut().find(|r| r.page == request.page) {
            Some(slot) => *slot = request,
            None => newest.push(request),
        }
    }
    newest
}

fn render(ctx: &Context, doc: &Document, request: &RenderRequest) -> CacheResult {
    let rendered = render_page(doc, request.page, &request.spec(doc))?;
    Ok(upload(ctx, request, rendered))
}

fn upload(ctx: &Context, request: &RenderRequest, rendered: RenderedPage) -> CachedPage {
    let RenderedPage {
        width,
        height,
        rgba,
        page_to_pixel,
    } = rendered;
    let digest = pixel_digest(&rgba);
    let image = ColorImage::from_rgba_unmultiplied([usize_from_u32(width), usize_from_u32(height)], &rgba);
    CachedPage {
        texture: ctx.load_texture(
            format!("page-{}-{}", request.page, request.scale.scale()),
            image,
            TextureOptions::LINEAR,
        ),
        size: (width, height),
        page_to_pixel,
        digest,
    }
}

/// The bitmaps shown so far and the thread that renders new ones.
#[derive(Default)]
pub(crate) struct PageCache {
    /// Least recently used first.
    entries: Vec<(RenderRequest, CacheResult)>,
    /// Started on the first request of a document.
    worker: Option<Result<Worker, RenderFailure>>,
    /// The request each page last gave the thread, until its result arrives.
    outstanding: HashMap<PageIx, RenderRequest>,
    /// The bitmap last put on the canvas for each page and the request it answers.
    shown: HashMap<PageIx, (RenderRequest, CachedPage)>,
    renders: usize,
}

impl PageCache {
    /// Number of render requests sent to the thread so far.
    pub(crate) fn renders(&self) -> usize {
        self.renders
    }

    /// True while the thread has not answered the latest request of some page.
    pub(crate) fn is_pending(&self) -> bool {
        !self.outstanding.is_empty()
    }

    /// Digest of the bitmap last shown on the canvas for `page`.
    pub(crate) fn shown_digest(&self, page: PageIx) -> Option<u64> {
        self.shown.get(&page).map(|(_, cached)| cached.digest)
    }

    /// The request the bitmap last shown on the canvas for `page` answers.
    pub(crate) fn shown_request(&self, page: PageIx) -> Option<&RenderRequest> {
        self.shown.get(&page).map(|(request, _)| request)
    }

    /// Forgets the shown bitmaps of pages that scrolled out of view, so their textures can go;
    /// the cache keeps the recent ones.
    pub(crate) fn keep_shown(&mut self, visible: &[PageIx]) {
        self.shown.retain(|page, _| visible.contains(page));
    }

    /// Drops everything, e.g. when another document is opened; the thread stops by itself.
    pub(crate) fn clear(&mut self) {
        self.entries.clear();
        self.worker = None;
        self.outstanding.clear();
        self.shown.clear();
    }

    /// Takes in the bitmaps the thread finished.
    pub(crate) fn poll(&mut self) {
        let Some(Ok(worker)) = &self.worker else {
            return;
        };
        let finished: Vec<Done> = worker.done.try_iter().collect();
        for (request, result) in finished {
            if self.outstanding.get(&request.page) == Some(&request) {
                self.outstanding.remove(&request.page);
            }
            self.insert(request, result);
        }
    }

    fn insert(&mut self, request: RenderRequest, result: CacheResult) {
        self.entries.retain(|(key, _)| *key != request);
        if self.entries.len() == CAPACITY {
            drop(self.entries.remove(0));
        }
        self.entries.push((request, result));
    }

    /// The bitmap for `request`; asks the thread for it on a miss.
    pub(crate) fn get(&mut self, ctx: &Context, doc: &Document, request: RenderRequest) -> Frame {
        self.poll();
        if let Some(at) = self.entries.iter().position(|(key, _)| *key == request) {
            let entry = self.entries.remove(at);
            self.entries.push(entry);
            return match &self.entries[self.entries.len() - 1].1 {
                Ok(page) => {
                    self.shown.insert(request.page, (request, page.clone()));
                    Frame::Ready(page.clone())
                }
                Err(failure) => Frame::Failed(failure.clone()),
            };
        }
        if let Err(failure) = self.submit(ctx, doc, &request) {
            return Frame::Failed(failure);
        }
        match self.shown.get(&request.page) {
            Some((_, stale)) => Frame::Stale(stale.clone()),
            None => Frame::Rendering,
        }
    }

    /// Sends `request` to the thread unless its page asked for the same bitmap already.
    fn submit(
        &mut self,
        ctx: &Context,
        doc: &Document,
        request: &RenderRequest,
    ) -> Result<(), RenderFailure> {
        if self.outstanding.get(&request.page) == Some(request) {
            return Ok(());
        }
        let worker = self
            .worker
            .get_or_insert_with(|| Worker::spawn(ctx, doc))
            .as_ref()
            .map_err(Clone::clone)?;
        worker
            .jobs
            .send(request.clone())
            .map_err(|_| RenderFailure::Stopped)?;
        self.renders += 1;
        self.outstanding.insert(request.page, request.clone());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use pdfxray_render::ContentFilter;
    use pretty_assertions::assert_eq;

    use super::*;
    use crate::view::render_scale;

    fn request(page: PageIx, scale: f32) -> RenderRequest {
        RenderRequest {
            page,
            scale: render_scale(scale, 1.0, egui::vec2(100.0, 100.0), u32::MAX),
            content: ContentFilter::All,
            xray: None,
            clip_op: None,
            heatmap: false,
        }
    }

    #[test]
    fn a_batch_keeps_the_newest_request_of_each_page_in_the_order_pages_asked() {
        let batch = vec![request(0, 1.0), request(1, 1.0), request(0, 2.0), request(2, 1.0)];

        let pages: Vec<(PageIx, f32)> = newest_per_page(batch)
            .iter()
            .map(|r| (r.page, r.scale.scale()))
            .collect();

        assert_eq!(pages, [(0, 2.0), (1, 1.0), (2, 1.0)]);
    }

    #[test]
    fn a_failure_reads_as_before_and_keeps_its_cause() {
        use pdfxray_render::RewriteError;

        use crate::state::describe;

        let spawn = RenderFailure::Spawn(Arc::new(io::Error::other("no more threads")));
        let filter = RenderFailure::Render(RenderError::Filter(RewriteError::NoPageObject));

        assert_eq!(
            describe(&spawn),
            "cannot start the render thread: no more threads"
        );
        assert_eq!(
            describe(&filter),
            "cannot filter the page content: the page object is unknown"
        );
        assert_eq!(
            describe(&RenderFailure::Crashed),
            "the renderer crashed on this page"
        );
        // smoke: the cause is the error's source, not only words in its text
        assert_eq!(
            std::error::Error::source(&spawn).map(ToString::to_string),
            Some("no more threads".to_owned())
        );
    }
}
