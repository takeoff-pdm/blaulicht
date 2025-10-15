/// This module deals with applying events on fixtures to produce a continuous DMX output.
mod clock;
mod management;
mod state;

use map_range::MapRange;
use serde::{Deserialize, Serialize};
use serialport::SerialPort;
pub use state::*;
pub mod animation;
// pub use fixture::*;
pub mod scene;

use log::{debug, error, warn};

use crate::{
    audio::defs::DMX_TICK_TIME,
    dmx::{
        animation::{generate, phaser},
        clock::Time,
    },
    event::SystemEventBusConnectionInst,
    msg::SystemMessage,
    state::AppState,
};
use blaulicht_shared::{
    fixture::state::{FixtureState, MergeStrategy},
    scene::FixtureSelection,
    ActiveAnimation, AnimationSpec, AnimationSpecBody, CollectedAudioSnapshot, ControlEvent,
    ControlEventMessage, EventOriginator, FixtureProperty, LogLevel, PhaserDuration, RGBColor,
    SyncMode, CONTROLS_REQUIRING_SELECTION,
};
use crossbeam_channel::Sender;
use std::{
    collections::{BTreeMap, HashSet, VecDeque},
    mem,
    sync::{Arc, RwLockWriteGuard},
    time::{Duration, Instant},
};

// TODO: maybe fuse this together?

pub struct DmxEngine {
    // TODO: check if this is too slow.
    state_ref: Arc<AppState>,
    dmx_previous: [u8; 513], // Starting at 1
    // TODO: add more, internal state.
    event_bus_connection: SystemEventBusConnectionInst,
    system_out: Sender<SystemMessage>,

    // This is not part of state_ref since this is only a cache
    animation_base_times: BTreeMap<u8, u64>,
    start_time: Instant,

    dmx_universe_ports: [Option<Box<dyn SerialPort>>; 2],

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
    ) -> Option<Box<dyn SerialPort>> {
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
                Some(port)
            }
            Err(err) => {
                sys.send(SystemMessage::Log(
                format!("[DMX] Could not establish link to interface {port_path} (baud = {baud_rate}): {err}"), LogLevel::Err)).unwrap();
                None
            }
        }
    }

    pub fn new(
        state_ref: Arc<AppState>,
        event_bus_connection: SystemEventBusConnectionInst,
        system_out: Sender<SystemMessage>,
    ) -> Self {
        let mut dmx_universe_ports = [None, None];

        for (universe, port) in dmx_universe_ports.iter_mut().enumerate() {
            let dmx_port =
                Self::open_hw_interface(&format!("/dev/ttyUSB{universe}"), system_out.clone());
            *port = dmx_port
        }

        Self {
            state_ref,
            dmx_previous: [0; 513],
            event_bus_connection,
            system_out,
            animation_base_times: BTreeMap::new(),
            start_time: Instant::now(),
            dmx_universe_ports,
            running_setup: false,
            setup_start_time: Instant::now(),
        }
    }

    /// Returns whether the DMX buffer changed.
    fn tick_internal(&mut self, audio_snapshot: CollectedAudioSnapshot) -> bool {
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
        self.build_animations_cache(audio_snapshot);

        {
            // // TODO: what's the plan for this?
            // //
            // // Go over all scenes and then over all selections for that scene.
            //
            let now = (Instant::now().duration_since(self.start_time)).as_millis() as u64;
            let mut state = self.state_ref.dmx_engine.write().unwrap();
            let animations = state.0.animations.clone();

            for (scene_id, scene) in state.0.scenes.iter_mut() {
                for (selection, scene_animations) in scene.sink.active_animations.iter_mut() {
                    // println!("scene anim: {scene_animations:?}");

                    for (animation_id, animation) in scene_animations.iter_mut() {
                        if !animation.enabled {
                            continue;
                        }

                        for (fixture_selec, fixture_anim_state) in
                            animation.fixture_timers.iter_mut()
                        {
                            let transition_time =
                                (*self.animation_base_times.get(animation_id).unwrap()) as f64
                                    * animation.speed_factor.as_float();

                            // TODO: limited by tick speed

                            // println!("{}", now - animation.last_tick_time);

                            let mut num_ticks = 1;

                            let millis = DMX_TICK_TIME.as_millis();
                            if transition_time < millis as f64 {
                                num_ticks = (millis as f64 / transition_time) as usize;
                                // println!("NUM TICKS: {num_ticks}");
                            }

                            if transition_time == 0.0 {
                                continue;
                            }

                            // TODO: extremely naiive implementation
                            // FLAWS:
                            //  - beat-timing is not considered
                            //  - syncing between animations is also not considered
                            //      - Different sync modes
                            //          - No sync (when playing current animation, disregard everything and
                            //          start it)
                            //          - Group sync (sync with all other fixtures in the parent group that
                            //          also use this animation)
                            //          - Global (sync with ALL other fixtures (also from other groups)
                            //          that also use this animation)
                            //      - How is syncing done?
                            //      - when sync mode is changed, timing is reset to 0 for all fixtures and
                            //      the stepper logic uses the sync
                            //      - Syncing shall be displayed graphically
                            //      - Running animations shall also be displayed graphically
                            //      - Each phaser can be absolute / relative!
                            // println!("{}", fixture_anim_state.last_tick_time);
                            if now - fixture_anim_state.last_tick_time >= transition_time as u64 {
                                for _ in 0..num_ticks {
                                    fixture_anim_state.tick(now);
                                }

                                let spec = animations.get(animation_id).unwrap();

                                let v = self.generate_animation_value(
                                    audio_snapshot,
                                    spec,
                                    *animation_id,
                                    fixture_anim_state.timer,
                                );

                                let fixture_state =
                                    scene.sink.fixture_states.get_mut(fixture_selec).unwrap();

                                fixture_state.apply_value(v, spec.property);

                                // println!("update animation");
                            }
                        }
                    }
                }
            }

            //
            // let fixtures = state
            //     .groups
            //     .iter_mut()
            //     .flat_map(|(_, g)| g.fixtures.values_mut());
            //
            // for fixture in fixtures {
            // }
        }
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

    pub fn tick(&mut self, audio_snapshot: CollectedAudioSnapshot) {
        if self.tick_internal(audio_snapshot) {
            // TODO: THIS might be too heavy.
            // self.system_out
            //     .send(SystemMessage::DMX(Box::new(self.dmx)))
            //     .unwrap();
        }
    }

    fn generate_animation_value(
        &self,
        audio_snapshot: CollectedAudioSnapshot,
        spec: &AnimationSpec,
        id: u8,
        time: u64,
    ) -> u16 {
        // let animations = &self.state_ref.dmx_engine.read().unwrap().animations;
        // let animation = animations.get(&id).unwrap();

        match &spec.body {
            AnimationSpecBody::Phaser(body) => phaser::generate(body, time as f32),
            AnimationSpecBody::AudioVolume(animation_spec_body_audio_volume) => {
                audio_snapshot.volume as u16
            }
            AnimationSpecBody::Beat(animation_spec_body_beat) => {
                audio_snapshot.bass_avg_short as u16
            }
            AnimationSpecBody::Wasm(animation_spec_body_wasm) => todo!(),
        }
    }

    fn build_animations_cache(&mut self, audio_snapshot: CollectedAudioSnapshot) {
        // Only build every 100ms or so?

        let animations = &self.state_ref.dmx_engine.read().unwrap().0.animations;

        for (anim_id, anim) in animations {
            let total_speed_raw = {
                let animation_spec = animations.get(anim_id).unwrap();
                match &animation_spec.body {
                    AnimationSpecBody::Phaser(body) => match body.time_total {
                        PhaserDuration::Fixed(time) => time,
                        PhaserDuration::Beat(beats) => {
                            (audio_snapshot.time_between_beats_millis as f64 * beats.as_float())
                                as u64
                        }
                    },
                    AnimationSpecBody::AudioVolume(animation_spec_body_audio_volume) => 0,
                    AnimationSpecBody::Beat(animation_spec_body_beat) => 0,
                    AnimationSpecBody::Wasm(animation_spec_body_wasm) => todo!(),
                }
            };

            let speed_per_step = total_speed_raw / 360;

            self.animation_base_times.insert(*anim_id, speed_per_step);
        }
    }

    fn write_to_hw(&mut self) {
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

        self.write_to_hw();
    }

    fn update_dmx_buffer(&mut self) {
        let state = self.state_ref.dmx_engine.read().unwrap();

        // For each fixture, merge all scene states.

        for group in &state.0.groups {
            for fixture in &group.1.fixtures {
                // Apply base scene state.
                let mut merged_state = state
                    .curr_scene()
                    .sink
                    .fixture_states
                    .get(&(*group.0, *fixture.0))
                    .unwrap()
                    .clone();

                for overlay_id in &state.0.current_overlay_scenes {
                    let this_scene = state.0.scenes.get(overlay_id).unwrap();
                    let scene_fixture_state = this_scene
                        .sink
                        .fixture_states
                        .get(&(*group.0, *fixture.0))
                        .unwrap();

                    let changeset = this_scene.get_fixture_changeset(*group.0, *fixture.0);
                    for change in changeset {
                        // TODO: pull change into merged state.
                        merged_state.merge_from(
                            scene_fixture_state,
                            change,
                            MergeStrategy::Highest,
                        );
                    }
                }

                // merged_state.
                // todo!();

                let fix = fixture.1;
                // let state = self.state_ref.dmx_engine.write().unwrap();
                // let mut buffer = state.u

                // TODO: we will need to use the merged fixture states here and then write them.
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

        self.write_to_hw();
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
            ControlEvent::PluginUi(_) => {
                (None, None)
            }
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
            ControlEvent::SetSceneFocus(id) => {
                debug_assert!(state.0.scenes.get(&id).is_some());
                state.0.current_scene_focus = id;
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
                                anim.enabled = true;

                                // TIMING mode: spread or sync the timing.
                                let amount = anim.fixture_timers.len();
                                match animation_sync {
                                    SyncMode::Synced => {
                                        for (counter, (_, timer_state)) in
                                            anim.fixture_timers.iter_mut().enumerate()
                                        {
                                            timer_state.last_tick_time = 0;
                                            timer_state.timer = 0;
                                        }
                                    }
                                    SyncMode::StretchedEven => {
                                        for (counter, (_, timer_state)) in
                                            anim.fixture_timers.iter_mut().enumerate()
                                        {
                                            timer_state.last_tick_time = 0;
                                            timer_state.timer =
                                                ((360.0 / amount as f32) * counter as f32) as u64;

                                            println!("timer: {}", timer_state.timer);
                                        }
                                    }
                                    SyncMode::StretchedHalfHalf => {
                                        for (counter, (_, timer_state)) in
                                            anim.fixture_timers.iter_mut().enumerate()
                                        {
                                            timer_state.last_tick_time = 0;
                                            timer_state.timer = (180 * (counter % 2)) as u64;
                                        }
                                    }
                                }

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
