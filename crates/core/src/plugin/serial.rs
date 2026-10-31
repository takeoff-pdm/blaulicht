use blaulicht_shared::SerialReceived;
use serialport::{available_ports, ErrorKind as SerialPortErrorKind, SerialPort};
use std::collections::HashMap;
use std::fmt;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, error};
// use wmidi::MidiMessage;

use crate::state::{AppState, SerialDeviceState};

const PORT_TIMEOUT: Duration = Duration::from_millis(3);

/// Directories searched for symlinks to serial ports. `serialport` only
/// reports kernel names (`/dev/ttyUSB0`), which can change across replugs,
/// while udev rules add stable names here (`/dev/antenna`, by-id links).
const SYMLINK_DIRS: &[&str] = &["/dev", "/dev/serial/by-id"];

/// Returns the symlinks in `dirs` that resolve to one of `ports`, sorted.
fn symlinks_to_ports(ports: &[String], dirs: &[&str]) -> Vec<String> {
    let targets: Vec<PathBuf> = ports
        .iter()
        .filter_map(|port| std::fs::canonicalize(port).ok())
        .collect();
    if targets.is_empty() {
        return Vec::new();
    }

    let mut links: Vec<String> = dirs
        .iter()
        .filter_map(|dir| std::fs::read_dir(dir).ok())
        .flatten()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_symlink()))
        .map(|entry| entry.path())
        // `/dev/stdin` and friends point into `/proc/self` and resolve to
        // whatever the engine's stdio is attached to.
        .filter(|path| std::fs::read_link(path).is_ok_and(|target| !target.starts_with("/proc")))
        .filter(|path| std::fs::canonicalize(path).is_ok_and(|target| targets.contains(&target)))
        .filter_map(|path| path.to_str().map(str::to_string))
        .collect();
    links.sort();
    links
}

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
    /// Ports owned by the DMX engine. Plugins can neither list nor open them.
    reserved_ports: Vec<String>,
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
            reserved_ports: Vec::new(),
            device_id_counter: 0,
            connection_map: HashMap::new(),
            app_state,
            // midi_in_sender,
            // midi_in_receiver,
            // to_manager_receiver: midi_out_receiver,
            // to_plugins_sender,
        }
    }

    /// Reserves `ports` (e.g. the DMX outputs from the config) for the host.
    pub fn with_reserved_ports(mut self, ports: impl IntoIterator<Item = String>) -> Self {
        self.reserved_ports = ports.into_iter().collect();
        self
    }

    /// Whether `port_path` resolves to the same device as a reserved port.
    /// Compares canonical paths, so `/dev/dmx_out_1` also covers `/dev/ttyUSB0`.
    fn is_reserved(&self, port_path: &str) -> bool {
        let canonical = |path: &str| std::fs::canonicalize(path).ok();
        let target = canonical(port_path);
        self.reserved_ports.iter().any(|reserved| {
            reserved == port_path || (target.is_some() && canonical(reserved) == target)
        })
    }

    /// Lists the serial ports plugins may open: stable symlinks such as
    /// `/dev/antenna` or `/dev/serial/by-id/...` first, then kernel names.
    pub fn enumerate_devices(&self) -> Result<Vec<String>, SerialError> {
        let ports: Vec<String> = available_ports()
            .map_err(|e| SerialError::Other(e.to_string()))?
            .into_iter()
            .map(|p| p.port_name)
            .collect();

        let mut device_names = symlinks_to_ports(&ports, SYMLINK_DIRS);
        device_names.extend(ports);
        device_names.retain(|name| !self.is_reserved(name));

        Ok(device_names)
    }

    pub fn request_device(&mut self, port_path: &str, baud_rate: u32) -> Option<u8> {
        if self.is_reserved(port_path) {
            error!("Refusing serial port '{port_path}': it is a DMX output");
            {
                let mut health_state = self.app_state.health_data.write().unwrap();
                health_state.serial_health.devices.insert(
                    port_path.to_string(),
                    SerialDeviceState::Error(SerialError::Other(
                        "Reserved for DMX output".to_string(),
                    )),
                );
            }
            return None;
        }

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
            error!("Device ID space exhausted; refusing '{port_path}'");
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
        // Ports that failed this tick: they are removed from the connection map
        // instead of erroring out, so a single unplugged device cannot take down
        // the engine mainloop.
        let mut dead_ports: Vec<String> = vec![];

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
                    dead_ports.push(port.port_path.clone());
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
                    dead_ports.push(port.port_path.clone());

                    let mut health_state = self.app_state.health_data.write().unwrap();
                    health_state.serial_health.devices.insert(
                        port.port_path.clone(),
                        SerialDeviceState::Error(serial_error),
                    );
                }
            }
        }

        for port_path in dead_ports {
            self.connection_map.remove(&port_path);
        }

        Ok(incoming_events)
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

    #[cfg(unix)]
    #[test]
    fn symlinks_to_ports_finds_aliases_of_listed_ports_only() {
        use std::os::unix::fs::symlink;

        let root = std::env::temp_dir().join(format!("bl_serial_links_{}", std::process::id()));
        let by_id = root.join("serial").join("by-id");
        std::fs::create_dir_all(&by_id).unwrap();
        let tty = root.join("ttyUSB0");
        let other = root.join("ttyUSB1");
        std::fs::write(&tty, b"").unwrap();
        std::fs::write(&other, b"").unwrap();
        symlink(&tty, root.join("antenna")).unwrap();
        symlink("../../ttyUSB0", by_id.join("usb-CP2102-if00")).unwrap();
        symlink(&other, root.join("unlisted")).unwrap();
        symlink(root.join("gone"), root.join("dangling")).unwrap();

        let dirs = [root.to_str().unwrap(), by_id.to_str().unwrap()];
        let links = symlinks_to_ports(&[tty.to_str().unwrap().to_string()], &dirs);
        std::fs::remove_dir_all(&root).unwrap();

        assert_eq!(
            links,
            vec![
                root.join("antenna").to_str().unwrap().to_string(),
                by_id.join("usb-CP2102-if00").to_str().unwrap().to_string(),
            ]
        );
    }
}
