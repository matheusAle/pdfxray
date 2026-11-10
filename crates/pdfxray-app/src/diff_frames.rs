//! The pictures of a comparison: one composite bitmap per page pair ([`pdfxray_render::composite`]),
//! drawn on a thread of its own from the two documents the UI holds.
//!
//! One bitmap per page pair on the canvas, not one per side: the composite is the picture an
//! export writes, so the app shows that very bitmap and lays the annotations over it with the
//! same layout ([`pdfxray_render::composite::layout`]). A page only one side has is that side's
//! page alone.
//!
//! Each picture of two pages on one grid also carries their visual score
//! ([`pdfxray_render::score`]): the mask counts it anyway, overlaid and side by side (pages of
//! one size) it is one more pass over pixels already drawn. The score is kept per page pair, so
//! the Changes list shows it for every page pair the canvas has drawn, at no further rendering.

use std::collections::HashMap;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::Duration;

use egui::{ColorImage, Context, TextureHandle, TextureOptions};
use pdfxray_core::diff::Side;
use pdfxray_core::numeric::usize_from_u32;
use pdfxray_core::{Document, PageIx};
use pdfxray_render::composite::{self, Comparator, Composite, CompositeError, Fit, Mode, Request};
use pdfxray_render::score::VisualScore;
use pdfxray_render::{ContentFilter, RenderSpec, Transform, render_page};

use crate::page_cache::pixel_digest;
use crate::view::ScaleKey;

/// How many pictures are kept; the least recently used goes first.
const CAPACITY: usize = 6;

/// How soon the UI is woken after the thread finished a picture.
const WAKE_DELAY: Duration = Duration::from_millis(1);

/// What to draw: a page pair, how, at what scale.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PictureKey {
    /// A's page, if A has one in the pair.
    pub(crate) a: Option<PageIx>,
    /// B's page, if B has one in the pair.
    pub(crate) b: Option<PageIx>,
    /// How the two are shown.
    pub(crate) mode: Mode,
    /// Pixels per point.
    pub(crate) scale: ScaleKey,
    /// What to do about differing page geometry.
    pub(crate) fit: Fit,
    /// The paint-order stepper's cut: that side's page painted up to this operator.
    pub(crate) step: Option<(Side, u32)>,
}

/// Where the sides landed on a picture.
#[derive(Debug, Clone)]
pub(crate) enum Placed {
    /// Both pages, as [`Composite`] says (its bitmap is on the GPU, not here).
    Both(Box<Composite>),
    /// One side's page alone, with its page-to-pixel transform.
    One(Side, Transform),
}

/// A picture on the GPU.
#[derive(Clone)]
pub(crate) struct Picture {
    /// The bitmap.
    pub(crate) texture: TextureHandle,
    /// Its size in pixels.
    pub(crate) size: (u32, u32),
    /// Where each side's page is on it.
    pub(crate) placed: Arc<Placed>,
    /// [`pixel_digest`] of the bitmap.
    pub(crate) digest: u64,
}

/// Why a page pair has no picture. Said, never swallowed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Refusal {
    /// The pages differ in size or `/Rotate` and scaling B to A was not asked for: the reason.
    Geometry(String),
    /// A side cannot be drawn (encrypted, unreadable): the reason.
    Render(String),
}

type Outcome = Result<Picture, Refusal>;
type Done = (PictureKey, Outcome);

/// What the UI shows for a key this frame.
pub(crate) enum Shown {
    /// The picture for the key.
    Ready(Picture),
    /// The key is being drawn; the last picture of the same page pair meanwhile.
    Stale(Picture),
    /// The key is being drawn and nothing was shown before.
    Drawing,
    /// No picture, and why.
    Refused(Refusal),
}

struct Worker {
    jobs: Sender<PictureKey>,
    done: Receiver<Done>,
    /// The documents it draws from, to tell when they were replaced.
    docs: (Arc<Document>, Arc<Document>),
}

/// The pictures drawn so far and the thread that draws new ones.
#[derive(Default)]
pub(crate) struct DiffFrames {
    entries: Vec<(PictureKey, Outcome)>,
    worker: Option<Worker>,
    outstanding: Option<PictureKey>,
    shown: Option<(PictureKey, Picture)>,
    renders: usize,
    /// The visual score of every page pair drawn on one grid, by A's and B's page: kept when
    /// the picture is evicted, so moving between page pairs never draws one again for it.
    scores: HashMap<(PageIx, PageIx), VisualScore>,
    /// Page pairs scored since [`DiffFrames::take_scored`] last asked.
    scored: Vec<(PageIx, PageIx)>,
}

impl DiffFrames {
    /// Number of pictures asked of the thread so far.
    pub(crate) fn renders(&self) -> usize {
        self.renders
    }

    /// True while the thread has not answered the latest key.
    pub(crate) fn is_pending(&self) -> bool {
        self.outstanding.is_some()
    }

    /// The digest of the picture last shown, and the key it answers.
    pub(crate) fn shown(&self) -> Option<(PictureKey, u64)> {
        self.shown.as_ref().map(|(key, p)| (*key, p.digest))
    }

    /// The visual score of the page pair `(a, b)`, once the canvas has drawn it unscaled.
    pub(crate) fn score(&self, a: PageIx, b: PageIx) -> Option<&VisualScore> {
        self.scores.get(&(a, b))
    }

    /// Every score so far, by page pair.
    pub(crate) fn scores(&self) -> &HashMap<(PageIx, PageIx), VisualScore> {
        &self.scores
    }

    /// The page pairs scored since the last call.
    pub(crate) fn take_scored(&mut self) -> Vec<(PageIx, PageIx)> {
        std::mem::take(&mut self.scored)
    }

    /// Drops everything: a side was read again, or the comparison ended.
    pub(crate) fn clear(&mut self) {
        *self = Self::default();
    }

    /// The picture for `key` of `a` against `b`; asks the thread for it on a miss.
    pub(crate) fn get(
        &mut self,
        ctx: &Context,
        a: &Arc<Document>,
        b: &Arc<Document>,
        key: PictureKey,
    ) -> Shown {
        let same_docs = self
            .worker
            .as_ref()
            .is_some_and(|w| Arc::ptr_eq(&w.docs.0, a) && Arc::ptr_eq(&w.docs.1, b));
        if !same_docs {
            self.clear();
            self.worker = Worker::spawn(ctx, a, b);
        }
        self.poll();
        if let Some(at) = self.entries.iter().position(|(k, _)| *k == key) {
            let entry = self.entries.remove(at);
            let shown = match &entry.1 {
                Ok(picture) => {
                    self.shown = Some((key, picture.clone()));
                    Shown::Ready(picture.clone())
                }
                Err(refusal) => Shown::Refused(refusal.clone()),
            };
            self.entries.push(entry);
            return shown;
        }
        if self.outstanding != Some(key) {
            let Some(worker) = &self.worker else {
                return Shown::Refused(Refusal::Render("cannot start the drawing thread".to_owned()));
            };
            if worker.jobs.send(key).is_err() {
                return Shown::Refused(Refusal::Render("the drawing thread stopped".to_owned()));
            }
            self.renders += 1;
            self.outstanding = Some(key);
        }
        match &self.shown {
            Some((shown, picture)) if shown.a == key.a && shown.b == key.b => Shown::Stale(picture.clone()),
            _ => Shown::Drawing,
        }
    }

    /// Takes in what the thread finished.
    pub(crate) fn poll(&mut self) {
        let Some(worker) = &self.worker else {
            return;
        };
        let finished: Vec<Done> = worker.done.try_iter().collect();
        for (key, outcome) in finished {
            if self.outstanding == Some(key) {
                self.outstanding = None;
            }
            if let (Some(a), Some(b), Ok(picture)) = (key.a, key.b, &outcome)
                && let Placed::Both(c) = picture.placed.as_ref()
                // A score of B scaled onto A is not the page pair's.
                && c.b_scale.is_none()
                && let Some(visual) = c.delta.as_ref().and_then(|d| d.visual())
                && self.scores.insert((a, b), *visual).is_none_or(|old| old != *visual)
            {
                self.scored.push((a, b));
            }
            self.entries.retain(|(k, _)| *k != key);
            if self.entries.len() == CAPACITY {
                drop(self.entries.remove(0));
            }
            self.entries.push((key, outcome));
        }
    }
}

impl Worker {
    fn spawn(ctx: &Context, a: &Arc<Document>, b: &Arc<Document>) -> Option<Self> {
        let (jobs, jobs_rx) = mpsc::channel::<PictureKey>();
        let (done_tx, done) = mpsc::channel();
        let (da, db, ctx) = (Arc::clone(a), Arc::clone(b), ctx.clone());
        thread::Builder::new()
            .name("pdfxray-diff".to_owned())
            .spawn(move || {
                while let Ok(first) = jobs_rx.recv() {
                    // Only the newest key matters: the canvas shows one page pair.
                    let key = jobs_rx.try_iter().last().unwrap_or(first);
                    let outcome = catch_unwind(AssertUnwindSafe(|| draw(&ctx, &da, &db, key)))
                        .unwrap_or_else(|_| {
                            Err(Refusal::Render("the renderer crashed on this page".to_owned()))
                        });
                    if done_tx.send((key, outcome)).is_err() {
                        return;
                    }
                    ctx.request_repaint_after(WAKE_DELAY);
                }
            })
            .ok()?;
        Some(Self {
            jobs,
            done,
            docs: (Arc::clone(a), Arc::clone(b)),
        })
    }
}

/// Draws `key`: the composite of both pages, or the one page a side has alone.
fn draw(ctx: &Context, a: &Document, b: &Document, key: PictureKey) -> Outcome {
    let scale = key.scale.scale();
    let (width, height, rgba, placed) = match (key.a, key.b) {
        (Some(pa), Some(pb)) => {
            let request = Request {
                mode: key.mode,
                scale,
                fit: key.fit,
                registration: None,
                comparator: Comparator::same_engine(),
                step: key.step,
            };
            let mut picture = composite::composite((a, pa), (b, pb), &request).map_err(|e| match e {
                CompositeError::Geometry(why) => Refusal::Geometry(why),
                render @ CompositeError::Render { .. } => Refusal::Render(render.to_string()),
            })?;
            let rgba = std::mem::take(&mut picture.rgba);
            (
                picture.width,
                picture.height,
                rgba,
                Placed::Both(Box::new(picture)),
            )
        }
        (Some(page), None) | (None, Some(page)) => {
            let (side, doc) = if key.a.is_some() {
                (Side::A, a)
            } else {
                (Side::B, b)
            };
            let content = match key.step {
                Some((stepped, ix)) if stepped == side => ContentFilter::UpTo(ix),
                _ => ContentFilter::All,
            };
            let spec = RenderSpec {
                content,
                ..RenderSpec::new(scale)
            };
            let rendered = render_page(doc, page, &spec).map_err(|e| {
                Refusal::Render(format!(
                    "side {} cannot be drawn, so there is no visual diff: {e}",
                    if side == Side::A { 'A' } else { 'B' }
                ))
            })?;
            (
                rendered.width,
                rendered.height,
                rendered.rgba,
                Placed::One(side, rendered.page_to_pixel),
            )
        }
        (None, None) => return Err(Refusal::Render("no page on either side".to_owned())),
    };
    let digest = pixel_digest(&rgba);
    let image = ColorImage::from_rgba_unmultiplied([usize_from_u32(width), usize_from_u32(height)], &rgba);
    Ok(Picture {
        texture: ctx.load_texture(
            format!("diff-{:?}-{:?}-{:?}", key.a, key.b, key.mode),
            image,
            TextureOptions::LINEAR,
        ),
        size: (width, height),
        placed: Arc::new(placed),
        digest,
    })
}
