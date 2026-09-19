//! [`prolink::adapter::Transport`] over the blaulicht plugin host.
//!
//! Receiving goes through three [`UdpPort`]s (one per Pro DJ Link channel)
//! that the host fills once per tick. Sending goes through the host's shared
//! outbound socket via [`bpf::send_udp`].

use std::collections::VecDeque;
use std::net::Ipv4Addr;

use blaulicht_plugin_framework as bpf;
use blaulicht_plugin_framework::UdpPort;
use prolink::adapter::{Channel, Destination, Received, Transport};
use prolink::wire::MacAddr;

/// One datagram pulled from the host, waiting for the session to read it.
struct Pending {
    channel: Channel,
    source: Ipv4Addr,
    body: Vec<u8>,
}

pub struct BlaulichtTransport {
    ports: [Option<UdpPort>; 3],
    pending: VecDeque<Pending>,
    identity: Option<(Ipv4Addr, MacAddr)>,
}

/// Sending through the host never reports failure (the host logs it), and the
/// receive path cannot fail either, so this is never constructed.
#[derive(Debug)]
pub enum Never {}

impl BlaulichtTransport {
    /// Binds all three channels. Returns the bind errors (one per failed port)
    /// alongside the transport so the UI can show them; a channel that failed
    /// to bind simply never receives anything.
    pub fn bind() -> (Self, Vec<String>) {
        let mut ports = [None, None, None];
        let mut errors = Vec::new();
        for channel in Channel::ALL {
            match UdpPort::open(channel.port()) {
                Ok(port) => ports[channel as usize] = Some(port),
                Err(e) => errors.push(format!("port {}: {e}", channel.port())),
            }
        }
        (
            Self {
                ports,
                pending: VecDeque::new(),
                identity: None,
            },
            errors,
        )
    }

    /// A transport that owns no ports. Used as a placeholder while the real
    /// transport is moved between sessions.
    pub fn detached() -> Self {
        Self {
            ports: [None, None, None],
            pending: VecDeque::new(),
            identity: None,
        }
    }

    pub fn is_bound(&self) -> bool {
        self.ports.iter().all(Option::is_some)
    }

    pub fn set_identity(&mut self, identity: Option<(Ipv4Addr, MacAddr)>) {
        self.identity = identity;
    }

    /// Pulls this tick's datagrams from the host. Call once per engine tick,
    /// before draining the session.
    pub fn collect(&mut self) -> usize {
        let mut count = 0;
        // Announce first: the library asks adapters to prioritise liveness.
        for channel in Channel::ALL {
            let Some(port) = &self.ports[channel as usize] else { continue };
            for packet in port.poll() {
                let source = packet
                    .src_addr
                    .rsplit_once(':')
                    .and_then(|(ip, _)| ip.parse().ok())
                    .unwrap_or(Ipv4Addr::UNSPECIFIED);
                self.pending.push_back(Pending {
                    channel,
                    source,
                    body: packet.body,
                });
                count += 1;
            }
        }
        count
    }
}

impl Transport for BlaulichtTransport {
    type Error = Never;

    fn send(&mut self, channel: Channel, dest: Destination, payload: &[u8]) -> Result<(), Never> {
        let ip = match dest {
            Destination::Broadcast => Ipv4Addr::BROADCAST,
            Destination::Unicast(ip) => ip,
        };
        bpf::send_udp(&format!("{ip}:{}", channel.port()), payload);
        Ok(())
    }

    fn poll_recv(&mut self, buf: &mut [u8]) -> Result<Option<Received>, Never> {
        let Some(datagram) = self.pending.pop_front() else {
            return Ok(None);
        };
        // Oversize datagrams are truncated, as the trait contract asks.
        let len = datagram.body.len().min(buf.len());
        buf[..len].copy_from_slice(&datagram.body[..len]);
        Ok(Some(Received {
            channel: datagram.channel,
            source: datagram.source,
            len,
        }))
    }

    fn local_ipv4(&self) -> Option<Ipv4Addr> {
        self.identity.map(|(ip, _)| ip)
    }

    fn local_mac(&self) -> Option<MacAddr> {
        self.identity.map(|(_, mac)| mac)
    }
}
