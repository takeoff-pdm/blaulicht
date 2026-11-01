//! Running the desk: the native window around [`BlaulichtApp`].

use std::path::PathBuf;

use blaulicht_engine::client::LocalClient;

use crate::app::BlaulichtApp;
use crate::cli::UiArgs;

/// Opens the desk window on `client` and runs it until the window closes
/// (after `HostCommand::Quit` / `Restart`, or when it is closed directly).
/// `config_dir` is where the file dialogs start without an open showfile.
pub fn run(client: LocalClient, args: UiArgs, config_dir: Option<PathBuf>) -> eframe::Result {
    let layout = args.layout();
    // The mobile layout is made for the desk's 800x480 panel; the desktop
    // layout is a resizable window.
    let viewport = if layout.is_desktop() {
        egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([640.0, 400.0])
            .with_resizable(true)
    } else {
        egui::ViewportBuilder::default()
            .with_inner_size([800.0, 480.0])
            .with_resizable(false)
    };
    let native_options = eframe::NativeOptions {
        viewport: viewport.with_decorations(args.window_decorations),
        // The visualizer is rendered in an egui GL callback and requires a
        // real depth attachment for opaque fixture geometry and outlines.
        depth_buffer: 24,
        multisampling: 4,
        ..Default::default()
    };

    let initial_popup = None;

    eframe::run_native(
        "blaulicht",
        native_options,
        Box::new(move |cc| {
            Ok(Box::new(BlaulichtApp::new(
                cc,
                client.into(),
                initial_popup,
                layout,
                args.showfile_home,
                config_dir,
                args.external_screens,
            )))
        }),
    )
}
