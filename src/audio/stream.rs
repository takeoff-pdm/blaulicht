use std::{
    collections::VecDeque,
    sync::{
        atomic::{AtomicU8, Ordering},
        Arc,
    },
    time::{self, Duration, Instant},
    u8,
};

use crossbeam_channel::{Receiver, Sender, TryRecvError};

use anyhow::{anyhow, bail};
use audioviz::audio_capture::{capture::Capture, config::Config as CaptureConfig};
use audioviz::spectrum::{stream::Stream, Frequency};
use cpal::{traits::DeviceTrait, Device};
use log::info;

use crate::{
    app::MidiEvent,
    audio::{analysis::{self, BASS_FRAMES, BASS_PEAK_FRAMES}, defs::{AudioConfig, AudioConverter, AudioThreadControlSignal}},
    dmx::DmxUniverse,
    msg::{Signal, SystemMessage},
    shift_push, signal, system_message, util,
};

const ROLLING_AVERAGE_LOOP_ITERATIONS: usize = 100;
const ROLLING_AVERAGE_VOLUME_SAMPLE_SIZE: usize = ROLLING_AVERAGE_LOOP_ITERATIONS / 2;

const SYSTEM_MESSAGE_SPEED: Duration = Duration::from_millis(1000);
pub const SIGNAL_SPEED: Duration = Duration::from_millis(50);

const DMX_TICK_TIME: Duration = Duration::from_millis(25);


pub fn run(
    device: Device,
    signal_out_0: Sender<Signal>,
    system_out: Sender<SystemMessage>,
    thread_control_signal: Arc<AtomicU8>,
    midi_in_receiver: Receiver<MidiEvent>,
    midi_out_sender: Sender<MidiEvent>,
) -> anyhow::Result<()> {
    let config = AudioConfig::default();

    let audio_capture_config = CaptureConfig {
        sample_rate: Some(device.default_input_config().unwrap().sample_rate().0),
        latency: None,
        device: device.name().unwrap(),
        buffer_size: CaptureConfig::default().buffer_size,
        max_buffer_size: CaptureConfig::default().max_buffer_size,
    };

    let capture = Capture::init(audio_capture_config.clone()).map_err(|err| anyhow!("{err:?}"))?;

    let mut converter: AudioConverter = {
        let stream = Stream::init_with_capture(&capture, config.0.clone());
        AudioConverter::from_stream(stream, config.clone())
    };

    //
    //
    //
    //
    //
    //
    //
    // END PREPWORK
    //
    //
    //
    //
    //
    //
    //
    //

    //
    // DMX interfaces.
    //

    info!("[DMX] Trying to establish hardware link...");
    let res = DmxUniverse::new(midi_out_sender.clone(), system_out.clone());
    let mut dmx_universe = match res {
        Ok(universe) => universe,
        Err(e) => {
            println!("[DMX] Failed to establish hardware link: {e}");
            let Ok(universe) = DmxUniverse::new_dummy(midi_out_sender, system_out.clone()) else {
                bail!("[DMX] Failed to create dummy universe, exiting.");
            };

            universe
        }
    };

    // Boost thread.
    util::increase_thread_priority();

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

    //
    // DMX
    //

    let mut time_of_last_dmx_tick = time::Instant::now();

    //
    //
    // TODO: beat detection
    //
    //
    //

    let mut bass_peaks: VecDeque<Instant> = VecDeque::with_capacity(BASS_PEAK_FRAMES);
    let bass_modifier = 65;

    loop {
        //
        // Loop control.
        //
        let control = thread_control_signal.load(Ordering::Relaxed);
        match control {
            AudioThreadControlSignal::ABORT => {
                log::debug!("[AUDIO] Received kill, terminating...");
                thread_control_signal.store(AudioThreadControlSignal::ABORTED, Ordering::Relaxed);
                break Ok(());
            }
            AudioThreadControlSignal::RELOAD => {
                system_out
                    .send(SystemMessage::Log("[ENGINE] Reload start.".into()))
                    .unwrap();

                dmx_universe.reload()?;
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
            // Check for MIDI signals.
            let mut midi = vec![];
            loop {
                match midi_in_receiver.try_recv() {
                    Ok(data) => midi.push(data),
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => break,
                }
            }

            let dmx_tick_duration = match dmx_universe.tick(&midi) {
                Ok(dur) => dur,
                Err(err) => {
                    log::error!("[WASM] Engine crash: {err}");
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

        //
        // Update volume signal.
        //
        {
            signal!(
                now,
                time_of_last_volume_publish,
                signal_out_0,
                dmx_universe,
                {
                    let volume_mean = ((volume_samples.iter().sum::<usize>() as f32)
                        / (volume_samples.len() as f32)
                        * 10.0) as usize;

                    let volume = volume_mean as u8;
                    &[Signal::Volume(volume)]
                }
            );

            shift_push!(
                volume_samples,
                ROLLING_AVERAGE_VOLUME_SAMPLE_SIZE,
                values
                    .iter()
                    .max_by_key(|f| (f.volume * 10.0) as usize)
                    .unwrap_or(&Frequency {
                        volume: 0f32,
                        freq: 0f32,
                        position: 0f32
                    })
                    .volume as usize
            );
        }

        //
        // Update Bass.
        //

        analysis::bass(
            now,
            time_of_last_beat_publish,
            &signal_out_0,
            &mut dmx_universe,
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
            now,
            time_of_last_beat_publish,
            &signal_out_0,
            &mut dmx_universe,
            &mut historic,
            &mut long_historic,
            rolling_average_frames,
            long_historic_frames,
            &mut last_index,
        )?;
    }
}
