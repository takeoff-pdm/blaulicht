//! [`prolink::adapter::Transport`] over the blaulicht plugin host.
//!
//! Receiving goes through three [`UdpPort`]s (one per Pro DJ Link channel)
//! that the host fills once per tick. Sending goes through the host's shared
//! outbound socket via [`bpf::send_udp`].
//!
//! Broadcasts are sent to the *directed* broadcast address of the interface
//! we identify as (e.g. `10.10.25.255`), not to `255.255.255.255`: the latter
//! leaves through the default route, which on a machine with wifi and a LINK
//! cable is usually the wrong interface, and the CDJs never hear us.

use std::collections::VecDeque;
use std::net::Ipv4Addr;

use blaulicht_plugin_framework as bpf;
use blaulicht_plugin_framework::UdpPort;
use prolink::adapter::{Channel, Destination, Received, Transport};
use prolink::wire::MacAddr;

/// The interface we present ourselves as on the LINK network.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Identity {
    pub ip: Ipv4Addr,
    pub prefix_len: u8,
    pub broadcast: Ipv4Addr,
    pub mac: MacAddr,
}

impl Identity {
    /// Builds an identity; the directed broadcast is derived from the prefix
    /// when the caller does not know it.
    pub fn new(ip: Ipv4Addr, prefix_len: u8, mac: MacAddr, broadcast: Option<Ipv4Addr>) -> Self {
        let prefix_len = prefix_len.min(32);
        let broadcast = broadcast.unwrap_or_else(|| {
            let mask = Self::mask(prefix_len);
            Ipv4Addr::from(u32::from(ip) | !mask)
        });
        Self {
            ip,
            prefix_len,
            broadcast,
            mac,
        }
    }

    fn mask(prefix_len: u8) -> u32 {
        if prefix_len == 0 {
            0
        } else {
            u32::MAX << (32 - prefix_len as u32)
        }
    }

    /// Whether `other` lies in this interface's subnet.
    pub fn contains(&self, other: Ipv4Addr) -> bool {
        let mask = Self::mask(self.prefix_len);
        (u32::from(self.ip) & mask) == (u32::from(other) & mask)
    }
}

impl std::fmt::Display for Identity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}/{} {}", self.ip, self.prefix_len, self.mac)
    }
}

/// One datagram pulled from the host, waiting for the session to read it.
struct Pending {
    channel: Channel,
    source: Ipv4Addr,
    body: Vec<u8>,
}

pub struct BlaulichtTransport {
    ports: [Option<UdpPort>; 3],
    pending: VecDeque<Pending>,
    identity: Option<Identity>,
    transmit_enabled: bool,
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
                transmit_enabled: false,
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
            transmit_enabled: false,
        }
    }

    pub fn is_bound(&self) -> bool {
        self.ports.iter().all(Option::is_some)
    }

    pub fn set_identity(&mut self, identity: Option<Identity>) {
        self.identity = identity;
    }

    pub fn set_transmit_enabled(&mut self, enabled: bool) {
        self.transmit_enabled = enabled;
    }

    /// Pulls this tick's datagrams from the host. Call once per engine tick,
    /// before draining the session.
    pub fn collect(&mut self) -> usize {
        let mut count = 0;
        // Announce first: the library asks adapters to prioritise liveness.
        for channel in Channel::ALL {
            let Some(port) = &self.ports[channel as usize] else {
                continue;
            };
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
        // A receive-only transport is the default even if the protocol library
        // attempts a response. Only an explicit Join may open this gate.
        if !self.transmit_enabled {
            return Ok(());
        }
        let ip = match dest {
            Destination::Broadcast => self
                .identity
                .map(|i| i.broadcast)
                .unwrap_or(Ipv4Addr::BROADCAST),
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
        self.identity.map(|i| i.ip)
    }

    fn local_mac(&self) -> Option<MacAddr> {
        self.identity.map(|i| i.mac)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MAC: MacAddr = MacAddr([0x38, 0xf3, 0xab, 0xbc, 0x44, 0xe1]);

    #[test]
    fn broadcast_is_derived_from_the_prefix() {
        let id = Identity::new(Ipv4Addr::new(10, 10, 25, 95), 24, MAC, None);
        assert_eq!(id.broadcast, Ipv4Addr::new(10, 10, 25, 255));
        let id = Identity::new(Ipv4Addr::new(169, 254, 3, 7), 16, MAC, None);
        assert_eq!(id.broadcast, Ipv4Addr::new(169, 254, 255, 255));
    }

    #[test]
    fn subnet_membership() {
        let id = Identity::new(Ipv4Addr::new(10, 10, 25, 95), 24, MAC, None);
        assert!(id.contains(Ipv4Addr::new(10, 10, 25, 58)));
        assert!(!id.contains(Ipv4Addr::new(10, 10, 42, 63)));
        let any = Identity::new(Ipv4Addr::new(1, 2, 3, 4), 0, MAC, None);
        assert!(any.contains(Ipv4Addr::new(9, 9, 9, 9)));
    }
}
