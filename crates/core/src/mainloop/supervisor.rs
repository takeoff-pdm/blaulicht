use anyhow::bail;
use crossbeam_channel::{Receiver, Sender, TryRecvError};
use enttecopendmx::EnttecOpenDMX;
use std::{
    sync::{
        atomic::{AtomicU8, Ordering},
        Arc,
    },
    thread,
    time::{Duration, Instant},
};

use crate::{
    app::MidiEvent, audio::defs::AudioThreadControlSignal, config::Config, mainloop, msg::{Signal, SystemMessage}, plugin::midi
};

use cpal::{traits::DeviceTrait, Device};
use log::{debug, info, warn, Log};

use crate::{
    app::FromFrontend,
    audio::{self},
    utils,
};

// pub struct DmxUniverseBasic {
//     tick_engine: TickEngine,
//     channels: [u8; 513],
//     tick_input: TickInput,
//     system_out: Sender<SystemMessage>,
// }

// impl DmxUniverseBasic {
//     fn new(
//         midi_out: Sender<MidiEvent>,
//         system_out: Sender<SystemMessage>,
//     ) -> wasmtime::Result<Self> {
//         let tick_engine = wasm::TickEngine::create(midi_out, system_out.clone())?;

//         Ok(Self {
//             tick_engine,
//             channels: [0; 513],
//             tick_input: CollectedAudioSnapshot::default(),
//             system_out,
//         })
//     }

//     fn signal(&mut self, signal: Signal) {
//         match signal {
//             Signal::Volume(v) => {
//                 self.tick_input.volume = v;
//             }
//             Signal::BeatVolume(v) => {
//                 self.tick_input.beat_volume = v;
//             }
//             Signal::Bass(v) => {
//                 self.tick_input.bass = v;
//             }
//             Signal::BassAvgShort(v) => {
//                 self.tick_input.bass_avg_short = v;
//             }
//             Signal::BassAvg(v) => {
//                 self.tick_input.bass_avg = v;
//             }
//             Signal::Bpm(v) => {
//                 self.tick_input.bpm = v.bpm;
//                 self.tick_input.time_between_beats_millis = v.time_between_beats_millis;
//             }
//         }
//     }

//     fn tick(&mut self, midi: &[MidiEvent]) -> anyhow::Result<Duration> {
//         let start = Instant::now();
//         self.tick_engine.tick(self.tick_input, midi, false)?;

//         for (index, value) in self.tick_engine.dmx().iter().enumerate() {
//             self.channels[index] = *value;
//         }

//         let elapsed = Instant::now().duration_since(start);
//         Ok(elapsed)
//     }

//     fn reload(&mut self) -> wasmtime::Result<()> {
//         self.tick_engine.reload()
//     }
// }

// pub struct DmxUniverseDummy {
//     basic: DmxUniverseBasic,
//     last_state: [u8; 513],
// }

// pub enum DmxUniverse {
//     Dummy(DmxUniverseDummy),
//     Real(DmxUniverseReal),
// }

// impl DmxUniverse {
//     pub fn new(
//         midi_out: Sender<MidiEvent>,
//         system_out: Sender<SystemMessage>,
//     ) -> anyhow::Result<Self> {
//         let base = DmxUniverseBasic::new(midi_out, system_out)?;
//         let real_universe = DmxUniverseReal::new(base)?;
//         Ok(Self::Real(real_universe))
//     }

//     pub fn new_dummy(
//         midi_out: Sender<MidiEvent>,
//         system_out: Sender<SystemMessage>,
//     ) -> wasmtime::Result<Self> {
//         let base = DmxUniverseBasic::new(midi_out, system_out)?;
//         Ok(Self::Dummy(DmxUniverseDummy {
//             basic: base,
//             last_state: [0; 513],
//         }))
//     }

//     pub fn signal(&mut self, signal: Signal) {
//         match self {
//             DmxUniverse::Dummy(dummy) => dummy.basic.signal(signal),
//             DmxUniverse::Real(dmx_universe_real) => dmx_universe_real.signal(signal),
//         }
//     }

//     pub fn tick(&mut self, midi: &[MidiEvent]) -> anyhow::Result<Duration> {
//         match self {
//             DmxUniverse::Dummy(ref mut dummy) => {
//                 let dur = dummy.basic.tick(midi)?;

//                 let mut modified = false;
//                 for (a, b) in dummy.basic.channels.iter().zip(dummy.last_state.iter()) {
//                     if a != b {
//                         modified = true;
//                         break;
//                     }
//                 }

//                 if modified {
//                     dummy
//                         .basic
//                         .system_out
//                         .send(SystemMessage::DMX(dummy.basic.channels.into()))
//                         .unwrap();

//                     dummy.last_state = dummy.basic.channels;
//                 }

//                 Ok(dur)
//             }
//             DmxUniverse::Real(dmx_universe_real) => dmx_universe_real.tick(midi),
//         }
//     }

//     pub fn reload(&mut self) -> wasmtime::Result<()> {
//         match self {
//             DmxUniverse::Dummy(dummy) => dummy.basic.reload(),
//             DmxUniverse::Real(dmx_universe_real) => dmx_universe_real.reload(),
//         }
//     }
// }

// pub struct DmxUniverseReal {
//     pub dmx: EnttecOpenDMX,
//     pub base: DmxUniverseBasic,
// }

// impl DmxUniverseReal {
//     fn new(base: DmxUniverseBasic) -> anyhow::Result<Self> {
//         let mut interface = enttecopendmx::EnttecOpenDMX::new()?;
//         interface.open().unwrap();

//         let this = Self {
//             dmx: interface,
//             base,
//         };

//         Ok(this)
//     }

//     fn reload(&mut self) -> wasmtime::Result<()> {
//         self.base.reload()
//     }

//     fn signal(&mut self, signal: Signal) {
//         self.base.signal(signal)
//     }

//     pub fn tick(&mut self, midi: &[MidiEvent]) -> anyhow::Result<Duration> {
//         let duration = self.base.tick(midi)?;
//         self.write_to_serial();
//         Ok(duration)
//     }

//     fn write_to_serial(&mut self) {
//         self.dmx.set_buffer(self.base.channels);
//         self.dmx.render().unwrap();
//     }
// }

pub fn supervisor_thread(
    from_frontend: Receiver<FromFrontend>,
    audio_thread_control_signal: Arc<AtomicU8>,
    signal_out_0: Sender<Signal>,
    system_out: Sender<SystemMessage>,
    config: Config,
) {
    log::info!("[SUPERVISOR] Thread started!");

    let heartbeat_delay = Duration::from_millis(1000);

    let mut audio_device: Option<Device> = None;
    let mut device_changed = false;

    // TODO: put the DMX thread under main!

    audio_thread_control_signal.store(AudioThreadControlSignal::ABORTED, Ordering::Relaxed);

    let mut seq = 0;

    let mut sent_no_device_available_log_message = false;

    loop {
        thread::sleep(heartbeat_delay);

        if system_out.send(SystemMessage::Heartbeat(seq)).is_err() {
            warn!("[SUPERVISOR] Shutting down...");
            break;
        };
        seq += 1;

        match from_frontend.try_recv() {
            Ok(FromFrontend::Reload) => {
                info!("[SUPERVISOR] Got reload request");
                if audio_thread_control_signal.load(Ordering::Relaxed)
                    == AudioThreadControlSignal::CONTINUE
                {
                    audio_thread_control_signal
                        .store(AudioThreadControlSignal::RELOAD, Ordering::Relaxed);
                }
            }
            Ok(FromFrontend::SelectSerialDevice(dev)) => {
                // TODO: maybe implement this
                // Get device by name.
            }
            Ok(FromFrontend::SelectInputDevice(dev)) => {
                // Get device by name.
                audio_device = dev;
                device_changed = true;
            }
            Ok(FromFrontend::MatrixControl(control)) => {
                // 255 is for the builtin device.
                // midi_in_sender
                //     .send(MidiEvent {
                //         device: control.device,
                //         status: control.y,
                //         data0: control.x,
                //         data1: control.value as u8,
                //     })
                //     .unwrap();
                todo!("no longer supported");
            }
            Err(TryRecvError::Disconnected) => {
                log::warn!("[SUPERVISOR] Shutting down.");
                break;
            }
            Err(TryRecvError::Empty) => {}
        };

        // Check if the thread crashed and attempt to restart it.
        if audio_thread_control_signal.load(Ordering::Relaxed) == AudioThreadControlSignal::CRASHED
            && audio_device.is_some()
        {
            thread::sleep(Duration::from_secs(2));
            device_changed = true;
        }

        if audio_device.is_none() {
            let devices = utils::get_input_devices_flat();

            // TODO: add an aggregate log macro which logs to the channel and the console.

            system_out
                .send(SystemMessage::AudioDevicesView(devices))
                .unwrap();

            if !sent_no_device_available_log_message {
                system_out
                    .send(SystemMessage::Log(
                        "[audio] No audio device selected, waiting for selection...".to_string(),
                    ))
                    .unwrap();
                sent_no_device_available_log_message = true;
            }

            device_changed = false;
        } else if device_changed {
            system_out
                .send(SystemMessage::AudioSelected(audio_device.clone()))
                .unwrap();

            let (sig_0, sys) = (signal_out_0.clone(), system_out.clone());
            {
                let audio_input_device = audio_device.clone().unwrap();
                let audio_thread_control_signal = audio_thread_control_signal.clone();

                let sys = sys.clone();

                let config = config.clone();

                thread::spawn(move || {
                    audio_thread_control_signal
                        .store(AudioThreadControlSignal::CONTINUE, Ordering::Relaxed);

                    if let Err(err) = mainloop::run(
                        audio_input_device,
                        sig_0,
                        sys.clone(),
                        audio_thread_control_signal.clone(),
                        config,
                    ) {
                        // TODO: handle the audio backend error.
                        sys.send(SystemMessage::Log(format!("[audio] {err}")))
                            .unwrap();

                        audio_thread_control_signal
                            .store(AudioThreadControlSignal::CRASHED, Ordering::Relaxed);
                    }

                    sys.send(SystemMessage::Log("[audio] Thread died.".into()))
                        .unwrap();
                });
            }

            device_changed = false;
            log::info!(
                "[AUDIO] Main thread started: <{}>",
                audio_device.clone().unwrap().name().unwrap()
            );

            sys.send(SystemMessage::Log("[audio] Thread started.".to_string()))
                .unwrap();
        }
    }
}

// fn init_dmx(
//     midi_out_sender: Sender<MidiEvent>,
//     system_out: Sender<SystemMessage>,
// ) -> anyhow::Result<DmxUniverse> {
//     debug!("[DMX] Trying to establish hardware link...");
//     let res = DmxUniverse::new(midi_out_sender.clone(), system_out.clone());
//     let dmx_universe = match res {
//         Ok(universe) => universe,
//         Err(e) => {
//             info!("[DMX] Failed to establish hardware link: {e}, using dummy...");
//             let Ok(universe) = DmxUniverse::new_dummy(midi_out_sender, system_out.clone()) else {
//                 bail!("[DMX] Failed to create dummy universe, exiting.");
//             };

//             universe
//         }
//     };

//     Ok(dmx_universe)
// }
