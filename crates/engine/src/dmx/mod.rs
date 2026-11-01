/// This module deals with applying events on fixtures to produce a continuous DMX output.
mod management;
mod performance_commands;
mod session;
mod setup_commands;
mod stage_commands;
mod state;
mod system_commands;

use artnet_protocol::ArtCommand;
use blaulicht_audio_engine::CollectorOutput;
pub use performance_commands::SCENE_GRAPH_CANVAS_HALF_EXTENT;
use serialport::SerialPort;
pub use session::*;
pub use stage_commands::StageState;
pub use state::*;
pub mod animation;
pub mod scene;

use crate::{
    event::SystemEventBusConnectionInst,
    hub::{
        BusBody, BusMessage, ClientId, CommandOutcome, CommandResult, CreatedId, HostCommand,
        SessionChange, SessionId, SystemHub,
    },
    msg::{DmxTickSpeeds, SystemMessage},
    state::{AppState, DmxHealth, NUM_DMX_UNIVERSES},
};
use animation::templates;
use blaulicht_shared::{
    fixture::{
        state::{FixtureState, MergeStrategy, ResolvedFixtureState},
        value::FixtureValue,
    },
    scene::{FixtureSelection, FixtureSelector, BLANK_SCENE_ID},
    scene_graph::{AudioConditions, SceneGraphRuntime},
    ActiveAnimation, AnimationPreset, AnimationSpecBody, ControlEvent, ControlEventMessage,
    EventOriginator, FixtureProperty, CONTROLS_REQUIRING_SELECTION,
};
use crossbeam_channel::{bounded, Receiver, Sender, TrySendError};
use std::{
    cell::{Cell, RefCell},
    collections::{BTreeMap, HashMap, HashSet},
    io::ErrorKind,
    mem,
    net::{SocketAddr, UdpSocket},
    sync::{Arc, PoisonError, RwLockWriteGuard},
    time::Duration,
};
use tracing::{debug, error, info, trace, warn};
use web_time::Instant;

// TODO: maybe fuse this together?

pub struct DmxEngineArtnetOutput {
    universe_buffers: [Vec<u8>; NUM_DMX_UNIVERSES],
    /// One non-blocking socket per destination. The kernel charges packets
    /// waiting for ARP resolution (unreachable receiver) to the sending
    /// socket's buffer; a shared blocking socket then stalls every
    /// destination and the serial DMX output for the ARP timeout (~1 s).
    sockets: HashMap<SocketAddr, UdpSocket>,
}

impl DmxEngineArtnetOutput {
    fn new() -> Self {
        Self {
            universe_buffers: std::array::from_fn(|_| vec![0; 512]),
            sockets: HashMap::new(),
        }
    }

    fn bind_socket() -> std::io::Result<UdpSocket> {
        let socket = UdpSocket::bind("0.0.0.0:0")?;
        socket.set_nonblocking(true)?;
        Ok(socket)
    }

    /// Returns the socket for `destination`, binding one on first use.
    fn socket_for(&mut self, destination: SocketAddr) -> Option<&UdpSocket> {
        if !self.sockets.contains_key(&destination) {
            match Self::bind_socket() {
                Ok(socket) => {
                    self.sockets.insert(destination, socket);
                }
                Err(err) => {
                    error!("Could not create Art-Net socket for {destination}: {err}");
                    return None;
                }
            }
        }
        self.sockets.get(&destination)
    }
}

pub struct DmxEngine {
    // TODO: check if this is too slow.
    state_ref: Arc<AppState>,
    dmx_previous: [u8; 513], // Starting at 1
    // TODO: add more, internal state.
    event_bus_connection: SystemEventBusConnectionInst,
    /// Replies ([`CommandResult`]) go to the sender's hub subscription.
    hub: SystemHub,
    /// Set by `apply` when it allocates an id (e.g. `CreatePalette`), so the
    /// reply can carry it. Reset before each bus message.
    created: Cell<Option<CreatedId>>,

    // This is not part of state_ref since this is only a cache
    // animation_base_times: BTreeMap<u8, f64>,
    start_time: Instant,

    output_tx: Sender<DmxOutputFrame>,

    running_setup: bool,
    setup_start_time: Instant,
    /// Per-fixture setup runs requested from the UI ("Initialize"): the fixture
    /// key and when the run started. Overlaid on the normal render for
    /// `SETUP_SECS`, so the rest of the show keeps playing.
    fixture_inits: Vec<((u8, u8), Instant)>,
    /// Fixtures `HostCommand::InitFixtures` asked to initialize since the
    /// last frame; moved into `fixture_inits` after rendering.
    pending_fixture_inits: RefCell<Vec<(u8, u8)>>,

    scene_graph_runtime: SceneGraphRuntime,
    animation_clock: animation::AnimationClockRuntime,
    /// Scene IDs the scene graph pushed into `current_overlay_scenes` last tick.
    /// Used to reconcile (add/remove) graph-driven overlays without clobbering
    /// manually-added ones. Runtime-only, not persisted.
    graph_overlay_scenes: Vec<u8>,
    /// Where audio input and plugin reload commands are forwarded
    /// (`with_supervisor`); `None` in a headless test engine.
    supervisor: Option<Sender<crate::mainloop::SupervisorRequest>>,
}

#[derive(Clone, Copy)]
struct DmxOutputFrame {
    universes: [[u8; 513]; NUM_DMX_UNIVERSES],
}

fn output_worker(
    receiver: Receiver<DmxOutputFrame>,
    mut ports: [Option<Box<dyn SerialPort>>; 2],
    mut artnet: DmxEngineArtnetOutput,
    state_ref: Arc<AppState>,
) {
    let retry_interval = Duration::from_secs(2);
    let mut retry_at = Instant::now();
    while let Ok(frame) = receiver.recv() {
        let retry_due = Instant::now() >= retry_at;
        if retry_due {
            retry_at = Instant::now() + retry_interval;
            for (universe_no, slot) in ports.iter_mut().enumerate() {
                if slot.is_some() {
                    continue;
                }
                let path = state_ref
                    .health_data
                    .read()
                    .ok()
                    .map(|health| health.dmx_universes_healthy[universe_no].port.clone())
                    .unwrap_or_default();
                if path.is_empty() {
                    continue;
                }
                if let Ok(port) = serialport::new(&path, 250_000)
                    .data_bits(serialport::DataBits::Eight)
                    .parity(serialport::Parity::None)
                    .stop_bits(serialport::StopBits::Two)
                    .timeout(Duration::from_millis(10))
                    .open()
                {
                    *slot = Some(port);
                    if let Ok(mut health) = state_ref.health_data.write() {
                        health.dmx_universes_healthy[universe_no] = DmxHealth::healthy(path);
                    }
                }
            }
        }
        // `(address, first_universe, last_universe)` per enabled receiver.
        let receivers: Vec<(SocketAddr, u8, u8)> = state_ref
            .artnet_output
            .read()
            .map(|output| {
                output
                    .receivers
                    .iter()
                    .filter(|destination| destination.enabled)
                    .map(|destination| {
                        (
                            destination.address,
                            destination.first_universe,
                            destination.last_universe,
                        )
                    })
                    .collect()
            })
            .unwrap_or_default();
        // Drop sockets of removed receivers so their queued packets are freed.
        artnet
            .sockets
            .retain(|address, _| receivers.iter().any(|(kept, ..)| kept == address));
        let mut artnet_failed = false;
        for (universe_no, universe) in frame.universes.iter().enumerate() {
            let wants_universe = |&(_, first, last): &(SocketAddr, u8, u8)| {
                (first as usize..=last as usize).contains(&universe_no)
            };
            // Skip serialising universes no receiver is subscribed to.
            if !receivers.iter().any(wants_universe) {
                continue;
            }
            artnet.universe_buffers[universe_no].copy_from_slice(&universe[1..]);
            let command = ArtCommand::Output(artnet_protocol::Output {
                port_address: (universe_no as u8).into(),
                physical: universe_no as u8,
                data: artnet.universe_buffers[universe_no].clone().into(),
                ..artnet_protocol::Output::default()
            });
            let Ok(bytes) = command.write_to_buffer() else {
                error!("Failed to serialize Art-Net output for universe {universe_no}");
                continue;
            };
            for &(destination, ..) in receivers.iter().filter(|r| wants_universe(r)) {
                let Some(socket) = artnet.socket_for(destination) else {
                    artnet_failed = true;
                    continue;
                };
                if let Err(err) = socket.send_to(&bytes, destination) {
                    // WouldBlock: the socket buffer is full of packets the
                    // kernel cannot deliver (unresolved ARP). Skip this frame
                    // for this destination instead of waiting for the kernel.
                    if retry_due && universe_no == 0 && err.kind() != ErrorKind::WouldBlock {
                        error!("Send ArtNet UDP to {destination}: {err:?}");
                    }
                    artnet_failed = true;
                }
            }
        }
        // A failed destination must not prevent sending to the others, and a
        // transient send error does not invalidate a UDP socket.
        if let Ok(mut health) = state_ref.health_data.write() {
            health.artnet_health_state = !artnet_failed;
        }

        for (universe_no, port_slot) in ports.iter_mut().enumerate() {
            let Some(mut port) = port_slot.take() else {
                continue;
            };
            let result = (|| -> anyhow::Result<()> {
                port.set_break()?;
                spin_sleep::sleep(Duration::from_micros(100));
                port.clear_break()?;
                spin_sleep::sleep(Duration::from_micros(12));
                port.write_all(&frame.universes[universe_no])?;
                Ok(())
            })();
            if let Err(err) = result {
                let port_path = state_ref
                    .health_data
                    .read()
                    .ok()
                    .and_then(|health| {
                        health
                            .dmx_universes_healthy
                            .get(universe_no)
                            .map(|state| state.port.clone())
                    })
                    .unwrap_or_else(|| format!("universe {universe_no}"));
                error!("Output disabled on {port_path}: {err}");
                if let Ok(mut health) = state_ref.health_data.write() {
                    health.dmx_universes_healthy[universe_no] =
                        DmxHealth::error(port_path, format!("DMX output disabled: {err}"));
                }
            } else {
                *port_slot = Some(port);
            }
        }
    }
}

const SETUP_SECS: u64 = 10;

/// Why a desk-only command (`HostCommand::is_desk_only`) from another client
/// is refused.
pub const DESK_ONLY_REFUSAL: &str = "Only the desk frontend can send this command";

impl DmxEngine {
    pub fn start_setup(&mut self) {
        self.running_setup = true;
        self.setup_start_time = Instant::now();
        debug!("Fixture setup started");
    }

    fn open_hw_interface(port_path: &str) -> (Option<Box<dyn SerialPort>>, DmxHealth) {
        // TODO: use USB intrinsics for detection: look at v1 branch

        // Open your Enttec device (likely /dev/ttyUSB0)
        let baud_rate = 250_000;

        match serialport::new(port_path, baud_rate)
            .data_bits(serialport::DataBits::Eight)
            .parity(serialport::Parity::None)
            .stop_bits(serialport::StopBits::Two)
            .timeout(Duration::from_millis(10))
            .open()
        {
            Ok(port) => {
                info!("Opened DMX interface {port_path} (baud = {baud_rate})");
                (Some(port), DmxHealth::healthy(port_path.to_string()))
            }
            Err(err) => {
                let error_message = format!(
                    "Could not establish link to DMX interface \"{port_path}\" (baud = {baud_rate}): {err}"
                );
                error!("{error_message}");
                (None, DmxHealth::error(port_path.to_string(), error_message))
            }
        }
    }

    pub fn new(
        state_ref: Arc<AppState>,
        event_bus_connection: SystemEventBusConnectionInst,
        hub: SystemHub,
        universe_dmx_out_devices: [String; 2],
    ) -> Self {
        let mut health_state = state_ref.health_data.write().unwrap();

        //
        // Initialize DMX.
        //
        let mut dmx_universe_ports = [None, None];

        for (universe, port) in dmx_universe_ports.iter_mut().enumerate() {
            let (dmx_port, port_health_state) =
                Self::open_hw_interface(&universe_dmx_out_devices[universe]);

            health_state.dmx_universes_healthy[universe] = port_health_state;

            *port = dmx_port
        }

        //
        // Initialize ArtNet.
        //
        // Sockets are bound per destination inside the output worker; here we
        // only verify that UDP sockets can be created at all.
        match DmxEngineArtnetOutput::bind_socket() {
            Ok(_) => {
                health_state.artnet_health_state = true;
                debug!("Initialized Art-Net");
            }
            Err(err) => {
                health_state.artnet_health_state = false;
                error!("Could not create Art-Net socket: {err}");
            }
        }
        let artnet_output = DmxEngineArtnetOutput::new();

        mem::drop(health_state);

        let (output_tx, output_rx) = bounded(2);
        let output_state = Arc::clone(&state_ref);
        let worker_result = std::thread::Builder::new()
            .name("dmx-output".to_string())
            .spawn(move || {
                output_worker(output_rx, dmx_universe_ports, artnet_output, output_state)
            });
        if let Err(err) = worker_result {
            error!("Failed to start output worker: {err}");
        }

        {
            let mut engine_state = state_ref.dmx_engine.write().unwrap();
            let _ = engine_state.0.validate_palette_mapping_integrity(true);
        }

        Self {
            state_ref,
            dmx_previous: [0; 513],
            event_bus_connection,
            hub,
            created: Cell::new(None),
            // animation_base_times: BTreeMap::new(),
            start_time: Instant::now(),
            output_tx,
            running_setup: false,
            setup_start_time: Instant::now(),
            fixture_inits: Vec::new(),
            pending_fixture_inits: RefCell::new(Vec::new()),
            scene_graph_runtime: SceneGraphRuntime::default(),
            animation_clock: animation::AnimationClockRuntime::default(),
            graph_overlay_scenes: Vec::new(),
            supervisor: None,
        }
    }

    /// Forwards `HostCommand::SelectAudioInput` and `ReloadPlugins` to the
    /// supervisor thread behind `supervisor`.
    pub fn with_supervisor(
        mut self,
        supervisor: Sender<crate::mainloop::SupervisorRequest>,
    ) -> Self {
        self.set_supervisor(supervisor);
        self
    }

    pub(crate) fn set_supervisor(
        &mut self,
        supervisor: Sender<crate::mainloop::SupervisorRequest>,
    ) {
        self.supervisor = Some(supervisor);
    }

    fn tick_internal(&mut self, audio_output: &CollectorOutput) {
        // Clear DMX buffers (slow).
        for buffer in self.state_ref.dmx_universes.iter() {
            let mut buffer = buffer.write().unwrap();
            buffer.dmx_buffer.fill(0);
        }

        if self.running_setup {
            self.run_setup();

            if self.setup_start_time.elapsed().as_secs() >= SETUP_SECS {
                self.running_setup = false;
                info!("Fixture setup complete");
            }

            return;
        }

        self.apply_bus_messages();

        // Scene graph activations can write scene speed/alpha controls, so they
        // must be applied before this frame's animation phase is evaluated.
        self.scene_graph_tick(&audio_output.snapshot);

        // Advance animations.
        self.animation_tick(audio_output);

        self.render_universes();
        self.run_fixture_inits();
    }

    /// Opens and closes the sessions the hub registered, applies every queued
    /// bus message, then sends each sender its [`CommandResult`].
    fn apply_bus_messages(&mut self) {
        let mut messages = vec![];
        while let Some(message) = self.event_bus_connection.try_recv() {
            messages.push(message)
        }
        // After draining the bus, so every session these messages use is
        // known (see `SystemHub::take_session_changes`).
        let session_changes = self.hub.take_session_changes();
        if messages.is_empty() && session_changes.is_empty() {
            return;
        }

        let mut results = Vec::with_capacity(messages.len());
        {
            let mut state = self.state_ref.dmx_engine.write().unwrap();

            // Open first and close last, so a client that sends and then
            // disconnects within one tick still gets its messages applied.
            for change in &session_changes {
                if let SessionChange::Opened(session) = *change {
                    state.open_session(session);
                }
            }
            for message in messages {
                results.extend(self.dispatch(&mut state, message));
            }
            for change in &session_changes {
                if let SessionChange::Closed(session) = *change {
                    state.close_session(session);
                }
            }

            let integrity = state.0.validate_palette_mapping_integrity(false);
            debug_assert!(
                integrity.is_ok(),
                "palette mapping integrity violated after applying events: {:?}",
                integrity
            );
        }
        // Outside the engine lock.
        for result in results {
            self.hub
                .publish_to(result.client, SystemMessage::CommandResult(result));
        }
    }

    /// Runs the `setup` routine of every fixture with a pending or active
    /// "Initialize" request on top of the rendered frame.
    fn run_fixture_inits(&mut self) {
        for key in std::mem::take(self.pending_fixture_inits.get_mut()) {
            match self.fixture_inits.iter_mut().find(|(k, _)| *k == key) {
                Some((_, started)) => *started = Instant::now(),
                None => self.fixture_inits.push((key, Instant::now())),
            }
        }
        if self.fixture_inits.is_empty() {
            return;
        }

        let state = self.state_ref.dmx_engine.read().unwrap();
        let mut finished = Vec::new();
        for ((group_id, fixture_id), started) in &self.fixture_inits {
            let key = (*group_id, *fixture_id);
            let Some(fix) = state
                .0
                .groups
                .get(group_id)
                .and_then(|group| group.fixtures.get(fixture_id))
            else {
                finished.push(key);
                continue;
            };
            let elapsed = started.elapsed();
            if elapsed.as_secs() >= SETUP_SECS {
                finished.push(key);
                info!("Initialized fixture `{}`", fix.name);
                continue;
            }
            let Some(universe) = self.state_ref.dmx_universes.get(fix.universe_no) else {
                finished.push(key);
                continue;
            };
            let mut buffer = universe.write().unwrap();
            fix.setup(
                elapsed.as_millis() as i32,
                &ResolvedFixtureState::default(),
                &mut buffer.dmx_buffer,
            );
        }
        mem::drop(state);
        self.fixture_inits
            .retain(|(key, _)| !finished.contains(key));
    }

    /// The fixtures whose setup routine runs (or is queued to run).
    #[cfg(test)]
    pub(crate) fn initializing_fixtures(&self) -> Vec<(u8, u8)> {
        let mut fixtures: Vec<_> = self.fixture_inits.iter().map(|(key, _)| *key).collect();
        fixtures.extend(self.pending_fixture_inits.borrow().iter().copied());
        fixtures
    }

    pub fn tick(&mut self, audio_snapshot: &CollectorOutput) -> DmxTickSpeeds {
        let dmx_engine = {
            let now = Instant::now();
            self.tick_internal(audio_snapshot);
            now.elapsed()
        };

        let dmx_write = self.submit_output_frame();

        DmxTickSpeeds {
            dmx_engine,
            dmx_write,
        }
    }

    fn submit_output_frame(&self) -> Duration {
        let now = Instant::now();
        let mut frame = DmxOutputFrame {
            universes: [[0; 513]; NUM_DMX_UNIVERSES],
        };
        for (index, universe) in self.state_ref.dmx_universes.iter().enumerate() {
            if let Ok(buffer) = universe.read() {
                frame.universes[index] = buffer.dmx_buffer;
            }
        }
        match self.output_tx.try_send(frame) {
            Ok(()) | Err(TrySendError::Full(_)) => {}
            Err(TrySendError::Disconnected(_)) => error!("Output worker is unavailable"),
        }
        now.elapsed()
    }

    fn run_setup(&mut self) {
        let state = self.state_ref.dmx_engine.read().unwrap();

        for group in &state.0.groups {
            for fixture in &group.1.fixtures {
                let fix = fixture.1;

                let time = self.setup_start_time.elapsed().as_millis() as u64;

                trace!("Fixture setup at {time} ms");

                let Some(universe) = self.state_ref.dmx_universes.get(fix.universe_no) else {
                    warn!(
                        "Skipping fixture {}: universe {} is out of range",
                        fix.name, fix.universe_no
                    );
                    continue;
                };
                let mut buffer = universe.write().unwrap();

                fix.setup(
                    time as i32,
                    &ResolvedFixtureState::default(),
                    &mut buffer.dmx_buffer,
                );
            }
        }

        mem::drop(state);
    }

    fn scene_graph_tick(&mut self, audio_snapshot: &blaulicht_shared::CollectedAudioSnapshot) {
        let now_ms = self.start_time.elapsed().as_millis() as u64;
        let audio = AudioConditions {
            beat_active: audio_snapshot.bass > audio_snapshot.bass_avg,
            beat_trigger: audio_snapshot.beat_trigger,
            section_state: audio_snapshot.section_state,
        };
        let prev_graph_scenes = mem::take(&mut self.graph_overlay_scenes);
        let mut state = self.state_ref.dmx_engine.write().unwrap();
        let activated_nodes =
            self.scene_graph_runtime
                .tick_all(&mut state.0.scene_graphs, now_ms, &audio);

        // Node controls are write-through shortcuts. Apply them once on activation;
        // they remain until another node or a manual control replaces them.
        let base = state.render_base_scene();
        apply_scene_graph_node_controls(&mut state.0, base, &activated_nodes);

        // Reflect the scenes of every enabled graph's active node into the normal
        // overlay list, so they show up in the performance view and render through
        // the standard overlay path (no separate scene-graph render path).
        let mut new_graph_scenes: Vec<u8> = state
            .0
            .scene_graphs
            .collect_active_scene_overrides()
            .into_iter()
            .map(|o| o.scene_id)
            .filter(|id| *id != base && state.0.scenes.contains_key(id))
            .collect();
        new_graph_scenes.sort_unstable();
        new_graph_scenes.dedup();

        // Remove overlays the graph added last tick but no longer wants; keep
        // manually-added overlays (those not in the previous graph set) untouched.
        state
            .0
            .current_overlay_scenes
            .retain(|id| !prev_graph_scenes.contains(id) || new_graph_scenes.contains(id));
        for id in &new_graph_scenes {
            if !state.0.current_overlay_scenes.contains(id) {
                state.0.current_overlay_scenes.push(*id);
            }
        }

        mem::drop(state);
        self.graph_overlay_scenes = new_graph_scenes;
    }

    fn render_universes(&mut self) {
        let state = self.state_ref.dmx_engine.read().unwrap();
        let palettes = &state.0.palettes;
        let base_scene_id = state.render_base_scene();
        let Some(base_scene) = state.0.scenes.get(&base_scene_id) else {
            // `curr_scene()` would unwrap here and take the DMX thread with it.
            warn!("Skipping render: base scene {base_scene_id} is missing");
            return;
        };
        let rendered_overlays = state.0.rendered_overlays();

        // For each fixture, merge all scene states.
        for group in &state.0.groups {
            for fixture in &group.1.fixtures {
                let curr_scene = base_scene;

                // Apply base scene state.
                let fixture_key = (*group.0, *fixture.0);
                let Some(base_fixture_state) = curr_scene.sink.fixture_states.get(&fixture_key)
                else {
                    warn!("Skipping fixture {fixture_key:?}: missing state in base scene");
                    continue;
                };
                let mut merged_state = base_fixture_state.clone();

                // Apply palette assignments for this fixture in the base scene.
                if let Some(palette_ids) = curr_scene.sink.palette_assignments.get(&fixture_key) {
                    for palette_id in palette_ids {
                        if let Some(palette) = state.0.palettes.get(palette_id) {
                            palette.kind.apply_to(&mut merged_state, &state.0.palettes);
                        }
                    }
                }

                // Animation output modulates the authored/palette-resolved
                // base before any scene merging happens.
                self.animation_clock.outputs.apply_to_fixture(
                    &mut merged_state,
                    base_scene_id,
                    fixture_key,
                    palettes,
                );

                // Resolve to a concrete state, then apply master alpha.
                let mut resolved = merged_state.resolve(palettes);
                resolved.alpha = ((resolved.alpha as f32 / 100.0)
                    * curr_scene.sink.master_alpha_fader as f32)
                    as u8;

                for overlay_id in rendered_overlays {
                    let Some(this_scene) = state.0.scenes.get(overlay_id) else {
                        warn!("Skipping missing overlay scene {overlay_id}");
                        continue;
                    };

                    if this_scene.sink.master_alpha_fader == 0 {
                        // Skip scene - it is disabled.
                        continue;
                    }

                    let Some(overlay_fixture_state) =
                        this_scene.sink.fixture_states.get(&fixture_key)
                    else {
                        warn!("Skipping fixture {fixture_key:?}: missing state in overlay scene {overlay_id}");
                        continue;
                    };
                    let mut scene_fixture_state = overlay_fixture_state.clone();

                    // Apply palette assignments for overlay scene.
                    if let Some(palette_ids) = this_scene.sink.palette_assignments.get(&fixture_key)
                    {
                        for palette_id in palette_ids {
                            if let Some(palette) = state.0.palettes.get(palette_id) {
                                palette
                                    .kind
                                    .apply_to(&mut scene_fixture_state, &state.0.palettes);
                            }
                        }
                    }

                    self.animation_clock.outputs.apply_to_fixture(
                        &mut scene_fixture_state,
                        *overlay_id,
                        fixture_key,
                        palettes,
                    );

                    // Apply master alpha as a literal scaling on the overlay's
                    // resolved alpha before merging.
                    {
                        let resolved_alpha = scene_fixture_state
                            .alpha
                            .resolve(palettes, FixtureProperty::Alpha)
                            as f32;
                        let scaled = ((resolved_alpha / 100.0)
                            * this_scene.sink.master_alpha_fader as f32)
                            as u16;
                        scene_fixture_state.alpha = FixtureValue::Literal(scaled);
                    }

                    let changeset = this_scene.get_fixture_changeset(*group.0, *fixture.0);
                    // Build a merged FixtureState for property-by-property merge,
                    // starting from the current resolved snapshot as literals.
                    let mut merged_for_overlay: FixtureState = resolved.clone().into();
                    for change in changeset {
                        merged_for_overlay.merge_from(
                            &scene_fixture_state,
                            change,
                            MergeStrategy::for_overlay_property(change),
                            palettes,
                        );
                    }
                    resolved = merged_for_overlay.resolve(palettes);
                }

                resolved.alpha =
                    apply_grand_master(resolved.alpha, self.state_ref.grand_master_percent());

                // TODO: we will need to use the merged fixture states here and then write them.
                let fix = fixture.1;
                let Some(universe) = self.state_ref.dmx_universes.get(fix.universe_no) else {
                    warn!(
                        "Skipping fixture {}: universe {} is out of range",
                        fix.name, fix.universe_no
                    );
                    continue;
                };
                let mut buffer = universe.write().unwrap();

                fix.write(&resolved, &mut buffer.dmx_buffer);
            }
        }

        // Apply overrides
        for ((universe, chan), value) in &state.0.overrides {
            if *universe >= self.state_ref.dmx_universes.len() {
                warn!("Ignoring override: UNI: {universe} CHAN: {chan} -> VAL: {value}; out of universes");
                continue;
            }
            let mut buffer = self.state_ref.dmx_universes[*universe].write().unwrap();
            if *chan == 0 || *chan >= buffer.dmx_buffer.len() {
                warn!("Ignoring override: UNI: {universe} CHAN: {chan} -> VAL: {value}; out of channels");
                continue;
            }
            buffer.dmx_buffer[*chan] = *value;
        }

        mem::drop(state);
    }

    /// Applies one bus message and returns the reply for its sender. The
    /// engine's own rollback notices get no reply. A rollback (or, for folder
    /// events, the undo event) goes back on the bus with the original client,
    /// session and sequence number, as a `DmxEngine` event, so plugins see
    /// exactly what they saw before.
    ///
    /// Every message is applied with the sender's session swapped into the
    /// `DESK` fields (see `dmx::session`), so selection, focus and control
    /// buffer are the sender's. Messages from a session that is not open are
    /// refused.
    pub fn dispatch(
        &self,
        state: &mut RwLockWriteGuard<'_, EngineState>,
        message: BusMessage,
    ) -> Option<CommandResult> {
        let BusMessage {
            client,
            session,
            seq,
            body,
        } = message;
        if matches!(&body, BusBody::Control(event) if event.originator() == EventOriginator::DmxEngine)
        {
            return None;
        }
        if !state.has_session(session) {
            debug!("Unknown {session} ({client}, seq {seq})");
            return Some(CommandResult {
                client,
                seq,
                outcome: CommandOutcome::Refused("Unknown session".to_string()),
            });
        }
        // The desk's own commands (its showfile section) from anyone else.
        if matches!(&body, BusBody::Host(command) if command.is_desk_only())
            && client != ClientId::Desk
        {
            debug!("Desk-only command from {client} (seq {seq})");
            return Some(CommandResult {
                client,
                seq,
                outcome: CommandOutcome::Refused(DESK_ONLY_REFUSAL.to_string()),
            });
        }
        // Showfile and lifecycle commands are the persistence service's: it
        // does their file IO on its own thread and answers them itself.
        let body = match body {
            BusBody::Host(command) => {
                match self.state_ref.persistence.forward(client, seq, command) {
                    Ok(None) => return None,
                    Ok(Some(outcome)) => {
                        return Some(CommandResult {
                            client,
                            seq,
                            outcome,
                        })
                    }
                    Err(command) => BusBody::Host(command),
                }
            }
            body => body,
        };
        // Whether the showfile saves what the message changes. The showfile
        // holds `DESK`'s selection and focus, not other sessions'.
        let edits_show = match &body {
            BusBody::Control(event) => crate::persistence::control_event_marks_showfile_dirty(
                &event.body(),
                session == SessionId::DESK,
            ),
            BusBody::Host(command) => command.edits_show(),
        };
        // Swapped outside `apply`, so a transaction's snapshot holds (and its
        // rollback restores) the session it ran in. Swapped back below,
        // before the lock is released.
        state.swap_session(session);
        let outcome = match body {
            BusBody::Control(event) => {
                self.created.set(None);
                let (refusal, rollback) = self.apply(state, event);
                if let Some(rollback) = rollback {
                    self.event_bus_connection.send(BusMessage::control(
                        client,
                        session,
                        seq,
                        ControlEventMessage::new(EventOriginator::DmxEngine, rollback),
                    ));
                }
                match refusal {
                    Some(reason) => {
                        debug!("{reason} ({client}, seq {seq})");
                        CommandOutcome::Refused(reason.to_string())
                    }
                    None => CommandOutcome::Ok {
                        created: self.created.take(),
                    },
                }
            }
            BusBody::Host(command) => {
                let changes_patch = command.changes_patch();
                let outcome = self.apply_host(state, command);
                if changes_patch && matches!(outcome, CommandOutcome::Ok { .. }) {
                    state.reconcile_stage();
                }
                outcome
            }
        };
        state.swap_session(session);
        if edits_show && matches!(outcome, CommandOutcome::Ok { .. }) {
            self.state_ref.persistence.mark_edited();
        }
        Some(CommandResult {
            client,
            seq,
            outcome,
        })
    }

    /// Applies a host-only command, with the sender's session swapped in
    /// (see [`Self::dispatch`]).
    fn apply_host(
        &self,
        state: &mut RwLockWriteGuard<'_, EngineState>,
        command: HostCommand,
    ) -> CommandOutcome {
        const OK: CommandOutcome = CommandOutcome::Ok { created: None };
        match command {
            HostCommand::Ping => OK,
            HostCommand::ToggleAnimationPreset(preset) => {
                self.toggle_animation_preset(state, preset)
            }
            HostCommand::SaveAnimationAsTemplate {
                selection,
                animation_id,
                name,
            } => self.save_animation_as_template(state, &selection, animation_id, &name),
            HostCommand::CloneAnimationInstanceState {
                plugin_key,
                source,
                target,
            } => {
                crate::plugin::wasm::clone_animation_instance_state(
                    &self.state_ref,
                    &plugin_key,
                    source,
                    target,
                );
                OK
            }
            HostCommand::KeepAnimationEditorAlive {
                plugin_key,
                instance_id,
                fixtures,
            } => {
                self.state_ref
                    .wasm_animation_editor_requests
                    .write()
                    .unwrap_or_else(PoisonError::into_inner)
                    .insert(
                        instance_id,
                        crate::state::WasmAnimationEditorRequest {
                            plugin_key,
                            instance_id,
                            fixtures,
                            last_seen: Instant::now(),
                        },
                    );
                OK
            }
            HostCommand::CreateGroup { name, folder } => state.create_group_command(name, folder),
            HostCommand::RenameGroup { group, name } => state.rename_group_command(group, &name),
            HostCommand::DeleteGroup(group) => state.delete_group_command(group),
            HostCommand::CreateScene { name, folder } => state.create_scene_command(name, folder),
            HostCommand::CloneScene { scene, name } => state.clone_scene_command(scene, name),
            HostCommand::RenameScene { scene, name } => state.rename_scene_command(scene, &name),
            HostCommand::DeleteScene(scene) => state.delete_scene_command(scene),
            HostCommand::AddFixtures { group, fixtures } => {
                state.add_fixtures_command(group, fixtures)
            }
            HostCommand::UpdateFixture {
                group,
                fixture,
                value,
            } => state.update_fixture_command(group, fixture, value),
            HostCommand::DeleteFixture { group, fixture } => {
                state.delete_fixture_command(group, fixture)
            }
            HostCommand::SwapFixtures { group, a, b } => {
                let outcome = state.swap_fixtures_command(group, a, b);
                if matches!(outcome, CommandOutcome::Ok { .. }) {
                    state.swap_stage_fixtures(group, a, b);
                }
                outcome
            }
            HostCommand::MoveFixtureToGroup {
                group,
                fixture,
                target,
            } => {
                let outcome = state.move_fixture_command(group, fixture, target);
                if let CommandOutcome::Ok {
                    created:
                        Some(CreatedId::Fixture {
                            group: to,
                            fixture: new_id,
                        }),
                } = outcome
                {
                    state.move_stage_fixture((group, fixture), (to, new_id));
                }
                outcome
            }
            HostCommand::InitFixtures(fixtures) => {
                if !state.has_fixtures(&fixtures) {
                    return CommandOutcome::Refused("Fixture not found".to_string());
                }
                let mut pending = self.pending_fixture_inits.borrow_mut();
                for key in fixtures {
                    if !pending.contains(&key) {
                        pending.push(key);
                    }
                }
                OK
            }
            HostCommand::CreateAnimationTemplate(spec) => {
                state.create_animation_template_command(spec)
            }
            HostCommand::UpdateAnimationTemplate { id, spec } => {
                state.update_animation_template_command(id, spec)
            }
            HostCommand::DeleteAnimationTemplate(id) => {
                state.delete_animation(id);
                OK
            }
            HostCommand::SetGrandMaster(percent) => {
                self.state_ref.set_grand_master_percent(percent);
                OK
            }
            HostCommand::CreateView { name } => state.create_view_command(&name),
            HostCommand::DeleteView(view) => state.delete_view_command(view),
            HostCommand::AddViewOverlay { view, scene } => {
                state.add_view_overlay_command(view, scene)
            }
            HostCommand::RemoveViewOverlay { view, scene } => {
                state.remove_view_overlay_command(view, scene)
            }
            HostCommand::SetViewSceneMasters {
                view,
                scene,
                masters,
            } => state.set_view_scene_masters_command(view, scene, masters),
            HostCommand::CreateSceneGraph { name } => state.create_scene_graph_command(name),
            HostCommand::DeleteSceneGraph(graph) => state.delete_scene_graph_command(graph),
            HostCommand::AddSceneGraphNode { graph, x, y } => {
                state.add_scene_graph_node_command(graph, x, y)
            }
            HostCommand::RemoveSceneGraphNode { graph, node } => {
                state.remove_scene_graph_node_command(graph, node)
            }
            HostCommand::MoveSceneGraphNodes { graph, positions } => {
                state.move_scene_graph_nodes_command(graph, &positions)
            }
            HostCommand::ConnectSceneGraphNodes { graph, from, to } => {
                state.connect_scene_graph_nodes_command(graph, from, to)
            }
            HostCommand::DisconnectSceneGraphNodes { graph, from, to } => {
                state.disconnect_scene_graph_nodes_command(graph, from, to)
            }
            HostCommand::SetSceneGraphNode { graph, node, param } => {
                state.set_scene_graph_node_command(graph, node, param)
            }
            HostCommand::SetSceneGraphEdge {
                graph,
                from,
                to,
                param,
            } => state.set_scene_graph_edge_command(graph, from, to, param),
            HostCommand::SetSceneGraphStart { graph, node } => {
                state.set_scene_graph_start_command(graph, node)
            }
            HostCommand::AddStageObject(object) => state.add_stage_object_command(object),
            HostCommand::RemoveStageObjects(ids) => state.remove_stage_objects_command(&ids),
            HostCommand::PatchStage(patch) => state.patch_stage_command(*patch),
            HostCommand::RestoreStage { stage, fixtures } => {
                state.restore_stage_command(*stage, &fixtures)
            }
            HostCommand::ImportStageModel { file_name, bytes } => {
                state.import_stage_model_command(&file_name, &bytes.0)
            }
            HostCommand::RemoveStageAssets(ids) => state.remove_stage_assets_command(&ids),
            HostCommand::SelectAudioInput { name } => self.select_audio_input_command(name),
            HostCommand::SetAudioParam(param) => self.set_audio_param_command(param),
            HostCommand::ReloadPlugins => self.reload_plugins_command(),
            HostCommand::AddArtNetReceiver(address) => self.add_artnet_receiver_command(address),
            HostCommand::RemoveArtNetReceiver(address) => {
                self.remove_artnet_receiver_command(address)
            }
            HostCommand::SetArtNetReceiverEnabled {
                address,
                owner,
                enabled,
            } => self.set_artnet_receiver_enabled_command(address, owner, enabled),
            HostCommand::SetArtNetReceiverUniverses {
                address,
                first,
                last,
            } => self.set_artnet_receiver_universes_command(address, first, last),
            // `dispatch` forwards these to the persistence service.
            HostCommand::SaveShowfile
            | HostCommand::SaveShowfileAs(_)
            | HostCommand::LoadShowfile(_)
            | HostCommand::CloseShowfile
            | HostCommand::SetFrontendSection { .. }
            | HostCommand::Quit { .. }
            | HostCommand::Restart { .. } => {
                CommandOutcome::Refused("Not an engine command".to_string())
            }
        }
    }

    /// [`HostCommand::ToggleAnimationPreset`], in the sender's session.
    fn toggle_animation_preset(
        &self,
        state: &mut RwLockWriteGuard<'_, EngineState>,
        preset: AnimationPreset,
    ) -> CommandOutcome {
        let existing = templates::preset_template_id(&state.0.animation_templates, preset);
        let Some(id) = templates::ensure_preset_template(&mut state.0.animation_templates, preset)
        else {
            return CommandOutcome::Refused(templates::ANIMATION_LIMIT_REACHED.to_string());
        };
        let active = templates::selection_animations(state.curr_scene(), &state.get_selection())
            .is_some_and(|animations| animations.contains_key(&id));
        let event = templates::toggle_animation_event(id, active);
        let (refusal, _rollback) =
            self.apply(state, ControlEventMessage::new(EventOriginator::Web, event));
        match refusal {
            Some(reason) => CommandOutcome::Refused(reason.to_string()),
            None => CommandOutcome::Ok {
                created: existing
                    .is_none()
                    .then_some(CreatedId::AnimationTemplate(id)),
            },
        }
    }

    /// [`HostCommand::SaveAnimationAsTemplate`], in the sender's session.
    fn save_animation_as_template(
        &self,
        state: &mut RwLockWriteGuard<'_, EngineState>,
        selection: &FixtureSelection,
        animation_id: u8,
        name: &str,
    ) -> CommandOutcome {
        let scene_id = state.0.current_scene_focus;
        let Some(spec) = templates::selection_animations(state.curr_scene(), selection)
            .and_then(|animations| animations.get(&animation_id))
            .map(|animation| animation.spec_cloned.clone())
        else {
            return CommandOutcome::Refused("Animation not found".to_string());
        };
        let wasm_key = match &spec.body {
            AnimationSpecBody::WasmPlugin(wasm) => Some(wasm.plugin_key.clone()),
            _ => None,
        };
        let Some(new_id) =
            templates::save_as_template(&mut state.0.animation_templates, spec, name)
        else {
            return CommandOutcome::Refused(templates::ANIMATION_LIMIT_REACHED.to_string());
        };

        // A WASM animation keeps its plugin state per instance; the template
        // gets a copy, as when the Animations page saves its editor state.
        if let Some(plugin_key) = wasm_key {
            let fixtures = selection.sorted().fixtures;
            let source = crate::plugin::tick::stable_animation_instance_id(
                &plugin_key,
                scene_id,
                animation_id,
                &fixtures,
            );
            let target = crate::plugin::tick::stable_animation_template_id(&plugin_key, new_id);
            crate::plugin::wasm::clone_animation_instance_state(
                &self.state_ref,
                &plugin_key,
                source,
                target,
            );
        }

        CommandOutcome::Ok {
            created: Some(CreatedId::AnimationTemplate(new_id)),
        }
    }

    pub fn apply(
        &self,
        state: &mut RwLockWriteGuard<'_, EngineState>,
        ev: ControlEventMessage,
    ) -> (Option<&'static str>, Option<ControlEvent>) {
        // TODO: is this valid?
        if ev.originator() == EventOriginator::DmxEngine {
            return (None, None);
        }

        // Require selection.
        let requires_selection = ev.requires_selection();
        if requires_selection && state.0.selection.is_empty() {
            return (Some("No selected object(s)"), None);
        }

        // Empties the fixture state buffer on selecion events. A scene
        // master fader is not one: the Programmer sends M. Alpha / M. Speed
        // while its attribute faders show the buffer.
        let body = ev.body();
        let keeps_buffer = matches!(
            body,
            ControlEvent::SetSceneMasterAlpha(..)
                | ControlEvent::SetSceneMasterSpeed(..)
                // The Scene graph page focuses and enables graphs.
                | ControlEvent::SetFocusedSceneGraph(..)
                | ControlEvent::SetSceneGraphEnabled(..)
        );
        if !requires_selection && !keeps_buffer {
            state.0.control_buffer.reset();
        }

        // Match event.
        match body {
            ControlEvent::Transaction(t) => {
                let snapshot = state.0.clone();
                let live_owner = state.live_owner();
                for t_ev in t {
                    // debug!("Apply transaction: {:?}", &ev);

                    let (err, rollback) =
                        self.apply(state, ControlEventMessage::new(ev.originator(), t_ev));

                    if err.is_some() {
                        error!("Error during transaction: {err:?}");
                        state.0 = snapshot;
                        // `live_mode` came back with the snapshot; its owner
                        // lives outside `.0`.
                        state.2.live_owner = live_owner;
                        return (err, rollback);
                    }
                }

                (None, None)
            }
            // Overrides
            ControlEvent::SetChannelOverride(uni, chan, value) => {
                if chan == 0 || chan > 512 {
                    return (
                        Some("Illegal channel no."),
                        Some(ControlEvent::RemoveChannelOverride(uni, chan)),
                    );
                }

                state
                    .0
                    .overrides
                    .insert((uni as usize, chan as usize), value);
                (None, None)
            }
            ControlEvent::RemoveChannelOverride(uni, chan) => {
                let ch = chan as usize;
                if state.0.overrides.remove(&(uni as usize, ch)).is_none() {
                    (
                        Some("Channel override does not exist (missing)"),
                        Some(ControlEvent::SetChannelOverride(uni, chan, 0)),
                    )
                } else {
                    (None, None)
                }
            }
            // UI-only plugin events: ignore in DMX engine
            ControlEvent::PluginUi(_, _) | ControlEvent::AnimationPluginUi(_, _, _) => (None, None),
            ControlEvent::MainUi(_) => (None, None),
            // Other
            ControlEvent::SelectGroup(group_id) => {
                if !state.groups().contains_key(&group_id) {
                    return (
                        Some("Illegal group"),
                        Some(ControlEvent::DeSelectGroup(group_id)),
                    );
                }

                if !state.0.selection.group_ids.insert(group_id) {
                    (Some("Already selected"), None)
                } else {
                    if state.0.selection.group_ids.len() != 1
                        || !state.0.selection.fixtures_in_group.is_empty()
                    {
                        state.0.selection.fixtures_in_group.clear();
                    }
                    (None, None)
                }
            }
            ControlEvent::DeSelectGroup(group_id) => {
                if !state.groups().contains_key(&group_id) {
                    return (
                        Some("Illegal group"),
                        Some(ControlEvent::DeSelectGroup(group_id)),
                    );
                }

                if !state.0.selection.group_ids.remove(&group_id) {
                    (Some("Not selected"), None)
                } else {
                    state.0.selection.fixtures_in_group.clear();
                    (None, None)
                }
            }
            ControlEvent::LimitSelectionToFixtureInCurrentGroup(fixture_id) => {
                let Some(&group_id) = state.selection().group_ids.iter().next() else {
                    return (
                        Some("Exactly 1 group shall be selected"),
                        Some(ControlEvent::UnLimitSelectionToFixtureInCurrentGroup(
                            fixture_id,
                        )),
                    );
                };
                if state.selection().group_ids.len() != 1 {
                    return (
                        Some("Exactly 1 group shall be selected"),
                        Some(ControlEvent::UnLimitSelectionToFixtureInCurrentGroup(
                            fixture_id,
                        )),
                    );
                }
                let Some(group) = state.0.groups.get(&group_id) else {
                    return (Some("Illegal group"), None);
                };
                if !group.fixtures.contains_key(&fixture_id) {
                    return (Some("Illegal fixture"), None);
                }

                if !state.0.selection.fixtures_in_group.insert(fixture_id) {
                    (Some("Already selected"), None)
                } else {
                    (None, None)
                }
            }
            ControlEvent::UnLimitSelectionToFixtureInCurrentGroup(fixture_id) => {
                if state.selection().group_ids.len() != 1 {
                    return (Some("Exactly 1 group shall be selected"), None);
                }

                if !state.0.selection.fixtures_in_group.remove(&fixture_id) {
                    (Some("Not selected"), None)
                } else {
                    (None, None)
                }
            }
            ControlEvent::RemoveSelection => {
                if state.0.selection.group_ids.is_empty()
                    && state.0.selection.fixtures_in_group.is_empty()
                {
                    (Some("No selection"), None)
                } else if !state.0.selection.fixtures_in_group.is_empty() {
                    state.0.selection.fixtures_in_group.clear();
                    (None, None)
                } else {
                    let Some(group_id) = state.0.selection.group_ids.iter().next().copied() else {
                        return (Some("No selection"), None);
                    };
                    state.0.selection.group_ids.remove(&group_id);
                    (None, None)
                }
            }
            ControlEvent::RemoveAllSelection => {
                state.0.selection.clear();
                (None, None)
            }
            ControlEvent::PushSelection => {
                let selection = state.selection().clone();

                // if let Some(top) = state.0.selection_stack.front() {
                //     if selection.is_empty() && top.is_empty() {
                //         return (Some("Push to empty selection"), None);
                //     }
                // }

                state.0.selection_stack.push_front(selection.clone());
                state.0.selection.clear();

                debug!("new selection after push: {selection:?}");

                (None, None)
            }
            ControlEvent::PopSelection => {
                if let Some(top) = state.0.selection_stack.pop_front() {
                    state.0.selection = top;
                    (None, None)
                } else {
                    (Some("Selection stack empty"), None)
                }
            }
            ControlEvent::MiscEvent { descriptor, value } => {
                warn!("Not implemented in DMX: {descriptor:?} | {value:?}");
                (None, None)
            }
            ControlEvent::RemoveChange {
                selection,
                property,
            } => {
                let scene_id = state.0.current_scene_focus;
                let Some(scene) = state.0.scenes.get_mut(&scene_id) else {
                    return (Some("Illegal scene id"), None);
                };

                let mut removed_any = false;

                for (gid, fid) in &selection.fixtures {
                    let selector = FixtureSelector {
                        gid: *gid,
                        fid: *fid,
                        property,
                    };

                    if scene.sink.changeset.remove(&selector) {
                        removed_any = true;

                        if let Some(fixture_state) =
                            scene.sink.fixture_states.get_mut(&(*gid, *fid))
                        {
                            fixture_state.apply_value(0, property);
                        }
                    }
                }

                if removed_any {
                    state.0.control_buffer.apply_value(0, property);
                }

                (None, None)
            }
            ControlEvent::SetSceneMasterAlpha(scene_id, value_percent) => {
                let Some(scene) = state.0.scenes.get_mut(&scene_id) else {
                    return (Some("Illegal scene id"), None);
                };

                if value_percent > 100 {
                    return (Some("Percent value out of range"), None);
                }

                scene.sink.master_alpha_fader = value_percent;

                (None, None)
            }
            ControlEvent::SetSceneMasterSpeed(scene_id, speed) => {
                let Some(scene) = state.0.scenes.get_mut(&scene_id) else {
                    return (Some("Illegal scene id"), None);
                };

                scene.sink.master_speed = speed;

                (None, None)
            }
            ControlEvent::SetSceneFocus(id) => {
                if id == BLANK_SCENE_ID {
                    return (Some("BLANK scene is read-only"), None);
                }

                if !state.0.scenes.contains_key(&id) {
                    return (Some("Illegal scene"), Some(ControlEvent::SetSceneFocus(0)));
                }

                // Focus is purely the edit target: it must not touch the
                // overlay stack, and it must not restart the animations of a
                // scene that may be playing live right now. Both of those
                // belong to entering live mode instead.
                state.0.current_scene_focus = id;
                (None, None)
            }
            // The sender takes the preview over, even from another session;
            // turning it off ends it for everyone (there is one rig).
            ControlEvent::SetLiveMode(enabled) => {
                let owner = enabled.then(|| state.applying_session());
                Self::set_live_mode(state, owner);
                (None, None)
            }
            ControlEvent::ToggleLiveMode => {
                let sender = state.applying_session();
                let owner = (state.live_owner() != Some(sender)).then_some(sender);
                Self::set_live_mode(state, owner);
                (None, None)
            }
            ControlEvent::RemoveOverlayScene(scene_id) => {
                match state
                    .0
                    .current_overlay_scenes
                    .iter()
                    .position(|s| *s == scene_id)
                {
                    Some(index) => {
                        state.0.current_overlay_scenes.remove(index);
                        (None, None)
                    }
                    None => (
                        Some("Scene is not an overlay"),
                        Some(ControlEvent::SetOverlays(
                            state.0.current_overlay_scenes.clone(),
                        )),
                    ),
                }
            }
            ControlEvent::AddOverlayScene(scene_id) => {
                if scene_id == BLANK_SCENE_ID {
                    return (Some("BLANK scene cannot be an overlay"), None);
                }

                if !state.0.scenes.contains_key(&scene_id) {
                    return (Some("Illegal scene"), None);
                }

                if state.0.current_overlay_scenes.contains(&scene_id) {
                    return (
                        Some("Scene already an overlay"),
                        Some(ControlEvent::SetOverlays(
                            state.0.current_overlay_scenes.clone(),
                        )),
                    );
                }

                {
                    let Some(scene) = state.0.scenes.get_mut(&scene_id) else {
                        return (Some("Illegal scene"), None);
                    };
                    let animations = &mut scene.sink.active_animations;

                    for (_selection, anim_set) in animations.iter_mut() {
                        for (anim_id, anim) in anim_set.iter_mut() {
                            if anim.enabled {
                                anim.reset_timers(anim.spec_cloned.sync_mode());
                                debug!("Reset animation: {anim_id}");
                            }
                        }
                    }
                }

                state.0.current_overlay_scenes.push(scene_id);
                (None, None)
            }
            ControlEvent::SetOverlays(overlays) => {
                if overlays.contains(&BLANK_SCENE_ID) {
                    return (Some("BLANK scene cannot be an overlay"), None);
                }

                if overlays
                    .iter()
                    .any(|scene_id| !state.0.scenes.contains_key(scene_id))
                {
                    return (Some("Illegal overlay scene"), None);
                }
                if overlays.iter().collect::<HashSet<_>>().len() != overlays.len() {
                    return (Some("Duplicate overlay scene"), None);
                }

                state.0.current_overlay_scenes.clear();

                debug!("SET OVERLAYS: {overlays:?}");

                for scene in &overlays {
                    // let animations = state.0.animation_templates.clone();

                    let anim = &mut state
                        .0
                        .scenes
                        .get_mut(scene)
                        .unwrap()
                        .sink
                        .active_animations;

                    for (_selec, anim_set) in anim.iter_mut() {
                        for (anim_id, anim) in anim_set.iter_mut() {
                            if anim.enabled {
                                anim.reset_timers(anim.spec_cloned.sync_mode());
                                debug!("Reset animation: {anim_id}");
                            }
                        }
                    }

                    state.0.current_overlay_scenes.push(*scene);
                }
                (None, None)
            }
            ControlEvent::CreatePalette(name, kind) => {
                let new_id = (0..=u8::MAX).find(|id| !state.0.palettes.contains_key(id));
                match new_id {
                    Some(id) => {
                        if blaulicht_shared::palette::would_create_cycle(
                            &state.0.palettes,
                            id,
                            &kind,
                        ) {
                            return (Some("Pointer palette would form a cycle"), None);
                        }
                        state.0.palettes.insert(
                            id,
                            blaulicht_shared::palette::Palette {
                                name,
                                kind,
                                folder: None,
                            },
                        );
                        self.created.set(Some(CreatedId::Palette(id)));
                        (None, None)
                    }
                    None => (Some("Max palettes reached"), None),
                }
            }
            ControlEvent::UpdatePalette(id, kind) => {
                if blaulicht_shared::palette::would_create_cycle(&state.0.palettes, id, &kind) {
                    return (Some("Pointer palette would form a cycle"), None);
                }
                match state.0.palettes.get_mut(&id) {
                    Some(palette) => {
                        palette.kind = kind;
                        // Property coverage may have changed; re-sync every scene.
                        let palettes_snapshot = state.0.palettes.clone();
                        for scene in state.0.scenes.values_mut() {
                            scene.sink.sync_palette_bindings(&palettes_snapshot);
                        }
                        (None, None)
                    }
                    None => (Some("Palette not found"), None),
                }
            }
            ControlEvent::DeletePalette(id) => {
                if state.0.palettes.remove(&id).is_none() {
                    return (Some("Palette not found"), None);
                }
                let palettes_snapshot = state.0.palettes.clone();
                for scene in state.0.scenes.values_mut() {
                    for assignments in scene.sink.palette_assignments.values_mut() {
                        assignments.retain(|pid| *pid != id);
                    }
                    scene.sink.sync_palette_bindings(&palettes_snapshot);
                }
                (None, None)
            }
            ControlEvent::UnassignPalette(id) => {
                if !state.0.palettes.contains_key(&id) {
                    return (Some("Palette not found"), None);
                }
                let palettes_snapshot = state.0.palettes.clone();
                for scene in state.0.scenes.values_mut() {
                    for assignments in scene.sink.palette_assignments.values_mut() {
                        assignments.retain(|pid| *pid != id);
                    }
                    scene.sink.sync_palette_bindings(&palettes_snapshot);
                }
                (None, None)
            }
            ControlEvent::RenamePalette(id, name) => match state.0.palettes.get_mut(&id) {
                Some(palette) => {
                    palette.name = name;
                    (None, None)
                }
                None => (Some("Palette not found"), None),
            },
            ControlEvent::SetFocusedSceneGraph(graph_id) => {
                if let Some(id) = graph_id {
                    if !state.0.scene_graphs.graphs.contains_key(&id) {
                        return (Some("Scene graph not found"), None);
                    }
                }
                state.0.scene_graphs.focused_graph = graph_id;
                (None, None)
            }
            ControlEvent::SetSceneGraphEnabled(graph_id, enabled) => {
                match state.0.scene_graphs.graphs.get_mut(&graph_id) {
                    Some(graph) => {
                        graph.enabled = enabled;
                        (None, None)
                    }
                    None => (Some("Scene graph not found"), None),
                }
            }
            ControlEvent::ToggleSceneGraphEnabled(graph_id) => {
                match state.0.scene_graphs.graphs.get_mut(&graph_id) {
                    Some(graph) => {
                        graph.enabled = !graph.enabled;
                        (None, None)
                    }
                    None => (Some("Scene graph not found"), None),
                }
            }
            // Folders: the logic and its tests live in `blaulicht_shared::folder`.
            // A successful event returns its rollback (the event that undoes it).
            folder_event @ (ControlEvent::CreateFolder { .. }
            | ControlEvent::RenameFolder(_, _)
            | ControlEvent::DeleteFolder(_)
            | ControlEvent::MoveToFolder { .. }
            | ControlEvent::SelectFolder(_)
            | ControlEvent::ReorderFolder(_, _)) => {
                match state.0.apply_folder_event(folder_event) {
                    Ok(rollback) => (None, Some(rollback)),
                    Err(err) => (Some(err), None),
                }
            }
            CONTROLS_REQUIRING_SELECTION!() => {
                let curr_selection = state.get_selection().sorted();
                self.apply_on_selection_and_scene(&curr_selection, state, ev)
            }
        }
    }

    fn apply_on_selection_and_scene(
        &self,
        curr_selection: &FixtureSelection,
        state: &mut RwLockWriteGuard<'_, EngineState>,
        ev: ControlEventMessage,
    ) -> (Option<&'static str>, Option<ControlEvent>) {
        let current_scene_focus = state.0.current_scene_focus;

        // Defensive: `SetSceneFocus` rejects BLANK, but every arm below
        // unwraps `scenes[current_scene_focus]` and would author into the
        // read-only scene if focus ever drifted there.
        if current_scene_focus == BLANK_SCENE_ID {
            return (Some("BLANK scene is read-only"), None);
        }

        let (msg, undo, effective_properties) = match ev.body() {
            ControlEvent::AddAnimation(id) => {
                // Get the source animation to clone the spec.
                let animation_template = state.0.animation_templates.get(&id).cloned();
                let Some(animation_template) = animation_template else {
                    return (Some("Animation template ID does not exist"), None);
                };

                let target_property = animation_template.spec.property;

                let this_scene = state.0.scenes.get_mut(&current_scene_focus).unwrap();

                // Reject if any fixture in the selection has the target
                // property bound to a palette — the animation could not
                // actually drive the slot.
                for fixture_key in &curr_selection.fixtures {
                    if let Some(fixture_state) = this_scene.sink.fixture_states.get(fixture_key) {
                        if fixture_state.slot(target_property).is_frozen() {
                            return (Some("Target property is bound to a palette"), None);
                        }
                    }
                }

                if !this_scene
                    .sink
                    .active_animations
                    .contains_key(curr_selection)
                {
                    this_scene
                        .sink
                        .active_animations
                        .insert(curr_selection.clone(), BTreeMap::new());
                }

                let selec_anim = this_scene
                    .sink
                    .active_animations
                    .get_mut(curr_selection)
                    .unwrap();

                match selec_anim.contains_key(&id) {
                    true => (Some("Animation already applied"), None, None),
                    false => {
                        let spec_cloned = animation_template.spec.clone();
                        let effective_property = spec_cloned.property;

                        if let AnimationSpecBody::WasmPlugin(wasm) = &spec_cloned.body {
                            let source = crate::plugin::tick::stable_animation_template_id(
                                &wasm.plugin_key,
                                id,
                            );
                            let target = crate::plugin::tick::stable_animation_instance_id(
                                &wasm.plugin_key,
                                current_scene_focus,
                                id,
                                &curr_selection.fixtures,
                            );
                            crate::plugin::wasm::clone_animation_instance_state(
                                &self.state_ref,
                                &wasm.plugin_key,
                                source,
                                target,
                            );
                        }

                        selec_anim.insert(
                            id,
                            ActiveAnimation::new(&curr_selection.fixtures, spec_cloned),
                        );

                        (None, None, Some(vec![effective_property]))
                    }
                }
            }
            ControlEvent::SetAnimationSpeed(id, md) => {
                debug!("set speed anim");

                let this_scene = state.0.scenes.get_mut(&current_scene_focus).unwrap();

                match !this_scene
                    .sink
                    .active_animations
                    .contains_key(curr_selection)
                {
                    true => (Some("No animations for this selection"), None, None),
                    false => {
                        let selec_anim = this_scene
                            .sink
                            .active_animations
                            .get_mut(curr_selection)
                            .unwrap();

                        match selec_anim.get_mut(&id) {
                            None => (Some("Animation not applied to selection"), None, None),
                            Some(anim) => {
                                debug!("set speed: {md:?}");

                                // Purge selection if empty.
                                // if selec_anim.is_empty() {
                                //     this_scene.sink.active_animations.remove(curr_selection);
                                //     debug!("moved selection entirely");
                                // }

                                anim.speed_factor = md;

                                // let animation = state.0.animation_templates.get(&id).unwrap();
                                (None, None, Some(vec![anim.spec_cloned.property]))
                            }
                        }
                    }
                }
            }
            ControlEvent::RemoveAnimation(id) => {
                debug!("REMove anim");

                let this_scene = state.0.scenes.get_mut(&current_scene_focus).unwrap();

                match !this_scene
                    .sink
                    .active_animations
                    .contains_key(curr_selection)
                {
                    true => {
                        debug!("NO SELEC: {:?}", curr_selection);
                        debug!("NO SELEC 2: {:?}", curr_selection.sorted());
                        (Some("No animations for this selection"), None, None)
                    }
                    false => {
                        let selec_anim = this_scene
                            .sink
                            .active_animations
                            .get_mut(curr_selection)
                            .unwrap();

                        match selec_anim.remove(&id) {
                            None => (Some("Animation not applied to selection"), None, None),
                            Some(old_anim) => {
                                debug!("REMOVED");

                                // Purge selection if empty.
                                if selec_anim.is_empty() {
                                    this_scene.sink.active_animations.remove(curr_selection);
                                    debug!("moved selection entirely");
                                }

                                // let animation = state.0.animation_templates.get(&id).unwrap();
                                (None, None, Some(vec![old_anim.spec_cloned.property]))
                            }
                        }
                    }
                }
            }
            ControlEvent::PlayAnimation(id) => {
                let this_scene = state.0.scenes.get_mut(&current_scene_focus).unwrap();
                match !this_scene
                    .sink
                    .active_animations
                    .contains_key(curr_selection)
                {
                    true => {
                        debug!("No such selection: {curr_selection:?}");

                        (
                            Some("No such selection"),
                            Some(ControlEvent::PauseAnimation(id)),
                            None,
                        )
                    }
                    false => {
                        let selec_anim = this_scene
                            .sink
                            .active_animations
                            .get_mut(curr_selection)
                            .unwrap();

                        match selec_anim.get_mut(&id) {
                            Some(anim) => {
                                anim.set_timers(anim.spec_cloned.sync_mode());
                                (None, None, Some(vec![anim.spec_cloned.property]))
                            }
                            None => (
                                Some("No such animation on selection"),
                                Some(ControlEvent::PauseAnimation(id)),
                                None,
                            ),
                        }
                    }
                }
            }
            ControlEvent::PauseAnimation(id) => {
                let this_scene = state.0.scenes.get_mut(&current_scene_focus).unwrap();
                match !this_scene
                    .sink
                    .active_animations
                    .contains_key(curr_selection)
                {
                    true => (
                        Some("No such selection"),
                        Some(ControlEvent::PauseAnimation(id)),
                        None,
                    ),
                    false => {
                        let selec_anim = this_scene
                            .sink
                            .active_animations
                            .get_mut(curr_selection)
                            .unwrap();

                        match selec_anim.get_mut(&id) {
                            Some(anim) => {
                                anim.enabled = false;
                                // let animation = state.0.animation_templates.get(&id).unwrap();
                                (None, None, Some(vec![anim.spec_cloned.property]))
                            }
                            None => (
                                Some("No such animation on selection"),
                                Some(ControlEvent::PauseAnimation(id)),
                                None,
                            ),
                        }
                    }
                }
            }
            ControlEvent::LoadSpecIntoAnimation(id, spec) => {
                let this_scene = state.0.scenes.get_mut(&current_scene_focus).unwrap();
                match !this_scene
                    .sink
                    .active_animations
                    .contains_key(curr_selection)
                {
                    true => {
                        debug!("No such selection: {curr_selection:?}");

                        (
                            Some("No such selection"),
                            Some(ControlEvent::PauseAnimation(id)),
                            None,
                        )
                    }
                    false => {
                        let selec_anim = this_scene
                            .sink
                            .active_animations
                            .get_mut(curr_selection)
                            .unwrap();

                        match selec_anim.get_mut(&id) {
                            Some(anim) => {
                                anim.spec_cloned = spec;
                                (None, None, Some(vec![anim.spec_cloned.property]))
                            }
                            None => (
                                Some("No such animation on selection"),
                                Some(ControlEvent::PauseAnimation(id)),
                                None,
                            ),
                        }
                    }
                }
            }
            ControlEvent::AssignPaletteToSelection(palette_id) => {
                let palettes_snapshot = state.0.palettes.clone();
                let properties = match palettes_snapshot.get(&palette_id) {
                    Some(p) => p.kind.properties(&palettes_snapshot),
                    None => return (Some("Palette not found"), None),
                };
                let this_scene = state.0.scenes.get_mut(&current_scene_focus).unwrap();

                // Reject mixed state: either every fixture in the selection
                // already has this palette assigned, or none of them do.
                let mut some_active = false;
                let mut some_inactive = false;
                for fixture_key in &curr_selection.fixtures {
                    let active = this_scene
                        .sink
                        .palette_assignments
                        .get(fixture_key)
                        .map(|ids| ids.contains(&palette_id))
                        .unwrap_or(false);
                    if active {
                        some_active = true;
                    } else {
                        some_inactive = true;
                    }
                }
                if some_active && some_inactive {
                    return (Some("Selection has mixed palette state"), None);
                }

                // Reject if another assigned palette already covers one of
                // this palette's properties for any fixture in the selection.
                for fixture_key in &curr_selection.fixtures {
                    let Some(existing) = this_scene.sink.palette_assignments.get(fixture_key)
                    else {
                        continue;
                    };
                    for other_id in existing {
                        if *other_id == palette_id {
                            continue;
                        }
                        let Some(other) = palettes_snapshot.get(other_id) else {
                            continue;
                        };
                        for property in &properties {
                            if other.kind.properties(&palettes_snapshot).contains(property) {
                                return (Some("Property already covered by another palette"), None);
                            }
                        }
                    }
                }

                // Reject if any fixture in the selection has an active
                // animation driving one of this palette's properties — the
                // animation would silently no-op once the slot is frozen.
                for (anim_selection, anims) in &this_scene.sink.active_animations {
                    let intersects_selection = anim_selection
                        .fixtures
                        .iter()
                        .any(|k| curr_selection.fixtures.contains(k));
                    if !intersects_selection {
                        continue;
                    }
                    for active in anims.values() {
                        if properties.contains(&active.spec_cloned.property) {
                            return (Some("Property is controlled by an active animation"), None);
                        }
                    }
                }

                for fixture_key in &curr_selection.fixtures {
                    let assignments = this_scene
                        .sink
                        .palette_assignments
                        .entry(*fixture_key)
                        .or_default();
                    if !assignments.contains(&palette_id) {
                        assignments.push(palette_id);
                    }
                }
                this_scene.sink.sync_palette_bindings(&palettes_snapshot);
                (None, None, Some(properties))
            }
            ControlEvent::UnassignPaletteFromSelection(palette_id) => {
                let palettes_snapshot = state.0.palettes.clone();
                let properties = match palettes_snapshot.get(&palette_id) {
                    Some(p) => p.kind.properties(&palettes_snapshot),
                    None => return (Some("Palette not found"), None),
                };
                let this_scene = state.0.scenes.get_mut(&current_scene_focus).unwrap();

                // Palette must be active on every fixture in the selection.
                for fixture_key in &curr_selection.fixtures {
                    let active = this_scene
                        .sink
                        .palette_assignments
                        .get(fixture_key)
                        .map(|ids| ids.contains(&palette_id))
                        .unwrap_or(false);
                    if !active {
                        return (Some("Palette not active on full selection"), None);
                    }
                }

                for fixture_key in &curr_selection.fixtures {
                    if let Some(assignments) =
                        this_scene.sink.palette_assignments.get_mut(fixture_key)
                    {
                        assignments.retain(|id| *id != palette_id);
                    }
                }
                this_scene.sink.sync_palette_bindings(&palettes_snapshot);
                (None, None, Some(properties))
            }
            _ => {
                // NOTE: this applies the changeset internally on the sink.
                let this_scene = state.0.scenes.get_mut(&current_scene_focus).unwrap();

                let rejected = this_scene
                    .sink
                    .apply_with_selection(curr_selection, ev.body());

                // Update control buffer for the UI.
                state.0.control_buffer.apply(ev.body());

                if rejected {
                    return (Some("Slot frozen to palette; unbind to edit"), None);
                }
                return (None, None);
            }
        };

        // Update the changeset
        let this_scene = state.0.scenes.get_mut(&current_scene_focus).unwrap();

        for selector in &curr_selection.fixtures {
            let Some(ref effective_properties) = effective_properties else {
                continue;
            };

            for property in effective_properties {
                this_scene
                    .sink
                    .changeset
                    .insert((*selector, *property).into());
            }
        }

        (msg, undo)
    }
}

impl DmxEngine {
    /// Enters or leaves exclusive programming mode: `owner` is the session
    /// whose focus becomes the base layer, `None` leaves live mode.
    ///
    /// Whenever the preview gets a new owner, the owner's focused scene (the
    /// new base layer) restarts its enabled animations, so the operator sees
    /// the scene from the top. Leaving live mode only clears the owner:
    /// `current_overlay_scenes` was never touched, so the rig comes back
    /// exactly as it was.
    fn set_live_mode(state: &mut EngineState, owner: Option<SessionId>) {
        if !state.set_live_owner(owner) {
            return;
        }
        let Some(focus) = owner.and_then(|owner| state.scene_focus_of(owner)) else {
            return;
        };
        let Some(scene) = state.0.scenes.get_mut(&focus) else {
            return;
        };
        for anim_set in scene.sink.active_animations.values_mut() {
            for (anim_id, anim) in anim_set.iter_mut() {
                if anim.enabled {
                    anim.reset_timers(anim.spec_cloned.sync_mode());
                    debug!("Reset animation: {anim_id}");
                }
            }
        }
    }
}

fn apply_grand_master(alpha: u8, percent: u8) -> u8 {
    ((alpha as u16 * percent.min(100) as u16) / 100) as u8
}

/// Applies the activated nodes' scene controls to every scene except `base`,
/// the rendered base layer.
fn apply_scene_graph_node_controls(
    state: &mut blaulicht_shared::EngineState,
    base: u8,
    activated_nodes: &[(u8, u8)],
) {
    for (graph_id, node_id) in activated_nodes {
        let Some(node) = state
            .scene_graphs
            .graphs
            .get(graph_id)
            .and_then(|graph| graph.nodes.get(node_id))
            .cloned()
        else {
            continue;
        };
        for scene_id in node.scenes.into_iter().filter(|scene_id| *scene_id != base) {
            if let Some(scene) = state.scenes.get_mut(&scene_id) {
                scene.sink.master_alpha_fader = node.master_alpha.min(100);
                scene.sink.master_speed = node.master_speed;
            }
        }
    }
}

#[cfg(test)]
mod grand_master_tests {
    use super::{apply_grand_master, apply_scene_graph_node_controls};
    use blaulicht_shared::{
        scene::BLANK_SCENE_ID,
        scene_graph::{SceneGraph, SceneGraphNode},
        AnimationSpeedModifier,
    };
    use std::collections::BTreeMap;

    #[test]
    fn grand_master_at_full_preserves_alpha() {
        assert_eq!(apply_grand_master(255, 100), 255);
        assert_eq!(apply_grand_master(37, 100), 37);
    }

    #[test]
    fn grand_master_scales_and_blackout_is_exact() {
        assert_eq!(apply_grand_master(200, 50), 100);
        assert_eq!(apply_grand_master(255, 0), 0);
        assert_eq!(apply_grand_master(255, 200), 255);
    }

    #[test]
    fn graph_node_controls_write_once_and_remain_until_replaced() {
        let mut state = blaulicht_shared::EngineState::default();
        state.new_scene("Base".to_string());
        state.new_scene("Overlay".to_string());
        state.scene_graphs.graphs.insert(
            1,
            SceneGraph {
                nodes: BTreeMap::from([(
                    7,
                    SceneGraphNode {
                        scenes: vec![1],
                        master_alpha: 42,
                        master_speed: AnimationSpeedModifier::_4,
                        ..Default::default()
                    },
                )]),
                ..Default::default()
            },
        );

        apply_scene_graph_node_controls(&mut state, BLANK_SCENE_ID, &[(1, 7)]);
        assert_eq!(state.scenes[&1].sink.master_alpha_fader, 42);
        assert_eq!(
            state.scenes[&1].sink.master_speed,
            AnimationSpeedModifier::_4
        );

        state.scenes.get_mut(&1).unwrap().sink.master_alpha_fader = 73;
        apply_scene_graph_node_controls(&mut state, BLANK_SCENE_ID, &[]);
        assert_eq!(state.scenes[&1].sink.master_alpha_fader, 73);
    }
}

#[cfg(test)]
mod artnet_scale_tests {
    use super::{output_worker, DmxEngineArtnetOutput, DmxOutputFrame};
    use crate::state::{AppState, ArtNetReceiver, NUM_DMX_UNIVERSES};
    use crossbeam_channel::bounded;
    use std::{net::UdpSocket, sync::Arc, time::Duration};

    #[test]
    fn bad_artnet_receiver_does_not_block_other_receivers_or_future_frames() {
        let receiver = UdpSocket::bind("127.0.0.1:0").unwrap();
        receiver
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let state = Arc::new(AppState::new(&[]));
        state.artnet_output.write().unwrap().receivers = vec![
            ArtNetReceiver::new("127.0.0.1:0".parse().unwrap()),
            ArtNetReceiver::new(receiver.local_addr().unwrap()),
        ];
        let (sender, output_rx) = bounded(1);
        let worker_state = state.clone();
        let worker = std::thread::spawn(move || {
            output_worker(
                output_rx,
                [None, None],
                DmxEngineArtnetOutput::new(),
                worker_state,
            )
        });
        for value in [12, 34] {
            sender
                .send(DmxOutputFrame {
                    universes: [[value; 513]; NUM_DMX_UNIVERSES],
                })
                .unwrap();
            for _ in 0..NUM_DMX_UNIVERSES {
                let mut packet = [0; 600];
                receiver.recv(&mut packet).unwrap();
                assert_eq!(packet[18], value);
            }
            if value == 12 {
                state.artnet_output.write().unwrap().receivers.remove(0);
            }
        }
        drop(sender);
        worker.join().unwrap();
        assert!(state.health_data.read().unwrap().artnet_health_state);
    }

    #[test]
    fn output_worker_sends_five_populated_led_strip_universes_without_truncation() {
        const STRIPS: usize = 5;
        const PIXELS_PER_STRIP: usize = 100;

        let receiver = UdpSocket::bind("127.0.0.1:0").unwrap();
        receiver
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let state = Arc::new(AppState::new(&[]));
        state
            .artnet_output
            .write()
            .unwrap()
            .receivers
            .push(ArtNetReceiver::new(receiver.local_addr().unwrap()));

        let artnet = DmxEngineArtnetOutput::new();
        let (sender, output_rx) = bounded(1);
        let worker_state = Arc::clone(&state);
        let worker = std::thread::spawn(move || {
            output_worker(
                output_rx,
                std::array::from_fn(|_| None),
                artnet,
                worker_state,
            )
        });

        let mut frame = DmxOutputFrame {
            universes: [[0; 513]; NUM_DMX_UNIVERSES],
        };
        for universe in 0..STRIPS {
            for pixel in 0..PIXELS_PER_STRIP {
                let channel = 1 + pixel * 3;
                frame.universes[universe][channel..channel + 3].copy_from_slice(&[
                    universe as u8 + 1,
                    pixel as u8,
                    255,
                ]);
            }
        }
        sender.send(frame).unwrap();

        let mut packets = Vec::with_capacity(NUM_DMX_UNIVERSES);
        for _ in 0..NUM_DMX_UNIVERSES {
            let mut packet = [0_u8; 600];
            let length = receiver.recv(&mut packet).unwrap();
            packets.push(packet[..length].to_vec());
        }
        packets.sort_by_key(|packet| packet[14]);

        assert_eq!(packets.len(), NUM_DMX_UNIVERSES);
        for (universe, packet) in packets.iter().take(STRIPS).enumerate() {
            assert_eq!(&packet[..8], b"Art-Net\0");
            assert_eq!(packet.len(), 18 + 512);
            assert_eq!(packet[13], universe as u8);
            assert_eq!(packet[14], universe as u8);
            assert_eq!(&packet[18..21], &[universe as u8 + 1, 0, 255]);
            assert_eq!(&packet[315..318], &[universe as u8 + 1, 99, 255]);
            assert!(packet[318..].iter().all(|channel| *channel == 0));
        }

        drop(sender);
        worker.join().unwrap();
    }
}

/// Folder events and their rollbacks through the real `DmxEngine::apply`,
/// including the `Transaction`, `DeSelectGroup` and
/// `LimitSelectionToFixtureInCurrentGroup` arms the rollbacks use. The folder
/// logic itself is tested in `blaulicht_shared::folder`.
#[cfg(test)]
mod folder_event_tests {
    use super::DmxEngine;
    use crate::{event::SystemEventBus, hub::SystemHub, state::AppState};
    use blaulicht_shared::{
        fixture::{
            light::Light,
            state::{Fixture, FixtureGroup},
            FixtureType,
        },
        folder::{FolderId, FolderItem, FolderKind},
        palette::{Palette, PaletteKind},
        ControlEvent, ControlEventMessage, EventOriginator, HSVColor, SaveEngineState,
    };
    use std::{
        collections::{BTreeMap, HashSet},
        sync::Arc,
    };

    struct Harness {
        engine: DmxEngine,
        app: Arc<AppState>,
    }

    /// Everything a folder event can change: the persisted state (as its
    /// showfile JSON) and the selection (as sets).
    #[derive(Debug, PartialEq)]
    struct Snapshot {
        saved: String,
        group_ids: HashSet<u8>,
        fixtures_in_group: HashSet<u8>,
    }

    impl Harness {
        fn new() -> Self {
            let app = Arc::new(AppState::new(&[]));
            let bus = SystemEventBus::new();
            let engine = DmxEngine::new(
                Arc::clone(&app),
                bus.new_connection(),
                SystemHub::new(),
                [String::new(), String::new()],
            );

            let fixture = |addr| {
                Fixture::new(
                    0,
                    addr,
                    format!("F{addr}"),
                    FixtureType::from(Light::Generic3ChanNoAlpha),
                )
            };
            {
                let state = &mut app.dmx_engine.write().unwrap().0;
                for gid in 0..4u8 {
                    let fixtures = (0..5u8)
                        .map(|fid| (fid, fixture(1 + 3 * (gid as usize * 5 + fid as usize))))
                        .collect();
                    state.groups.insert(
                        gid,
                        FixtureGroup {
                            name: format!("G{gid}"),
                            fixtures,
                            folder: None,
                        },
                    );
                }
                state.new_scene("A".to_string());
                state.new_scene("B".to_string());
                state.palettes = BTreeMap::from([(
                    0,
                    Palette {
                        name: "P".to_string(),
                        kind: PaletteKind::Color(HSVColor::default()),
                        folder: None,
                    },
                )]);
                state.ensure_blank_scene();
            }
            Self { engine, app }
        }

        fn send(&self, event: ControlEvent) -> (Option<&'static str>, Option<ControlEvent>) {
            let mut state = self.app.dmx_engine.write().unwrap();
            self.engine.apply(
                &mut state,
                ControlEventMessage::new(EventOriginator::Web, event),
            )
        }

        fn ok(&self, event: ControlEvent) -> Option<ControlEvent> {
            let (err, rollback) = self.send(event.clone());
            assert_eq!(err, None, "{event:?} failed");
            rollback
        }

        fn snapshot(&self) -> Snapshot {
            let state = self.app.dmx_engine.read().unwrap().0.clone();
            Snapshot {
                group_ids: state.selection.group_ids.clone(),
                fixtures_in_group: state.selection.fixtures_in_group.clone(),
                saved: serde_json::to_string(&SaveEngineState::from(state)).unwrap(),
            }
        }

        /// Applies `event` and then the rollback the engine returned for it,
        /// and checks that the state is back where it was.
        fn round_trip(&self, event: ControlEvent) {
            let before = self.snapshot();
            let rollback = self
                .ok(event.clone())
                .unwrap_or_else(|| panic!("{event:?} returned no rollback"));
            assert_ne!(self.snapshot(), before, "{event:?} changed nothing");
            self.ok(rollback);
            assert_eq!(self.snapshot(), before, "rollback of {event:?}");
        }

        fn create(&self, id: FolderId, kind: FolderKind, parent: Option<FolderId>, name: &str) {
            let rollback = self.ok(ControlEvent::CreateFolder {
                id,
                kind,
                parent,
                name: name.to_string(),
            });
            assert!(matches!(rollback, Some(ControlEvent::DeleteFolder(rid)) if rid == id));
        }
    }

    #[test]
    fn folder_events_and_their_rollbacks_through_the_engine() {
        let h = Harness::new();
        let (top, mid, leaf, side, songs, colors) = (0, 1, 2, 3, 4, 5);
        h.create(top, FolderKind::Groups, None, "Top");
        h.create(mid, FolderKind::Groups, Some(top), "Mid");
        h.create(leaf, FolderKind::Groups, Some(mid), "Leaf");
        h.create(side, FolderKind::Groups, Some(top), "Side");
        h.create(songs, FolderKind::Scenes, None, "Songs");
        h.create(colors, FolderKind::Palettes, None, "Colors");
        for (item, folder) in [
            (FolderItem::Group(0), mid),
            (FolderItem::Group(1), mid),
            (FolderItem::Group(2), leaf),
            (FolderItem::Scene(1), songs),
        ] {
            h.ok(ControlEvent::MoveToFolder {
                item,
                folder: Some(folder),
            });
        }
        h.ok(ControlEvent::ReorderFolder(mid, 2));

        // Rejected events change nothing and return no rollback.
        let before = h.snapshot();
        for event in [
            ControlEvent::CreateFolder {
                id: mid,
                kind: FolderKind::Groups,
                parent: None,
                name: "Taken id".to_string(),
            },
            ControlEvent::CreateFolder {
                id: 9,
                kind: FolderKind::Groups,
                parent: None,
                name: "  ".to_string(),
            },
            ControlEvent::CreateFolder {
                id: 9,
                kind: FolderKind::Groups,
                parent: Some(top),
                name: "side".to_string(),
            },
            ControlEvent::RenameFolder(side, "MID".to_string()),
            ControlEvent::MoveToFolder {
                item: FolderItem::Folder(top),
                folder: Some(leaf),
            },
        ] {
            let (err, rollback) = h.send(event.clone());
            assert!(err.is_some(), "{event:?} was accepted");
            assert!(rollback.is_none());
        }
        assert_eq!(h.snapshot(), before);

        h.round_trip(ControlEvent::CreateFolder {
            id: 9,
            kind: FolderKind::Groups,
            parent: Some(side),
            name: "New".to_string(),
        });
        h.round_trip(ControlEvent::RenameFolder(mid, "Middle".to_string()));
        h.round_trip(ControlEvent::ReorderFolder(mid, 7));
        for (item, folder) in [
            (FolderItem::Group(0), Some(side)),
            (FolderItem::Group(3), Some(leaf)),
            (FolderItem::Scene(0), Some(songs)),
            (FolderItem::Scene(1), None),
            (FolderItem::Palette(0), Some(colors)),
            (FolderItem::Folder(mid), Some(side)),
            (FolderItem::Folder(leaf), None),
        ] {
            h.round_trip(ControlEvent::MoveToFolder { item, folder });
        }
        // Nested child folders and items, then a root folder, then the other
        // kinds.
        h.round_trip(ControlEvent::DeleteFolder(mid));
        h.round_trip(ControlEvent::DeleteFolder(top));
        h.round_trip(ControlEvent::DeleteFolder(songs));
        h.round_trip(ControlEvent::DeleteFolder(colors));

        // A group with a fixture filter is selected; the folder adds groups
        // 0..=2, which clears the filter. The rollback restores both.
        h.ok(ControlEvent::SelectGroup(3));
        h.ok(ControlEvent::LimitSelectionToFixtureInCurrentGroup(4));
        h.ok(ControlEvent::LimitSelectionToFixtureInCurrentGroup(1));
        h.round_trip(ControlEvent::SelectFolder(top));
        assert_eq!(h.snapshot().group_ids, HashSet::from([3]));
        assert_eq!(h.snapshot().fixtures_in_group, HashSet::from([1, 4]));
    }
}

/// Bus messages through the real `DmxEngine`: replies go to their sender
/// only, refusals included, and rollbacks keep the sender's envelope.
#[cfg(test)]
mod command_result_tests {
    use super::DmxEngine;
    use crate::{
        event::SystemEventBus,
        frontend_ui::FrontendUiMirror,
        hub::{
            BusBody, BusMessage, ClientConnection, ClientId, CommandOutcome, CommandResult,
            CreatedId, HostCommand, SessionId, SystemHub,
        },
        msg::SystemMessage,
        state::AppState,
    };
    use blaulicht_shared::{
        palette::PaletteKind, ControlEvent, ControlEventMessage, EventOriginator, HSVColor,
    };
    use std::{sync::Arc, time::Duration};
    use web_time::Instant;

    fn results(client: &ClientConnection) -> Vec<CommandResult> {
        std::iter::from_fn(|| client.try_recv_system())
            .filter_map(|message| match message {
                SystemMessage::CommandResult(result) => Some(result),
                _ => None,
            })
            .collect()
    }

    /// Ticks the engine's bus processing until `client` got `count` replies.
    fn wait_for_results(
        engine: &mut DmxEngine,
        client: &ClientConnection,
        count: usize,
    ) -> Vec<CommandResult> {
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut got = Vec::new();
        while got.len() < count {
            assert!(Instant::now() < deadline, "only got {got:?}");
            engine.apply_bus_messages();
            got.extend(results(client));
            std::thread::sleep(Duration::from_millis(1));
        }
        got
    }

    #[test]
    fn replies_reach_only_their_sender_and_rollbacks_keep_the_envelope() {
        let app = Arc::new(AppState::new(&[]));
        let hub = SystemHub::new();
        let mut bus = SystemEventBus::<BusMessage>::new();
        let handle = bus.handle();
        let mut engine = DmxEngine::new(
            Arc::clone(&app),
            handle.connect(),
            hub.clone(),
            [String::new(), String::new()],
        );
        let mirror = FrontendUiMirror::new();
        let desk =
            ClientConnection::connect(ClientId::Desk, SessionId::DESK, &hub, &handle, &mirror);
        let remote_id = hub.allocate_client();
        let remote = ClientConnection::connect_new_session(remote_id, &hub, &handle, &mirror);
        std::thread::spawn(move || bus.run());

        // An engine refusal: group 9 doesn't exist.
        let seq = remote.send(ControlEventMessage::new(
            EventOriginator::Web,
            ControlEvent::SelectGroup(9),
        ));
        assert_eq!(
            wait_for_results(&mut engine, &remote, 1),
            vec![CommandResult {
                client: remote_id,
                seq,
                outcome: CommandOutcome::Refused("Illegal group".to_string()),
            }]
        );

        // The desk got no reply, but sees the rollback on the bus with the
        // remote's envelope and the engine as originator.
        let deadline = Instant::now() + Duration::from_secs(5);
        let rollback = loop {
            assert!(Instant::now() < deadline, "no rollback on the bus");
            match desk.try_recv_bus() {
                Some(BusMessage {
                    body: BusBody::Control(event),
                    client,
                    session,
                    seq: rollback_seq,
                }) if event.originator() == EventOriginator::DmxEngine => {
                    break (client, session, rollback_seq, event.body())
                }
                Some(_) => {}
                None => std::thread::sleep(Duration::from_millis(1)),
            }
        };
        assert_eq!(
            (rollback.0, rollback.1, rollback.2),
            (remote_id, remote.session(), seq)
        );
        assert!(matches!(rollback.3, ControlEvent::DeSelectGroup(9)));

        // A created id, and a host command.
        let create = desk.send(ControlEventMessage::new(
            EventOriginator::Web,
            ControlEvent::CreatePalette("Red".to_string(), PaletteKind::Color(HSVColor::default())),
        ));
        let ping = desk.send_host(HostCommand::Ping);
        assert_eq!(
            wait_for_results(&mut engine, &desk, 2),
            vec![
                CommandResult {
                    client: ClientId::Desk,
                    seq: create,
                    outcome: CommandOutcome::Ok {
                        created: Some(CreatedId::Palette(0)),
                    },
                },
                CommandResult {
                    client: ClientId::Desk,
                    seq: ping,
                    outcome: CommandOutcome::Ok { created: None },
                },
            ]
        );
        assert!(app.dmx_engine.read().unwrap().0.palettes.contains_key(&0));

        // The engine's own rollback notice got no reply, and nothing leaked
        // to the other client.
        for _ in 0..20 {
            engine.apply_bus_messages();
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(results(&remote).is_empty());
        assert!(results(&desk).is_empty());
    }
}

#[cfg(test)]
mod host_command_tests {
    use super::templates::{selection_animations, ANIMATION_LIMIT_REACHED};
    use crate::{
        client::{
            testing::{small_rig, TwoClients},
            EngineClient,
        },
        hub::{CommandOutcome, CreatedId, HostCommand},
    };
    use blaulicht_shared::{
        scene::FixtureSelection, AnimationPreset, AnimationSpec, AnimationTemplate, ControlEvent,
        ControlEventMessage, EventOriginator,
    };

    const OK: CommandOutcome = CommandOutcome::Ok { created: None };

    fn web(event: ControlEvent) -> ControlEventMessage {
        ControlEventMessage::new(EventOriginator::Web, event)
    }

    /// The desk selects group 0 in scene 0; the remote selects fixture 1 of
    /// group 1 in scene 1.
    fn two_selections() -> TwoClients {
        let mut h = TwoClients::start(small_rig);
        assert_eq!(
            h.engine
                .round_trip(&h.desk, web(ControlEvent::SelectGroup(0))),
            OK
        );
        for event in [
            ControlEvent::SetSceneFocus(1),
            ControlEvent::SelectGroup(1),
            ControlEvent::LimitSelectionToFixtureInCurrentGroup(1),
        ] {
            assert_eq!(h.engine.round_trip(&h.remote, web(event)), OK);
        }
        h
    }

    /// The animations on the client's selection in its focused scene.
    fn own_animations(client: &impl EngineClient) -> Vec<u8> {
        client
            .with_session(|show, me| {
                let selection = me.fixture_selection(&show.0.groups);
                show.get_scene(me.scene_focus)
                    .and_then(|scene| selection_animations(scene, &selection))
                    .map(|animations| animations.keys().copied().collect())
                    .unwrap_or_default()
            })
            .unwrap()
    }

    #[test]
    fn a_preset_toggles_on_the_senders_selection_and_focus() {
        let mut h = two_selections();
        let toggle = HostCommand::ToggleAnimationPreset(AnimationPreset::KickFlash);
        assert!(toggle.edits_show());

        // The first tap creates the template and applies it.
        let CommandOutcome::Ok {
            created: Some(CreatedId::AnimationTemplate(id)),
        } = h.engine.round_trip(&h.remote, toggle.clone())
        else {
            panic!("the template is created");
        };
        h.desk.with_show(|show| {
            assert_eq!(show.0.animation_templates[&id].spec.name, "Kick Flash");
            let scene = &show.0.scenes[&1];
            let selection = FixtureSelection {
                fixtures: vec![(1, 1)],
            };
            let animation = &selection_animations(scene, &selection).unwrap()[&id];
            assert!(animation.enabled, "an applied preset runs");
            assert!(show.0.scenes[&0].sink.active_animations.is_empty());
        });
        assert_eq!(own_animations(&h.remote), vec![id]);
        assert!(own_animations(&h.desk).is_empty());

        // The desk reuses the template, on its own selection.
        let templates = h.desk.with_show(|show| show.0.animation_templates.len());
        assert_eq!(h.engine.round_trip(&h.desk, toggle.clone()), OK);
        assert_eq!(own_animations(&h.desk), vec![id]);
        assert_eq!(
            h.desk.with_show(|show| show.0.animation_templates.len()),
            templates
        );

        // A second tap removes it again, for that session only.
        assert_eq!(h.engine.round_trip(&h.remote, toggle), OK);
        assert!(own_animations(&h.remote).is_empty());
        assert_eq!(own_animations(&h.desk), vec![id]);
    }

    /// The Programmer's M. Alpha / M. Speed are events now; they must not
    /// clear the attribute faders (the control buffer) the way selection
    /// events do.
    #[test]
    fn scene_master_faders_keep_the_control_buffer() {
        let mut h = two_selections();
        let buffer = |client: &crate::client::LocalClient| {
            client
                .with_session(|_, me| format!("{:?}", me.control_buffer))
                .unwrap()
        };
        assert_eq!(
            h.engine
                .round_trip(&h.desk, web(ControlEvent::SetColorHue(200))),
            OK
        );
        let set = buffer(&h.desk);
        assert_ne!(set, buffer(&h.remote), "the hue is in the desk's buffer");
        for event in [
            ControlEvent::SetSceneMasterAlpha(0, 40),
            ControlEvent::SetSceneMasterSpeed(0, blaulicht_shared::AnimationSpeedModifier::_1),
        ] {
            assert_eq!(h.engine.round_trip(&h.desk, web(event)), OK);
        }
        assert_eq!(buffer(&h.desk), set);
        assert_eq!(
            h.desk
                .with_show(|show| show.0.scenes[&0].sink.master_alpha_fader),
            40
        );
    }

    #[test]
    fn a_preset_is_refused_when_every_template_id_is_taken() {
        let mut h = two_selections();
        h.engine.edit_show(|show| {
            for id in 0..=u8::MAX {
                let mut spec = AnimationSpec::empty();
                spec.name = "Full".to_string();
                show.0
                    .animation_templates
                    .insert(id, AnimationTemplate { spec });
            }
        });
        assert_eq!(
            h.engine.round_trip(
                &h.remote,
                HostCommand::ToggleAnimationPreset(AnimationPreset::HueDrift)
            ),
            CommandOutcome::Refused(ANIMATION_LIMIT_REACHED.to_string())
        );
        assert!(own_animations(&h.remote).is_empty());
    }

    #[test]
    fn a_preset_without_selection_is_refused() {
        let mut h = TwoClients::start(small_rig);
        let CommandOutcome::Refused(_) = h.engine.round_trip(
            &h.remote,
            HostCommand::ToggleAnimationPreset(AnimationPreset::KickFlash),
        ) else {
            panic!("nothing is selected");
        };
    }

    #[test]
    fn saving_an_animation_reads_the_senders_focused_scene() {
        let mut h = two_selections();
        let CommandOutcome::Ok {
            created: Some(CreatedId::AnimationTemplate(preset_id)),
        } = h.engine.round_trip(
            &h.remote,
            HostCommand::ToggleAnimationPreset(AnimationPreset::TempoSweep),
        )
        else {
            panic!("the template is created");
        };
        let save = HostCommand::SaveAnimationAsTemplate {
            selection: FixtureSelection {
                fixtures: vec![(1, 1)],
            },
            animation_id: preset_id,
            name: "  My Sweep ".to_string(),
        };
        assert!(save.edits_show());

        // The desk's focus (scene 0) has no such animation.
        assert_eq!(
            h.engine.round_trip(&h.desk, save.clone()),
            CommandOutcome::Refused("Animation not found".to_string())
        );

        let CommandOutcome::Ok {
            created: Some(CreatedId::AnimationTemplate(saved_id)),
        } = h.engine.round_trip(&h.remote, save)
        else {
            panic!("the template is saved");
        };
        assert_ne!(saved_id, preset_id);
        h.desk.with_show(|show| {
            let saved = &show.0.animation_templates[&saved_id].spec;
            assert_eq!(saved.name, "My Sweep");
            assert_eq!(
                saved.property,
                show.0.animation_templates[&preset_id].spec.property
            );
        });
    }

    #[test]
    fn cloning_animation_state_copies_it_and_restarts_the_target() {
        let mut h = TwoClients::start(small_rig);
        let key = "animation_plugin::sparkle";
        h.engine.app().plugin_state_storage.lock().unwrap().insert(
            key.to_string(),
            r#"{"version":1,"animation_instances":{"7":"saved"}}"#.to_string(),
        );
        let clone = HostCommand::CloneAnimationInstanceState {
            plugin_key: "sparkle".to_string(),
            source: 7,
            target: 9,
        };
        assert!(!clone.edits_show());
        assert_eq!(h.engine.round_trip(&h.remote, clone), OK);

        let stored = h.engine.app().plugin_state_storage.lock().unwrap()[key].clone();
        let stored: serde_json::Value = serde_json::from_str(&stored).unwrap();
        assert_eq!(stored["animation_instances"]["7"], "saved");
        assert_eq!(stored["animation_instances"]["9"], "saved");
        assert!(h
            .engine
            .app()
            .wasm_animation_instances_to_reset
            .lock()
            .unwrap()
            .contains(&9));
    }

    #[test]
    fn an_editor_keepalive_registers_the_editor() {
        let mut h = TwoClients::start(small_rig);
        let keepalive = HostCommand::KeepAnimationEditorAlive {
            plugin_key: "sparkle".to_string(),
            instance_id: 42,
            fixtures: vec![(0, 1), (2, 0)],
        };
        assert!(!keepalive.edits_show());
        assert_eq!(h.engine.round_trip(&h.remote, keepalive), OK);

        let requests = h
            .engine
            .app()
            .wasm_animation_editor_requests
            .read()
            .unwrap();
        let request = &requests[&42];
        assert_eq!(request.plugin_key, "sparkle");
        assert_eq!(request.instance_id, 42);
        assert_eq!(request.fixtures, vec![(0, 1), (2, 0)]);
    }

    /// The Programmer draws from `clone_for_session`: the sender's selection
    /// and focused scene in the `DESK` accessors.
    #[test]
    fn a_session_snapshot_reads_that_sessions_fields() {
        let h = two_selections();
        let session = h.remote.session();
        let (remote, desk) = h.desk.with_show(|show| {
            (
                show.clone_for_session(session).unwrap(),
                show.clone_for_session(crate::hub::SessionId::DESK).unwrap(),
            )
        });
        assert_eq!(remote.0.current_scene_focus, 1);
        assert_eq!(remote.curr_scene().name, "A");
        assert_eq!(
            remote
                .selection()
                .group_ids
                .iter()
                .copied()
                .collect::<Vec<_>>(),
            vec![1]
        );
        assert_eq!(remote.get_selection().fixtures, vec![(1, 1)]);
        let desk_in_remote = remote.session(crate::hub::SessionId::DESK).unwrap();
        assert_eq!(desk_in_remote.scene_focus, 0);
        assert_eq!(remote.session(session).unwrap().scene_focus, 1);

        assert_eq!(desk.0.current_scene_focus, 0);
        assert_eq!(
            desk.selection()
                .group_ids
                .iter()
                .copied()
                .collect::<Vec<_>>(),
            vec![0]
        );
        assert_eq!(desk.session(session).unwrap().scene_focus, 1);
    }
}
