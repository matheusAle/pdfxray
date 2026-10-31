//! The states of the window that are not the workspace: nothing open, a file opening, an
//! encrypted file and a damaged one. They are states of the one window, not pages: the title bar,
//! rail and status bar stay, and [`Screen`] says what the centre shows.

pub mod broken;
pub mod empty;
pub mod encrypted;
pub mod opening;

use egui::{Color32, Frame, Margin, Stroke};

use crate::state::AppState;
use crate::theme::tokens;

/// What the window shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    /// No file: the drop zone and the recent files.
    Empty,
    /// A file is opening and has taken long enough to show its progress.
    Opening,
    /// An encrypted file's explainer, until a rail entry is picked.
    Encrypted,
    /// Navigator, centre, inspector and drawer.
    Workspace,
}

impl Screen {
    /// True when the rail entries work: there is a document to look into.
    pub const fn has_workspace(self) -> bool {
        matches!(self, Self::Encrypted | Self::Workspace)
    }
}

impl AppState {
    /// What the window shows now.
    pub fn screen(&self) -> Screen {
        if self.opening.card_visible() {
            Screen::Opening
        } else if let Some(loaded) = &self.document {
            if self.explainer && loaded.encryption.is_some() {
                Screen::Encrypted
            } else {
                Screen::Workspace
            }
        } else {
            Screen::Empty
        }
    }
}

/// The frame of the cards of the empty, opening and encrypted states.
fn card(fill: Color32, margin: Margin) -> Frame {
    Frame::new()
        .fill(fill)
        .stroke(Stroke::new(1.0, tokens::BORDER_STRONG))
        .corner_radius(tokens::RADIUS_LG * 2.0)
        .inner_margin(margin)
}
