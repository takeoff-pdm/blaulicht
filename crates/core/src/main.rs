//! The desk: the engine and the native egui UI in one process.

use blaulicht_engine::cli::EngineArgs;
use blaulicht_engine::persistence::ExitRequest;
use blaulicht_engine::{Engine, EngineOptions};
use blaulicht_ui_egui::cli::UiArgs;
use clap::Parser;

#[cfg(feature = "dhat")]
#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct CliArgs {
    #[command(flatten)]
    ui: UiArgs,

    #[command(flatten)]
    engine: EngineArgs,
}

fn main() -> anyhow::Result<()> {
    #[cfg(feature = "dhat")]
    let _profiler = dhat::Profiler::new_heap();

    let args = CliArgs::parse();

    let Some(mut engine) = Engine::start(EngineOptions {
        config_file: args.engine.config_file(),
        attach_desk: true,
    })?
    else {
        return Ok(());
    };
    let desk = engine
        .take_desk()
        .expect("the engine was started with a desk client");
    let config_dir = engine.config_file().parent().map(|dir| dir.to_path_buf());

    let result = blaulicht_ui_egui::desk::run(desk, args.ui, config_dir);
    if let Err(error) = &result {
        tracing::error!("The desk window failed: {error}");
    }

    // The desk closed (after `HostCommand::Quit` / `Restart`, or its window
    // was closed): let the persistence service finish, then stop the engine.
    let exit = engine.shutdown();
    result.map_err(|error| anyhow::anyhow!("{error}"))?;
    if let Some(exit @ ExitRequest::Restart) = exit {
        std::process::exit(exit.code());
    }

    Ok(())
}
