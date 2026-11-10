//! *Save image…*: an image of the document written to a PNG file, at its own size (one pixel
//! per sample, however large) and with its transparency (a soft mask, a stencil mask), as the
//! engine that draws the pages decodes it ([`pdfxray_render::render_image`]).
//!
//! It is offered for an image object and for a `Do` that paints one. The image is drawn on the UI
//! thread when the file is chosen; the status bar says where it went, or why it could not.

use std::path::{Path, PathBuf};

use pdfxray_core::{NodeId, ObjRef, VisualKind};
use pdfxray_render::render_image;

use crate::state::AppState;

/// The name the Save dialog suggests for image `r`: `image-5-0.png`.
pub fn suggested_name(r: ObjRef) -> String {
    format!("image-{}-{}.png", r.num, r.generation)
}

/// What the status bar says after a save: `Saved image 5 0 R to image-5-0.png · 2100 × 300 px`.
pub fn saved_text(r: ObjRef, path: &Path, width: u32, height: u32) -> String {
    let name = path.file_name().map_or_else(
        || path.display().to_string(),
        |n| n.to_string_lossy().into_owned(),
    );
    format!("Saved image {r} to {name} · {width} × {height} px")
}

impl AppState {
    /// The image object behind `id`: an image object itself, or the image `XObject` a `Do`
    /// paints. `None` for anything else, an inline image included (it is not an object).
    pub fn image_object(&self, id: &NodeId) -> Option<ObjRef> {
        let doc = &self.document.as_ref()?.doc;
        let r = match id {
            NodeId::Object(r) => *r,
            NodeId::Op { page, ix } => doc.xobject_of_op(*page, *ix)?,
            _ => return None,
        };
        matches!(doc.visual(r)?.kind, VisualKind::Image(_)).then_some(r)
    }

    /// Asks where to save image `r` and writes it there as a PNG. Returns the path written; the
    /// status bar says what happened, also when nothing was written (cancelled: nothing is
    /// said).
    pub fn save_image(&mut self, r: ObjRef) -> Option<PathBuf> {
        let visual = self.document.as_ref()?.doc.visual(r)?;
        let path = (self.save_picker)(&suggested_name(r))?;
        let doc = &self.document.as_ref()?.doc;
        let written = render_image(doc, &visual)
            .map_err(|e| e.to_string())
            .and_then(|page| {
                image::save_buffer_with_format(
                    &path,
                    &page.rgba,
                    page.width,
                    page.height,
                    image::ExtendedColorType::Rgba8,
                    image::ImageFormat::Png,
                )
                .map(|()| (page.width, page.height))
                .map_err(|e| e.to_string())
            });
        match written {
            Ok((width, height)) => {
                self.status = saved_text(r, &path, width, height);
                Some(path)
            }
            Err(why) => {
                self.status = format!("Could not save image {r}: {why}");
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;
    use crate::test_support::install_fixture;

    #[test]
    fn an_image_or_the_do_that_paints_it_saves_as_a_png_of_its_own_size() {
        // images: object 5 is a 2 × 2 RGB image; op 2 paints it, op 6 is an inline image.
        let mut state = AppState::default();
        install_fixture(&mut state, "images");
        let dir = std::env::temp_dir().join("pdfxray-image-save");
        std::fs::create_dir_all(&dir).unwrap();
        let target = dir.join("saved.png");
        let given = target.clone();
        state.save_picker = Box::new(move |name| {
            assert_eq!(name, "image-5-0.png");
            Some(given.clone())
        });

        let image = state.image_object(&NodeId::Op { page: 0, ix: 2 });
        let written = image.and_then(|r| state.save_image(r));

        assert_eq!(image, Some(ObjRef::new(5, 0)));
        assert_eq!(written.as_deref(), Some(target.as_path()));
        assert_eq!(state.status, "Saved image 5 0 R to saved.png · 2 × 2 px");
        let png = image::open(&target).unwrap().to_rgba8();
        assert_eq!(png.dimensions(), (2, 2));
        assert_eq!(
            png.get_pixel(0, 0).0,
            [0xFF, 0, 0, 0xFF],
            "the top-left sample, red"
        );
        // Smoke: an inline image, a page and a cancelled dialog save nothing.
        assert_eq!(state.image_object(&NodeId::Op { page: 0, ix: 6 }), None);
        assert_eq!(state.image_object(&NodeId::Page(0)), None);
        state.save_picker = Box::new(|_| None);
        assert_eq!(state.save_image(ObjRef::new(5, 0)), None);
    }
}
