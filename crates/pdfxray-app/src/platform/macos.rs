//! The macOS menu bar and the files Finder and the Dock send.
//!
//! The menu bar is built with `muda`. Its items carry the ids of [`MenuCommand`]; a click is
//! queued in the [`Inbox`] and carried out at the start of the next frame. `AppKit` handles a key
//! equivalent of a menu item before the window sees the key, so the window leaves those keys to
//! the menu (`AppState::native_menu`).
//!
//! There is deliberately no Edit menu: its ⌘C, ⌘V and ⌘A would be taken by `AppKit` before egui's
//! text fields and copy handling see them.
//!
//! In a release app, which carries Sparkle, the app menu has "Check for Updates…". It is enabled
//! in the one window process that runs the updater ([`super::updater`]) and disabled in the others;
//! builds without Sparkle do not show it.
//!
//! [`MenuCommand`]: crate::commands::MenuCommand

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use muda::accelerator::{Accelerator, Code, Modifiers};
use muda::{AboutMetadata, Icon, IsMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu};

use super::Inbox;
use super::updater::{self, UpdaterLock, UpdaterSlot};
use crate::brand;
use crate::commands::MenuCommand;
use crate::recents;
use crate::state::AppState;

/// Title of the item that empties File › Open Recent.
pub const CLEAR_RECENT: &str = "Clear Menu";

/// Id of App › Check for Updates…, which Sparkle carries out (not a window command).
pub const CHECK_FOR_UPDATES: &str = "app.check-for-updates";

/// App › Check for Updates… and the updater behind it.
struct AutoUpdate {
    item: MenuItem,
    /// The lock file that makes one window process the updater's; `None` without a data
    /// directory, and then no process runs it.
    lock_path: Option<PathBuf>,
    slot: UpdaterSlot<UpdaterLock>,
}

impl AutoUpdate {
    /// Starts the updater in this process if no other window runs it (cheap between retries).
    fn poll(&mut self) {
        let was_running = self.slot.is_running();
        let lock_path = self.lock_path.as_deref();
        let running = self.slot.poll(
            Instant::now(),
            || lock_path.and_then(|path| UpdaterLock::try_acquire(path).ok().flatten()),
            start_updater,
        );
        if running != was_running {
            self.item.set_enabled(running);
        }
    }
}

/// Starts Sparkle; a bundled framework that does not start is reported once on stderr (Console
/// shows it), since it means a broken release.
fn start_updater() -> bool {
    match pdfxray_macos::start_updater() {
        Ok(()) => true,
        Err(error) => {
            #[expect(clippy::print_stderr, reason = "the only trace a broken updater leaves")]
            {
                eprintln!("pdfxray: the updater did not start: {error}");
            }
            false
        }
    }
}

/// The menu bar of the app and the parts of it that follow the app's state.
pub struct MenuBar {
    /// Owns the native menu; dropping it would empty the bar.
    menu: Menu,
    /// File › Open Recent.
    recent: Submenu,
    /// Items that need a document: Validate and the View commands.
    document_items: Vec<MenuItem>,
    /// The recent files the Open Recent menu lists: path, name and whether the file is there.
    listed: Option<Vec<(PathBuf, String, bool)>>,
    /// Whether the document items are enabled.
    enabled: Option<bool>,
    /// App › Check for Updates…; `None` when the app does not carry Sparkle.
    updates: Option<AutoUpdate>,
}

impl std::fmt::Debug for MenuBar {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MenuBar")
            .field("listed", &self.listed)
            .field("enabled", &self.enabled)
            .finish_non_exhaustive()
    }
}

/// Connects the app to macOS: files opened from Finder and the Dock go to `inbox`, and the menu
/// bar is installed with its clicks going there too.
///
/// Call on the main thread after the event loop is built and before it runs. `None` when the menu
/// bar could not be built; the window then keeps its own shortcuts.
pub fn install(inbox: &Arc<Inbox>) -> Option<MenuBar> {
    let files = Arc::clone(inbox);
    pdfxray_macos::install_open_handler(move |paths| files.push_paths(paths));

    let mut bar = build().ok()?;
    bar.menu.init_for_nsapp();
    if let Some(updates) = &mut bar.updates {
        updates.poll();
    }
    let clicks = Arc::clone(inbox);
    // muda calls this on the main thread, from the menu item's action.
    MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
        if event.id.0 == CHECK_FOR_UPDATES {
            pdfxray_macos::check_for_updates();
        } else if let Some(command) = MenuCommand::from_id(&event.id.0) {
            clicks.push_command(command);
        }
    }));
    Some(bar)
}

fn command_item(command: MenuCommand, title: &str, accelerator: Option<Accelerator>) -> MenuItem {
    MenuItem::with_id(command.id(), title, true, accelerator)
}

fn key(modifiers: Modifiers, code: Code) -> Accelerator {
    Accelerator::new(Some(modifiers), code)
}

fn about() -> AboutMetadata {
    AboutMetadata {
        name: Some(brand::NAME.to_owned()),
        version: Some(brand::VERSION.to_owned()),
        copyright: Some(brand::COPYRIGHT.to_owned()),
        // macOS shows the credits, not the licence or website fields.
        credits: Some(format!("{}\n{}", brand::LICENSE, brand::REPOSITORY)),
        license: Some(brand::LICENSE.to_owned()),
        website: Some(brand::REPOSITORY.to_owned()),
        icon: brand::logo_rgba(brand::LogoArt::Master)
            .and_then(|(rgba, width, height)| Icon::from_rgba(rgba, width, height).ok()),
        ..AboutMetadata::default()
    }
}

fn build() -> muda::Result<MenuBar> {
    let cmd = Modifiers::SUPER;
    // Disabled until this process runs the updater.
    let check_updates = pdfxray_macos::updater_bundled()
        .then(|| MenuItem::with_id(CHECK_FOR_UPDATES, "Check for Updates\u{2026}", false, None));
    let about_item = PredefinedMenuItem::about(Some("About pdfxray"), Some(about()));
    let mut top: Vec<&dyn IsMenuItem> = vec![&about_item];
    if let Some(item) = &check_updates {
        top.push(item);
    }
    let app = Submenu::with_items(
        brand::NAME,
        true,
        &[
            top.as_slice(),
            &[
                &PredefinedMenuItem::separator(),
                &PredefinedMenuItem::services(None),
                &PredefinedMenuItem::separator(),
                &PredefinedMenuItem::hide(None),
                &PredefinedMenuItem::hide_others(None),
                &PredefinedMenuItem::show_all(None),
                &PredefinedMenuItem::separator(),
                &PredefinedMenuItem::quit(None),
            ],
        ]
        .concat(),
    )?;

    let recent = Submenu::new("Open Recent", true);
    let validate = command_item(
        MenuCommand::Validate,
        "Validate",
        Some(key(cmd | Modifiers::SHIFT, Code::KeyV)),
    );
    let file = Submenu::with_items(
        "File",
        true,
        &[
            &command_item(MenuCommand::NewWindow, "New Window", Some(key(cmd, Code::KeyN))),
            &command_item(MenuCommand::Open, "Open\u{2026}", Some(key(cmd, Code::KeyO))),
            &recent,
            &PredefinedMenuItem::separator(),
            &command_item(
                MenuCommand::CloseWindow,
                "Close Window",
                Some(key(cmd, Code::KeyW)),
            ),
            &PredefinedMenuItem::separator(),
            &validate,
        ],
    )?;

    let zoom_in = command_item(MenuCommand::ZoomIn, "Zoom In", Some(key(cmd, Code::Equal)));
    let zoom_out = command_item(MenuCommand::ZoomOut, "Zoom Out", Some(key(cmd, Code::Minus)));
    let actual = command_item(
        MenuCommand::ActualSize,
        "Actual Size",
        Some(key(cmd, Code::Digit0)),
    );
    let fit = command_item(MenuCommand::FitPage, "Fit Page", Some(key(cmd, Code::Digit9)));
    let view = Submenu::with_items(
        "View",
        true,
        &[
            &zoom_in,
            &zoom_out,
            &actual,
            &fit,
            &PredefinedMenuItem::separator(),
            &PredefinedMenuItem::fullscreen(Some("Enter Full Screen")),
        ],
    )?;

    let window = Submenu::with_items(
        "Window",
        true,
        &[
            &PredefinedMenuItem::minimize(None),
            &PredefinedMenuItem::maximize(Some("Zoom")),
            &PredefinedMenuItem::separator(),
            &PredefinedMenuItem::bring_all_to_front(None),
        ],
    )?;
    window.set_as_windows_menu_for_nsapp();

    let help = Submenu::with_items(
        "Help",
        true,
        &[&command_item(MenuCommand::UserGuide, "pdfxray User Guide", None)],
    )?;
    help.set_as_help_menu_for_nsapp();

    let menu = Menu::with_items(&[&app, &file, &view, &window, &help])?;
    let mut bar = MenuBar {
        menu,
        recent,
        document_items: vec![validate, zoom_in, zoom_out, actual, fit],
        listed: None,
        enabled: None,
        updates: check_updates.map(|item| AutoUpdate {
            item,
            lock_path: recents::data_dir().map(|dir| updater::lock_path(&dir)),
            slot: UpdaterSlot::default(),
        }),
    };
    bar.list_recents(&[]);
    Ok(bar)
}

impl MenuBar {
    /// Brings the menu bar in line with `state`: the recent files and the items that need a
    /// document. Cheap when nothing changed; called once per frame.
    pub fn sync(&mut self, state: &AppState) {
        // A window that is still open takes the updater over when the window running it quits.
        if let Some(updates) = &mut self.updates {
            updates.poll();
        }
        let recents: Vec<(PathBuf, String, bool)> = state
            .recents
            .entries()
            .iter()
            .map(|recent| (recent.path.clone(), recent.name.clone(), !recent.missing))
            .collect();
        if self.listed.as_ref() != Some(&recents) {
            // A file that just came to the top was opened: tell the system's recent documents.
            let previous_top = self
                .listed
                .as_ref()
                .and_then(|listed| listed.first().map(|(path, _, _)| path.clone()));
            if let Some((top, _, _)) = recents.first()
                && self.listed.is_some()
                && previous_top.as_ref() != Some(top)
            {
                pdfxray_macos::note_recent_document(top);
            }
            self.list_recents(&recents);
        }
        let enabled = state.screen().has_workspace();
        if self.enabled != Some(enabled) {
            for item in &self.document_items {
                item.set_enabled(enabled);
            }
            self.enabled = Some(enabled);
        }
    }

    fn list_recents(&mut self, recents: &[(PathBuf, String, bool)]) {
        for _ in 0..self.recent.items().len() {
            self.recent.remove_at(0);
        }
        let mut items: Vec<Box<dyn IsMenuItem>> = recents
            .iter()
            .enumerate()
            .map(|(index, (_, name, present))| {
                Box::new(MenuItem::with_id(
                    MenuCommand::OpenRecent(index).id(),
                    name,
                    *present,
                    None,
                )) as Box<dyn IsMenuItem>
            })
            .collect();
        if !recents.is_empty() {
            items.push(Box::new(PredefinedMenuItem::separator()));
        }
        items.push(Box::new(MenuItem::with_id(
            MenuCommand::ClearRecent.id(),
            CLEAR_RECENT,
            !recents.is_empty(),
            None,
        )));
        for item in &items {
            // Appending to a submenu of ours only fails for an item that is already in it.
            let _ = self.recent.append(item.as_ref());
        }
        self.listed = Some(recents.to_vec());
    }
}
