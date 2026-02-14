mod audio;
pub mod bg_worker;
pub mod supervisor;

use crate::{
    audio::defs::AudioThreadControlSignal,
    config::Config,
    dmx::DmxEngine,
    event::SystemEventBusConnectionInst,
    mainloop::supervisor::signal_mainloop,
    msg::{AudioDeviceT, DmxTickSpeeds, SystemMessage, TickSpeeds},
    plugin::{midi::MidiManager, serial::SerialManager, PluginManager},
    state::AppState,
    syslog, system_message, util,
};
use anyhow::{anyhow, Context};
use blaulicht_audio_engine::{
    AudioSource, CollectorOutputSpec, CollectorScratchParameters, SignalCollector,
    SignalCollectorParams, BASS_FRAMES, BASS_PEAK_FRAMES, LONG_HISTORIC_FRAMES,
    ROLLING_AVERAGE_FRAMES, ROLLING_AVERAGE_VOLUME_SAMPLE_SIZE,
};
use blaulicht_shared::LogLevel;
use crossbeam_channel::Sender;
use std::{
    mem,
    sync::{
        atomic::{AtomicU8, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};
pub use supervisor::supervisor_thread;

pub const DMX_TICK_TIME: Duration = Duration::from_millis(25);
pub const PLUGINS_TICK_TIME: Duration = Duration::from_millis(12);
const SYSTEM_MESSAGE_SPEED: Duration = Duration::from_millis(100);

pub fn run(
    device: AudioDeviceT,
    system_out: Sender<SystemMessage>,
    thread_control_signal: Arc<AtomicU8>,
    config: Config,
    event_bus_plugins: SystemEventBusConnectionInst,
    event_bus_dmx: SystemEventBusConnectionInst,
    app_state: Arc<AppState>,
) -> anyhow::Result<()> {
    //
    // MIDI.
    //
    let (to_midi_manager_sender, midi_out_receiver) = crossbeam_channel::bounded(1000);
    let (to_plugins_sender, to_plugins_receiver) = crossbeam_channel::bounded(1000); // LIMIT
    let midi_manager = Arc::new(Mutex::new(MidiManager::new(
        midi_out_receiver,
        to_plugins_sender,
        Arc::clone(&app_state),
        system_out.clone(),
    )));

    //
    // Serial.
    //
    let serial_manager = Arc::new(Mutex::new(SerialManager::new(Arc::clone(&app_state))));

    //
    // Plugin system.
    //
    let p_app_state = Arc::clone(&app_state);
    let mut plugin_manager = PluginManager::new(
        config.plugins.clone(),
        to_midi_manager_sender,
        to_plugins_receiver,
        system_out.clone(),
        Arc::clone(&midi_manager),
        Arc::clone(&serial_manager),
        event_bus_plugins,
        p_app_state,
    );

    {
        let plugin_state = app_state.plugin_state_storage.lock().unwrap().clone();
        plugin_manager.load_plugin_states(plugin_state);
    }

    plugin_manager
        .init()
        .map_err(|e| anyhow!("Failed to init plugin manager: {e}"))?;

    //
    // DMX Engine.
    //
    let dmx_appstate = Arc::clone(&app_state);
    let mut dmx_engine = DmxEngine::new(
        dmx_appstate,
        event_bus_dmx,
        system_out.clone(),
        config.dmx_out_devices.clone(),
    );
    // TODO: add a command for starting + stopping setup.
    if config.run_setup_on_reload {
        dmx_engine.start_setup();
    }

    //
    // Audio signal collector.
    //
    // Configure spectrogram window to keep configured seconds based on spectrogram refresh rate.
    // Fall back to at least 1 Hz; default configured to 60 Hz.
    let spec_refresh_hz = config.spectrogram_refresh_hz.max(1) as usize;
    let window_secs = config.spectrogram_window_seconds.max(1) as usize;
    let spec_refresh_hz = 60;
    let window_secs = 10;
    let desired_columns = spec_refresh_hz * window_secs;
    let spec_period = Duration::from_millis((1000f32 / spec_refresh_hz as f32) as u64);
    {
        let mut spec = app_state.audio_spectrogram.write().unwrap();
        spec.max_columns = desired_columns;
        // Trim if we already exceed.
        while spec.columns.len() > spec.max_columns {
            spec.columns.pop_front();
        }
    }

    const COLLECTOR_DMX: usize = 0;
    const COLLECTOR_SPECTROGRAM: usize = 1;

    let mut collector_outputs = [CollectorOutputSpec::default(); 2];
    collector_outputs[COLLECTOR_DMX] = CollectorOutputSpec {
        // TODO: maybe this needs some tweaking.
        // Interesting to think about, probably make this even bigger?
        bins_p_column: Some(128),
        raw: true,
    };
    collector_outputs[COLLECTOR_SPECTROGRAM] = CollectorOutputSpec {
        bins_p_column: Some(128),
        raw: false,
    };

    let audio_source = audio::open_stream(device, config.clone())
        .with_context(|| "Failed to initialize audio stream")?;

    let mut sig_collector = SignalCollector::new(
        SignalCollectorParams::default(),
        collector_outputs,
        CollectorScratchParameters::default(),
        audio_source,
        0,
    )
    .with_context(|| "Failed to create signal collector")?;

    let mut plugin_wasm_engine_crashed = false;

    // Loop speed.
    let mut time_of_last_system_publish = 0;
    let mainloop_begin_time = Instant::now();

    // Ticks.
    let mut dmx_time_of_last_tick = Instant::now();
    let mut plugins_time_of_last_tick = Instant::now();
    let mut spectrogram_time_of_last_tick = Instant::now();

    // Speeds.
    let mut plugin_manager_tick_duration = Duration::default();
    let mut dmx_tick_durations = DmxTickSpeeds::default();

    // Boost the current thread.
    // NOTE: this is done only now since the previous init code could starve the UI thread.
    // (mainly the Wasm setup).
    util::increase_thread_priority(system_out.clone());

    loop {
        let now = mainloop_begin_time.elapsed().as_millis() as u64;
        let loop_begin_time = now;

        // Loop control.
        let control: AudioThreadControlSignal =
            thread_control_signal.load(Ordering::Relaxed).into();

        match control {
            AudioThreadControlSignal::ABORT => {
                syslog!(system_out, "[AUDIO] Received kill signal.");
                break;
            }
            AudioThreadControlSignal::RELOAD => {
                syslog!(system_out, "[ENGINE] Reload start.");
                match midi_manager.lock() {
                    Ok(mut manager) => manager.reload(),
                    // TODO: i think this can never happen.
                    Err(err) => log::warn!(
                        "[ENGINE] Failed to acquire MIDI manager lock during reload: {err}"
                    ),
                }

                plugin_manager.reload()?;

                syslog!(system_out, "[ENGINE] Reload complete");

                signal_mainloop(
                    Arc::clone(&thread_control_signal),
                    Arc::clone(&app_state),
                    AudioThreadControlSignal::CONTINUE,
                );
            }
            AudioThreadControlSignal::CRASHED | AudioThreadControlSignal::ABORTED => {
                unreachable!("Illegal state: {control:?}")
            }
            _ => {}
        }

        if plugins_time_of_last_tick.elapsed() >= PLUGINS_TICK_TIME {
            let midi = {
                let mut midi_manager = midi_manager.lock().unwrap();
                midi_manager
                    .tick()
                    .map_err(|e| anyhow!("Failed to tick MIDI manager: {e:?}"))?
            };

            let serial = {
                let mut serial_manager = serial_manager.lock().unwrap();
                serial_manager
                    .tick()
                    .map_err(|e| anyhow!("Failed to tick serial manager: {e:?}"))?
            };

            // TODO: this is cursed code! - SLOW plugins cause DMX output congestion
            plugin_manager_tick_duration = plugin_manager.tick_external(
                sig_collector.take_snapshot(),
                &midi,
                serial,
                Arc::clone(&app_state),
            );

            plugins_time_of_last_tick = Instant::now();
        }

        // Constant tick.
        if dmx_time_of_last_tick.elapsed() >= DMX_TICK_TIME {
            let audio_into_dmx_engine = sig_collector.tick_output::<COLLECTOR_DMX>();
            dmx_tick_durations = dmx_engine.tick(&audio_into_dmx_engine);
            dmx_time_of_last_tick = Instant::now();
        }

        let start = Instant::now();
        sig_collector
            .tick(now)
            .with_context(|| "Failed to tick audio input")?;

        if spectrogram_time_of_last_tick.elapsed() >= spec_period {
            // deadlock issues here!
            {
                let mut ui_params = app_state.audio_params.write().unwrap();
                if ui_params.changed {
                    sig_collector.params = *ui_params;
                    ui_params.changed = false;
                } else if sig_collector.params.changed {
                    *ui_params = sig_collector.params;
                    sig_collector.params.changed = false;
                }
            }

            let output = sig_collector.tick_output::<COLLECTOR_SPECTROGRAM>();
            app_state
                .audio_spectrogram
                .write()
                .unwrap()
                .push_data(output.clone());

            spectrogram_time_of_last_tick = Instant::now();
        }

        // Measure speeds.
        let audio_processor_speed = start.elapsed();
        let now = mainloop_begin_time.elapsed().as_millis() as u64;
        let speeds = TickSpeeds {
            loop_total: Duration::from_millis(now - loop_begin_time),
            plugins: plugin_manager_tick_duration,
            audio_processing: audio_processor_speed,
            dmx: dmx_tick_durations,
        };

        system_message!(now, time_of_last_system_publish, system_out, {
            &[SystemMessage::TickSpeeds(speeds)]
        });
    }

    Ok(())
}
