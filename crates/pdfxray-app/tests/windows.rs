//! Windows and the operating system: which window a file opens in, new windows, the menu bar's
//! commands, the window title and the logo of the empty window.
//!
//! New windows are new processes; a recording spawner stands in for starting them.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::float_cmp)]

mod common;

use std::io;
use std::path::PathBuf;
use std::sync::Arc;

use common::fake::FakeValidator;
use common::{
    Region, Started, app, builder, drop_file, fixture_path, has_label, open_fixture, recording_spawner,
    wait_for_open,
};
use egui::{Key, Modifiers, OutputCommand, Role, ViewportCommand, ViewportId};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use pdfxray_app::brand::{LOGO_LABEL, NAME, VERSION};
use pdfxray_app::view::ZoomMode;
use pdfxray_app::{Inbox, MenuCommand, PdfxrayApp, RailItem, Recent, Recents, window_title};
use pretty_assertions::assert_eq;

/// The app with a recording spawner, after its first frames.
fn recorded_app() -> (Harness<'static, PdfxrayApp>, Started) {
    let mut harness = app();
    let (spawner, started) = recording_spawner();
    harness.state_mut().state.spawner = spawner;
    (harness, started)
}

/// The app fed by `inbox`, as `run` connects it, with a recording spawner.
fn app_with_inbox(inbox: &Arc<Inbox>) -> (Harness<'static, PdfxrayApp>, Started) {
    let inbox = Arc::clone(inbox);
    let mut harness = builder().build_eframe(move |cc| PdfxrayApp::new(cc).with_inbox(inbox, &cc.egui_ctx));
    let (spawner, started) = recording_spawner();
    harness.state_mut().state.spawner = spawner;
    (harness, started)
}

fn shown(harness: &Harness<'_, PdfxrayApp>) -> Option<String> {
    harness
        .state()
        .state
        .document
        .as_ref()
        .map(|loaded| loaded.info.name.clone())
}

/// The window commands of the last frame.
fn viewport_commands(harness: &Harness<'_, PdfxrayApp>) -> Vec<ViewportCommand> {
    harness
        .output()
        .viewport_output
        .get(&ViewportId::ROOT)
        .map(|viewport| viewport.commands.clone())
        .unwrap_or_default()
}

/// Runs `command` through the inbox, as a click in the menu bar does, for one frame.
fn menu(harness: &mut Harness<'_, PdfxrayApp>, inbox: &Inbox, command: MenuCommand) {
    inbox.push_command(command);
    harness.step();
}

/// Presses ⌘`key` in a single frame and returns that frame's window commands.
fn command_key(harness: &mut Harness<'_, PdfxrayApp>, key: Key) -> Vec<ViewportCommand> {
    harness.event(egui::Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::COMMAND,
    });
    harness.step();
    viewport_commands(harness)
}

fn recent(path: PathBuf) -> Recent {
    Recent {
        name: path.file_name().unwrap().to_string_lossy().into_owned(),
        path,
        pages: 1,
        tagged: false,
        opened_at: 0,
        missing: false,
    }
}

// ---- where files open ----

#[test]
fn a_file_from_the_system_opens_in_an_empty_window() {
    let inbox = Arc::new(Inbox::default());
    // Files a launch was asked to open arrive before the first frame, and wait for it.
    inbox.push_paths([fixture_path("minimal-one-page")]);
    let (mut harness, started) = app_with_inbox(&inbox);
    harness.run();
    wait_for_open(&mut harness);

    assert_eq!(shown(&harness).as_deref(), Some("minimal-one-page.pdf"));
    assert_eq!(*started.borrow(), []);
    // smoke: the inbox was emptied
    assert_eq!(inbox.take_paths(), Vec::<PathBuf>::new());
}

#[test]
fn a_file_from_the_system_opens_a_new_window_when_this_one_has_a_document() {
    let inbox = Arc::new(Inbox::default());
    let (mut harness, started) = app_with_inbox(&inbox);
    harness.run();
    open_fixture(&mut harness, "minimal-one-page");

    inbox.push_paths([fixture_path("struct-rich")]);
    harness.run();

    assert_eq!(*started.borrow(), [Some(fixture_path("struct-rich"))]);
    assert_eq!(shown(&harness).as_deref(), Some("minimal-one-page.pdf"));
    // smoke: nothing is opening here
    assert!(harness.state().state.opening.job().is_none());
}

#[test]
fn of_several_files_the_first_takes_the_empty_window_and_each_other_gets_its_own() {
    let (mut harness, started) = recorded_app();
    let files = [
        fixture_path("minimal-one-page"),
        fixture_path("struct-rich"),
        fixture_path("page-boxes"),
        // the same file twice in one request opens once
        fixture_path("struct-rich"),
    ];

    harness.state_mut().state.open_files(files);
    harness.run();
    wait_for_open(&mut harness);

    assert_eq!(shown(&harness).as_deref(), Some("minimal-one-page.pdf"));
    assert_eq!(
        *started.borrow(),
        [
            Some(fixture_path("struct-rich")),
            Some(fixture_path("page-boxes"))
        ]
    );
}

#[test]
fn a_file_this_window_shows_or_is_opening_is_not_opened_again() {
    let (mut harness, started) = recorded_app();
    let path = fixture_path("minimal-one-page");
    // While it opens (the launch argument and macOS's open event name the same file): skipped.
    harness.state_mut().state.open_files([path.clone()]);
    harness.state_mut().state.open_files([path.clone()]);
    wait_for_open(&mut harness);
    assert_eq!(*started.borrow(), []);

    // Once it shows: skipped, and the status bar says why.
    harness.state_mut().state.open_files([path]);
    harness.run();

    assert_eq!(*started.borrow(), []);
    assert_eq!(
        harness.state().state.status,
        "minimal-one-page.pdf is already open in this window"
    );
    assert_eq!(shown(&harness).as_deref(), Some("minimal-one-page.pdf"));
}

#[test]
fn dropping_a_file_replaces_the_document_of_this_window() {
    let (mut harness, started) = recorded_app();
    open_fixture(&mut harness, "minimal-one-page");

    drop_file(&mut harness, &fixture_path("struct-rich"));
    wait_for_open(&mut harness);

    assert_eq!(shown(&harness).as_deref(), Some("struct-rich.pdf"));
    // smoke: no new window
    assert_eq!(*started.borrow(), []);
}

#[test]
fn a_new_window_that_cannot_start_is_reported_in_the_status_bar() {
    let mut harness = app();
    open_fixture(&mut harness, "minimal-one-page");
    harness.state_mut().state.spawner = Box::new(|_| Err(io::Error::other("no more processes")));

    harness
        .state_mut()
        .state
        .open_files([fixture_path("struct-rich")]);
    assert_eq!(
        harness.state().state.status,
        "Cannot open struct-rich.pdf in a new window: no more processes"
    );
    harness.state_mut().state.new_window();
    assert_eq!(
        harness.state().state.status,
        "Cannot open a new window: no more processes"
    );
    // smoke: this window keeps its file
    assert_eq!(shown(&harness).as_deref(), Some("minimal-one-page.pdf"));
}

// ---- keys and the menu bar ----

#[test]
fn command_n_opens_an_empty_window_and_command_o_with_a_file_opens_the_choice_in_a_new_one() {
    let (mut harness, started) = recorded_app();
    harness.key_press_modifiers(Modifiers::COMMAND, Key::N);
    harness.run();
    assert_eq!(*started.borrow(), [None]);
    // An empty window stays empty.
    assert_eq!(shown(&harness), None);

    open_fixture(&mut harness, "minimal-one-page");
    harness.state_mut().state.picker = Box::new(|| Some(fixture_path("page-boxes")));
    harness.key_press_modifiers(Modifiers::COMMAND, Key::O);
    harness.run();

    assert_eq!(*started.borrow(), [None, Some(fixture_path("page-boxes"))]);
    assert_eq!(shown(&harness).as_deref(), Some("minimal-one-page.pdf"));
}

#[test]
fn command_w_and_command_q_close_the_window() {
    let (mut harness, _spawned) = recorded_app();
    harness.step();
    assert!(!viewport_commands(&harness).contains(&ViewportCommand::Close));
    assert!(command_key(&mut harness, Key::W).contains(&ViewportCommand::Close));

    // ⌘Q is egui's quit shortcut; one process is one window, so quitting closes it.
    harness.run();
    let commands = command_key(&mut harness, Key::Q);
    assert!(commands.contains(&ViewportCommand::Close), "{commands:?}");
}

#[test]
fn with_a_native_menu_bar_the_window_leaves_its_shortcuts_to_it() {
    let (mut harness, started) = recorded_app();
    harness.state_mut().state.native_menu = true;
    harness.state_mut().state.picker = Box::new(|| panic!("the menu bar asks, not the window"));

    for key in [Key::N, Key::O, Key::W] {
        let commands = command_key(&mut harness, key);
        assert!(!commands.contains(&ViewportCommand::Close), "{key:?}");
    }
    harness.run();

    assert_eq!(*started.borrow(), []);
    // smoke: the window is still empty and open
    assert_eq!(shown(&harness), None);
}

#[test]
fn menu_commands_zoom_fit_and_validate_the_open_document() {
    let inbox = Arc::new(Inbox::default());
    let (mut harness, _spawned) = app_with_inbox(&inbox);
    harness.state_mut().state.validator = FakeValidator::ready();
    harness.run();

    // Without a document they do nothing.
    menu(&mut harness, &inbox, MenuCommand::ZoomIn);
    menu(&mut harness, &inbox, MenuCommand::Validate);
    assert_eq!(harness.state().state.canvas.zoom_mode, ZoomMode::Fit);
    assert_eq!(harness.state().state.nav.rail, RailItem::Tags);

    open_fixture(&mut harness, "minimal-one-page");
    menu(&mut harness, &inbox, MenuCommand::ActualSize);
    assert_eq!(harness.state().state.canvas.zoom, 1.0);
    assert_eq!(harness.state().state.canvas.zoom_mode, ZoomMode::Manual);
    menu(&mut harness, &inbox, MenuCommand::ZoomIn);
    let zoomed_in = harness.state().state.canvas.zoom;
    assert!(zoomed_in > 1.0, "zoom {zoomed_in}");
    menu(&mut harness, &inbox, MenuCommand::ZoomOut);
    menu(&mut harness, &inbox, MenuCommand::ZoomOut);
    assert!(harness.state().state.canvas.zoom < 1.0);
    menu(&mut harness, &inbox, MenuCommand::FitPage);
    assert_eq!(harness.state().state.canvas.zoom_mode, ZoomMode::Fit);

    menu(&mut harness, &inbox, MenuCommand::Validate);
    assert_eq!(harness.state().state.nav.rail, RailItem::Validation);
    // smoke: the run was asked for
    assert!(harness.state().state.validation.autorun || harness.state().state.validation.is_running());
}

#[test]
fn menu_commands_close_the_window_open_a_new_one_and_the_guide() {
    let inbox = Arc::new(Inbox::default());
    let (mut harness, started) = app_with_inbox(&inbox);
    harness.run();

    menu(&mut harness, &inbox, MenuCommand::CloseWindow);
    assert!(viewport_commands(&harness).contains(&ViewportCommand::Close));

    menu(&mut harness, &inbox, MenuCommand::NewWindow);
    assert_eq!(*started.borrow(), [None]);

    menu(&mut harness, &inbox, MenuCommand::UserGuide);
    let urls: Vec<String> = harness
        .output()
        .platform_output
        .commands
        .iter()
        .filter_map(|command| match command {
            OutputCommand::OpenUrl(open) => Some(open.url.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(urls.len(), 1);
    // smoke: a test build runs from the checkout, so the guide is the local file
    assert!(urls[0].ends_with("docs/user-guide.md"), "{urls:?}");
}

#[test]
fn open_recent_follows_the_window_rules_and_clear_menu_empties_the_list() {
    let inbox = Arc::new(Inbox::default());
    let (mut harness, started) = app_with_inbox(&inbox);
    let gone = std::env::temp_dir().join(format!("pdfxray-gone-{}.pdf", std::process::id()));
    {
        let recents = &mut harness.state_mut().state.recents;
        *recents = Recents::at(None);
        for path in [
            fixture_path("struct-rich"),
            gone.clone(),
            fixture_path("minimal-one-page"),
        ] {
            recents.record(recent(path)).unwrap();
        }
    }
    harness.run();
    // newest first: minimal-one-page, gone, struct-rich

    // Empty window: here.
    menu(&mut harness, &inbox, MenuCommand::OpenRecent(0));
    wait_for_open(&mut harness);
    assert_eq!(shown(&harness).as_deref(), Some("minimal-one-page.pdf"));

    // A file that is no longer there: said so, nothing starts.
    let gone_index = harness
        .state()
        .state
        .recents
        .entries()
        .iter()
        .position(|r| r.path == gone)
        .unwrap();
    menu(&mut harness, &inbox, MenuCommand::OpenRecent(gone_index));
    assert!(
        harness
            .state()
            .state
            .status
            .starts_with("Cannot open pdfxray-gone-"),
        "{}",
        harness.state().state.status
    );

    // With a document: a new window.
    let struct_rich = harness
        .state()
        .state
        .recents
        .entries()
        .iter()
        .position(|r| r.name == "struct-rich.pdf")
        .unwrap();
    menu(&mut harness, &inbox, MenuCommand::OpenRecent(struct_rich));
    assert_eq!(*started.borrow(), [Some(fixture_path("struct-rich"))]);

    // An index past the list is ignored.
    menu(&mut harness, &inbox, MenuCommand::OpenRecent(99));
    assert_eq!(started.borrow().len(), 1);

    menu(&mut harness, &inbox, MenuCommand::ClearRecent);
    assert_eq!(harness.state().state.recents.entries(), []);
    // smoke: clearing the list does not touch the document
    assert_eq!(shown(&harness).as_deref(), Some("minimal-one-page.pdf"));
}

// ---- the title and the logo ----

/// The titles sent to the window while `harness` steps until the file that is opening is open.
fn titles_until_open(harness: &mut Harness<'_, PdfxrayApp>) -> Vec<String> {
    let mut sent = Vec::new();
    for _ in 0..500 {
        harness.step();
        sent.extend(
            viewport_commands(harness)
                .into_iter()
                .filter_map(|command| match command {
                    ViewportCommand::Title(title) => Some(title),
                    _ => None,
                }),
        );
        if harness.state().state.opening.job().is_none() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    for _ in 0..3 {
        harness.step();
        sent.extend(
            viewport_commands(harness)
                .into_iter()
                .filter_map(|command| match command {
                    ViewportCommand::Title(title) => Some(title),
                    _ => None,
                }),
        );
    }
    sent
}

#[test]
fn the_window_title_names_the_file_and_is_sent_once_per_change() {
    let (mut harness, _spawned) = recorded_app();
    assert_eq!(window_title(&harness.state().state), "pdfxray");
    // No card while opening, so the title goes straight from one file to the next.
    harness.state_mut().state.opening.card_delay = std::time::Duration::from_secs(3600);

    harness
        .state_mut()
        .state
        .open_files([fixture_path("minimal-one-page")]);
    assert_eq!(
        titles_until_open(&mut harness),
        ["minimal-one-page.pdf \u{2014} pdfxray"]
    );

    harness
        .state_mut()
        .state
        .accept_drop(&fixture_path("struct-rich"));
    assert_eq!(
        titles_until_open(&mut harness),
        ["struct-rich.pdf \u{2014} pdfxray"]
    );
    // smoke: the title text agrees
    assert_eq!(
        window_title(&harness.state().state),
        "struct-rich.pdf \u{2014} pdfxray"
    );
}

/// The logos of `region`: the images named [`LOGO_LABEL`].
fn logos_in(harness: &Harness<'_, PdfxrayApp>, region: Region) -> usize {
    common::nodes_in(harness, region, Role::Image)
        .into_iter()
        .filter(|node| node.accesskit_node().label().as_deref() == Some(LOGO_LABEL))
        .count()
}

#[test]
fn the_logo_heads_the_rail_and_the_drop_zone() {
    let mut harness = app();
    assert_eq!(logos_in(&harness, Region::Rail), 1);
    assert_eq!(logos_in(&harness, Region::Center), 1);
    // The drop zone's logo sits above its headline; the rail's above the first entry.
    let headline = harness.get_by_label("Drop a PDF to inspect it").rect();
    let logos: Vec<_> = harness
        .query_all_by_label(LOGO_LABEL)
        .map(|node| node.rect())
        .collect();
    assert!(
        logos
            .iter()
            .any(|logo| logo.bottom() < headline.top() && logo.width() > 70.0)
    );
    let tags = common::the(&harness, Region::Rail, Role::Button, "Tags").rect();
    assert!(
        logos
            .iter()
            .any(|logo| logo.bottom() < tags.top() && logo.width() < 30.0)
    );

    // Hovering the rail's logo names the app and its version; it is not a button.
    let rail_logo = logos.iter().find(|logo| logo.width() < 30.0).copied().unwrap();
    harness.hover_at(rail_logo.center());
    harness.run();
    assert!(has_label(&harness, &format!("{NAME} {VERSION}")));
    assert_eq!(common::nodes_in(&harness, Region::Rail, Role::Button).len(), 9);

    open_fixture(&mut harness, "minimal-one-page");
    // smoke: with a document only the rail keeps its logo
    assert_eq!(
        (
            logos_in(&harness, Region::Rail),
            logos_in(&harness, Region::Center)
        ),
        (1, 0)
    );
}
