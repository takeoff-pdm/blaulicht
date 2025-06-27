pub mod supervisor;
use blaulicht_shared::TickInput;
pub use supervisor::supervisor_thread;

use std::{
    cell::RefCell,
    collections::VecDeque,
    mem,
    rc::Rc,
    sync::{
        atomic::{AtomicU8, Ordering},
        Arc, Mutex,
    },
    time::{self, Duration, Instant},
};

use crossbeam_channel::{Receiver, Sender, TryRecvError};

use anyhow::{anyhow, bail, Context};
use audioviz::spectrum::stream::Stream;
use audioviz::{
    audio_capture::{capture::Capture, config::Config as CaptureConfig},
    spectrum::config::StreamConfig,
};
use cpal::{traits::DeviceTrait, Device};
use log::{debug, info};

use crate::{
    app::MidiEvent,
    audio::{
        analysis::{self, BASS_FRAMES, BASS_PEAK_FRAMES, ROLLING_AVERAGE_LOOP_ITERATIONS},
        capture,
        defs::{AudioConverter, AudioThreadControlSignal},
    },
    config::Config,
    msg::{Signal, SystemMessage},
    plugin::{
        midi::{self, MidiManager},
        PluginManager,
    },
    system_message, util,
};

const SYSTEM_MESSAGE_SPEED: Duration = Duration::from_millis(1000);
pub const SIGNAL_SPEED: Duration = Duration::from_millis(50);

const DMX_TICK_TIME: Duration = Duration::from_millis(25);

pub fn run(
    device: Device,
    signal_out_0: Sender<Signal>,
    system_out: Sender<SystemMessage>,
    thread_control_signal: Arc<AtomicU8>,
    config: Config,
) -> anyhow::Result<()> {
    //
    // MIDI.
    //
    let (to_midi_manager_sender, midi_out_receiver) = crossbeam_channel::bounded(10);
    let (to_plugins_sender, to_plugins_receiver) = crossbeam_channel::bounded(10);
    let midi_manager = Arc::new(Mutex::new(MidiManager::new(
        midi_out_receiver,
        to_plugins_sender,
    )));

    //
    // Plugin system.
    //
    let mut plugin_manager = PluginManager::new(
        config.plugins,
        to_midi_manager_sender,
        to_plugins_receiver,
        system_out.clone(),
        Arc::clone(&midi_manager),
    );

    plugin_manager
        .init()
        .map_err(|e| anyhow!("Failed to init plugin manager: {e}"))?;

    //
    // Audio signal collector.
    //
    let mut collector = capture::SignalCollector::new();
    let (mut converter, capture) = capture::init_converter(device, config.stream)
        .with_context(|| "Failed to initialize audio converter")?;

    //
    // State for the analyzers.
    //

    // Loop speed.
    let mut time_of_last_system_publish = time::Instant::now();
    let mut loop_begin_time = time::Instant::now();

    // Volume.
    let mut time_of_last_volume_publish = time::Instant::now();
    let time_of_last_volume_publish = &mut time_of_last_volume_publish;

    let mut volume_samples: VecDeque<usize> =
        VecDeque::with_capacity(ROLLING_AVERAGE_LOOP_ITERATIONS);

    // Beat
    let mut time_of_last_beat_publish = time::Instant::now();
    let time_of_last_beat_publish = &mut time_of_last_beat_publish;
    let mut last_index = 0;
    let rolling_average_frames = 100;
    let long_historic_frames = rolling_average_frames * 1000;
    let mut long_historic = VecDeque::with_capacity(long_historic_frames);
    let mut historic = VecDeque::with_capacity(rolling_average_frames);

    let mut bass_samples = VecDeque::with_capacity(BASS_FRAMES);
    let mut bass_peaks: VecDeque<Instant> = VecDeque::with_capacity(BASS_PEAK_FRAMES);
    let bass_modifier = 65;

    // Dmx last tick.
    let mut time_of_last_dmx_tick = time::Instant::now();

    let mut plugin_wasm_engine_crashed = false;

    // Boost the current thread.
    util::increase_thread_priority();

    loop {
        //
        // Loop control.
        //
        let control = thread_control_signal.load(Ordering::Relaxed);
        match control {
            AudioThreadControlSignal::ABORT => {
                log::debug!("[AUDIO] Received kill, terminating...");
                thread_control_signal.store(AudioThreadControlSignal::ABORTED, Ordering::Relaxed);
                break;
            }
            AudioThreadControlSignal::RELOAD => {
                system_out
                    .send(SystemMessage::Log("[ENGINE] Reload start.".into()))
                    .unwrap();

                // dmx_universe.reload()?;
                plugin_manager.reload()?;

                system_out
                    .send(SystemMessage::Log("[ENGINE] Reload complete".into()))
                    .unwrap();
                thread_control_signal.store(AudioThreadControlSignal::CONTINUE, Ordering::Relaxed);
            }
            AudioThreadControlSignal::CRASHED | AudioThreadControlSignal::ABORTED => {
                unreachable!("Illegal state: {control}")
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
        if now.duration_since(time_of_last_dmx_tick) > DMX_TICK_TIME {
            // TODO: does this even work?
            let midi_manager = Arc::clone(&midi_manager);
            let mut midi_manager = midi_manager.lock().unwrap();

            let midi = midi_manager
                .tick()
                .map_err(|e| anyhow!("Failed to tick MIDI manager: {e:?}"))?;

            let dmx_tick_duration = match plugin_manager.tick(collector.take_snapshot(), &midi) {
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

            time_of_last_dmx_tick = now;

            system_message!(now, time_of_last_system_publish, system_out, {
                &[
                    SystemMessage::TickSpeed(dmx_tick_duration),
                    SystemMessage::LoopSpeed(loop_speed),
                ]
            });
        }

        /////////////////// Signal Begin ///////////////

        let values = converter.freqs();
        // println!("freqs: {:?}", values);

        //
        // Update volume signal.
        //

        analysis::volume(
            now,
            time_of_last_volume_publish,
            &signal_out_0,
            &mut collector,
            &values,
            &mut volume_samples,
        )?;

        //
        // Update Bass.
        //

        analysis::bass(
            now,
            time_of_last_beat_publish,
            &signal_out_0,
            &mut collector,
            &values,
            &mut bass_samples,
            bass_modifier,
            &mut bass_peaks,
        )?;

        //
        // Update signals.
        //

        analysis::beat_volume(
            &values,
            time_of_last_beat_publish,
            &signal_out_0,
            &mut collector,
            &mut historic,
            &mut long_historic,
            rolling_average_frames,
            long_historic_frames,
            &mut last_index,
        )?;
    }

    mem::drop(capture);
    Ok(())
}
