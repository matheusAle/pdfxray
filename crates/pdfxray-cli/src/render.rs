//! `pdfxray render`: a page as PNG or JPEG, with the debug overlays and the View menu's
//! annotations, and one orthogonal axis: what to compare the page against, another file
//! (`--against`) or a reference engine (`--reference`), shown side by side, overlaid or as a mask.
//!
//! A single page can also be drawn as a Windows viewer shows it (`--emulate`) on a given screen
//! (`--dpi`, `--screen`, `--zoom`): at that screen's device pixels, rounded as the viewer rounds.

use std::collections::{BTreeSet, HashMap};
use std::io::Write;
use std::path::PathBuf;

use anyhow::{Context, anyhow};
use clap::{Args, ValueEnum};
use pdfxray_core::diff::{Pairing, Side};
use pdfxray_core::{
    BoxKind, Document, HeaderScope, NodeId, PageIx, ReadingOrder, ReadingOrderSource, TagBox,
};
use pdfxray_render::annotate::{self, ChangeShow, Grid, Legend, Palette, Scene, TagShow, TagView};
use pdfxray_render::composite::{self, Comparator, Composite, Delta, Fit, Mode, Registration, Request};
use pdfxray_render::pdfium::PdfiumRenderer;
use pdfxray_render::{
    AcrobatPrefs, ColorBy, ContentFilter, Display, Emulation, Overlay, RenderSpec, Transform, Viewer,
    ViewerZoom, XrayOptions, alpha_heatmap_overlays, clip_overlays, ops_for, page_dimensions, page_transform,
    render_page, write_image, xray_overlays,
};
use serde_json::{Value, json};

use crate::visual::{ScoreView, VisualArgs};
use crate::{PresetArg, open, write_json};

/// The flags of an emulated screen and viewer: they draw one page, not a comparison.
const EMULATION: [&str; 4] = ["emulate", "dpi", "screen", "zoom"];

/// The debug overlays draw over one page; a comparison draws two.
const ONE_PAGE: [&str; 9] = [
    "boxes",
    "highlight",
    "xray",
    "invisible",
    "isolate",
    "hide",
    "step",
    "clip",
    "alpha",
];

/// Arguments of `render`.
#[derive(Debug, Args)]
pub struct RenderArgs {
    /// PDF file to render (side A of a comparison).
    file: PathBuf,
    /// Zero-based page index.
    page: PageIx,
    /// Output image; `.png`, `.jpg` or `.jpeg`.
    #[arg(short, long)]
    output: PathBuf,
    /// Pixels per point. An emulated screen sets its own (`--dpi`, `--zoom`).
    #[arg(long, default_value_t = 2.0, conflicts_with_all = EMULATION)]
    scale: f32,

    /// Draw the page as this Windows viewer rounds it onto device pixels: Chrome and Edge
    /// (`PDFium`) or Acrobat. On the screen of `--dpi`, `--screen` and `--zoom`.
    #[arg(long, value_enum, conflicts_with_all = ["against", "reference"])]
    emulate: Option<ViewerArg>,
    /// The emulated screen's density in DPI: 96 is Windows' 100 % scaling, 120 is 125 %, 144 is
    /// 150 %. Default 96.
    #[arg(long, value_parser = clap::value_parser!(u32).range(MIN_DPI..=MAX_DPI), conflicts_with_all = ["against", "reference"])]
    dpi: Option<u32>,
    /// The emulated screen's size in device pixels, `WIDTHxHEIGHT`, for the fit zooms. Default
    /// 1920x1080.
    #[arg(long, value_parser = parse_screen, conflicts_with_all = ["against", "reference"])]
    screen: Option<(u32, u32)>,
    /// The emulated viewer's zoom: a percentage (`125`), `fit-width` or `fit-page`. Default 100.
    #[arg(long, value_parser = parse_zoom, conflicts_with_all = ["against", "reference"])]
    zoom: Option<ViewerZoom>,
    /// With `--emulate acrobat`: Acrobat's *Enhance thin lines* off (it is on as Acrobat
    /// starts: a stroke under 1 pt is a solid 1 px black line).
    #[arg(long)]
    no_enhance_thin_lines: bool,
    /// With `--emulate acrobat`: Acrobat's *Line Weights* off (Ctrl+5): every stroke 1 px.
    #[arg(long)]
    no_line_weights: bool,
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

    /// View › All tag boxes: outline every structure element. Compared with another file, the
    /// unchanged elements too.
    #[arg(long)]
    tags: bool,
    /// View › Tag labels: a chip naming each element's type (compared: its role).
    #[arg(long)]
    labels: bool,
    /// View › Artifacts: the artifact sequences and the PDF 2.0 `Artifact` element.
    #[arg(long)]
    artifacts: bool,
    /// View › Nesting: containers too.
    #[arg(long)]
    nesting: bool,
    /// View › Reading order: number the elements in this order and join them with the path.
    #[arg(long, value_enum)]
    reading_order: Option<OrderFrom>,
    /// The reader preset of `--reading-order speech`.
    #[arg(long, value_enum, default_value_t = PresetArg::Structure)]
    speech_preset: PresetArg,

    /// Compare with this PDF: status colours on the elements that changed, the role on each
    /// chip, and a legend.
    #[arg(long, conflicts_with = "reference", conflicts_with_all = ONE_PAGE)]
    against: Option<PathBuf>,
    /// The page of `--against` to compare with; by default the page the pairing matches.
    #[arg(long, requires = "against")]
    against_page: Option<PageIx>,
    /// Compare with the same page drawn by a reference engine.
    #[arg(long, value_enum, conflicts_with_all = ONE_PAGE)]
    reference: Option<EngineArg>,
    /// `PDFium` library (file or directory) instead of the default search (`PDFXRAY_PDFIUM`,
    /// `target/pdfium`, the pdfxray data directory).
    #[arg(long, requires = "reference")]
    pdfium: Option<PathBuf>,
    /// How the two pages are shown. Default: side by side against a file, a mask against an
    /// engine.
    #[arg(long, value_enum)]
    mode: Option<ModeArg>,
    /// Against an engine: the per-channel difference that still counts as equal. Default 48,
    /// lenient because two engines anti-alias every glyph differently. Two files drawn by one
    /// engine are compared perceptually instead (`--threshold`, `--include-aa`).
    #[arg(long)]
    tolerance: Option<u8>,
    /// Against a file: how the pixels are compared.
    #[command(flatten)]
    visual: VisualArgs,
    /// Remove the global offset the pairing reports from B before overlaying it. Labelled in
    /// the legend and the output.
    #[arg(long, requires = "against")]
    register: bool,
    /// Scale B to fit A when the page sizes or `/Rotate` differ, instead of refusing to overlay.
    #[arg(long, requires = "against")]
    scale_b_to_a: bool,
    /// Whose reading order an overlay or a mask numbers: one side at a time.
    #[arg(long, value_enum, default_value_t = SideArg::A)]
    order_side: SideArg,
}

/// `--xray` colouring.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub(crate) enum XrayArg {
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

/// `--reading-order` values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub(crate) enum OrderFrom {
    /// The structure tree's document order.
    Structure,
    /// The order the content stream starts painting each element.
    Paint,
    /// What a screen reader announces, with `--speech-preset`.
    Speech,
}

/// `--emulate` viewers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub(crate) enum ViewerArg {
    /// Google Chrome: `PDFium`.
    Chrome,
    /// Microsoft Edge: Chromium's viewer, `PDFium`.
    Edge,
    /// Adobe Acrobat and Reader: Adobe's AGM.
    Acrobat,
}

impl From<ViewerArg> for Viewer {
    fn from(v: ViewerArg) -> Self {
        match v {
            ViewerArg::Chrome => Self::Chrome,
            ViewerArg::Edge => Self::Edge,
            ViewerArg::Acrobat => Self::Acrobat,
        }
    }
}

/// The lowest `--dpi`: half of Windows' 100 %.
const MIN_DPI: i64 = 48;
/// The highest `--dpi`: 500 % scaling.
const MAX_DPI: i64 = 480;
/// The highest `--zoom` percentage, as viewers allow.
const MAX_ZOOM_PERCENT: u32 = 6400;
/// The longest side `--screen` takes, in device pixels.
const MAX_SCREEN_SIDE: u32 = 16_384;

/// `--screen`: `WIDTHxHEIGHT` in device pixels.
fn parse_screen(text: &str) -> Result<(u32, u32), String> {
    let wrong = || format!("`{text}` is not a screen size: write it as WIDTHxHEIGHT, e.g. 1920x1080");
    let (w, h) = text.split_once(['x', 'X', '×']).ok_or_else(wrong)?;
    let side = |v: &str| {
        v.trim()
            .parse::<u32>()
            .ok()
            .filter(|n| (1..=MAX_SCREEN_SIDE).contains(n))
    };
    match (side(w), side(h)) {
        (Some(w), Some(h)) => Ok((w, h)),
        _ => Err(format!("{}; each side is 1 to {MAX_SCREEN_SIDE} pixels", wrong())),
    }
}

/// `--zoom`: a percentage, `fit-width` or `fit-page`.
fn parse_zoom(text: &str) -> Result<ViewerZoom, String> {
    match text {
        "fit-width" => Ok(ViewerZoom::FitWidth),
        "fit-page" => Ok(ViewerZoom::FitPage),
        _ => text
            .trim_end_matches('%')
            .parse::<u32>()
            .ok()
            .filter(|p| (1..=MAX_ZOOM_PERCENT).contains(p))
            .map(ViewerZoom::Percent)
            .ok_or_else(|| {
                format!("`{text}` is not a zoom: write a percentage from 1 to {MAX_ZOOM_PERCENT}, fit-width or fit-page")
            }),
    }
}

/// The emulated screen and viewer the flags ask for, if any.
fn emulation(args: &RenderArgs) -> Option<Emulation> {
    if args.emulate.is_none() && args.dpi.is_none() && args.screen.is_none() && args.zoom.is_none() {
        return None;
    }
    let defaults = Display::default();
    let (width, height) = args.screen.unwrap_or((defaults.width, defaults.height));
    Some(Emulation {
        viewer: args.emulate.map(Viewer::from),
        display: Display {
            width,
            height,
            dpi: args.dpi.unwrap_or(defaults.dpi),
        },
        zoom: args.zoom.unwrap_or_default(),
        acrobat: AcrobatPrefs {
            enhance_thin_lines: !args.no_enhance_thin_lines,
            line_weights: !args.no_line_weights,
        },
    })
}

/// The `emulation` part of the report: the screen, the zoom it gives this page and the scale
/// that follows.
fn emulation_report(emulation: &Emulation, page: (f32, f32), scale: f32) -> Value {
    let mut report = json!({
        "viewer": emulation.viewer.map(Viewer::name),
        "engine": emulation.viewer.map(|v| v.engine().name()),
        "screen": [emulation.display.width, emulation.display.height],
        "dpi": emulation.display.dpi,
        "scale_factor": emulation.display.scale_factor(),
        "zoom": emulation.zoom.to_string(),
        "zoom_percent": emulation.zoom_for(page) * 100.0,
        "pixels_per_point": scale,
    });
    if emulation.viewer == Some(Viewer::Acrobat) {
        report["acrobat"] = json!({
            "enhance_thin_lines": emulation.acrobat.enhance_thin_lines,
            "line_weights": emulation.acrobat.line_weights,
        });
    }
    report
}

/// `--reference` engines.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub(crate) enum EngineArg {
    /// `PDFium`, loaded at run time (`cargo xtask pdfium fetch`), never linked.
    Pdfium,
}

/// `--mode` values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub(crate) enum ModeArg {
    /// A on the left, B on the right.
    SideBySide,
    /// A and B in two ink colours, multiplied.
    Overlay,
    /// A in grey, the pixels that differ in colour: against a file, ink B adds and ink B
    /// removes apart; against an engine, in red.
    Mask,
}

impl From<ModeArg> for Mode {
    fn from(m: ModeArg) -> Self {
        match m {
            ModeArg::SideBySide => Self::SideBySide,
            ModeArg::Overlay => Self::Overlay,
            ModeArg::Mask => Self::Mask,
        }
    }
}

/// `--order-side` values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub(crate) enum SideArg {
    /// The rendered file.
    A,
    /// The file or engine it is compared with.
    B,
}

const fn mode_name(mode: Mode) -> &'static str {
    match mode {
        Mode::SideBySide => "side-by-side",
        Mode::Overlay => "overlay",
        Mode::Mask => "mask",
    }
}

/// Runs `render`.
pub(crate) fn run(args: &RenderArgs, pretty: bool, out: &mut impl Write) -> anyhow::Result<()> {
    if args.mode.is_some() && args.against.is_none() && args.reference.is_none() {
        return Err(anyhow!("--mode needs --against <file> or --reference <engine>"));
    }
    if (args.no_enhance_thin_lines || args.no_line_weights) && args.emulate != Some(ViewerArg::Acrobat) {
        return Err(anyhow!(
            "--no-enhance-thin-lines and --no-line-weights are Acrobat's preferences: they need --emulate acrobat"
        ));
    }
    if args.tolerance.is_some() && args.reference.is_none() {
        return Err(anyhow!(
            "--tolerance needs --reference <engine>; two files are compared with --threshold"
        ));
    }
    args.visual
        .only_with(args.against.is_some(), "--against <file>")?;
    let doc = open(&args.file)?;
    if doc.page(args.page).is_none() {
        return Err(anyhow!(
            "page {} out of range (document has {})",
            args.page,
            doc.page_count()
        ));
    }
    let report = if let Some(b) = &args.against {
        against(args, &doc, b)?
    } else if let Some(engine) = args.reference {
        reference(args, &doc, engine)?
    } else {
        single(args, &doc)?
    };
    write_json(out, &report, pretty)
}

/// One page with its debug overlays and annotations.
fn single(args: &RenderArgs, doc: &Document) -> anyhow::Result<Value> {
    let page = args.page;
    let emulation = emulation(args);
    let size = page_dimensions(doc, page).with_context(|| format!("rendering page {page}"))?;
    let scale = emulation.map_or(args.scale, |e| e.scale_for(size));
    let to_pixel = page_transform(doc, page, scale).with_context(|| format!("rendering page {page}"))?;
    let mut spec = RenderSpec::new(scale);
    spec.viewer = emulation.and_then(|e| e.viewer);
    if let Some(emulation) = &emulation {
        spec.acrobat = emulation.acrobat;
    }
    spec.overlays = debug_overlays(args, doc, &to_pixel)?;
    let grid = Grid::page(doc, page, scale, scale)?;
    let scenes = [tag_scene(args, doc, page)];
    spec.overlays.extend(annotate::overlays(&annotate::layout(
        &grid,
        &Palette::DESIGN,
        &scenes,
        None,
    )));
    spec.content = content_filter(args, doc)?;
    let rendered = render_page(doc, page, &spec).with_context(|| format!("rendering page {page}"))?;
    write_image(&rendered, &args.output).with_context(|| format!("writing {}", args.output.display()))?;
    let mut report = json!({ "output": args.output, "width": rendered.width, "height": rendered.height });
    if let Some(emulation) = &emulation {
        report["emulation"] = emulation_report(emulation, size, scale);
    }
    Ok(report)
}

/// The page boxes, highlights, x-ray, clip and heat map asked for, placed with `to_pixel`.
fn debug_overlays(args: &RenderArgs, doc: &Document, to_pixel: &Transform) -> anyhow::Result<Vec<Overlay>> {
    let page = args.page;
    let mut overlays = Vec::new();
    if args.boxes
        && let Some(info) = doc.page(page)
    {
        overlays.extend(
            info.defined_boxes()
                .map(|(kind, rect)| Overlay::outline(to_pixel, rect, box_colour(kind), None, 2.0)),
        );
    }
    for id in &args.highlight {
        match doc.bbox(id) {
            Some((p, rect)) if p == page => {
                overlays.push(Overlay::outline(to_pixel, rect, HIGHLIGHT, None, 2.0));
            }
            Some((p, _)) => return Err(anyhow!("`{id}` is on page {p}, not {page}")),
            None => return Err(anyhow!("`{id}` has no geometry")),
        }
    }
    if let Some(by) = args.xray {
        let options = XrayOptions {
            color_by: by.into(),
            include_invisible: args.invisible,
            ..XrayOptions::default()
        };
        overlays.extend(xray_overlays(doc, page, options, to_pixel));
    }
    if let Some(op) = args.clip {
        overlays.extend(clip_overlays(doc, page, op, to_pixel));
    }
    if args.alpha {
        overlays.extend(alpha_heatmap_overlays(doc, page, to_pixel));
    }
    Ok(overlays)
}

fn content_filter(args: &RenderArgs, doc: &Document) -> anyhow::Result<ContentFilter> {
    Ok(if !args.isolate.is_empty() {
        ContentFilter::Only(op_set(doc, &args.isolate)?)
    } else if !args.hide.is_empty() {
        ContentFilter::Hide(op_set(doc, &args.hide)?)
    } else if let Some(n) = args.step {
        ContentFilter::UpTo(n)
    } else {
        ContentFilter::All
    })
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

/// The View menu's annotations of `page` of `doc`, as the canvas shows them.
fn tag_scene(args: &RenderArgs, doc: &Document, page: PageIx) -> Scene {
    let show = TagShow {
        boxes: args.tags,
        labels: args.labels,
        nesting: args.nesting,
        artifacts: args.artifacts,
    };
    let tagged = show.boxes || show.labels;
    let tags = if tagged { doc.tag_boxes(page) } else { Vec::new() };
    let artifacts = if tagged && show.artifacts {
        annotate::artifact_boxes(doc, page)
    } else {
        Vec::new()
    };
    let scopes: HashMap<NodeId, HeaderScope> = if tagged {
        doc.header_scopes(page).into_iter().collect()
    } else {
        HashMap::new()
    };
    let order = order(args, doc, page);
    let scope_of = |id: &NodeId| scopes.get(id);
    annotate::tag_scene(&TagView {
        page,
        tags: &tags,
        artifacts: &artifacts,
        order: order.as_ref(),
        show,
        scope_of: &scope_of,
        in_focus: &|_, _| true,
    })
}

/// The reading order `--reading-order` asks for on `page` of `doc`.
fn order(args: &RenderArgs, doc: &Document, page: PageIx) -> Option<ReadingOrder> {
    Some(match args.reading_order? {
        OrderFrom::Structure => doc.reading_order(page, ReadingOrderSource::Structure),
        OrderFrom::Paint => doc.reading_order(page, ReadingOrderSource::Paint),
        OrderFrom::Speech => doc.speech_reading_order(page, &doc.speech_order(args.speech_preset.into())),
    })
}

/// Only the numbered path of `page` of `doc`: a side of a comparison.
fn order_scene(args: &RenderArgs, doc: &Document, page: PageIx) -> Scene {
    let Some(order) = order(args, doc, page) else {
        return Scene::default();
    };
    annotate::tag_scene(&TagView {
        page,
        tags: &[],
        artifacts: &[],
        order: Some(&order),
        show: TagShow::default(),
        scope_of: &|_| None,
        in_focus: &|_, _| true,
    })
}

/// `render a.pdf --against b.pdf`.
fn against(args: &RenderArgs, doc_a: &Document, file_b: &PathBuf) -> anyhow::Result<Value> {
    if args.artifacts {
        return Err(anyhow!(
            "--artifacts is not drawn in a comparison: artifacts are not paired yet, only the \
             structure elements are"
        ));
    }
    let doc_b = open(file_b)?;
    let pairing = pdfxray_core::diff::pair(doc_a, &doc_b);
    let page_a = args.page;
    let page_b = match args.against_page {
        Some(p) => p,
        None => matched_page(&pairing, page_a).ok_or_else(|| {
            anyhow!(
                "page {page_a} of {} has no corresponding page in {}; pass --against-page",
                args.file.display(),
                file_b.display()
            )
        })?,
    };
    if doc_b.page(page_b).is_none() {
        return Err(anyhow!(
            "page {page_b} out of range ({} has {})",
            file_b.display(),
            doc_b.page_count()
        ));
    }
    let mode = args.mode.map_or(Mode::SideBySide, Mode::from);
    let mut notes: Vec<String> = Vec::new();
    let registration = if args.register && mode.shares_grid() {
        let offset = pairing.offset.map(|o| Registration { dx: o.dx, dy: o.dy });
        if offset.is_none() {
            notes.push("not registered: the pairing reports no global offset".to_owned());
        }
        offset
    } else {
        if args.register {
            notes.push("not registered: side by side, each page keeps its own grid".to_owned());
        }
        None
    };
    let request = Request {
        mode,
        scale: args.scale,
        fit: if args.scale_b_to_a {
            Fit::ScaleBToA
        } else {
            Fit::Refuse
        },
        registration,
        comparator: Comparator::SameEngine(args.visual.perceptual()),
        steps: composite::Steps::NONE,
    };
    let mut picture = composite::composite((doc_a, page_a), (&doc_b, page_b), &request)?;

    let mut warnings = Vec::new();
    for (side, info, page) in [("A", &pairing.sides.a, page_a), ("B", &pairing.sides.b, page_b)] {
        if info.degraded_pages.contains(&page) {
            warnings.push(format!("{side} page {page}: capture degraded"));
        }
    }
    if let Some(r) = picture.registration {
        warnings.push(format!("B registered by {:+.1}, {:+.1} pt", -r.dx, -r.dy));
    }
    if let Some(by) = picture.b_scale {
        warnings.push(format!("B scaled to A by {by:.3}"));
    }
    let show = ChangeShow {
        unchanged: args.tags,
        labels: args.labels,
        nesting: args.nesting,
    };
    let boxes = [boxes_by_id(doc_a, page_a), boxes_by_id(&doc_b, page_b)];
    let tag_of = |side: Side, id: &NodeId| boxes[usize::from(side == Side::B)].get(id);
    let [scene_a, scene_b] =
        annotate::change_scenes(&pairing, (Some(page_a), Some(page_b)), mode, show, &tag_of);
    let (order_a, order_b) = (
        order_scene(args, doc_a, page_a),
        order_scene(args, &doc_b, page_b),
    );
    let mut scenes = vec![scene_a, map_b(&picture, scene_b)];
    match (mode, args.order_side) {
        (Mode::SideBySide, _) => scenes.extend([order_a, map_b(&picture, order_b)]),
        (_, SideArg::A) => scenes.push(order_a),
        (_, SideArg::B) => scenes.push(map_b(&picture, order_b)),
    }
    let visual = picture.delta.as_ref().and_then(Delta::visual).copied();
    let legend = annotate::change_legend(mode, show, visual.as_ref(), &warnings, Palette::DESIGN.warn);
    picture.annotate(args.scale, &Palette::DESIGN, &scenes, &legend);

    let (registration, b_scale) = (picture.registration, picture.b_scale);
    let mut report = written(args, picture)?;
    report["against"] = json!({ "file": file_b, "page": page_b });
    report["registration"] = registration.map_or(
        Value::Null,
        |r| json!({ "dx": r.dx, "dy": r.dy, "pairs": pairing.offset.map(|o| o.pairs) }),
    );
    report["b_scale"] = json!(b_scale);
    report["summary"] = json!(page_summary(&pairing, page_a, page_b));
    report["warnings"] = json!(warnings);
    report["notes"] = json!(notes);
    if visual.is_some() {
        report["visual"]["settings"] = json!(args.visual.settings(args.scale));
    }
    Ok(report)
}

/// The page of B that page `page` of A corresponds to.
fn matched_page(pairing: &Pairing, page: PageIx) -> Option<PageIx> {
    pairing.pages.iter().find(|m| m.a == Some(page)).and_then(|m| m.b)
}

/// The pairs with an element on page `a` of A or page `b` of B, by status.
fn page_summary(pairing: &Pairing, a: PageIx, b: PageIx) -> HashMap<&'static str, usize> {
    let mut counts = HashMap::new();
    for pair in pairing
        .pairs
        .iter()
        .filter(|p| p.page_a == Some(a) || p.page_b == Some(b))
    {
        *counts.entry(annotate::status_name(pair.status)).or_default() += 1;
    }
    counts
}

/// Every element with a box on `page` of `doc`, by id.
fn boxes_by_id(doc: &Document, page: PageIx) -> HashMap<NodeId, TagBox> {
    doc.tag_boxes(page)
        .into_iter()
        .map(|tag| (tag.id.clone(), tag))
        .collect()
}

/// `scene`, a scene of B, in A's page space on `picture`.
fn map_b(picture: &Composite, scene: Scene) -> Scene {
    scene.mapped(|rect| picture.b_in_a(rect))
}

/// Writes `picture` and reports what it is.
fn written(args: &RenderArgs, picture: Composite) -> anyhow::Result<Value> {
    let mut report = json!({
        "output": args.output,
        "width": picture.width,
        "height": picture.height,
        "mode": mode_name(picture.mode),
    });
    match picture.delta {
        Some(Delta::Channel(d)) => {
            report["differing_pixels"] = json!(d.differing_pixels);
            report["differing_fraction"] = json!(d.differing_fraction);
            report["max_channel_delta"] = json!(d.max_channel_delta);
            report["mean_delta"] = json!(d.mean_delta);
            report["tolerance"] = json!(d.tolerance);
        }
        Some(Delta::Visual(v)) => {
            report["differing_pixels"] = json!(v.differing_pixels);
            report["differing_fraction"] = json!(v.ratio());
            report["visual"] = json!({ "score": ScoreView::from(&v) });
        }
        None => {}
    }
    write_image(&picture.into_page(), &args.output)
        .with_context(|| format!("writing {}", args.output.display()))?;
    Ok(report)
}

/// The ink colours of a cross-engine overlay.
fn engine_legend(mode: Mode) -> Legend {
    let rows = if mode == Mode::Overlay {
        vec![
            (composite::TINT_A, "ink of hayro".to_owned()),
            (composite::TINT_B, "ink of PDFium".to_owned()),
        ]
    } else {
        Vec::new()
    };
    Legend { rows }
}

/// `render a.pdf --reference pdfium`.
fn reference(args: &RenderArgs, doc: &Document, engine: EngineArg) -> anyhow::Result<Value> {
    let EngineArg::Pdfium = engine;
    let page = args.page;
    let pdfium = PdfiumRenderer::load(args.pdfium.as_deref())
        .context("loading PDFium; run `cargo xtask pdfium fetch` or pass --pdfium <path>")?;
    let ours = render_page(doc, page, &RenderSpec::new(args.scale))
        .with_context(|| format!("rendering page {page} with hayro"))?;
    // PDFium draws on hayro's geometry, so the two bitmaps share a grid.
    let theirs = pdfium
        .render_page(doc, page, args.scale)
        .with_context(|| format!("rendering page {page} with PDFium"))?;
    let mode = args.mode.map_or(Mode::Mask, Mode::from);
    let comparator = Comparator::CrossEngine {
        tolerance: args.tolerance.unwrap_or(composite::CROSS_ENGINE_TOLERANCE),
    };
    let mut picture = composite::compose(mode, &ours, &theirs, None, None, &comparator);
    let scene = tag_scene(args, doc, page);
    let mut scenes = vec![scene.clone()];
    if mode == Mode::SideBySide {
        scenes.push(map_b(&picture, scene));
    }
    picture.annotate(args.scale, &Palette::DESIGN, &scenes, &engine_legend(mode));
    let mut report = written(args, picture)?;
    report["reference"] = json!("pdfium");
    report["pdfium_version"] = json!(pdfium.version());
    Ok(report)
}
