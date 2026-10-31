//! Running the desk: the native window around [`BlaulichtApp`].

use std::path::PathBuf;

use blaulicht_engine::client::LocalClient;

use crate::app::BlaulichtApp;
use crate::cli::UiArgs;

/// Opens the desk window on `client` and runs it until the window closes
/// (after `HostCommand::Quit` / `Restart`, or when it is closed directly).
/// `config_dir` is where the file dialogs start without an open showfile.
pub fn run(client: LocalClient, args: UiArgs, config_dir: Option<PathBuf>) -> eframe::Result {
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([800.0, 480.0])
            .with_resizable(args.desktop_mode)
            .with_decorations(args.window_decorations),
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
                client,
                initial_popup,
                args.desktop_mode,
                args.showfile_home,
                config_dir,
                args.external_screens,
            )))
        }),
    )
}
