//! Object previews (`pdfxray_render::render_preview`): pictures rendered one at a time on a thread
//! of their own, and kept for the objects looked at lately.
//!
//! Like [`crate::thumbnails`], the thread owns its copy of the document and renders the object
//! asked for last first: the one on screen now matters most. A frame never waits for a preview;
//! until it is there the panel says it is being rendered.

use std::collections::HashSet;
use std::io;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

use egui::{Color32, ColorImage, Context, TextureHandle, TextureOptions};
use pdfxray_core::numeric::usize_from_u32;
use pdfxray_core::{CharCode, CoreError, Document, ObjRef, Visual, VisualKind};
use pdfxray_render::{InkMask, Preview, PreviewError, render_preview};

use crate::page_cache::pixel_digest;

/// How many previews are kept; the least recently used one is dropped first.
const CAPACITY: usize = 24;

/// A bitmap on the GPU.
#[derive(Clone)]
pub(crate) struct Bitmap {
    /// The uploaded bitmap.
    pub(crate) texture: TextureHandle,
    /// Size in pixels.
    pub(crate) size: (u32, u32),
}

/// An ink mask on the GPU: white, with the coverage as alpha, so a tint paints it in any colour.
#[derive(Clone)]
pub(crate) struct Mask {
    /// The mask.
    pub(crate) bitmap: Bitmap,
    /// Pixels per em.
    pub(crate) em: f32,
    /// Pixels from the left edge to where the pen starts, and from the top edge to the top of the
    /// font's glyph box.
    pub(crate) pad: f32,
}

/// What a preview shows, uploaded.
#[derive(Clone)]
pub(crate) enum Picture {
    /// The object's bitmap. An image is sampled nearest-neighbour, anything else linearly.
    Bitmap(Bitmap),
    /// A font's specimen and glyphs, drawn with its own program.
    Font {
        /// The specimen, when the program holds its characters.
        specimen: Option<Mask>,
        /// Each code the document shows, with its mask.
        glyphs: Vec<(CharCode, Mask)>,
    },
    /// The colour the engine paints for each swatch of a colour space.
    Swatches(Vec<Color32>),
}

/// A rendered preview.
#[derive(Clone)]
pub(crate) struct PreviewBitmap {
    /// What it shows.
    pub(crate) picture: Picture,
    /// [`preview_digest`] of what the renderer made.
    pub(crate) digest: u64,
}

/// A digest of what [`render_preview`] made, stable between runs: lets tests compare previews
/// without keeping them.
pub fn preview_digest(preview: &Preview) -> u64 {
    match preview {
        Preview::Picture(page) => pixel_digest(&page.rgba),
        Preview::Font(font) => {
            let mut bytes = Vec::new();
            for mask in font
                .specimen
                .iter()
                .chain(font.glyphs.iter().map(|(_, mask)| mask))
            {
                bytes.extend(mask.width.to_le_bytes());
                bytes.extend(&mask.coverage);
            }
            pixel_digest(&bytes)
        }
        Preview::Swatches(colors) => pixel_digest(colors.as_flattened()),
    }
}

/// Why an object has no preview. Failures are kept like bitmaps, so it is cheap to clone.
#[derive(Debug, Clone, thiserror::Error)]
pub(crate) enum PreviewFailure {
    /// The system did not start the preview thread.
    #[error("cannot start the preview thread")]
    Spawn(#[source] Arc<io::Error>),
    /// The preview thread is gone, so it takes no more requests.
    #[error("the preview thread stopped")]
    Stopped,
    /// The renderer panicked on this object.
    #[error("the renderer crashed on this object")]
    Crashed,
    /// The preview thread could not open its copy of the document.
    #[error(transparent)]
    Open(Arc<CoreError>),
    /// The object cannot be previewed.
    #[error(transparent)]
    Preview(#[from] PreviewError),
}

/// A preview, or why the object cannot be previewed; failures are kept too so they are not
/// retried every frame.
type PreviewResult = Result<PreviewBitmap, PreviewFailure>;

/// What a panel shows for a preview this frame.
pub(crate) enum PreviewFrame {
    /// The bitmap.
    Ready(PreviewBitmap),
    /// The bitmap is being rendered.
    Rendering,
    /// The object cannot be previewed, and why.
    Failed(PreviewFailure),
}

type Done = (ObjRef, PreviewResult);

/// The thread, as seen from the UI. Dropping it stops the thread once it is idle.
struct Worker {
    jobs: Sender<Visual>,
    done: Receiver<Done>,
}

impl Worker {
    fn spawn(ctx: &Context, doc: &Document) -> Result<Self, PreviewFailure> {
        let bytes = doc.bytes().to_vec();
        let (jobs, jobs_rx) = mpsc::channel();
        let (done_tx, done) = mpsc::channel();
        let ctx = ctx.clone();
        thread::Builder::new()
            .name("pdfxray-previews".to_owned())
            .spawn(move || work(&ctx, bytes, &jobs_rx, &done_tx))
            .map_err(|e| PreviewFailure::Spawn(Arc::new(e)))?;
        Ok(Self { jobs, done })
    }
}

/// Body of the thread: renders the object asked for last, until the UI hangs up.
fn work(ctx: &Context, bytes: Vec<u8>, jobs: &Receiver<Visual>, done: &Sender<Done>) {
    let doc = Document::from_bytes("previews", bytes).map_err(|e| PreviewFailure::Open(Arc::new(e)));
    let mut stack: Vec<Visual> = Vec::new();
    loop {
        if stack.is_empty() {
            let Ok(visual) = jobs.recv() else {
                return;
            };
            stack.push(visual);
        }
        stack.extend(jobs.try_iter());
        let Some(visual) = stack.pop() else {
            continue;
        };
        let result = match &doc {
            Ok(doc) => catch_unwind(AssertUnwindSafe(|| render(ctx, doc, &visual)))
                .unwrap_or(Err(PreviewFailure::Crashed)),
            Err(failure) => Err(failure.clone()),
        };
        if done.send((visual.obj, result)).is_err() {
            return;
        }
        ctx.request_repaint();
    }
}

fn render(ctx: &Context, doc: &Document, visual: &Visual) -> PreviewResult {
    let preview = render_preview(doc, visual)?;
    let name = |part: &str| format!("preview-{}-{part}", visual.obj);
    let picture = match &preview {
        Preview::Picture(page) => {
            // An image is shown pixel for pixel; any other picture is a drawing, smoothed.
            let options = if matches!(visual.kind, VisualKind::Image(_)) {
                TextureOptions::NEAREST
            } else {
                TextureOptions::LINEAR
            };
            let image = ColorImage::from_rgba_unmultiplied(
                [usize_from_u32(page.width), usize_from_u32(page.height)],
                &page.rgba,
            );
            Picture::Bitmap(Bitmap {
                texture: ctx.load_texture(name("picture"), image, options),
                size: (page.width, page.height),
            })
        }
        Preview::Font(font) => Picture::Font {
            specimen: font
                .specimen
                .as_ref()
                .map(|mask| upload_mask(ctx, &name("specimen"), mask)),
            glyphs: font
                .glyphs
                .iter()
                .map(|(code, mask)| {
                    let texture = upload_mask(ctx, &name(&format!("glyph-{}", code.hex)), mask);
                    (code.clone(), texture)
                })
                .collect(),
        },
        Preview::Swatches(colors) => Picture::Swatches(
            colors
                .iter()
                .map(|[r, g, b]| Color32::from_rgb(*r, *g, *b))
                .collect(),
        ),
    };
    Ok(PreviewBitmap {
        picture,
        digest: preview_digest(&preview),
    })
}

fn upload_mask(ctx: &Context, name: &str, mask: &InkMask) -> Mask {
    let pixels = mask
        .coverage
        .iter()
        .map(|a| Color32::from_white_alpha(*a))
        .collect();
    let image = ColorImage::new([usize_from_u32(mask.width), usize_from_u32(mask.height)], pixels);
    Mask {
        bitmap: Bitmap {
            texture: ctx.load_texture(name, image, TextureOptions::LINEAR),
            size: (mask.width, mask.height),
        },
        em: mask.em,
        pad: mask.pad,
    }
}

/// The previews rendered so far and the thread that renders more.
#[derive(Default)]
pub(crate) struct Previews {
    /// Started on the first request of a document.
    worker: Option<Result<Worker, PreviewFailure>>,
    /// Objects the thread was asked for and has not answered yet.
    outstanding: HashSet<ObjRef>,
    /// Finished previews, least recently used first.
    ready: Vec<Done>,
}

impl Previews {
    /// Drops everything, e.g. when another document is opened; the thread stops by itself.
    pub(crate) fn clear(&mut self) {
        self.worker = None;
        self.outstanding.clear();
        self.ready.clear();
    }

    /// True while an object that was asked for has no preview yet.
    pub(crate) fn is_pending(&self) -> bool {
        !self.outstanding.is_empty()
    }

    /// Digest of the preview of `r`, once it is rendered.
    pub(crate) fn digest(&self, r: ObjRef) -> Option<u64> {
        self.ready
            .iter()
            .find(|(key, _)| *key == r)
            .and_then(|(_, result)| result.as_ref().ok())
            .map(|bitmap| bitmap.digest)
    }

    /// Takes in what the thread finished.
    pub(crate) fn poll(&mut self) {
        let Some(Ok(worker)) = &self.worker else {
            return;
        };
        let finished: Vec<Done> = worker.done.try_iter().collect();
        for (r, result) in finished {
            self.outstanding.remove(&r);
            self.ready.retain(|(key, _)| *key != r);
            if self.ready.len() == CAPACITY {
                drop(self.ready.remove(0));
            }
            self.ready.push((r, result));
        }
    }

    /// The preview of `visual`; asks the thread for it the first time.
    pub(crate) fn get(&mut self, ctx: &Context, doc: &Document, visual: &Visual) -> PreviewFrame {
        self.poll();
        if let Some(at) = self.ready.iter().position(|(key, _)| *key == visual.obj) {
            let entry = self.ready.remove(at);
            let frame = match &entry.1 {
                Ok(bitmap) => PreviewFrame::Ready(bitmap.clone()),
                Err(failure) => PreviewFrame::Failed(failure.clone()),
            };
            self.ready.push(entry);
            return frame;
        }
        if self.outstanding.contains(&visual.obj) {
            return PreviewFrame::Rendering;
        }
        let worker = match self.worker.get_or_insert_with(|| Worker::spawn(ctx, doc)) {
            Ok(worker) => worker,
            Err(failure) => return PreviewFrame::Failed(failure.clone()),
        };
        if worker.jobs.send(visual.clone()).is_err() {
            return PreviewFrame::Failed(PreviewFailure::Stopped);
        }
        self.outstanding.insert(visual.obj);
        PreviewFrame::Rendering
    }
}

#[cfg(test)]
mod tests {
    use pdfxray_render::RewriteError;
    use pretty_assertions::assert_eq;

    use super::*;
    use crate::state::describe;

    #[test]
    fn a_failure_reads_as_before_and_keeps_its_cause() {
        let build = PreviewFailure::Preview(PreviewError::Build(RewriteError::TooLarge));
        let spawn = PreviewFailure::Spawn(Arc::new(io::Error::other("no more threads")));

        assert_eq!(
            describe(&build),
            "cannot build the preview page: the file is too large to extend"
        );
        assert_eq!(
            describe(&spawn),
            "cannot start the preview thread: no more threads"
        );
        assert_eq!(describe(&PreviewFailure::Stopped), "the preview thread stopped");
        // smoke: the font reasons the panel turns into a sentence are unchanged
        assert_eq!(
            describe(&PreviewFailure::Preview(PreviewError::Unreadable)),
            "the embedded font program cannot be read; a viewer would substitute another font"
        );
    }
}
