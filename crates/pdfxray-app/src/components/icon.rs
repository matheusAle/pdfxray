//! The icon set: [Lucide](https://lucide.dev) glyphs under our own names.
//!
//! Icons are SVG documents rasterised by egui's image loader at the size and device scale they
//! are drawn at, so they stay crisp at 14 to 16 px. [`crate::theme::install`] registers the
//! loaders.

use egui::{Color32, Image, Rect, Response, Sense, Ui, Vec2, vec2};
use egui_lucide::Lucide;

/// Stroke width in viewBox units; Lucide's default of 2 is heavy at 14 px.
const STROKE: f32 = 1.75;

/// Default side of an icon, in points.
pub const SIZE: f32 = 16.0;

/// An icon, named for what it means in pdfxray.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Icon {
    /// Structure tree (rail).
    Tags,
    /// Content streams (rail).
    Content,
    /// Artifacts (rail).
    Artifacts,
    /// Pages and boxes (rail).
    Pages,
    /// Raw objects (rail).
    Objects,
    /// Revisions (rail).
    Revisions,
    /// Revision history (rev chip).
    History,
    /// The open document (title bar).
    Document,
    /// Validation (rail).
    Validation,
    /// Diagnostics (rail).
    Warning,
    /// Settings (rail).
    Settings,
    /// X-ray tool.
    Xray,
    /// Isolate tool.
    Isolate,
    /// Hide tool.
    Hide,
    /// Paint-order tool.
    PaintOrder,
    /// Clip tool.
    Clip,
    /// Alpha heat map tool.
    Alpha,
    /// Measure tool.
    Measure,
    /// Page boxes overlay.
    Boxes,
    /// Zoom in.
    ZoomIn,
    /// Zoom out.
    ZoomOut,
    /// Fit the page.
    Fit,
    /// Copy.
    Copy,
    /// Copied.
    Check,
    /// Collapsed tree row, breadcrumb separator, next page.
    ChevronRight,
    /// Expanded tree row, open section.
    ChevronDown,
    /// Previous page.
    ChevronLeft,
    /// A document.
    File,
    /// Open a file.
    Open,
    /// The sample document.
    Flask,
    /// Locked, e.g. an encrypted file.
    Lock,
    /// The pointer position.
    Pointer,
    /// How long opening took.
    Timer,
    /// Filter field.
    Search,
    /// Dismiss.
    Close,
    /// Collapse all rows.
    CollapseAll,
    /// Expand all rows.
    ExpandAll,
    /// Step back in the paint order.
    StepBack,
    /// Step forward in the paint order.
    StepForward,
    /// Raw object button.
    Braces,
    /// Raw bytes and stream bytes.
    Bytes,
    /// A font resource.
    Font,
    /// An image resource.
    Image,
    /// A colour space resource.
    Palette,
    /// A pattern or shading resource.
    Shapes,
    /// A property list resource.
    Hash,
    /// A hint that explains something.
    Help,
    /// Keeps the inspector on its node.
    Pin,
    /// A visible overlay, e.g. a page box outline (the hidden one is [`Icon::Hide`]).
    Eye,
    /// An informational note, e.g. a diagnostic that is not a problem.
    Info,
    /// A reference into the content stream: the marker of a content reference row (`↳`).
    CornerDownRight,
}

impl Icon {
    /// Every icon, for galleries and tests.
    pub const ALL: [Self; 51] = [
        Self::Tags,
        Self::Content,
        Self::Artifacts,
        Self::Pages,
        Self::Objects,
        Self::Revisions,
        Self::History,
        Self::Document,
        Self::Validation,
        Self::Warning,
        Self::Settings,
        Self::Xray,
        Self::Isolate,
        Self::Hide,
        Self::PaintOrder,
        Self::Clip,
        Self::Alpha,
        Self::Measure,
        Self::Boxes,
        Self::ZoomIn,
        Self::ZoomOut,
        Self::Fit,
        Self::Copy,
        Self::Check,
        Self::ChevronRight,
        Self::ChevronDown,
        Self::ChevronLeft,
        Self::File,
        Self::Open,
        Self::Flask,
        Self::Lock,
        Self::Pointer,
        Self::Timer,
        Self::Search,
        Self::Close,
        Self::CollapseAll,
        Self::ExpandAll,
        Self::StepBack,
        Self::StepForward,
        Self::Braces,
        Self::Bytes,
        Self::Font,
        Self::Image,
        Self::Palette,
        Self::Shapes,
        Self::Hash,
        Self::Help,
        Self::Pin,
        Self::Eye,
        Self::Info,
        Self::CornerDownRight,
    ];

    const fn lucide(self) -> Lucide {
        match self {
            Self::Tags => Lucide::Network,
            Self::Content => Lucide::Layers,
            Self::Artifacts => Lucide::Ghost,
            Self::Pages => Lucide::Boxes,
            Self::Objects | Self::Braces => Lucide::Braces,
            Self::Revisions => Lucide::FileClock,
            Self::History => Lucide::RotateCcwClock,
            Self::Document => Lucide::FileScan,
            Self::Validation => Lucide::ShieldCheck,
            Self::Warning => Lucide::TriangleAlert,
            Self::Settings => Lucide::SlidersHorizontal,
            Self::Xray => Lucide::Scan,
            Self::Isolate => Lucide::Focus,
            Self::Hide => Lucide::EyeOff,
            Self::PaintOrder => Lucide::ListOrdered,
            Self::Clip => Lucide::Crop,
            Self::Alpha => Lucide::Blend,
            Self::Measure => Lucide::Ruler,
            Self::Boxes => Lucide::Frame,
            Self::ZoomIn => Lucide::ZoomIn,
            Self::ZoomOut => Lucide::ZoomOut,
            Self::Fit => Lucide::Maximize,
            Self::Copy => Lucide::Copy,
            Self::Check => Lucide::Check,
            Self::ChevronRight => Lucide::ChevronRight,
            Self::ChevronDown => Lucide::ChevronDown,
            Self::ChevronLeft => Lucide::ChevronLeft,
            Self::File => Lucide::FileText,
            Self::Open => Lucide::FolderOpen,
            Self::Flask => Lucide::FlaskConical,
            Self::Lock => Lucide::Lock,
            Self::Pointer => Lucide::MousePointer2,
            Self::Timer => Lucide::Timer,
            Self::Search => Lucide::Search,
            Self::Close => Lucide::X,
            Self::CollapseAll => Lucide::ChevronsDownUp,
            Self::ExpandAll => Lucide::ChevronsUpDown,
            Self::StepBack => Lucide::SkipBack,
            Self::StepForward => Lucide::SkipForward,
            Self::Bytes => Lucide::FileCode,
            Self::Font => Lucide::Type,
            Self::Image => Lucide::Image,
            Self::Palette => Lucide::Palette,
            Self::Shapes => Lucide::Shapes,
            Self::Hash => Lucide::Hash,
            Self::Help => Lucide::CircleQuestionMark,
            Self::Pin => Lucide::Pin,
            Self::Eye => Lucide::Eye,
            Self::Info => Lucide::Info,
            Self::CornerDownRight => Lucide::CornerDownRight,
        }
    }

    /// The image of this icon at `size` points in `color`.
    pub fn image(self, size: f32, color: Color32) -> Image<'static> {
        self.lucide().size(size).color(color).stroke_width(STROKE).image()
    }

    /// Paints the icon centred in `rect`, at `size` points.
    pub fn paint(self, ui: &Ui, rect: Rect, size: f32, color: Color32) {
        let target = Rect::from_center_size(rect.center(), Vec2::splat(size));
        self.image(size, color).paint_at(ui, target);
    }

    /// Lays out the icon as a plain, non-interactive widget.
    pub fn show(self, ui: &mut Ui, size: f32, color: Color32) -> Response {
        let (rect, response) = ui.allocate_exact_size(vec2(size, size), Sense::hover());
        self.paint(ui, rect, size, color);
        response
    }
}
