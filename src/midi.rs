use crossbeam_channel::{Receiver, Sender, TryRecvError};
use midir::{Ignore, MidiInput, MidiOutput, MidiOutputConnection};
use std::collections::HashMap;
use std::error::Error;
use std::io::{stdin, stdout, Write};
use std::thread;
use std::time::Duration;
use wmidi::MidiMessage;

use crate::app::MidiEvent;

#[derive(Debug)]
pub enum MidiError {
    DeviceNotFound,
    Other(String),
}

//
// TODO: multi-device input support.
//

struct MidiOutHandle {
    device_id: u8,
    conn: MidiOutputConnection,
}

pub fn midi(
    signal_from_controller_sender: Sender<MidiEvent>,
    signal_to_controller_receiver: Receiver<MidiEvent>,
) -> Result<(), MidiError> {
    log::trace!("[MIDI] Started thread");

    let connections = vec![/*"DDJ-400",*/  "DDJ-200"];

    let mut conns_out = HashMap::new();
    let mut conns_in = HashMap::new();

    for (device_id, conn) in connections.iter().enumerate() {
        let mut midi_in =
            MidiInput::new("ddj-listener").map_err(|e| MidiError::Other(e.to_string()))?;
        midi_in.ignore(Ignore::None);
        let in_ports = midi_in.ports();

        let in_port = in_ports
            .iter()
            .find(|p| midi_in.port_name(p).unwrap().contains(conn))
            .ok_or(MidiError::DeviceNotFound)?;

        log::debug!(
            "[MIDI-IN] Connecting to: {}",
            midi_in
                .port_name(in_port)
                .map_err(|e| MidiError::Other(e.to_string()))?
        );

        let send = signal_from_controller_sender.clone();

        let _conn_in = midi_in
            .connect(
                in_port,
                "ddj-read",
                move |_, message, _| {
                    println!("MIDI received: {:?}", message);
                    if message.len() != 3 {
                        panic!("weird message");
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
            .find(|p| midi_out.port_name(p).unwrap().contains(conn))
            .ok_or("MIDI device not found")
            .map_err(|_| MidiError::DeviceNotFound)?;

        log::trace!(
            "[MIDI-OUT] Connecting to: {}",
            midi_out
                .port_name(out_port)
                .map_err(|e| MidiError::Other(e.to_string()))?
        );

        let conn_out = midi_out
            .connect(out_port, "ddj-send")
            .map_err(|e| MidiError::Other(e.to_string()))?;

        conns_out.insert(device_id, conn_out);
        conns_in.insert(device_id, _conn_in);
    }

    loop {
        thread::sleep(Duration::from_millis(10));
        match signal_to_controller_receiver.try_recv() {
            Ok(sig) => {
                let c = conns_out.get_mut(&(sig.device as usize));

                match c {
                    Some(c) => {
                        c.send(&[sig.status, sig.data0, sig.data1]).unwrap();
                    }
                    None => {
                        log::error!("MIDI output not found: {}", sig.device)
                    },
                };
            }
            Err(TryRecvError::Empty) => {}
            Err(TryRecvError::Disconnected) => {
                log::warn!("[MIDI] Terminating...");
                break;
            }
        }
    }

    Ok(())
}
