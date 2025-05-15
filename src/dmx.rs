use crossbeam_channel::{Receiver, Sender, TryRecvError};
use enttecopendmx::EnttecOpenDMX;
use std::{
    mem,
    net::UdpSocket,
    sync::{
        atomic::{AtomicU8, Ordering},
        Arc,
    },
    thread,
    time::{Duration, Instant},
    vec,
};

use crate::{
    app::MidiEvent, midi, utils::device_from_name, wasm::{self, TickEngine, TickInput}, ToFrontent
};

use cpal::{traits::DeviceTrait, Device};
use log::{info, warn};
use serialport::{SerialPort, SerialPortInfo, SerialPortType};

use crate::{
    app::FromFrontend,
    audio::{self, AudioThreadControlSignal, Signal, SystemMessage},
    utils,
};

struct DmxUniverseBasic {
    tick_engine: TickEngine,
    channels: [u8; 513],
    tickinput: TickInput,
    system_out: Sender<SystemMessage>,
}

impl DmxUniverseBasic {
    fn new(midi_out: Sender<MidiEvent>, system_out: Sender<SystemMessage>) -> Self {
        let tick_engine = wasm::TickEngine::create(midi_out, system_out.clone()).unwrap();

        Self {
            tick_engine,
            channels: [0; 513],
            tickinput: TickInput::default(),
            system_out,
        }
    }

    fn signal(&mut self, signal: Signal) {
        match signal {
            Signal::Volume(v) => {
                self.tickinput.volume = v;
            }
            Signal::BeatVolume(v) => {
                self.tickinput.beat_volume = v;
            }
            Signal::Bass(v) => {
                self.tickinput.bass = v;
            }
            Signal::BassAvgShort(v) => {
                self.tickinput.bass_avg_short = v;
            }
            Signal::BassAvg(v) => {
                self.tickinput.bass_avg = v;
            }
            Signal::Bpm(v) => {
                self.tickinput.bpm = v.bpm;
                self.tickinput.time_between_beats_millis = v.time_between_beats_millis;
            }
        }
    }

    fn tick(&mut self, midi: &[MidiEvent]) -> anyhow::Result<Duration> {
        let start = Instant::now();
        self.tick_engine.tick(self.tickinput, &midi, false)?;

        for (index, value) in self.tick_engine.dmx().iter().enumerate() {
            self.channels[index] = *value as u8;
        }

        let elapsed = Instant::now().duration_since(start);
        Ok(elapsed)
    }

    fn reload(&mut self) -> wasmtime::Result<()> {
        self.tick_engine.reload()
    }
}

struct DmxUniverseDummy {
    basic: DmxUniverseBasic,
    last_state: [u8; 513],
}

pub enum DmxUniverse {
    Dummy(DmxUniverseDummy),
    Real(DmxUniverseReal),
}

impl DmxUniverse {
    pub fn new(
        port_path: String,
        signal_out: Sender<Signal>,
        midi_out: Sender<MidiEvent>,
        system_out: Sender<SystemMessage>,
    ) -> Self {
        // let mut wasm_engine = wasm::TickEngine::create(midi_out).unwrap();
        let base = DmxUniverseBasic::new(midi_out, system_out);

        Self::Real(DmxUniverseReal::new(port_path, base))
    }

    pub fn new_dummy(midi_out: Sender<MidiEvent>, system_out: Sender<SystemMessage>) -> Self {
        let base = DmxUniverseBasic::new(midi_out, system_out);
        Self::Dummy(DmxUniverseDummy {
            basic: base,
            last_state: [0; 513],
        })
    }

    pub fn signal(&mut self, signal: Signal) {
        match self {
            DmxUniverse::Dummy(dummy) => dummy.basic.signal(signal),
            DmxUniverse::Real(dmx_universe_real) => dmx_universe_real.signal(signal),
        }
    }

    pub fn tick(&mut self, midi: &[MidiEvent]) -> anyhow::Result<Duration> {
        match self {
            DmxUniverse::Dummy(ref mut dummy) => {
                let dur = dummy.basic.tick(midi)?;

                let mut modified = false;
                for (a, b) in dummy.basic.channels.iter().zip(dummy.last_state.iter()) {
                    if a != b {
                        modified = true;
                        break;
                    }
                }

                if modified {
                    dummy
                        .basic
                        .system_out
                        .send(SystemMessage::DMX(dummy.basic.channels))
                        .unwrap();

                    dummy.last_state = dummy.basic.channels;
                }

                Ok(dur)
            }
            DmxUniverse::Real(dmx_universe_real) => dmx_universe_real.tick(midi),
        }
    }

    pub fn reload(&mut self) -> wasmtime::Result<()> {
        match self {
            DmxUniverse::Dummy(dummy) => dummy.basic.reload(),
            DmxUniverse::Real(dmx_universe_real) => dmx_universe_real.reload(),
        }
    }
}

enum Color {
    Red,
    Purple,
    Blue,
    Cyan,
    Green,
    Yellow,
}

impl Color {
    fn from_index(index: u8) -> Self {
        match index {
            0 => Self::Red,
            1 => Self::Purple,
            2 => Self::Blue,
            3 => Self::Cyan,
            4 => Self::Green,
            5 => Color::Yellow,
            _ => unreachable!(),
        }
    }

    fn channels(&self) -> [u8; 3] {
        match self {
            Color::Red => [255, 0, 0],
            Color::Purple => [255, 0, 255],
            Color::Blue => [0, 0, 255],
            Color::Cyan => [0, 255, 255],
            Color::Green => [0, 255, 0],
            Color::Yellow => [255, 255, 0],
        }
    }
}

struct DmxUniverseReal {
    // serial: Box<dyn SerialPort>,
    dmx: EnttecOpenDMX,
    base: DmxUniverseBasic,
}

impl DmxUniverseReal {
    fn new(
        port_path: String,
        // signal_out: Sender<Signal>,
        // system_out: Sender<SystemMessage>,
        base: DmxUniverseBasic,
    ) -> Self {
        // let serial = serialport::new(port_path, 250000)
        //     .timeout(Duration::from_millis(1))
        //     .stop_bits(serialport::StopBits::Two)
        //     .data_bits(serialport::DataBits::Eight)
        //     .parity(serialport::Parity::None)
        //     .open()
        //     .expect("Failed to open port");

        let mut interface = enttecopendmx::EnttecOpenDMX::new().unwrap();
        interface.open().unwrap();

        // let base = DmxUniverseBasic::new(midi_out, system_out);

        Self {
            dmx: interface,
            base,
        }
    }

    fn reload(&mut self) -> wasmtime::Result<()> {
        self.base.reload()
    }

    fn signal(&mut self, signal: Signal) {
        self.base.signal(signal)
    }

    pub fn tick(&mut self, midi: &[MidiEvent]) -> anyhow::Result<Duration> {
        let duration = self.base.tick(midi)?;

        // TODO: is this right?
        // Only update on write?
        // self.base
        //     .system_out
        //     .send(SystemMessage::DMX(self.base.channels.clone()))
        //     .unwrap();

        self.write_to_serial();

        Ok(duration)
    }

    // fn send_break(&self, duration: Duration) {
    //     self.serial.set_break().expect("Failed to set break");
    //     spin_sleep::sleep(duration);
    //     self.serial.clear_break().expect("Failed to clear break");
    // }

    fn write_to_serial(&mut self) {
        // interface.set_channel(1 as usize, 255 as u8);
        self.dmx.set_buffer(self.base.channels);
        self.dmx.render().unwrap();

        // self.send_break(Duration::from_micros(100));
        // spin_sleep::sleep(Duration::from_micros(100));
        // self.serial.write_all(&self.base.channels).unwrap();
        // self.serial.flush().unwrap();
    }
}

pub struct UsbDevice {
    pub vid: u16,
    pub pid: u16,
}

pub const EUROLITE_USB_DMX512_PRO_CABLE_INTERFACE: UsbDevice = UsbDevice {
    vid: 1027,
    pid: 24577,
};

pub const USB_DEVICES: [UsbDevice; 1] = [EUROLITE_USB_DMX512_PRO_CABLE_INTERFACE];
// const SERIAL_ERROR_RETRY: Duration = Duration::from_secs(5);

pub enum DMXControl {
    ChangePort(Option<SerialPortInfo>),
}

pub fn audio_thread(
    from_frontend: Receiver<FromFrontend>,
    audio_thread_control_signal: Arc<AtomicU8>,
    signal_out_0: Sender<Signal>,
    system_out: Sender<SystemMessage>,
    midi_in_receiver: Receiver<MidiEvent>,
    midi_in_sender: Sender<MidiEvent>,
    midi_out_sender: Sender<MidiEvent>,
) {
    log::info!("[SUPERVISOR] Thread started!");

    let heartbeat_delay = Duration::from_millis(1000);

    let mut audio_device: Option<Device> = None;
    let mut device_changed = false;

    // TODO: put the DMX thread under main!

    let mut seq = 0;

    loop {
        thread::sleep(heartbeat_delay);

        if let Err(_) = system_out.send(SystemMessage::Heartbeat(seq)) {
            warn!("[SUPERVISOR] Shutting down...");
            break;
        };
        seq += 1;

        match from_frontend.try_recv() {
            Ok(FromFrontend::Reload) => {
                audio_thread_control_signal
                    .store(AudioThreadControlSignal::RELOAD, Ordering::Relaxed);
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
                midi_in_sender
                    .send(MidiEvent { device: control.device, status: control.y, data0: control.x, data1: control.value as u8 })
                    .unwrap();
            }
            Err(TryRecvError::Empty) => {}
            Err(TryRecvError::Disconnected) => {
                log::warn!("[SUPERVISOR] Shutting down.");
                break;
            }
        };

        if audio_device.is_none() {
            let devices = utils::get_input_devices_flat();
            system_out
                .send(SystemMessage::AudioDevicesView(devices))
                .unwrap();

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

                let midi_recv = midi_in_receiver.clone();
                let midi_send = midi_out_sender.clone();

                thread::spawn(move || {
                    if let Err(err) = audio::run(
                        audio_input_device,
                        sig_0,
                        sys.clone(),
                        audio_thread_control_signal.clone(),
                        midi_recv,
                        midi_send,
                    ) {
                        // TODO: handle the audio backend error.
                        sys.send(SystemMessage::Log(format!("[audio] {err}")))
                            .unwrap();
                    }

                    sys.send(SystemMessage::Log("[audio] Thread died.".into()))
                        .unwrap();

                    audio_thread_control_signal
                        .store(AudioThreadControlSignal::DEAD, Ordering::Relaxed);
                });
            }

            device_changed = false;
            log::info!(
                "[AUDIO] Main thread started: <{}>",
                audio_device.clone().unwrap().name().unwrap()
            );

            sys.send(SystemMessage::Log(format!("[audio] Thread started.")))
                .unwrap();
        }
    }
}
