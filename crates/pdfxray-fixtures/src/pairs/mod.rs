//! Fixture *pairs* for the PDF diff: a base document A and a variant B produced by a declared
//! mutation.
//!
//! Every pair starts from the same three-page tagged report (see [`report`]) and applies one
//! mutation. [`MutationFacts`] declares what changed in this crate's own terms; they never mention
//! diff statuses, so a diff test that derives its expectation from them is not circular.
//!
//! The committed files live in `fixtures/generated/pairs/` as `<name>.a.pdf`, `<name>.b.pdf` and
//! `<name>.mutation.json`; [`generate`] writes them and [`pair`] names them.
//!
//! ```
//! let (a, b) = pdfxray_fixtures::pairs::pair("drop-alt");
//! assert!(a.ends_with("pairs/drop-alt.a.pdf") && b.ends_with("pairs/drop-alt.b.pdf"));
//! let facts = pdfxray_fixtures::pairs::pair_mutation("drop-alt");
//! assert_eq!(facts.name, "drop-alt");
//! ```

mod facts;
mod report;

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

pub use facts::{ArtifactText, ContentRewrite, ElementRef, Mutation, MutationFacts, Renumbering, SideFacts};
use report::{Edit, Footer, Numbers, Side};

use crate::GenerateError;
use crate::writer::WriterError;

/// Names of every pair, in catalog order.
pub const PAIRS: [&str; 11] = [
    "renumber-objects",
    "rewrite-content-same-ink",
    "global-shift-2pt",
    "reorder-tags",
    "move-paragraph-3pt",
    "drop-alt",
    "change-heading-level",
    "retag-figure-as-formula",
    "insert-page",
    "artifact-page-number",
    "identical-rows",
];

/// A built pair: both files and the declared mutation.
#[derive(Debug, Clone, PartialEq)]
pub struct BuiltPair {
    /// The base document.
    pub a: Vec<u8>,
    /// The mutated document.
    pub b: Vec<u8>,
    /// What the mutation did.
    pub mutation: MutationFacts,
}

impl BuiltPair {
    /// The facts as pretty JSON with a trailing newline.
    ///
    /// # Errors
    /// Fails only if serialization fails, which cannot happen for [`MutationFacts`] in practice.
    pub fn mutation_json(&self) -> Result<String, serde_json::Error> {
        let mut json = serde_json::to_string_pretty(&self.mutation)?;
        json.push('\n');
        Ok(json)
    }
}

/// Builds pair `name`, or `None` for a name not in [`PAIRS`]. Building is pure and deterministic.
///
/// # Errors
/// The writer's error if the pair definition is internally inconsistent.
pub fn build(name: &str) -> Option<Result<BuiltPair, WriterError>> {
    let (description, builder) = definition(name)?;
    Some(builder().map(|(a, b, mutations)| BuiltPair {
        mutation: MutationFacts {
            name: name.to_owned(),
            description: description.to_owned(),
            a: side_facts(&a),
            b: side_facts(&b),
            mutations,
        },
        a: a.pdf,
        b: b.pdf,
    }))
}

/// Directory of the committed pairs, `<fixtures_dir>/pairs`.
#[must_use]
pub fn pairs_dir() -> PathBuf {
    crate::fixtures_dir().join("pairs")
}

/// Paths of the committed `(a.pdf, b.pdf)` of pair `name`.
#[must_use]
pub fn pair(name: &str) -> (PathBuf, PathBuf) {
    let dir = pairs_dir();
    (
        dir.join(format!("{name}.a.pdf")),
        dir.join(format!("{name}.b.pdf")),
    )
}

/// Path of the committed `<name>.mutation.json`.
#[must_use]
pub fn mutation_path(name: &str) -> PathBuf {
    pairs_dir().join(format!("{name}.mutation.json"))
}

static FACTS: LazyLock<BTreeMap<&'static str, MutationFacts>> = LazyLock::new(|| {
    PAIRS
        .iter()
        .map(|&name| {
            #[expect(clippy::expect_used, reason = "every name in PAIRS has a definition")]
            let built = build(name).expect("defined pair");
            #[expect(
                clippy::panic,
                reason = "a pair that cannot be built is a generator bug every test would hit"
            )]
            let built = built.unwrap_or_else(|e| panic!("pair `{name}` failed to build: {e}"));
            (name, built.mutation)
        })
        .collect()
});

/// The declared mutation of pair `name`.
///
/// # Panics
/// When `name` is not in [`PAIRS`]: callers name pairs from that constant.
#[must_use]
pub fn pair_mutation(name: &str) -> &'static MutationFacts {
    #[expect(clippy::panic, reason = "documented: names come from PAIRS")]
    FACTS
        .get(name)
        .unwrap_or_else(|| panic!("no pair named `{name}`; see PAIRS"))
}

/// Writes `<name>.a.pdf`, `<name>.b.pdf` and `<name>.mutation.json` for every pair into `dir`.
///
/// Returns the paths written, in [`PAIRS`] order.
///
/// # Errors
/// Fails on the first pair that cannot be built or serialized, or on any I/O error.
pub fn generate(dir: &Path) -> Result<Vec<PathBuf>, GenerateError> {
    fs::create_dir_all(dir).map_err(|source| GenerateError::Io {
        path: dir.to_path_buf(),
        source,
    })?;
    let mut written = Vec::new();
    for name in PAIRS {
        let built = build(name)
            .ok_or(GenerateError::Build {
                name,
                source: WriterError::UnknownLabel(name.to_owned()),
            })?
            .map_err(|source| GenerateError::Build { name, source })?;
        let json = built
            .mutation_json()
            .map_err(|source| GenerateError::Json { name, source })?;
        let files = [
            (format!("{name}.a.pdf"), built.a.as_slice()),
            (format!("{name}.b.pdf"), built.b.as_slice()),
            (format!("{name}.mutation.json"), json.as_bytes()),
        ];
        for (file, bytes) in files {
            let path = dir.join(file);
            fs::write(&path, bytes).map_err(|source| GenerateError::Io {
                path: path.clone(),
                source,
            })?;
            written.push(path);
        }
    }
    Ok(written)
}

type Built = (Side, Side, Vec<Mutation>);
/// Builds one pair: both sides and the declared mutations.
type Builder = fn() -> Result<Built, WriterError>;

fn definition(name: &str) -> Option<(&'static str, Builder)> {
    let found: (&'static str, Builder) = match name {
        "renumber-objects" => (
            "Every indirect object renumbered (reversed, sparse) and written in a different byte order; content and tagging byte-identical otherwise.",
            renumber_objects,
        ),
        "rewrite-content-same-ink" => (
            "Every page's content re-encoded (shared BT blocks with relative Td under a cm, split TJ strings, figures moved into Form XObjects drawn with paths, MCIDs reassigned) and split across three streams at token boundaries inside operations; the ink is identical.",
            rewrite_content_same_ink,
        ),
        "global-shift-2pt" => (
            "Every coordinate of the page content, artifacts and Layout BBox attributes included, translated by (2, -2) pt.",
            global_shift_2pt,
        ),
        "reorder-tags" => (
            "The two paragraphs on page 2 swap places in the Document's /K; their ink does not move.",
            reorder_tags,
        ),
        "move-paragraph-3pt" => (
            "The first paragraph on page 2 is drawn 3 pt lower; nothing else moves.",
            move_paragraph_3pt,
        ),
        "drop-alt" => ("The bar chart Figure loses its /Alt.", drop_alt),
        "change-heading-level" => (
            "The Method heading's /S changes from H2 to H3; ink and content tag unchanged.",
            change_heading_level,
        ),
        "retag-figure-as-formula" => (
            "The flow diagram's /S changes from Figure to Formula; ink, /Alt and content tag unchanged.",
            retag_figure_as_formula,
        ),
        "insert-page" => (
            "A note page is inserted as page 2; later pages shift by one and their page-number footers follow.",
            insert_page,
        ),
        "artifact-page-number" => (
            "The page-number footer artifact reads `- n -` instead of `Page n` on every page.",
            artifact_page_number,
        ),
        "identical-rows" => (
            "The third of four identical table rows is removed without reflow; no row has a unique text, role or neighbour.",
            identical_rows,
        ),
        _ => return None,
    };
    Some(found)
}

fn side_facts(side: &Side) -> SideFacts {
    SideFacts {
        page_count: side.pages.len(),
        page_objects: side.pages.iter().filter_map(|p| side.number(&p.label)).collect(),
        element_count: side.elements.len(),
    }
}

/// Describes element `label`, from A when it exists there.
fn element(label: &str, a: &Side, b: &Side) -> ElementRef {
    let info = a.elements.get(label).or_else(|| b.elements.get(label));
    ElementRef {
        label: label.to_owned(),
        a: a.elements.contains_key(label).then(|| a.number(label)).flatten(),
        b: b.elements.contains_key(label).then(|| b.number(label)).flatten(),
        role: info.map(|i| i.role.clone()).unwrap_or_default(),
        text: info.and_then(|i| i.text.clone()),
        page_a: a.elements.get(label).and_then(|i| i.page),
        page_b: b.elements.get(label).and_then(|i| i.page),
    }
}

/// The base document, numbered sequentially.
fn base() -> Result<Side, WriterError> {
    report::build(&Edit::default())
}

fn renumber_objects() -> Result<Built, WriterError> {
    let a = base()?;
    let b = report::build(&Edit {
        numbers: Numbers::Scrambled,
        ..Edit::default()
    })?;
    let objects = a
        .labels
        .iter()
        .filter_map(|label| {
            Some(Renumbering {
                label: label.clone(),
                a: a.number(label)?,
                b: b.number(label)?,
            })
        })
        .collect();
    let mutations = vec![Mutation::Renumbered {
        objects,
        reordered_in_file: true,
    }];
    Ok((a, b, mutations))
}

/// B built from `edit` with A's object numbers kept for every object both sides have.
fn variant(a: &Side, edit: Edit<'_>) -> Result<Side, WriterError> {
    report::build(&Edit {
        numbers: Numbers::Stable(&a.numbers),
        ..edit
    })
}

fn rewrite_content_same_ink() -> Result<Built, WriterError> {
    let a = base()?;
    let b = variant(
        &a,
        Edit {
            rewrite_content: true,
            ..Edit::default()
        },
    )?;
    let numbers = |side: &Side, labels: &[String]| -> Vec<u32> {
        labels.iter().filter_map(|l| side.number(l)).collect()
    };
    let pages = a
        .pages
        .iter()
        .zip(&b.pages)
        .enumerate()
        .map(|(page, (pa, pb))| ContentRewrite {
            page,
            a_streams: numbers(&a, &pa.streams),
            b_streams: numbers(&b, &pb.streams),
            b_forms: numbers(&b, &pb.forms),
            mcids: pa
                .mcids
                .iter()
                .map(|(label, ma)| {
                    let mb = pb.mcids.iter().find(|(l, _)| l == label).map_or(-1, |(_, m)| *m);
                    (label.clone(), *ma, mb)
                })
                .collect(),
        })
        .collect();
    Ok((a, b, vec![Mutation::ContentRewritten { pages }]))
}

fn global_shift_2pt() -> Result<Built, WriterError> {
    let (dx, dy) = (2, -2);
    let a = base()?;
    let b = variant(
        &a,
        Edit {
            offset: (dx, dy),
            ..Edit::default()
        },
    )?;
    let mutation = Mutation::Translated {
        dx: f64::from(dx),
        dy: f64::from(dy),
    };
    Ok((a, b, vec![mutation]))
}

fn reorder_tags() -> Result<Built, WriterError> {
    let (first, second) = ("el.compare", "el.audit");
    let a = base()?;
    let b = variant(
        &a,
        Edit {
            swap: Some((first, second)),
            ..Edit::default()
        },
    )?;
    let mutation = Mutation::SiblingsSwapped {
        parent: element("el.document", &a, &b),
        first: element(first, &a, &b),
        second: element(second, &a, &b),
    };
    Ok((a, b, vec![mutation]))
}

fn move_paragraph_3pt() -> Result<Built, WriterError> {
    let (label, dy) = ("el.compare", -3);
    let a = base()?;
    let b = variant(
        &a,
        Edit {
            moved: Some((label, (0, dy))),
            ..Edit::default()
        },
    )?;
    let mutation = Mutation::ElementMoved {
        element: element(label, &a, &b),
        dx: 0.0,
        dy: f64::from(dy),
    };
    Ok((a, b, vec![mutation]))
}

fn drop_alt() -> Result<Built, WriterError> {
    let label = "el.chart";
    let a = base()?;
    let b = variant(
        &a,
        Edit {
            drop_alt: Some(label),
            ..Edit::default()
        },
    )?;
    let alt = a
        .elements
        .get(label)
        .and_then(|e| e.alt.clone())
        .unwrap_or_default();
    let mutation = Mutation::AltRemoved {
        element: element(label, &a, &b),
        alt,
    };
    Ok((a, b, vec![mutation]))
}

/// B with `label`'s `/S` replaced by `to`.
fn retag(label: &str, to: &'static str) -> Result<Built, WriterError> {
    let a = base()?;
    let b = variant(
        &a,
        Edit {
            retag: Some((label, to)),
            ..Edit::default()
        },
    )?;
    let from = a.elements.get(label).map(|e| e.role.clone()).unwrap_or_default();
    let mutation = Mutation::RoleChanged {
        element: element(label, &a, &b),
        from,
        to: to.to_owned(),
    };
    Ok((a, b, vec![mutation]))
}

fn change_heading_level() -> Result<Built, WriterError> {
    retag("el.method", "H3")
}

fn retag_figure_as_formula() -> Result<Built, WriterError> {
    retag("el.flow", "Formula")
}

/// Footer changes between the pages of A and the pages of B they correspond to.
fn footer_changes(a: &Side, b: &Side, b_index_of: impl Fn(usize) -> usize) -> Vec<ArtifactText> {
    a.pages
        .iter()
        .enumerate()
        .filter_map(|(page_a, pa)| {
            let page_b = b_index_of(page_a);
            let pb = b.pages.get(page_b)?;
            (pa.footer != pb.footer).then(|| ArtifactText {
                page_a,
                page_b,
                artifact_type: "Pagination".to_owned(),
                subtype: "Footer".to_owned(),
                a_text: pa.footer.clone(),
                b_text: pb.footer.clone(),
            })
        })
        .collect()
}

fn insert_page() -> Result<Built, WriterError> {
    let at = 1;
    let a = base()?;
    let b = variant(
        &a,
        Edit {
            insert_page_at: Some(at),
            ..Edit::default()
        },
    )?;
    let new_page = b.pages.get(at);
    let elements = b
        .labels
        .iter()
        .filter(|l| !a.elements.contains_key(*l) && b.elements.contains_key(*l))
        .map(|l| element(l, &a, &b))
        .collect();
    let page_inserted = Mutation::PageInserted {
        index: at,
        page_object: new_page.and_then(|p| b.number(&p.label)).unwrap_or_default(),
        elements,
    };
    let footers = Mutation::ArtifactTextChanged {
        changes: footer_changes(&a, &b, |i| if i < at { i } else { i + 1 }),
    };
    Ok((a, b, vec![page_inserted, footers]))
}

fn artifact_page_number() -> Result<Built, WriterError> {
    let a = base()?;
    let b = variant(
        &a,
        Edit {
            footer: Footer::Dashes,
            ..Edit::default()
        },
    )?;
    let mutation = Mutation::ArtifactTextChanged {
        changes: footer_changes(&a, &b, |i| i),
    };
    Ok((a, b, vec![mutation]))
}

fn identical_rows() -> Result<Built, WriterError> {
    let row = 2;
    let label = format!("el.row{row}");
    let a = base()?;
    let b = variant(
        &a,
        Edit {
            remove_row: Some(row),
            ..Edit::default()
        },
    )?;
    let info = a.elements.get(&label);
    let parent = info.map(|i| i.parent.clone()).unwrap_or_default();
    let index_in_parent = a
        .elements
        .get(&parent)
        .and_then(|p| p.kids.iter().position(|k| *k == label))
        .unwrap_or_default();
    let descendants = info
        .map(|i| i.kids.iter().map(|k| element(k, &a, &b)).collect())
        .unwrap_or_default();
    let mutation = Mutation::ElementRemoved {
        element: element(&label, &a, &b),
        parent: element(&parent, &a, &b),
        index_in_parent,
        descendants,
    };
    Ok((a, b, vec![mutation]))
}
