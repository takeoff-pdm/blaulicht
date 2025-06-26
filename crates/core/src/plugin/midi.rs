use actix_web::cookie::time::error;
use crossbeam_channel::{Receiver, Sender, TryRecvError};
use log::{debug, error, info, trace, warn};
use midir::{Ignore, MidiInput, MidiInputConnection, MidiOutput, MidiOutputConnection};
use std::collections::HashMap;
use std::thread;
use std::time::Duration;

use crate::app::MidiEvent;

#[derive(Debug)]
pub enum MidiError {
    DeviceNotFound,
    Other(String),
}

//
// TODO: multi-device input support.
//

pub struct MidiManager {
    connection_map: HashMap<u8, MidiDeviceHandle>,
    device_id_counter: u8,

    // Events from a outside to the manager.
    midi_in_sender: Sender<MidiEvent>,
    midi_in_receiver: Receiver<MidiEvent>,

    // Events from the manager to a device.
    to_manager_receiver: Receiver<ToMidiManagerMessage>,

    // To plugins sender.
    to_plugins_sender: Sender<FromMidiManagerMessage>,
}

struct MidiDeviceHandle {
    device_name: String,
    device_id: u8,
    output: MidiOutputConnection,
    input: MidiInputConnection<()>,
}

// struct MidiOutHandle {
//     device_id: u8,
//     conn: MidiOutputConnection,
// }

pub enum ToMidiManagerMessage {
    RequestDevice(String),
    SendMidiEvent(MidiEvent),
}

pub enum FromMidiManagerMessage {
    DeviceRequestStatus(String, Option<u8>),
}

impl MidiManager {
    pub fn new(
        midi_out_receiver: Receiver<ToMidiManagerMessage>,
        to_plugins_sender: Sender<FromMidiManagerMessage>,
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

    fn request_device(&mut self, device_name: &str) {
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

        self.to_plugins_sender
            .send(FromMidiManagerMessage::DeviceRequestStatus(
                device_name.to_string(),
                id,
            ))
            .expect("Failed to send device request status");
    }

    pub fn request_device_internal(&mut self, device_name: &str) -> Result<u8, MidiError> {
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

        debug!(
            "Available MIDI input ports: [{}]",
            in_ports
                .iter()
                .map(|p| midi_in.port_name(p).unwrap())
                .collect::<Vec<_>>()
                .join(", ")
        );

        let in_port = in_ports
            .iter()
            .find(|p| midi_in.port_name(p).unwrap() == device_name)
            .ok_or(MidiError::DeviceNotFound)?;

        debug!(
            "[MIDI-IN] Connecting to: {}",
            midi_in
                .port_name(in_port)
                .map_err(|e| MidiError::Other(e.to_string()))?
        );

        let send = self.midi_in_sender.clone();

        let _conn_in = midi_in
            .connect(
                in_port,
                format!("{device_name}_listener").as_str(),
                move |_, message, _| {
                    println!("MIDI received: {message:?}");
                    if message.len() != 3 {
                        // TODO: handle this more gracefully.
                        panic!("BUG: weird message");
                    }

                    send.send(MidiEvent {
                        device: device_id as u8,
                        status: message[0],
                        data0: message[1],
                        data1: message[2],
                    })
                    .map_err(|e| MidiError::Other(e.to_string()))
                    .unwrap();
                },
                (),
            )
            .map_err(|e| MidiError::Other(e.to_string()))?;

        let midi_out = MidiOutput::new("ddj-sender").unwrap();
        // midi_out.ignore(Ignore::None);
        let out_ports = midi_out.ports();
        let out_port = out_ports
            .iter()
            .find(|p| midi_out.port_name(p).unwrap() == device_name)
            .ok_or("MIDI device not found")
            .map_err(|_| MidiError::DeviceNotFound)?;

        trace!(
            "[MIDI-OUT] Connecting to: {}",
            midi_out
                .port_name(out_port)
                .map_err(|e| MidiError::Other(e.to_string()))?
        );

        let conn_out = midi_out
            .connect(out_port, "ddj-send")
            .map_err(|e| MidiError::Other(e.to_string()))?;

        self.connection_map.insert(
            device_id,
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
                Err(TryRecvError::Disconnected) => break,
            }
        }

        //
        // Send any outgoing MIDI events.
        //
        match self.to_manager_receiver.try_recv() {
            Ok(ToMidiManagerMessage::SendMidiEvent(sig)) => {
                // Get matching MIDI output connection.
                let Some(output_device) = self.connection_map.get_mut(&sig.device) else {
                    // Device disconnected.
                    warn!(
                        "MIDI device {} not found, removing from connection map.",
                        sig.device
                    );
                    self.connection_map.remove(&sig.device);
                    return Err(MidiError::DeviceNotFound);
                };

                output_device
                    .output
                    .send(&[sig.status, sig.data0, sig.data1])
                    .unwrap();
            }
            Ok(ToMidiManagerMessage::RequestDevice(device_name)) => {
                self.request_device(&device_name);
            }
            Err(TryRecvError::Empty) => {}
            Err(TryRecvError::Disconnected) => {
                // TODO: how to handle this?
                log::warn!("[MIDI] Terminating...");
                return Ok(vec![]);
            }
        };

        Ok(incoming_events)
    }
}
