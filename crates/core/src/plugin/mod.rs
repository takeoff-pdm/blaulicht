use std::{
    cell::{RefCell, UnsafeCell}, collections::HashMap, hash::Hash, path::{Path, PathBuf}, rc::Rc, sync::{mpsc, Arc, Mutex}, time::{Duration, Instant}
};

use anyhow::Context;
use blaulicht_shared::CollectedAudioSnapshot;
use crossbeam_channel::{Receiver, RecvTimeoutError, Sender};
use log::{debug, trace};
use notify::{
    event::{DataChange, ModifyKind},
    Config, Error, Event, EventKind, FsEventWatcher, RecommendedWatcher, RecursiveMode, Watcher,
};
use wasmtime::{Instance, Memory, Store};

use crate::{
    app::{FromFrontend, MidiEvent},
    config::PluginConfig,
    msg::SystemMessage,
    plugin::{
        midi::MidiManager, wasm::MidiStatus
    },
};

pub mod midi;
mod tick;
mod wasm;

pub struct PluginManager {
    timer_start: Instant,
    is_initial_tick: bool,
    plugin_config: Vec<PluginConfig>,
    plugins: HashMap<String, Plugin>,
    // Channels.
    system_out: Sender<SystemMessage>,
    to_midi_devices: Sender<MidiEvent>,
    from_midi_manager: Receiver<MidiEvent>,

    // todo: this is completely borked; the most intelligent way to do this is to put the midi manager into the plugin manager!
    midi_manager_ref: Arc<Mutex<MidiManager>>,
}

pub struct Plugin {
    path: String,
    memory: Memory,
    store: Store<()>,
    instance: Instance,
    // DANGER: this is not always populated.
    midi_status: MidiStatus,
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
    ) -> Self {
        Self {
            timer_start: Instant::now(),
            is_initial_tick: true,
            plugin_config,
            plugins: HashMap::new(),
            to_midi_devices: to_midi_manager,
            from_midi_manager,
            system_out,
            midi_manager_ref,
        }
    }

    pub fn init(&mut self) -> anyhow::Result<()> {
        self.instantiate_plugins()?;
        self.is_initial_tick = true;
        self.tick(CollectedAudioSnapshot::default(), &[])?;

        Ok(())
    }

    pub fn reload(&mut self) -> anyhow::Result<()> {
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
            watcher.watch(file, RecursiveMode::NonRecursive)?;
            debug!("Watching file: {:?}", file);
        }

        println!("Watching files...");

        // Process events
        loop {
            match rx.recv_timeout(Duration::from_secs(1)) {
                Ok(event) => {
                    if let Ok(event) = event {
                        if !matches!(
                            event.kind,
                            EventKind::Modify(ModifyKind::Data(DataChange::Content))
                        ) {
                            // Only handle data changes.
                            continue;
                        }

                        println!(
                            "Change detected in: {:?} (kind: {:?}) ----> RELOADING...",
                            event.paths, event.kind
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
