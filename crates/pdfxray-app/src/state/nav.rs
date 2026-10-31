//! The activity rail and the navigator it drives.

use super::drawer::DrawerTab;

/// An entry of the activity rail. Each one decides what the navigator lists, what the centre
/// shows and which drawer tab opens.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum RailItem {
    /// The structure tree.
    #[default]
    Tags,
    /// The content streams of the page on the canvas.
    Content,
    /// The content tree filtered to artifacts.
    Artifacts,
    /// Pages and their boxes.
    Pages,
    /// Raw objects.
    Objects,
    /// The revisions of the file.
    Revisions,
    /// veraPDF validation.
    Validation,
}

impl RailItem {
    /// The entries at the top of the rail, in order.
    pub const TOP: [Self; 6] = [
        Self::Tags,
        Self::Content,
        Self::Artifacts,
        Self::Pages,
        Self::Objects,
        Self::Revisions,
    ];

    /// Name of the entry: its tooltip and accessible name.
    pub const fn title(self) -> &'static str {
        match self {
            Self::Tags => "Tags",
            Self::Content => "Content",
            Self::Artifacts => "Artifacts",
            Self::Pages => "Pages",
            Self::Objects => "Objects",
            Self::Revisions => "Revisions",
            Self::Validation => "Validation",
        }
    }

    /// Title of the navigator header.
    pub const fn heading(self) -> &'static str {
        match self {
            Self::Tags => "Structure",
            other => other.title(),
        }
    }

    /// Placeholder of the navigator filter.
    pub const fn filter_hint(self) -> &'static str {
        match self {
            Self::Tags => "Filter by role, obj, MCID, /Alt…",
            Self::Content | Self::Artifacts => "Filter by operator, font, text…",
            Self::Pages => "Jump to page, size, rotation…",
            Self::Objects => "Filter by obj num, /Type, key…",
            Self::Revisions => "Filter revisions…",
            Self::Validation => "Filter by rule…",
        }
    }

    /// The drawer tab that opens with this entry; `None` leaves the drawer as it is.
    pub const fn drawer_default(self) -> Option<DrawerTab> {
        match self {
            Self::Tags => Some(DrawerTab::HitStack),
            Self::Content | Self::Artifacts => Some(DrawerTab::Operators),
            Self::Pages => Some(DrawerTab::Diagnostics),
            Self::Validation => Some(DrawerTab::Validation),
            Self::Objects => Some(DrawerTab::References),
            Self::Revisions => None,
        }
    }

    /// True when the drawer starts open: its tab is the main content of the entry (the operators
    /// of Content, the references of Objects, the findings of Validation). Elsewhere it rests as
    /// a strip until the page is clicked.
    pub const fn drawer_opens(self) -> bool {
        matches!(self, Self::Content | Self::Objects | Self::Validation)
    }

    /// True when the centre shows the object view instead of the canvas.
    pub const fn replaces_canvas(self) -> bool {
        matches!(self, Self::Objects)
    }
}

/// Navigator state.
#[derive(Debug, Default)]
pub struct NavState {
    /// The active rail entry.
    pub rail: RailItem,
    /// Text of the navigator filter.
    pub filter: String,
    /// The filter should take the keyboard focus the next time it is drawn (⌘F).
    pub focus_filter: bool,
    /// The filter text of the Pages entry the canvas last jumped for, so a filter jumps once.
    pub jumped_for: String,
}

impl NavState {
    /// Switches to `rail`; the filter starts empty on every entry.
    pub fn set_rail(&mut self, rail: RailItem) {
        if self.rail != rail {
            self.rail = rail;
            self.filter.clear();
            self.jumped_for.clear();
        }
    }
}
