use anyhow::{bail, Context};
use blaulicht_core::app::{BlaulichtApp, PopupSpec};
use blaulicht_core::audio::defs::AudioThreadControlSignal;
use blaulicht_core::event::SystemEventBus;
use blaulicht_core::msg::{FromFrontend, SystemMessage};
use blaulicht_core::plugin::PluginManager;
use blaulicht_core::state::{AppState, AppStateWrapper};
use blaulicht_core::{config, mainloop, utils};
use blaulicht_shared::LogLevel;
use env_logger::Env;
use log::info;
use std::sync::atomic::AtomicU8;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

// #[actix_web::main]
fn main() -> anyhow::Result<()> {
    let config_filepath = "./config.toml";

    let cfg = config::read_config(config_filepath.into())?;
    let Some(cfg) = cfg else {
        info!("Created default configuration file at {config_filepath}");
        return Ok(());
    };

    env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();

    let version = env!("CARGO_PKG_VERSION");
    info!("[INIT] Starting BLAULICHT {version}");

    //
    // Audio.
    //

    let (from_frontend_sender, from_frontend_receiver) = crossbeam_channel::unbounded();

    let (system_out, app_system_receiver) = crossbeam_channel::unbounded();
    let audio_thread_control_signal =
        Arc::new(AtomicU8::new(AudioThreadControlSignal::CONTINUE.into()));

    //
    // Read config file.
    //

    match cfg.default_audio_device {
        None => {}
        Some(ref name) => {
            let Some(dev) = utils::device_from_name(name.clone()) else {
                bail!("No such device: {name}");
            };

            info!("Using default audio device: <{name}> from configuration file.");
            from_frontend_sender
                .send(FromFrontend::SelectInputDevice(Some(dev)))
                .unwrap();
        }
    }

    //
    // Event bus.
    //

    let mut event_bus = SystemEventBus::new();

    let event_bus_connection_mainloop = event_bus.new_connection();
    let event_bus_connection_websocket = event_bus.new_connection();
    let event_bus_connection_dmx = event_bus.new_connection();

    thread::spawn(move || {
        event_bus.run();
    });

    let app_state = Arc::new(AppState::new(&cfg.plugins));

    {
        // Audio recording and analysis thread.
        let system_out = system_out.clone();
        let audio_thread_control_signal = audio_thread_control_signal.clone();
        let cfg = cfg.clone();
        let app_state = Arc::clone(&app_state);
        thread::spawn(move || {
            mainloop::supervisor_thread(
                from_frontend_receiver,
                audio_thread_control_signal,
                system_out,
                cfg,
                event_bus_connection_mainloop,
                event_bus_connection_dmx,
                app_state,
            )
        });
    }

    //
    // Filesystem plugin watcher.
    //
    let from_frontend_sender_clone = from_frontend_sender.clone();
    let plugins = cfg.plugins.clone();
    thread::spawn(move || {
        PluginManager::watch_plugins(from_frontend_sender_clone, &plugins)
            .context("Failed to start plugin watcher")
            .unwrap();
    });

    let state_wrapper = AppStateWrapper {
        from_frontend_sender,
        config: Arc::new(Mutex::new(cfg.clone())),
        config_path: config_filepath.to_string(),
        event_bus_connection: event_bus_connection_websocket,
        state: Arc::clone(&app_state),
        system_message_receiver: app_system_receiver,
        system_message_sender: system_out.clone(),
    };
    // let data = Data::new(state_wrapper);

    // thread::spawn(|| {
    // let server = HttpServer::new(move || {
    //     App::new()
    //         .app_data(data.clone())
    //         .service(Files::new("/assets", "./web/dist/assets/"))
    //         // HTML endpoints
    //         .service(routes::get_index)
    //         .service(routes::get_dash)
    //         .service(routes::get_state)
    //         // API endpoints
    //         .route("/api/ws", web::get().to(routes::ws_handler))
    //         .route("/api/ws/sink", web::get().to(routes::binary_ws_handler))
    // })
    // .bind(("0.0.0.0", cfg.port))
    // .with_context(|| "could not start webserver")?;

    // info!("Blaulicht is running on `http://localhost:{}`", cfg.port,);

    // server
    //     .run()
    //     .await
    //     .with_context(|| "Could not start webserver")?;

    // info!("Blaulicht is shutting down...");

    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([800.0, 480.0])
            .with_resizable(true)
            .with_decorations(false),
        ..Default::default()
    };

    if let Some(showfile) = cfg.last_open_showfile {
        let mut dmx = app_state.dmx_engine.write().unwrap();
        let mut artnet = app_state.artnet_output.write().unwrap();
        config::read_showfile(
            showfile.clone(),
            &mut dmx,
            &mut artnet,
            &app_state.plugin_state_storage,
            system_out.clone(),
        );
    };

    // let initial_popup = Some(PopupSpec::with_duration(
    //     Duration::from_secs(2),
    //     "Initializing...".to_string(),
    // ));

    let initial_popup = None;

    eframe::run_native(
        "blaulicht",
        native_options,
        Box::new(|cc| {
            Ok(Box::new(BlaulichtApp::new(
                cc,
                state_wrapper,
                initial_popup,
            )))
        }),
    )
    .unwrap();

    Ok(())
}
