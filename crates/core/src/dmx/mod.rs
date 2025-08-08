/// This module deals with applying events on fixtures to produce a continuous DMX output.
mod clock;
mod fixture;
mod state;

pub use state::*;
pub mod animation;
pub use fixture::*;

use crate::{
    audio::{self, defs::DMX_TICK_TIME},
    dmx::animation::{AnimationSpec, AnimationSpecBody, PhaserDuration},
    event::SystemEventBusConnectionInst,
    msg::SystemMessage,
    routes::AppState,
};
use blaulicht_shared::{
    AnimationSpeedModifier, CollectedAudioSnapshot, ControlEvent, ControlEventMessage,
    EventOriginator, FixtureProperty, CONTROLS_REQUIRING_SELECTION,
};
use crossbeam_channel::Sender;
use std::{
    collections::BTreeMap,
    sync::{Arc, RwLockWriteGuard},
    time::Instant,
};

pub struct FixtureSelection {
    fixtures: Vec<(u8, u8)>,
}

impl Fixture {
    fn apply(&mut self, ev: ControlEvent) {
        // match ev {
        //     ControlEvent::SetEnabled(enabled) => {
        //         todo!("not supported");
        //     }
        //     ControlEvent::SetBrightness(brightness) => {
        //         self.set_alpha(brightness);
        //     }
        //     ControlEvent::SetColor(clr) => {
        //         self.set_color(clr);
        //     }
        //     ControlEvent::MiscEvent { descriptor, value } => todo!(),
        //     _ => {}
        // }
        self.state.apply(ev);
    }
}

// TODO: maybe fuse this together?

impl FixtureState {
    fn apply(&mut self, ev: ControlEvent) {
        match ev {
            ControlEvent::SetBrightness(alpha) => {
                self.alpha = alpha;
            }
            ControlEvent::SetColor(clr) => {
                self.color = clr.into();
            }
            ControlEvent::AddAnimation(id) => {
                self.animations.insert(
                    id,
                    AppliedAnimation {
                        id,
                        speed_factor: AnimationSpeedModifier::_1,
                        enabled: false,
                        timer: 0,
                        last_tick_time: 0,
                    },
                );
            }
            ControlEvent::RemoveAnimation(id) => {
                self.animations.remove(&id);
            }
            ControlEvent::SetAnimationSpeed(animation_id, speed) => {
                // TODO: this can go run
                let Some(mut anim) = self.animations.get_mut(&animation_id) else {
                    // TODO: shall not happen
                    todo!();
                    return;
                };
                anim.speed_factor = speed;
            }
            ControlEvent::PauseAnimation(animation_id) => {
                let Some(mut anim) = self.animations.get_mut(&animation_id) else {
                    // TODO: shall not happen
                    todo!();
                    return;
                };
                anim.enabled = false;
            }
            ControlEvent::PlayAnimation(animation_id) => {
                let Some(mut anim) = self.animations.get_mut(&animation_id) else {
                    // TODO: shall not happen
                    todo!();
                    return;
                };
                anim.enabled = true;
            }
            _ => todo!(),
        }
    }
}

impl FixtureSelection {
    fn apply(
        &mut self,
        state: &mut RwLockWriteGuard<'_, EngineState>,
        ev: ControlEvent,
    ) -> Option<&'static str> {
        for (group_id, fix_id) in &self.fixtures {
            println!("apply: (ev = {ev:?}) on (gid={group_id} fid={fix_id})");
            let fixture = state
                .groups
                .get_mut(group_id)
                .unwrap()
                .fixtures
                .get_mut(fix_id)
                .unwrap();

            fixture.apply(ev.clone());
        }

        state.control_buffer.apply(ev);

        None
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
}

impl DmxEngine {
    pub fn new(
        state_ref: Arc<AppState>,
        event_bus_connection: SystemEventBusConnectionInst,
        system_out: Sender<SystemMessage>,
    ) -> Self {
        Self {
            state_ref,
            dmx_previous: [0; 513],
            // dmx: [0; 513],
            event_bus_connection,
            system_out,
            animation_base_times: BTreeMap::new(),
            start_time: Instant::now(),
        }
    }

    /// Returns whether the DMX buffer changed.
    fn tick_internal(&mut self, audio_snapshot: CollectedAudioSnapshot) -> bool {
        // Read back events.
        let mut events = vec![];
        loop {
            match self.event_bus_connection.try_recv() {
                Some(ev) => events.push(ev),
                None => break,
            }
        }

        self.build_animations_cache(audio_snapshot);

        // Advance animations.
        {
            let now = (Instant::now().duration_since(self.start_time)).as_millis() as u64;
            let mut state = self.state_ref.dmx_engine.write().unwrap();
            let animations = state.animations.clone();
            let fixtures = state
                .groups
                .iter_mut()
                .flat_map(|(_, g)| g.fixtures.values_mut());

            for fixture in fixtures {
                for (animation_id, animation) in &mut fixture.state.animations {
                    if !animation.enabled {
                        continue;
                    }

                    let transition_time = (*self.animation_base_times.get(animation_id).unwrap())
                        as f32
                        * animation.speed_factor.as_float();

                    // TODO: limited by tick speed

                    println!("animation-speed: {transition_time}");
                    println!("{}", now - animation.last_tick_time);

                    let mut num_ticks = 1;

                    let millis = DMX_TICK_TIME.as_millis();
                    if transition_time < millis as f32 {
                        num_ticks = (millis as f32 / transition_time) as usize;
                        println!("NUM TICKS: {num_ticks}");
                    }

                    if transition_time == 0.0 {
                        continue;
                    }

                    if now - animation.last_tick_time >= transition_time as u64 {
                        for _ in 0..num_ticks {
                            animation.tick(now);
                        }

                        let spec = animations.get(animation_id).unwrap();

                        let v = self.generate_animation_value(
                            audio_snapshot,
                            spec,
                            *animation_id,
                            animation.timer,
                        );

                        match spec.property {
                            FixtureProperty::Brightness => fixture.state.alpha = v,
                            FixtureProperty::ColorHue => todo!(),
                            FixtureProperty::ColorSaturation => todo!(),
                            FixtureProperty::ColorValue => todo!(),
                            FixtureProperty::Tilt => fixture.state.orientation.tilt = v,
                            FixtureProperty::Pan => fixture.state.orientation.pan = v,
                            FixtureProperty::Rotation => fixture.state.orientation.rotation = v,
                        }

                        println!("update animation");
                    };
                }
            }
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
    ) -> u8 {
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

    fn update_dmx_buffer(&mut self) {
        let state = self.state_ref.dmx_engine.read().unwrap();
        let mut buffer = self.state_ref.dmx_buffer.write().unwrap();

        for group in &state.groups {
            for fixture in &group.1.fixtures {
                let fix = fixture.1;
                fix.write(&mut buffer.dmx_buffer);
            }
        }
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
            state.control_buffer = FixtureState::default();
        }

        let mut selection = self.get_selection(state);

        // Match event.
        match ev.body() {
            ControlEvent::Transaction(t) => {
                todo!("Transaction: {t:?}");
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
            CONTROLS_REQUIRING_SELECTION!() => (selection.apply(state, ev.body()), None),
        }
    }
}
