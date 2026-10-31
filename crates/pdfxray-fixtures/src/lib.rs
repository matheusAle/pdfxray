//! Deterministic PDF fixtures for the pdfxray test suite.
//!
//! Every [`Fixture`] targets one feature or one deliberate defect and carries [`Expected`]
//! facts, so other crates can assert exact values instead of eyeballing output.
//!
//! * [`writer`] holds [`PdfWriter`], a byte emitter that can produce valid and broken files.
//! * [`expected`] holds the serde types describing what is true about a fixture.
//! * [`catalog`] lists the fixtures; [`catalog::all`] returns them in a stable order.
//! * [`gallery`] builds a file with one object of every kind an object preview draws.
//! * [`generate`] writes `<name>.pdf` and `<name>.expected.json` for every fixture, and
//!   [`fixtures_dir`] tells other crates where the committed copies live.
//!
//! ```
//! let fixture = pdfxray_fixtures::find("minimal-one-page").ok_or("missing fixture")?;
//! let built = fixture.build()?;
//! assert!(built.pdf.starts_with(b"%PDF-1.4\n"));
//! assert_eq!(built.expected.page_count, 1);
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

pub mod catalog;
mod deflate;
pub mod expected;
pub mod gallery;
pub mod writer;

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

pub use expected::{
    AnnotationFacts, ArtifactFacts, AttributeFacts, AttributeSetFacts, AttributeSource, BoxKind,
    CatalogFacts, ColorFacts, CompressedObject, Defect, EmbeddedFileFacts, Expected, FormFacts, GlyphPaint,
    ImageFacts, ImageKind, KidFacts, LineFacts, NamespaceFacts, NodeFacts, ObjectParentFacts, OutlineFacts,
    PageFacts, PageGeometryFacts, PageKeyFacts, PaintOpFacts, ParentTreeEntry, Rect, ResourceEntryFacts,
    ResourceScopeFacts, RevisionFacts, RoleTarget, SharedResourceFacts, Sourced, StructElemFacts,
    StructFacts, TextOpFacts, Type3GlyphFacts,
};
pub use writer::{PdfVersion, PdfWriter, StartxrefMismatch, Trailer, WriterError, XrefKind};

/// A built fixture: the file bytes and the facts that hold for them.
#[derive(Debug, Clone, PartialEq)]
pub struct Built {
    /// The complete PDF file.
    pub pdf: Vec<u8>,
    /// What is true about [`Built::pdf`].
    pub expected: Expected,
}

impl Built {
    /// The facts as pretty JSON with stable key order and a trailing newline.
    ///
    /// # Errors
    /// Fails only if serialization fails, which cannot happen for [`Expected`] in practice.
    pub fn expected_json(&self) -> Result<String, serde_json::Error> {
        let mut json = serde_json::to_string_pretty(&self.expected)?;
        json.push('\n');
        Ok(json)
    }
}

/// One entry of the fixture catalog.
#[derive(Debug, Clone, Copy)]
pub struct Fixture {
    /// File stem used for `<name>.pdf` and `<name>.expected.json`.
    pub name: &'static str,
    /// One sentence on what the fixture exercises.
    pub description: &'static str,
    builder: fn() -> Result<Built, WriterError>,
}

impl Fixture {
    /// Declares a fixture. Used by the catalog.
    #[must_use]
    pub const fn new(
        name: &'static str,
        description: &'static str,
        builder: fn() -> Result<Built, WriterError>,
    ) -> Self {
        Self {
            name,
            description,
            builder,
        }
    }

    /// Builds the file and its expected facts. Building is pure and deterministic.
    ///
    /// # Errors
    /// Returns the writer's error if the fixture definition is internally inconsistent.
    pub fn build(&self) -> Result<Built, WriterError> {
        (self.builder)()
    }

    /// File name of the PDF, `<name>.pdf`.
    #[must_use]
    pub fn pdf_file_name(&self) -> String {
        format!("{}.pdf", self.name)
    }

    /// File name of the facts, `<name>.expected.json`.
    #[must_use]
    pub fn expected_file_name(&self) -> String {
        format!("{}.expected.json", self.name)
    }
}

/// All fixtures in catalog order.
#[must_use]
pub fn all() -> &'static [Fixture] {
    catalog::all()
}

/// Looks a fixture up by name.
#[must_use]
pub fn find(name: &str) -> Option<&'static Fixture> {
    all().iter().find(|f| f.name == name)
}

/// Directory holding the committed generator output (`<workspace>/fixtures/generated`).
#[must_use]
pub fn fixtures_dir() -> PathBuf {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let workspace = manifest.ancestors().nth(2).unwrap_or(manifest);
    workspace.join("fixtures").join("generated")
}

/// Path of the committed PDF of fixture `name`: `<fixtures_dir>/<name>.pdf`.
///
/// The one way tests name a fixture file. This crate cannot open it as a `Document` itself:
/// `pdfxray-core` dev-depends on this crate, so depending back on the core would be a cycle.
/// Each crate's tests wrap this in their own `Document::open` helper instead.
#[must_use]
pub fn pdf_path(name: &str) -> PathBuf {
    fixtures_dir().join(format!("{name}.pdf"))
}

/// Everything that can go wrong while generating fixtures.
#[derive(Debug, thiserror::Error)]
pub enum GenerateError {
    /// A fixture definition is internally inconsistent.
    #[error("building fixture `{name}` failed")]
    Build {
        /// Fixture name.
        name: &'static str,
        /// Underlying writer error.
        #[source]
        source: WriterError,
    },
    /// The expected facts could not be serialized.
    #[error("serializing facts of fixture `{name}` failed")]
    Json {
        /// Fixture name.
        name: &'static str,
        /// Underlying serde error.
        #[source]
        source: serde_json::Error,
    },
    /// A file or directory could not be written.
    #[error("writing `{}` failed", path.display())]
    Io {
        /// The path being written.
        path: PathBuf,
        /// Underlying I/O error.
        #[source]
        source: io::Error,
    },
}

/// Writes `<name>.pdf` and `<name>.expected.json` for every fixture into `dir`.
///
/// The directory is created when missing. Returns the paths written, in catalog order.
///
/// # Errors
/// Fails on the first fixture that cannot be built or serialized, or on any I/O error.
pub fn generate(dir: &Path) -> Result<Vec<PathBuf>, GenerateError> {
    fs::create_dir_all(dir).map_err(|source| GenerateError::Io {
        path: dir.to_path_buf(),
        source,
    })?;

    let mut written = Vec::new();
    for fixture in all() {
        let built = fixture.build().map_err(|source| GenerateError::Build {
            name: fixture.name,
            source,
        })?;
        let json = built.expected_json().map_err(|source| GenerateError::Json {
            name: fixture.name,
            source,
        })?;

        let pdf_path = dir.join(fixture.pdf_file_name());
        let json_path = dir.join(fixture.expected_file_name());
        for (path, bytes) in [(&pdf_path, built.pdf.as_slice()), (&json_path, json.as_bytes())] {
            fs::write(path, bytes).map_err(|source| GenerateError::Io {
                path: path.clone(),
                source,
            })?;
        }
        written.push(pdf_path);
        written.push(json_path);
    }
    Ok(written)
}
