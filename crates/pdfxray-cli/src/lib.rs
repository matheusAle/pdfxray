//! `pdfxray` command line: JSON over the core query API.

use std::collections::BTreeSet;
use std::io::Write;
use std::path::PathBuf;
use std::str::FromStr;
use std::time::Duration;

use anyhow::{Context, anyhow};
use clap::{Parser, Subcommand, ValueEnum};
use pdfxray_core::{
    BoxKind, Document, LayerMode, NodeId, PageIx, Point, ReadingOrderSource, Rect, SpeechPreset, TreeKind,
};
use pdfxray_render::pdfium::PdfiumRenderer;
use pdfxray_render::{
    ColorBy, ContentFilter, Overlay, RenderSpec, XrayOptions, alpha_heatmap_overlays, clip_overlays, diff,
    ops_for, render_page, write_png, xray_overlays,
};
use pdfxray_verapdf::{Discovery, DiscoveryConfig, Profile, RunOptions, VeraPdf, discover};
use serde::Serialize;

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
    /// Render a page to PNG, optionally with page boxes and highlighted nodes drawn on top.
    Render {
        /// PDF file to render.
        file: PathBuf,
        /// Zero-based page index.
        page: PageIx,
        /// Output PNG path.
        #[arg(short, long)]
        output: PathBuf,
        /// Pixels per point.
        #[arg(long, default_value_t = 2.0)]
        scale: f32,
        /// Outline every defined page box.
        #[arg(long)]
        boxes: bool,
        /// Outline these nodes (repeatable), e.g. `--highlight mc:0:1`.
        #[arg(long)]
        highlight: Vec<NodeId>,
        /// Outline every painted item, coloured by kind, tag or artifact status.
        #[arg(long, value_enum)]
        xray: Option<XrayArg>,
        /// Also mark invisible items in X-ray (invisible text, white-on-white, off-page).
        #[arg(long)]
        invisible: bool,
        /// Paint only the content of these nodes (repeatable).
        #[arg(long, conflicts_with_all = ["hide", "step"])]
        isolate: Vec<NodeId>,
        /// Paint everything except the content of these nodes (repeatable).
        #[arg(long, conflicts_with = "step")]
        hide: Vec<NodeId>,
        /// Paint-order stepper: paint operators up to this execution index only.
        #[arg(long)]
        step: Option<u32>,
        /// Show the clip in effect after this operator.
        #[arg(long)]
        clip: Option<u32>,
        /// Tint items with transparency, soft masks or non-Normal blend modes.
        #[arg(long)]
        alpha: bool,
    },
    /// Render a page with hayro and with the `PDFium` reference engine and write a diff image:
    /// grey = agreement, red = the engines disagree. Tells engine bugs from file bugs.
    Compare {
        /// PDF file to compare.
        file: PathBuf,
        /// Zero-based page index.
        page: PageIx,
        /// Output PNG path for the diff image.
        #[arg(short, long)]
        output: PathBuf,
        /// Pixels per point.
        #[arg(long, default_value_t = 1.0)]
        scale: f32,
        /// Per-channel difference that still counts as equal.
        #[arg(long, default_value_t = 48)]
        tolerance: u8,
        /// `PDFium` library (file or directory) instead of the default search
        /// (`PDFXRAY_PDFIUM`, `target/pdfium`, the pdfxray data directory).
        #[arg(long)]
        pdfium: Option<PathBuf>,
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

/// Runs `cli`, writing JSON to `out`.
pub fn run(cli: &Cli, out: &mut impl Write) -> anyhow::Result<()> {
    match &cli.command {
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
        Command::Render {
            file,
            page,
            output,
            scale,
            boxes,
            highlight,
            xray,
            invisible,
            isolate,
            hide,
            step,
            clip,
            alpha,
        } => {
            let doc = open(file)?;
            let info = doc
                .page(*page)
                .ok_or_else(|| anyhow!("page {page} out of range (document has {})", doc.page_count()))?;
            let mut spec = RenderSpec::new(*scale);
            if *boxes {
                spec.overlays.extend(
                    info.defined_boxes()
                        .map(|(kind, rect)| outline(rect, box_colour(kind))),
                );
            }
            for id in highlight {
                match doc.bbox(id) {
                    Some((p, rect)) if p == *page => spec.overlays.push(outline(rect, HIGHLIGHT)),
                    Some((p, _)) => return Err(anyhow!("`{id}` is on page {p}, not {page}")),
                    None => return Err(anyhow!("`{id}` has no geometry")),
                }
            }
            if let Some(by) = xray {
                let options = XrayOptions {
                    color_by: (*by).into(),
                    include_invisible: *invisible,
                    ..XrayOptions::default()
                };
                spec.overlays.extend(xray_overlays(&doc, *page, options));
            }
            if let Some(op) = clip {
                spec.overlays.extend(clip_overlays(&doc, *page, *op));
            }
            if *alpha {
                spec.overlays.extend(alpha_heatmap_overlays(&doc, *page));
            }
            spec.content = if !isolate.is_empty() {
                ContentFilter::Only(op_set(&doc, isolate)?)
            } else if !hide.is_empty() {
                ContentFilter::Hide(op_set(&doc, hide)?)
            } else if let Some(n) = step {
                ContentFilter::UpTo(*n)
            } else {
                ContentFilter::All
            };
            let rendered =
                render_page(&doc, *page, &spec).with_context(|| format!("rendering page {page}"))?;
            write_png(&rendered, output).with_context(|| format!("writing {}", output.display()))?;
            write_json(
                out,
                &serde_json::json!({ "output": output, "width": rendered.width, "height": rendered.height }),
                cli.pretty,
            )
        }
        Command::Compare {
            file,
            page,
            output,
            scale,
            tolerance,
            pdfium,
        } => {
            let doc = open(file)?;
            if doc.page(*page).is_none() {
                return Err(anyhow!(
                    "page {page} out of range (document has {})",
                    doc.page_count()
                ));
            }
            let reference = PdfiumRenderer::load(pdfium.as_deref())
                .context("loading PDFium; run `cargo xtask pdfium fetch` or pass --pdfium <path>")?;
            let ours = render_page(&doc, *page, &RenderSpec::new(*scale))
                .with_context(|| format!("rendering page {page} with hayro"))?;
            let theirs = reference
                .render_page(&doc, *page, *scale)
                .with_context(|| format!("rendering page {page} with PDFium"))?;
            let d = diff(&ours, &theirs, *tolerance);
            image::save_buffer(
                output,
                &d.image,
                d.width,
                d.height,
                image::ExtendedColorType::Rgba8,
            )
            .with_context(|| format!("writing {}", output.display()))?;
            let total = f64::from(d.width) * f64::from(d.height);
            let differing = u32::try_from(d.differing_pixels).map_or(f64::INFINITY, f64::from);
            write_json(
                out,
                &serde_json::json!({
                    "output": output,
                    "pdfium_version": reference.version(),
                    "width": d.width,
                    "height": d.height,
                    "differing_pixels": d.differing_pixels,
                    "differing_fraction": if total > 0.0 { differing / total } else { 0.0 },
                    "max_channel_delta": d.max_channel_delta,
                    "mean_delta": d.mean_delta,
                }),
                cli.pretty,
            )
        }
        Command::Validate {
            file,
            profile,
            profile_file,
            verapdf,
            install,
            timeout,
        } => {
            let mut doc = open(file)?;
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
            write_json(out, &doc.validation(), cli.pretty)
        }
    }
}

/// `--xray` colouring.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum XrayArg {
    /// By item kind: glyph, path, image, shading.
    Kind,
    /// By innermost marked-content tag.
    Tag,
    /// Artifact vs tagged vs untagged.
    Artifact,
}

impl From<XrayArg> for ColorBy {
    fn from(x: XrayArg) -> Self {
        match x {
            XrayArg::Kind => Self::Kind,
            XrayArg::Tag => Self::Tag,
            XrayArg::Artifact => Self::Artifact,
        }
    }
}

fn op_set(doc: &Document, ids: &[NodeId]) -> anyhow::Result<BTreeSet<u32>> {
    let mut ops = BTreeSet::new();
    for id in ids {
        let found = ops_for(doc, id);
        if found.is_empty() {
            return Err(anyhow!("`{id}` paints nothing that can be isolated or hidden"));
        }
        ops.extend(found);
    }
    Ok(ops)
}

const HIGHLIGHT: [u8; 4] = [255, 0, 160, 255];

fn box_colour(kind: BoxKind) -> [u8; 4] {
    match kind {
        BoxKind::Media => [220, 40, 40, 255],
        BoxKind::Crop => [40, 160, 40, 255],
        BoxKind::Bleed => [40, 90, 220, 255],
        BoxKind::Trim => [200, 60, 200, 255],
        BoxKind::Art => [230, 150, 0, 255],
    }
}

fn outline(rect: Rect, stroke: [u8; 4]) -> Overlay {
    Overlay::Rect {
        rect,
        stroke,
        fill: None,
        width: 2.0,
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
