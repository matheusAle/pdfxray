//! Machine-readable facts that accompany every generated fixture.
//!
//! [`Expected`] is what a fixture's author *knows* to be true about the file. Other crates load
//! the matching `<name>.expected.json` (or call [`crate::Fixture::build`]) and assert exact values
//! against it. All maps use [`BTreeMap`] and all structs serialize in declaration order, so the
//! JSON is stable.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::writer::{PdfVersion, RevisionRecord, XrefKind};

mod content;
mod geometry;
mod structure;

pub use content::{
    ColorFacts, GlyphPaint, ImageFacts, ImageKind, LineFacts, PaintOpFacts, TextOpFacts, Type3GlyphFacts,
};
pub use geometry::{PageGeometryFacts, ResourceEntryFacts, ResourceScopeFacts, SharedResourceFacts};
pub use structure::{
    AnnotationFacts, AttributeFacts, AttributeSetFacts, AttributeSource, EmbeddedFileFacts, KidFacts,
    NamespaceFacts, NodeFacts, ObjectParentFacts, OutlineFacts, PageKeyFacts, RoleTarget,
};

/// A rectangle as `[llx, lly, urx, ury]`, exactly as written in the PDF.
pub type Rect = [f64; 4];

/// A value together with the page-tree node that defines it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Sourced<T> {
    /// The value.
    pub value: T,
    /// Object number of the page or `/Pages` node that carries the entry.
    pub node: u32,
    /// True when `node` is an ancestor of the page rather than the page itself.
    pub inherited: bool,
}

impl<T> Sourced<T> {
    /// A value written directly on the page object `node`.
    #[must_use]
    pub const fn own(value: T, node: u32) -> Self {
        Self {
            value,
            node,
            inherited: false,
        }
    }

    /// A value the page inherits from the ancestor `node`.
    #[must_use]
    pub const fn inherited(value: T, node: u32) -> Self {
        Self {
            value,
            node,
            inherited: true,
        }
    }
}

/// Everything a test may assert about one fixture.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Expected {
    /// Version in the `%PDF-x.y` header line, for example `"1.7"`.
    pub header_version: String,
    /// Number of revisions (xref sections) in the file.
    pub revision_count: u32,
    /// One entry per revision, oldest first.
    pub revisions: Vec<RevisionFacts>,
    /// Number of leaf pages in the page tree.
    pub page_count: u32,
    /// Facts about each page in document order.
    pub pages: Vec<PageFacts>,
    /// Catalog level facts.
    pub catalog: CatalogFacts,
    /// Logical structure, when the document has a `/StructTreeRoot`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub struct_tree: Option<StructFacts>,
    /// Every artifact marked-content sequence in page content streams, in document order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub artifacts: Vec<ArtifactFacts>,
    /// Form `XObject`s painted (directly or transitively) by the pages.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub forms: Vec<FormFacts>,
    /// Deliberate defects. Empty means the file is expected to be well formed.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub defects: Vec<Defect>,
    /// Text-showing operators in paint order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub text_ops: Vec<TextOpFacts>,
    /// Path-painting and shading operators in paint order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub paint_ops: Vec<PaintOpFacts>,
    /// Painted images in paint order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub images: Vec<ImageFacts>,
    /// Type 3 glyphs in paint order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub type3_glyphs: Vec<Type3GlyphFacts>,
    /// Rich logical structure (attributes, classes, namespaces, content references). Fixtures
    /// whose structure fits [`StructFacts`] use that instead.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub struct_outline: Option<OutlineFacts>,
    /// Annotations per page.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub annotations: Vec<AnnotationFacts>,
    /// Embedded files.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub embedded_files: Vec<EmbeddedFileFacts>,
    /// Derived geometry per page.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub page_geometry: Vec<PageGeometryFacts>,
    /// Effective resources per page and form.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub resources: Vec<ResourceScopeFacts>,
    /// Resource objects used from more than one scope.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub shared_resources: Vec<SharedResourceFacts>,
}

impl Expected {
    /// Facts for a file with the given header version and revisions; everything else empty.
    ///
    /// `catalog` and `pages` still need to be filled in by the caller.
    #[must_use]
    pub fn new(version: PdfVersion, revisions: &[RevisionRecord], catalog: CatalogFacts) -> Self {
        Self {
            header_version: version.to_string(),
            revision_count: u32::try_from(revisions.len()).unwrap_or(u32::MAX),
            revisions: revisions
                .iter()
                .enumerate()
                .map(|(i, r)| RevisionFacts::from_record(i, r))
                .collect(),
            page_count: 0,
            pages: Vec::new(),
            catalog,
            struct_tree: None,
            artifacts: Vec::new(),
            forms: Vec::new(),
            defects: Vec::new(),
            text_ops: Vec::new(),
            paint_ops: Vec::new(),
            images: Vec::new(),
            type3_glyphs: Vec::new(),
            struct_outline: None,
            annotations: Vec::new(),
            embedded_files: Vec::new(),
            page_geometry: Vec::new(),
            resources: Vec::new(),
            shared_resources: Vec::new(),
        }
    }

    /// Appends a page and keeps [`Expected::page_count`] in sync.
    #[must_use]
    pub fn with_page(mut self, page: PageFacts) -> Self {
        self.pages.push(page);
        self.page_count = u32::try_from(self.pages.len()).unwrap_or(u32::MAX);
        self
    }

    /// Sets the logical structure facts.
    #[must_use]
    pub fn with_struct_tree(mut self, tree: StructFacts) -> Self {
        self.struct_tree = Some(tree);
        self
    }

    /// Appends an artifact.
    #[must_use]
    pub fn with_artifact(mut self, artifact: ArtifactFacts) -> Self {
        self.artifacts.push(artifact);
        self
    }

    /// Appends a form `XObject`.
    #[must_use]
    pub fn with_form(mut self, form: FormFacts) -> Self {
        self.forms.push(form);
        self
    }

    /// Appends a deliberate defect.
    #[must_use]
    pub fn with_defect(mut self, defect: Defect) -> Self {
        self.defects.push(defect);
        self
    }

    /// Appends a text operator.
    #[must_use]
    pub fn with_text_op(mut self, op: TextOpFacts) -> Self {
        self.text_ops.push(op);
        self
    }

    /// Appends a painting operator.
    #[must_use]
    pub fn with_paint_op(mut self, op: PaintOpFacts) -> Self {
        self.paint_ops.push(op);
        self
    }

    /// Appends an image.
    #[must_use]
    pub fn with_image(mut self, image: ImageFacts) -> Self {
        self.images.push(image);
        self
    }

    /// Appends a Type 3 glyph.
    #[must_use]
    pub fn with_type3_glyph(mut self, glyph: Type3GlyphFacts) -> Self {
        self.type3_glyphs.push(glyph);
        self
    }

    /// Sets the rich structure outline.
    #[must_use]
    pub fn with_struct_outline(mut self, outline: OutlineFacts) -> Self {
        self.struct_outline = Some(outline);
        self
    }

    /// Appends an annotation.
    #[must_use]
    pub fn with_annotation(mut self, annotation: AnnotationFacts) -> Self {
        self.annotations.push(annotation);
        self
    }

    /// Appends an embedded file.
    #[must_use]
    pub fn with_embedded_file(mut self, file: EmbeddedFileFacts) -> Self {
        self.embedded_files.push(file);
        self
    }

    /// Appends the derived geometry of a page.
    #[must_use]
    pub fn with_page_geometry(mut self, geometry: PageGeometryFacts) -> Self {
        self.page_geometry.push(geometry);
        self
    }

    /// Appends the resource scope of a page or form.
    #[must_use]
    pub fn with_resource_scope(mut self, scope: ResourceScopeFacts) -> Self {
        self.resources.push(scope);
        self
    }

    /// Appends a shared resource object.
    #[must_use]
    pub fn with_shared_resource(mut self, shared: SharedResourceFacts) -> Self {
        self.shared_resources.push(shared);
        self
    }

    /// True when the fixture contains at least one deliberate defect.
    #[must_use]
    pub fn is_defective(&self) -> bool {
        !self.defects.is_empty()
    }
}

/// Facts about one revision (one xref section and the objects it introduces).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RevisionFacts {
    /// One-based revision number, oldest first.
    pub number: u32,
    /// Whether the xref section is a classic table or a stream.
    pub xref_kind: XrefKind,
    /// Byte offset of the xref section, as written after `startxref`.
    pub xref_offset: u64,
    /// `/Prev` value, when the revision updates an earlier one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prev: Option<u64>,
    /// Object numbers this revision defines, ascending. Includes compressed objects and the xref
    /// stream object itself; excludes the free-list head (object 0).
    pub objects: Vec<u32>,
    /// Objects stored in object streams.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub compressed: Vec<CompressedObject>,
    /// Offset of the `/XRefStm` cross-reference stream of a hybrid-reference file. For such a
    /// revision `objects` and `compressed` merge the table and the stream.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub xref_stm: Option<u64>,
}

impl RevisionFacts {
    fn from_record(index: usize, record: &RevisionRecord) -> Self {
        Self {
            number: u32::try_from(index + 1).unwrap_or(u32::MAX),
            xref_kind: record.kind,
            xref_offset: record.xref_offset,
            prev: record.prev,
            objects: record.objects(),
            compressed: record
                .compressed()
                .into_iter()
                .map(|(obj, stream, index)| CompressedObject { obj, stream, index })
                .collect(),
            xref_stm: record.xref_stm,
        }
    }
}

/// An object stored inside an object stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompressedObject {
    /// Object number of the compressed object.
    pub obj: u32,
    /// Object number of the containing `/ObjStm`.
    pub stream: u32,
    /// Zero-based position inside the object stream.
    pub index: u32,
}

/// Document catalog facts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CatalogFacts {
    /// Object number of the catalog (the trailer `/Root`).
    pub object: u32,
    /// Value of `/MarkInfo << /Marked >>`, when `/MarkInfo` is present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub marked: Option<bool>,
    /// Value of `/Lang`, when present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lang: Option<String>,
}

impl CatalogFacts {
    /// A catalog with neither `/MarkInfo` nor `/Lang`.
    #[must_use]
    pub const fn new(object: u32) -> Self {
        Self {
            object,
            marked: None,
            lang: None,
        }
    }

    /// Records `/MarkInfo << /Marked marked >>`.
    #[must_use]
    pub const fn marked(mut self, marked: bool) -> Self {
        self.marked = Some(marked);
        self
    }

    /// Records `/Lang`.
    #[must_use]
    pub fn lang(mut self, lang: impl Into<String>) -> Self {
        self.lang = Some(lang.into());
        self
    }
}

/// Which page box an entry refers to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BoxKind {
    /// `/MediaBox`.
    Media,
    /// `/CropBox`.
    Crop,
    /// `/BleedBox`.
    Bleed,
    /// `/TrimBox`.
    Trim,
    /// `/ArtBox`.
    Art,
}

/// Facts about one page.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PageFacts {
    /// Zero-based position in document order.
    pub index: u32,
    /// Object number of the page dictionary.
    pub object: u32,
    /// Effective `/MediaBox` and the node that defines it.
    pub media_box: Sourced<Rect>,
    /// `/CropBox`, when present on the page or an ancestor.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub crop_box: Option<Sourced<Rect>>,
    /// `/BleedBox`, when present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bleed_box: Option<Sourced<Rect>>,
    /// `/TrimBox`, when present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trim_box: Option<Sourced<Rect>>,
    /// `/ArtBox`, when present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub art_box: Option<Sourced<Rect>>,
    /// `/Rotate`, when present on the page or an ancestor (absent means 0).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rotate: Option<Sourced<i32>>,
    /// `/UserUnit`, when present (absent means 1.0).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_unit: Option<Sourced<f64>>,
    /// MCIDs used by marked content in the page's own content stream, in order of appearance.
    pub mcids: Vec<i32>,
}

impl PageFacts {
    /// A page with only a media box that it defines itself.
    #[must_use]
    pub fn new(index: u32, object: u32, media_box: Sourced<Rect>) -> Self {
        Self {
            index,
            object,
            media_box,
            crop_box: None,
            bleed_box: None,
            trim_box: None,
            art_box: None,
            rotate: None,
            user_unit: None,
            mcids: Vec::new(),
        }
    }

    /// Sets the crop box.
    #[must_use]
    pub fn crop(mut self, rect: Sourced<Rect>) -> Self {
        self.crop_box = Some(rect);
        self
    }

    /// Sets the bleed box.
    #[must_use]
    pub fn bleed(mut self, rect: Sourced<Rect>) -> Self {
        self.bleed_box = Some(rect);
        self
    }

    /// Sets the trim box.
    #[must_use]
    pub fn trim(mut self, rect: Sourced<Rect>) -> Self {
        self.trim_box = Some(rect);
        self
    }

    /// Sets the art box.
    #[must_use]
    pub fn art(mut self, rect: Sourced<Rect>) -> Self {
        self.art_box = Some(rect);
        self
    }

    /// Sets `/Rotate`.
    #[must_use]
    pub fn rotate(mut self, rotate: Sourced<i32>) -> Self {
        self.rotate = Some(rotate);
        self
    }

    /// Sets `/UserUnit`.
    #[must_use]
    pub fn user_unit(mut self, unit: Sourced<f64>) -> Self {
        self.user_unit = Some(unit);
        self
    }

    /// Sets the MCIDs of the page's own content stream.
    #[must_use]
    pub fn mcids(mut self, mcids: &[i32]) -> Self {
        self.mcids = mcids.to_vec();
        self
    }
}

/// Logical structure facts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StructFacts {
    /// Object number of the `/StructTreeRoot`.
    pub root: u32,
    /// The `/RoleMap` entries, custom name to mapped name.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub role_map: BTreeMap<String, String>,
    /// Top-level structure elements (children of the root).
    pub children: Vec<StructElemFacts>,
    /// The `/ParentTree` contents as the file records them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parent_tree: Vec<ParentTreeEntry>,
}

impl StructFacts {
    /// A structure tree rooted at object `root` with no children yet.
    #[must_use]
    pub const fn new(root: u32) -> Self {
        Self {
            root,
            role_map: BTreeMap::new(),
            children: Vec::new(),
            parent_tree: Vec::new(),
        }
    }

    /// Adds `/RoleMap` entries as `(custom, mapped)` pairs.
    #[must_use]
    pub fn role_map(mut self, pairs: &[(&str, &str)]) -> Self {
        self.role_map.extend(
            pairs
                .iter()
                .map(|(from, to)| ((*from).to_owned(), (*to).to_owned())),
        );
        self
    }

    /// Appends a top-level structure element.
    #[must_use]
    pub fn child(mut self, child: StructElemFacts) -> Self {
        self.children.push(child);
        self
    }

    /// Appends a parent tree entry.
    #[must_use]
    pub fn parent_tree(mut self, key: i32, parents: &[Option<u32>]) -> Self {
        self.parent_tree.push(ParentTreeEntry {
            key,
            parents: parents.to_vec(),
        });
        self
    }
}

/// One `/Nums` entry of the parent tree.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParentTreeEntry {
    /// The `/StructParents` (or `/StructParent`) key.
    pub key: i32,
    /// For a page or form stream: the owning struct element per MCID (`null` where none).
    pub parents: Vec<Option<u32>>,
}

/// One structure element in the outline.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StructElemFacts {
    /// Object number of the struct element.
    pub object: u32,
    /// The `/S` name exactly as written.
    pub role: String,
    /// The role after following the `/RoleMap` to a standard type; `null` when the chain is
    /// cyclic or never reaches a standard type.
    pub standard_role: Option<String>,
    /// Zero-based index of the page named by `/Pg`, when any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page: Option<u32>,
    /// Object number of the form `XObject` named by an MCR `/Stm`, when the content is inside one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stream: Option<u32>,
    /// MCIDs this element references through `/K`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mcids: Vec<i32>,
    /// Child structure elements.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<StructElemFacts>,
}

impl StructElemFacts {
    /// An element with a role and its resolved standard role.
    #[must_use]
    pub fn new(object: u32, role: &str, standard_role: Option<&str>) -> Self {
        Self {
            object,
            role: role.to_owned(),
            standard_role: standard_role.map(str::to_owned),
            page: None,
            stream: None,
            mcids: Vec::new(),
            children: Vec::new(),
        }
    }

    /// Sets the page index named by `/Pg`.
    #[must_use]
    pub const fn page(mut self, page: u32) -> Self {
        self.page = Some(page);
        self
    }

    /// Sets the form `XObject` named by `/Stm`.
    #[must_use]
    pub const fn stream(mut self, stream: u32) -> Self {
        self.stream = Some(stream);
        self
    }

    /// Sets the referenced MCIDs.
    #[must_use]
    pub fn mcids(mut self, mcids: &[i32]) -> Self {
        self.mcids = mcids.to_vec();
        self
    }

    /// Appends a child.
    #[must_use]
    pub fn child(mut self, child: Self) -> Self {
        self.children.push(child);
        self
    }
}

/// One `/Artifact` marked-content sequence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactFacts {
    /// Zero-based page index whose content stream holds the artifact.
    pub page: u32,
    /// The `/Type` property (`Pagination`, `Layout`, `Page`, ...), when given.
    pub artifact_type: Option<String>,
    /// The `/Subtype` property (`Header`, `Footer`, ...), when given.
    pub subtype: Option<String>,
}

impl ArtifactFacts {
    /// An artifact on `page` with optional type and subtype.
    #[must_use]
    pub fn new(page: u32, artifact_type: Option<&str>, subtype: Option<&str>) -> Self {
        Self {
            page,
            artifact_type: artifact_type.map(str::to_owned),
            subtype: subtype.map(str::to_owned),
        }
    }
}

/// A Form `XObject` reachable from page content.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FormFacts {
    /// Object number of the form `XObject` stream.
    pub object: u32,
    /// Name under which the painter's resources list the form (without the slash).
    pub name: String,
    /// Object number of the page or form whose content stream paints this form.
    pub painted_by: u32,
    /// The form's `/Matrix`, when present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub matrix: Option<[f64; 6]>,
    /// MCIDs used by marked content inside this form's content stream, in order of appearance.
    pub mcids: Vec<i32>,
}

/// A deliberate defect a fixture carries.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Defect {
    /// An xref entry records an offset other than where `obj 0 obj` really starts.
    XrefOffsetWrong {
        /// Object number.
        obj: u32,
        /// Offset the xref claims.
        recorded: u64,
        /// Offset of the object header in the file.
        actual: u64,
    },
    /// The `/RoleMap` contains a cycle.
    RoleMapCycle {
        /// The roles on the cycle, in order, starting from the lexicographically smallest.
        cycle: Vec<String>,
    },
    /// Content carries an MCID that no structure element references.
    OrphanMcid {
        /// Zero-based page index.
        page: u32,
        /// The unreferenced MCID.
        mcid: i32,
    },
    /// A structure element references an MCID that the page content never defines.
    MissingMcid {
        /// Object number of the structure element.
        elem: u32,
        /// Zero-based page index named by `/Pg`.
        page: u32,
        /// The MCID that is missing from the content.
        mcid: i32,
    },
    /// The parent tree and a struct element's `/K` disagree about who owns an MCID.
    ParentTreeMismatch {
        /// The `/StructParents` key of the page.
        key: i32,
        /// The MCID, which is the index into the parent tree array.
        mcid: i32,
        /// Object the parent tree names (`null` when the slot is empty).
        parent_tree: Option<u32>,
        /// Object whose `/K` actually references the MCID.
        k_parent: u32,
    },
    /// A page box extends outside the page's media box.
    BoxOutsideMediaBox {
        /// Zero-based page index.
        page: u32,
        /// The offending box.
        box_kind: BoxKind,
    },
    /// The file declares encryption, which pdfxray v1 does not support.
    Encrypted {
        /// The security handler `/Filter`.
        handler: String,
        /// The `/V` algorithm code.
        v: i32,
        /// The `/R` revision of the handler.
        r: i32,
    },
    /// The last `startxref` value does not point at the xref section.
    StartxrefWrong {
        /// Offset the file claims after `startxref`.
        recorded: u64,
        /// Offset where the xref section really starts.
        actual: u64,
    },
    /// A run of painting operators outside any marked-content sequence: neither tagged nor
    /// artifact.
    ///
    /// The range `[start, end)` is in operator indices of the page content stream. It spans from
    /// the first to the last painting operator (text showing, path painting, `sh`, `Do`, inline
    /// image) of one run; a run is broken by any marked-content operator (`BMC`, `BDC`, `EMC`),
    /// while non-painting operators (`BT`, `Tf`, `re`, `q`, ...) between painting operators do
    /// not break it.
    UntaggedContent {
        /// Zero-based page index.
        page: u32,
        /// Half-open operator index range `[start, end)`.
        op_index_range: [u32; 2],
    },
    /// The same MCID is used by more than one marked-content sequence of one content stream.
    DuplicateMcid {
        /// Zero-based page index.
        page: u32,
        /// The repeated MCID.
        mcid: i32,
        /// How many sequences use it.
        count: u32,
    },
    /// A marked-content sequence (tagged content or another artifact) starts inside an artifact.
    MarkedContentInsideArtifact {
        /// Zero-based page index.
        page: u32,
        /// Operator index of the nested `BMC` or `BDC`.
        op_index: u32,
        /// The nested sequence's tag.
        tag: String,
    },
}
