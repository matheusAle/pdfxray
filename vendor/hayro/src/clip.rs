use crate::{Renderer, convert_fill_rule};
use hayro_interpret::{ClipPath, FillRule};
use kurbo::{Affine, BezPath, Rect, Shape};

impl Renderer<'_> {
    pub(super) fn push_clip_path_inner(&mut self, clip_path: &BezPath, fill: FillRule) {
        let old_transform = *self.ctx.transform();

        self.ctx.set_fill_rule(convert_fill_rule(fill));
        self.ctx.set_transform(Affine::IDENTITY);
        self.ctx.push_clip_path(clip_path);

        self.ctx.set_transform(old_transform);
    }

    pub(super) fn push_clip_path(&mut self, clip_path: &ClipPath) {
        // pdfxray patch: the rules place the clip (already in device space).
        if let Some(rules) = self.rules() {
            let mut path = clip_path.path.clone();
            rules.clip(&mut path);
            self.push_clip_path_inner(&path, clip_path.fill);
            return;
        }
        self.push_clip_path_inner(&clip_path.path, clip_path.fill);
    }

    pub(super) fn push_clip_rect(&mut self, rect: &Rect) {
        if self.rules().is_some() {
            // pdfxray patch: a rectangle is a path to the rules.
            return self.push_clip_path(&ClipPath {
                path: rect.to_path(0.1),
                fill: FillRule::NonZero,
            });
        }
        self.ctx.set_fill_rule(convert_fill_rule(FillRule::NonZero));
        self.ctx.set_transform(Affine::IDENTITY);
        self.ctx.push_clip_rect(rect);
    }

    pub(super) fn pop_clip(&mut self) {
        self.ctx.pop_clip();
    }
}
