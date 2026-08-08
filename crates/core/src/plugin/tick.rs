use crate::state::AppState;
use crate::{
    msg::{MidiEvent, SystemMessage},
    plugin::{Plugin, PluginManager},
};
#[cfg(not(feature = "wasmtime"))]
use blaulicht_shared::SerialReceived;
#[cfg(feature = "wasmtime")]
use blaulicht_shared::SerialReceived;
use blaulicht_shared::{
    CollectedAudioSnapshot, ControlEventCollection, LogLevel, TickInput, UdpReceived,
};
use std::sync::Arc;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

const MAX_MIDI_EVENTS: usize = 100;
const STATE_BUFFER_CAPACITY: usize = 1024 * 100;
const SERIAL_BUFFER_CAPACITY: usize = 1000 * 1024;
const UDP_BUFFER_CAPACITY: usize = 256 * 1024;

///
/// Mock implementation
///
#[cfg(not(feature = "wasmtime"))]
impl PluginManager {
    pub fn tick_external(
        &mut self,
        _: CollectedAudioSnapshot,
        _: &[MidiEvent],
        _: Vec<SerialReceived>,
        _: Vec<UdpReceived>,
        _: Arc<AppState>,
    ) -> Duration {
        Duration::from_secs(0)
    }

    pub fn tick(
        &mut self,
        _: CollectedAudioSnapshot,
        _: &[MidiEvent],
        _: Vec<SerialReceived>,
        _: Vec<UdpReceived>,
        _: Option<Arc<AppState>>,
    ) -> anyhow::Result<Duration> {
        Ok(Duration::from_micros(1))
    }

    pub fn disable_errored_plugins(
        &mut self,
        _: HashMap<u8, anyhow::Error>,
    ) -> Option<anyhow::Error> {
        None
    }
}

///
/// Real implementation
///
#[cfg(feature = "wasmtime")]
impl PluginManager {
    pub fn tick_external(
        &mut self,
        audio_data: CollectedAudioSnapshot,
        midi_events: &[MidiEvent],
        serial_received: Vec<SerialReceived>,
        udp_received: Vec<UdpReceived>,
        app_state: Arc<AppState>,
    ) -> Duration {
        let tick_duration = match self.tick(
            audio_data,
            midi_events,
            serial_received,
            udp_received,
            Some(Arc::clone(&app_state)),
        ) {
            Ok(dur) => {
                // Reset crash state on successful tick.
                self.has_crashed = false;
                dur
            }
            Err(err) => {
                if !self.has_crashed {
                    tracing::error!("[Plugin] Wasm engine crash: {err}");
                    self.has_crashed = true;
                }
                Duration::from_micros(0)
            }
        };

        tick_duration
    }

    pub fn tick(
        &mut self,
        audio_data: CollectedAudioSnapshot,
        midi_events: &[MidiEvent],
        serial_received: Vec<SerialReceived>,
        udp_received: Vec<UdpReceived>,
        // If present and enough (WAIT_BETWEEN_ENGINE_SERIALIZE_UPDATES) time has passed, write engine state into plugin.
        app_state: Option<Arc<AppState>>,
    ) -> anyhow::Result<Duration> {
        let start = Instant::now();

        //
        // Collect recent events.
        //
        let mut events = vec![];
        while let Some(event) = self.event_bus.try_recv() {
            events.push(event);
        }

        let mut err_res: HashMap<u8, _> = HashMap::new();
        {
            // TODO: skip all disabled plugins.
            // TODO: the active plugins function is probably extremely slow.
            let active_plugins = self.active_plugins();
            for plugin_key in active_plugins {
                // Generate tick input.
                let input = TickInput {
                    id: plugin_key,
                    clock: self.timer_start.elapsed().as_millis() as u32, // TODO: what if we overflow?
                    initial: self.is_initial_tick,
                    audio_data: audio_data.clone(),
                    events: ControlEventCollection {
                        events: events.clone(),
                    },
                };

                let Some(plugin) = self.plugins.get_mut(&plugin_key) else {
                    tracing::warn!("Skipping plugin {plugin_key}: it disappeared during tick");
                    continue;
                };
                // TODO: handle errors for each plugin separately.
                // TODO: this clone might hurt?
                if let Err(err) = plugin.tick(
                    input,
                    midi_events,
                    serial_received.clone(),
                    udp_received.clone(),
                    app_state.clone(),
                ) {
                    let path = plugin_key;
                    err_res.insert(
                        path,
                        anyhow::anyhow!("Failed to tick plugin '{}': {}", plugin_key, err),
                    );
                }
            }
        }

        if self.is_initial_tick {
            self.is_initial_tick = false;
        }

        // Process any errors.
        match self.disable_errored_plugins(err_res) {
            Some(err) => Err(err),
            None => Ok(start.elapsed()),
        }
    }

    pub fn disable_errored_plugins(
        &mut self,
        plugins_err: HashMap<u8, anyhow::Error>,
    ) -> Option<anyhow::Error> {
        if plugins_err.is_empty() {
            return None;
        }

        // Drop any Art-Net receivers owned by failing plugins so the next reload
        // starts from a clean slate.
        {
            let mut artnet_output = self.state_ref.artnet_output.write().unwrap();
            for plugin_key in plugins_err.keys() {
                artnet_output.remove_receivers_for_plugin(*plugin_key);
            }
        }

        {
            let mut spawned_commands = self.state_ref.spawned_commands.lock().unwrap();
            for plugin_key in plugins_err.keys() {
                spawned_commands.remove_all_for_plugin(*plugin_key);
            }
        }

        let mut ret = None;

        let ret_val = {
            let mut plugins = self.state_ref.plugins.write().unwrap();

            for (plugin_key, err) in plugins_err.into_iter() {
                tracing::warn!("Disabling plugin with error(s): (id={plugin_key}): {err:#}");

                if ret.is_none() {
                    ret = Some(err);
                }

                if let Some(plugin) = plugins.get_mut(&plugin_key) {
                    plugin.set_errored(true);
                } else {
                    tracing::warn!("Cannot disable missing plugin {plugin_key}");
                }
            }

            ret
        };

        ret_val
    }
}

//
// Real Wasmtime implementation.
//

// TODO: tune this.
const WAIT_BETWEEN_ENGINE_SERIALIZE_UPDATES: Duration = Duration::from_millis(0);

#[cfg(feature = "wasmtime")]
impl Plugin {
    fn tick(
        &mut self,
        input: TickInput,
        mut midi_events: &[MidiEvent],
        serial_received: Vec<SerialReceived>,
        udp_received: Vec<UdpReceived>,
        app_state: Option<Arc<AppState>>,
    ) -> anyhow::Result<()> {
        //
        // Tick function. (TODO: how slow is this?) -> replace with fixed handle?
        //
        let func = self.wasm_state.instance.get_typed_func::<(i32, i32), ()>(
            &mut self.wasm_state.store,
            "internal_tick", // TODO: external type and name constants.
        )?;

        ////////////// Tick Input ////////////
        let tick_array_offset = 0x10000; // Arbitrary offset
        let tick_array_data = input.serialize();
        let tick_array_len = tick_array_data.len() as i32;

        {
            let mut tick_array_bytes = Vec::new();
            for &num in &tick_array_data {
                tick_array_bytes.extend_from_slice(&num.to_le_bytes());
            }
            self.wasm_state.memory.write(
                &mut self.wasm_state.store,
                tick_array_offset,
                &tick_array_bytes,
            )?;
        }

        ////////////// MIDI ////////////
        {
            let midi_array_len = midi_events.len() as u32;

            if midi_array_len as usize > MAX_MIDI_EVENTS {
                midi_events = &midi_events[0..MAX_MIDI_EVENTS];
                tracing::warn!("TOO many MIDI events! TRUNCATING");
            }

            let midi_array_len = midi_events.len() as u32;

            let mut midi_array_bytes = Vec::new();

            let midi_events_packed: Vec<u32> = midi_events
                .iter()
                .map(|event| {
                    ((event.device as u32) << 24)
                        | (event.status as u32) << 16
                        | (event.data0 as u32) << 8
                        | (event.data1 as u32)
                })
                .collect::<Vec<u32>>();

            for &num in &midi_events_packed {
                midi_array_bytes.extend_from_slice(&num.to_le_bytes());
            }

            // Write the MIDI array to memory.
            self.wasm_state.memory.write(
                &mut self.wasm_state.store,
                self.midi_buffers.buffer_addr(),
                &midi_array_bytes,
            )?;

            // Write the length of the MIDI array to memory.
            let mut midi_length_bytes = Vec::new();
            midi_length_bytes.extend_from_slice(&midi_array_len.to_le_bytes());
            self.wasm_state.memory.write(
                &mut self.wasm_state.store,
                self.midi_buffers.buffer_len_addr(),
                &midi_length_bytes,
            )?;
        }

        ////////////// APP STATE ////////////
        if let Some(app) = app_state {
            // TODO: profile this if this causes issues.
            if self.last_dmx_engine_sync.elapsed().as_millis()
                > WAIT_BETWEEN_ENGINE_SERIALIZE_UPDATES.as_millis()
            {
                self.last_dmx_engine_sync = Instant::now();
                let state_array_bytes = {
                    let engine = app.dmx_engine.read().unwrap();
                    engine.0.serialize()
                };

                let state_array_len = state_array_bytes.len() as u32;

                anyhow::ensure!(
                    state_array_bytes.len() <= STATE_BUFFER_CAPACITY,
                    "serialized engine state ({}) exceeds WASM buffer capacity ({STATE_BUFFER_CAPACITY})",
                    state_array_bytes.len()
                );

                // Write the state array to memory.
                self.wasm_state.memory.write(
                    &mut self.wasm_state.store,
                    self.state_buffers.buffer_addr(),
                    &state_array_bytes,
                )?;

                // Write the length of the state array to memory.
                let mut state_length_bytes = Vec::new();
                state_length_bytes.extend_from_slice(&state_array_len.to_le_bytes());
                self.wasm_state.memory.write(
                    &mut self.wasm_state.store,
                    self.state_buffers.buffer_len_addr(),
                    &state_length_bytes,
                )?;
            }
        }

        ////////////////// Serial /////////////////////
        {
            let mut serial_received = serial_received;
            let serial_bytes = {
                use blaulicht_shared::SerialCollector;

                loop {
                    let bytes = SerialCollector::new(serial_received.clone()).serialize();
                    if bytes.len() <= SERIAL_BUFFER_CAPACITY || serial_received.is_empty() {
                        break bytes;
                    }
                    serial_received.pop();
                }
            };

            let serial_array_len = serial_bytes.len() as u32;

            // Write the state array to memory.
            self.wasm_state.memory.write(
                &mut self.wasm_state.store,
                self.serial_buffers.buffer_addr(),
                &serial_bytes,
            )?;

            // Write the length of the state array to memory.
            let mut serial_length_bytes = Vec::new();
            serial_length_bytes.extend_from_slice(&serial_array_len.to_le_bytes());
            self.wasm_state.memory.write(
                &mut self.wasm_state.store,
                self.serial_buffers.buffer_len_addr(),
                &serial_length_bytes,
            )?;
        }

        ////////////////// UDP /////////////////////
        {
            let mut udp_received = udp_received;
            let udp_bytes = {
                use blaulicht_shared::UdpCollector;

                loop {
                    let bytes = UdpCollector::new(udp_received.clone()).serialize();
                    if bytes.len() <= UDP_BUFFER_CAPACITY || udp_received.is_empty() {
                        break bytes;
                    }
                    udp_received.pop();
                }
            };

            let udp_array_len = udp_bytes.len() as u32;

            self.wasm_state.memory.write(
                &mut self.wasm_state.store,
                self.udp_buffers.buffer_addr(),
                &udp_bytes,
            )?;

            let mut udp_length_bytes = Vec::new();
            udp_length_bytes.extend_from_slice(&udp_array_len.to_le_bytes());
            self.wasm_state.memory.write(
                &mut self.wasm_state.store,
                self.udp_buffers.buffer_len_addr(),
                &udp_length_bytes,
            )?;
        }

        // Call the function with the pointer and length
        catch_unwind(AssertUnwindSafe(|| {
            func.call(
                &mut self.wasm_state.store,
                (tick_array_offset as i32, tick_array_len),
            )
        }))
        .map_err(|_| anyhow::anyhow!("WASM plugin panicked during tick"))??;

        Ok(())
    }
}
