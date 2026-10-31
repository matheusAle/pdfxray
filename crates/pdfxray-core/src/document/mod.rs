//! [`Document`]: an opened PDF and the query API the UI and the CLI read it through.
//!
//! This module opens the file (every stage of [`Document::open_with_progress`]) and holds the
//! document's facts. The navigation queries (trees, inspector properties, hit testing,
//! breadcrumbs, bounding boxes) are in [`queries`]; attaching a veraPDF report is in [`report`];
//! the serialisable [`DiagnosticInfo`] and the structural checks are in [`diagnostics`]. The
//! content, tag and resource queries live next to their models in their own modules.

mod diagnostics;
mod queries;
mod report;

use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, OnceLock};

use hayro_syntax::page::Page;
use hayro_syntax::{LoadPdfError, Pdf};
use pdfxray_raw::RawDocument;
use serde::Serialize;

pub use self::diagnostics::DiagnosticInfo;
use self::diagnostics::structural_diagnostics;
use crate::capture::CapturedPage;
use crate::content::ContentIndex;
use crate::numeric::usize_from_u32;
use crate::objects::ObjectIndex;
use crate::open::{OpenStage, Progress};
use crate::page::PageInfo;
use crate::pagetree;
use crate::tags::TagTree;
use crate::validation::ValidationRun;
use crate::{CoreError, PageIx, TreeKind, TreeNode};

/// An opened PDF. Loading is eager: the whole file is held in memory and every page is
/// analysed at open time.
pub struct Document {
    pub(crate) name: String,
    pub(crate) bytes: Arc<Vec<u8>>,
    pub(crate) raw: RawDocument,
    pub(crate) objects: ObjectIndex,
    pub(crate) engine: Result<Pdf, EngineError>,
    pub(crate) pages: Vec<PageInfo>,
    pub(crate) diagnostics: Vec<DiagnosticInfo>,
    pub(crate) validation: Option<ValidationRun>,
    pub(crate) content: ContentIndex,
    pub(crate) tags: TagTree,
    trees: TreeCache,
}

/// Files with at least this many indirect objects get their large trees built at open.
const WARM_TREES_FROM_OBJECTS: usize = 1_000;

/// The navigation trees that cost real time to build and never change once the file is open
/// (they do not depend on the validation run). Built once, then cloned per request.
#[derive(Default)]
struct TreeCache {
    objects: OnceLock<Vec<TreeNode>>,
    tags: OnceLock<Vec<TreeNode>>,
    resources: OnceLock<Vec<TreeNode>>,
}

/// Why the rendering engine could not load the file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum EngineError {
    /// The file is encrypted; pdfxray v1 does not interpret encrypted files.
    Encrypted,
    /// The engine could not make sense of the file.
    Invalid,
}

impl std::fmt::Debug for Document {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Document")
            .field("name", &self.name)
            .field("byte_len", &self.bytes.len())
            .field("pages", &self.pages.len())
            .finish_non_exhaustive()
    }
}

impl Document {
    /// Reads and opens the file at `path`.
    ///
    /// # Errors
    ///
    /// [`CoreError::Io`] when the file cannot be read, [`CoreError::NotPdf`] when it has no
    /// `%PDF-` header. Every other defect is reported as a diagnostic of the opened document.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, CoreError> {
        let (name, bytes) = read_file(path.as_ref())?;
        Self::from_bytes(name, bytes)
    }

    /// Like [`Document::open`], reporting progress and honouring a cancel flag.
    ///
    /// `progress` is called as `(stage, done, total)` (see [`OpenStage`] for when and with what)
    /// always on the calling thread, also while pages are captured on worker threads. Raising
    /// `cancel` from any thread makes this return [`CoreError::Cancelled`] at the next stage
    /// boundary or page.
    ///
    /// # Errors
    ///
    /// As [`Document::open`], plus [`CoreError::Cancelled`] when `cancel` was raised.
    pub fn open_with_progress(
        path: impl AsRef<Path>,
        progress: &mut dyn FnMut(OpenStage, usize, usize),
        cancel: &AtomicBool,
    ) -> Result<Self, CoreError> {
        let mut progress = Progress::new(progress, cancel);
        progress.check()?;
        progress.report(OpenStage::ReadStructure, 0, 0);
        let (name, bytes) = read_file(path.as_ref())?;
        Self::build(name, bytes, &mut progress)
    }

    /// Opens a document from bytes already in memory; `name` labels it in reports.
    ///
    /// Encrypted files are parsed structurally (dictionaries, page boxes, xref) but never
    /// decrypted, rendered or interpreted (v1 decision).
    ///
    /// # Errors
    ///
    /// [`CoreError::NotPdf`] when `bytes` have no `%PDF-` header.
    pub fn from_bytes(name: impl Into<String>, bytes: Vec<u8>) -> Result<Self, CoreError> {
        Self::from_bytes_with_progress(name, bytes, &mut |_, _, _| {}, &AtomicBool::new(false))
    }

    /// Like [`Document::from_bytes`], reporting progress and honouring a cancel flag. See
    /// [`Document::open_with_progress`].
    ///
    /// # Errors
    ///
    /// As [`Document::from_bytes`], plus [`CoreError::Cancelled`] when `cancel` was raised.
    pub fn from_bytes_with_progress(
        name: impl Into<String>,
        bytes: Vec<u8>,
        progress: &mut dyn FnMut(OpenStage, usize, usize),
        cancel: &AtomicBool,
    ) -> Result<Self, CoreError> {
        let mut progress = Progress::new(progress, cancel);
        progress.check()?;
        progress.report(OpenStage::ReadStructure, 0, 0);
        Self::build(name.into(), bytes, &mut progress)
    }

    /// The stages after the file is in memory; `ReadStructure` has been announced.
    fn build(name: String, bytes: Vec<u8>, progress: &mut Progress<'_>) -> Result<Self, CoreError> {
        let bytes = Arc::new(bytes);
        let raw = RawDocument::parse(bytes.clone())?;
        let revisions = raw.revisions().len();
        progress.report(OpenStage::ReadStructure, revisions, revisions);
        progress.check()?;

        progress.report(OpenStage::IndexObjects, 0, 0);
        let objects = ObjectIndex::build(&raw);
        progress.report(OpenStage::IndexObjects, objects.len(), objects.len());
        progress.check()?;

        progress.report(OpenStage::ReadPageTree, 0, 0);
        let engine = if raw.encryption().is_some() {
            Err(EngineError::Encrypted)
        } else {
            Pdf::new(bytes.clone()).map_err(|e| match e {
                LoadPdfError::Decryption(_) => EngineError::Encrypted,
                LoadPdfError::Invalid => EngineError::Invalid,
            })
        };
        let (raw_pages, tree_issues) = pagetree::walk(&raw);
        let pages: Vec<PageInfo> = match &engine {
            Ok(pdf) => pdf
                .pages()
                .iter()
                .zip(0..)
                .map(|(page, i)| PageInfo::new(i, raw_pages.get(usize_from_u32(i)), Some(page)))
                .collect(),
            Err(_) => raw_pages
                .iter()
                .zip(0..)
                .map(|(p, i)| PageInfo::new(i, Some(p), None))
                .collect(),
        };
        let mut diagnostics: Vec<DiagnosticInfo> = raw
            .diagnostics()
            .iter()
            .map(|d| {
                DiagnosticInfo::from(d)
                    .with_object(d.kind.object().or_else(|| objects.containing(d.span.start)))
            })
            .collect();
        diagnostics.extend(structural_diagnostics(
            &objects,
            &pages,
            raw_pages.len(),
            &tree_issues,
            engine.is_ok(),
        ));
        progress.report(OpenStage::ReadPageTree, pages.len(), pages.len());
        progress.check()?;

        let content = ContentIndex::build(engine.as_ref().ok(), pages.len(), progress)?;

        progress.report(OpenStage::BuildStructureTree, 0, 0);
        let mut doc = Self {
            name,
            bytes,
            raw,
            objects,
            engine,
            pages,
            diagnostics,
            validation: None,
            content,
            tags: TagTree::default(),
            trees: TreeCache::default(),
        };
        let (tags, tag_diagnostics) = TagTree::build(&doc);
        doc.tags = tags;
        doc.diagnostics.extend(tag_diagnostics);
        doc.warm_trees();
        let elements = doc.tags.element_count();
        progress.report(OpenStage::BuildStructureTree, elements, elements);
        Ok(doc)
    }

    /// Builds the cached navigation trees now, side by side, so the first request for one is as
    /// fast as the rest. Loading stays eager (plan decision 13); a lazy loader would drop this call
    /// and let [`Document::tree`] fill the cache on first use.
    fn warm_trees(&self) {
        // Small files build their trees in microseconds; waking the pool would cost more.
        if self.objects.len() < WARM_TREES_FROM_OBJECTS {
            return;
        }
        crate::parallel::install(|| {
            rayon::join(
                || self.cached_tree(TreeKind::Tags),
                || {
                    rayon::join(
                        || self.cached_tree(TreeKind::Objects),
                        || self.cached_tree(TreeKind::Resources),
                    )
                },
            )
        });
    }

    /// The cached tree of `kind`, built first when nobody asked before.
    fn cached_tree(&self, kind: TreeKind) -> &[TreeNode] {
        let (slot, build): (&OnceLock<Vec<TreeNode>>, &dyn Fn() -> Vec<TreeNode>) = match kind {
            TreeKind::Tags => (&self.trees.tags, &|| self.tags_tree()),
            TreeKind::Resources => (&self.trees.resources, &|| self.resources_tree()),
            _ => (&self.trees.objects, &|| self.objects.tree(&self.raw)),
        };
        slot.get_or_init(build)
    }

    /// The file's bytes, exactly as read.
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Page `page` (zero-based) as the rendering engine loaded it, for `pdfxray-render` to
    /// rasterise. This is the only way to the engine from outside this crate, and it reaches
    /// single pages, not the engine's whole view of the file. `None` when the engine could not
    /// load the file ([`Document::engine_error`] says why) or `page` is past the last page.
    ///
    /// The UI and the CLI read documents through the query API, never through this. The
    /// workspace `clippy.toml` enforces it: `disallowed_methods` rejects the call everywhere but
    /// at the one place in `pdfxray-render` that allows it.
    pub fn engine_page(&self, page: PageIx) -> Option<&Page<'_>> {
        self.engine.as_ref().ok()?.pages().get(usize_from_u32(page))
    }

    /// Why the engine could not load the file, if it could not.
    pub fn engine_error(&self) -> Option<EngineError> {
        self.engine.as_ref().err().copied()
    }

    /// Number of pages the engine found.
    pub fn page_count(&self) -> usize {
        self.pages.len()
    }

    /// Facts about one page.
    pub fn page(&self, page: PageIx) -> Option<&PageInfo> {
        self.pages.get(usize_from_u32(page))
    }

    /// Summary facts about the file.
    pub fn info(&self) -> DocInfo {
        DocInfo {
            name: self.name.clone(),
            byte_len: self.bytes.len(),
            header_version: self.raw.header().version.to_string(),
            header_offset: self.raw.header().span.start,
            object_count: self.objects.len(),
            revision_count: self.raw.revisions().len(),
            encrypted: self.raw.encryption().is_some(),
            page_count: self.pages.len(),
            engine_error: self.engine_error(),
            diagnostics: self.diagnostics.clone(),
        }
    }

    /// Everything captured from a page's content, for renderers and advanced views.
    pub fn captured(&self, page: PageIx) -> Option<&CapturedPage> {
        self.content.page(page)
    }
}

/// Reads `path` and derives the label reports carry for it.
fn read_file(path: &Path) -> Result<(String, Vec<u8>), CoreError> {
    let bytes = std::fs::read(path).map_err(|source| CoreError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let name = path.file_name().map_or_else(
        || path.display().to_string(),
        |n| n.to_string_lossy().into_owned(),
    );
    Ok((name, bytes))
}

/// Summary returned by [`Document::info`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DocInfo {
    /// File name or caller-supplied label.
    pub name: String,
    /// File size in bytes.
    pub byte_len: usize,
    /// Version from the `%PDF-x.y` header.
    pub header_version: String,
    /// Byte offset of the header (0 for a well-formed file).
    pub header_offset: usize,
    /// Number of pages.
    pub page_count: usize,
    /// Number of indirect objects in the newest revision.
    pub object_count: usize,
    /// Number of revisions (1 + incremental updates).
    pub revision_count: usize,
    /// Whether the trailer has `/Encrypt`.
    pub encrypted: bool,
    /// Why the engine could not load the file, if it could not.
    pub engine_error: Option<EngineError>,
    /// Deviations found so far.
    pub diagnostics: Vec<DiagnosticInfo>,
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;
    use crate::test_support::fixture;

    #[test]
    fn info_reports_raw_facts_for_clean_fixture() {
        let doc = fixture("minimal-one-page");
        let info = doc.info();

        assert_eq!(
            info,
            DocInfo {
                name: "minimal-one-page.pdf".into(),
                byte_len: doc.bytes().len(),
                header_version: "1.4".into(),
                header_offset: 0,
                page_count: 1,
                object_count: 5,
                revision_count: 1,
                encrypted: false,
                engine_error: None,
                diagnostics: vec![],
            }
        );
    }

    #[test]
    fn header_only_file_reports_structure_problems_not_repairs() {
        let doc = Document::from_bytes("b.pdf", b"xx%PDF-2.0\n".to_vec()).unwrap();
        let codes: Vec<&str> = doc.info().diagnostics.iter().map(|d| d.code).collect();

        assert_eq!(codes.first(), Some(&"header-not-at-start"));
        assert!(codes.contains(&"startxref-missing"), "{codes:?}");
        assert_eq!(doc.info().engine_error, Some(EngineError::Invalid));
    }

    #[test]
    fn document_can_be_shared_between_threads() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<Document>();

        let doc = std::sync::Arc::new(fixture("struct-rich"));
        let tags: Vec<usize> = (0..4)
            .map(|_| {
                let doc = std::sync::Arc::clone(&doc);
                std::thread::spawn(move || doc.tree(TreeKind::Tags).len())
            })
            .collect::<Vec<_>>()
            .into_iter()
            .map(|t| t.join().unwrap_or_default())
            .collect();

        assert_eq!(tags, vec![1; 4], "every thread sees the same structure tree root");
    }

    #[test]
    fn non_pdf_is_rejected() {
        let err = Document::from_bytes("c.txt", b"hello".to_vec()).unwrap_err();

        assert!(matches!(err, CoreError::NotPdf(_)), "got {err:?}");
    }

    #[test]
    fn missing_file_is_io_error_with_path() {
        let err = Document::open("/definitely/not/here.pdf").unwrap_err();

        assert_eq!(err.to_string(), "cannot read /definitely/not/here.pdf");
    }

    #[test]
    #[expect(
        clippy::disallowed_methods,
        reason = "the engine's one door is what is under test"
    )]
    fn engine_pages_are_the_documents_pages_and_none_without_an_engine() {
        let doc = fixture("minimal-one-page");
        let encrypted = fixture("encrypted-stub");

        assert_eq!(doc.page_count(), 1);
        assert!(doc.engine_page(0).is_some());
        assert!(doc.engine_page(1).is_none(), "past the last page");
        // Smoke: the encrypted file has a page in the raw tree but none in the engine.
        assert_eq!(encrypted.page_count(), 1);
        assert!(encrypted.engine_page(0).is_none());
    }

    #[test]
    fn encrypted_fixture_is_refused_by_engine() {
        let doc = fixture("encrypted-stub");

        assert_eq!(doc.engine_error(), Some(EngineError::Encrypted));
        assert!(doc.engine.is_err());
        // Structure is still shown: page boxes come from the raw page tree.
        assert_eq!(doc.page_count(), 1);
        assert!(doc.info().encrypted);
    }
}
