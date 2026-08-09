use blaulicht_shared::UdpReceived;
use std::collections::HashMap;
use std::fmt;
use std::net::UdpSocket;
use std::sync::Arc;
use tracing::{debug, error, info};

use crate::state::AppState;

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum UdpError {
    BindFailed(String),
    Other(String),
}

impl fmt::Display for UdpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            UdpError::BindFailed(err) => write!(f, "Bind failed: {err}"),
            UdpError::Other(err) => write!(f, "{err}"),
        }
    }
}

pub struct UdpManager {
    connection_map: HashMap<u16, UdpPortHandle>,
    port_id_counter: u8,
    app_state: Arc<AppState>,
}

struct UdpPortHandle {
    port_id: u8,
    bind_port: u16,
    socket: UdpSocket,
}

impl UdpManager {
    pub fn new(app_state: Arc<AppState>) -> Self {
        Self {
            port_id_counter: 0,
            connection_map: HashMap::new(),
            app_state,
        }
    }

    pub fn request_port(&mut self, bind_port: u16) -> Option<u8> {
        if let Some(handle) = self.connection_map.get(&bind_port) {
            debug!(
                "Reusing existing UDP port with id: {} for port: {}",
                handle.port_id, bind_port
            );
            return Some(handle.port_id);
        }

        let addr = format!("0.0.0.0:{}", bind_port);
        let socket = match UdpSocket::bind(&addr) {
            Ok(s) => {
                if let Err(err) = s.set_nonblocking(true) {
                    error!("[UDP] Failed to set {} non-blocking: {}", addr, err);
                    return None;
                }
                info!("[UDP] Listening on {}", addr);
                s
            }
            Err(err) => {
                error!("[UDP] Failed to bind to {}: {}", addr, err);
                return None;
            }
        };

        let Some(port_id) = self.port_id_counter.checked_add(1).map(|next| {
            let current = self.port_id_counter;
            self.port_id_counter = next;
            current
        }) else {
            error!("[UDP] Port ID space exhausted; refusing {}", addr);
            return None;
        };

        self.connection_map.insert(
            bind_port,
            UdpPortHandle {
                port_id,
                bind_port,
                socket,
            },
        );

        Some(port_id)
    }

    pub fn release_all(&mut self) {
        let count = self.connection_map.len();
        self.connection_map.clear();
        self.port_id_counter = 0;
        if count > 0 {
            info!("[UDP] Released {} port(s)", count);
        }
    }

    pub fn tick(&mut self) -> Result<Vec<UdpReceived>, UdpError> {
        let mut incoming_events = vec![];

        for handle in self.connection_map.values_mut() {
            let mut buf = [0u8; 65535];
            loop {
                match handle.socket.recv_from(&mut buf) {
                    Ok((n, _addr)) => {
                        incoming_events.push(UdpReceived {
                            port_id: handle.port_id,
                            body: buf[..n].to_vec(),
                        });
                    }
                    Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        break;
                    }
                    Err(e) => {
                        error!("[UDP] Receive error on port {}: {}", handle.bind_port, e);
                        break;
                    }
                }
            }
        }

        Ok(incoming_events)
    }
}
