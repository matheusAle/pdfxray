//! One window per process, and where a file that is asked for opens.
//!
//! pdfxray shows one document per window and runs one window per process: a new window is a new
//! process of the same executable ([`native_spawner`]), started with the file to open as its
//! argument. Which window a file goes to:
//!
//! * Files the system hands over (Finder's "Open With…", a double-click, a drop on the Dock icon,
//!   `open -a pdfxray`), a recent file from the menu bar and the file chosen with Open… (⌘O)
//!   open in this window while it is vacant (no document and none opening), else each in a
//!   new window. Of several at once, the first may take this window and the rest get one each.
//! * A file this window already shows, or is opening, is not opened again.
//! * A file dropped onto the window replaces the document of this window
//!   ([`AppState::accept_drop`]): the drop says which window it is meant for.
//! * New Window (⌘N) starts an empty window.
//!
//! Starting a process is injected ([`AppState::spawner`]), like the file dialog, so tests check
//! the routing without starting anything.

use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::open::Source;
use crate::state::AppState;
use crate::states::Screen;

/// Starts a new window: a new process of the app, opening the file when there is one.
///
/// [`native_spawner`] is the real one; tests inject a closure that records what it was asked.
pub type Spawner = Box<dyn FnMut(Option<&Path>) -> io::Result<()>>;

/// Starts `pdfxray-app [file]` from the running executable.
pub fn native_spawner() -> Spawner {
    Box::new(|path| {
        let mut command = Command::new(std::env::current_exe()?);
        if let Some(path) = path {
            command.arg(path);
        }
        let mut child = command.spawn()?;
        // Wait for it on the side, so a window that is closed first leaves no zombie process.
        let _reaper = std::thread::Builder::new()
            .name("pdfxray-window".to_owned())
            .spawn(move || child.wait());
        Ok(())
    })
}

/// The title of the window: `file.pdf — pdfxray` with a file, `pdfxray` without. macOS shows it
/// in Mission Control, the Window menu and the Dock; other systems in the title bar and taskbar.
pub fn window_title(state: &AppState) -> String {
    let name = match state.screen() {
        Screen::Empty => None,
        Screen::Opening => state.opening_name(),
        Screen::Encrypted | Screen::Workspace => state.document.as_ref().map(|d| d.info.name.clone()),
    };
    match name {
        Some(name) => format!("{name} \u{2014} {}", crate::APP_TITLE),
        None => crate::APP_TITLE.to_owned(),
    }
}

/// Whether `a` and `b` name the same file, following links where both exist.
fn same_file(a: &Path, b: &Path) -> bool {
    let resolve = |path: &Path| {
        std::fs::canonicalize(path)
            .or_else(|_| std::path::absolute(path))
            .unwrap_or_else(|_| path.to_path_buf())
    };
    a == b || resolve(a) == resolve(b)
}

fn file_name(path: &Path) -> String {
    Source::Path(path.to_path_buf()).name()
}

impl AppState {
    /// True when the window has no document and is not opening one: a file asked for opens here.
    pub fn is_vacant(&self) -> bool {
        self.document.is_none() && self.opening.job().is_none()
    }

    /// Opens files the user asked for outside this window's drop zone (see the [module
    /// docs](self)): the first in this window if it is vacant, every other one in a new window.
    /// Files this window shows or is opening are skipped.
    pub fn open_files(&mut self, paths: impl IntoIterator<Item = PathBuf>) {
        let mut wanted: Vec<PathBuf> = Vec::new();
        for path in paths {
            if self.is_opening(&path) || wanted.iter().any(|seen| same_file(seen, &path)) {
                continue;
            }
            if self.is_showing(&path) {
                self.status = format!("{} is already open in this window", file_name(&path));
                continue;
            }
            wanted.push(path);
        }
        let mut wanted = wanted.into_iter();
        if self.is_vacant()
            && let Some(first) = wanted.next()
        {
            self.open_path(&first);
        }
        for path in wanted {
            self.spawn_window(Some(&path));
        }
    }

    /// Handler of "Open…" (the drop zone's button, ⌘O, File › Open…): asks the picker, then opens
    /// the choice as [`AppState::open_files`] does.
    pub fn request_open(&mut self) {
        if let Some(path) = (self.picker)() {
            self.open_files([path]);
        }
    }

    /// New Window (⌘N): starts an empty window.
    pub fn new_window(&mut self) {
        self.spawn_window(None);
    }

    fn is_showing(&self, path: &Path) -> bool {
        self.document
            .as_ref()
            .is_some_and(|loaded| same_file(&loaded.path, path))
    }

    fn is_opening(&self, path: &Path) -> bool {
        self.opening
            .job()
            .is_some_and(|job| matches!(job.source(), Source::Path(opening) if same_file(opening, path)))
    }

    /// Starts a window, reporting a failure in the status bar.
    fn spawn_window(&mut self, path: Option<&Path>) {
        if let Err(error) = (self.spawner)(path) {
            self.status = match path {
                Some(path) => format!("Cannot open {} in a new window: {error}", file_name(path)),
                None => format!("Cannot open a new window: {error}"),
            };
        }
    }
}
