use anyhow::anyhow;
use anyhow::Context;
use blaulicht_shared::ArtNetReceiverInfo;
use blaulicht_shared::ControlEvent;
use blaulicht_shared::ControlEventMessage;
use blaulicht_shared::EventOriginator;
use blaulicht_shared::LogLevel;
use blaulicht_shared::MainUiEvent;
use blaulicht_shared::PluginStateLocation;

use egui::ahash::HashMapExt;
use std::sync::Arc;
use std::u8;
use std::{collections::HashMap, fs, net::UdpSocket};
// use tracing::{debug, error, info, warn};

#[cfg(feature = "wasmtime")]
use super::PluginWasmState;
#[cfg(feature = "wasmtime")]
use wasmtime::*;

use crate::command::Command;
use crate::msg::MidiEvent;
use crate::msg::WasmLogBody;
use crate::state::PluginOpenState;
use crate::ui_ops::WasmUiOp;
use crate::{
    msg::SystemMessage,
    plugin::{Plugin, PluginManager},
};

// TODO: optimize this module:
// Load multiple plugins at once, not just one.
// Manage plugins and their health / allow activation / deactivation.

// TODO: do this!

//
// MOCKED IMPLEMENTATION FOR FAST DEBUG BUILDS USING CRANELIFT.
//

#[cfg(not(feature = "wasmtime"))]
impl PluginManager {
    pub fn instantiate_plugins(&mut self) -> anyhow::Result<()> {
        self.system_out
            .send(SystemMessage::Log(
                "WASM subsystem is disabled. (compile flags)".to_string(),
                LogLevel::Warn,
            ))
            .unwrap();
        Ok(())
    }
}

//
// REAL WASMTIME IMPLEMENTATION.
//

#[cfg(feature = "wasmtime")]
impl PluginManager {
    pub fn instantiate_plugins(&mut self) -> anyhow::Result<()> {
        use tempdir::TempDir;

        tracing::debug!("[WASM] subsystem initializing...");

        //
        // basic engine setup.
        //
        let mut config = wasmtime::Config::new();
        config.strategy(wasmtime::Strategy::Cranelift);
        config.cranelift_opt_level(wasmtime::OptLevel::Speed);
        config.wasm_simd(true);
        config.wasm_relaxed_simd(true);
        config.wasm_backtrace_details(WasmBacktraceDetails::Enable);
        // TODO: make this tweakale
        config.memory_reservation(1 << 16);
        config.memory_guard_size(1 << 16);

        let wasmtime_cache_dir = TempDir::new("blaulicht_wasm_cache")?;

        tracing::info!(
            "[WASM] Cache dir at {}",
            wasmtime_cache_dir.path().to_string_lossy()
        );

        let path = wasmtime_cache_dir.path().join("config.toml");
        fs::write(&path, include_bytes!("../../wasmtime-cache-config.toml"))?;
        config.cache_config_load(path)?;

        let engine = Engine::new(&config).with_context(|| "failed to create wasmtime engine")?;

        let mut linker = Linker::new(&engine);
        self.provide_host_functions(&mut linker)?;

        let mut modules = HashMap::new();

        for (plugin_id, plugin) in self.plugin_config.iter().enumerate() {
            if !plugin.enabled {
                tracing::warn!("Plugin <{}> is disabled, skipping.", plugin.file_path);
                continue;
            }

            let plugin_name = plugin.file_path.to_string();

            tracing::debug!("[WASM] Initializing plugin <{plugin_name}>...");

            let wasm_bytes = fs::read(&plugin.file_path)
                .with_context(|| format!("failed to read wasm file: <{plugin_name}>"))?;
            let module =
                Module::new(&engine, wasm_bytes).with_context(|| "failed to create wasm module")?;
            modules.insert(plugin_name.clone(), module.clone());

            let mut store = Store::new(&engine, ());

            // instantiate the module
            let instance = linker
                .instantiate(&mut store, &module)
                .map_err(|e| anyhow!("failed to instantiate wasm module linker: {e}"))?;

            //
            // initialize data.
            //

            tracing::info!("[WASM] Loaded plugin <{plugin_name}>");

            // store the instance and store for future use
            let plugin_wasm_state = PluginWasmState {
                memory: instance
                    .get_memory(&mut store, "memory")
                    .expect("memory not found"),
                store,
                instance,
            };

            // TODO: more context here, also just fail this one plugin and not crash whole plugin
            // manager.
            let mut plugin = Plugin::new(plugin.file_path.clone().into(), plugin_wasm_state)?;

            plugin
                .acquire_midi_buffer_addresses()
                .map_err(|e| anyhow!("failed to acquire midi buffer addresses: {e}"))?;

            plugin
                .acquire_serial_buffer_addresses()
                .map_err(|e| anyhow!("failed to acquire serial buffer addresses: {e}"))?;

            plugin
                .acquire_state_buffer_address()
                .map_err(|e| anyhow!("failed to acquire state buffer addresses: {e}"))?;

            plugin
                .acquire_udp_buffer_addresses()
                .map_err(|e| anyhow!("failed to acquire UDP buffer addresses: {e}"))?;

            debug_assert!(plugin_id < u8::MAX as usize);

            self.plugins.insert(plugin_id as u8, plugin);
        }

        tracing::debug!(
            "[WASM]: loaded and instantiated {} wasm modules.",
            self.plugins.len()
        );

        Ok(())
    }

    fn provide_host_functions(&mut self, linker: &mut Linker<()>) -> anyhow::Result<()> {
        let so = self.system_out.clone();

        let socket = UdpSocket::bind("0.0.0.0:0")?;

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
                    .expect("failed to find memory");

                let mut body_buffer = vec![0u8; byte_arr_len as usize];
                memory
                    .read(&caller, byte_arr_pointer as usize, &mut body_buffer)
                    .expect("failed to read memory");

                let mut addr_buffer = vec![0u8; target_addr_len as usize];
                memory
                    .read(&caller, target_addr_pointer as usize, &mut addr_buffer)
                    .expect("failed to read memory");

                let target_addr = String::from_utf8_lossy(&addr_buffer).to_string();

                // todo: implement udp support.
                // todo!("udp support not implemented yet");
                socket
                    .send_to(&body_buffer, target_addr.clone())
                    .unwrap_or_else(|e| {
                        so.send(SystemMessage::Log(
                            format!("udp error: send to {target_addr}: {e}"),
                            LogLevel::Err,
                        ))
                        .expect("failed to send log message");
                        0
                    });
            },
        )?;

        linker.func_wrap::<_, ()>(
            "blaulicht",
            "sys",
            move |mut caller: Caller<'_, ()>,
                  _plugin_id: i32,
                  str_pointer: i32,
                  str_len: i32,
                  output_str_pointer: i32,
                  output_str_capacity: i32| {
                let memory = caller
                    .get_export("memory")
                    .and_then(|export| export.into_memory())
                    .expect("failed to find memory");

                let mut buffer = vec![0u8; str_len as usize];
                memory
                    .read(&caller, str_pointer as usize, &mut buffer)
                    .expect("failed to read memory");

                let report_memory_error =
                    |action: &str, err: wasmtime::MemoryAccessError| {
                        let msg = format!("WASM: {action}: {err}");
                        tracing::error!("{msg}");
                    };

                let write_stdout_to_guest = |caller: &mut Caller<'_, ()>,
                                             stdout_bytes: &[u8]|
                 -> bool {
                    let capacity = output_str_capacity.max(0) as usize;

                    if capacity == 0 {
                        return true;
                    }

                    let zero_buf = vec![0u8; capacity];
                    if let Err(err) = memory.write(
                        &mut *caller,
                        output_str_pointer as usize,
                        &zero_buf,
                    ) {
                        report_memory_error(
                            "Failed to clear stdout buffer in guest memory",
                            err,
                        );
                        return false;
                    }

                    let copy_len = stdout_bytes
                        .len()
                        .min(capacity.saturating_sub(1));

                    if copy_len > 0 {
                        if let Err(err) = memory.write(
                            &mut *caller,
                            output_str_pointer as usize,
                            &stdout_bytes[..copy_len],
                        ) {
                            report_memory_error(
                                "Failed to write stdout into guest memory",
                                err,
                            );
                            return false;
                        }
                    }

                    true
                };

                let received_string = String::from_utf8_lossy(&buffer).to_string();

                let output = Command::new("bash")
                    .arg("-c")
                    .arg(received_string)
                    .run()
                    .and_then(|handle| handle.wait());

                match output {
                    Ok(o) => {
                        let stdout_bytes = &o.stdout;
                        let stdout = String::from_utf8_lossy(stdout_bytes);
                        let stderr = String::from_utf8_lossy(&o.stderr);

                        if write_stdout_to_guest(&mut caller, stdout_bytes) {
                            let capacity = output_str_capacity.max(0) as usize;
                            let max_payload = capacity.saturating_sub(1);
                            if capacity > 0 && stdout_bytes.len() > max_payload {
                                tracing::warn!(
                                        "WASM: Command STDOUT truncated to fit into buffer of size {capacity}");
                            }
                        }

                        tracing::debug!("WASM: Command STDOUT: {stdout}");
                        tracing::debug!("WASM: Command STDERR: {stderr}");

                        if !o.status.success() {
                            let code = o.status.code().unwrap_or(199);
                            tracing::error!("WASM: Command failed with code: {code}");
                        }
                    }
                    Err(err) => {
                        write_stdout_to_guest(&mut caller, &[]);

                        tracing::error!("WASM: Command invocation error: {err}");
                    }
                }
            },
        )?;

        let so = self.system_out.clone();
        linker.func_wrap::<_, ()>(
            "blaulicht",
            "log",
            move |mut caller: Caller<'_, ()>,
                  plugin_id: i32,
                  str_pointer: i32,
                  str_len: i32,
                  level_raw: i32| {
                let memory = caller
                    .get_export("memory")
                    .and_then(|export| export.into_memory())
                    .expect("failed to find memory");

                let mut buffer = vec![0u8; str_len as usize];
                memory
                    .read(&caller, str_pointer as usize, &mut buffer)
                    .expect("failed to read memory");

                let received_string = String::from_utf8_lossy(&buffer).to_string();

                let level : LogLevel =
                    level_raw.try_into().unwrap_or_else(|_|  {
                        tracing::error!("a plugin called blaulicht::log with an illegal log-level-integer: {level_raw}");
                        LogLevel::Info
                    });

                tracing::debug!("WASM: {received_string}");

                so.send(SystemMessage::WasmLog(WasmLogBody {
                    plugin_id: plugin_id as u8,
                    msg: received_string.into(),
                    level,
                }))
                .expect("failed to send log message");
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
                    .expect("failed to find memory");

                let mut buffer = vec![0u8; str_len as usize];
                memory
                    .read(&caller, str_pointer as usize, &mut buffer)
                    .expect("failed to read memory");

                let event = match std::panic::catch_unwind(|| ControlEvent::deserialize(&buffer)) {
                    Ok(event) => event,
                    Err(_) => {
                        tracing::warn!(
                            "WASM: Failed to deserialize ControlEvent (len={})",
                            str_len
                        );
                        return;
                    }
                };
                event_bus.send(ControlEventMessage::new(EventOriginator::Plugin, event));
            },
        )?;

        // ---- egui UI bridging ----
        let state_ref = Arc::clone(&self.state_ref);
        linker.func_wrap::<_, ()>("blaulicht", "ui_begin", move |plugin_id: i32| {
            // Double-buffer swap: move completed back-buffer ops to the front buffer atomically,
            // then clear the back buffer to start recording the next frame.
            let pid = plugin_id as u8;
            let mut back_map = state_ref.plugin_ui_ops_back.write().unwrap();
            let mut front_map = state_ref.plugin_ui_ops.write().unwrap();

            if let Some(back_vec) = back_map.get_mut(&pid) {
                let completed = std::mem::take(back_vec);
                front_map.insert(pid, completed);
            } else {
                // No back buffer yet: ensure a front entry exists (empty) for UI to read
                front_map.entry(pid).or_default();
            }

            // Start a fresh back buffer for recording
            back_map.insert(pid, Vec::new());
        })?;

        let state_ref = Arc::clone(&self.state_ref);
        linker.func_wrap::<_, i32>("blaulicht", "ui_is_open", move |plugin_id: i32| {
            let pid = plugin_id as u8;
            let map = state_ref.plugin_ui_visibility.read().unwrap();
            if map.get(&pid).is_some_and(|visibility| visibility.open) {
                1
            } else {
                0
            }
        })?;

        let state_ref = Arc::clone(&self.state_ref);
        let event_bus = self.event_bus.clone();
        linker.func_wrap::<_, ()>("blaulicht", "ui_maximize_screen", move |plugin_id: i32| {
            let pid = plugin_id as u8;

            let was_open = {
                let mut map = state_ref.plugin_ui_visibility.write().unwrap();
                let entry = map.entry(pid).or_insert(PluginOpenState::CLOSED);
                let was_open = entry.open;
                entry.open = true;
                was_open
            };

            {
                let mut map = state_ref.plugin_ui_popped_out.write().unwrap();
                map.insert(pid, true);
            }

            {
                let mut map = state_ref.plugin_ui_maximize_requested.write().unwrap();
                map.insert(pid, true);
            }

            if !was_open {
                event_bus.send(ControlEventMessage::new(
                    EventOriginator::Plugin,
                    ControlEvent::MainUi(MainUiEvent::SetPluginUIOpen {
                        plugin_id: pid,
                        open: true,
                    }),
                ));
            }
        })?;

        let system_out = self.system_out.clone();
        linker.func_wrap::<_, ()>(
            "blaulicht",
            "ui_alert",
            move |mut caller: Caller<'_, ()>,
                  plugin_id: i32,
                  str_pointer: i32,
                  str_len: i32,
                  duration_ms: u32| {
                let memory = caller
                    .get_export("memory")
                    .and_then(|export| export.into_memory())
                    .expect("failed to find memory");

                let mut buffer = vec![0u8; str_len.max(0) as usize];
                memory
                    .read(&caller, str_pointer as usize, &mut buffer)
                    .expect("failed to read memory");

                let label = String::from_utf8_lossy(&buffer).to_string();

                system_out
                    .send(SystemMessage::PluginAlert {
                        plugin_id: plugin_id as u8,
                        label,
                        duration_ms: duration_ms.max(1),
                    })
                    .expect("failed to send plugin alert");
            },
        )?;

        let state_ref = Arc::clone(&self.state_ref);
        linker.func_wrap::<_, u32>(
            "blaulicht",
            "ui_list_external_screens",
            move |mut caller: Caller<'_, ()>, _plugin_id: i32, buffer_ptr: i32, buffer_len: i32| {
                let screens = state_ref.external_screens.read().unwrap().clone();
                let json = serde_json::to_string(&screens).unwrap_or_else(|_| "[]".to_string());
                let json_bytes = json.as_bytes();

                let memory = caller
                    .get_export("memory")
                    .and_then(|export| export.into_memory())
                    .expect("failed to find memory");

                let write_len = std::cmp::min(json_bytes.len(), buffer_len.max(0) as usize);

                if write_len > 0 {
                    memory
                        .write(&mut caller, buffer_ptr as usize, &json_bytes[..write_len])
                        .expect("failed to write memory");
                }

                write_len as u32
            },
        )?;

        let event_bus = self.event_bus.clone();
        linker.func_wrap::<_, i32>(
            "blaulicht",
            "ui_create_external_screen",
            move |plugin_id: i32, width: i32, height: i32| {
                if width <= 0 || height <= 0 {
                    tracing::warn!(
                        "WASM: Refusing to create external screen with invalid size {width}x{height}"
                    );
                    return 0;
                }

                event_bus.send(ControlEventMessage::new(
                    EventOriginator::Plugin,
                    ControlEvent::MainUi(MainUiEvent::CreateOwnedExternalScreen {
                        owner_plugin_id: plugin_id as u8,
                        width: width as u32,
                        height: height as u32,
                    }),
                ));
                1
            },
        )?;

        let state_ref = Arc::clone(&self.state_ref);
        let event_bus = self.event_bus.clone();
        linker.func_wrap::<_, i32>(
            "blaulicht",
            "ui_remove_external_screen",
            move |plugin_id: i32, _index: i32| {
                let owner_plugin_id = plugin_id as u8;
                let exists = state_ref
                    .external_screens
                    .read()
                    .unwrap()
                    .iter()
                    .any(|screen| screen.owner_plugin_id == Some(owner_plugin_id));

                if !exists {
                    tracing::warn!(
                        "WASM: Refusing to remove unknown owned external screen for plugin {owner_plugin_id}"
                    );
                    return 0;
                }

                event_bus.send(ControlEventMessage::new(
                    EventOriginator::Plugin,
                    ControlEvent::MainUi(MainUiEvent::RemoveOwnedExternalScreen {
                        owner_plugin_id,
                    }),
                ));
                1
            },
        )?;

        let state_ref = Arc::clone(&self.state_ref);
        linker.func_wrap::<_, ()>(
            "blaulicht",
            "ui_label",
            move |mut caller: Caller<'_, ()>, plugin_id: i32, str_pointer: i32, str_len: i32| {
                let memory = caller
                    .get_export("memory")
                    .and_then(|export| export.into_memory())
                    .expect("failed to find memory");

                let mut buffer = vec![0u8; str_len as usize];
                memory
                    .read(&caller, str_pointer as usize, &mut buffer)
                    .expect("failed to read memory");
                let text = String::from_utf8_lossy(&buffer).to_string();

                let mut map = state_ref.plugin_ui_ops_back.write().unwrap();
                map.entry(plugin_id as u8)
                    .or_default()
                    .push(WasmUiOp::Label(text));
            },
        )?;

        let state_ref = Arc::clone(&self.state_ref);
        linker.func_wrap::<_, ()>(
            "blaulicht",
            "ui_label_styled",
            move |mut caller: Caller<'_, ()>,
                  plugin_id: i32,
                  str_pointer: i32,
                  str_len: i32,
                  size: i32,
                  monospace: i32| {
                let memory = caller
                    .get_export("memory")
                    .and_then(|export| export.into_memory())
                    .expect("failed to find memory");

                let mut buffer = vec![0u8; str_len as usize];
                memory
                    .read(&caller, str_pointer as usize, &mut buffer)
                    .expect("failed to read memory");
                let text = String::from_utf8_lossy(&buffer).to_string();

                let mut map = state_ref.plugin_ui_ops_back.write().unwrap();
                map.entry(plugin_id as u8)
                    .or_default()
                    .push(WasmUiOp::LabelStyled {
                        text,
                        size,
                        monospace: monospace != 0,
                    });
            },
        )?;

        let state_ref = Arc::clone(&self.state_ref);
        linker.func_wrap::<_, ()>(
            "blaulicht",
            "ui_set_max_width",
            move |_caller: Caller<'_, ()>, plugin_id: i32, width: i32| {
                let mut map = state_ref.plugin_ui_ops_back.write().unwrap();
                map.entry(plugin_id as u8)
                    .or_default()
                    .push(WasmUiOp::SetMaxWidth { width });
            },
        )?;

        let state_ref = Arc::clone(&self.state_ref);
        linker.func_wrap::<_, ()>(
            "blaulicht",
            "ui_set_min_width",
            move |_caller: Caller<'_, ()>, plugin_id: i32, width: i32| {
                let mut map = state_ref.plugin_ui_ops_back.write().unwrap();
                map.entry(plugin_id as u8)
                    .or_default()
                    .push(WasmUiOp::SetMinWidth { width });
            },
        )?;

        let state_ref = Arc::clone(&self.state_ref);
        linker.func_wrap::<_, ()>("blaulicht", "ui_separator", move |plugin_id: i32| {
            let mut map = state_ref.plugin_ui_ops_back.write().unwrap();
            map.entry(plugin_id as u8)
                .or_default()
                .push(WasmUiOp::Separator);
        })?;

        let state_ref = Arc::clone(&self.state_ref);
        linker.func_wrap::<_, ()>(
            "blaulicht",
            "ui_button",
            move |mut caller: Caller<'_, ()>,
                  plugin_id: i32,
                  str_pointer: i32,
                  str_len: i32,
                  id: i32| {
                let memory = caller
                    .get_export("memory")
                    .and_then(|export| export.into_memory())
                    .expect("failed to find memory");

                let mut buffer = vec![0u8; str_len as usize];
                memory
                    .read(&caller, str_pointer as usize, &mut buffer)
                    .expect("failed to read memory");
                let label = String::from_utf8_lossy(&buffer).to_string();

                let mut map = state_ref.plugin_ui_ops_back.write().unwrap();
                map.entry(plugin_id as u8)
                    .or_default()
                    .push(WasmUiOp::Button {
                        label,
                        id: id as u8,
                    });
            },
        )?;

        let state_ref = Arc::clone(&self.state_ref);
        linker.func_wrap::<_, ()>(
            "blaulicht",
            "ui_button_styled",
            move |mut caller: Caller<'_, ()>,
                  plugin_id: i32,
                  str_pointer: i32,
                  str_len: i32,
                  id: i32,
                  enabled: i32| {
                let memory = caller
                    .get_export("memory")
                    .and_then(|export| export.into_memory())
                    .expect("failed to find memory");

                let mut buffer = vec![0u8; str_len as usize];
                memory
                    .read(&caller, str_pointer as usize, &mut buffer)
                    .expect("failed to read memory");
                let label = String::from_utf8_lossy(&buffer).to_string();

                let mut map = state_ref.plugin_ui_ops_back.write().unwrap();
                map.entry(plugin_id as u8)
                    .or_default()
                    .push(WasmUiOp::ButtonStyled {
                        label,
                        id: id as u8,
                        enabled: enabled != 0,
                    });
            },
        )?;

        let state_ref = Arc::clone(&self.state_ref);
        linker.func_wrap::<_, ()>(
            "blaulicht",
            "ui_combo_box",
            move |mut caller: Caller<'_, ()>,
                  plugin_id: i32,
                  label_ptr: i32,
                  label_len: i32,
                  id: i32,
                  options_ptr: i32,
                  options_len: i32,
                  selected: i32| {
                let memory = caller
                    .get_export("memory")
                    .and_then(|export| export.into_memory())
                    .expect("failed to find memory");

                let mut label_buf = vec![0u8; label_len as usize];
                memory
                    .read(&caller, label_ptr as usize, &mut label_buf)
                    .expect("failed to read memory");
                let label = String::from_utf8_lossy(&label_buf).to_string();

                let mut options_buf = vec![0u8; options_len as usize];
                memory
                    .read(&caller, options_ptr as usize, &mut options_buf)
                    .expect("failed to read memory");
                let options_str = String::from_utf8_lossy(&options_buf).to_string();
                let options: Vec<String> = if options_str.is_empty() {
                    Vec::new()
                } else {
                    options_str.split('\n').map(|s| s.to_string()).collect()
                };

                let mut map = state_ref.plugin_ui_ops_back.write().unwrap();
                map.entry(plugin_id as u8)
                    .or_default()
                    .push(WasmUiOp::ComboBox {
                        label,
                        id: id as u8,
                        options,
                        selected: selected as u8,
                    });
            },
        )?;

        // Checkbox
        let state_ref = Arc::clone(&self.state_ref);
        linker.func_wrap::<_, ()>(
            "blaulicht",
            "ui_checkbox",
            move |mut caller: Caller<'_, ()>,
                  plugin_id: i32,
                  str_pointer: i32,
                  str_len: i32,
                  id: i32,
                  checked: i32| {
                let memory = caller
                    .get_export("memory")
                    .and_then(|export| export.into_memory())
                    .expect("failed to find memory");

                let mut buffer = vec![0u8; str_len as usize];
                memory
                    .read(&caller, str_pointer as usize, &mut buffer)
                    .expect("failed to read memory");
                let label = String::from_utf8_lossy(&buffer).to_string();

                let mut map = state_ref.plugin_ui_ops_back.write().unwrap();
                map.entry(plugin_id as u8)
                    .or_default()
                    .push(WasmUiOp::Checkbox {
                        label,
                        id: id as u8,
                        checked: checked != 0,
                    });
            },
        )?;

        // Switch
        let state_ref = Arc::clone(&self.state_ref);
        linker.func_wrap::<_, ()>(
            "blaulicht",
            "ui_switch",
            move |mut caller: Caller<'_, ()>,
                  plugin_id: i32,
                  str_pointer: i32,
                  str_len: i32,
                  id: i32,
                  value: i32| {
                let memory = caller
                    .get_export("memory")
                    .and_then(|export| export.into_memory())
                    .expect("failed to find memory");

                let mut buffer = vec![0u8; str_len as usize];
                memory
                    .read(&caller, str_pointer as usize, &mut buffer)
                    .expect("failed to read memory");
                let label = String::from_utf8_lossy(&buffer).to_string();

                let mut map = state_ref.plugin_ui_ops_back.write().unwrap();
                map.entry(plugin_id as u8)
                    .or_default()
                    .push(WasmUiOp::Switch {
                        label,
                        id: id as u8,
                        value: value != 0,
                    });
            },
        )?;

        // Slider
        let state_ref = Arc::clone(&self.state_ref);
        linker.func_wrap::<_, ()>(
            "blaulicht",
            "ui_slider",
            move |mut caller: Caller<'_, ()>,
                  plugin_id: i32,
                  str_pointer: i32,
                  str_len: i32,
                  id: i32,
                  min: i32,
                  max: i32,
                  value: i32| {
                let memory = caller
                    .get_export("memory")
                    .and_then(|export| export.into_memory())
                    .expect("failed to find memory");

                let mut buffer = vec![0u8; str_len as usize];
                memory
                    .read(&caller, str_pointer as usize, &mut buffer)
                    .expect("failed to read memory");
                let label = String::from_utf8_lossy(&buffer).to_string();

                let min = min.clamp(0, 255) as u8;
                let max = max.clamp(0, 255) as u8;
                let value = value.clamp(0, 255) as u8;

                let mut map = state_ref.plugin_ui_ops_back.write().unwrap();
                map.entry(plugin_id as u8)
                    .or_default()
                    .push(WasmUiOp::Slider {
                        label,
                        id: id as u8,
                        min,
                        max,
                        value,
                    });
            },
        )?;

        // Horizontal fader
        let state_ref = Arc::clone(&self.state_ref);
        linker.func_wrap::<_, ()>(
            "blaulicht",
            "ui_hfader",
            move |mut caller: Caller<'_, ()>,
                  plugin_id: i32,
                  str_pointer: i32,
                  str_len: i32,
                  id: i32,
                  min: i32,
                  max: i32,
                  value: i32| {
                let memory = caller
                    .get_export("memory")
                    .and_then(|export| export.into_memory())
                    .expect("failed to find memory");

                let mut buffer = vec![0u8; str_len as usize];
                memory
                    .read(&caller, str_pointer as usize, &mut buffer)
                    .expect("failed to read memory");
                let label = String::from_utf8_lossy(&buffer).to_string();

                let min = min.clamp(0, 255) as u8;
                let max = max.clamp(0, 255) as u8;
                let value = value.clamp(0, 255) as u8;

                let mut map = state_ref.plugin_ui_ops_back.write().unwrap();
                map.entry(plugin_id as u8)
                    .or_default()
                    .push(WasmUiOp::HFader {
                        label,
                        id: id as u8,
                        min,
                        max,
                        value,
                    });
            },
        )?;

        // Text edit
        let state_ref = Arc::clone(&self.state_ref);
        linker.func_wrap::<_, ()>(
            "blaulicht",
            "ui_text_edit",
            move |mut caller: Caller<'_, ()>,
                  plugin_id: i32,
                  label_ptr: i32,
                  label_len: i32,
                  id: i32,
                  text_ptr: i32,
                  text_len: i32| {
                let memory = caller
                    .get_export("memory")
                    .and_then(|export| export.into_memory())
                    .expect("failed to find memory");

                let mut label_buf = vec![0u8; label_len as usize];
                memory
                    .read(&caller, label_ptr as usize, &mut label_buf)
                    .expect("failed to read memory");
                let label = String::from_utf8_lossy(&label_buf).to_string();

                let mut text_buf = vec![0u8; text_len as usize];
                memory
                    .read(&caller, text_ptr as usize, &mut text_buf)
                    .expect("failed to read memory");
                let text = String::from_utf8_lossy(&text_buf).to_string();

                let mut map = state_ref.plugin_ui_ops_back.write().unwrap();
                map.entry(plugin_id as u8)
                    .or_default()
                    .push(WasmUiOp::TextEdit {
                        label,
                        id: id as u8,
                        text,
                    });
            },
        )?;

        // Text edit multiline
        let state_ref = Arc::clone(&self.state_ref);
        linker.func_wrap::<_, ()>(
            "blaulicht",
            "ui_text_edit_multiline",
            move |mut caller: Caller<'_, ()>,
                  plugin_id: i32,
                  label_ptr: i32,
                  label_len: i32,
                  id: i32,
                  text_ptr: i32,
                  text_len: i32| {
                let memory = caller
                    .get_export("memory")
                    .and_then(|export| export.into_memory())
                    .expect("failed to find memory");

                let mut label_buf = vec![0u8; label_len as usize];
                memory
                    .read(&caller, label_ptr as usize, &mut label_buf)
                    .expect("failed to read memory");
                let label = String::from_utf8_lossy(&label_buf).to_string();

                let mut text_buf = vec![0u8; text_len as usize];
                memory
                    .read(&caller, text_ptr as usize, &mut text_buf)
                    .expect("failed to read memory");
                let text = String::from_utf8_lossy(&text_buf).to_string();

                let mut map = state_ref.plugin_ui_ops_back.write().unwrap();
                map.entry(plugin_id as u8)
                    .or_default()
                    .push(WasmUiOp::TextEditMultiline {
                        label,
                        id: id as u8,
                        text,
                    });
            },
        )?;

        // Layout: vertical/horizontal begin/end
        let state_ref = Arc::clone(&self.state_ref);
        linker.func_wrap::<_, ()>("blaulicht", "ui_begin_vertical", move |plugin_id: i32| {
            let mut map = state_ref.plugin_ui_ops_back.write().unwrap();
            map.entry(plugin_id as u8)
                .or_default()
                .push(WasmUiOp::BeginVertical);
        })?;
        let state_ref = Arc::clone(&self.state_ref);
        linker.func_wrap::<_, ()>("blaulicht", "ui_end_vertical", move |plugin_id: i32| {
            let mut map = state_ref.plugin_ui_ops_back.write().unwrap();
            map.entry(plugin_id as u8)
                .or_default()
                .push(WasmUiOp::EndVertical);
        })?;
        let state_ref = Arc::clone(&self.state_ref);
        linker.func_wrap::<_, ()>("blaulicht", "ui_begin_horizontal", move |plugin_id: i32| {
            let mut map = state_ref.plugin_ui_ops_back.write().unwrap();
            map.entry(plugin_id as u8)
                .or_default()
                .push(WasmUiOp::BeginHorizontal);
        })?;
        let state_ref = Arc::clone(&self.state_ref);
        linker.func_wrap::<_, ()>("blaulicht", "ui_end_horizontal", move |plugin_id: i32| {
            let mut map = state_ref.plugin_ui_ops_back.write().unwrap();
            map.entry(plugin_id as u8)
                .or_default()
                .push(WasmUiOp::EndHorizontal);
        })?;

        // Painter: begin/rect/circle/end
        let state_ref = Arc::clone(&self.state_ref);
        linker.func_wrap::<_, ()>(
            "blaulicht",
            "ui_painter_begin",
            move |plugin_id: i32, id: i32, width: i32, height: i32| {
                let mut map = state_ref.plugin_ui_ops_back.write().unwrap();
                map.entry(plugin_id as u8)
                    .or_default()
                    .push(WasmUiOp::PainterBegin {
                        id: id as u8,
                        width,
                        height,
                    });
            },
        )?;

        let state_ref = Arc::clone(&self.state_ref);
        linker.func_wrap::<_, ()>(
            "blaulicht",
            "ui_painter_rect",
            move |plugin_id: i32,
                  x: i32,
                  y: i32,
                  w: i32,
                  h: i32,
                  r: i32,
                  g: i32,
                  b: i32,
                  a: i32| {
                let mut map = state_ref.plugin_ui_ops_back.write().unwrap();
                map.entry(plugin_id as u8)
                    .or_default()
                    .push(WasmUiOp::PainterRect {
                        x,
                        y,
                        w,
                        h,
                        r: r as u8,
                        g: g as u8,
                        b: b as u8,
                        a: a as u8,
                    });
            },
        )?;

        let state_ref = Arc::clone(&self.state_ref);
        linker.func_wrap::<_, ()>(
            "blaulicht",
            "ui_painter_circle",
            move |plugin_id: i32, x: i32, y: i32, radius: i32, r: i32, g: i32, b: i32, a: i32| {
                let mut map = state_ref.plugin_ui_ops_back.write().unwrap();
                map.entry(plugin_id as u8)
                    .or_default()
                    .push(WasmUiOp::PainterCircle {
                        x,
                        y,
                        radius,
                        r: r as u8,
                        g: g as u8,
                        b: b as u8,
                        a: a as u8,
                    });
            },
        )?;

        let state_ref = Arc::clone(&self.state_ref);
        linker.func_wrap::<_, ()>("blaulicht", "ui_painter_end", move |plugin_id: i32| {
            let mut map = state_ref.plugin_ui_ops_back.write().unwrap();
            map.entry(plugin_id as u8)
                .or_default()
                .push(WasmUiOp::PainterEnd);
        })?;

        // Painter line
        let state_ref = Arc::clone(&self.state_ref);
        linker.func_wrap::<_, ()>(
            "blaulicht",
            "ui_painter_line",
            move |plugin_id: i32,
                  x1: i32,
                  y1: i32,
                  x2: i32,
                  y2: i32,
                  r: i32,
                  g: i32,
                  b: i32,
                  a: i32,
                  thickness: i32| {
                let mut map = state_ref.plugin_ui_ops_back.write().unwrap();
                map.entry(plugin_id as u8)
                    .or_default()
                    .push(WasmUiOp::PainterLine {
                        x1,
                        y1,
                        x2,
                        y2,
                        r: r as u8,
                        g: g as u8,
                        b: b as u8,
                        a: a as u8,
                        thickness,
                    });
            },
        )?;

        // Painter text
        let state_ref = Arc::clone(&self.state_ref);
        linker.func_wrap::<_, ()>(
            "blaulicht",
            "ui_painter_text",
            move |mut caller: Caller<'_, ()>,
                  plugin_id: i32,
                  x: i32,
                  y: i32,
                  size: i32,
                  r: i32,
                  g: i32,
                  b: i32,
                  a: i32,
                  ptr: i32,
                  len: i32| {
                let memory = caller
                    .get_export("memory")
                    .and_then(|export| export.into_memory())
                    .expect("failed to find memory");

                let mut buf = vec![0u8; len as usize];
                memory
                    .read(&caller, ptr as usize, &mut buf)
                    .expect("failed to read memory");
                let text = String::from_utf8_lossy(&buf).to_string();

                let mut map = state_ref.plugin_ui_ops_back.write().unwrap();
                map.entry(plugin_id as u8)
                    .or_default()
                    .push(WasmUiOp::PainterText {
                        x,
                        y,
                        size,
                        r: r as u8,
                        g: g as u8,
                        b: b as u8,
                        a: a as u8,
                        text,
                    });
            },
        )?;

        // Painter strokes
        let state_ref = Arc::clone(&self.state_ref);
        linker.func_wrap::<_, ()>(
            "blaulicht",
            "ui_painter_rect_stroke",
            move |plugin_id: i32,
                  x: i32,
                  y: i32,
                  w: i32,
                  h: i32,
                  r: i32,
                  g: i32,
                  b: i32,
                  a: i32,
                  thickness: i32| {
                let mut map = state_ref.plugin_ui_ops_back.write().unwrap();
                map.entry(plugin_id as u8)
                    .or_default()
                    .push(WasmUiOp::PainterRectStroke {
                        x,
                        y,
                        w,
                        h,
                        r: r as u8,
                        g: g as u8,
                        b: b as u8,
                        a: a as u8,
                        thickness,
                    });
            },
        )?;

        let state_ref = Arc::clone(&self.state_ref);
        linker.func_wrap::<_, ()>(
            "blaulicht",
            "ui_painter_circle_stroke",
            move |plugin_id: i32,
                  x: i32,
                  y: i32,
                  radius: i32,
                  r: i32,
                  g: i32,
                  b: i32,
                  a: i32,
                  thickness: i32| {
                let mut map = state_ref.plugin_ui_ops_back.write().unwrap();
                map.entry(plugin_id as u8)
                    .or_default()
                    .push(WasmUiOp::PainterCircleStroke {
                        x,
                        y,
                        radius,
                        r: r as u8,
                        g: g as u8,
                        b: b as u8,
                        a: a as u8,
                        thickness,
                    });
            },
        )?;

        // Cubic bezier
        let state_ref = Arc::clone(&self.state_ref);
        linker.func_wrap::<_, ()>(
            "blaulicht",
            "ui_painter_cubic_bezier",
            move |plugin_id: i32,
                  x1: i32,
                  y1: i32,
                  cx1: i32,
                  cy1: i32,
                  cx2: i32,
                  cy2: i32,
                  x2: i32,
                  y2: i32,
                  r: i32,
                  g: i32,
                  b: i32,
                  a: i32,
                  thickness: i32| {
                let mut map = state_ref.plugin_ui_ops_back.write().unwrap();
                map.entry(plugin_id as u8)
                    .or_default()
                    .push(WasmUiOp::PainterCubicBezier {
                        x1,
                        y1,
                        cx1,
                        cy1,
                        cx2,
                        cy2,
                        x2,
                        y2,
                        r: r as u8,
                        g: g as u8,
                        b: b as u8,
                        a: a as u8,
                        thickness,
                    });
            },
        )?;

        // Color picker
        let state_ref = Arc::clone(&self.state_ref);
        linker.func_wrap::<_, ()>(
            "blaulicht",
            "ui_color_picker",
            move |plugin_id: i32, id: i32, r: i32, g: i32, b: i32, a: i32| {
                let mut map = state_ref.plugin_ui_ops_back.write().unwrap();
                map.entry(plugin_id as u8)
                    .or_default()
                    .push(WasmUiOp::ColorPicker {
                        id: id as u8,
                        r: r as u8,
                        g: g as u8,
                        b: b as u8,
                        a: a as u8,
                    });
            },
        )?;

        // Frames
        let state_ref = Arc::clone(&self.state_ref);
        linker.func_wrap::<_, ()>(
            "blaulicht",
            "ui_begin_frame",
            move |plugin_id: i32, id: i32| {
                let mut map = state_ref.plugin_ui_ops_back.write().unwrap();
                map.entry(plugin_id as u8)
                    .or_default()
                    .push(WasmUiOp::BeginFrame { id: id as u8 });
            },
        )?;
        let state_ref = Arc::clone(&self.state_ref);
        linker.func_wrap::<_, ()>("blaulicht", "ui_end_frame", move |plugin_id: i32| {
            let mut map = state_ref.plugin_ui_ops_back.write().unwrap();
            map.entry(plugin_id as u8)
                .or_default()
                .push(WasmUiOp::EndFrame);
        })?;

        // Frame styled
        let state_ref = Arc::clone(&self.state_ref);
        linker.func_wrap::<_, ()>(
            "blaulicht",
            "ui_begin_frame_styled",
            move |mut caller: Caller<'_, ()>,
                  plugin_id: i32,
                  id: i32,
                  ptr: i32,
                  len: i32,
                  pad_x: i32,
                  pad_y: i32,
                  margin_x: i32,
                  margin_y: i32| {
                let memory = caller
                    .get_export("memory")
                    .and_then(|export| export.into_memory())
                    .expect("failed to find memory");
                let mut buf = vec![0u8; len as usize];
                memory
                    .read(&caller, ptr as usize, &mut buf)
                    .expect("failed to read memory");
                let title = String::from_utf8_lossy(&buf).to_string();
                let mut map = state_ref.plugin_ui_ops_back.write().unwrap();
                map.entry(plugin_id as u8)
                    .or_default()
                    .push(WasmUiOp::BeginFrameStyled {
                        id: id as u8,
                        title,
                        pad_x,
                        pad_y,
                        margin_x,
                        margin_y,
                    });
            },
        )?;

        let state_ref = Arc::clone(&self.state_ref);
        linker.func_wrap::<_, ()>(
            "blaulicht",
            "ui_begin_frame_styled_border",
            move |mut caller: Caller<'_, ()>,
                  plugin_id: i32,
                  id: i32,
                  title_ptr: i32,
                  title_len: i32,
                  pad_x: i32,
                  pad_y: i32,
                  margin_x: i32,
                  margin_y: i32,
                  border_r: i32,
                  border_g: i32,
                  border_b: i32,
                  border_a: i32,
                  border_thickness: i32| {
                let memory = caller
                    .get_export("memory")
                    .and_then(|export| export.into_memory())
                    .expect("failed to find memory");

                let mut buffer = vec![0u8; title_len as usize];
                memory
                    .read(&caller, title_ptr as usize, &mut buffer)
                    .expect("failed to read memory");
                let title = String::from_utf8_lossy(&buffer).to_string();

                let mut map = state_ref.plugin_ui_ops_back.write().unwrap();
                map.entry(plugin_id as u8)
                    .or_default()
                    .push(WasmUiOp::BeginFrameStyledBorder {
                        id: id as u8,
                        title,
                        pad_x,
                        pad_y,
                        margin_x,
                        margin_y,
                        border_r: border_r as u8,
                        border_g: border_g as u8,
                        border_b: border_b as u8,
                        border_a: border_a as u8,
                        border_thickness,
                    });
            },
        )?;

        // Collapsing
        let state_ref = Arc::clone(&self.state_ref);
        linker.func_wrap::<_, ()>(
            "blaulicht",
            "ui_begin_collapsing",
            move |mut caller: Caller<'_, ()>,
                  plugin_id: i32,
                  id: i32,
                  ptr: i32,
                  len: i32,
                  default_open: i32| {
                let memory = caller
                    .get_export("memory")
                    .and_then(|export| export.into_memory())
                    .expect("failed to find memory");
                let mut buf = vec![0u8; len as usize];
                memory
                    .read(&caller, ptr as usize, &mut buf)
                    .expect("failed to read memory");
                let title = String::from_utf8_lossy(&buf).to_string();
                let mut map = state_ref.plugin_ui_ops_back.write().unwrap();
                map.entry(plugin_id as u8)
                    .or_default()
                    .push(WasmUiOp::BeginCollapsing {
                        id: id as u8,
                        title,
                        default_open: default_open != 0,
                    });
            },
        )?;
        let state_ref = Arc::clone(&self.state_ref);
        linker.func_wrap::<_, ()>("blaulicht", "ui_end_collapsing", move |plugin_id: i32| {
            let mut map = state_ref.plugin_ui_ops_back.write().unwrap();
            map.entry(plugin_id as u8)
                .or_default()
                .push(WasmUiOp::EndCollapsing);
        })?;

        // Tabs
        let state_ref = Arc::clone(&self.state_ref);
        linker.func_wrap::<_, ()>(
            "blaulicht",
            "ui_begin_tabs",
            move |plugin_id: i32, id: i32| {
                let mut map = state_ref.plugin_ui_ops_back.write().unwrap();
                map.entry(plugin_id as u8)
                    .or_default()
                    .push(WasmUiOp::BeginTabs { id: id as u8 });
            },
        )?;
        let state_ref = Arc::clone(&self.state_ref);
        linker.func_wrap::<_, ()>(
            "blaulicht",
            "ui_begin_tab",
            move |mut caller: Caller<'_, ()>,
                  plugin_id: i32,
                  tabs_id: i32,
                  tab_id: i32,
                  ptr: i32,
                  len: i32| {
                let memory = caller
                    .get_export("memory")
                    .and_then(|export| export.into_memory())
                    .expect("failed to find memory");
                let mut buf = vec![0u8; len as usize];
                memory
                    .read(&caller, ptr as usize, &mut buf)
                    .expect("failed to read memory");
                let title = String::from_utf8_lossy(&buf).to_string();
                let mut map = state_ref.plugin_ui_ops_back.write().unwrap();
                map.entry(plugin_id as u8)
                    .or_default()
                    .push(WasmUiOp::BeginTab {
                        tabs_id: tabs_id as u8,
                        tab_id: tab_id as u8,
                        title,
                    });
            },
        )?;
        let state_ref = Arc::clone(&self.state_ref);
        linker.func_wrap::<_, ()>("blaulicht", "ui_end_tab", move |plugin_id: i32| {
            let mut map = state_ref.plugin_ui_ops_back.write().unwrap();
            map.entry(plugin_id as u8)
                .or_default()
                .push(WasmUiOp::EndTab);
        })?;
        let state_ref = Arc::clone(&self.state_ref);
        linker.func_wrap::<_, ()>("blaulicht", "ui_end_tabs", move |plugin_id: i32| {
            let mut map = state_ref.plugin_ui_ops_back.write().unwrap();
            map.entry(plugin_id as u8)
                .or_default()
                .push(WasmUiOp::EndTabs);
        })?;

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

        let so = self.system_out.clone();
        linker.func_wrap::<_, ()>(
            "blaulicht",
            "bl_report_panic",
            move |mut caller: Caller<'_, ()>, plugin_id: i32, str_pointer: i32, str_len: i32| {
                let memory = caller
                    .get_export("memory")
                    .and_then(|export| export.into_memory())
                    .expect("failed to find memory");

                let mut buffer = vec![0u8; str_len as usize];
                memory
                    .read(&caller, str_pointer as usize, &mut buffer)
                    .expect("failed to read memory");

                let received_string = String::from_utf8_lossy(&buffer).to_string();

                tracing::debug!("***WASM PANIC***: {received_string}");

                let msg = format!("***PANIC***:\n{received_string}");

                so.send(SystemMessage::WasmLog(WasmLogBody {
                    plugin_id: plugin_id as u8,
                    msg: msg.into(),
                    level: LogLevel::Err,
                }))
                .expect("failed to send log message");
            },
        )?;

        // let mo = self.to_midi_devices.clone();
        // linker.func_wrap::<_, ()>(
        //     "blaulicht",
        //     "bl_transmit_midi_bulk",
        //     move |buf_start: u32, buf_len: u32| {
        //         mo.send(midievent {

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
                    .expect("failed to find memory");

                let mut buffer = vec![0u8; str_len as usize];
                memory
                    .read(&caller, str_pointer as usize, &mut buffer)
                    .expect("failed to read memory");

                let device_name = String::from_utf8_lossy(&buffer).to_string();

                tracing::debug!("open midi device: {}", device_name);

                let mut midi_manager = midi_manager.lock().unwrap();
                midi_manager.request_device(&device_name).unwrap_or(u8::MAX) as u32
            },
        )?;

        let midi_manager = Arc::clone(&self.midi_manager_ref);
        linker.func_wrap::<_, u32>(
            "blaulicht",
            "bl_enumerate_midi_devices",
            move |mut caller: Caller<'_, ()>, buffer_ptr: i32, buffer_len: i32| {
                let midi_manager = midi_manager.lock().unwrap();
                let devices = midi_manager
                    .enumerate_devices()
                    .unwrap_or_else(|_| Vec::new());
                let json = serde_json::to_string(&devices).unwrap_or_else(|_| "[]".to_string());
                let json_bytes = json.as_bytes();

                let memory = caller
                    .get_export("memory")
                    .and_then(|export| export.into_memory())
                    .expect("failed to find memory");

                let write_len = std::cmp::min(json_bytes.len(), buffer_len as usize);

                if write_len > 0 {
                    memory
                        .write(&mut caller, buffer_ptr as usize, &json_bytes[..write_len])
                        .expect("failed to write memory");
                }

                write_len as u32
            },
        )?;

        //
        // Serial interface.
        //

        let udp_manager = Arc::clone(&self.udp_manager_ref);
        linker.func_wrap::<_, u32>(
            "blaulicht",
            "bl_open_udp_port",
            move |bind_port: u32| {
                let mut udp_manager = udp_manager.lock().unwrap();
                udp_manager
                    .request_port(bind_port as u16)
                    .unwrap_or(u8::MAX) as u32
            },
        )?;

        let serial_manager = Arc::clone(&self.serial_manager_ref);
        linker.func_wrap::<_, u32>(
            "blaulicht",
            "bl_open_serial_device",
            move |mut caller: Caller<'_, ()>, str_pointer: i32, str_len: i32, baud_rate: u32| {
                let memory = caller
                    .get_export("memory")
                    .and_then(|export| export.into_memory())
                    .expect("failed to find memory");

                let mut buffer = vec![0u8; str_len as usize];
                memory
                    .read(&caller, str_pointer as usize, &mut buffer)
                    .expect("failed to read memory");

                let device_name = String::from_utf8_lossy(&buffer).to_string();

                tracing::debug!("open serial device: {}", device_name);

                let mut serial_manager = serial_manager.lock().unwrap();
                serial_manager
                    .request_device(&device_name, baud_rate)
                    .unwrap_or(u8::MAX) as u32
            },
        )?;

        let serial_manager = Arc::clone(&self.serial_manager_ref);
        linker.func_wrap::<_, u32>(
            "blaulicht",
            "bl_enumerate_serial_devices",
            move |mut caller: Caller<'_, ()>, buffer_ptr: i32, buffer_len: i32| {
                let serial_manager = serial_manager.lock().unwrap();
                let devices = serial_manager
                    .enumerate_devices()
                    .unwrap_or_else(|_| Vec::new());
                let json = serde_json::to_string(&devices).unwrap_or_else(|_| "[]".to_string());
                let json_bytes = json.as_bytes();

                let memory = caller
                    .get_export("memory")
                    .and_then(|export| export.into_memory())
                    .expect("failed to find memory");

                let write_len = std::cmp::min(json_bytes.len(), buffer_len as usize);

                if write_len > 0 {
                    memory
                        .write(&mut caller, buffer_ptr as usize, &json_bytes[..write_len])
                        .expect("failed to write memory");
                }

                write_len as u32
            },
        )?;

        //
        // End serial interface.
        //

        //
        // Art-Net receiver registration (plugin-owned).
        //

        let state_ref = Arc::clone(&self.state_ref);
        let so = self.system_out.clone();
        linker.func_wrap::<_, u32>(
            "blaulicht",
            "bl_artnet_register_receiver",
            move |mut caller: Caller<'_, ()>,
                  plugin_id: i32,
                  addr_ptr: i32,
                  addr_len: i32|
                  -> u32 {
                let memory = caller
                    .get_export("memory")
                    .and_then(|export| export.into_memory())
                    .expect("failed to find memory");

                let mut buffer = vec![0u8; addr_len as usize];
                if memory
                    .read(&caller, addr_ptr as usize, &mut buffer)
                    .is_err()
                {
                    return 0;
                }

                let addr_str = String::from_utf8_lossy(&buffer).to_string();
                let socket_addr: std::net::SocketAddr = match addr_str.parse() {
                    Ok(a) => a,
                    Err(err) => {
                        let _ = so.send(SystemMessage::Log(
                            format!(
                                "Plugin {plugin_id} tried to register invalid ArtNet \
                                 receiver address '{addr_str}': {err}"
                            ),
                            LogLevel::Warn,
                        ));
                        return 0;
                    }
                };

                if !socket_addr.is_ipv4() {
                    let _ = so.send(SystemMessage::Log(
                        format!(
                            "Plugin {plugin_id} tried to register non-IPv4 ArtNet \
                             receiver '{addr_str}'"
                        ),
                        LogLevel::Warn,
                    ));
                    return 0;
                }

                let mut artnet_output = state_ref.artnet_output.write().unwrap();
                let handle =
                    artnet_output.register_plugin_receiver(plugin_id as u8, socket_addr);
                drop(artnet_output);

                if handle == 0 {
                    let _ = so.send(SystemMessage::Log(
                        format!(
                            "Plugin {plugin_id} could not register ArtNet receiver \
                             '{addr_str}' (address conflict)"
                        ),
                        LogLevel::Warn,
                    ));
                } else {
                    tracing::debug!(
                        "Plugin {plugin_id} registered ArtNet receiver '{addr_str}' (handle={handle})"
                    );
                }

                handle
            },
        )?;

        let state_ref = Arc::clone(&self.state_ref);
        linker.func_wrap::<_, u32>(
            "blaulicht",
            "bl_artnet_unregister_receiver",
            move |plugin_id: i32, handle: u32| -> u32 {
                if handle == 0 {
                    return 0;
                }
                let mut artnet_output = state_ref.artnet_output.write().unwrap();
                let removed =
                    artnet_output.unregister_plugin_receiver(plugin_id as u8, handle);
                drop(artnet_output);

                if removed {
                    tracing::debug!(
                        "Plugin {plugin_id} unregistered ArtNet receiver handle={handle}"
                    );
                    1
                } else {
                    0
                }
            },
        )?;

        let state_ref = Arc::clone(&self.state_ref);
        linker.func_wrap::<_, u32>(
            "blaulicht",
            "bl_artnet_enumerate_receivers",
            move |mut caller: Caller<'_, ()>, buffer_ptr: i32, buffer_len: i32| -> u32 {
                let infos: Vec<ArtNetReceiverInfo> = {
                    let artnet_output = state_ref.artnet_output.read().unwrap();
                    artnet_output
                        .receivers
                        .iter()
                        .map(|r| ArtNetReceiverInfo {
                            address: r.address.to_string(),
                            enabled: r.enabled,
                            owner_plugin_id: r.owner_plugin_id,
                            handle: r.handle,
                        })
                        .collect()
                };

                let json = serde_json::to_string(&infos).unwrap_or_else(|_| "[]".to_string());
                let json_bytes = json.as_bytes();

                let memory = caller
                    .get_export("memory")
                    .and_then(|export| export.into_memory())
                    .expect("failed to find memory");

                let write_len = std::cmp::min(json_bytes.len(), buffer_len as usize);

                if write_len > 0 {
                    memory
                        .write(&mut caller, buffer_ptr as usize, &json_bytes[..write_len])
                        .expect("failed to write memory");
                }

                write_len as u32
            },
        )?;

        //
        // End Art-Net receiver registration.
        //

        let showfile_state_storage = Arc::clone(&self.state_ref.plugin_state_storage);
        let global_state_storage = Arc::clone(&self.state_ref.plugin_state_storage_global);
        let system_out = self.system_out.clone();
        linker.func_wrap::<_, ()>(
            "blaulicht",
            "bl_save_plugin_state",
            move |mut caller: Caller<'_, ()>,
                  plugin_id: i32,
                  location: i32,
                  data_ptr: i32,
                  data_len: i32| {
                let memory = caller
                    .get_export("memory")
                    .and_then(|export| export.into_memory())
                    .expect("failed to find memory");

                let mut buf = vec![0u8; data_len as usize];
                memory
                    .read(&caller, data_ptr as usize, &mut buf)
                    .expect("failed to read memory");

                let state_data = String::from_utf8_lossy(&buf).to_string();
                let plugin_name = format!("plugin_{}", plugin_id);
                let location = PluginStateLocation::try_from(location as u8)
                    .unwrap_or(PluginStateLocation::Showfile);

                tracing::debug!(
                    "Saving plugin state for {}: {} bytes",
                    plugin_name,
                    state_data.len()
                );

                {
                    let storage = match location {
                        PluginStateLocation::Showfile => Arc::clone(&showfile_state_storage),
                        PluginStateLocation::Global => Arc::clone(&global_state_storage),
                    };
                    let mut storage = storage.lock().unwrap();
                    storage.insert(plugin_name.clone(), state_data.clone());
                }

                system_out
                    .send(SystemMessage::SavePluginState {
                        plugin_name,
                        state_data,
                        location,
                    })
                    .unwrap_or_else(|e| {
                        tracing::error!("Failed to send save plugin state message: {}", e);
                    });
            },
        )?;

        let showfile_state_storage = Arc::clone(&self.state_ref.plugin_state_storage);
        let global_state_storage = Arc::clone(&self.state_ref.plugin_state_storage_global);
        linker.func_wrap::<_, u32>(
            "blaulicht",
            "bl_load_plugin_state",
            move |mut caller: Caller<'_, ()>,
                  plugin_id: i32,
                  location: i32,
                  buffer_ptr: i32,
                  buffer_len: i32| {
                let plugin_name = format!("plugin_{}", plugin_id);
                let location = PluginStateLocation::try_from(location as u8)
                    .unwrap_or(PluginStateLocation::Showfile);

                tracing::trace!("Loading plugin state for {}", plugin_name);

                let storage = match location {
                    PluginStateLocation::Showfile => Arc::clone(&showfile_state_storage),
                    PluginStateLocation::Global => Arc::clone(&global_state_storage),
                };
                let storage = storage.lock().unwrap();
                let state_data = storage.get(&plugin_name);

                if let Some(data) = state_data {
                    let memory = caller
                        .get_export("memory")
                        .and_then(|export| export.into_memory())
                        .expect("failed to find memory");

                    let bytes = data.as_bytes();
                    let write_len = bytes.len().min(buffer_len as usize);

                    memory
                        .write(&mut caller, buffer_ptr as usize, &bytes[..write_len])
                        .expect("failed to write memory");

                    tracing::debug!(
                        "Loaded {} bytes of plugin state for {}",
                        write_len,
                        plugin_name
                    );
                    write_len as u32
                } else {
                    tracing::debug!("No saved state found for {}", plugin_name);
                    0u32
                }
            },
        )?;

        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct AddrDescriptor {
    start_addr: i32,
    length_start_addr: i32,
}

impl AddrDescriptor {
    pub fn dummy() -> Self {
        AddrDescriptor {
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

//
// Real wasmtime implementation.
//

#[cfg(feature = "wasmtime")]
impl Plugin {
    fn acquire_midi_buffer_addresses(&mut self) -> anyhow::Result<()> {
        tracing::trace!("Acquiring MIDI buffer addresses for plugin: {}", self.path);
        //
        // Get midi buffer start address.
        //
        let func = self.wasm_state.instance.get_typed_func::<(), i32>(
            &mut self.wasm_state.store,
            "__internal_get_global_midi_buffer_start_addr", // TODO: external type and name constants.
        )?;

        let midi_buffer_start_addr = func.call(&mut self.wasm_state.store, ())?;

        //
        // Get midi buffer length start address.
        //
        let func = self.wasm_state.instance.get_typed_func::<(), i32>(
            &mut self.wasm_state.store,
            "__internal_get_global_midi_buffer_length_start_addr", // TODO: external type and name constants.
        )?;

        let midi_buffer_length_start_addr = func.call(&mut self.wasm_state.store, ())?;

        let addrs = AddrDescriptor {
            start_addr: midi_buffer_start_addr,
            length_start_addr: midi_buffer_length_start_addr,
        };

        tracing::debug!("Acquired MIDI buffer addresses: {:?}", addrs);

        self.midi_buffers = addrs;
        Ok(())
    }

    fn acquire_serial_buffer_addresses(&mut self) -> anyhow::Result<()> {
        tracing::trace!(
            "Acquiring SERIAL buffer addresses for plugin: {}",
            self.path
        );
        //
        // Get serial buffer start address.
        //
        let func = self.wasm_state.instance.get_typed_func::<(), i32>(
            &mut self.wasm_state.store,
            "__internal_get_global_serial_buffer_start_addr", // TODO: external type and name constants.
        )?;

        let serial_buffer_start_addr = func.call(&mut self.wasm_state.store, ())?;

        //
        // Get serial buffer length start address.
        //
        let func = self.wasm_state.instance.get_typed_func::<(), i32>(
            &mut self.wasm_state.store,
            "__internal_get_global_serial_buffer_length_start_addr", // TODO: external type and name constants.
        )?;

        let serial_buffer_length_start_addr = func.call(&mut self.wasm_state.store, ())?;

        let addrs = AddrDescriptor {
            start_addr: serial_buffer_start_addr,
            length_start_addr: serial_buffer_length_start_addr,
        };

        tracing::debug!("Acquired SERIAL buffer addresses: {:?}", addrs);

        self.serial_buffers = addrs;
        Ok(())
    }

    fn acquire_state_buffer_address(&mut self) -> anyhow::Result<()> {
        tracing::trace!("Acquiring state buffer address for plugin: {}", self.path);
        //
        // Get state buffer start address.
        //
        let func = self.wasm_state.instance.get_typed_func::<(), i32>(
            &mut self.wasm_state.store,
            "__internal_get_global_state_buffer_start_addr", // TODO: external type and name constants.
        )?;

        let state_buffer_start_addr = func.call(&mut self.wasm_state.store, ())?;

        //
        // Get state buffer length start address.
        //
        let func = self.wasm_state.instance.get_typed_func::<(), i32>(
            &mut self.wasm_state.store,
            "__internal_get_global_state_buffer_length_start_addr", // TODO: external type and name constants.
        )?;

        let state_buffer_length_start_addr = func.call(&mut self.wasm_state.store, ())?;

        let addrs = AddrDescriptor {
            start_addr: state_buffer_start_addr,
            length_start_addr: state_buffer_length_start_addr,
        };

        tracing::debug!("Acquired State buffer addresses: {:?}", addrs);

        self.state_buffers = addrs;
        Ok(())
    }

    fn acquire_udp_buffer_addresses(&mut self) -> anyhow::Result<()> {
        tracing::trace!("Acquiring UDP buffer addresses for plugin: {}", self.path);

        let func = self.wasm_state.instance.get_typed_func::<(), i32>(
            &mut self.wasm_state.store,
            "__internal_get_global_udp_buffer_start_addr",
        )?;

        let udp_buffer_start_addr = func.call(&mut self.wasm_state.store, ())?;

        let func = self.wasm_state.instance.get_typed_func::<(), i32>(
            &mut self.wasm_state.store,
            "__internal_get_global_udp_buffer_length_start_addr",
        )?;

        let udp_buffer_length_start_addr = func.call(&mut self.wasm_state.store, ())?;

        let addrs = AddrDescriptor {
            start_addr: udp_buffer_start_addr,
            length_start_addr: udp_buffer_length_start_addr,
        };

        tracing::debug!("Acquired UDP buffer addresses: {:?}", addrs);

        self.udp_buffers = addrs;
        Ok(())
    }
}
