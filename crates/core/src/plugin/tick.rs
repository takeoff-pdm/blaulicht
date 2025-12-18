use crate::state::AppState;
use crate::{
    msg::{MidiEvent, SystemMessage},
    plugin::{Plugin, PluginManager},
    system_message,
};
use blaulicht_shared::EngineState;
#[cfg(not(feature = "wasmtime"))]
use blaulicht_shared::SerialReceived;
#[cfg(feature = "wasmtime")]
use blaulicht_shared::SerialReceived;
use blaulicht_shared::{CollectedAudioSnapshot, ControlEventCollection, LogLevel, TickInput};
use log::warn;
use std::sync::{Arc, RwLock, RwLockWriteGuard};
use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

// #[cfg(feature = "wasmtime")]
// use blaulicht_shared::EngineState;

// #[cfg(feature = "wasmtime")]
// use crate::{
//     msg::{MidiEvent, SystemMessage},
//     plugin::{Plugin, PluginManager},
//     system_message,
// };

///
/// Mock implementation
///
#[cfg(not(feature = "wasmtime"))]
impl PluginManager {
    pub fn tick(
        &mut self,
        _: CollectedAudioSnapshot,
        _: &[MidiEvent],
        _: Vec<SerialReceived>,
        _: Option<Arc<AppState>>,
    ) -> anyhow::Result<Duration> {
        Ok(Duration::from_millis(42))
    }

    pub fn disable_errored_plugins(
        &mut self,
        _: HashMap<u8, anyhow::Error>,
    ) -> Option<anyhow::Error> {
        None
    }
}

#[cfg(feature = "wasmtime")]
impl PluginManager {
    pub fn tick(
        &mut self,
        audio_data: CollectedAudioSnapshot,
        midi_events: &[MidiEvent],
        serial_received: Vec<SerialReceived>,
        // If present and enough (WAIT_BETWEEN_ENGINE_SERIALIZE_UPDATES) time has passed, write engine state into plugin.
        app_state: Option<Arc<AppState>>,
    ) -> anyhow::Result<Duration> {
        let start = Instant::now();

        //
        // Collect recent events.
        //
        let mut events = vec![];
        loop {
            let Some(event) = self.event_bus.try_recv() else {
                break;
            };

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
                    audio_data,
                    events: ControlEventCollection {
                        events: events.clone(),
                    },
                };

                let plugin = self.plugins.get_mut(&plugin_key).unwrap();
                // TODO: handle errors for each plugin separately.
                // TODO: this clone might hurt?
                if let Err(err) = plugin.tick(
                    input,
                    midi_events,
                    serial_received.clone(),
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
        let mut ret = None;

        let ret_val = {
            let mut plugins = self.state_ref.plugins.write().unwrap();

            for (plugin_key, err) in plugins_err.into_iter() {
                if ret.is_none() {
                    ret = Some(err);
                }

                self.system_out
                    .send(SystemMessage::Log(
                        format!("Disabling plugin with error(s): (id={})...", plugin_key),
                        LogLevel::Warn,
                    ))
                    .unwrap();

                plugins.get_mut(&plugin_key).unwrap().set_errored(true);
            }

            ret
        };

        ret_val
    }
}

//
// Real Wasmtime implementation.
//

const WAIT_BETWEEN_ENGINE_SERIALIZE_UPDATES: Duration = Duration::from_millis(50);

#[cfg(feature = "wasmtime")]
impl Plugin {
    fn tick(
        &mut self,
        input: TickInput,
        midi_events: &[MidiEvent],
        serial_received: Vec<SerialReceived>,
        app_state: Option<Arc<AppState>>,
    ) -> anyhow::Result<()> {
        //
        // Tick function.
        //

        // use blaulicht_shared::EngineState;
        let func = self.wasm_state.instance.get_typed_func::<(i32, i32), ()>(
            &mut self.wasm_state.store,
            "internal_tick", // TODO: external type and name constants.
        )?;

        //
        // Tick input array.
        //
        let tick_array_offset = 0x10000; // Arbitrary offset
        let tick_array_data = input.serialize();
        let tick_array_len = tick_array_data.len() as i32;
        let mut tick_array_bytes = Vec::new();
        for &num in &tick_array_data {
            tick_array_bytes.extend_from_slice(&num.to_le_bytes());
        }
        self.wasm_state.memory.write(
            &mut self.wasm_state.store,
            tick_array_offset,
            &tick_array_bytes,
        )?;

        // TODO: macro for this array stuff.

        //
        // MIDI array.
        //
        // let midi_array_offset = 0x80000; // TODO: make this offset a const.
        let midi_array_len = midi_events.len() as u32;

        if midi_array_len > 100 {
            panic!("TOO many MIDI events!");
        }

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

        ////////////// MIDI ////////////

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

        ////////////// STATE ////////////

        if let Some(app) = app_state {
            if self.last_dmx_engine_sync.elapsed().as_millis()
                > WAIT_BETWEEN_ENGINE_SERIALIZE_UPDATES.as_millis()
            {
                use log::debug;

                self.last_dmx_engine_sync = Instant::now();
                let state_array_bytes = {
                    let engine = app.dmx_engine.read().unwrap();
                    engine.0.serialize()
                };

                let state_array_len = state_array_bytes.len() as u32;

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

                // debug!("SYNCED ENGINE STATE");
            }
        }

        ////////////////// Serial /////////////////////

        {
            let serial_bytes = {
                use blaulicht_shared::SerialCollector;

                let collector = SerialCollector::new(serial_received);
                collector.serialize()
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

            // debug!("SYNCED ENGINE STATE");
        }

        /// END
        // for &num in &self.dmx {
        //     dmx_array_bytes.extend_from_slice(&num.to_le_bytes());
        // }
        // wasm.memory
        //     .write(&mut wasm.store, dmx_array_offset, &dmx_array_bytes)?;

        //
        // Data array.
        //
        // let data_array_offset = 0x90000; // Arbitrary offset

        // let mut data_array_bytes = Vec::new();
        // for &num in &self.data {
        //     data_array_bytes.extend_from_slice(&num.to_le_bytes());
        // }
        // wasm.memory
        //     .write(&mut wasm.store, data_array_offset, &data_array_bytes)?;

        // let dmx_array_offset = 0x20000; // TODO: make this offset a const.

        // Call the function with the pointer and length
        func.call(
            &mut self.wasm_state.store,
            (
                tick_array_offset as i32,
                tick_array_len,
                // midi_array_offset as i32,
                // midi_array_len,
            ),
        )?;

        //
        // Read back the modified DMX array
        //
        // let mut dmx_array_bytes: Vec<u8> = Vec::with_capacity(dmx_array_len as usize);
        // let mut updated_dmx_bytes = vec![0u8; DMX_LEN];
        // wasm.memory
        //     .read(&mut wasm.store, dmx_array_offset, &mut updated_dmx_bytes)?;
        // self.dmx = updated_dmx_bytes;

        //
        // Read back data array.
        //
        // let mut updated_data_bytes = vec![0u8; data_array_bytes.len()];
        // wasm.memory
        //     .read(&mut wasm.store, data_array_offset, &mut updated_data_bytes)?;
        // let updated_data_bytes: Vec<i32> = updated_data_bytes
        //     .chunks_exact(4)
        //     .map(|chunk| i32::from_le_bytes(chunk.try_into().unwrap()))
        //     .collect();
        // self.data = updated_data_bytes;

        Ok(())
    }
}
