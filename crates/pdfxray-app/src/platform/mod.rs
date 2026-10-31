//! What the operating system sends the window from outside egui: files to open (macOS "Open
//! With…", the Dock) and menu bar commands. They arrive on the main thread between frames, are
//! queued in the [`Inbox`], wake the UI, and are taken in at the start of the next frame.
//!
//! [`macos`] builds the native menu bar and connects the open handler of `pdfxray-macos`;
//! [`updater`] decides which of the window processes runs the updater.

#[cfg(target_os = "macos")]
pub mod macos;
#[cfg(any(target_os = "macos", test))]
pub mod updater;

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock, PoisonError};

use crate::commands::MenuCommand;

/// The queue between the operating system and the frame loop.
///
/// Anything that arrives before the app has its egui context (files opened at launch come
/// before the first frame) waits in the queue; [`Inbox::attach`] wakes the UI to take it in.
#[derive(Debug, Default)]
pub struct Inbox {
    paths: Mutex<VecDeque<PathBuf>>,
    commands: Mutex<VecDeque<MenuCommand>>,
    ctx: OnceLock<egui::Context>,
}

impl Inbox {
    /// Queues files to open and wakes the UI.
    pub fn push_paths(&self, paths: impl IntoIterator<Item = PathBuf>) {
        self.paths
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .extend(paths);
        self.wake();
    }

    /// Queues a menu command and wakes the UI.
    pub fn push_command(&self, command: MenuCommand) {
        self.commands
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push_back(command);
        self.wake();
    }

    /// Gives the inbox the context to wake, and wakes it for what is already queued.
    pub fn attach(&self, ctx: &egui::Context) {
        let _first = self.ctx.set(ctx.clone());
        self.wake();
    }

    /// The files queued since the last call, oldest first.
    pub fn take_paths(&self) -> Vec<PathBuf> {
        self.paths
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .drain(..)
            .collect()
    }

    /// The commands queued since the last call, oldest first.
    pub fn take_commands(&self) -> Vec<MenuCommand> {
        self.commands
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .drain(..)
            .collect()
    }

    fn wake(&self) {
        if let Some(ctx) = self.ctx.get() {
            ctx.request_repaint();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_inbox_keeps_order_and_empties_when_taken() {
        let inbox = Inbox::default();
        // before the app has a context, things wait
        inbox.push_paths([PathBuf::from("/a.pdf"), PathBuf::from("/b.pdf")]);
        inbox.push_command(MenuCommand::ZoomIn);
        inbox.push_paths([PathBuf::from("/c.pdf")]);
        inbox.push_command(MenuCommand::FitPage);

        assert_eq!(
            inbox.take_paths(),
            [
                PathBuf::from("/a.pdf"),
                PathBuf::from("/b.pdf"),
                PathBuf::from("/c.pdf")
            ]
        );
        assert_eq!(inbox.take_commands(), [MenuCommand::ZoomIn, MenuCommand::FitPage]);
        // smoke: taken means gone
        assert_eq!(inbox.take_paths(), Vec::<PathBuf>::new());
    }

    #[test]
    fn the_ui_is_woken_once_attached() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicUsize, Ordering};

        let inbox = Inbox::default();
        let ctx = egui::Context::default();
        let wakes = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&wakes);
        ctx.set_request_repaint_callback(move |_| {
            counter.fetch_add(1, Ordering::SeqCst);
        });

        // Nothing to wake before the app has its context; the path waits.
        inbox.push_paths([PathBuf::from("/a.pdf")]);
        assert_eq!(wakes.load(Ordering::SeqCst), 0);
        inbox.attach(&ctx);
        assert_eq!(wakes.load(Ordering::SeqCst), 1);
        inbox.push_command(MenuCommand::Open);
        // (egui folds wake-ups between two frames into one)
        assert!(ctx.has_requested_repaint());
        // smoke: the waiting path is still there for the first frame
        assert_eq!(inbox.take_paths(), [PathBuf::from("/a.pdf")]);
    }
}
