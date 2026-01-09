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
    msg::SystemMessage,
    state::{AppState, DmxHealth, NUM_DMX_UNIVERSES},
};
use blaulicht_shared::{
    fixture::state::{FixtureState, MergeStrategy},
    scene::FixtureSelection,
    ActiveAnimation, ControlEvent, ControlEventMessage, EventOriginator, LogLevel,
    CONTROLS_REQUIRING_SELECTION,
};
use crossbeam_channel::Sender;
use log::{debug, error, warn};
use std::{
    collections::BTreeMap,
    mem,
    net::UdpSocket,
    sync::{Arc, RwLockWriteGuard},
    time::{Duration, Instant},
};

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
    animation_base_times: BTreeMap<u8, f64>,
    start_time: Instant,

    pub dmx_universe_ports: [Option<Box<dyn SerialPort>>; 2],
    artnet_output: DmxEngineArtnetOutput,

    running_setup: bool,
    setup_start_time: Instant,
}

const SETUP_SECS: u64 = 10;

impl DmxEngine {
    pub fn start_setup(&mut self) {
        self.running_setup = true;
        self.setup_start_time = Instant::now();
        self.system_out
            .send(SystemMessage::Log(
                "[DMX] Engine setup started...".to_string(),
                LogLevel::Debug,
            ))
            .unwrap();
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
                sys.send(SystemMessage::Log(
                    format!("[DMX] iface {port_path} (baud = {baud_rate}): OK"),
                    LogLevel::Info,
                ))
                .unwrap();
                (Some(port), DmxHealth::healthy(port_path.to_string()))
            }
            Err(err) => {
                let error_message = format!("[DMX] Could not establish link to interface \"{port_path}\" (baud = {baud_rate}): {err}");
                sys.send(SystemMessage::Log(error_message.clone(), LogLevel::Err))
                    .unwrap();
                (
                    None,
                    DmxHealth::error(port_path.to_string(), error_message),
                )
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

                    system_out
                        .send(SystemMessage::Log(
                            "Initialized ArtNet".to_string(),
                            LogLevel::Debug,
                        ))
                        .unwrap();

                    Some(s)
                }
                Err(err) => {
                    health_state.artnet_health_state = false;

                    system_out
                        .send(SystemMessage::Log(
                            format!("Could not create ARTNET socket: {err}"),
                            LogLevel::Err,
                        ))
                        .unwrap();
                    None
                }
            }
        };
        let artnet_output = DmxEngineArtnetOutput {
            universe_buffers: std::array::from_fn(|_| vec![0; 512]),
            socket,
        };

        mem::drop(health_state);

        Self {
            state_ref,
            dmx_previous: [0; 513],
            event_bus_connection,
            system_out,
            animation_base_times: BTreeMap::new(),
            start_time: Instant::now(),
            dmx_universe_ports,
            artnet_output,
            running_setup: false,
            setup_start_time: Instant::now(),
        }
    }

    /// Returns whether the DMX buffer changed.
    fn tick_internal(&mut self, audio_output: &CollectorOutput) -> bool {
        // Read back events.
        let mut events = vec![];
        while let Some(ev) = self.event_bus_connection.try_recv() {
            events.push(ev)
        }

        if self.running_setup {
            self.run_setup();

            if self.setup_start_time.elapsed().as_secs() >= SETUP_SECS {
                self.running_setup = false;
                self.system_out
                    .send(SystemMessage::Log(
                        "[DMX] Engine setup complete.".to_string(),
                        LogLevel::Info,
                    ))
                    .unwrap();
            }

            return true;
        }

        // Clear DMX buffers (slow).
        for buffer in self.state_ref.dmx_universes.iter() {
            let mut buffer = buffer.write().unwrap();
            buffer.dmx_buffer.fill(0);
        }

        // Advance animations.
        self.build_animations_cache(audio_output.snapshot);
        self.animation_tick(audio_output);

        // let mut state = self.state_ref.dmx_engine.write().unwrap();
        // state.groups().iter().flat_map(|g|g.values());

        if !events.is_empty() {
            let mut state = self.state_ref.dmx_engine.write().unwrap();

            for ev in events {
                let (msg, event) = self.apply(&mut state, ev);

                if let Some(msg) = msg {
                    self.system_out
                        .send(SystemMessage::Log(msg.to_string(), LogLevel::Debug))
                        .unwrap();
                }

                if let Some(ev) = event {
                    self.event_bus_connection
                        .send(ControlEventMessage::new(EventOriginator::DmxEngine, ev));
                }
            }
        }

        // Only read if there were no events.
        self.update_dmx_buffer();
        // TODO: flatten to DMX buffer.

        // Return true if the dmx buffer changed.
        // let mut buffer = self.state_ref.dmx_buffer.write().unwrap();
        // if buffer.dmx_buffer != self.dmx_previous {
        //     self.dmx_previous = buffer.dmx_buffer;
        //     true
        // } else {
        //     false
        // }

        // TODO: this always returns true in the current impl
        true
    }

    pub fn tick(&mut self, audio_snapshot: &CollectorOutput) {
        if self.tick_internal(audio_snapshot) {
            // TODO: THIS might be too heavy.
            // self.system_out
            //     .send(SystemMessage::DMX(Box::new(self.dmx)))
            //     .unwrap();
        }
    }

    fn write_to_output(&mut self) {
        let start = Instant::now();
        self.write_to_artnet();
        self.write_to_serial();
        log::debug!("DMX HW OUTPUT TIME: {:?}", start.elapsed());
    }

    fn write_to_artnet(&mut self) {
        let Some(ref mut socket) = self.artnet_output.socket else {
            return;
        };

        let artnet_out = self.state_ref.artnet_output.read().unwrap();

        for (universe_no, universe_lock) in self.state_ref.dmx_universes.iter().enumerate() {
            let buffer = universe_lock.read().unwrap();

            // NOTE: removing the first element since internally, we use a length of 513.
            self.artnet_output.universe_buffers[universe_no]
                .copy_from_slice(&buffer.dmx_buffer[1..]);

            let command = ArtCommand::Output(artnet_protocol::Output {
                port_address: (universe_no as u8).into(),
                sequence: 0, // 3. Disable sequence checking for stability
                physical: universe_no as u8,
                // TODO: this is evil.
                data: self.artnet_output.universe_buffers[universe_no]
                    .clone()
                    .into(),
                ..artnet_protocol::Output::default()
            });

            let bytes = command.write_to_buffer().unwrap();

            for destination in &artnet_out.receivers {
                if let Err(err) = socket.send_to(&bytes, destination) {
                    log::error!("Send ArtNet UDP to {destination}: {err:?}");
                }
            }
        }
    }

    fn write_to_serial(&mut self) {
        for (universe_no, mut port) in self.dmx_universe_ports.iter_mut().enumerate() {
            let buffer = self.state_ref.dmx_universes[universe_no].read().unwrap();

            if let Some(port) = &mut port {
                port.set_break().unwrap();
                spin_sleep::sleep(Duration::from_micros(100));
                port.clear_break().unwrap();
                spin_sleep::sleep(Duration::from_micros(12));

                // Write frame
                port.write_all(&buffer.dmx_buffer).unwrap();
            };
        }
    }

    fn run_setup(&mut self) {
        let state = self.state_ref.dmx_engine.read().unwrap();

        for group in &state.0.groups {
            for fixture in &group.1.fixtures {
                let fix = fixture.1;

                let time = (Instant::now().duration_since(self.start_time)).as_millis() as u64;

                println!("SETUP T: {time}");

                let mut buffer = self.state_ref.dmx_universes[fix.universe_no]
                    .write()
                    .unwrap();

                fix.setup(
                    time as i32,
                    &FixtureState::default(),
                    &mut buffer.dmx_buffer,
                );
            }
        }

        mem::drop(state);

        self.write_to_output();
    }

    fn update_dmx_buffer(&mut self) {
        let state = self.state_ref.dmx_engine.read().unwrap();

        // For each fixture, merge all scene states.
        for group in &state.0.groups {
            for fixture in &group.1.fixtures {
                let curr_scene = state.curr_scene();

                // Apply base scene state.
                let mut merged_state = curr_scene
                    .sink
                    .fixture_states
                    .get(&(*group.0, *fixture.0))
                    .unwrap()
                    .clone();

                // Apply master alpha of this scene on the scene fixture state.
                //  0-255                         / 0 - 100
                merged_state.alpha = (merged_state.alpha as f32 / 100.0
                    * curr_scene.sink.master_alpha_fader as f32)
                    as u8;

                for overlay_id in &state.0.current_overlay_scenes {
                    let this_scene = state.0.scenes.get(overlay_id).unwrap();
                    let mut scene_fixture_state = this_scene
                        .sink
                        .fixture_states
                        .get(&(*group.0, *fixture.0))
                        .unwrap()
                        .clone();

                    // Apply master alpha of this scene on the scene fixture state.
                    //  0-255                         / 0 - 100
                    scene_fixture_state.alpha = (scene_fixture_state.alpha as f32 / 100.0
                        * this_scene.sink.master_alpha_fader as f32)
                        as u8;

                    let changeset = this_scene.get_fixture_changeset(*group.0, *fixture.0);
                    for change in changeset {
                        // TODO: pull change into merged state.
                        merged_state.merge_from(
                            &scene_fixture_state,
                            change,
                            MergeStrategy::Highest, // WAS HIGHEST ONCE
                        );
                    }
                }

                // TODO: we will need to use the merged fixture states here and then write them.
                let fix = fixture.1;
                let mut buffer = self.state_ref.dmx_universes[fix.universe_no]
                    .write()
                    .unwrap();

                fix.write(&merged_state, &mut buffer.dmx_buffer);
            }
        }

        // Apply overrides
        for ((universe, chan), value) in &state.0.overrides {
            if *universe >= self.state_ref.dmx_universes.len() {
                warn!("Ignoring override: UNI: {universe} CHAN: {chan} -> VAL: {value}; out of universes");
                continue;
            }
            let mut buffer = self.state_ref.dmx_universes[*universe].write().unwrap();
            buffer.dmx_buffer[*chan] = *value;
        }

        mem::drop(state);

        self.write_to_output();
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
                for t_ev in t {
                    // debug!("Apply transaction: {:?}", &ev);

                    let (err, rollback) =
                        self.apply(state, ControlEventMessage::new(ev.originator(), t_ev));

                    if err.is_some() {
                        error!("Error during transaction: {err:?}");
                        return (err, rollback);
                    }
                }

                (None, None)
            }
            // Overrides
            ControlEvent::SetChannelOverride(uni, chan, value) => {
                if chan > 512 {
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
                    (None, None)
                }
            }
            ControlEvent::LimitSelectionToFixtureInCurrentGroup(fixture_id) => {
                if state.selection().group_ids.len() != 1 {
                    return (
                        Some("Exactly 1 group shall be selected"),
                        Some(ControlEvent::UnLimitSelectionToFixtureInCurrentGroup(
                            fixture_id,
                        )),
                    );
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
                if state.selection().is_empty() {
                    (Some("No selection"), None)
                } else {
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

                println!("new selection after push: {selection:?}");

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
                println!("MISC: Not implemented in DMX: {descriptor:?} | {value:?}");
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

                let animations = state.0.animations.clone();

                let anim = &mut state.curr_scene_mut().sink.active_animations;
                for (_selec, anim_set) in anim.iter_mut() {
                    for (anim_id, anim) in anim_set.iter_mut() {
                        // anim.reset();
                        let animation_sync = {
                            let anim = match animations.get(anim_id) {
                                Some(a) => a,
                                None => {
                                    println!("WARN: animation not found");
                                    continue;
                                }
                            };
                            let anim = anim.clone();
                            anim.sync
                        };
                        anim.set_timers(animation_sync);
                        println!("Reset animation: {anim_id}");
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
            ControlEvent::SetOverlays(overlays) => {
                if overlays.contains(&state.0.current_scene_focus) {
                    return (Some("Base scene cannot appear in overlays"), None);
                }

                // if !state.0.scenes.get(&id).is_some() {
                //     return (Some("Illegal scene"), Some(ControlEvent::SetSceneFocus(0)));
                // }

                // PATCH: reset all animations in that scene
                // state.0.current_scene_focus = id;

                state.0.current_overlay_scenes.clear();

                println!("SET OVERLAYS: {overlays:?}");

                for scene in &overlays {
                    let animations = state.0.animations.clone();

                    let anim = &mut state
                        .0
                        .scenes
                        .get_mut(scene)
                        .unwrap()
                        .sink
                        .active_animations;
                    for (_selec, anim_set) in anim.iter_mut() {
                        for (anim_id, anim) in anim_set.iter_mut() {
                            // anim.reset();
                            let animation_sync = {
                                let anim = match animations.get(anim_id) {
                                    Some(a) => a,
                                    None => {
                                        println!("WARN: animation not found");
                                        continue;
                                    }
                                };
                                let anim = anim.clone();
                                anim.sync
                            };
                            anim.set_timers(animation_sync);
                            println!("Reset animation: {anim_id}");
                        }
                    }

                    state.0.current_overlay_scenes.push(*scene);
                }
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
                let this_scene = state.0.scenes.get_mut(&current_scene_focus).unwrap();
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
                        selec_anim.insert(id, ActiveAnimation::new(&curr_selection.fixtures));

                        // Get the source animation to determine its property.
                        let animation = state.0.animations.get(&id).unwrap();

                        (None, None, Some(vec![animation.property]))
                    }
                }
            }
            ControlEvent::SetAnimationSpeed(id, md) => {
                println!("set speed anim");

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
                                println!("set speed: {md:?}");

                                // Purge selection if empty.
                                // if selec_anim.is_empty() {
                                //     this_scene.sink.active_animations.remove(curr_selection);
                                //     println!("moved selection entirely");
                                // }

                                anim.speed_factor = md;

                                let animation = state.0.animations.get(&id).unwrap();
                                (None, None, Some(vec![animation.property]))
                            }
                        }
                    }
                }
            }
            ControlEvent::RemoveAnimation(id) => {
                println!("REMove anim");

                let this_scene = state.0.scenes.get_mut(&current_scene_focus).unwrap();

                match !this_scene
                    .sink
                    .active_animations
                    .contains_key(curr_selection)
                {
                    true => {
                        println!("NO SELEC: {:?}", curr_selection);
                        println!("NO SELEC 2: {:?}", curr_selection.sorted());
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
                            Some(_) => {
                                println!("REMOVED");

                                // Purge selection if empty.
                                if selec_anim.is_empty() {
                                    this_scene.sink.active_animations.remove(curr_selection);
                                    println!("moved selection entirely");
                                }

                                let animation = state.0.animations.get(&id).unwrap();
                                (None, None, Some(vec![animation.property]))
                            }
                        }
                    }
                }
            }
            ControlEvent::PlayAnimation(id) => {
                let (animation_sync, anim_prop) = {
                    let anim = state.0.animations.get(&id).unwrap();
                    let anim = anim.clone();
                    (anim.sync, anim.property)
                };

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
                                anim.set_timers(animation_sync);
                                (None, None, Some(vec![anim_prop]))
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
                                let animation = state.0.animations.get(&id).unwrap();
                                (None, None, Some(vec![animation.property]))
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
            _ => {
                // NOTE: this applies the changeset internally on the sink.
                let this_scene = state.0.scenes.get_mut(&current_scene_focus).unwrap();

                this_scene
                    .sink
                    .apply_with_selection(curr_selection, ev.body());

                // Update control buffer for the UI.
                state.0.control_buffer.apply(ev.body());

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
