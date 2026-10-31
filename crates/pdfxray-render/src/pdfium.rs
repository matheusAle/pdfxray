//! `PDFium` reference renderer, behind the `pdfium` feature.
//!
//! `PDFium` (Chrome's PDF engine, BSD-3/Apache-2.0) draws the same page as [`crate::render_page`]
//! so a difference between the two ([`crate::diff`]) separates an engine bug from a file bug.
//! The library is never linked and never part of this repository: it is loaded at run time with
//! `dlopen` from a build fetched by `cargo xtask pdfium fetch`.
//!
//! # Where the library is looked for
//!
//! [`PdfiumRenderer::load`] takes the first match of:
//!
//! 1. the `explicit` argument, a library file or a directory holding one. When given it is the
//!    only place searched, so a wrong path is reported instead of silently replaced;
//! 2. the environment variable `PDFXRAY_PDFIUM`, same shape (the value `required` is a switch of
//!    the test-suite, not a path, and is ignored here);
//! 3. `<workspace>/target/pdfium/<version>/lib`, where `cargo xtask pdfium fetch` installs;
//! 4. `<data dir>/pdfium/<version>/lib`, where `<data dir>` is `$PDFXRAY_DATA_DIR` or the
//!    per-user data directory (`~/Library/Application Support/pdfxray` on macOS,
//!    `$XDG_DATA_HOME` or `~/.local/share` plus `pdfxray` elsewhere on Unix, `%APPDATA%\pdfxray`
//!    on Windows).
//!
//! With several `<version>` directories the highest version wins.
//!
//! # One library per process
//!
//! `PDFium` can be initialised once per process. [`PdfiumRenderer::load`] therefore binds the
//! first library it is given and hands out clones of that renderer to every later caller.
//!
//! # Pixel geometry
//!
//! The bitmap has exactly the size hayro produces at the same scale and carries the same
//! `page_to_pixel` transform. `PDFium` scales the page to fill that bitmap, so it can differ from
//! hayro's uniform scale by less than one pixel across the page when the size is truncated.

use std::ffi::{OsStr, OsString};
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

use pdfium_render::prelude::{PdfRenderConfig, Pdfium};
use pdfxray_core::numeric::usize_from_u32;
use pdfxray_core::{Document, PageIx};

use crate::{RenderError, RenderedPage};

/// Environment variable naming the `PDFium` library (or a directory holding it).
pub const LIBRARY_ENV: &str = "PDFXRAY_PDFIUM";

/// Why `PDFium` could not render a page.
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum PdfiumError {
    /// No `PDFium` library exists at any searched location.
    #[error("no PDFium library found (searched {}); run `cargo xtask pdfium fetch`", searched_list(.searched))]
    NotFound {
        /// Locations that were looked at, in search order.
        searched: Vec<PathBuf>,
    },
    /// The library file exists but could not be loaded.
    #[error("cannot load the PDFium library {}: {message}", path.display())]
    Load {
        /// The library file.
        path: PathBuf,
        /// What the dynamic loader said.
        message: String,
    },
    /// A different library was already bound in this process.
    #[error("PDFium is already loaded from {} and cannot be rebound to {}", loaded.display(), requested.display())]
    AlreadyLoaded {
        /// The library bound first.
        loaded: PathBuf,
        /// The library asked for later.
        requested: PathBuf,
    },
    /// `PDFium` rejected the document.
    #[error("PDFium cannot open the document: {0}")]
    Open(String),
    /// `PDFium` failed while drawing the page.
    #[error("PDFium cannot render the page: {0}")]
    Render(String),
    /// The page or scale is invalid for the shared geometry (see [`crate::render_page`]).
    #[error(transparent)]
    Geometry(#[from] RenderError),
}

fn searched_list(paths: &[PathBuf]) -> String {
    if paths.is_empty() {
        return "nowhere".to_owned();
    }
    paths
        .iter()
        .map(|p| p.display().to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

struct Loaded {
    pdfium: Pdfium,
    library: PathBuf,
    version: String,
}

static LOADED: Mutex<Option<Arc<Loaded>>> = Mutex::new(None);

/// A loaded `PDFium` library that renders pages.
///
/// Cheap to clone; all clones share the process-wide library.
#[derive(Clone)]
pub struct PdfiumRenderer {
    loaded: Arc<Loaded>,
}

impl fmt::Debug for PdfiumRenderer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PdfiumRenderer")
            .field("library", &self.loaded.library)
            .field("version", &self.loaded.version)
            .finish()
    }
}

impl PdfiumRenderer {
    /// Finds and loads the `PDFium` library, see the [module documentation](self) for the search.
    ///
    /// The first successful call binds the library for the whole process. Later calls return
    /// the same renderer; when they pass `explicit` it is still checked, so a missing path is
    /// [`PdfiumError::NotFound`] and a different library is [`PdfiumError::AlreadyLoaded`].
    pub fn load(explicit: Option<&Path>) -> Result<Self, PdfiumError> {
        let search = Search::from_process();
        let mut slot = LOADED.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(loaded) = slot.as_ref() {
            if let Some(explicit) = explicit {
                let requested = search.find(Some(explicit))?;
                if !same_file(&requested, &loaded.library) {
                    return Err(PdfiumError::AlreadyLoaded {
                        loaded: loaded.library.clone(),
                        requested,
                    });
                }
            }
            return Ok(Self {
                loaded: Arc::clone(loaded),
            });
        }
        let library = search.find(explicit)?;
        let bindings = Pdfium::bind_to_library(&library).map_err(|e| PdfiumError::Load {
            path: library.clone(),
            message: e.to_string(),
        })?;
        let loaded = Arc::new(Loaded {
            pdfium: Pdfium::new(bindings),
            version: version_of(&library),
            library,
        });
        *slot = Some(Arc::clone(&loaded));
        Ok(Self { loaded })
    }

    /// The `PDFium` version, `MAJOR.MINOR.BUILD.PATCH` (for example `158.0.8125.0`), read from the
    /// `VERSION` file next to the library or from the name of its version directory; `unknown`
    /// when neither tells.
    pub fn version(&self) -> &str {
        &self.loaded.version
    }

    /// The library file that was loaded.
    pub fn library(&self) -> &Path {
        &self.loaded.library
    }

    /// Rasterises `page` (zero-based) of `doc` at `scale` pixels per point on a white background.
    ///
    /// The document is loaded from [`Document::bytes`], so what `PDFium` draws is the file on
    /// disk, not hayro's view of it. Size and `page_to_pixel` are hayro's, see the
    /// [module documentation](self#pixel-geometry). Encrypted documents fail with
    /// [`PdfiumError::Geometry`] because hayro, which supplies the geometry, cannot open them.
    pub fn render_page(&self, doc: &Document, page: PageIx, scale: f32) -> Result<RenderedPage, PdfiumError> {
        let (width, height, page_to_pixel) = crate::page_geometry(doc, page, scale)?;
        let size_error = || RenderError::BadSize { width, height };
        let (px_width, px_height) = (
            i32::try_from(width).map_err(|_| size_error())?,
            i32::try_from(height).map_err(|_| size_error())?,
        );
        let index = i32::try_from(page).map_err(|_| RenderError::NoSuchPage {
            page,
            count: doc.page_count(),
        })?;

        let document = self
            .loaded
            .pdfium
            .load_pdf_from_byte_slice(doc.bytes(), None)
            .map_err(|e| PdfiumError::Open(format!("{e:?}")))?;
        let pdf_page = document
            .pages()
            .get(index)
            .map_err(|e| PdfiumError::Render(format!("page {page} cannot be loaded: {e:?}")))?;
        let bitmap = pdf_page
            .render_with_config(&PdfRenderConfig::new().set_fixed_size(px_width, px_height))
            .map_err(|e| PdfiumError::Render(format!("{e:?}")))?;

        let rgba = bitmap.as_rgba_bytes();
        let expected_len = usize_from_u32(width)
            .saturating_mul(usize_from_u32(height))
            .saturating_mul(4);
        if (bitmap.width(), bitmap.height()) != (px_width, px_height) || rgba.len() != expected_len {
            return Err(PdfiumError::Render(format!(
                "expected a {width} x {height} bitmap of {expected_len} bytes, got {} x {} with {} bytes",
                bitmap.width(),
                bitmap.height(),
                rgba.len()
            )));
        }
        Ok(RenderedPage {
            width,
            height,
            rgba,
            page_to_pixel,
        })
    }
}

/// A place that may hold the library.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Location {
    /// A library file, or a directory with `libpdfium.*` in it or in its `lib` subdirectory.
    Path(PathBuf),
    /// A directory of `<version>/lib` installs, as `cargo xtask pdfium fetch` writes.
    Versions(PathBuf),
}

impl Location {
    fn path(&self) -> &Path {
        match self {
            Self::Path(p) | Self::Versions(p) => p,
        }
    }
}

/// The inputs of the search, injectable for tests.
struct Search {
    env: Option<OsString>,
    workspace: PathBuf,
    data_dir: Option<PathBuf>,
}

impl Search {
    fn from_process() -> Self {
        Self::from_vars(|k| std::env::var_os(k))
    }

    fn from_vars(var: impl Fn(&str) -> Option<OsString>) -> Self {
        Self {
            env: var(LIBRARY_ENV).filter(|v| v.as_os_str() != OsStr::new("required") && !v.is_empty()),
            workspace: Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join(".."),
            data_dir: data_dir_from(&var).map(|d| d.join("pdfium")),
        }
    }

    /// Locations in search order.
    fn locations(&self) -> Vec<Location> {
        let mut out = Vec::new();
        if let Some(env) = &self.env {
            out.push(Location::Path(PathBuf::from(env)));
        }
        out.push(Location::Versions(self.workspace.join("target").join("pdfium")));
        if let Some(dir) = &self.data_dir {
            out.push(Location::Versions(dir.clone()));
        }
        out
    }

    fn find(&self, explicit: Option<&Path>) -> Result<PathBuf, PdfiumError> {
        let locations = match explicit {
            Some(path) => vec![Location::Path(path.to_path_buf())],
            None => self.locations(),
        };
        resolve(&locations)
    }
}

/// The first location that holds a library.
fn resolve(locations: &[Location]) -> Result<PathBuf, PdfiumError> {
    locations
        .iter()
        .find_map(resolve_location)
        .ok_or_else(|| PdfiumError::NotFound {
            searched: locations.iter().map(|l| l.path().to_path_buf()).collect(),
        })
}

fn library_name() -> OsString {
    Pdfium::pdfium_platform_library_name()
}

fn resolve_location(location: &Location) -> Option<PathBuf> {
    match location {
        Location::Path(path) if path.is_file() => Some(path.clone()),
        Location::Path(dir) => [dir.join(library_name()), dir.join("lib").join(library_name())]
            .into_iter()
            .find(|candidate| candidate.is_file()),
        Location::Versions(root) => {
            // A versions folder that cannot be listed holds no usable library: keep looking.
            let mut versions: Vec<(Vec<u64>, PathBuf)> = std::fs::read_dir(root)
                .ok()?
                .filter_map(Result::ok)
                .filter(|entry| entry.path().is_dir())
                .map(|entry| (version_key(&entry.file_name()), entry.path()))
                .collect();
            versions.sort_by(|a, b| b.cmp(a));
            versions
                .into_iter()
                .map(|(_, dir)| dir.join("lib").join(library_name()))
                .find(|candidate| candidate.is_file())
        }
    }
}

/// `158.0.8125.0` as `[158, 0, 8125, 0]`; non-numeric parts sort as 0.
fn version_key(name: &OsStr) -> Vec<u64> {
    name.to_string_lossy()
        .split('.')
        .map(|part| part.parse().unwrap_or(0))
        .collect()
}

fn same_file(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    }
}

/// Version of the install holding `library` (`<root>/lib/<library>`).
fn version_of(library: &Path) -> String {
    let root = library.parent().and_then(Path::parent);
    // The VERSION file is optional: without it the version comes from the folder name.
    root.and_then(|root| std::fs::read_to_string(root.join("VERSION")).ok())
        .and_then(|text| parse_version_file(&text))
        .or_else(|| {
            let name = root?.file_name()?.to_str()?;
            name.starts_with(|c: char| c.is_ascii_digit())
                .then(|| name.to_owned())
        })
        .unwrap_or_else(|| "unknown".to_owned())
}

/// Reads bblanchon's `VERSION` file (`MAJOR=158` ... one `KEY=value` per line).
fn parse_version_file(text: &str) -> Option<String> {
    let value = |key: &str| {
        text.lines().find_map(|line| {
            let (k, v) = line.split_once('=')?;
            (k.trim() == key).then(|| v.trim().parse::<u64>().ok()).flatten()
        })
    };
    Some(format!(
        "{}.{}.{}.{}",
        value("MAJOR")?,
        value("MINOR")?,
        value("BUILD")?,
        value("PATCH")?
    ))
}

/// Per-user data directory of pdfxray; the same rule as `pdfxray_verapdf::default_install_dir`.
fn data_dir_from(var: &impl Fn(&str) -> Option<OsString>) -> Option<PathBuf> {
    if let Some(dir) = var("PDFXRAY_DATA_DIR") {
        return Some(PathBuf::from(dir));
    }
    if cfg!(windows) {
        return var("APPDATA").map(|d| PathBuf::from(d).join("pdfxray"));
    }
    if cfg!(target_os = "macos") {
        return var("HOME").map(|h| PathBuf::from(h).join("Library/Application Support/pdfxray"));
    }
    var("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| var("HOME").map(|h| PathBuf::from(h).join(".local/share")))
        .map(|d| d.join("pdfxray"))
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::sync::atomic::{AtomicU32, Ordering};

    use pretty_assertions::assert_eq;

    use super::*;

    static NEXT: AtomicU32 = AtomicU32::new(0);

    /// A unique temp directory removed on drop.
    struct Scratch(PathBuf);

    impl Scratch {
        fn new() -> Self {
            let n = NEXT.fetch_add(1, Ordering::Relaxed);
            let dir = std::env::temp_dir().join(format!("pdfxray-pdfium-test-{}-{n}", std::process::id()));
            let _ = fs::remove_dir_all(&dir);
            fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }

        /// Creates `<rel>/lib/<library>` (and `VERSION` when given) and returns the library path.
        fn install(&self, rel: &str, version_file: Option<&str>) -> PathBuf {
            let root = self.0.join(rel);
            fs::create_dir_all(root.join("lib")).unwrap();
            let library = root.join("lib").join(library_name());
            fs::write(&library, b"not a real library").unwrap();
            if let Some(text) = version_file {
                fs::write(root.join("VERSION"), text).unwrap();
            }
            library
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn search(workspace: &Path, env: Option<&str>, data_dir: Option<PathBuf>) -> Search {
        Search {
            env: env.map(OsString::from),
            workspace: workspace.to_path_buf(),
            data_dir,
        }
    }

    const VERSION_FILE: &str = "MAJOR=158\nMINOR=0\nBUILD=8125\nPATCH=0\n";

    #[test]
    fn explicit_path_wins_and_is_exclusive() {
        let tmp = Scratch::new();
        let explicit = tmp.install("explicit", None);
        let env = tmp.install("env", None);
        let s = search(&tmp.0.join("ws"), env.to_str(), None);

        assert_eq!(s.find(Some(&explicit)).unwrap(), explicit);
        let missing = tmp.0.join("nope");
        assert_eq!(
            s.find(Some(&missing)),
            Err(PdfiumError::NotFound {
                searched: vec![missing]
            })
        );
    }

    #[test]
    fn explicit_directory_may_hold_the_library_directly_or_in_lib() {
        let tmp = Scratch::new();
        let nested = tmp.install("a", None);
        let s = search(&tmp.0, None, None);

        assert_eq!(s.find(tmp.0.join("a").as_path().into()).unwrap(), nested);
        assert_eq!(s.find(tmp.0.join("a/lib").as_path().into()).unwrap(), nested);
    }

    #[test]
    fn env_beats_workspace_beats_data_dir() {
        let tmp = Scratch::new();
        let env = tmp.install("env", None);
        let workspace = tmp.install("ws/target/pdfium/1.0.0.0", None);
        let data = tmp.install("data/pdfium/9.9.9.9", None);
        let data_root = Some(tmp.0.join("data/pdfium"));

        let with_env = search(&tmp.0.join("ws"), env.to_str(), data_root.clone());
        assert_eq!(with_env.find(None).unwrap(), env);
        let without_env = search(&tmp.0.join("ws"), None, data_root.clone());
        assert_eq!(without_env.find(None).unwrap(), workspace);
        let data_only = search(&tmp.0.join("empty"), None, data_root);
        assert_eq!(data_only.find(None).unwrap(), data);
    }

    #[test]
    fn highest_version_directory_wins_numerically() {
        let tmp = Scratch::new();
        tmp.install("ws/target/pdfium/9.0.100.0", None);
        let newest = tmp.install("ws/target/pdfium/158.0.8125.0", None);
        tmp.install("ws/target/pdfium/158.0.999.0", None);
        let s = search(&tmp.0.join("ws"), None, None);

        assert_eq!(s.find(None).unwrap(), newest);
    }

    #[test]
    fn a_version_directory_without_the_library_is_skipped() {
        let tmp = Scratch::new();
        fs::create_dir_all(tmp.0.join("ws/target/pdfium/200.0.0.0/lib")).unwrap();
        let older = tmp.install("ws/target/pdfium/158.0.8125.0", None);
        let s = search(&tmp.0.join("ws"), None, None);

        assert_eq!(s.find(None).unwrap(), older);
    }

    #[test]
    fn not_found_lists_every_location_searched() {
        let tmp = Scratch::new();
        let s = search(
            &tmp.0.join("ws"),
            Some("/x/pdfium"),
            Some(tmp.0.join("data/pdfium")),
        );

        let Err(PdfiumError::NotFound { searched }) = s.find(None) else {
            panic!("expected NotFound");
        };
        assert_eq!(
            searched,
            [
                PathBuf::from("/x/pdfium"),
                tmp.0.join("ws/target/pdfium"),
                tmp.0.join("data/pdfium")
            ]
        );
        let message = PdfiumError::NotFound { searched }.to_string();
        assert!(message.contains("cargo xtask pdfium fetch"), "{message}");
    }

    #[test]
    fn the_required_switch_is_not_a_path() {
        let vars = |k: &str| (k == LIBRARY_ENV).then(|| OsString::from("required"));
        assert_eq!(Search::from_vars(vars).env, None);
        let vars = |k: &str| (k == LIBRARY_ENV).then(|| OsString::from("/opt/pdfium"));
        assert_eq!(Search::from_vars(vars).env, Some(OsString::from("/opt/pdfium")));
        let vars = |k: &str| (k == LIBRARY_ENV).then(OsString::new);
        assert_eq!(Search::from_vars(vars).env, None);
    }

    #[test]
    fn version_comes_from_the_version_file_then_the_directory_name() {
        let tmp = Scratch::new();
        let with_file = tmp.install("a/158.0.8125.0", Some(VERSION_FILE));
        let by_name = tmp.install("b/12.3.4.5", None);
        let neither = tmp.install("c/custom", None);
        let broken = tmp.install("d/7.0.0.0", Some("MAJOR=x\n"));

        assert_eq!(version_of(&with_file), "158.0.8125.0");
        assert_eq!(version_of(&by_name), "12.3.4.5");
        assert_eq!(version_of(&neither), "unknown");
        // an unreadable VERSION file falls back to the directory name
        assert_eq!(version_of(&broken), "7.0.0.0");
    }

    #[test]
    fn version_file_needs_all_four_numbers() {
        assert_eq!(parse_version_file(VERSION_FILE).as_deref(), Some("158.0.8125.0"));
        assert_eq!(parse_version_file("MAJOR=1\nMINOR=2\nBUILD=3\n"), None);
        assert_eq!(parse_version_file(""), None);
        // smoke: key order and spacing do not matter
        assert_eq!(
            parse_version_file("PATCH = 4\nBUILD=3\r\nMINOR=2\nMAJOR=1").as_deref(),
            Some("1.2.3.4")
        );
    }

    #[test]
    fn data_dir_follows_the_verapdf_rule() {
        let var = |k: &str| (k == "PDFXRAY_DATA_DIR").then(|| OsString::from("/x/y"));
        assert_eq!(data_dir_from(&var), Some(PathBuf::from("/x/y")));

        let home = |k: &str| (k == "HOME").then(|| OsString::from("/home/u"));
        let expected = if cfg!(target_os = "macos") {
            PathBuf::from("/home/u/Library/Application Support/pdfxray")
        } else {
            PathBuf::from("/home/u/.local/share/pdfxray")
        };
        assert_eq!(data_dir_from(&home), Some(expected));
        assert_eq!(data_dir_from(&|_: &str| None), None);
    }

    #[test]
    fn loading_a_file_that_is_not_a_library_reports_the_path() {
        let tmp = Scratch::new();
        let fake = tmp.install("fake", None);
        let search = search(&tmp.0, None, None);
        let found = search.find(Some(&fake)).unwrap();

        let error = Pdfium::bind_to_library(&found).err().expect("must fail to load");
        let wrapped = PdfiumError::Load {
            path: found.clone(),
            message: error.to_string(),
        };

        assert!(
            wrapped.to_string().starts_with("cannot load the PDFium library "),
            "{wrapped}"
        );
        assert!(wrapped.to_string().contains(found.to_str().unwrap()), "{wrapped}");
    }
}
