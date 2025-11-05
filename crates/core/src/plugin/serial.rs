use blaulicht_shared::SerialReceived;
use crossbeam_channel::{Receiver, Sender, TryRecvError};
use log::{debug, error, info, trace, warn};
use midir::{Ignore, MidiInput, MidiInputConnection, MidiOutput, MidiOutputConnection};
use serialport::SerialPort;
use std::collections::HashMap;
use std::time::Duration;
use std::{mem, thread};
use wmidi::MidiMessage;

const PORT_TIMEOUT: Duration = Duration::from_millis(10);

#[derive(Debug)]
pub enum SerialError {
    DeviceNotFound,
    Other(String),
}

pub struct SerialManager {
    connection_map: HashMap<String, SerialDeviceHandle>,
    device_id_counter: u8,
    // // Events from a outside to the manager.
    // midi_in_sender: Sender<MidiEvent>,
    // midi_in_receiver: Receiver<MidiEvent>,

    // // Events from the manager to a device.
    // to_manager_receiver: Receiver<MidiEvent>,

    // // To plugins sender.
    // to_plugins_sender: Sender<MidiEvent>,
}

struct SerialDeviceHandle {
    port_path: String,
    device_id: u8,
    serial_port: Box<dyn SerialPort>,
    buffer: Vec<u8>,
}

impl SerialManager {
    pub fn new(// midi_out_receiver: Receiver<MidiEvent>,
        // to_plugins_sender: Sender<MidiEvent>,
    ) -> Self {
        // let (midi_in_sender, midi_in_receiver) = crossbeam_channel::bounded(100);

        Self {
            device_id_counter: 0,
            connection_map: HashMap::new(),
            // midi_in_sender,
            // midi_in_receiver,
            // to_manager_receiver: midi_out_receiver,
            // to_plugins_sender,
        }
    }

    pub fn enumerate_devices() -> Result<Vec<String>, SerialError> {
        todo!("not implemented")
        // let midi_in =
        //     MidiInput::new("device_enumerator").map_err(|e| SerialError::Other(e.to_string()))?;

        // let in_ports = midi_in.ports();
        // let mut device_names = Vec::new();

        // for port in &in_ports {
        //     if let Ok(name) = midi_in.port_name(port) {
        //         device_names.push(name);
        //     }
        // }

        // Ok(device_names)
    }

    pub fn request_device(&mut self, port_path: &str, baud_rate: u32) -> Option<u8> {
        // TODO: would normally check that this is not used by one of the DMX interfaces.

        // Check if there is already a handle for this device.
        if let Some(dev) = self.connection_map.get(port_path) {
            debug!(
                "Reusing existing SERIAL connection with id: {} for device: '{port_path}'",
                dev.device_id
            );
            return Some(dev.device_id);
        }

        let serial_port = match serialport::new(port_path, baud_rate)
            .timeout(PORT_TIMEOUT)
            .open()
        {
            Ok(port) => port,
            Err(err) => {
                error!("Could not open serial port: {err}");
                return None;
            }
        };

        let device_id = self.device_id_counter;
        self.device_id_counter += 1;

        self.connection_map.insert(
            port_path.to_string(),
            SerialDeviceHandle {
                port_path: port_path.to_string(),
                device_id,
                serial_port,
                buffer: vec![],
            },
        );

        Some(device_id)
    }

    pub fn tick(&mut self) -> Result<Vec<SerialReceived>, SerialError> {
        let mut error = None;

        let mut incoming_events = vec![];

        for port in self.connection_map.values_mut() {
            // Poll the port.
            let mut buf = [0u8; 10_000];
            match port.serial_port.read(&mut buf) {
                Ok(n) if n > 0 => {
                    let sliced_buf = &buf[..n];

                    // Check for linebreaks.
                    let mut linefeed_index = None;

                    for i in 0..n {
                        if sliced_buf[i] == b'\n' {
                            linefeed_index = Some(i);
                            break;
                        }
                    }


                    println!("Read {} bytes: {:?}", n, &sliced_buf);

                    port.buffer.extend_from_slice(sliced_buf);

                    println!("buf_so_far {:?}", port.buffer);

                    if let Some(linefeed_end) = linefeed_index {
                        println!("got line feed");

                        let sliced_buf = &sliced_buf[..linefeed_end];
                        port.buffer.extend_from_slice(sliced_buf);

                        let mut taken = mem::replace(&mut port.buffer, vec![]);
                        // Trim ending zeroes.
                        let mut zero_end = 0;
                        for (index, char) in taken.iter().enumerate() {
                            if *char == 0x0 {
                                zero_end = index;
                                break;
                            }
                        }

                        taken.truncate(zero_end);
                        // let taken = taken[..zero_end].to_vec();

                        if !taken.is_empty() {
                            incoming_events.push(SerialReceived { device: port.device_id, body:  taken});
                        }
                    }
                }
                Ok(_) /* n = 0 */ => {
                    panic!("Critical serial port error: 0 bytes read.");
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::TimedOut => {
                    // No data available yet, continue polling
                }
                Err(e) => {
                    error!("Serial port error: {:?}", e);
                    if error.is_none() {
                        error = Some(e);
                    }
                }
            }
        }

        // TODO: error handling.

        // println!("incoming: {incoming_events:?}");

        Ok(incoming_events)
    }
}
