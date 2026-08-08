use blaulicht_shared::SerialReceived;
use serialport::{available_ports, ErrorKind as SerialPortErrorKind, SerialPort};
use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, error};
// use wmidi::MidiMessage;

use crate::state::{AppState, SerialDeviceState};

const PORT_TIMEOUT: Duration = Duration::from_millis(3);

fn drain_serial_lines(buffer: &mut Vec<u8>, device: u8) -> Vec<SerialReceived> {
    let mut events = Vec::new();
    while let Some(linefeed_end) = buffer.iter().position(|byte| *byte == b'\n') {
        let mut line: Vec<u8> = buffer.drain(..=linefeed_end).collect();
        line.pop();
        if line.last() == Some(&b'\r') {
            line.pop();
        }
        if !line.is_empty() {
            events.push(SerialReceived { device, body: line });
        }
    }
    events
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum SerialError {
    DeviceNotFound,
    Other(String),
}

impl fmt::Display for SerialError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SerialError::DeviceNotFound => write!(f, "Device not found"),
            SerialError::Other(err) => write!(f, "{err}"),
        }
    }
}

pub struct SerialManager {
    connection_map: HashMap<String, SerialDeviceHandle>,
    device_id_counter: u8,
    app_state: Arc<AppState>,
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
    pub fn new(app_state: Arc<AppState>) -> Self {
        // let (midi_in_sender, midi_in_receiver) = crossbeam_channel::bounded(100);

        Self {
            device_id_counter: 0,
            connection_map: HashMap::new(),
            app_state,
            // midi_in_sender,
            // midi_in_receiver,
            // to_manager_receiver: midi_out_receiver,
            // to_plugins_sender,
        }
    }

    pub fn enumerate_devices(&self) -> Result<Vec<String>, SerialError> {
        let ports = available_ports().map_err(|e| SerialError::Other(e.to_string()))?;
        let device_names: Vec<String> = ports.into_iter().map(|p| p.port_name).collect();

        Ok(device_names)
    }

    pub fn request_device(&mut self, port_path: &str, baud_rate: u32) -> Option<u8> {
        // TODO: would normally check that this is not used by one of the DMX interfaces.

        // Check if there is already a handle for this device.
        if let Some(device_id) = self.connection_map.get(port_path).map(|dev| dev.device_id) {
            debug!(
                "Reusing existing SERIAL connection with id: {} for device: '{port_path}'",
                device_id
            );

            {
                let mut health_state = self.app_state.health_data.write().unwrap();
                health_state
                    .serial_health
                    .devices
                    .insert(port_path.to_string(), SerialDeviceState::Open(1));
            }

            return Some(device_id);
        }

        let serial_port = match serialport::new(port_path, baud_rate)
            .timeout(PORT_TIMEOUT)
            .open()
        {
            Ok(port) => port,
            Err(err) => {
                error!("Could not open serial port '{port_path}': {err}");

                let serial_error = match err.kind() {
                    SerialPortErrorKind::NoDevice => SerialError::DeviceNotFound,
                    _ => SerialError::Other(err.to_string()),
                };
                {
                    let mut health_state = self.app_state.health_data.write().unwrap();
                    health_state.serial_health.devices.insert(
                        port_path.to_string(),
                        SerialDeviceState::Error(serial_error),
                    );
                }

                return None;
            }
        };

        let Some(device_id) = self.device_id_counter.checked_add(1).map(|next| {
            let current = self.device_id_counter;
            self.device_id_counter = next;
            current
        }) else {
            error!("[SERIAL] Device ID space exhausted; refusing '{port_path}'");
            return None;
        };

        self.connection_map.insert(
            port_path.to_string(),
            SerialDeviceHandle {
                port_path: port_path.to_string(),
                device_id,
                serial_port,
                buffer: vec![],
            },
        );

        {
            let mut health_state = self.app_state.health_data.write().unwrap();
            health_state
                .serial_health
                .devices
                .insert(port_path.to_string(), SerialDeviceState::Open(1));
        }

        Some(device_id)
    }

    pub fn tick(&mut self) -> Result<Vec<SerialReceived>, SerialError> {
        let mut error: Option<SerialError> = None;

        let mut incoming_events = vec![];

        for port in self.connection_map.values_mut() {
            // Poll the port.
            let mut buf = [0u8; 10_000];
            match port.serial_port.read(&mut buf) {
                Ok(n) if n > 0 => {
                    {
                        let mut health_state = self.app_state.health_data.write().unwrap();
                        health_state
                            .serial_health
                            .devices
                            .insert(port.port_path.clone(), SerialDeviceState::Open(1));
                    }

                    let sliced_buf = &buf[..n];

                    port.buffer.extend_from_slice(sliced_buf);

                    incoming_events.extend(drain_serial_lines(&mut port.buffer, port.device_id));
                }
                Ok(_) /* n = 0 */ => {
                    let serial_error = SerialError::Other("Serial device disconnected".to_string());
                    error!("Serial device '{}' returned EOF", port.port_path);
                    if error.is_none() {
                        error = Some(serial_error.clone());
                    }
                    let mut health_state = self.app_state.health_data.write().unwrap();
                    health_state.serial_health.devices.insert(
                        port.port_path.clone(),
                        SerialDeviceState::Error(serial_error),
                    );
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::TimedOut => {
                    // No data available yet, continue polling
                }
                Err(e) => {
                    error!("Serial port error: {:?}", e);
                    let serial_error = SerialError::Other(e.to_string());
                    if error.is_none() {
                        error = Some(serial_error.clone());
                    }

                    let mut health_state = self.app_state.health_data.write().unwrap();
                    health_state.serial_health.devices.insert(
                        port.port_path.clone(),
                        SerialDeviceState::Error(serial_error),
                    );
                }
            }
        }

        if let Some(err) = error {
            Err(err)
        } else {
            Ok(incoming_events)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serial_parser_preserves_partial_lines_and_plain_text() {
        let mut buffer = b"first\nsecond".to_vec();
        let events = drain_serial_lines(&mut buffer, 3);

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].body, b"first");
        assert_eq!(buffer, b"second");

        buffer.extend_from_slice(b"\r\n");
        let events = drain_serial_lines(&mut buffer, 3);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].body, b"second");
        assert!(buffer.is_empty());
    }
}
