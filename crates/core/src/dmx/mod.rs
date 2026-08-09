/// This module deals with applying events on fixtures to produce a continuous DMX output.
mod clock;
mod management;
mod state;

use artnet_protocol::ArtCommand;
use blaulicht_audio_engine::CollectorOutput;
use serialport::SerialPort;
pub use state::*;
pub mod animation;
pub mod scene;
use crate::{
    event::SystemEventBusConnectionInst,
    msg::{DmxTickSpeeds, SystemMessage},
    state::{AppState, DmxHealth, NUM_DMX_UNIVERSES},
};
use blaulicht_shared::{
    fixture::{
        state::{FixtureState, MergeStrategy, ResolvedFixtureState},
        value::FixtureValue,
    },
    scene::{FixtureSelection, FixtureSelector},
    scene_graph::{AudioConditions, SceneGraphRuntime},
    ActiveAnimation, ControlEvent, ControlEventMessage, EventOriginator, FixtureProperty, LogLevel,
    CONTROLS_REQUIRING_SELECTION,
};
use crossbeam_channel::{bounded, Receiver, Sender, TrySendError};
use std::{
    collections::{BTreeMap, HashSet},
    mem,
    net::UdpSocket,
    sync::{Arc, RwLockWriteGuard},
    time::{Duration, Instant},
};
use tracing::{debug, error, warn};

// TODO: maybe fuse this together?

pub struct DmxEngineArtnetOutput {
    universe_buffers: [Vec<u8>; NUM_DMX_UNIVERSES],
    socket: Option<UdpSocket>,
}

pub struct DmxEngine {
    // TODO: check if this is too slow.
    state_ref: Arc<AppState>,
    dmx_previous: [u8; 513], // Starting at 1
    // TODO: add more, internal state.
    event_bus_connection: SystemEventBusConnectionInst,
    system_out: Sender<SystemMessage>,

    // This is not part of state_ref since this is only a cache
    // animation_base_times: BTreeMap<u8, f64>,
    start_time: Instant,

    output_tx: Sender<DmxOutputFrame>,

    running_setup: bool,
    setup_start_time: Instant,

    scene_graph_runtime: SceneGraphRuntime,
    /// Scene IDs the scene graph pushed into `current_overlay_scenes` last tick.
    /// Used to reconcile (add/remove) graph-driven overlays without clobbering
    /// manually-added ones. Runtime-only, not persisted.
    graph_overlay_scenes: Vec<u8>,
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
    while let Ok(frame) = receiver.recv() {
        let mut artnet_failed = false;
        if let Some(ref mut socket) = artnet.socket {
            let receivers = state_ref
                .artnet_output
                .read()
                .map(|output| output.receivers.clone())
                .unwrap_or_default();
            for (universe_no, universe) in frame.universes.iter().enumerate() {
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
                for destination in receivers.iter().filter(|destination| destination.enabled) {
                    if let Err(err) = socket.send_to(&bytes, destination.address) {
                        error!("Send ArtNet UDP to {}: {err:?}", destination.address);
                        artnet_failed = true;
                        if let Ok(mut health) = state_ref.health_data.write() {
                            health.artnet_health_state = false;
                        }
                        break;
                    }
                }
                if artnet_failed {
                    break;
                }
            }
        }
        if artnet_failed {
            artnet.socket = None;
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
                error!("[DMX] Output disabled on {port_path}: {err}");
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

impl DmxEngine {
    pub fn start_setup(&mut self) {
        self.running_setup = true;
        self.setup_start_time = Instant::now();
        let _ = self.system_out.send(SystemMessage::Log(
            "[DMX] Engine setup started...".to_string(),
            LogLevel::Debug,
        ));
    }

    fn open_hw_interface(
        port_path: &str,
        sys: Sender<SystemMessage>,
    ) -> (Option<Box<dyn SerialPort>>, DmxHealth) {
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
                let _ = sys.send(SystemMessage::Log(
                    format!("[DMX] iface {port_path} (baud = {baud_rate}): OK"),
                    LogLevel::Info,
                ));
                (Some(port), DmxHealth::healthy(port_path.to_string()))
            }
            Err(err) => {
                let error_message = format!("[DMX] Could not establish link to interface \"{port_path}\" (baud = {baud_rate}): {err}");
                let _ = sys.send(SystemMessage::Log(error_message.clone(), LogLevel::Err));
                (None, DmxHealth::error(port_path.to_string(), error_message))
            }
        }
    }

    pub fn new(
        state_ref: Arc<AppState>,
        event_bus_connection: SystemEventBusConnectionInst,
        system_out: Sender<SystemMessage>,
        universe_dmx_out_devices: [String; 2],
    ) -> Self {
        let mut health_state = state_ref.health_data.write().unwrap();

        //
        // Initialize DMX.
        //
        let mut dmx_universe_ports = [None, None];

        for (universe, port) in dmx_universe_ports.iter_mut().enumerate() {
            let (dmx_port, port_health_state) =
                Self::open_hw_interface(&universe_dmx_out_devices[universe], system_out.clone());

            health_state.dmx_universes_healthy[universe] = port_health_state;

            *port = dmx_port
        }

        //
        // Initialize ArtNet.
        //
        let socket = {
            match UdpSocket::bind("0.0.0.0:0") {
                Ok(s) => {
                    health_state.artnet_health_state = true;

                    let _ = system_out.send(SystemMessage::Log(
                        "Initialized ArtNet".to_string(),
                        LogLevel::Debug,
                    ));

                    Some(s)
                }
                Err(err) => {
                    health_state.artnet_health_state = false;

                    let _ = system_out.send(SystemMessage::Log(
                        format!("Could not create ARTNET socket: {err}"),
                        LogLevel::Err,
                    ));
                    None
                }
            }
        };
        let artnet_output = DmxEngineArtnetOutput {
            universe_buffers: std::array::from_fn(|_| vec![0; 512]),
            socket,
        };

        mem::drop(health_state);

        let (output_tx, output_rx) = bounded(2);
        let output_state = Arc::clone(&state_ref);
        let worker_result = std::thread::Builder::new()
            .name("dmx-output".to_string())
            .spawn(move || {
                output_worker(output_rx, dmx_universe_ports, artnet_output, output_state)
            });
        if let Err(err) = worker_result {
            error!("[DMX] Failed to start output worker: {err}");
        }

        {
            let mut engine_state = state_ref.dmx_engine.write().unwrap();
            let _ = engine_state.0.validate_palette_mapping_integrity(true);
        }

        Self {
            state_ref,
            dmx_previous: [0; 513],
            event_bus_connection,
            system_out,
            // animation_base_times: BTreeMap::new(),
            start_time: Instant::now(),
            output_tx,
            running_setup: false,
            setup_start_time: Instant::now(),
            scene_graph_runtime: SceneGraphRuntime::default(),
            graph_overlay_scenes: Vec::new(),
        }
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
                let _ = self.system_out.send(SystemMessage::Log(
                    "[DMX] Engine setup complete.".to_string(),
                    LogLevel::Info,
                ));
            }

            return;
        }

        // Read back events.
        let mut events = vec![];
        while let Some(ev) = self.event_bus_connection.try_recv() {
            events.push(ev)
        }

        {
            if !events.is_empty() {
                let mut state = self.state_ref.dmx_engine.write().unwrap();

                for ev in events {
                    let (msg, event) = self.apply(&mut state, ev);

                    if let Some(msg) = msg {
                        let _ = self
                            .system_out
                            .send(SystemMessage::Log(msg.to_string(), LogLevel::Debug));
                    }

                    if let Some(ev) = event {
                        self.event_bus_connection
                            .send(ControlEventMessage::new(EventOriginator::DmxEngine, ev));
                    }
                }

                let integrity = state.0.validate_palette_mapping_integrity(false);
                debug_assert!(
                    integrity.is_ok(),
                    "palette mapping integrity violated after applying events: {:?}",
                    integrity
                );
            }
        }

        // Advance animations.
        self.animation_tick(audio_output);

        // Advance scene graphs.
        self.scene_graph_tick(&audio_output.snapshot);

        self.render_universes();
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
            Err(TrySendError::Disconnected(_)) => error!("[DMX] Output worker is unavailable"),
        }
        now.elapsed()
    }

    fn run_setup(&mut self) {
        let state = self.state_ref.dmx_engine.read().unwrap();

        for group in &state.0.groups {
            for fixture in &group.1.fixtures {
                let fix = fixture.1;

                let time = (Instant::now().duration_since(self.start_time)).as_millis() as u64;

                debug!("SETUP T: {time}");

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
        self.scene_graph_runtime
            .tick_all(&mut state.0.scene_graphs, now_ms, &audio);

        // Reflect the scenes of every enabled graph's active node into the normal
        // overlay list, so they show up in the performance view and render through
        // the standard overlay path (no separate scene-graph render path).
        let base = state.0.current_scene_focus;
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

        // For each fixture, merge all scene states.
        for group in &state.0.groups {
            for fixture in &group.1.fixtures {
                let curr_scene = state.curr_scene();

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

                // Resolve to a concrete state, then apply master alpha.
                let mut resolved = merged_state.resolve(palettes);
                resolved.alpha = ((resolved.alpha as f32 / 100.0)
                    * curr_scene.sink.master_alpha_fader as f32)
                    as u8;

                for overlay_id in &state.0.current_overlay_scenes {
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
                            MergeStrategy::Highest, // WAS HIGHEST ONCE
                            palettes,
                        );
                    }
                    resolved = merged_for_overlay.resolve(palettes);
                }

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

        // Empties the fixture state buffer on selecion events.
        if !requires_selection {
            state.0.control_buffer.reset();
        }

        // Match event.
        match ev.body() {
            ControlEvent::Transaction(t) => {
                let snapshot = state.0.clone();
                for t_ev in t {
                    // debug!("Apply transaction: {:?}", &ev);

                    let (err, rollback) =
                        self.apply(state, ControlEventMessage::new(ev.originator(), t_ev));

                    if err.is_some() {
                        error!("Error during transaction: {err:?}");
                        state.0 = snapshot;
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
            ControlEvent::PluginUi(_, _) => (None, None),
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
                warn!("MISC: Not implemented in DMX: {descriptor:?} | {value:?}");
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
                if !state.0.scenes.contains_key(&id) {
                    return (Some("Illegal scene"), Some(ControlEvent::SetSceneFocus(0)));
                }

                // PATCH: reset all animations in that scene
                // BUG: this also starts all animations that were manually paused.
                // HOW TO FIX: ADD A SWITCH TO ANIMATIONS THAT DISABLE THEM.
                state.0.current_scene_focus = id;

                // let animations = state.0.animation_templates.clone();

                let anim = &mut state.curr_scene_mut().sink.active_animations;
                for (_selec, anim_set) in anim.iter_mut() {
                    for (anim_id, anim) in anim_set.iter_mut() {
                        // TODO: this can be done prettier.
                        if anim.enabled {
                            anim.reset_timers(anim.spec_cloned.sync_mode());
                            debug!("Reset animation: {anim_id}");
                        }
                    }
                }
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
                if !state.0.scenes.contains_key(&scene_id) {
                    return (Some("Illegal scene"), None);
                }

                if scene_id == state.0.current_scene_focus {
                    return (Some("Base scene cannot appear in overlays"), None);
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
                if overlays.contains(&state.0.current_scene_focus) {
                    return (Some("Base scene cannot appear in overlays"), None);
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
                        state
                            .0
                            .palettes
                            .insert(id, blaulicht_shared::palette::Palette { name, kind });
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
