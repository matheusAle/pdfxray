//! The window-level commands of the menu bar, and what each one does.
//!
//! Every command has a stable id ([`MenuCommand::id`]), which the native menu bar (macOS) gives
//! its item; a click comes back as that id and [`MenuCommand::from_id`] turns it into the command.
//! [`MenuCommand::apply`] is the one place that carries a command out, for the menu bar and for
//! the keys the window handles itself where there is no menu bar.

use std::path::{Path, PathBuf};

use egui::{Context, OpenUrl, ViewportCommand};

use crate::state::AppState;

/// A command of the menu bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuCommand {
    /// File › New Window (⌘N): an empty window.
    NewWindow,
    /// File › Open… (⌘O): the file dialog.
    Open,
    /// File › Open Recent › the recent file at this index of [`crate::Recents::entries`].
    OpenRecent(usize),
    /// File › Open Recent › Clear Menu.
    ClearRecent,
    /// File › Close Window (⌘W): closes this window, which ends its process.
    CloseWindow,
    /// File › Validate (⇧⌘V): the title bar's Validate.
    Validate,
    /// View › Zoom In (⌘=).
    ZoomIn,
    /// View › Zoom Out (⌘−).
    ZoomOut,
    /// View › Actual Size (⌘0): 100 %.
    ActualSize,
    /// View › Fit Page (⌘9).
    FitPage,
    /// Help › pdfxray User Guide.
    UserGuide,
}

/// Prefix of the ids of the recent files.
const RECENT_PREFIX: &str = "file.recent.";

impl MenuCommand {
    /// Every command except the recent files, in menu order.
    pub const FIXED: [Self; 10] = [
        Self::NewWindow,
        Self::Open,
        Self::ClearRecent,
        Self::CloseWindow,
        Self::Validate,
        Self::ZoomIn,
        Self::ZoomOut,
        Self::ActualSize,
        Self::FitPage,
        Self::UserGuide,
    ];

    /// The id of the menu item.
    pub fn id(self) -> String {
        let fixed = match self {
            Self::NewWindow => "file.new-window",
            Self::Open => "file.open",
            Self::OpenRecent(index) => return format!("{RECENT_PREFIX}{index}"),
            Self::ClearRecent => "file.recent-clear",
            Self::CloseWindow => "file.close-window",
            Self::Validate => "file.validate",
            Self::ZoomIn => "view.zoom-in",
            Self::ZoomOut => "view.zoom-out",
            Self::ActualSize => "view.actual-size",
            Self::FitPage => "view.fit-page",
            Self::UserGuide => "help.user-guide",
        };
        fixed.to_owned()
    }

    /// The command of the menu item with this id; `None` for an id that is not a command (the
    /// predefined items, such as Quit, act by themselves).
    pub fn from_id(id: &str) -> Option<Self> {
        if let Some(index) = id.strip_prefix(RECENT_PREFIX) {
            return index.parse().ok().map(Self::OpenRecent);
        }
        Self::FIXED.into_iter().find(|command| command.id() == id)
    }

    /// True for the commands that act on the open document; they do nothing without one.
    pub const fn needs_document(self) -> bool {
        matches!(
            self,
            Self::Validate | Self::ZoomIn | Self::ZoomOut | Self::ActualSize | Self::FitPage
        )
    }

    /// Carries the command out on `state`; `ctx` takes the window commands (close) and links.
    pub fn apply(self, state: &mut AppState, ctx: &Context) {
        if self.needs_document() && !state.screen().has_workspace() {
            return;
        }
        match self {
            Self::NewWindow => state.new_window(),
            Self::Open => state.request_open(),
            Self::OpenRecent(index) => open_recent(state, index),
            Self::ClearRecent => {
                state.recents_error = state.recents.clear().err();
            }
            Self::CloseWindow => ctx.send_viewport_cmd(ViewportCommand::Close),
            Self::Validate => state.request_validation(ctx),
            Self::ZoomIn => state.zoom_in(),
            Self::ZoomOut => state.zoom_out(),
            Self::ActualSize => state.set_zoom(1.0),
            Self::FitPage => state.fit_page(),
            Self::UserGuide => ctx.open_url(OpenUrl::new_tab(user_guide_url(
                running_from_bundle(),
                Some(&local_user_guide()),
            ))),
        }
    }
}

/// Opens the recent file at `index`, in this window or a new one ([`AppState::open_files`]).
fn open_recent(state: &mut AppState, index: usize) {
    let Some(recent) = state.recents.entries().get(index) else {
        return;
    };
    if recent.path.exists() {
        let path = recent.path.clone();
        state.open_files([path]);
    } else {
        state.status = format!(
            "Cannot open {}: it is no longer at {}",
            recent.name,
            recent.path.display()
        );
    }
}

/// The user guide in the source checkout this binary was built from.
fn local_user_guide() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/user-guide.md")
}

/// True when the app runs from a macOS app bundle (`….app/Contents/MacOS/`).
fn running_from_bundle() -> bool {
    std::env::current_exe().is_ok_and(|exe| exe.to_string_lossy().contains(".app/Contents/MacOS/"))
}

/// Where Help › pdfxray User Guide goes: the guide of the checkout the app was built from when it
/// runs unbundled and that file is there (a developer build), else the guide in the repository.
pub fn user_guide_url(bundled: bool, local: Option<&Path>) -> String {
    // A local guide that cannot be resolved is not there: the repository's guide is the answer.
    let local = local
        .filter(|_| !bundled)
        .and_then(|path| std::fs::canonicalize(path).ok());
    match local {
        Some(path) => format!("file://{}", path.display()),
        None => format!("{}/blob/main/docs/user-guide.md", crate::brand::REPOSITORY),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_round_trip_and_unknown_ids_are_not_commands() {
        for command in MenuCommand::FIXED {
            assert_eq!(MenuCommand::from_id(&command.id()), Some(command), "{command:?}");
        }
        assert_eq!(
            MenuCommand::from_id("file.recent.3"),
            Some(MenuCommand::OpenRecent(3))
        );
        assert_eq!(MenuCommand::OpenRecent(7).id(), "file.recent.7");
        assert_eq!(MenuCommand::from_id("file.recent.x"), None);
        assert_eq!(MenuCommand::from_id("quit"), None);
        // smoke: every id is distinct
        let mut ids: Vec<String> = MenuCommand::FIXED.iter().map(|c| c.id()).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), MenuCommand::FIXED.len());
    }

    #[test]
    fn the_user_guide_is_local_only_for_an_unbundled_build_with_the_checkout() {
        let here = Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");
        let local = user_guide_url(false, Some(&here));
        assert!(local.starts_with("file:///"), "{local}");
        assert!(local.ends_with("pdfxray-app/Cargo.toml"), "{local}");

        let remote = format!("{}/blob/main/docs/user-guide.md", crate::brand::REPOSITORY);
        assert_eq!(user_guide_url(true, Some(&here)), remote);
        assert_eq!(
            user_guide_url(false, Some(Path::new("/no/such/guide.md"))),
            remote
        );
        // smoke: the checkout this test runs in has the guide
        assert!(local_user_guide().is_file());
    }
}
