use anyhow::{anyhow, Context};
use blaulicht_shared::CollectedAudioSnapshot;
use crossbeam_channel::{Receiver, Sender};
use notify::{
    event::{DataChange, ModifyKind},
    EventKind, RecommendedWatcher, RecursiveMode, Watcher,
};
use std::{
    borrow::Cow,
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::{mpsc, Arc, Mutex},
    time::{Duration, Instant},
};

#[cfg(feature = "wasmtime")]
use wasmtime::{Instance, Memory, Store, TypedFunc};

use crate::{
    config::PluginConfig,
    event::SystemEventBusConnectionInst,
    msg::{FromFrontend, MidiEvent, SystemMessage},
    plugin::{midi::MidiManager, serial::SerialManager, udp::UdpManager, wasm::AddrDescriptor},
    state::AppState,
};

pub mod midi;
pub mod serial;
pub(crate) mod tick;
pub mod udp;
pub(crate) mod wasm;

pub struct PluginManager {
    // true if TODO
    has_crashed: bool,

    timer_start: Instant,
    is_initial_tick: bool,
    plugin_config: Vec<PluginConfig>,
    plugins: HashMap<u8, Plugin>,
    animation_instance_ids: HashMap<u8, HashSet<u64>>,
    // Channels.
    system_out: Sender<SystemMessage>,
    to_midi_devices: Sender<MidiEvent>,
    from_midi_manager: Receiver<MidiEvent>,

    // todo: this is completely borked; the most intelligent way to do this is to put the midi manager into the plugin manager!
    midi_manager_ref: Arc<Mutex<MidiManager>>,
    serial_manager_ref: Arc<Mutex<SerialManager>>,
    udp_manager_ref: Arc<Mutex<UdpManager>>,

    event_bus: SystemEventBusConnectionInst,

    state_ref: Arc<AppState>,
}

#[cfg(not(feature = "wasmtime"))]
pub struct PluginWasmState {}

#[cfg(feature = "wasmtime")]
pub struct PluginWasmState {
    memory: Memory,
    store: Store<()>,
    instance: Instance,
}

pub struct Plugin {
    path: Cow<'static, str>,

    #[cfg(feature = "wasmtime")]
    wasm_state: PluginWasmState,

    #[cfg(feature = "wasmtime")]
    tick_func: TypedFunc<(i32, i32), ()>,

    // DANGER: this is not always populated.
    midi_buffers: AddrDescriptor,
    serial_buffers: AddrDescriptor,
    state_buffers: AddrDescriptor,
    udp_buffers: AddrDescriptor,
    animation_output_buffers: AddrDescriptor,

    // When was the last time the engine state was written into that plugin?
    last_dmx_engine_sync: Instant,
}

impl Plugin {
    pub fn new(path: Cow<'static, str>, wasm_state: PluginWasmState) -> anyhow::Result<Self> {
        Self::new_internal(path, wasm_state)
    }

    #[cfg(feature = "wasmtime")]
    fn new_internal(
        path: Cow<'static, str>,
        mut wasm_state: PluginWasmState,
    ) -> anyhow::Result<Self> {
        let tick_func = wasm_state.instance.get_typed_func::<(i32, i32), ()>(
            &mut wasm_state.store,
            "internal_tick", // TODO: external type and name constants.
        )?;

        Ok(Self {
            path,
            wasm_state,
            tick_func,
            midi_buffers: AddrDescriptor::dummy(),
            state_buffers: AddrDescriptor::dummy(),
            serial_buffers: AddrDescriptor::dummy(),
            udp_buffers: AddrDescriptor::dummy(),
            animation_output_buffers: AddrDescriptor::dummy(),
            last_dmx_engine_sync: Instant::now(),
        })
    }

    #[cfg(not(feature = "wasmtime"))]
    fn new_internal(
        path: Cow<'static, str>,
        mut wasm_state: PluginWasmState,
    ) -> anyhow::Result<Self> {
        Ok(Self {
            path,
            midi_buffers: AddrDescriptor::dummy(),
            state_buffers: AddrDescriptor::dummy(),
            serial_buffers: AddrDescriptor::dummy(),
            udp_buffers: AddrDescriptor::dummy(),
            animation_output_buffers: AddrDescriptor::dummy(),
            last_dmx_engine_sync: Instant::now(),
        })
    }
}

pub struct PluginReloadRequest {
    pub file_path: PathBuf,
}

impl PluginManager {
    pub fn new(
        plugin_config: Vec<PluginConfig>,
        to_midi_manager: Sender<MidiEvent>,
        from_midi_manager: Receiver<MidiEvent>,
        system_out: Sender<SystemMessage>,
        midi_manager_ref: Arc<Mutex<MidiManager>>,
        serial_manager_ref: Arc<Mutex<SerialManager>>,
        udp_manager_ref: Arc<Mutex<UdpManager>>,
        event_bus: SystemEventBusConnectionInst,
        app_state_ref: Arc<AppState>,
    ) -> Self {
        Self {
            timer_start: Instant::now(),
            has_crashed: false,
            is_initial_tick: true,
            plugin_config,
            plugins: HashMap::new(),
            animation_instance_ids: HashMap::new(),
            to_midi_devices: to_midi_manager,
            from_midi_manager,
            system_out,
            midi_manager_ref,
            serial_manager_ref,
            udp_manager_ref,
            event_bus,
            state_ref: app_state_ref,
        }
    }

    pub fn load_plugin_states(&mut self, plugin_state: HashMap<String, String>) {
        let mut storage = self.state_ref.plugin_state_storage.lock().unwrap();
        *storage = plugin_state;
    }

    pub fn active_plugins(&self) -> Vec<u8> {
        let plugins = self.state_ref.plugins.read().unwrap();
        plugins
            .iter()
            .filter(|(_, p)| p.is_enabled() && !p.has_errored())
            .map(|(key, _)| *key)
            .collect()
    }

    pub fn init(&mut self) -> anyhow::Result<()> {
        if let Err(err) = self.instantiate_plugins() {
            let plugin_error_list: HashMap<u8, anyhow::Error> = self
                .state_ref
                .plugins
                .read()
                .unwrap()
                .keys()
                .map(|plugin_id| {
                    (
                        *plugin_id,
                        anyhow!("Engine initialization log::error: {err}"),
                    )
                })
                .collect();

            self.disable_errored_plugins(plugin_error_list);

            return Err(err);
        };

        self.is_initial_tick = true;

        // Ignore any tick log::errors caused by misbehaving plugins.
        // Only return on serious log::errors.
        tracing::debug!("[Wasm] Running initial tick...");
        if self
            .tick(CollectedAudioSnapshot::default(), &[], vec![], vec![], None)
            .is_err()
        {
            tracing::error!("Plugin(s) failed to initialize.");
        };

        Ok(())
    }

    pub fn reload(&mut self) -> anyhow::Result<()> {
        {
            let mut artnet_output = self.state_ref.artnet_output.write().unwrap();
            artnet_output.remove_all_plugin_receivers();
        }

        {
            let mut spawned_commands = self.state_ref.spawned_commands.lock().unwrap();
            spawned_commands.remove_all();
        }

        {
            // Reset all plugins which got temporarily disabled due to log::errors.
            let mut plugins = self.state_ref.plugins.write().unwrap();
            for (_, plug) in plugins.iter_mut() {
                plug.set_errored(false);
            }
        }

        self.init().with_context(|| "Failed to reload plugins")
    }

    /// Associated method to start the watcher.
    /// Should be started in a main thread since the actual watcher instance is re-instantiated once a change occurs.
    pub fn watch_plugins(
        from_frontend_sender: Sender<FromFrontend>,
        plugin_configuration_list: &[PluginConfig],
    ) -> anyhow::Result<!> {
        // Channel to receive events
        let (tx, rx) = mpsc::channel();

        // Create a watcher
        let mut watcher: RecommendedWatcher = Watcher::new(tx, notify::Config::default())?;

        let files_to_watch: Vec<PathBuf> = plugin_configuration_list
            .iter()
            .filter(|plugin| plugin.enable_watcher)
            .map(|plugin| PathBuf::from(&plugin.file_path))
            .collect();

        // Watch each file of the plugins to watch.
        for file in &files_to_watch {
            if let Err(err) = watcher.watch(file, RecursiveMode::NonRecursive) {
                tracing::error!("Watching file: {:?} failed: {}", file, err);
            } else {
                tracing::debug!("Watching file: {:?}", file);
            }
        }

        tracing::debug!("Watching {} files...", files_to_watch.len());

        // Process events
        loop {
            match rx.recv_timeout(Duration::from_secs(1)) {
                Ok(event) => {
                    if let Ok(event) = event {
                        tracing::trace!("Plugin file change event: {:?}", event);
                        if !matches!(
                            event.kind,
                            EventKind::Modify(ModifyKind::Data(DataChange::Any))
                        ) {
                            // Only handle data changes.
                            continue;
                        }

                        tracing::info!(
                            "Change detected in: {:?} (kind: {:?}) ----> RELOADING...",
                            event.paths,
                            event.kind
                        );
                        from_frontend_sender
                            .send(FromFrontend::Reload)
                            .with_context(|| "Failed to send reload request")?;
                    }
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => panic!("Watcher channel disconnected"),
                Err(mpsc::RecvTimeoutError::Timeout) => {}
            }
        }
    }
}
