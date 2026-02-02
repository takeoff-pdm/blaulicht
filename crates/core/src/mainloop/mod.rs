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
    system_message,
    util::increase_thread_priority,
};
use anyhow::{anyhow, Context};

#[cfg(not(feature = "audio"))]
use blaulicht_audio_engine::noise::AudioSourceNoise;

#[cfg(feature = "audio")]
use blaulicht_audio_engine::audio_source::microphone::AudioSourceMicrophone;

use blaulicht_audio_engine::{
    CollectorOutputSpec, CollectorScratchParameters, SignalCollector, SignalCollectorParams,
    BASS_FRAMES, BASS_PEAK_FRAMES, LONG_HISTORIC_FRAMES, ROLLING_AVERAGE_FRAMES,
    ROLLING_AVERAGE_VOLUME_SAMPLE_SIZE,
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
const SYSTEM_MESSAGE_SPEED: Duration = Duration::from_millis(100);

const AUDIO_SOURCE_FREQ_BUFFER_SIZE: usize = 2048;

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
        config.plugins,
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

    let dmx_appstate = Arc::clone(&app_state);
    let mut dmx_engine = DmxEngine::new(
        dmx_appstate,
        event_bus_dmx,
        system_out.clone(),
        config.dmx_out_devices,
    );

    // {
    //     let mut health_data = app_state.health_data.write().unwrap();
    //     health_data.dmx_universes_healthy.iter_mut().set_from(
    //         dmx_engine.dmx_universe_ports.iter().map(|port| DmxHealth {
    //             is_healthy: ,
    //             error: todo!(),
    //         }),
    //     );
    // }

    // Check for the state of the DMX output.

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
    println!("set spec period: {spec_period:?}");
    {
        let mut spec = app_state.audio_spectrogram.write().unwrap();
        spec.max_columns = desired_columns;
        println!("set spec max columns {desired_columns}");
        println!("set spec bin count {}", spec.bin_count);
        // Trim if we already exceed
        while spec.columns.len() > spec.max_columns {
            spec.columns.pop_front();
            println!("popping front");
        }
    }

    let mut collector_outputs = [CollectorOutputSpec::default(); 2];

    const COLLECTOR_DMX: usize = 0;
    const COLLECTOR_SPECTROGRAM: usize = 1;

    collector_outputs[COLLECTOR_DMX] = CollectorOutputSpec {
        bins_p_column: Some(128), // TODO: maybe this needs some tweaking.
        raw: true,
    };
    collector_outputs[COLLECTOR_SPECTROGRAM] = CollectorOutputSpec {
        bins_p_column: Some(128),
        raw: false,
    };

    #[cfg(feature = "audio")]
    let audio_source = AudioSourceMicrophone::new(
        audioviz::io::Device::Cpal(device),
        config.stream,
        AUDIO_SOURCE_FREQ_BUFFER_SIZE,
    )
    .with_context(|| "Failed to initialize audio stream")?;

    #[cfg(not(feature = "audio"))]
    let audio_source = AudioSourceNoise::new(41100, 100000, AUDIO_SOURCE_FREQ_BUFFER_SIZE);

    let mut sig_collector = SignalCollector::new(
        SignalCollectorParams::default(),
        collector_outputs,
        CollectorScratchParameters {
            volume_frames: ROLLING_AVERAGE_VOLUME_SAMPLE_SIZE,
            long_historic_frames: LONG_HISTORIC_FRAMES,
            rolling_frames: ROLLING_AVERAGE_FRAMES,
            bass_frames: BASS_FRAMES,
            bass_peak_frames: BASS_PEAK_FRAMES,
        },
        audio_source,
        0,
    )
    .with_context(|| "Failed to create signal collector")?;

    //
    // State for the analyzers.
    //

    // Loop speed.
    let mut time_of_last_system_publish = 0;

    let mainloop_begin_time = Instant::now();

    // Dmx last tick.
    let mut time_of_last_dmx_tick = 0;
    let mut last_spectrogram_tick = 0;

    let mut plugin_wasm_engine_crashed = false;

    // Speeds.
    let mut plugin_manager_tick_duration = Duration::default();
    let mut dmx_tick_durations = DmxTickSpeeds::default();

    // Boost the current thread.
    // NOTE: this is done only now since the previous init code could starve the UI thread.
    increase_thread_priority(system_out.clone());

    loop {
        let now = mainloop_begin_time.elapsed().as_millis() as u64;
        let loop_begin_time = now;

        //
        // Loop control.
        //
        let control: AudioThreadControlSignal =
            thread_control_signal.load(Ordering::Relaxed).into();

        match control {
            AudioThreadControlSignal::ABORT => {
                system_out
                    .send(SystemMessage::Log(
                        "[AUDIO] Received kill signal.".into(),
                        LogLevel::Debug,
                    ))
                    .unwrap();

                // signal_mainloop(
                //     Arc::clone(&thread_control_signal),
                //     Arc::clone(&app_state),
                //     AudioThreadControlSignal::ABORTED,
                // );

                break;
            }
            AudioThreadControlSignal::RELOAD => {
                system_out
                    .send(SystemMessage::Log(
                        "[ENGINE] Reload start.".into(),
                        LogLevel::Debug,
                    ))
                    .unwrap();

                match midi_manager.lock() {
                    Ok(mut manager) => manager.reload(),
                    Err(err) => log::warn!(
                        "[ENGINE] Failed to acquire MIDI manager lock during reload: {err}"
                    ),
                }

                plugin_manager.reload()?;

                system_out
                    .send(SystemMessage::Log(
                        "[ENGINE] Reload complete".into(),
                        LogLevel::Info,
                    ))
                    .unwrap();

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

        // Constant tick.
        if now - time_of_last_dmx_tick >= DMX_TICK_TIME.as_millis() as u64 {
            // TODO: does this even work?
            let midi_manager = Arc::clone(&midi_manager);
            let midi = {
                let mut midi_manager = midi_manager.lock().unwrap();
                midi_manager
                    .tick()
                    .map_err(|e| anyhow!("Failed to tick MIDI manager: {e:?}"))?
            };

            let serial_manager = Arc::clone(&serial_manager);
            let serial = {
                let mut serial_manager = serial_manager.lock().unwrap();
                serial_manager
                    .tick()
                    .map_err(|e| anyhow!("Failed to tick serial manager: {e:?}"))?
            };

            // TODO: this is cursed code! - SLOW plugins cause DMX output congestion
            plugin_manager_tick_duration = match plugin_manager.tick(
                sig_collector.take_snapshot(),
                &midi,
                serial,
                Some(Arc::clone(&app_state)),
            ) {
                Ok(dur) => {
                    plugin_wasm_engine_crashed = false; // Reset crash state on successful tick.
                    dur
                }
                Err(err) => {
                    if !plugin_wasm_engine_crashed {
                        log::error!("[Plugin] Wasm engine crash: {err}");
                        plugin_wasm_engine_crashed = true;
                    }
                    Duration::from_micros(0)
                }
            };

            let audio_into_dmx_engine = sig_collector.tick_output::<COLLECTOR_DMX>();
            dmx_tick_durations = dmx_engine.tick(&audio_into_dmx_engine);

            let now = mainloop_begin_time.elapsed().as_millis() as u64;
            time_of_last_dmx_tick = now;
        }

        /////////////////// Signal Begin ///////////////
        let start = Instant::now();
        sig_collector
            .tick(now)
            .with_context(|| "Failed to tick audio input")?;

        if now - last_spectrogram_tick >= spec_period.as_millis() as u64 {
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

            // println!("spec period: {:?}", spec_period);
            let output = sig_collector.tick_output::<COLLECTOR_SPECTROGRAM>();

            app_state
                .audio_spectrogram
                .write()
                .unwrap()
                .push_data(output);

            last_spectrogram_tick = now;
        }

        let audio_processor_speed = start.elapsed();

        //
        // Measure loop speed.
        //
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

    mem::drop(sig_collector);
    Ok(())
}
