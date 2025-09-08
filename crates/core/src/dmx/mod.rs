/// This module deals with applying events on fixtures to produce a continuous DMX output.
mod clock;
mod fixture;
mod state;

use map_range::MapRange;
use serde::{Deserialize, Serialize};
use serialport::SerialPort;
pub use state::*;
pub mod animation;
pub use fixture::*;
pub mod scene;

use log::{debug, error};

use crate::{
    audio::defs::DMX_TICK_TIME,
    dmx::{
        animation::{AnimationSpec, AnimationSpecBody, PhaserDuration},
        scene::{EngineSink, Scene},
    },
    event::SystemEventBusConnectionInst,
    msg::SystemMessage,
    routes::AppState,
};
use blaulicht_shared::{
    CollectedAudioSnapshot, ControlEvent, ControlEventMessage, EventOriginator, FixtureProperty,
    RGBColor, CONTROLS_REQUIRING_SELECTION,
};
use crossbeam_channel::Sender;
use std::{
    collections::{BTreeMap, HashSet, VecDeque},
    mem,
    sync::{Arc, RwLockWriteGuard},
    thread,
    time::{Duration, Instant},
};

#[derive(Clone, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
pub struct FixtureSelection {
    fixtures: Vec<(u8, u8)>,
}

impl FixtureSelection {
    pub fn generate_instructions(&self) -> VecDeque<ControlEvent> {
        let mut gids = HashSet::new();
        let mut fids = HashSet::new();

        for (gid, fid) in &self.fixtures {
            gids.insert(gid);
            fids.insert(fid);
        }

        match (gids.len(), fids.len()) {
            (1, 1) => vec![
                ControlEvent::SelectGroup(**gids.iter().next().unwrap()),
                ControlEvent::LimitSelectionToFixtureInCurrentGroup(**fids.iter().next().unwrap()),
            ]
            .into(),
            (_, _) => {
                let mut group_instr = VecDeque::new();

                for gid in gids {
                    group_instr.push_back(ControlEvent::SelectGroup(*gid))
                }

                group_instr
            }
        }
    }
}

// impl Fixture {
//     fn apply(&mut self, ev: ControlEvent) {
//         // match ev {
//         //     ControlEvent::SetEnabled(enabled) => {
//         //         todo!("not supported");
//         //     }
//         //     ControlEvent::SetBrightness(brightness) => {
//         //         self.set_alpha(brightness);
//         //     }
//         //     ControlEvent::SetColor(clr) => {
//         //         self.set_color(clr);
//         //     }
//         //     ControlEvent::MiscEvent { descriptor, value } => todo!(),
//         //     _ => {}
//         // }
//         self.state.apply(ev);
//     }
// }
//
// TODO: maybe fuse this together?

impl FixtureState {
    fn reset(&mut self) {
        *self = Self::default();
    }

    fn apply(&mut self, ev: ControlEvent) -> Vec<FixtureProperty> {
        match ev {
            ControlEvent::SetAlpha(alpha) => {
                self.alpha = alpha;
                vec![FixtureProperty::Alpha]
            }
            ControlEvent::SetColor(clr) => {
                let color: RGBColor = clr.into();
                self.color = color.into();
                vec![
                    FixtureProperty::ColorHue,
                    FixtureProperty::ColorSaturation,
                    FixtureProperty::ColorValue,
                ]
            }
            ControlEvent::SetColorHue(hue) => {
                self.color.h = (hue as f64).map_range(0.0..255.0, 0.0..360.0);
                println!("hue: {}", self.color.h);
                vec![FixtureProperty::ColorHue]
            }
            ControlEvent::SetColorSaturation(sat) => {
                self.color.s = (sat as f64).map_range(0.0..255.0, 0.0..1.0);
                vec![FixtureProperty::ColorSaturation]
            }
            ControlEvent::SetColorValue(val) => {
                self.color.v = (val as f64).map_range(0.0..255.0, 0.0..1.0);
                vec![FixtureProperty::ColorValue]
            }
            _ => todo!(),
        }
    }
}

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

    dmx_port: Option<Box<dyn SerialPort>>,
}

impl DmxEngine {
    fn open_hw_interface() -> Option<Box<dyn SerialPort>> {
        // TODO: use USB intrinsics for detection: look at v1 branch

        // Open your Enttec device (likely /dev/ttyUSB0)
        let port_name = "/dev/ttyUSB0";
        let baud_rate = 250_000;

        match serialport::new(port_name, baud_rate)
            .data_bits(serialport::DataBits::Eight)
            .parity(serialport::Parity::None)
            .stop_bits(serialport::StopBits::Two)
            .timeout(Duration::from_millis(10))
            .open()
        {
            Ok(port) => Some(port),
            Err(err) => {
                error!("Could not establish link to DMX interface: {err}");
                None
            }
        }
    }

    pub fn new(
        state_ref: Arc<AppState>,
        event_bus_connection: SystemEventBusConnectionInst,
        system_out: Sender<SystemMessage>,
    ) -> Self {
        let dmx_port = Self::open_hw_interface();

        Self {
            state_ref,
            dmx_previous: [0; 513],
            // dmx: [0; 513],
            event_bus_connection,
            system_out,
            animation_base_times: BTreeMap::new(),
            start_time: Instant::now(),
            dmx_port,
        }
    }

    /// Returns whether the DMX buffer changed.
    fn tick_internal(&mut self, audio_snapshot: CollectedAudioSnapshot) -> bool {
        // Read back events.
        let mut events = vec![];
        while let Some(ev) = self.event_bus_connection.try_recv() {
            events.push(ev)
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
            let animations = state.animations.clone();

            for (scene_id, scene) in state.scenes.iter_mut() {
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
                                (*self.animation_base_times.get(animation_id).unwrap()) as f32
                                    * animation.speed_factor.as_float();

                            // TODO: limited by tick speed

                            // println!("{}", now - animation.last_tick_time);

                            let mut num_ticks = 1;

                            let millis = DMX_TICK_TIME.as_millis();
                            if transition_time < millis as f32 {
                                num_ticks = (millis as f32 / transition_time) as usize;
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
                        .send(SystemMessage::Log(msg.to_string()))
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
            AnimationSpecBody::Phaser(body) => body.generate(time as f32),
            AnimationSpecBody::AudioVolume(animation_spec_body_audio_volume) => {
                todo!()
            }
            AnimationSpecBody::Beat(animation_spec_body_beat) => todo!(),
            AnimationSpecBody::Wasm(animation_spec_body_wasm) => todo!(),
        }
    }

    fn build_animations_cache(&mut self, audio_snapshot: CollectedAudioSnapshot) {
        // Only build every 100ms or so?

        let animations = &self.state_ref.dmx_engine.read().unwrap().animations;

        for (anim_id, anim) in animations {
            let total_speed_raw = {
                let animation_spec = animations.get(anim_id).unwrap();
                match &animation_spec.body {
                    AnimationSpecBody::Phaser(body) => match body.time_total {
                        PhaserDuration::Fixed(time) => time,
                        PhaserDuration::Beat(beats) => {
                            audio_snapshot.time_between_beats_millis as u64 * beats as u64
                        }
                    },
                    AnimationSpecBody::AudioVolume(animation_spec_body_audio_volume) => {
                        todo!()
                    }
                    AnimationSpecBody::Beat(animation_spec_body_beat) => todo!(),
                    AnimationSpecBody::Wasm(animation_spec_body_wasm) => todo!(),
                }
            };

            let speed_per_step = total_speed_raw / 360;

            self.animation_base_times.insert(*anim_id, speed_per_step);
        }
    }

    fn write_to_hw(&mut self) {
        let mut buffer = self.state_ref.dmx_buffer.read().unwrap();

        if let Some(port) = &mut self.dmx_port {
            port.set_break().unwrap();
            spin_sleep::sleep(Duration::from_micros(100));
            port.clear_break().unwrap();
            spin_sleep::sleep(Duration::from_micros(12));

            // Write frame
            port.write_all(&buffer.dmx_buffer).unwrap();
        };

        // let this = Self {
        //     dmx: interface,
        //     base,
        // };
        //
        // Ok(this)
    }

    fn update_dmx_buffer(&mut self) {
        let state = self.state_ref.dmx_engine.read().unwrap();
        let mut buffer = self.state_ref.dmx_buffer.write().unwrap();

        // For each fixture, merge all scene states.

        for group in &state.groups {
            for fixture in &group.1.fixtures {
                // Apply base scene state.
                let mut merged_state = state
                    .curr_scene()
                    .sink
                    .fixture_states
                    .get(&(*group.0, *fixture.0))
                    .unwrap()
                    .clone();

                for overlay_id in &state.current_overlay_scenes {
                    let this_scene = state.scenes.get(overlay_id).unwrap();
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

                // TODO: we will need to use the merged fixture states here and then write them.
                fix.write(&merged_state, &mut buffer.dmx_buffer);
            }
        }

        mem::drop(state);
        mem::drop(buffer);

        self.write_to_hw();
    }

    fn get_selection<'engine>(
        &self,
        state: &'engine mut RwLockWriteGuard<'_, EngineState>,
    ) -> FixtureSelection {
        let group_ids = state.selection.group_ids.clone();
        let fixtures_in_group = state.selection.fixtures_in_group.clone();

        // let g_fixtures_mut = &mut group.1.fixtures;

        let mut fixtures_to_add = vec![];

        let groups_clone = state.groups.clone();

        for group in groups_clone.iter().filter(|(k, _)| group_ids.contains(&k)) {
            if fixtures_in_group.is_empty() {
                for (fix_id, _) in &group.1.fixtures {
                    fixtures_to_add.push((*group.0, *fix_id));
                }
            } else {
                // let group_fixture = g_fixtures.values_mut();
                // fixtures.extend(group_fixture);
                for fix in fixtures_in_group.clone() {
                    fixtures_to_add.push((*group.0, fix));
                }
            }
        }

        println!("GOT SELECTION: {:?}", fixtures_to_add);

        FixtureSelection {
            fixtures: fixtures_to_add,
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
        if requires_selection && state.selection.is_empty() {
            return (Some("No selected object(s)"), None);
        }

        // Empties the fixture state buffer on selecion events.
        if !requires_selection {
            state.control_buffer.reset();
        }

        // Match event.
        match ev.body() {
            ControlEvent::Transaction(t) => {
                for t_ev in t {
                    debug!("Apply transaction: {:?}", &ev);

                    let (err, rollback) =
                        self.apply(state, ControlEventMessage::new(ev.originator(), t_ev));

                    if err.is_some() {
                        error!("Error during transaction: {err:?}");
                        return (err, rollback);
                    }
                }

                (None, None)
            }
            ControlEvent::SelectGroup(group_id) => {
                if !state.groups.contains_key(&group_id) {
                    return (
                        Some("Illegal group"),
                        Some(ControlEvent::DeSelectGroup(group_id)),
                    );
                }

                if !state.selection.group_ids.insert(group_id) {
                    (Some("Already selected"), None)
                } else {
                    (None, None)
                }
            }
            ControlEvent::DeSelectGroup(group_id) => {
                if !state.groups.contains_key(&group_id) {
                    return (
                        Some("Illegal group"),
                        Some(ControlEvent::DeSelectGroup(group_id)),
                    );
                }

                if !state.selection.group_ids.remove(&group_id) {
                    (Some("Not selected"), None)
                } else {
                    (None, None)
                }
            }
            ControlEvent::LimitSelectionToFixtureInCurrentGroup(fixture_id) => {
                if state.selection.group_ids.len() != 1 {
                    return (
                        Some("Exactly 1 group shall be selected"),
                        Some(ControlEvent::UnLimitSelectionToFixtureInCurrentGroup(
                            fixture_id,
                        )),
                    );
                }

                if !state.selection.fixtures_in_group.insert(fixture_id) {
                    (Some("Already selected"), None)
                } else {
                    (None, None)
                }
            }
            ControlEvent::UnLimitSelectionToFixtureInCurrentGroup(fixture_id) => {
                if state.selection.group_ids.len() != 1 {
                    return (Some("Exactly 1 group shall be selected"), None);
                }

                if !state.selection.fixtures_in_group.remove(&fixture_id) {
                    (Some("Not selected"), None)
                } else {
                    (None, None)
                }
            }
            ControlEvent::RemoveSelection => {
                if state.selection.is_empty() {
                    (Some("No selection"), None)
                } else {
                    (None, None)
                }
            }
            ControlEvent::RemoveAllSelection => {
                state.selection.clear();
                (None, None)
            }
            ControlEvent::PushSelection => {
                let selection = state.selection.clone();

                if let Some(top) = state.selection_stack.front() {
                    if selection.is_empty() && top.is_empty() {
                        return (Some("Push to empty selection"), None);
                    }
                }

                state.selection_stack.push_front(selection);
                state.selection.clear();
                (None, None)
            }
            ControlEvent::PopSelection => {
                if let Some(top) = state.selection_stack.pop_front() {
                    state.selection = top;
                    (None, None)
                } else {
                    (Some("Selection stack empty"), None)
                }
            }
            ControlEvent::MiscEvent { descriptor, value } => {
                todo!("Not implemented");
            }
            ControlEvent::SetSceneFocus(id) => {
                debug_assert!(state.scenes.get(&id).is_some());
                state.current_scene_focus = id;
                (None, None)
            }
            CONTROLS_REQUIRING_SELECTION!() => {
                let curr_selection = self.get_selection(state);
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
        let current_scene_focus = state.current_scene_focus;

        let (msg, undo, effective_properties) = match ev.body() {
            ControlEvent::AddAnimation(id) => {
                let this_scene = state.scenes.get_mut(&current_scene_focus).unwrap();
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
                        let animation = state.animations.get(&id).unwrap();

                        (None, None, Some(vec![animation.property]))
                    }
                }
            }
            ControlEvent::RemoveAnimation(id) => {
                println!("REMove anim");

                let this_scene = state.scenes.get_mut(&current_scene_focus).unwrap();
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

                        match selec_anim.remove(&id) {
                            None => (Some("Animation not applied to selection"), None, None),
                            Some(_) => {
                                println!("REMOVED");
                                let animation = state.animations.get(&id).unwrap();
                                (None, None, Some(vec![animation.property]))
                            }
                        }
                    }
                }
            }
            ControlEvent::PlayAnimation(id) => {
                let this_scene = state.scenes.get_mut(&current_scene_focus).unwrap();
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
                                anim.enabled = true;
                                let animation = state.animations.get(&id).unwrap();
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
            ControlEvent::PauseAnimation(id) => {
                let this_scene = state.scenes.get_mut(&current_scene_focus).unwrap();
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
                                let animation = state.animations.get(&id).unwrap();
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
                let this_scene = state.scenes.get_mut(&current_scene_focus).unwrap();

                this_scene
                    .sink
                    .apply_with_selection(curr_selection, ev.body());

                // Update control buffer for the UI.
                state.control_buffer.apply(ev.body());

                return (None, None);
            }
        };

        // Update the changeset
        let this_scene = state.scenes.get_mut(&current_scene_focus).unwrap();

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
