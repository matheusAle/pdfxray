//! The Objects entry: which view of the object is showing, the revision being looked at, and
//! the lists and facts the navigator, the centre, the drawer and the inspector share.

use std::collections::{BTreeSet, HashSet};
use std::ops::Range;
use std::sync::Arc;

use pdfxray_core::{NodeId, ObjRef, PageIx, ResKind};

use super::{AppState, RailItem};
use crate::object_facts::{self, ObjectFacts, short_label};

/// The two views of an object.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ObjectMode {
    /// The dictionary and what the data means.
    #[default]
    Decoded,
    /// The bytes of the file, in hex.
    Raw,
}

impl ObjectMode {
    /// Both views, in selector order.
    pub const ALL: [Self; 2] = [Self::Decoded, Self::Raw];

    /// Text on the selector.
    pub const fn title(self) -> &'static str {
        match self {
            Self::Decoded => "Decoded",
            Self::Raw => "Raw bytes",
        }
    }
}

/// How the centre preview sizes its picture: the Fit · 1:1 control of the card.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum PreviewScale {
    /// The picture fits the card; an image by a whole factor while it is enlarged.
    #[default]
    Fit,
    /// One pixel of the preview per screen pixel; the canvas scrolls.
    Actual,
}

impl PreviewScale {
    /// Both, in control order.
    pub const ALL: [Self; 2] = [Self::Fit, Self::Actual];

    /// Text on the control.
    pub const fn title(self) -> &'static str {
        match self {
            Self::Fit => "Fit",
            Self::Actual => "1:1",
        }
    }
}

/// One row of the "All objects" list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectRow {
    /// The object.
    pub r: ObjRef,
    /// What it is: `stream`, `/Page`.
    pub detail: String,
    /// Lowercase text the filter looks in.
    pub haystack: String,
    /// The short word of the first diagnostic on it.
    pub diagnostic: Option<String>,
    /// The revision number (1-based) of its newest definition, when a later revision wrote it
    /// again.
    pub changed_in: Option<u32>,
}

/// A row of the "Document" section: the trailer, the catalog, the info dictionary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentRow {
    /// What is selected when the row is clicked.
    pub id: NodeId,
    /// `trailer`, `/Catalog`, `/Info`.
    pub label: String,
    /// The reference, for the objects.
    pub detail: Option<String>,
}

/// A resource of the current page, as a row of the navigator.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourceRow {
    /// The node of the resource.
    pub id: NodeId,
    /// `/F1`.
    pub name: String,
    /// What the core says about it, without the use count.
    pub detail: String,
    /// The object it resolves to, when known.
    pub obj: Option<ObjRef>,
    /// How many operators select it, when known.
    pub uses: Option<usize>,
    /// Fonts only: no font program is embedded.
    pub not_embedded: bool,
    /// A validation finding points at it.
    pub finding: bool,
}

/// The resources of one category.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourceGroup {
    /// The category.
    pub kind: ResKind,
    /// Its resources.
    pub rows: Vec<ResourceRow>,
}

/// What the resource list was built for: the page, and the validation run whose findings it marks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ResourcesKey {
    page: PageIx,
    findings: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct FactsKey {
    object: Option<ObjRef>,
    revision: Option<u32>,
}

/// State of the Objects entry.
#[derive(Debug)]
pub struct ObjectsState {
    /// Decoded or raw bytes.
    pub mode: ObjectMode,
    /// Whether the plain-language note shows next to each operator.
    pub explain: bool,
    /// How the centre preview sizes its picture.
    pub preview_scale: PreviewScale,
    /// The bytes to highlight in the raw view (a diagnostic's range, the stream's data).
    pub highlight: Option<Range<usize>>,
    /// The raw view should scroll to the highlight the next time it is drawn.
    pub(crate) scroll_to_highlight: bool,
    /// The revision shown read-only; `None` shows the newest.
    revision: Option<u32>,
    /// The operator row the user clicked.
    pub selected_op: Option<u32>,
    /// The resource categories that are expanded in the navigator.
    pub open_groups: HashSet<ResKind>,
    /// The navigator should scroll the selected object into view.
    pub(crate) reveal: bool,
    /// The selection the navigator last saw, to tell a click on its own row from a selection made
    /// elsewhere.
    pub(crate) seen_selection: Option<NodeId>,
    facts: Option<(FactsKey, Option<Arc<ObjectFacts>>)>,
    rows: Option<(Option<u32>, Arc<Vec<ObjectRow>>)>,
    document_rows: Option<Arc<Vec<DocumentRow>>>,
    resources: Option<(ResourcesKey, Arc<Vec<ResourceGroup>>)>,
}

impl Default for ObjectsState {
    fn default() -> Self {
        Self {
            mode: ObjectMode::Decoded,
            explain: true,
            preview_scale: PreviewScale::Fit,
            highlight: None,
            scroll_to_highlight: false,
            revision: None,
            selected_op: None,
            open_groups: HashSet::from([ResKind::Font, ResKind::XObject]),
            reveal: false,
            seen_selection: None,
            facts: None,
            rows: None,
            document_rows: None,
            resources: None,
        }
    }
}

impl AppState {
    /// The revision shown read-only, `None` for the newest.
    pub const fn viewed_revision(&self) -> Option<u32> {
        self.objects.revision
    }

    /// Shows the objects as of `revision` (0 is the original file). The newest revision is the
    /// file as it is, so it clears the read-only view. Out-of-range revisions are ignored.
    pub fn set_revision(&mut self, revision: Option<u32>) {
        let Some(loaded) = &self.document else {
            return;
        };
        let count = u32::try_from(loaded.info.revision_count).unwrap_or(u32::MAX);
        let revision = match revision {
            Some(r) if r + 1 < count => Some(r),
            Some(r) if r + 1 == count => None,
            Some(_) => return,
            None => None,
        };
        if revision == self.objects.revision {
            return;
        }
        self.objects.revision = revision;
        self.objects.selected_op = None;
        self.objects.highlight = None;
        self.objects.reveal = true;
        self.set_rail(RailItem::Objects);
    }

    /// Switches the object view to the raw bytes. `highlight` is the range to mark and scroll to.
    pub fn show_raw(&mut self, highlight: Option<Range<usize>>) {
        self.objects.mode = ObjectMode::Raw;
        self.objects.scroll_to_highlight = highlight.is_some();
        self.objects.highlight = highlight;
        self.set_rail(RailItem::Objects);
    }

    /// "Show bytes" on a diagnostic: the raw view with `range` marked.
    pub fn show_bytes(&mut self, range: Range<usize>) {
        self.show_raw(Some(range));
    }

    /// The facts of the selected object for the current revision, as of the last
    /// [`Self::refresh_object_facts`].
    pub fn object_facts(&self) -> Option<&Arc<ObjectFacts>> {
        self.objects.facts.as_ref()?.1.as_ref()
    }

    /// Builds the facts of the selected object, when it changed since the last frame. Called once
    /// per frame before any panel draws.
    pub fn refresh_object_facts(&mut self) {
        let wanted = match &self.selection.primary {
            Some(NodeId::Object(r)) => Some(FactsKey {
                object: Some(*r),
                revision: self.objects.revision,
            }),
            Some(NodeId::Trailer) => Some(FactsKey {
                object: None,
                revision: None,
            }),
            _ => None,
        };
        let Some(wanted) = wanted else {
            self.objects.facts = None;
            return;
        };
        if self.objects.facts.as_ref().is_some_and(|(key, _)| *key == wanted) {
            return;
        }
        let Some(loaded) = &self.document else {
            return;
        };
        let built = match wanted.object {
            Some(r) => object_facts::build(loaded, r, wanted.revision),
            None => object_facts::build_trailer(loaded),
        };
        self.objects.selected_op = None;
        self.objects.facts = Some((wanted, built.map(Arc::new)));
    }

    /// Every object that exists as of the viewed revision, in object order.
    pub fn object_rows(&mut self) -> Arc<Vec<ObjectRow>> {
        let revision = self.objects.revision;
        if let Some((cached, rows)) = &self.objects.rows
            && *cached == revision
        {
            return Arc::clone(rows);
        }
        let rows = Arc::new(self.build_rows());
        self.objects.rows = Some((revision, Arc::clone(&rows)));
        rows
    }

    fn build_rows(&self) -> Vec<ObjectRow> {
        let Some(loaded) = &self.document else {
            return Vec::new();
        };
        let doc = &loaded.doc;
        let latest = u32::try_from(loaded.info.revision_count.saturating_sub(1)).unwrap_or(u32::MAX);
        let at = self.objects.revision.unwrap_or(latest);
        let refs: BTreeSet<ObjRef> = doc
            .revisions()
            .into_iter()
            .filter(|rev| rev.index <= at)
            .flat_map(|rev| rev.objects_defined)
            .collect();
        refs.into_iter()
            .filter_map(|r| {
                let summary = doc.object_summary_at(r, at)?;
                let diagnostic = (at == latest)
                    .then(|| {
                        loaded
                            .info
                            .diagnostics
                            .iter()
                            .find(|d| d.object == Some(r))
                            .map(|d| short_label(d.code))
                    })
                    .flatten();
                let changed_in = (at == latest && doc.changed_in_later_revision(r))
                    .then(|| doc.object_last_changed(r).map(|rev| rev + 1))
                    .flatten();
                let detail = row_detail(&summary);
                Some(ObjectRow {
                    haystack: format!(
                        "{} {} obj {detail} {}",
                        r.num,
                        r.generation,
                        diagnostic.as_deref().unwrap_or("")
                    )
                    .to_lowercase(),
                    r,
                    detail,
                    diagnostic,
                    changed_in,
                })
            })
            .collect()
    }

    /// The rows of the "Document" section: the trailer and the objects it names.
    pub fn document_rows(&mut self) -> Arc<Vec<DocumentRow>> {
        if let Some(rows) = &self.objects.document_rows {
            return Arc::clone(rows);
        }
        let mut rows = vec![DocumentRow {
            id: NodeId::Trailer,
            label: "trailer".to_owned(),
            detail: None,
        }];
        let trailer = self
            .document
            .as_ref()
            .and_then(|loaded| loaded.doc.node(&NodeId::Trailer));
        for (key, label) in [("/Root", "/Catalog"), ("/Info", "/Info")] {
            let link = trailer
                .iter()
                .flat_map(|props| props.groups.iter().flat_map(|g| g.rows.iter()))
                .find(|row| row.key == key)
                .and_then(|row| match row.link {
                    Some(NodeId::Object(r)) => Some(r),
                    _ => None,
                });
            if let Some(r) = link {
                rows.push(DocumentRow {
                    id: NodeId::Object(r),
                    label: label.to_owned(),
                    detail: Some(r.to_string()),
                });
            }
        }
        let rows = Arc::new(rows);
        self.objects.document_rows = Some(Arc::clone(&rows));
        rows
    }

    /// The resources of the page on the canvas, by category.
    pub fn page_resources(&mut self) -> Arc<Vec<ResourceGroup>> {
        let key = ResourcesKey {
            page: self.canvas.page,
            findings: self
                .document
                .as_ref()
                .and_then(|loaded| loaded.doc.validation())
                .map(|run| run.findings.len()),
        };
        if let Some((cached, groups)) = &self.objects.resources
            && *cached == key
        {
            return Arc::clone(groups);
        }
        let groups = Arc::new(self.build_resources(key.page));
        self.objects.resources = Some((key, Arc::clone(&groups)));
        groups
    }

    fn build_resources(&self, page: PageIx) -> Vec<ResourceGroup> {
        let Some(loaded) = &self.document else {
            return Vec::new();
        };
        let doc = &loaded.doc;
        let uses = doc
            .page(page)
            .and_then(|p| p.obj)
            .map(|obj| doc.object_refs(obj).uses)
            .unwrap_or_default();
        let root = doc
            .tree(pdfxray_core::TreeKind::Resources)
            .into_iter()
            .find(|node| node.id == NodeId::Page(page));
        let Some(root) = root else {
            return Vec::new();
        };
        root.children
            .into_iter()
            .filter_map(|group| {
                let mut kind = None;
                let rows: Vec<ResourceRow> = group
                    .children
                    .into_iter()
                    .filter_map(|entry| {
                        let NodeId::Resource { kind: k, name, .. } = &entry.id else {
                            return None;
                        };
                        kind = Some(*k);
                        let (detail, count) = split_uses(entry.detail.as_deref().unwrap_or(""));
                        let obj = uses
                            .iter()
                            .find(|u| u.kind == *k && u.name == *name)
                            .and_then(|u| u.obj);
                        Some(ResourceRow {
                            name: entry.label.clone(),
                            not_embedded: detail.contains("not embedded"),
                            detail: detail.replace(", not embedded", ""),
                            obj,
                            uses: count,
                            finding: entry.badges.iter().any(|b| b.code == "validation"),
                            id: entry.id,
                        })
                    })
                    .collect();
                Some(ResourceGroup { kind: kind?, rows })
            })
            .collect()
    }

    /// The object a resource resolves to, asked from the core when the page does not use it.
    pub fn resource_target(&self, id: &NodeId) -> Option<ObjRef> {
        let props = self.document.as_ref()?.doc.node(id)?;
        match props
            .groups
            .iter()
            .find(|g| g.name == "Resource")?
            .rows
            .iter()
            .find(|r| r.key == "Object")?
            .link
        {
            Some(NodeId::Object(r)) => Some(r),
            _ => None,
        }
    }
}

/// The text after the number in a row of "All objects": `stream`, `/Page`, `/Font /Type1`.
fn row_detail(summary: &str) -> String {
    if let Some(rest) = summary.strip_prefix("stream") {
        let rest = rest.trim();
        return if rest.starts_with("<<") {
            "stream".to_owned()
        } else {
            format!("stream {}", rest.replace("/Type ", "")).trim().to_owned()
        };
    }
    if summary.starts_with("<<") {
        return "dictionary".to_owned();
    }
    summary.replace("/Type ", "")
}

/// Splits `... · 5 uses` into the text before it and the count.
fn split_uses(detail: &str) -> (String, Option<usize>) {
    if let Some((head, tail)) = detail.rsplit_once(" · ")
        && let Some(count) = tail
            .strip_suffix(" uses")
            .or_else(|| tail.strip_suffix(" use"))
            .and_then(|n| n.parse().ok())
    {
        return (head.to_owned(), Some(count));
    }
    (detail.to_owned(), None)
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn row_details_drop_the_type_key_and_hide_key_counts() {
        assert_eq!(row_detail("/Type /Page"), "/Page");
        assert_eq!(row_detail("/Type /Font /Type1"), "/Font /Type1");
        assert_eq!(row_detail("stream << 1 keys >>"), "stream");
        assert_eq!(row_detail("stream /Type /XObject /Form"), "stream /XObject /Form");
        assert_eq!(row_detail("<< 3 keys >>"), "dictionary");
        assert_eq!(row_detail("[4 items]"), "[4 items]");
    }

    #[test]
    fn use_counts_are_split_from_the_detail() {
        assert_eq!(
            split_uses("Type1 Helvetica, not embedded · 5 uses"),
            ("Type1 Helvetica, not embedded".to_owned(), Some(5))
        );
        assert_eq!(split_uses("ca · 1 use"), ("ca".to_owned(), Some(1)));
        assert_eq!(split_uses("ca"), ("ca".to_owned(), None));
        assert_eq!(
            split_uses("Form, BBox [0 0 1 1]"),
            ("Form, BBox [0 0 1 1]".to_owned(), None)
        );
    }
}
