//! pdfxray desktop app.
//!
//! UI only: panels draw what `pdfxray-core` answers and `pdfxray-render` rasterises; there is no
//! PDF logic here. All state lives in [`AppState`] and is rendered by [`show`], so tests can
//! drive the whole app without a native window.
//!
//! The window is one fixed shell ([`shell`]): title bar, activity rail, navigator, centre,
//! inspector, drawer and status bar. Each region has its own module:
//!
//! * [`theme`]: design tokens, fonts and the egui style; [`components`]: the shared widgets.
//! * [`state`]: one sub-struct per area (navigation, selection, history, drawer, canvas).
//! * [`navigator`], [`center`], [`inspector`], [`drawer`]: the hosts and their panels.
//! * [`tree_view`], [`validation`], [`findings`]: the virtualised tree, the veraPDF runs and the
//!   failed checks arranged for the lists, the canvas and the inspector.
//! * [`states`]: what the window shows without a workspace (empty, opening, encrypted, damaged),
//!   [`open`]: opening a file on a background thread, [`recents`]: the files opened lately.
//! * [`roles`]: the colour of each tag role, shared by the tree, the canvas overlay and the legend.
//! * [`view`], [`units`], [`modes`]: zoom maths, units, and the debug render modes (bitmaps are
//!   rendered on a background thread); object previews have a thread of their own.
//! * [`instances`]: one window per process, and which window a file opens in; [`commands`]: the
//!   menu bar's commands; [`platform`]: what the operating system sends (files to open, menu
//!   clicks) and the macOS menu bar; [`brand`]: the logo, the window icon and the About facts.

pub mod actions;
pub mod brand;
pub mod center;
pub mod commands;
pub mod components;
pub mod drawer;
pub mod findings;
pub mod inspector;
pub mod instances;
pub mod modes;
pub mod navigator;
pub mod object_facts;
pub mod open;
mod page_cache;
pub mod platform;
mod previews;
pub mod recents;
pub mod roles;
pub mod selection;
pub mod shell;
pub mod state;
pub mod states;
pub mod stepper;
#[cfg(test)]
mod test_support;
pub mod theme;
mod thumbnails;
pub mod tree_view;
pub mod units;
pub mod validation;
pub mod view;

use std::path::PathBuf;
use std::sync::Arc;

pub use center::canvas::{measure, overlay, rulers};
pub use commands::MenuCommand;
pub use instances::{Spawner, native_spawner, window_title};
pub use measure::{Comparison, Cursor, MeasureTool};
pub use modes::{
    COLOR_MODES, IsolateMode, Isolation, LegendEntry, OrderSource, RenderRequest, TagShow, ViewMode,
    ViewTool, XRAY_ALPHA, color_title, heatmap_legend, legend_entries,
};
pub use open::{FilePicker, OpenProgress, Opener, Source, StageState, native_picker};
pub use page_cache::pixel_digest;
pub use platform::Inbox;
pub use previews::preview_digest;
pub use recents::{Recent, Recents, RecentsError};
pub use roles::role_color;
pub use shell::show;
pub use state::{
    AppState, CanvasState, DrawerState, DrawerTab, HitEntry, HitStack, LoadedDocument, NavState, NavTree,
    ObjectHistory, ObjectMode, ObjectRow, ObjectsState, READY, RailItem, ResourceGroup, ResourceRow,
    SelectionState,
};
pub use states::Screen;
pub use units::{Lengths, Unit};
pub use validation::{
    Availability, Environment, InstallProgress, ValidationState, Validator, ValidatorError, VeraPdfValidator,
};

/// The app's name: the window title without a file, and the name in the menu bar.
pub const APP_TITLE: &str = brand::NAME;

/// The eframe application: [`AppState`] rendered by [`show`], plus what connects it to the
/// operating system (the files and menu commands of the [`Inbox`], the window title and, on
/// macOS, the menu bar).
#[derive(Default)]
pub struct PdfxrayApp {
    /// Current state.
    pub state: AppState,
    inbox: Option<Arc<Inbox>>,
    /// The window title last sent to the platform.
    title: String,
    #[cfg(target_os = "macos")]
    menu_bar: Option<platform::macos::MenuBar>,
}

impl PdfxrayApp {
    /// Create the app: dark theme, fonts, style and icon loaders.
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        theme::install(&cc.egui_ctx);
        Self::default()
    }

    /// Remembers the files opened in the app data directory, and lists them in the empty state.
    #[must_use]
    pub fn with_recents_in_data_dir(mut self) -> Self {
        self.state.recents = Recents::in_data_dir();
        self
    }

    /// Opens `path` right away, as if it had been dropped on the window.
    #[must_use]
    pub fn with_document(mut self, path: Option<PathBuf>) -> Self {
        if let Some(path) = path {
            self.state.open_path(&path);
        }
        self
    }

    /// Takes in the files and menu commands queued in `inbox` at the start of every frame; `ctx`
    /// is woken when something arrives. What was queued before is taken in on the first frame.
    #[must_use]
    pub fn with_inbox(mut self, inbox: Arc<Inbox>, ctx: &egui::Context) -> Self {
        inbox.attach(ctx);
        self.inbox = Some(inbox);
        self
    }

    /// Keeps the macOS menu bar in line with the app (recent files, document commands), and
    /// leaves the shortcuts the menu has to it.
    #[cfg(target_os = "macos")]
    #[must_use]
    pub fn with_menu_bar(mut self, menu_bar: Option<platform::macos::MenuBar>) -> Self {
        self.state.native_menu = menu_bar.is_some();
        self.menu_bar = menu_bar;
        self
    }

    /// Takes in what the operating system sent since the last frame.
    fn take_inbox(&mut self, ctx: &egui::Context) {
        let Some(inbox) = &self.inbox else {
            return;
        };
        let paths = inbox.take_paths();
        if !paths.is_empty() {
            self.state.open_files(paths);
        }
        for command in inbox.take_commands() {
            command.apply(&mut self.state, ctx);
        }
    }

    /// Sends the window title when it changed.
    fn sync_title(&mut self, ctx: &egui::Context) {
        let title = window_title(&self.state);
        if title != self.title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.title = title;
        }
    }
}

impl eframe::App for PdfxrayApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.take_inbox(&ctx);
        show(ui, &mut self.state);
        self.sync_title(&ctx);
        #[cfg(target_os = "macos")]
        if let Some(menu_bar) = &mut self.menu_bar {
            menu_bar.sync(&self.state);
        }
    }
}

/// The options of the native window.
pub fn native_options() -> eframe::NativeOptions {
    eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(APP_TITLE)
            .with_app_id(APP_TITLE)
            .with_icon(brand::window_icon())
            .with_inner_size([1440.0, 900.0])
            .with_min_inner_size([1100.0, 700.0])
            // macOS: the content runs under a transparent title bar, so the traffic lights sit in
            // our own title bar (which leaves room for them) instead of a strip above it.
            .with_fullsize_content_view(true)
            .with_titlebar_shown(false)
            .with_title_shown(false),
        ..Default::default()
    }
}

/// Open the native window, optionally with `document` already loaded, and run until it is closed.
///
/// The event loop is built here rather than by eframe so that, on macOS, the handler of the files
/// Finder and the Dock send is in place before the loop starts (the files a launch was asked to
/// open arrive then), and so the menu bar replaces winit's default one.
pub fn run(document: Option<PathBuf>) -> eframe::Result {
    let event_loop = event_loop()?;
    let inbox = Arc::new(Inbox::default());
    #[cfg(target_os = "macos")]
    let menu_bar = platform::macos::install(&inbox);
    let creator: eframe::AppCreator<'_> = Box::new(move |cc| {
        let app = PdfxrayApp::new(cc)
            .with_recents_in_data_dir()
            .with_document(document)
            .with_inbox(inbox, &cc.egui_ctx);
        #[cfg(target_os = "macos")]
        let app = app.with_menu_bar(menu_bar);
        Ok(Box::new(app))
    });
    let mut app = eframe::create_native(APP_TITLE, native_options(), creator, &event_loop);
    event_loop.run_app(&mut app)?;
    Ok(())
}

fn event_loop() -> eframe::Result<winit::event_loop::EventLoop<eframe::UserEvent>> {
    let mut builder = winit::event_loop::EventLoop::<eframe::UserEvent>::with_user_event();
    #[cfg(target_os = "macos")]
    {
        use winit::platform::macos::EventLoopBuilderExtMacOS as _;
        // Our menu bar replaces it; winit would otherwise install its own over ours at launch.
        builder.with_default_menu(false);
    }
    Ok(builder.build()?)
}
