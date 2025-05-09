use crossbeam_channel::{Receiver, Sender, TryRecvError};
use midir::{Ignore, MidiInput, MidiOutput};
use std::error::Error;
use std::io::{stdin, stdout, Write};
use std::thread;
use std::time::Duration;
use wmidi::MidiMessage;

#[derive(Debug)]
pub enum MidiError {
    DeviceNotFound,
    Other(String),
}

pub fn midi(
    signal_from_controller_sender: Sender<(u8, u8, u8)>,
    signal_to_controller_receiver: Receiver<(u8, u8, u8)>,
) -> Result<(), MidiError> {
    log::trace!("[MIDI] Started thread");
    let mut midi_in =
        MidiInput::new("ddj-listener").map_err(|e| MidiError::Other(e.to_string()))?;
    midi_in.ignore(Ignore::None);

    let in_ports = midi_in.ports();
    let in_port = in_ports
        .iter()
        .find(|p| midi_in.port_name(p).unwrap().contains("DDJ-400"))
        .ok_or(MidiError::DeviceNotFound)?;

    log::debug!(
        "[MIDI-IN] Connecting to: {}",
        midi_in
            .port_name(in_port)
            .map_err(|e| MidiError::Other(e.to_string()))?
    );

    let _conn_in = midi_in
        .connect(
            in_port,
            "ddj-read",
            move |_, message, _| {
                // println!("MIDI received: {:?}", message);
                if message.len() != 3 {
                    panic!("weird message");
                }
                signal_from_controller_sender
                    .send((message[0], message[1], message[2]))
                    .map_err(|e| MidiError::Other(e.to_string()))
                    .unwrap();
            },
            (),
        )
        .map_err(|e| MidiError::Other(e.to_string()))?;

    let mut midi_out = MidiOutput::new("ddj-sender").unwrap();
    // midi_out.ignore(Ignore::None);
    let out_ports = midi_out.ports();
    let out_port = out_ports
        .iter()
        .find(|p| midi_out.port_name(p).unwrap().contains("DDJ-400"))
        .ok_or("DDJ-400 not found")
        .map_err(|e| MidiError::DeviceNotFound)?;

    log::trace!(
        "[MIDI-OUT] Connecting to: {}",
        midi_out
            .port_name(out_port)
            .map_err(|e| MidiError::Other(e.to_string()))?
    );

    let mut conn_out = midi_out
        .connect(out_port, "ddj-send")
        .map_err(|e| MidiError::Other(e.to_string()))?;

    loop {
        thread::sleep(Duration::from_millis(10));
        match signal_to_controller_receiver.try_recv() {
            Ok(sig) => {
                conn_out.send(&[sig.0, sig.1, sig.2]).unwrap();
            }
            Err(TryRecvError::Empty) => {}
            Err(TryRecvError::Disconnected) => panic!("ERROR"),
        }
    }

    Ok(())
}
