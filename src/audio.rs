use std::{
    collections::VecDeque,
    net::UdpSocket,
    sync::{
        atomic::{AtomicU8, Ordering},
        Arc,
    },
    thread,
    time::{self, Duration, Instant},
    u8,
};

use crossbeam_channel::{Sender, TryRecvError};

use actix::System;
use anyhow::anyhow;
use audioviz::audio_capture::{capture::Capture, config::Config as CaptureConfig};
use audioviz::{
    audio_capture::capture::CaptureReceiver,
    spectrum::{
        stream::{Stream, StreamController},
        Frequency,
    },
};
use cpal::{traits::DeviceTrait, Device, HostId};
use itertools::Itertools;
use log::{debug, info, warn};
use serde::Serialize;
use serialport::{SerialPortInfo, SerialPortType};

use crate::{
    dmx::{DmxUniverse, USB_DEVICES},
    midi, DmxData, ToFrontent,
};

fn map(x: isize, in_min: isize, in_max: isize, out_min: isize, out_max: isize) -> usize {
    let divisor = (in_max - in_min).max(1);
    ((x - in_min) * (out_max - out_min) / (divisor) + out_min).max(0) as usize
}

pub enum ConverterType {
    Stream(Stream),
    Capture(Capture),
}

pub struct Converter {
    conv_type: ConverterType,
    raw_buf: Vec<f32>,
    show_vec: Vec<f32>,
    pub raw_receiver: Option<CaptureReceiver>,
    pub stream_controller: Option<StreamController>,
    pub config: Config,
    pub resolution: usize,
}

#[derive(Debug, Clone)]
pub enum Visualisation {
    Spectrum,
    Scope,
}

#[derive(Debug, Clone)]
pub struct Config {
    pub audio: audioviz::spectrum::config::StreamConfig,
    pub mirror_x_achsis: bool,
    pub fps: u64,
    pub width: u8,
    pub spacing: u8,
    pub mirror: bool,
    pub visualisation: Visualisation,
}
impl Default for Config {
    fn default() -> Self {
        Config {
            audio: audioviz::spectrum::config::StreamConfig {
                gravity: Some(100.0),
                ..Default::default()
            },
            mirror_x_achsis: true,
            // fps: 60,
            fps: 1,
            width: 1,
            spacing: 0,
            mirror: true,
            visualisation: Visualisation::Spectrum,
        }
    }
}

impl Converter {
    pub fn from_capture(capture: Capture, config: Config) -> Self {
        let raw_receiver = capture.get_receiver().unwrap();
        Self {
            conv_type: ConverterType::Capture(capture),
            raw_buf: Vec::new(),
            show_vec: Vec::new(),
            raw_receiver: Some(raw_receiver),
            stream_controller: None,
            config,
            resolution: 0,
        }
    }

    pub fn from_stream(stream: Stream, config: Config) -> Self {
        let stream_controller = stream.get_controller();
        Self {
            conv_type: ConverterType::Stream(stream),
            raw_buf: Vec::new(),
            show_vec: Vec::new(),
            raw_receiver: None,
            stream_controller: Some(stream_controller),
            config,
            resolution: 0,
        }
    }

    pub fn get_data(&mut self) -> Option<Vec<f32>> {
        if let Some(raw) = &self.raw_receiver {
            let mut data: Vec<f32> = match raw.receive_data() {
                Ok(d) => {
                    let mut b: Vec<f32> = Vec::new();

                    let bufs = d.chunks(1);
                    for buf in bufs {
                        let mut max: f32 = 0.0;
                        for value in buf {
                            let value = value * 30.0 * self.config.audio.processor.volume;
                            if value > max {
                                max = value
                            }
                        }
                        b.push(max)
                    }
                    b
                }
                Err(_) => Vec::new(),
            };
            self.raw_buf.append(&mut data);
            if self.raw_buf.len() >= self.resolution {
                self.show_vec = self.raw_buf[0..self.resolution].to_vec();
                self.raw_buf.drain(..);
            }
            return Some(self.show_vec.clone());
        }
        if let Some(stream) = &self.stream_controller {
            let freqs = stream.get_frequencies();

            let data: Vec<f32> = freqs.into_iter().map(|x| x.volume).collect();

            return Some(data);
        }
        None
    }

    pub fn freqs(&mut self) -> Vec<Frequency> {
        if let Some(stream) = &self.stream_controller {
            let freqs = stream.get_frequencies();
            return freqs;
        }

        panic!("broken");
    }
}

#[derive(Clone, Serialize, Debug)]
pub enum Signal {
    Bpm(u8),
    BeatVolume(u8),
    Bass(u8),
    BassAvg(u8),
    Volume(u8),
}

#[derive(Clone)]
pub enum SystemMessage {
    Heartbeat(usize),
    Log(String),
    LoopSpeed(Duration),
    TickSpeed(Duration),
    // Audio.
    AudioSelected(Option<Device>),
    AudioDevicesView(Vec<(HostId, Device)>),
    // Serial.
    SerialSelected(Option<SerialPortInfo>),
    SerialDevicesView(Vec<SerialPortInfo>),
    // DMX.
    DMX([u8; 513]),
}

const ROLLING_AVERAGE_LOOP_ITERATIONS: usize = 100;
const ROLLING_AVERAGE_VOLUME_SAMPLE_SIZE: usize = ROLLING_AVERAGE_LOOP_ITERATIONS / 2;

const SYSTEM_MESSAGE_SPEED: Duration = Duration::from_millis(1000);
const SIGNAL_SPEED: Duration = Duration::from_millis(50);

const DMX_TICK_TIME: Duration = Duration::from_millis(25);

macro_rules! system_message {
    ($now:ident,$last_publish:ident,$system_out:ident,$tx_signal:expr) => {
        if $now - $last_publish > SYSTEM_MESSAGE_SPEED {
            for signal in $tx_signal {
                $system_out.send(signal.clone()).unwrap();
            }
            $last_publish = $now
        }
    };
}

macro_rules! signal {
    ($now:ident,$last_publish:ident,$out0:ident,$dmx:ident,$tx_signal:expr) => {
        if $now - $last_publish > SIGNAL_SPEED {
            for signal in $tx_signal {
                $out0.send(signal.clone()).unwrap();
            }
            $last_publish = $now;
        }

        for signal in $tx_signal {
            $dmx.signal(signal.clone());
        }
    };
}

///
///
/// Vector push operations.
///
///

macro_rules! shift_push {
    ($vector:ident,$capacity:ident,$item:expr) => {
        $vector.push_back($item);
        if $vector.len() > $capacity {
            $vector.pop_front();
        }
    };
}

#[non_exhaustive]
pub struct AudioThreadControlSignal;

impl AudioThreadControlSignal {
    pub const CONTINUE: u8 = 0;
    pub const ABORT: u8 = 1;
    pub const DEAD: u8 = 2;
    pub const RELOAD: u8 = 3;
}

pub fn run(
    device: Device,
    signal_out_0: Sender<Signal>,
    // signal_out_1: Sender<Signal>,
    system_out: Sender<SystemMessage>,
    thread_control_signal: Arc<AtomicU8>,
    // to_frontend_sender: Sender<ToFrontent>,
) -> anyhow::Result<()> {
    let config = Config::default();

    // TODO: select a device!

    let audio_capture_config = CaptureConfig {
        sample_rate: Some(device.default_input_config().unwrap().sample_rate().0),
        latency: None,
        device: device.name().unwrap(),
        buffer_size: CaptureConfig::default().buffer_size,
        max_buffer_size: CaptureConfig::default().max_buffer_size,
    };

    let capture = Capture::init(audio_capture_config.clone()).map_err(|err| anyhow!("{err:?}"))?;

    let mut converter: Converter = match config.visualisation {
        Visualisation::Spectrum => {
            let stream = Stream::init_with_capture(&capture, config.audio.clone());

            Converter::from_stream(stream, config.clone())
        }
        Visualisation::Scope => Converter::from_capture(capture, config.clone()),
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
    let ports = serialport::available_ports()?;

    // Update available ports to frontend.
    system_out
        .send(SystemMessage::SerialDevicesView(ports.clone()))
        .unwrap();

    println!("{ports:?}");

    let serial_port = ports.into_iter().find(|p| match p.port_type.clone() {
        SerialPortType::UsbPort(usb) => USB_DEVICES
            .iter()
            .any(|d| d.pid == usb.pid && d.vid == usb.vid),
        _ => false,
    });

    if serial_port.is_none() {
        warn!("No default DMX serial output available");
    }

    println!("Creating midi...");
    let (midi_in_sender, midi_in_receiver) = crossbeam_channel::bounded(100);
    let (midi_out_sender, midi_out_receiver) = crossbeam_channel::bounded(10);

    thread::spawn(move || {
        loop {
            match midi::midi(midi_in_sender.clone(), midi_out_receiver.clone()) {
                Ok(_) => panic!("Unreachable."),
                Err(err) => { 
                    eprintln!("MIDI thread chrashed! {err:?}");
                    break;
                },
            }
        }

        loop {
            thread::sleep(Duration::from_millis(50));
            match midi_out_receiver.try_recv() {
                Ok(midi) => { println!("MIDI recv: {midi:?}") },
                Err(TryRecvError::Empty) => {},
                Err(TryRecvError::Disconnected) => panic!("MIDI dender gone."),
            }
        }
    });

    let mut dmx_universe = match serial_port {
        Some(port) => {
            let serial_port_name = port.port_name.clone();
            info!("[DMX] Using serial device: {serial_port_name}");
            system_out
                .send(SystemMessage::SerialSelected(Some(port.clone())))
                .unwrap();
            DmxUniverse::new(serial_port_name, signal_out_0.clone(), midi_out_sender, system_out.clone())
        }
        None => DmxUniverse::new_dummy(
            midi_out_sender,
            system_out.clone(),
        ),
    };

    // Energy saving.
    let mut loop_inactive = true;

    // Loop speed.
    let mut time_of_last_system_publish = time::Instant::now();
    let mut loop_begin_time = time::Instant::now();

    // Volume.
    let mut time_of_last_volume_publish = time::Instant::now();
    let mut volume_samples: VecDeque<usize> =
        VecDeque::with_capacity(ROLLING_AVERAGE_LOOP_ITERATIONS);

    // Beat
    let mut time_of_last_beat_publish = time::Instant::now();
    let mut last_index = 0;
    let rolling_average_frames = 100;
    let long_historic_frames = rolling_average_frames * 1000;
    let mut long_historic = VecDeque::with_capacity(long_historic_frames);
    let mut historic = VecDeque::with_capacity(rolling_average_frames);

    const BASS_FRAMES: usize = 8000; // 800
    let mut bass_samples = VecDeque::with_capacity(BASS_FRAMES);
    let mut last_bass_udp_update = Instant::now();

    const PEAK_FRAMES: usize = 300;
    let mut bass_peaks: VecDeque<Instant> = VecDeque::with_capacity(PEAK_FRAMES);

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
    // let socket = UdpSocket::bind("0.0.0.0:0").unwrap();

    loop {
        //
        // Loop control.
        //
        let control = thread_control_signal.load(Ordering::Relaxed);
        match control {
            AudioThreadControlSignal::ABORT => {
                println!("Received kill, giving up...");
                break Ok(());
            }
            AudioThreadControlSignal::RELOAD => {
                dmx_universe.reload().unwrap();
                system_out
                    .send(SystemMessage::Log("Reloaded DMX engine".into()))
                    .unwrap();
                thread_control_signal.store(AudioThreadControlSignal::CONTINUE, Ordering::Relaxed);
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
                    eprintln!("[WASM] Engine crash: {err}");
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

                    let volume = (volume_mean as u8);
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

        signal!(
            now,
            time_of_last_beat_publish,
            signal_out_0,
            dmx_universe,
            {
                let v = values
                    .iter()
                    .filter(|f| f.freq <= 150.0)
                    .map(|f| f.volume as usize)
                    .collect::<Vec<usize>>();

                let avg = v.iter().sum::<usize>() as f32 / v.len() as f32;
                let sig = (avg * 100.0) as u8;

                bass_samples.push_back(sig);

                if bass_samples.len() >= BASS_FRAMES {
                    bass_samples.pop_front();
                }

                // Drop detection.
                // let len = bass_samples.len();
                // let drop = bass_samples.iter().filter(|b| **b > 20).count() >= len / 3;

                // if last_bass_udp_update.elapsed().as_millis() >= 1000 {
                //     last_bass_udp_update = Instant::now();
                //     println!("drop={drop}");
                //     socket
                //         .send_to(
                //             &[b'D', if drop { b'1' } else { b'0' }],
                //             "192.168.0.100:33333",
                //         )
                //         .unwrap_or_else(|err| {
                //             println!("error UDP: {:?}", err);
                //             0
                //         });
                // }

                let elapsed = match bass_peaks.iter().last() {
                    Some(last) => last.elapsed().as_millis(),
                    None => 10000,
                };

                // let hypothetical_bpm = 60.0 / (elapsed as f64);

                if sig == 255 {
                    if elapsed > 300 {
                        // println!("bass peak.");
                        bass_peaks.push_back(Instant::now());
                    } else {
                        // println!("bass peak stop");
                    }
                }

                if bass_peaks.len() >= PEAK_FRAMES {
                    bass_peaks.pop_front();
                }

                // TODO: Macro for filter logic

                const SECONDS_IN_A_MINUTE: f64 = 60.0;
                const MINIMUM_BPM: f64 = 90.0;
                const MAXIMUM_BPM: f64 = 200.0;
                const MAX_BPM_TIME_BETWEEN_SECS: f64 = SECONDS_IN_A_MINUTE / MINIMUM_BPM;
                const MIN_BPM_TIME_BETWEEN_SECS: f64 = SECONDS_IN_A_MINUTE / MAXIMUM_BPM;

                let bass_peak_durations = bass_peaks.iter().tuple_windows().filter_map(|(a, b)| {
                    let d = (b.duration_since(*a).as_millis() as f64) / 1000.0;
                    if d > MIN_BPM_TIME_BETWEEN_SECS && d < MAX_BPM_TIME_BETWEEN_SECS {
                        Some(d)
                    } else {
                        None
                    }
                });

                let bass_len = bass_peak_durations
                    .clone()
                    .filter(|v| *v > MIN_BPM_TIME_BETWEEN_SECS && *v < MAX_BPM_TIME_BETWEEN_SECS)
                    .count();
                // let bass_len = bass_peak_durations.len();

                let avg_bass_peak_durations =
                    (bass_peak_durations.sum::<f64>() / (bass_len as f64));

                let bass_moving_average =
                    bass_samples.iter().map(|v| *v as f64).sum::<f64>() / BASS_FRAMES as f64;

                let bpm = if bass_moving_average <= 10.0 {
                    0.0
                } else {
                    SECONDS_IN_A_MINUTE / avg_bass_peak_durations
                };

                // println!("bpm={bpm}, avg_bass_dur={avg_bass_peak_durations} seconds, bass_moving_avg={bass_moving_average}");
                // if !bass_peak_durations.is_empty() {
                //     println!("bpm={:?}", bass_peak_durations);
                // }

                &[
                    Signal::Bass(sig),
                    Signal::Bpm(bpm as u8),
                    Signal::BassAvg(bass_moving_average as u8),
                ]
            }
        );

        //
        // Update loudest signal.
        //

        {
            let curr: Vec<usize> = values
                .chunks(2)
                // TODO: only look at the base line?
                .map(|f| f.iter().map(|e| e.volume as usize).max().unwrap())
                .collect();

            let curr = curr.iter().max().unwrap_or(&0);

            let curr_unfiltered: usize = values.iter().map(|f| f.volume as usize).sum();

            long_historic.push_back(curr_unfiltered);
            if long_historic.len() >= long_historic_frames {
                long_historic.pop_front();
            }

            historic.push_back(*curr);

            if historic.len() >= rolling_average_frames {
                historic.pop_front();
            }

            let sum = historic.iter().sum::<usize>();
            let avg = sum / rolling_average_frames;
            let max = historic.iter().max().unwrap_or(&usize::MAX);
            let min = historic.iter().min().unwrap_or(&usize::MIN);

            // let long_sum = long_historic.iter().sum::<usize>();
            // if long_sum == 0 {
            //     if !loop_inactive {
            //         eprintln!("long historic is 0: sleeping");
            //     }

            //     debug!("[AUDIO] Entering sleep mode...");
            //     thread::sleep(DMX_TICK_TIME);
            //     loop_inactive = true;

            //     let mut midi = vec![];
            //     loop {
            //         match midi_in_receiver.try_recv() {
            //             Ok(data) => midi.push(data),
            //             Err(TryRecvError::Empty) => break,
            //             Err(TryRecvError::Disconnected) => panic!("error"),
            //         }
            //     }

            //     // if !midi.is_empty() {
            //     //     println!("MIDI MSGS: {}", midi.len());
            //     // }

            //     dmx_universe.tick(&midi);
            // } else if loop_inactive {
            //     eprintln!("long = {long_sum}");
            //     loop_inactive = false
            // }

            const MAX_BEAT_VOLUME: u8 = 255;
            let index_mapped = map(
                *curr as isize,
                *min as isize,
                *max as isize,
                0,
                MAX_BEAT_VOLUME as isize,
            );

            if last_index != index_mapped {
                let now = time::Instant::now();
                signal!(
                    now,
                    time_of_last_beat_publish,
                    signal_out_0,
                    dmx_universe,
                    {
                        //         eprintln!(
                        //     "index = {index_mapped:02} | curr = {curr:03} | min = {min:03} | avg = {avg:03} | max = {max:03}",
                        // );

                        last_index = index_mapped;

                        &[Signal::BeatVolume(index_mapped as u8)]
                    }
                );
            }
        }
    }
}
