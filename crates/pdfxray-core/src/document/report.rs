//! A veraPDF report attached to the document, with each failed check resolved to the node it
//! concerns.

use super::Document;
use crate::capture::StreamRef;
use crate::validation::{Locator, Report, ValidationRun, locate_static};
use crate::{NodeId, PageIx, ResKind};

impl Document {
    /// Attaches a veraPDF report, replacing any previous one. Each failed check is resolved to
    /// the node it concerns where possible.
    pub fn set_validation(&mut self, report: &Report) {
        let run = ValidationRun::from_report(report, |l| self.locate(l));
        self.validation = Some(run);
    }

    /// The attached validation run, if any.
    pub fn validation(&self) -> Option<&ValidationRun> {
        self.validation.as_ref()
    }

    /// Resolves a veraPDF locator to a node: operators via the captured page content (veraPDF
    /// counts operators of the page's own content, like `OpRecord::local_ix`), fonts via the
    /// page's resources, annotations to their page.
    fn locate(&self, locator: &Locator) -> Option<NodeId> {
        let id = match locator {
            Locator::ContentOperator { page, operator_index } => self
                .captured(*page)
                .and_then(|c| {
                    c.ops.iter().find(|op| {
                        matches!(op.stream, StreamRef::PageContent { .. }) && op.local_ix == *operator_index
                    })
                })
                .map_or(NodeId::Page(*page), |op| NodeId::Op {
                    page: *page,
                    ix: op.ix,
                }),
            Locator::Annotation { page, .. } => NodeId::Page(*page),
            Locator::Font {
                page: Some(page),
                name,
            } => {
                match (
                    self.page(*page).and_then(|p| p.obj),
                    self.font_resource_named(*page, name),
                ) {
                    (Some(owner), Some(res)) => NodeId::Resource {
                        owner,
                        kind: ResKind::Font,
                        name: res,
                    },
                    _ => NodeId::Page(*page),
                }
            }
            Locator::Font { page: None, .. } => NodeId::Document,
            other => locate_static(other)?,
        };
        match &id {
            NodeId::Page(p) | NodeId::MarkedContent { page: p, .. } | NodeId::Op { page: p, .. }
                if self.page(*p).is_none() =>
            {
                None
            }
            _ => Some(id),
        }
    }

    /// The resource name (e.g. `F1`) under which `page`'s resources hold the font whose
    /// `/BaseFont` is `base_font`.
    fn font_resource_named(&self, page: PageIx, base_font: &str) -> Option<String> {
        let owner = self.page(page)?.resources_from?;
        let owner_obj = self.objects.get(owner)?;
        let resources = self
            .raw
            .resolve(&owner_obj.value.value.as_dict()?.get(b"Resources")?.value)
            .into_owned();
        let fonts = self
            .raw
            .resolve(&resources.as_dict()?.get(b"Font")?.value)
            .into_owned();
        fonts.as_dict()?.entries.iter().find_map(|(key, value)| {
            let font = self.raw.resolve(&value.value).into_owned();
            let base = font.as_dict()?.get_name(b"BaseFont")?.to_string();
            (base.trim_start_matches('/') == base_font)
                .then(|| key.value.to_string().trim_start_matches('/').to_owned())
        })
    }
}
