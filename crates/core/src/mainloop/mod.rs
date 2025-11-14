pub mod supervisor;
use crate::{
    audio::{
        analysis::{self, BASS_FRAMES, BASS_PEAK_FRAMES, ROLLING_AVERAGE_VOLUME_SAMPLE_SIZE},
        collector,
        defs::AudioThreadControlSignal,
    },
    config::Config,
    dmx::DmxEngine,
    event::SystemEventBusConnectionInst,
    mainloop::supervisor::signal_mainloop,
    msg::{Signal, SystemMessage},
    plugin::{midi::MidiManager, serial::SerialManager, PluginManager},
    state::AppState,
    system_message,
};
use anyhow::{anyhow, Context};
use audioviz::spectrum::Frequency;
use blaulicht_shared::LogLevel;
use core::{f32, num};
use cpal::Device;
use crossbeam_channel::Sender;
use itertools::Itertools;
use std::{
    collections::VecDeque,
    mem,
    sync::{
        atomic::{AtomicU8, Ordering},
        Arc, Mutex,
    },
    thread,
    time::{self, Duration, Instant},
};
pub use supervisor::supervisor_thread;

pub const DMX_TICK_TIME: Duration = Duration::from_millis(25);
const SYSTEM_MESSAGE_SPEED: Duration = Duration::from_millis(1000);
pub const SIGNAL_SPEED: Duration = Duration::from_millis(50);

pub fn run(
    device: Device,
    signal_out_0: Sender<Signal>,
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
    )));
    let serial_manager = Arc::new(Mutex::new(SerialManager::new()));

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

    let plugin_state = app_state.plugin_state_storage.lock().unwrap().clone();
    plugin_manager.load_plugin_states(plugin_state);

    thread::sleep(Duration::from_secs(2)); // TODO: hack

    plugin_manager
        .init()
        .map_err(|e| anyhow!("Failed to init plugin manager: {e}"))?;

    let dmx_appstate = Arc::clone(&app_state);
    let mut dmx_engine = DmxEngine::new(dmx_appstate, event_bus_dmx, system_out.clone());

    // dmx_engine.start_setup();

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
    {
        let mut spec = app_state.audio_spectrogram.write().unwrap();
        spec.max_columns = desired_columns;
        // Trim if we already exceed
        while spec.columns.len() > spec.max_columns {
            spec.columns.pop_front();
            println!("popping front");
        }
    }
    let mut last_spec_push = Instant::now();
    let spec_period = Duration::from_millis((1000usize / spec_refresh_hz) as u64);

    let mut sig_collector = collector::SignalCollector::default();

    //
    // State for the analyzers.
    //

    // Loop speed.
    let mut time_of_last_system_publish = time::Instant::now();
    let mut loop_begin_time = time::Instant::now();

    // // Volume.
    // let mut time_of_last_volume_publish = time::Instant::now();
    // let time_of_last_volume_publish = &mut time_of_last_volume_publish;
    //
    // let mut volume_samples: VecDeque<usize> =
    //     VecDeque::with_capacity(ROLLING_AVERAGE_VOLUME_SAMPLE_SIZE);
    //
    // // Beat
    // let mut time_of_last_beat_publish = time::Instant::now();
    // let time_of_last_beat_publish = &mut time_of_last_beat_publish;
    // let mut last_index = 0;
    // let rolling_average_frames = 100;
    // let long_historic_frames = rolling_average_frames * 1000;
    // let mut long_historic = VecDeque::with_capacity(long_historic_frames);
    // let mut historic = VecDeque::with_capacity(rolling_average_frames);
    //
    // let mut bass_samples = VecDeque::with_capacity(BASS_FRAMES);
    // let mut bass_peaks: VecDeque<Instant> = VecDeque::with_capacity(BASS_PEAK_FRAMES);
    // let bass_modifier = 65;
    //
    // let mut time_of_last_bpm_marker = Instant::now();
    // let mut is_on_beat = false;
    // let mut num_beat_mismatches = 0;
    // let mut beat_needs_sync = true;
    // let mut is_on_beat_memo = 0;

    // Dmx last tick.
    let mut time_of_last_dmx_tick = time::Instant::now();

    let mut plugin_wasm_engine_crashed = false;

    // Boost the current thread.

    // TODO: enable again.
    // util::increase_thread_priority();

    loop {
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

                signal_mainloop(
                    Arc::clone(&thread_control_signal),
                    Arc::clone(&app_state),
                    AudioThreadControlSignal::ABORTED,
                );

                break;
            }
            AudioThreadControlSignal::RELOAD => {
                system_out
                    .send(SystemMessage::Log(
                        "[ENGINE] Reload start.".into(),
                        LogLevel::Debug,
                    ))
                    .unwrap();

                // dmx_universe.reload()?;
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

        //
        // Measure loop speed.
        //
        let now = time::Instant::now();
        let loop_speed = now - loop_begin_time;
        loop_begin_time = now;

        // Constant tick.
        if now.duration_since(time_of_last_dmx_tick) >= DMX_TICK_TIME {
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

            //
            // Collect control events.
            //

            let dmx_tick_duration = match plugin_manager.tick(
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

            // Update the collector one last time to include the beat trigger.
            if is_on_beat_memo == 2 {
                sig_collector.signal(Signal::BeatTrigger(true));
                is_on_beat_memo -= 1;
            }

            // TODO: maybe feed with audio signals.
            dmx_engine.tick(sig_collector.take_snapshot());

            time_of_last_dmx_tick = now;

            system_message!(now, time_of_last_system_publish, system_out, {
                &[
                    SystemMessage::TickSpeed(dmx_tick_duration),
                    SystemMessage::LoopSpeed(loop_speed),
                ]
            });
        }

        /////////////////// Signal Begin ///////////////

        {
            let mut audio_sig = app_state.audio_snapshot.write().unwrap();
            *audio_sig = sig_collector.take_snapshot();
        }

        // println!("freqs: {:?}", values);
    }

    mem::drop(sig_collector);
    Ok(())
}
