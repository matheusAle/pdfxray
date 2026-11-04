//! `pdfxray` command line: JSON over the core query API.

mod diff;
mod render;
mod visual;

use std::io::Write;
use std::path::PathBuf;
use std::str::FromStr;
use std::time::Duration;

use anyhow::{Context, anyhow};
use clap::{Parser, Subcommand, ValueEnum};
use pdfxray_core::{Document, LayerMode, NodeId, PageIx, Point, ReadingOrderSource, SpeechPreset, TreeKind};
use pdfxray_verapdf::{Discovery, DiscoveryConfig, Profile, RunOptions, VeraPdf, discover};
use serde::Serialize;

pub use self::diff::ScopeArg;
pub use self::render::RenderArgs;
pub use self::visual::VisualArgs;

/// Command-line arguments.
#[derive(Debug, Parser)]
#[command(
    name = "pdfxray",
    version,
    about = "Inspect PDF structure, layout and resources"
)]
pub struct Cli {
    /// Pretty-print JSON output.
    #[arg(long, global = true)]
    pub pretty: bool,
    /// What to do.
    #[command(subcommand)]
    pub command: Command,
}

/// Subcommands.
#[derive(Debug, Subcommand)]
pub enum Command {
    /// Summary facts and diagnostics for a file.
    Info {
        /// PDF file to inspect.
        file: PathBuf,
    },
    /// A navigation tree.
    Tree {
        /// PDF file to inspect.
        file: PathBuf,
        /// Which tree: pages, objects, tags, artifacts, resources, diagnostics, or
        /// content:<page> (zero-based).
        // clap prints this comment verbatim as `--help` text, so `<page>` stays unescaped.
        #[allow(rustdoc::invalid_html_tags)]
        #[arg(long, default_value = "pages")]
        kind: TreeArg,
    },
    /// Inspector properties of one node, e.g. `page:0`, `obj:12:0`, `mc:0:3`.
    Node {
        /// PDF file to inspect.
        file: PathBuf,
        /// Node id.
        id: NodeId,
    },
    /// Path from the document root to a node.
    Breadcrumb {
        /// PDF file to inspect.
        file: PathBuf,
        /// Node id.
        id: NodeId,
    },
    /// Everything under a page-space point (points, PDF coordinates), most specific first.
    Hit {
        /// PDF file to inspect.
        file: PathBuf,
        /// Zero-based page index.
        page: PageIx,
        /// X in points.
        #[arg(allow_negative_numbers = true)]
        x: f64,
        /// Y in points.
        #[arg(allow_negative_numbers = true)]
        y: f64,
        /// Which kinds of nodes may be hit.
        #[arg(long, value_enum, default_value_t = LayerArg::All)]
        layer: LayerArg,
    },
    /// The order a page's tagged content is read in: the elements that directly own content, with
    /// their role class, box and a text preview. Each step has a kind: first, forward, column
    /// break, detour (a note or aside) or backtrack. Only a step that goes back up the page within
    /// its flow is a backtrack.
    ReadingOrder {
        /// PDF file to inspect.
        file: PathBuf,
        /// Zero-based page index.
        page: PageIx,
        /// Which order to list.
        #[arg(long = "from", value_enum, default_value_t = OrderArg::Structure)]
        from: OrderArg,
    },
    /// What a screen reader announces for the whole document, one item per announcement in order:
    /// the structure tree read with the substitution and skip rules of a reader preset (Alt and
    /// `ActualText` replacing elements, artifacts skipped, table cells with their header cells,
    /// language changes, footnote and link flags). The presets other than `structure` are
    /// approximations of real viewers, not recordings.
    Speech {
        /// PDF file to inspect.
        file: PathBuf,
        /// Which reader to model.
        #[arg(long, value_enum, default_value_t = PresetArg::Structure)]
        preset: PresetArg,
        /// Also list what is not spoken (artifacts, `Private` content, content hidden under an
        /// `Alt`, empty elements), each with its reason.
        #[arg(long)]
        show_skipped: bool,
    },
    /// The grid of a table element and the header cells of each cell, with the rule that found
    /// each one. A PDF 2.0 file with namespaces follows ISO 32000-2's header-finding algorithm,
    /// any other file the PDF 1.7 rules.
    Table {
        /// PDF file to inspect.
        file: PathBuf,
        /// The `Table` structure element, e.g. `struct:12:0`.
        id: NodeId,
    },
    /// Every structure element that draws on a page, with its role class, depth and box.
    TagBoxes {
        /// PDF file to inspect.
        file: PathBuf,
        /// Zero-based page index.
        page: PageIx,
    },
    /// Render a page to PNG or JPEG, optionally with page boxes, highlights, the x-ray and the
    /// View menu's annotations drawn on top; or compare it with another file (`--against`) or a
    /// reference engine (`--reference pdfium`), side by side, overlaid or as a mask.
    Render(render::RenderArgs),
    /// Compare two PDFs: which structure elements correspond and what changed (modified,
    /// moved, added, removed), matched by what they say and where they are, never by object
    /// number. Exits 0 when nothing in scope changed, 1 when something did, 2 on error.
    Diff {
        /// The PDF compared against (A).
        a: PathBuf,
        /// The PDF compared (B).
        b: PathBuf,
        /// Only page N of A (zero-based) and what corresponds to it in B.
        #[arg(long)]
        page: Option<PageIx>,
        /// Which part of the comparison: element pairs and page residue, element pairs only, or
        /// only the residue of each page (painted content outside the structure tree).
        #[arg(long, value_enum, default_value_t = ScopeArg::All)]
        scope: ScopeArg,
        /// Also list the pairs and pages that did not change.
        #[arg(long)]
        include_unchanged: bool,
        /// Exit 0 when the documents differ too (errors still exit 2).
        #[arg(long)]
        exit_zero: bool,
        /// Also score each page pair visually: both pages drawn by hayro on one pixel grid and
        /// compared perceptually, beside the residue (never instead of it). Costs two renders
        /// per page pair; without it nothing is drawn. Never changes the exit status.
        #[arg(long)]
        visual: bool,
        /// Pixels per point the pages are drawn at for `--visual`. Default 2.
        #[arg(long)]
        visual_scale: Option<f32>,
        /// How `--visual` compares the pixels.
        #[command(flatten)]
        perceptual: VisualArgs,
    },
    /// Validate with veraPDF (run as a separate Java process) and map failures to nodes.
    Validate {
        /// PDF file to validate.
        file: PathBuf,
        /// Validation profile.
        #[arg(long, value_enum, default_value_t = ProfileArg::Ua1)]
        profile: ProfileArg,
        /// Custom veraPDF profile XML (overrides `--profile`).
        #[arg(long)]
        profile_file: Option<PathBuf>,
        /// veraPDF launcher or CLI jar to use instead of discovery.
        #[arg(long)]
        verapdf: Option<PathBuf>,
        /// Download and install the pinned veraPDF release if none is found.
        #[arg(long)]
        install: bool,
        /// Give up after this many seconds.
        #[arg(long, default_value_t = 300)]
        timeout: u64,
    },
}

/// `--profile` values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum ProfileArg {
    /// PDF/UA-1 (ISO 14289-1).
    Ua1,
    /// PDF/UA-2 (ISO 14289-2).
    Ua2,
    /// WCAG 2.2, machine-checkable rules.
    Wcag22,
    /// Well-Tagged PDF 1.0, accessibility.
    Wtpdf,
    /// ISO 32000-1 Tagged PDF.
    Tagged1,
    /// ISO 32000-2 Tagged PDF.
    Tagged2,
    /// PDF/A-1a.
    Pdfa1a,
    /// PDF/A-1b.
    Pdfa1b,
    /// PDF/A-2a.
    Pdfa2a,
    /// PDF/A-2b.
    Pdfa2b,
    /// PDF/A-2u.
    Pdfa2u,
    /// PDF/A-3a.
    Pdfa3a,
    /// PDF/A-3b.
    Pdfa3b,
    /// PDF/A-3u.
    Pdfa3u,
    /// PDF/A-4.
    Pdfa4,
    /// PDF/A-4e.
    Pdfa4e,
    /// PDF/A-4f.
    Pdfa4f,
}

impl From<ProfileArg> for Profile {
    fn from(p: ProfileArg) -> Self {
        match p {
            ProfileArg::Ua1 => Self::PdfUa1,
            ProfileArg::Ua2 => Self::PdfUa2,
            ProfileArg::Wcag22 => Self::Wcag22Machine,
            ProfileArg::Wtpdf => Self::Wtpdf10Accessibility,
            ProfileArg::Tagged1 => Self::Iso32000_1Tagged,
            ProfileArg::Tagged2 => Self::Iso32000_2Tagged,
            ProfileArg::Pdfa1a => Self::PdfA1a,
            ProfileArg::Pdfa1b => Self::PdfA1b,
            ProfileArg::Pdfa2a => Self::PdfA2a,
            ProfileArg::Pdfa2b => Self::PdfA2b,
            ProfileArg::Pdfa2u => Self::PdfA2u,
            ProfileArg::Pdfa3a => Self::PdfA3a,
            ProfileArg::Pdfa3b => Self::PdfA3b,
            ProfileArg::Pdfa3u => Self::PdfA3u,
            ProfileArg::Pdfa4 => Self::PdfA4,
            ProfileArg::Pdfa4e => Self::PdfA4e,
            ProfileArg::Pdfa4f => Self::PdfA4f,
        }
    }
}

/// Parsed `--kind` value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TreeArg(pub TreeKind);

impl FromStr for TreeArg {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let kind = match s {
            "pages" => TreeKind::Pages,
            "objects" => TreeKind::Objects,
            "tags" => TreeKind::Tags,
            "artifacts" => TreeKind::Artifacts,
            "resources" => TreeKind::Resources,
            "diagnostics" => TreeKind::Diagnostics,
            other => {
                let page = other
                    .strip_prefix("content:")
                    .and_then(|p| p.parse().ok())
                    .ok_or_else(|| format!("unknown tree kind `{other}`"))?;
                TreeKind::Content(page)
            }
        };
        Ok(Self(kind))
    }
}

/// `--from` values of `reading-order`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum OrderArg {
    /// The structure tree's document order (what a screen reader follows).
    Structure,
    /// The order the content stream starts painting each element.
    Paint,
}

impl OrderArg {
    const fn name(self) -> &'static str {
        match self {
            Self::Structure => "structure",
            Self::Paint => "paint",
        }
    }
}

impl From<OrderArg> for ReadingOrderSource {
    fn from(o: OrderArg) -> Self {
        match o {
            OrderArg::Structure => Self::Structure,
            OrderArg::Paint => Self::Paint,
        }
    }
}

/// `--preset` values of `speech`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum PresetArg {
    /// The structure tree read as ISO 32000 describes it (the spec-neutral default).
    Structure,
    /// Acrobat on Windows, as NVDA and JAWS consume it.
    Acrobat,
    /// Firefox's pdf.js viewer.
    Firefox,
    /// The default Chrome and Edge viewer: tags ignored, content in paint order.
    Chromium,
}

impl From<PresetArg> for SpeechPreset {
    fn from(p: PresetArg) -> Self {
        match p {
            PresetArg::Structure => Self::Structure,
            PresetArg::Acrobat => Self::AcrobatWindows,
            PresetArg::Firefox => Self::FirefoxPdfJs,
            PresetArg::Chromium => Self::ChromiumDefault,
        }
    }
}

/// `--layer` values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum LayerArg {
    /// Everything.
    All,
    /// Structure elements and marked content.
    Tags,
    /// Operators and glyphs.
    Content,
    /// Annotations.
    Annotations,
    /// Page boxes.
    Boxes,
    /// Artifacts.
    Artifacts,
}

impl From<LayerArg> for LayerMode {
    fn from(l: LayerArg) -> Self {
        match l {
            LayerArg::All => Self::All,
            LayerArg::Tags => Self::Tags,
            LayerArg::Content => Self::Content,
            LayerArg::Annotations => Self::Annotations,
            LayerArg::Boxes => Self::Boxes,
            LayerArg::Artifacts => Self::Artifacts,
        }
    }
}

/// Default `--visual-scale`: `render`'s default scale, and a retina canvas at 100 %.
const DEFAULT_VISUAL_SCALE: f32 = 2.0;

/// Runs `cli`, writing JSON to `out`.
pub fn run(cli: &Cli, out: &mut impl Write) -> anyhow::Result<Outcome> {
    let mut outcome = Outcome::Done;
    run_command(cli, out, &mut outcome)?;
    Ok(outcome)
}

/// `pdfxray diff --visual`: how the pages are scored.
#[derive(Debug, Clone, Copy)]
struct VisualRequest {
    scale: f32,
    args: visual::VisualArgs,
}

/// `pdfxray diff`: pairs the two files and prints the part `view` asks for, with the visual
/// score of each page pair in it when `visual` asks for one.
fn run_diff(
    (a, b): (&PathBuf, &PathBuf),
    view: diff::DiffView,
    visual: Option<VisualRequest>,
    exit_zero: bool,
    pretty: bool,
    out: &mut impl Write,
) -> anyhow::Result<Outcome> {
    let (doc_a, doc_b) = (open(a)?, open(b)?);
    if let Some(n) = view.page
        && doc_a.page(n).is_none()
    {
        return Err(anyhow!(
            "page {n} out of range ({} has {})",
            a.display(),
            doc_a.page_count()
        ));
    }
    let pairing = pdfxray_core::diff::pair(&doc_a, &doc_b);
    let scores = visual.map(|v| {
        let how = v.args.perceptual();
        let scores = match view.page {
            None => pdfxray_render::page_scores(&doc_a, &doc_b, &pairing, v.scale, &how),
            // Only the page pair shown is drawn.
            Some(n) => pairing
                .pages
                .iter()
                .filter(|m| m.a == Some(n))
                .map(|m| pdfxray_render::page_score(&doc_a, &doc_b, m, v.scale, &how))
                .collect(),
        };
        diff::Scores {
            settings: v.args.settings(v.scale),
            pages: scores,
        }
    });
    let shown = diff::narrow(&pairing, view, scores.as_ref());
    write_json(out, &shown, pretty)?;
    Ok(if shown.identical || exit_zero {
        Outcome::Done
    } else {
        Outcome::Differences
    })
}

/// How a successful run ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// The command did what it was asked; exit status 0.
    Done,
    /// `diff` found differences; exit status 1.
    Differences,
}

impl Command {
    /// The exit status of a run of this command that failed: 2 for `diff` (whose 1 means
    /// "differences"), 1 for the others.
    pub fn error_status(&self) -> u8 {
        if matches!(self, Self::Diff { .. }) { 2 } else { 1 }
    }
}

fn run_command(cli: &Cli, out: &mut impl Write, outcome: &mut Outcome) -> anyhow::Result<()> {
    match &cli.command {
        Command::Diff {
            a,
            b,
            page,
            scope,
            include_unchanged,
            exit_zero,
            visual,
            visual_scale,
            perceptual,
        } => {
            let view = diff::DiffView {
                scope: *scope,
                page: *page,
                include_unchanged: *include_unchanged,
            };
            perceptual.only_with(*visual, "--visual")?;
            if visual_scale.is_some() && !*visual {
                return Err(anyhow!("--visual-scale needs --visual"));
            }
            if *visual && *scope == ScopeArg::Tags {
                return Err(anyhow!(
                    "--visual scores ink: use it with --scope all or --scope ink"
                ));
            }
            let scale = visual_scale.unwrap_or(DEFAULT_VISUAL_SCALE);
            if !(scale.is_finite() && scale > 0.0) {
                return Err(anyhow!("--visual-scale must be a positive number"));
            }
            let request = visual.then_some(VisualRequest {
                scale,
                args: *perceptual,
            });
            *outcome = run_diff((a, b), view, request, *exit_zero, cli.pretty, out)?;
            Ok(())
        }
        Command::Info { file } => write_json(out, &open(file)?.info(), cli.pretty),
        Command::Tree { file, kind } => write_json(out, &open(file)?.tree(kind.0), cli.pretty),
        Command::Node { file, id } => {
            let props = open(file)?
                .node(id)
                .ok_or_else(|| anyhow!("no node `{id}` in {}", file.display()))?;
            write_json(out, &props, cli.pretty)
        }
        Command::Breadcrumb { file, id } => write_json(out, &open(file)?.breadcrumb(id), cli.pretty),
        Command::Hit {
            file,
            page,
            x,
            y,
            layer,
        } => {
            let doc = open(file)?;
            if doc.page(*page).is_none() {
                return Err(anyhow!(
                    "page {page} out of range (document has {})",
                    doc.page_count()
                ));
            }
            let hits = doc.hit(*page, Point::new(*x, *y), (*layer).into());
            write_json(out, &hits, cli.pretty)
        }
        Command::ReadingOrder { file, page, from } => {
            let doc = open_page(file, *page)?;
            let order = doc.reading_order(*page, (*from).into());
            write_json(
                out,
                &serde_json::json!({
                    "page": page,
                    "from": from.name(),
                    "backtracks": order.backtracks,
                    "steps": order.steps,
                }),
                cli.pretty,
            )
        }
        Command::Speech {
            file,
            preset,
            show_skipped,
        } => {
            let doc = open(file)?;
            let preset = SpeechPreset::from(*preset);
            let order = doc.speech_order(preset);
            let items: Vec<_> = order
                .items
                .iter()
                .filter(|item| *show_skipped || item.skipped.is_none())
                .collect();
            write_json(
                out,
                &serde_json::json!({
                    "preset": preset,
                    "label": preset.label(),
                    "approximation": preset.is_approximation(),
                    "rules": preset.rules(),
                    "items": items,
                }),
                cli.pretty,
            )
        }
        Command::Table { file, id } => {
            let model = open(file)?
                .table(id)
                .ok_or_else(|| anyhow!("`{id}` is not a Table element of {}", file.display()))?;
            write_json(out, &model, cli.pretty)
        }
        Command::TagBoxes { file, page } => {
            let doc = open_page(file, *page)?;
            write_json(out, &doc.tag_boxes(*page), cli.pretty)
        }
        Command::Render(args) => render::run(args, cli.pretty, out),
        Command::Validate {
            file,
            profile,
            profile_file,
            verapdf,
            install,
            timeout,
        } => {
            let doc = open(file)?;
            let profile = profile_file
                .clone()
                .map_or_else(|| Profile::from(*profile), Profile::Custom);
            let vera = find_verapdf(verapdf.clone(), *install)?;
            let opts = RunOptions {
                timeout: Duration::from_secs(*timeout),
                ..RunOptions::default()
            };
            let report = vera
                .validate(file, &profile, &opts)
                .with_context(|| format!("validating {} with {}", file.display(), profile.label()))?;
            doc.set_validation(&report);
            write_json(out, &doc.validation().as_deref(), cli.pretty)
        }
    }
}

fn find_verapdf(explicit: Option<PathBuf>, install: bool) -> anyhow::Result<VeraPdf> {
    let config = DiscoveryConfig {
        explicit_path: explicit,
        ..DiscoveryConfig::default()
    };
    match discover(&config) {
        Discovery::Found(v) => Ok(v),
        Discovery::JavaMissing => Err(anyhow!("veraPDF needs Java 11 or newer on PATH or in JAVA_HOME")),
        Discovery::NotInstalled if install => {
            let dir = config
                .app_data_dir
                .ok_or_else(|| anyhow!("no data directory; set PDFXRAY_DATA_DIR"))?;
            Ok(pdfxray_verapdf::install(&dir, |_, _| {})?)
        }
        Discovery::NotInstalled => Err(anyhow!(
            "veraPDF not found; pass --verapdf <path> or --install to download the pinned release"
        )),
    }
}

/// Opens `file` and checks that it has page `page`.
fn open_page(file: &PathBuf, page: PageIx) -> anyhow::Result<Document> {
    let doc = open(file)?;
    if doc.page(page).is_none() {
        return Err(anyhow!(
            "page {page} out of range (document has {})",
            doc.page_count()
        ));
    }
    Ok(doc)
}

fn open(file: &PathBuf) -> anyhow::Result<Document> {
    Document::open(file).with_context(|| format!("opening {}", file.display()))
}

fn write_json(out: &mut impl Write, value: &impl Serialize, pretty: bool) -> anyhow::Result<()> {
    if pretty {
        serde_json::to_writer_pretty(&mut *out, value)?;
    } else {
        serde_json::to_writer(&mut *out, value)?;
    }
    writeln!(out)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tree_arg_parses_all_kinds() {
        assert_eq!("pages".parse(), Ok(TreeArg(TreeKind::Pages)));
        assert_eq!("content:3".parse(), Ok(TreeArg(TreeKind::Content(3))));
        assert_eq!("diagnostics".parse(), Ok(TreeArg(TreeKind::Diagnostics)));
        assert_eq!(
            "content:x".parse::<TreeArg>(),
            Err("unknown tree kind `content:x`".to_owned())
        );
    }
}
