use std::{
    sync::Arc,
    thread,
    time::{Duration, Instant},
};

use anyhow::Context;
use blaulicht_shared::{CollectedAudioSnapshot, TickInput};
use crossbeam_channel::{RecvError, TryRecvError};
use log::debug;

use crate::{
    app::MidiEvent, plugin::{Plugin, PluginManager},
};

impl PluginManager {
    pub fn tick(
        &mut self,
        audio_data: CollectedAudioSnapshot,
        midi_events: &[MidiEvent],
    ) -> anyhow::Result<Duration> {
        let start = Instant::now();
        // Generate tick input.
        let input = TickInput {
            clock: self.timer_start.elapsed().as_millis() as u32, // TODO: what if we overflow?
            initial: self.is_initial_tick,
            audio_data,
        };

        let mut err_res = None;

        for (_, plugin) in self.plugins.iter_mut() {
            // TODO: handle errors for each plugin separately.
            if let Err(err) = plugin.tick(input, midi_events) {
                err_res = Some(anyhow::anyhow!(
                    "Failed to tick plugin '{}': {}",
                    plugin.path,
                    err
                ));
            }
        }

        if self.is_initial_tick {
            self.is_initial_tick = false;
        }

        match err_res {
            Some(e) => Err(e),
            None => Ok(start.elapsed()),
        }
    }
}

impl Plugin {
    fn tick(&mut self, input: TickInput, midi_events: &[MidiEvent]) -> anyhow::Result<()> {
        //
        // Tick function.
        //
        let func = self.instance.get_typed_func::<(i32, i32, i32), ()>(
            &mut self.store,
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
        self.memory
            .write(&mut self.store, tick_array_offset, &tick_array_bytes)?;

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

        // Write the MIDI array to memory.
        self.memory.write(
            &mut self.store,
            self.midi_status.buffer_addr(),
            &midi_array_bytes,
        )?;

        // Write the length of the MIDI array to memory.
        let mut midi_length_bytes = Vec::new();
        midi_length_bytes.extend_from_slice(&midi_array_len.to_le_bytes());
        self.memory.write(
            &mut self.store,
            self.midi_status.buffer_len_addr(),
            &midi_length_bytes,
        )?;

        // for &num in &self.dmx {
        //     dmx_array_bytes.extend_from_slice(&num.to_le_bytes());
        // }
        // wasm.memory
        //     .write(&mut wasm.store, dmx_array_offset, &dmx_array_bytes)?;

        //
        // Data array.
        //
        let data_array_offset = 0x90000; // Arbitrary offset

        // let mut data_array_bytes = Vec::new();
        // for &num in &self.data {
        //     data_array_bytes.extend_from_slice(&num.to_le_bytes());
        // }
        // wasm.memory
        //     .write(&mut wasm.store, data_array_offset, &data_array_bytes)?;

        // let dmx_array_offset = 0x20000; // TODO: make this offset a const.

        // Call the function with the pointer and length
        func.call(
            &mut self.store,
            (
                tick_array_offset as i32,
                tick_array_len,
                data_array_offset,
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
