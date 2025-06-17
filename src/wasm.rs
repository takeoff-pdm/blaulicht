use crossbeam_channel::Sender;
use std::{net::UdpSocket, time::Instant};

use wasmtime::*;

use crate::{
    app::MidiEvent,
    audio::{SystemMessage, WasmControlsConfig, WasmControlsLog, WasmControlsSet},
};

#[derive(Clone, Copy)]
pub struct TickInput {
    pub volume: u8,
    pub beat_volume: u8,
    pub bass: u8,
    pub bass_avg_short: u8,
    pub bass_avg: u8,
    pub bpm: u8,
    pub time_between_beats_millis: u16,
}

impl TickInput {
    fn serialize(&self, timer_start: Instant, initial: bool) -> [i32; 9] {
        [
            Instant::now().duration_since(timer_start).as_millis() as i32,
            self.volume.into(),
            self.beat_volume.into(),
            self.bass.into(),
            self.bass_avg_short.into(),
            self.bass_avg.into(),
            self.bpm.into(),
            self.time_between_beats_millis.into(),
            initial as i32,
        ]
    }
}

impl Default for TickInput {
    fn default() -> Self {
        Self {
            volume: 0,
            beat_volume: 0,
            bass: 0,
            bass_avg_short: 0,
            bass_avg: 0,
            bpm: 0,
            time_between_beats_millis: 0,
        }
    }
}

pub struct TickEngine {
    timer_start: Instant,
    data: Vec<i32>,
    dmx: Vec<u8>,
    wasm: Option<WasmEngine>,
    midi_out: Sender<MidiEvent>,
    system_out: Sender<SystemMessage>,
}

pub struct WasmEngine {
    store: Store<()>,
    instance: Instance,
    memory: Memory,
}

const DMX_LEN: usize = 513;

impl TickEngine {
    pub fn create(midi_out: Sender<MidiEvent>, system_out: Sender<SystemMessage>) -> Result<Self> {
        let mut engine = TickEngine {
            timer_start: Instant::now(),
            data: vec![0; 1000],
            dmx: vec![0; DMX_LEN],
            wasm: None,
            midi_out,
            system_out,
        };

        engine.init_wasm()?;
        engine.first_tick()?;

        Ok(engine)
    }

    pub fn dmx(&self) -> &[u8] {
        &self.dmx
    }

    fn init_wasm(&mut self) -> Result<()> {
        let mut config = Config::new();
        config.strategy(wasmtime::Strategy::Cranelift);
        config.cranelift_opt_level(wasmtime::OptLevel::Speed);

        let engine = Engine::new(&config)?;
        let module = Module::from_file(&engine, "./wasm/output.wasm")?;

        let mut store = Store::new(&engine, ());

        let mut linker = Linker::new(&engine);

        // UDP support.
        let socket = UdpSocket::bind("0.0.0.0:0")?;

        let so = self.system_out.clone();
        linker.func_wrap(
            "blaulicht",
            "udp",
            move |mut caller: Caller<'_, ()>,
                  target_addr_pointer: i32,
                  target_addr_len: i32,
                  byte_arr_pointer: i32,
                  byte_arr_len: i32| {
                let memory = caller
                    .get_export("memory")
                    .and_then(|export| export.into_memory())
                    .expect("Failed to find memory");

                let mut body_buffer = vec![0u8; byte_arr_len as usize];
                memory
                    .read(&caller, byte_arr_pointer as usize, &mut body_buffer)
                    .expect("Failed to read memory");

                let mut addr_buffer = vec![0u8; target_addr_len as usize];
                memory
                    .read(&caller, target_addr_pointer as usize, &mut addr_buffer)
                    .expect("Failed to read memory");

                let target_addr = String::from_utf8_lossy(&addr_buffer).to_string();

                socket
                    .send_to(&body_buffer, target_addr.clone())
                    .unwrap_or_else(|e| {
                        so.send(SystemMessage::Log(format!(
                            "UDP error: SEND to {target_addr}: {e}"
                        )))
                        .expect("Failed to send log message");
                        0
                    });
            },
        )?;

        let so = self.system_out.clone();
        linker.func_wrap(
            "blaulicht",
            "log",
            move |mut caller: Caller<'_, ()>, str_pointer: i32, str_len: i32| {
                let memory = caller
                    .get_export("memory")
                    .and_then(|export| export.into_memory())
                    .expect("Failed to find memory");

                let mut buffer = vec![0u8; str_len as usize];
                memory
                    .read(&caller, str_pointer as usize, &mut buffer)
                    .expect("Failed to read memory");

                let received_string = String::from_utf8_lossy(&buffer).to_string();

                log::debug!("[WASM] {received_string}");

                so.send(SystemMessage::WasmLog(received_string))
                    .expect("Failed to send log message");
            },
        )?;

        let so = self.system_out.clone();
        linker.func_wrap(
            "blaulicht",
            "controls_log",
            move |mut caller: Caller<'_, ()>, x: i32, y: i32, str_pointer: i32, str_len: i32| {
                let memory = caller
                    .get_export("memory")
                    .and_then(|export| export.into_memory())
                    .expect("Failed to find memory");

                let mut buffer = vec![0u8; str_len as usize];
                memory
                    .read(&caller, str_pointer as usize, &mut buffer)
                    .expect("Failed to read memory");

                let received_string = String::from_utf8_lossy(&buffer).to_string();

                so.send(SystemMessage::WasmControlsLog(WasmControlsLog {
                    x: x as u8,
                    y: y as u8,
                    value: received_string,
                }))
                .expect("Failed to send controls log message");
            },
        )?;

        let so = self.system_out.clone();
        linker.func_wrap(
            "blaulicht",
            "controls_set",
            move |mut _caller: Caller<'_, ()>, x: i32, y: i32, value: i32| {
                so.send(SystemMessage::WasmControlsSet(WasmControlsSet {
                    x: x as u8,
                    y: y as u8,
                    value: value != 0,
                }))
                .expect("Failed to send controls set message");
            },
        )?;

        let so = self.system_out.clone();
        linker.func_wrap(
            "blaulicht",
            "controls_config",
            move |mut _caller: Caller<'_, ()>, x: i32, y: i32| {
                so.send(SystemMessage::WasmControlsConfig(WasmControlsConfig {
                    x: x as u8,
                    y: y as u8,
                }))
                .expect("Failed to send controls config message");
                log::debug!("[WASM] controls_config: {x} {y}");
            },
        )?;

        let mo = self.midi_out.clone();
        linker.func_wrap(
            "blaulicht",
            "bl_midi",
            move |device: i32, status: i32, kind: i32, value: i32| {
                mo.send(MidiEvent {
                    device: device as u8,
                    status: status as u8,
                    data0: kind as u8,
                    data1: value as u8,
                })
                .unwrap();
            },
        )?;

        let instance = linker.instantiate(&mut store, &module)?;

        let memory = instance
            .get_memory(&mut store, "memory")
            .expect("Memory not found");

        // Initialize DMX.
        let dmx_array_offset = 0x20000; // TODO: make this offset a const.
        let mut dmx_array_bytes: Vec<u8> = vec![0; DMX_LEN];
        for &num in &self.dmx {
            dmx_array_bytes.extend_from_slice(&num.to_le_bytes());
        }
        memory.write(&mut store, dmx_array_offset, &dmx_array_bytes)?;

        self.wasm = Some(WasmEngine {
            store,
            instance,
            memory,
        });

        log::info!("[WASM] initialized.");

        Ok(())
    }

    pub fn reload(&mut self) -> Result<()> {
        // Reset the data.
        self.data.fill(0);
        // Reset the clock.
        self.timer_start = Instant::now();
        self.init_wasm()?;
        self.first_tick()
    }

    pub fn first_tick(&mut self) -> Result<()> {
        self.tick(
            TickInput {
                volume: 0,
                beat_volume: 0,
                bass: 0,
                bass_avg_short: 0,
                bass_avg: 0,
                bpm: 0,
                time_between_beats_millis: 0,
            },
            &[],
            true,
        )
    }

    pub fn tick(
        &mut self,
        input: TickInput,
        midi_events: &[MidiEvent],
        initial: bool,
    ) -> Result<()> {
        let wasm = self.wasm.as_mut().unwrap();

        //
        // Tick function.
        //
        let func = wasm
            .instance
            .get_typed_func::<(i32, i32, i32, i32, i32, i32, i32, i32), ()>(
                &mut wasm.store,
                "internal_tick", // TODO: external type and name constants.
            )?;

        //
        // Tick input array.
        //
        let tick_array_offset = 0x10000; // Arbitrary offset
        let tick_array_data = input.serialize(self.timer_start, initial);
        let tick_array_len = tick_array_data.len() as i32;
        let mut tick_array_bytes = Vec::new();
        for &num in &tick_array_data {
            tick_array_bytes.extend_from_slice(&num.to_le_bytes());
        }
        wasm.memory
            .write(&mut wasm.store, tick_array_offset, &tick_array_bytes)?;

        // TODO: macro for this array stuff.

        //
        // MIDI array.
        //
        let midi_array_offset = 0x80000; // TODO: make this offset a const.
        let midi_array_len = midi_events.len() as i32;

        if midi_array_len > 100 {
            panic!("TOO many MIDI events!");
        }

        let mut midi_array_bytes = Vec::new();

        let midi_events_packed: Vec<u32> = midi_events
            .iter()
            .map(|event| {
                0u32
                    | (event.device as u32) << 24
                    | (event.status as u32) << 16
                    | (event.data0 as u32) << 8
                    | (event.data1 as u32)
            })
            .collect::<Vec<u32>>();

        for &num in &midi_events_packed {
            midi_array_bytes.extend_from_slice(&num.to_le_bytes());
        }
        wasm.memory
            .write(&mut wasm.store, midi_array_offset, &midi_array_bytes)?;

        // for &num in &self.dmx {
        //     dmx_array_bytes.extend_from_slice(&num.to_le_bytes());
        // }
        // wasm.memory
        //     .write(&mut wasm.store, dmx_array_offset, &dmx_array_bytes)?;

        //
        // Data array.
        //
        let data_array_offset = 0x90000; // Arbitrary offset
        let data_array_len = self.data.len();
        // let mut data_array_bytes = Vec::new();
        // for &num in &self.data {
        //     data_array_bytes.extend_from_slice(&num.to_le_bytes());
        // }
        // wasm.memory
        //     .write(&mut wasm.store, data_array_offset, &data_array_bytes)?;

        let dmx_array_offset = 0x20000; // TODO: make this offset a const.

        // Call the function with the pointer and length
        func.call(
            &mut wasm.store,
            (
                tick_array_offset as i32,
                tick_array_len,
                dmx_array_offset as i32,
                DMX_LEN as i32,
                data_array_offset as i32,
                data_array_len as i32,
                midi_array_offset as i32,
                midi_array_len as i32,
            ),
        )?;

        //
        // Read back the modified DMX array
        //
        // let mut dmx_array_bytes: Vec<u8> = Vec::with_capacity(dmx_array_len as usize);
        let mut updated_dmx_bytes = vec![0u8; DMX_LEN];
        wasm.memory
            .read(&mut wasm.store, dmx_array_offset, &mut updated_dmx_bytes)?;
        self.dmx = updated_dmx_bytes;

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
