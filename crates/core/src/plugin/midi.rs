use crossbeam_channel::{Receiver, Sender, TryRecvError};
use log::{debug, error, info, trace, warn};
use midir::{Ignore, MidiInput, MidiInputConnection, MidiOutput, MidiOutputConnection};
use std::collections::HashMap;
use std::thread;
use std::time::Duration;
use wmidi::MidiMessage;

use crate::msg::MidiEvent;

#[derive(Debug)]
pub enum MidiError {
    DeviceNotFound,
    Other(String),
}

//
// TODO: multi-device input support.
//

pub struct MidiManager {
    connection_map: HashMap<String, MidiDeviceHandle>,
    device_id_counter: u8,

    // Events from a outside to the manager.
    midi_in_sender: Sender<MidiEvent>,
    midi_in_receiver: Receiver<MidiEvent>,

    // Events from the manager to a device.
    to_manager_receiver: Receiver<MidiEvent>,

    // To plugins sender.
    to_plugins_sender: Sender<MidiEvent>,
}

struct MidiDeviceHandle {
    device_name: String,
    device_id: u8,
    output: Option<MidiOutputConnection>,
    input: MidiInputConnection<()>,
}

impl MidiManager {
    pub fn new(
        midi_out_receiver: Receiver<MidiEvent>,
        to_plugins_sender: Sender<MidiEvent>,
    ) -> Self {
        let (midi_in_sender, midi_in_receiver) = crossbeam_channel::bounded(100);

        Self {
            device_id_counter: 0,
            connection_map: HashMap::new(),
            midi_in_sender,
            midi_in_receiver,
            to_manager_receiver: midi_out_receiver,
            to_plugins_sender,
        }
    }

    pub fn enumerate_devices() -> Result<Vec<String>, MidiError> {
        let midi_in = MidiInput::new("device_enumerator")
            .map_err(|e| MidiError::Other(e.to_string()))?;
        
        let in_ports = midi_in.ports();
        let mut device_names = Vec::new();
        
        for port in &in_ports {
            if let Ok(name) = midi_in.port_name(port) {
                device_names.push(name);
            }
        }
        
        Ok(device_names)
    }

    pub fn request_device(&mut self, device_name: &str) -> Option<u8> {
        // Check if there is already a handle for this device.
        if let Some(dev) = self.connection_map.get(device_name) {
            debug!(
                "Reusing existing MIDI connection with id: {} for device: '{device_name}'",
                dev.device_id
            );
            return Some(dev.device_id);
        }

        let id = match self.request_device_internal(device_name) {
            Ok(id) => {
                info!("Opened MIDI device '{}' with ID {}", device_name, id);
                Some(id)
            }
            Err(err) => {
                error!("Failed to request MIDI device '{}': {:?}", device_name, err);
                None
            }
        };

        id
    }

    fn request_device_internal(&mut self, device_name: &str) -> Result<u8, MidiError> {
        let device_id = {
            if self.device_id_counter == u8::MAX {
                return Err(MidiError::Other("Maximum device ID reached".to_string()));
            }
            self.device_id_counter += 1;
            self.device_id_counter
        };

        let mut midi_in = MidiInput::new(format!("{device_name}_listener").as_str())
            .map_err(|e| MidiError::Other(e.to_string()))?;

        midi_in.ignore(Ignore::None);
        let in_ports = midi_in.ports();

        // TODO: make this viewable in a different way!.
        debug!(
            "Available MIDI input ports: [\n{}\n]",
            in_ports
                .iter()
                .map(|p| format!("'{}'", midi_in.port_name(p).unwrap()))
                .collect::<Vec<_>>()
                .join(",\n")
        );

        let in_port = in_ports
            .iter()
            .find(|p| midi_in.port_name(p).unwrap().contains(device_name))
            .ok_or(MidiError::DeviceNotFound)?;

        debug!(
            "[MIDI-IN] Connecting to: {}",
            midi_in
                .port_name(in_port)
                .map_err(|e| MidiError::Other(e.to_string()))?
        );

        info!("[MIDI-IN] About to call midi_in.connect()...");
        let send = self.midi_in_sender.clone();

        let _conn_in = midi_in
            .connect(
                in_port,
                format!("{device_name}_listener").as_str(),
                move |_, message, _| {
                    // Handle different MIDI message lengths
                    // 2-byte messages: Program Change (0xC0-0xCF), Channel Pressure (0xD0-0xDF)
                    // 3-byte messages: Note On/Off, Control Change, Pitch Bend, etc.
                    let (data0, data1) = match message.len() {
                        2 => (message[1], 0),
                        3 => (message[1], message[2]),
                        _ => {
                            warn!("Unexpected MIDI message length: {}, message: {:?}", message.len(), message);
                            return;
                        }
                    };

                    send.send(MidiEvent {
                        device: device_id as u8,
                        status: message[0],
                        data0,
                        data1,
                    })
                    .map_err(|e| MidiError::Other(e.to_string()))
                    .unwrap();
                },
                (),
            )
            .map_err(|e| MidiError::Other(e.to_string()))?;

        info!("[MIDI-IN] Successfully connected input!");

        let conn_out = match MidiOutput::new("midi-sender") {
            Ok(midi_out) => {
                let out_ports = midi_out.ports();
                let out_port = out_ports
                    .iter()
                    .find(|p| midi_out.port_name(p).unwrap().contains(device_name));

                match out_port {
                    Some(port) => {
                        trace!(
                            "[MIDI-OUT] Connecting to: {}",
                            midi_out
                                .port_name(port)
                                .map_err(|e| MidiError::Other(e.to_string()))?
                        );

                        match midi_out.connect(port, "midi-sender") {
                            Ok(conn) => Some(conn),
                            Err(e) => {
                                warn!("[MIDI-OUT] Failed to connect: {}", e);
                                None
                            }
                        }
                    }
                    None => {
                        debug!("[MIDI-OUT] No output port found for device: {}", device_name);
                        None
                    }
                }
            }
            Err(e) => {
                warn!("[MIDI-OUT] Failed to create MIDI output: {}", e);
                None
            }
        };

        self.connection_map.insert(
            device_name.to_string(),
            MidiDeviceHandle {
                device_name: device_name.to_string(),
                device_id,
                output: conn_out,
                input: _conn_in,
            },
        );

        Ok(device_id as u8)
    }

    pub fn tick(&mut self) -> Result<Vec<MidiEvent>, MidiError> {
        //
        // Process MIDI input events.
        //
        let mut incoming_events = vec![];
        loop {
            match self.midi_in_receiver.try_recv() {
                Ok(data) => incoming_events.push(data),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    unreachable!("MIDI detached")
                }
            }
        }

        //
        // Send any outgoing MIDI events.
        //
        loop {
            debug_assert!(!self.to_manager_receiver.is_full());
            match self.to_manager_receiver.try_recv() {
                Ok(sig) => {
                    // Get matching MIDI output connection.
                    let Some(output_device) = self
                        .connection_map
                        .values_mut()
                        .find(|d| d.device_id == sig.device)
                    else {
                        // Device disconnected.
                        warn!("MIDI device {} not found in connection map", sig.device);
                        return Err(MidiError::DeviceNotFound);
                    };

                    if let Some(ref mut output) = output_device.output {
                        output
                            .send(&[sig.status, sig.data0, sig.data1])
                            .unwrap();
                    } else {
                        debug!("MIDI output not available for device {}", sig.device);
                    }
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    // TODO: how to handle this?
                    log::warn!("[MIDI] Terminating...");
                    return Ok(vec![]);
                }
            };
        }

        Ok(incoming_events)
    }
}
