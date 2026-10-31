//! pdfxray's macOS integration: the files macOS asks the app to open, the recent documents, and
//! the updater.
//!
//! * [`install_open_handler`] receives the files of "Open With…" in Finder, a double-click on a
//!   PDF the app is the default for, a PDF dropped on the Dock icon and `open -a pdfxray file.pdf`.
//!   macOS sends them as an Apple Event that `AppKit` hands to the application delegate's
//!   `application:openURLs:`; the delegate winit installs does not implement it, so this adds it.
//! * [`note_recent_document`] adds a file to the system's recent documents, which the Dock icon's
//!   menu and Apple menu › Recent Items list.
//! * [`start_updater`] and [`check_for_updates`] run Sparkle, the updater, when the release app
//!   carries `Contents/Frameworks/Sparkle.framework` ([`updater_bundled`]). The framework is loaded
//!   at run time, never linked, so builds from source have no updater and need no Sparkle.
//!
//! On other systems all of them are no-ops with the same signatures, so callers need no `cfg`.
//!
//! # Why this crate may use `unsafe`
//!
//! The rest of the workspace denies `unsafe_code`. Talking to `AppKit` needs a little of it:
//! adding a method to the Objective-C class of winit's application delegate (an Objective-C
//! runtime call, and the cast of a Rust function to a method implementation). That code lives
//! here and nowhere else, so the unsafe surface of pdfxray is two files (`src/macos.rs`, and
//! `src/sparkle.rs`, which loads Sparkle and sends its controller two messages), and every block
//! in them carries a `// SAFETY:` comment (enforced by the lints below).

// The only crate of the workspace that may use `unsafe`; see the crate documentation.
#![allow(unsafe_code)]
#![deny(clippy::undocumented_unsafe_blocks, clippy::missing_safety_doc)]

use std::path::{Path, PathBuf};

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
mod sparkle;

/// Has `on_open` called with the files macOS asks the app to open, from now on.
///
/// Call it on the main thread, after the winit event loop is created (which installs winit's
/// application delegate) and before it runs: macOS delivers the files a launch was asked to open
/// while the loop starts, and a handler added later misses them. A second call replaces the
/// first handler.
///
/// Returns `false` when nothing was installed: on another system, off the main thread, without
/// an application delegate, or when the delegate already answers `application:openURLs:` itself.
pub fn install_open_handler(on_open: impl Fn(Vec<PathBuf>) + 'static) -> bool {
    #[cfg(target_os = "macos")]
    {
        macos::install_open_handler(Box::new(on_open))
    }
    #[cfg(not(target_os = "macos"))]
    {
        drop(on_open);
        false
    }
}

/// Adds `path` to the recent documents of the system (the Dock icon's menu, Apple menu › Recent
/// Items). Takes effect in the app bundle, whose `Info.plist` declares the PDF document type.
///
/// Does nothing on another system, off the main thread, or for a path that cannot be a file URL.
pub fn note_recent_document(path: &Path) {
    #[cfg(target_os = "macos")]
    macos::note_recent_document(path);
    #[cfg(not(target_os = "macos"))]
    let _ = path;
}

/// Why [`start_updater`] did not start the updater.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdaterError {
    /// The running app has no `Contents/Frameworks/Sparkle.framework` (a build from source, or
    /// another system). The normal case outside a release; not worth reporting.
    NotBundled,
    /// Called off the main thread.
    NotMainThread,
    /// The framework is there but could not be loaded (for example refused by library
    /// validation); the text is macOS's reason.
    Load(String),
    /// The framework loaded but is not a Sparkle 2 that has `SPUStandardUpdaterController`.
    Incompatible,
}

impl std::fmt::Display for UpdaterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotBundled => f.write_str("the app does not include Sparkle.framework"),
            Self::NotMainThread => f.write_str("the updater can only start on the main thread"),
            Self::Load(reason) => write!(f, "cannot load Sparkle.framework: {reason}"),
            Self::Incompatible => f.write_str("Sparkle.framework has no usable SPUStandardUpdaterController"),
        }
    }
}

impl std::error::Error for UpdaterError {}

/// Whether the running app carries Sparkle (`Contents/Frameworks/Sparkle.framework`), so that
/// [`start_updater`] can work. False on other systems and for builds from source.
pub fn updater_bundled() -> bool {
    #[cfg(target_os = "macos")]
    {
        sparkle::bundled_framework().is_some()
    }
    #[cfg(not(target_os = "macos"))]
    {
        false
    }
}

/// Loads the bundled Sparkle and starts its updater: automatic checks as `Info.plist` configures
/// them, and the standard update dialogs. Starting it again is a no-op.
///
/// Call on the main thread, in one process only: every window of pdfxray is a process, and each
/// running updater would check and ask on its own.
pub fn start_updater() -> Result<(), UpdaterError> {
    #[cfg(target_os = "macos")]
    {
        sparkle::start()
    }
    #[cfg(not(target_os = "macos"))]
    {
        Err(UpdaterError::NotBundled)
    }
}

/// Checks for updates now and shows the result, as Sparkle's "Check for Updates…" does. Returns
/// `false`, doing nothing, when the updater is not running in this process or off the main thread.
pub fn check_for_updates() -> bool {
    #[cfg(target_os = "macos")]
    {
        sparkle::check_for_updates()
    }
    #[cfg(not(target_os = "macos"))]
    {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn without_a_bundled_framework_there_is_no_updater() {
        // `cargo test` binaries are not app bundles.
        assert!(!updater_bundled());
        let off_main = std::thread::spawn(start_updater).join().expect("thread ends");
        let expected = if cfg!(target_os = "macos") {
            UpdaterError::NotMainThread
        } else {
            UpdaterError::NotBundled
        };
        assert_eq!(off_main, Err(expected));
        assert!(!check_for_updates());
        // smoke: the errors explain themselves
        assert_eq!(
            UpdaterError::Load("code signature not valid".into()).to_string(),
            "cannot load Sparkle.framework: code signature not valid"
        );
    }

    #[test]
    fn off_the_main_thread_nothing_is_installed() {
        // `cargo test` runs tests on worker threads, which AppKit must not be touched from; on
        // other systems there is nothing to install at all.
        let installed = std::thread::spawn(|| install_open_handler(|_paths| {}))
            .join()
            .expect("the test thread ends");
        assert!(!installed);
        // smoke: noting a document off the main thread is a quiet no-op
        note_recent_document(Path::new("/tmp/does-not-matter.pdf"));
    }
}
