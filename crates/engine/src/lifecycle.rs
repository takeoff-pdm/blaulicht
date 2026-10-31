//! Starting and stopping the engine. Both binaries (the desk and
//! `blaulicht-headless`) go through [`Engine::start`] and
//! [`EngineHandle::shutdown`].

use std::net::{Ipv4Addr, SocketAddr};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicU8;
use std::sync::Arc;
use std::thread;

use anyhow::Context;
use tracing::info;

use crate::audio::defs::AudioThreadControlSignal;
use crate::client::LocalClient;
use crate::config::{self, SharedConfig};
use crate::event::{EventBusHandle, SystemEventBus};
use crate::hub::{BusMessage, ClientId, SessionId, SystemHub};
use crate::mainloop::{self, SupervisorRequest};
use crate::msg::{StageStatus, StartupStage, SystemMessage};
use crate::persistence::{ExitRequest, PersistenceService, PersistenceThread};
use crate::plugin::PluginManager;
use crate::server::{Server, ServerHandle, ServerOptions};
use crate::state::AppState;
use crate::utils;

/// How to start the engine.
#[derive(Clone, Debug)]
pub struct EngineOptions {
    /// The engine's `config.toml`. A missing file is created with defaults,
    /// and then nothing starts (see [`Engine::start`]).
    pub config_file: PathBuf,
    /// Connect the desk client (`ClientId::Desk` in `SessionId::DESK`)
    /// before the startup showfile loads and the plugins start, so it gets
    /// the startup `ShowLoaded` and plugin requests sent during startup
    /// (an owned external screen, for example). Take it with
    /// [`EngineHandle::take_desk`]. The headless binary has no desk.
    pub attach_desk: bool,
}

/// The engine as a whole: config, event bus, persistence, the supervisor
/// (main loop, plugins, audio) and the plugin watcher.
pub struct Engine;

impl Engine {
    /// Starts the engine in this order: tracing and the system hub, config,
    /// audio device, event bus, `AppState`, the desk client (if asked for),
    /// the startup showfile and the persistence thread, the supervisor, the
    /// plugin watcher. Returns `Ok(None)` without starting anything when the
    /// config file did not exist and a default one was written instead.
    ///
    /// Installs the global tracing subscriber, so call it once per process.
    pub fn start(options: EngineOptions) -> anyhow::Result<Option<EngineHandle>> {
        let EngineOptions {
            config_file,
            attach_desk,
        } = options;

        // Every system message (logs, startup stages, heartbeats, ...) goes
        // through the hub; frontends subscribe to it.
        let hub = SystemHub::new();
        crate::log::init_tracing(hub.clone());

        let Some(cfg) = config::read_config(config_file.clone())? else {
            info!(
                "Created default configuration file at {}",
                config_file.to_string_lossy()
            );
            return Ok(None);
        };

        let send_stage = |stage, status| {
            hub.publish(SystemMessage::StartupStage { stage, status });
        };
        send_stage(
            StartupStage::Config,
            StageStatus::ok(
                config_file
                    .file_name()
                    .unwrap_or(config_file.as_os_str())
                    .to_string_lossy(),
            ),
        );

        let version = env!("CARGO_PKG_VERSION");
        info!("Starting BLAULICHT {version}");

        //
        // Audio.
        //

        // Requests to the supervisor thread: from the engine (audio input and
        // plugin reload commands) and from the plugin watcher.
        let (supervisor_sender, supervisor_receiver) =
            crossbeam_channel::unbounded::<SupervisorRequest>();
        // The engine's config: the supervisor writes the selected audio input
        // into it, the persistence service the open showfile and the global
        // plugin state.
        let shared_config = SharedConfig::new(cfg.clone(), config_file.clone());

        let audio_thread_control_signal =
            Arc::new(AtomicU8::new(AudioThreadControlSignal::CONTINUE.into()));

        let initial_audio_input = match cfg.default_audio_device {
            None => None,
            Some(ref name) => {
                let input = utils::input_from_name(name.clone());
                if input.is_some() {
                    info!("Using default audio device: <{name}> from configuration file.");
                } else {
                    tracing::warn!(
                        "Configured audio device <{name}> is unavailable; selecting one automatically"
                    );
                }
                input
            }
        };

        //
        // Event bus.
        //

        let mut event_bus = SystemEventBus::<BusMessage>::new();
        // Adds connections while the bus runs (frontends attaching later).
        let event_bus_handle = event_bus.handle();

        let event_bus_connection_mainloop = event_bus_handle.connect();
        let event_bus_connection_dmx = event_bus_handle.connect();

        thread::spawn(move || {
            event_bus.run();
        });

        let app_state = Arc::new(AppState::new(&cfg.plugins));
        // Attach the desk frontend before the engine starts, so plugin requests
        // sent during startup (e.g. creating an owned external screen) reach it.
        // Its hub subscription replays the startup stages and logs sent so far.
        let desk = attach_desk.then(|| {
            LocalClient::connect(
                ClientId::Desk,
                SessionId::DESK,
                Arc::clone(&app_state),
                &hub,
                &event_bus_handle,
            )
        });
        {
            let mut global_state = app_state.plugin_state_storage_global.lock().unwrap();
            *global_state = cfg.plugin_state.clone();
        }

        // The persistence service loads the startup showfile before the engine
        // starts (so plugins find its state when they initialize), then saves,
        // loads and autosaves on its own thread.
        let mut persistence =
            PersistenceService::new(Arc::clone(&app_state), hub.clone(), shared_config.clone());
        if let Some(showfile) = &cfg.last_open_showfile {
            let name = showfile
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default();
            send_stage(StartupStage::Showfile, StageStatus::running(name.clone()));
            let loaded = persistence.open_startup(Some(showfile.clone()));
            send_stage(
                StartupStage::Showfile,
                match loaded {
                    Ok(()) => StageStatus::ok(name),
                    Err(_) => StageStatus::failed(format!("{name}: could not load, see Logs")),
                },
            );
        } else {
            // A new, empty show (still announced, so frontends start in step).
            let _ = persistence.open_startup(None);
            send_stage(StartupStage::Showfile, StageStatus::ok("none"));
        }
        let persistence = persistence
            .spawn()
            .context("Failed to start the persistence service")?;

        {
            // Audio recording and analysis thread.
            let hub = hub.clone();
            let audio_thread_control_signal = audio_thread_control_signal.clone();
            let cfg = cfg.clone();
            let shared_config = shared_config.clone();
            let supervisor_sender = supervisor_sender.clone();
            let app_state = Arc::clone(&app_state);
            thread::spawn(move || {
                mainloop::supervisor_thread(
                    supervisor_receiver,
                    supervisor_sender,
                    initial_audio_input,
                    audio_thread_control_signal,
                    hub,
                    cfg,
                    shared_config,
                    event_bus_connection_mainloop,
                    event_bus_connection_dmx,
                    app_state,
                )
            });
        }

        //
        // Filesystem plugin watcher.
        //
        let plugins = cfg.plugins.clone();
        thread::spawn(move || {
            PluginManager::watch_plugins(supervisor_sender, &plugins)
                .context("Failed to start plugin watcher")
                .unwrap();
        });

        //
        // Remote clients.
        //
        let server = if cfg.server_enabled {
            let bind = SocketAddr::from((Ipv4Addr::UNSPECIFIED, cfg.port));
            match Server::start(
                ServerOptions::new(bind),
                Arc::clone(&app_state),
                hub.clone(),
                event_bus_handle.clone(),
            ) {
                Ok(server) => {
                    info!(
                        "Remote clients can connect to ws://{}/",
                        server.local_addr()
                    );
                    Some(server)
                }
                Err(err) => {
                    tracing::error!(
                        "Remote clients can't connect: could not listen on {bind}: {err}"
                    );
                    None
                }
            }
        } else {
            info!("Remote clients are turned off (server_enabled = false)");
            None
        };

        Ok(Some(EngineHandle {
            hub,
            bus: event_bus_handle,
            app: app_state,
            persistence,
            desk,
            server,
            config_file,
        }))
    }
}

/// A running engine, held by the binary's `main`. Frontends get a
/// [`LocalClient`] from it, never the engine state itself.
pub struct EngineHandle {
    hub: SystemHub,
    bus: EventBusHandle<BusMessage>,
    app: Arc<AppState>,
    persistence: PersistenceThread,
    desk: Option<LocalClient>,
    server: Option<ServerHandle>,
    config_file: PathBuf,
}

impl EngineHandle {
    /// The desk client connected during startup ([`EngineOptions::attach_desk`]);
    /// `None` without one or once taken.
    pub fn take_desk(&mut self) -> Option<LocalClient> {
        self.desk.take()
    }

    /// Connects another client in a new operator session (closed when the
    /// client is dropped).
    pub fn connect_client(&self) -> LocalClient {
        LocalClient::connect_new_session(Arc::clone(&self.app), &self.hub, &self.bus)
    }

    /// The engine's `config.toml`.
    pub fn config_file(&self) -> &Path {
        &self.config_file
    }

    /// Where remote clients connect (`crate::server`); `None` if the server
    /// is turned off or could not listen.
    pub fn server_addr(&self) -> Option<SocketAddr> {
        self.server.as_ref().map(ServerHandle::local_addr)
    }

    /// Set once a client's `HostCommand::Quit` / `Restart` ran the shutdown
    /// hook; the binary then calls [`EngineHandle::shutdown`] and exits.
    pub fn exit_request(&self) -> Option<ExitRequest> {
        self.app.persistence.exit_request()
    }

    /// Lets the persistence service finish its queued work (resetting the
    /// grand master if no `Quit` / `Restart` ran), then stops the supervisor
    /// and with it the main loop. Returns how the process should exit
    /// ([`ExitRequest::code`]); `None` means no client asked to quit.
    pub fn shutdown(self) -> Option<ExitRequest> {
        // No remote commands from here on.
        if let Some(server) = self.server {
            server.stop();
        }
        let exit = self.persistence.stop();
        self.hub.shutdown();
        exit
    }
}
