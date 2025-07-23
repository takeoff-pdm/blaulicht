use anyhow::anyhow;
use anyhow::Context;
use blaulicht_shared::ControlEvent;
use blaulicht_shared::ControlEventMessage;
use blaulicht_shared::EventOriginator;
use blaulicht_shared::TickInput;
use cpal::Device;
use crossbeam_channel::Sender;
use log::{debug, info, warn};
use std::borrow::Cow;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::Mutex;
use std::thread;
use std::time::Duration;
use std::u8;
use std::{collections::HashMap, fs, net::UdpSocket, path::PathBuf, time::Instant};

use wasmtime::*;

use crate::msg::MidiEvent;
use crate::msg::WasmLogBody;
use crate::{
    config::PluginConfig,
    msg::{SystemMessage, WasmControlsConfig, WasmControlsLog, WasmControlsSet},
    plugin::{
        midi::{self},
        Plugin, PluginManager,
    },
};

// TODO: optimize this module:
// Load multiple plugins at once, not just one.
// Manage plugins and their health / allow activation / deactivation.

impl PluginManager {
    pub fn instantiate_plugins(&mut self) -> anyhow::Result<()> {
        //
        // Basic engine setup.
        //
        let mut config = Config::new();
        config.strategy(wasmtime::Strategy::Cranelift);
        config.cranelift_opt_level(wasmtime::OptLevel::Speed);
        let engine = Engine::new(&config).with_context(|| "Failed to create Wasmtime engine")?;

        let mut linker = Linker::new(&engine);
        self.provide_host_functions(&mut linker)?;

        let mut modules = HashMap::new();

        for (plugin_id, plugin) in self.plugin_config.iter().enumerate() {
            if !plugin.enabled {
                warn!("Plugin {} is disabled, skipping.", plugin.file_path);
                continue;
            }

            let plugin_name = plugin.file_path.to_string();

            let wasm_bytes =
                fs::read(&plugin.file_path).with_context(|| "Failed to read Wasm file")?;
            let module =
                Module::new(&engine, wasm_bytes).with_context(|| "Failed to create Wasm module")?;
            modules.insert(plugin_name.clone(), module.clone());

            let mut store = Store::new(&engine, ());

            // Instantiate the module
            let instance = linker
                .instantiate(&mut store, &module)
                .map_err(|e| anyhow!("Failed to instantiate Wasm module linker: {e}"))?;

            //
            // Initialize data.
            //

            // TODO: do not initialize DMX buffer?
            // let memory = instance
            //     .get_memory(&mut store, "memory")
            //     .expect("Memory not found");

            // // Initialize DMX.
            // let dmx_array_offset = 0x20000; // TODO: make this offset a const.
            // let mut dmx_array_bytes: Vec<u8> = vec![0; DMX_LEN];
            // for &num in &self.dmx {
            //     dmx_array_bytes.extend_from_slice(&num.to_le_bytes());
            // }
            // memory.write(&mut store, dmx_array_offset, &dmx_array_bytes)?;

            debug!("Loaded plugin: {}", plugin_name);

            // Store the instance and store for future use
            let mut plugin = Plugin {
                path: plugin.file_path.clone().into(),
                memory: instance
                    .get_memory(&mut store, "memory")
                    .expect("Memory not found"),
                store,
                instance,
                midi_status: MidiStatus::dummy(),
            };

            plugin
                .acquire_midi_buffer_addresses()
                .map_err(|e| anyhow!("Failed to acquire MIDI buffer addresses: {e}"))?;

            debug_assert!(plugin_id < u8::MAX as usize);

            self.plugins.insert(plugin_id as u8, plugin);
        }

        info!(
            "Loaded and instantiated {} wasm modules.",
            self.plugins.len()
        );

        //
        // Bind Wasm functions.
        //

        // TODO: UDP support.
        // let socket = UdpSocket::bind("0.0.0.0:0")?;

        log::info!("[WASM] initialized.");

        Ok(())
    }

    fn provide_host_functions(&mut self, linker: &mut Linker<()>) -> anyhow::Result<()> {
        let so = self.system_out.clone();
        linker.func_wrap::<_, ()>(
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

                // TODO: implement UDP support.
                todo!("UDP support not implemented yet");
                // socket
                //     .send_to(&body_buffer, target_addr.clone())
                //     .unwrap_or_else(|e| {
                //         so.send(SystemMessage::Log(format!(
                //             "UDP error: SEND to {target_addr}: {e}"
                //         )))
                //         .expect("Failed to send log message");
                //         0
                //     });
            },
        )?;

        let so = self.system_out.clone();
        linker.func_wrap::<_, ()>(
            "blaulicht",
            "log",
            move |mut caller: Caller<'_, ()>, plugin_id: i32, str_pointer: i32, str_len: i32| {
                let memory = caller
                    .get_export("memory")
                    .and_then(|export| export.into_memory())
                    .expect("Failed to find memory");

                let mut buffer = vec![0u8; str_len as usize];
                memory
                    .read(&caller, str_pointer as usize, &mut buffer)
                    .expect("Failed to read memory");

                let received_string = String::from_utf8_lossy(&buffer).to_string();

                so.send(SystemMessage::WasmLog(WasmLogBody {
                    plugin_id: plugin_id as u8,
                    msg: received_string.into(),
                }))
                .expect("Failed to send log message");
            },
        )?;

        let event_bus = self.event_bus.clone();
        linker.func_wrap::<_, ()>(
            "blaulicht",
            "bl_send_event",
            move |mut caller: Caller<'_, ()>, str_pointer: i32, str_len: i32| {
                let memory = caller
                    .get_export("memory")
                    .and_then(|export| export.into_memory())
                    .expect("Failed to find memory");

                let mut buffer = vec![0u8; str_len as usize];
                memory
                    .read(&caller, str_pointer as usize, &mut buffer)
                    .expect("Failed to read memory");

                // TODO: may panic.
                let event = ControlEvent::deserialize(&buffer);
                event_bus.send(ControlEventMessage::new(EventOriginator::Plugin, event));
            },
        )?;

        let so = self.system_out.clone();
        linker.func_wrap::<_, ()>(
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
        linker.func_wrap::<_, ()>(
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
        linker.func_wrap::<_, ()>(
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

        let mo = self.to_midi_devices.clone();
        linker.func_wrap::<_, ()>(
            "blaulicht",
            "bl_transmit_midi",
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

        linker.func_wrap::<_, ()>("blaulicht", "bl_report_panic", move || {
            println!("REPORT PANIC!");
            // TODO: do we really need this?
        })?;

        // let mo = self.to_midi_devices.clone();
        // linker.func_wrap::<_, ()>(
        //     "blaulicht",
        //     "bl_transmit_midi_bulk",
        //     move |buf_start: u32, buf_len: u32| {
        //         mo.send(MidiEvent {

        //         })
        //         .unwrap();
        //     },
        // )?;

        let midi_manager = Arc::clone(&self.midi_manager_ref);
        linker.func_wrap::<_, u32>(
            "blaulicht",
            "bl_open_midi_device",
            move |mut caller: Caller<'_, ()>, str_pointer: i32, str_len: i32| {
                let memory = caller
                    .get_export("memory")
                    .and_then(|export| export.into_memory())
                    .expect("Failed to find memory");

                let mut buffer = vec![0u8; str_len as usize];
                memory
                    .read(&caller, str_pointer as usize, &mut buffer)
                    .expect("Failed to read memory");

                let device_name = String::from_utf8_lossy(&buffer).to_string();

                println!("open midi...");

                let mut midi_manager = midi_manager.lock().unwrap();
                midi_manager.request_device(&device_name).unwrap_or(u8::MAX) as u32
            },
        )?;

        Ok(())
    }
}

// -------------------------------------------------------------

// pub struct TickEngine {
//     timer_start: Instant,
//     data: Vec<i32>,
//     midi_out: Sender<MidiEvent>,
//     system_out: Sender<SystemMessage>,
// }

// impl TickEngine {
//     pub fn create(midi_out: Sender<MidiEvent>, system_out: Sender<SystemMessage>) -> Result<Self> {
//         let mut engine = TickEngine {
//             timer_start: Instant::now(),
//             data: vec![0; 1000],
//             midi_out,
//             system_out,
//         };

//         engine.init_wasm()?;
//         engine.first_tick()?;

//         Ok(engine)
//     }

//     pub fn dmx(&self) -> &[u8] {
//         &self.dmx
//     }

//     fn init_wasm(&mut self) -> Result<()> {
//         let mut config = Config::new();
//         config.strategy(wasmtime::Strategy::Cranelift);
//         config.cranelift_opt_level(wasmtime::OptLevel::Speed);

//         let engine = Engine::new(&config)?;
//         let module = Module::from_file(&engine, "./wasm/output.wasm")?;

//         let mut store = Store::new(&engine, ());

//         let mut linker = Linker::new(&engine);

//         // UDP support.
//         let socket = UdpSocket::bind("0.0.0.0:0")?;

//         let so = self.system_out.clone();
//         linker.func_wrap(
//             "blaulicht",
//             "udp",
//             move |mut caller: Caller<'_, ()>,
//                   target_addr_pointer: i32,
//                   target_addr_len: i32,
//                   byte_arr_pointer: i32,
//                   byte_arr_len: i32| {
//                 let memory = caller
//                     .get_export("memory")
//                     .and_then(|export| export.into_memory())
//                     .expect("Failed to find memory");

//                 let mut body_buffer = vec![0u8; byte_arr_len as usize];
//                 memory
//                     .read(&caller, byte_arr_pointer as usize, &mut body_buffer)
//                     .expect("Failed to read memory");

//                 let mut addr_buffer = vec![0u8; target_addr_len as usize];
//                 memory
//                     .read(&caller, target_addr_pointer as usize, &mut addr_buffer)
//                     .expect("Failed to read memory");

//                 let target_addr = String::from_utf8_lossy(&addr_buffer).to_string();

//                 socket
//                     .send_to(&body_buffer, target_addr.clone())
//                     .unwrap_or_else(|e| {
//                         so.send(SystemMessage::Log(format!(
//                             "UDP error: SEND to {target_addr}: {e}"
//                         )))
//                         .expect("Failed to send log message");
//                         0
//                     });
//             },
//         )?;

//         let so = self.system_out.clone();
//         linker.func_wrap(
//             "blaulicht",
//             "log",
//             move |mut caller: Caller<'_, ()>, str_pointer: i32, str_len: i32| {
//                 let memory = caller
//                     .get_export("memory")
//                     .and_then(|export| export.into_memory())
//                     .expect("Failed to find memory");

//                 let mut buffer = vec![0u8; str_len as usize];
//                 memory
//                     .read(&caller, str_pointer as usize, &mut buffer)
//                     .expect("Failed to read memory");

//                 let received_string = String::from_utf8_lossy(&buffer).to_string();

//                 log::debug!("[WASM] {received_string}");

//                 so.send(SystemMessage::WasmLog(received_string))
//                     .expect("Failed to send log message");
//             },
//         )?;

//         let so = self.system_out.clone();
//         linker.func_wrap(
//             "blaulicht",
//             "controls_log",
//             move |mut caller: Caller<'_, ()>, x: i32, y: i32, str_pointer: i32, str_len: i32| {
//                 let memory = caller
//                     .get_export("memory")
//                     .and_then(|export| export.into_memory())
//                     .expect("Failed to find memory");

//                 let mut buffer = vec![0u8; str_len as usize];
//                 memory
//                     .read(&caller, str_pointer as usize, &mut buffer)
//                     .expect("Failed to read memory");

//                 let received_string = String::from_utf8_lossy(&buffer).to_string();

//                 so.send(SystemMessage::WasmControlsLog(WasmControlsLog {
//                     x: x as u8,
//                     y: y as u8,
//                     value: received_string,
//                 }))
//                 .expect("Failed to send controls log message");
//             },
//         )?;

//         let so = self.system_out.clone();
//         linker.func_wrap(
//             "blaulicht",
//             "controls_set",
//             move |mut _caller: Caller<'_, ()>, x: i32, y: i32, value: i32| {
//                 so.send(SystemMessage::WasmControlsSet(WasmControlsSet {
//                     x: x as u8,
//                     y: y as u8,
//                     value: value != 0,
//                 }))
//                 .expect("Failed to send controls set message");
//             },
//         )?;

//         let so = self.system_out.clone();
//         linker.func_wrap(
//             "blaulicht",
//             "controls_config",
//             move |mut _caller: Caller<'_, ()>, x: i32, y: i32| {
//                 so.send(SystemMessage::WasmControlsConfig(WasmControlsConfig {
//                     x: x as u8,
//                     y: y as u8,
//                 }))
//                 .expect("Failed to send controls config message");
//                 log::debug!("[WASM] controls_config: {x} {y}");
//             },
//         )?;

//         let mo = self.midi_out.clone();
//         linker.func_wrap(
//             "blaulicht",
//             "bl_midi",
//             move |device: i32, status: i32, kind: i32, value: i32| {
//                 mo.send(MidiEvent {
//                     device: device as u8,
//                     status: status as u8,
//                     data0: kind as u8,
//                     data1: value as u8,
//                 })
//                 .unwrap();
//             },
//         )?;

//         let instance = linker.instantiate(&mut store, &module)?;

//         let memory = instance
//             .get_memory(&mut store, "memory")
//             .expect("Memory not found");

//         // Initialize DMX.
//         let dmx_array_offset = 0x20000; // TODO: make this offset a const.
//         let mut dmx_array_bytes: Vec<u8> = vec![0; DMX_LEN];
//         for &num in &self.dmx {
//             dmx_array_bytes.extend_from_slice(&num.to_le_bytes());
//         }
//         memory.write(&mut store, dmx_array_offset, &dmx_array_bytes)?;

//         self.wasm = Some(WasmEngine {
//             store,
//             instance,
//             memory,
//         });

//         log::info!("[WASM] initialized.");

//         Ok(())
//     }

//     pub fn reload(&mut self) -> Result<()> {
//         // Reset the data.
//         self.data.fill(0);
//         // Reset the clock.
//         self.timer_start = Instant::now();
//         self.init_wasm()?;
//         self.first_tick()
//     }

//     pub fn first_tick(&mut self) -> Result<()> {
//         self.tick(
//             TickInput {
//                 clock: self.timer_start.elapsed().as_millis() as u32,
//                 initial: true,
//             },
//             &[],
//         )
//     }

//     pub fn tick(&mut self, input: TickInput, midi_events: &[MidiEvent]) -> Result<()> {
//         let wasm = self.wasm.as_mut().unwrap();

//         //
//         // Tick function.
//         //
//         let func = wasm
//             .instance
//             .get_typed_func::<(i32, i32, i32, i32, i32, i32, i32, i32), ()>(
//                 &mut wasm.store,
//                 "internal_tick", // TODO: external type and name constants.
//             )?;

//         //
//         // Tick input array.
//         //
//         let tick_array_offset = 0x10000; // Arbitrary offset
//         let tick_array_data = input.serialize();
//         let tick_array_len = tick_array_data.len() as i32;
//         let mut tick_array_bytes = Vec::new();
//         for &num in &tick_array_data {
//             tick_array_bytes.extend_from_slice(&num.to_le_bytes());
//         }
//         wasm.memory
//             .write(&mut wasm.store, tick_array_offset, &tick_array_bytes)?;

//         // TODO: macro for this array stuff.

//         //
//         // MIDI array.
//         //
//         let midi_array_offset = 0x80000; // TODO: make this offset a const.
//         let midi_array_len = midi_events.len() as i32;

//         if midi_array_len > 100 {
//             panic!("TOO many MIDI events!");
//         }

//         let mut midi_array_bytes = Vec::new();

//         let midi_events_packed: Vec<u32> = midi_events
//             .iter()
//             .map(|event| {
//                 ((event.device as u32) << 24)
//                     | (event.status as u32) << 16
//                     | (event.data0 as u32) << 8
//                     | (event.data1 as u32)
//             })
//             .collect::<Vec<u32>>();

//         for &num in &midi_events_packed {
//             midi_array_bytes.extend_from_slice(&num.to_le_bytes());
//         }
//         wasm.memory
//             .write(&mut wasm.store, midi_array_offset, &midi_array_bytes)?;

//         // for &num in &self.dmx {
//         //     dmx_array_bytes.extend_from_slice(&num.to_le_bytes());
//         // }
//         // wasm.memory
//         //     .write(&mut wasm.store, dmx_array_offset, &dmx_array_bytes)?;

//         //
//         // Data array.
//         //
//         let data_array_offset = 0x90000; // Arbitrary offset
//         let data_array_len = self.data.len();
//         // let mut data_array_bytes = Vec::new();
//         // for &num in &self.data {
//         //     data_array_bytes.extend_from_slice(&num.to_le_bytes());
//         // }
//         // wasm.memory
//         //     .write(&mut wasm.store, data_array_offset, &data_array_bytes)?;

//         let dmx_array_offset = 0x20000; // TODO: make this offset a const.

//         // Call the function with the pointer and length
//         func.call(
//             &mut wasm.store,
//             (
//                 tick_array_offset as i32,
//                 tick_array_len,
//                 dmx_array_offset as i32,
//                 DMX_LEN as i32,
//                 data_array_offset,
//                 data_array_len as i32,
//                 midi_array_offset as i32,
//                 midi_array_len,
//             ),
//         )?;

//         //
//         // Read back the modified DMX array
//         //
//         // let mut dmx_array_bytes: Vec<u8> = Vec::with_capacity(dmx_array_len as usize);
//         let mut updated_dmx_bytes = vec![0u8; DMX_LEN];
//         wasm.memory
//             .read(&mut wasm.store, dmx_array_offset, &mut updated_dmx_bytes)?;
//         self.dmx = updated_dmx_bytes;

//         //
//         // Read back data array.
//         //
//         // let mut updated_data_bytes = vec![0u8; data_array_bytes.len()];
//         // wasm.memory
//         //     .read(&mut wasm.store, data_array_offset, &mut updated_data_bytes)?;
//         // let updated_data_bytes: Vec<i32> = updated_data_bytes
//         //     .chunks_exact(4)
//         //     .map(|chunk| i32::from_le_bytes(chunk.try_into().unwrap()))
//         //     .collect();
//         // self.data = updated_data_bytes;

//         Ok(())
//     }
// }

#[derive(Debug, Clone)]
pub struct MidiStatus {
    start_addr: i32,
    length_start_addr: i32,
}

impl MidiStatus {
    pub fn dummy() -> Self {
        MidiStatus {
            start_addr: 0,
            length_start_addr: 0,
        }
    }

    pub fn buffer_addr(&self) -> usize {
        self.start_addr as usize
    }

    pub fn buffer_len_addr(&self) -> usize {
        self.length_start_addr as usize
    }
}

impl Plugin {
    fn acquire_midi_buffer_addresses(&mut self) -> anyhow::Result<()> {
        debug!("Acquiring MIDI buffer addresses for plugin: {}", self.path);
        //
        // Get midi buffer start address.
        //
        let func = self.instance.get_typed_func::<(), i32>(
            &mut self.store,
            "__internal_get_global_midi_buffer_start_addr", // TODO: external type and name constants.
        )?;

        let midi_buffer_start_addr = func.call(&mut self.store, ())?;

        //
        // Get midi buffer length start address.
        //
        let func = self.instance.get_typed_func::<(), i32>(
            &mut self.store,
            "__internal_get_global_midi_buffer_length_start_addr", // TODO: external type and name constants.
        )?;

        let midi_buffer_length_start_addr = func.call(&mut self.store, ())?;

        let addrs = MidiStatus {
            start_addr: midi_buffer_start_addr,
            length_start_addr: midi_buffer_length_start_addr,
        };

        debug!("Acquired MIDI buffer addresses: {:?}", addrs);

        self.midi_status = addrs;
        Ok(())
    }
}
