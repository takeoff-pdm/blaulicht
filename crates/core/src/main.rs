use anyhow::{bail, Context};
use blaulicht_core::app::BlaulichtApp;
use blaulicht_core::audio::defs::AudioThreadControlSignal;
use blaulicht_core::event::SystemEventBus;
use blaulicht_core::msg::FromFrontend;
use blaulicht_core::plugin::PluginManager;
use blaulicht_core::routes::{AppState, AppStateWrapper};
use blaulicht_core::{config, mainloop, utils};
use env_logger::Env;
use log::info;
use std::sync::atomic::AtomicU8;
use std::sync::{Arc, Mutex};
use std::thread;

// #[actix_web::main]
fn main() -> anyhow::Result<()> {
    let config_filepath = "./config.toml";

    let cfg = config::read_config(config_filepath.into())?;
    let Some(cfg) = cfg else {
        info!("Created default configuration file at {config_filepath}");
        return Ok(());
    };

    env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();

    //
    // Audio.
    //

    let (from_frontend_sender, from_frontend_receiver) = crossbeam_channel::unbounded();
    let (app_signal_out, app_signal_receiver) = crossbeam_channel::unbounded();

    let (system_out, app_system_receiver) = crossbeam_channel::unbounded();
    let audio_thread_control_signal = Arc::new(AtomicU8::new(AudioThreadControlSignal::CONTINUE));

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

    // let send = midi_in_sender.clone();
    // TODO: use every midi device available on the system?
    // OR: have a midi pool which is dynamic and each plugin can request a midi device?
    // -> this seems reasonable

    // thread::spawn(move || {
    // match plugin::midi::midi(send, midi_out_receiver.clone()) {
    //     Ok(_) => panic!("Unreachable."),
    //     Err(err) => {
    //         let msg = format!("MIDI thread crashed! {err:?}");
    //         sys_out.send(SystemMessage::Log(msg)).unwrap();
    //     }
    // }

    // Debug midi thread.
    // Allows the dev to se what MIDI messages are sent to the device.
    // loop {
    //     thread::sleep(Duration::from_millis(50));
    //     match midi_out_receiver.try_recv() {
    //         Ok(_midi) => {
    //             // TODO: include if required
    //             // println!("MIDI to dev: {_midi:?}")
    //         }
    //         Err(TryRecvError::Empty) => {}
    //         Err(TryRecvError::Disconnected) => {
    //             log::warn!("[MIDI] Shutting down.");
    //             break;
    //         }
    //     }
    // }
    // });

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
                app_signal_out,
                system_out,
                cfg,
                event_bus_connection_mainloop,
                event_bus_connection_dmx,
                app_state,
            )
        });
    }

    //
    // End audio.
    //

    // TODO: SLOOOWW DISTRIBUTOR LOGIC!

    // let consumers: Arc<Mutex<HashMap<String, Sender<UnifiedMessage>>>> =
    //     Arc::new(Mutex::new(HashMap::new()));

    // let consumers2 = consumers.clone();
    // let app_state_temp = Arc::clone(&app_state);
    // thread::spawn(move || {
    //     loop {
    //         //
    //         // System messages.
    //         //
    //         let system_res = app_system_receiver.try_recv();
    //         match system_res {
    //             Ok(res) => {
    //                 match &res {
    //                     SystemMessage::Log(msg) => {
    //                         app_state_temp.log(msg.clone().into()); // TODO: GRR, clone!
    //                     }
    //                     SystemMessage::WasmLog(body) => {
    //                         println!("{}", body.msg);
    //                         app_state_temp.log_plugin(body.plugin_id, body.msg.clone());
    //                         // TODO: GRR, clone!
    //                     }
    //                     _ => {}
    //                 }

    //                 let consumers = consumers2.lock().unwrap();
    //                 for c in consumers.values() {
    //                     if c.send(UnifiedMessage::System(res.clone())).is_err() {
    //                         continue;
    //                     }
    //                 }
    //             }
    //             Err(crossbeam_channel::TryRecvError::Empty) => {}
    //             Err(crossbeam_channel::TryRecvError::Disconnected) => unreachable!(),
    //         }

    //         //
    //         // Signal messages.
    //         //
    //         let signal_res = app_signal_receiver.try_recv();
    //         match signal_res {
    //             Ok(res) => {
    //                 let consumers = consumers2.lock().unwrap();
    //                 for c in consumers.values() {
    //                     if c.send(UnifiedMessage::Signal(res.clone())).is_err() {
    //                         continue;
    //                     }
    //                 }
    //             }
    //             Err(crossbeam_channel::TryRecvError::Empty) => {}
    //             Err(crossbeam_channel::TryRecvError::Disconnected) => {
    //                 log::warn!("[BROADCAST] Shutting down.");
    //                 break;
    //             }
    //         }
    //     }
    // });

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

    // Spawn UI thread.
    // thread::spawn(|| {});

    let state_wrapper = AppStateWrapper {
        from_frontend_sender,
        config: Arc::new(Mutex::new(cfg.clone())),
        config_path: config_filepath.to_string(),
        event_bus_connection: event_bus_connection_websocket,
        state: Arc::clone(&app_state),
        system_message_receiver: app_system_receiver,
        signal_receiver: app_signal_receiver,
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
            .with_inner_size([400.0, 300.0])
            .with_min_inner_size([300.0, 220.0]),
        ..Default::default()
    };

    eframe::run_native(
        "blaulicht",
        native_options,
        Box::new(|cc| Ok(Box::new(BlaulichtApp::new(cc, state_wrapper)))),
    )
    .unwrap();

    // TODO: what is this?
    // let sig = audio_thread_control_signal.load(Ordering::Relaxed);
    // let start_shutdown = Instant::now();
    // if sig == AudioThreadControlSignal::CONTINUE {
    //     audio_thread_control_signal.store(AudioThreadControlSignal::ABORT, Ordering::Relaxed);
    //     loop {
    //         thread::sleep(Duration::from_secs(1));

    //         let sig = audio_thread_control_signal.load(Ordering::Relaxed);

    //         log::trace!("Waiting for audio thread to die: {sig}");

    //         if start_shutdown.elapsed() > Duration::from_secs(5) {
    //             log::warn!("Shutdown timeout");
    //             break;
    //         }

    //         if sig == AudioThreadControlSignal::ABORTED {
    //             break;
    //         }
    //     }
    // }

    Ok(())
}
