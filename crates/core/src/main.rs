use anyhow::{bail, Context};
use blaulicht_core::app::BlaulichtApp;
use blaulicht_core::audio::defs::AudioThreadControlSignal;
use blaulicht_core::cli::CliArgs;
use blaulicht_core::event::SystemEventBus;
use blaulicht_core::msg::{FromFrontend, SystemMessage};
use blaulicht_core::plugin::PluginManager;
use blaulicht_core::state::{AppState, AppStateWrapper};
use blaulicht_core::{config, mainloop, utils};
use blaulicht_shared::LogLevel;
use clap::Parser;
use crossbeam_channel::Sender;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::atomic::AtomicU8;
use std::sync::{Arc, Mutex};
use std::thread;
use tracing::{info, Event, Level, Subscriber};
use tracing_subscriber::layer::{Context as TraceContext, SubscriberExt};
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, Layer};
use winit::event_loop::EventLoop;

#[cfg(feature = "dhat")]
#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;

#[derive(Clone)]
struct SystemOutLayer {
    system_out: Sender<SystemMessage>,
}

impl SystemOutLayer {
    fn new(system_out: Sender<SystemMessage>) -> Self {
        Self { system_out }
    }
}

#[derive(Default)]
struct LogVisitor {
    message: Option<String>,
    fields: Vec<String>,
}

impl tracing::field::Visit for LogVisitor {
    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        if field.name() == "message" {
            self.message = Some(value.to_string());
        } else {
            self.fields.push(format!("{}={value}", field.name()));
        }
    }

    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            self.message = Some(format!("{value:?}"));
        } else {
            self.fields.push(format!("{}={value:?}", field.name()));
        }
    }
}

impl<S> Layer<S> for SystemOutLayer
where
    S: Subscriber,
{
    fn on_event(&self, event: &Event<'_>, _ctx: TraceContext<'_, S>) {
        let level = match *event.metadata().level() {
            Level::ERROR => LogLevel::Err,
            Level::WARN => LogLevel::Warn,
            Level::INFO => LogLevel::Info,
            Level::DEBUG | Level::TRACE => LogLevel::Debug,
        };

        let mut visitor = LogVisitor::default();
        event.record(&mut visitor);

        let message = match visitor.message {
            Some(msg) => msg,
            None if !visitor.fields.is_empty() => visitor.fields.join(" "),
            None => event.metadata().target().to_string(),
        };

        let _ = self.system_out.send(SystemMessage::Log(message, level));
    }
}

fn init_tracing(system_out: Sender<SystemMessage>) {
    let _ = tracing_log::LogTracer::init();
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    let subscriber = tracing_subscriber::registry()
        .with(env_filter)
        .with(tracing_subscriber::fmt::layer())
        .with(SystemOutLayer::new(system_out));

    let _ = subscriber.try_init();
}

fn main() -> anyhow::Result<()> {
    #[cfg(feature = "dhat")]
    let _profiler = dhat::Profiler::new_heap();

    let args = CliArgs::parse();

    let config_filepath = match args.config_file {
        Some(path) => path,
        None => PathBuf::from_str("./config.toml").unwrap(),
    };

    let (system_out, app_system_receiver) = crossbeam_channel::unbounded();
    init_tracing(system_out.clone());

    let cfg = config::read_config(config_filepath.clone())?;
    let Some(cfg) = cfg else {
        info!(
            "Created default configuration file at {}",
            config_filepath.to_string_lossy()
        );
        return Ok(());
    };

    let version = env!("CARGO_PKG_VERSION");
    info!("[INIT] Starting BLAULICHT {version}");

    //
    // Audio.
    //

    let (from_frontend_sender, from_frontend_receiver) = crossbeam_channel::unbounded();

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
        let mut global_state = app_state.plugin_state_storage_global.lock().unwrap();
        *global_state = cfg.plugin_state.clone();
    }

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
        config_path: config_filepath.to_string_lossy().to_string(),
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

    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([800.0, 480.0])
            .with_resizable(args.desktop_mode)
            .with_decorations(args.window_decorations),
        ..Default::default()
    };

    let initial_popup = None;

    eframe::run_native(
        "blaulicht",
        native_options,
        Box::new(|cc| {
            Ok(Box::new(BlaulichtApp::new(
                cc,
                state_wrapper,
                initial_popup,
                args.desktop_mode,
                args.showfile_home.clone(),
            )))
        }),
    )
    .unwrap();

    // system_out.send(SystemMessage::AudioSelected(()));

    Ok(())
}
