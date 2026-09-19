//! Finding the interface that is on the LINK network.
//!
//! A WASM plugin cannot enumerate network interfaces, but the host can run a
//! shell command for us, so this asks `ip -j addr` and picks the interface
//! whose subnet contains a player we have already heard from.

use std::net::Ipv4Addr;

use blaulicht_plugin_framework as bpf;
use blaulicht_shared::LogLevel;
use prolink::wire::MacAddr;
use serde::Deserialize;

use crate::transport::Identity;

// `ip -j addr` output. Note that `-4` must not be passed: it drops the
// `link_type` and `address` fields, so IPv4 entries are filtered here instead.
#[derive(Deserialize)]
struct IpAddrInfo {
    #[serde(default)]
    family: String,
    #[serde(default)]
    local: String,
    #[serde(default)]
    prefixlen: u8,
    #[serde(default)]
    broadcast: Option<String>,
}

#[derive(Deserialize)]
struct IpLink {
    #[serde(default)]
    ifname: String,
    #[serde(default)]
    link_type: String,
    #[serde(default)]
    address: Option<String>,
    #[serde(default)]
    addr_info: Vec<IpAddrInfo>,
}

/// One IPv4 address on one ethernet interface.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Iface {
    pub name: String,
    pub identity: Identity,
}

/// Every ethernet interface with an IPv4 address, in the order `ip` lists them.
pub fn discover_ifaces() -> Vec<Iface> {
    let out = bpf::system("ip -j addr show up 2>/dev/null");
    if out.trim().is_empty() {
        bpf::bl_log(
            "Pro DJ Link: `ip -j addr` produced no output; cannot detect the LINK interface",
            LogLevel::Err,
        );
        return Vec::new();
    }
    match parse_ifaces(&out) {
        Ok(ifaces) => ifaces,
        Err(e) => {
            bpf::bl_log(
                &format!("Pro DJ Link: cannot parse `ip -j addr`: {e}"),
                LogLevel::Err,
            );
            Vec::new()
        }
    }
}

fn parse_ifaces(json: &str) -> Result<Vec<Iface>, serde_json::Error> {
    let links: Vec<IpLink> = serde_json::from_str(json.trim())?;
    let mut ifaces = Vec::new();
    for link in links {
        // Loopback, tunnels and wireguard have no hardware address; the CDJs
        // are on a cable.
        if link.link_type != "ether" {
            continue;
        }
        let Some(mac) = link
            .address
            .as_deref()
            .and_then(|m| m.parse::<MacAddr>().ok())
        else {
            continue;
        };
        for addr in &link.addr_info {
            if addr.family != "inet" {
                continue;
            }
            let Ok(ip) = addr.local.parse::<Ipv4Addr>() else {
                continue;
            };
            // Link-local (169.254/16) is kept on purpose: players without DHCP
            // self-assign there.
            let broadcast = addr.broadcast.as_deref().and_then(|b| b.parse().ok());
            ifaces.push(Iface {
                name: link.ifname.clone(),
                identity: Identity::new(ip, addr.prefixlen, mac, broadcast),
            });
        }
    }
    Ok(ifaces)
}

/// The interface to join through: one that shares a subnet with a player we
/// have heard from, else the first wired one, else anything.
pub fn choose_iface<'a>(ifaces: &'a [Iface], device_ips: &[Ipv4Addr]) -> Option<&'a Iface> {
    ifaces
        .iter()
        .find(|i| device_ips.iter().any(|ip| i.identity.contains(*ip)))
        .or_else(|| ifaces.iter().find(|i| !i.name.starts_with("wl")))
        .or_else(|| ifaces.first())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"[
      {"ifindex":1,"ifname":"lo","link_type":"loopback","address":"00:00:00:00:00:00",
       "addr_info":[{"family":"inet","local":"127.0.0.1","prefixlen":8,"scope":"host"}]},
      {"ifindex":3,"ifname":"wlp3s0","link_type":"ether","address":"74:4c:a1:b1:89:5b",
       "addr_info":[{"family":"inet","local":"10.22.8.176","prefixlen":20,"broadcast":"10.22.15.255"}]},
      {"ifindex":2,"ifname":"enp2s0f0","link_type":"ether","address":"38:f3:ab:bc:44:e1",
       "addr_info":[{"family":"inet","local":"10.10.25.95","prefixlen":24,"broadcast":"10.10.25.255"},
                    {"family":"inet6","local":"fe80::1","prefixlen":64}]},
      {"ifindex":9,"ifname":"tailscale0","link_type":"none",
       "addr_info":[{"family":"inet","local":"100.106.104.44","prefixlen":32}]}
    ]"#;

    #[test]
    fn only_ethernet_ipv4_entries_survive() {
        let ifaces = parse_ifaces(SAMPLE).unwrap();
        let names: Vec<&str> = ifaces.iter().map(|i| i.name.as_str()).collect();
        assert_eq!(names, ["wlp3s0", "enp2s0f0"]);
        let eth = &ifaces[1].identity;
        assert_eq!(eth.ip, Ipv4Addr::new(10, 10, 25, 95));
        assert_eq!(eth.broadcast, Ipv4Addr::new(10, 10, 25, 255));
        assert_eq!(eth.mac.to_string(), "38:f3:ab:bc:44:e1");
    }

    #[test]
    fn a_seen_device_picks_its_subnet_and_wifi_is_a_last_resort() {
        let ifaces = parse_ifaces(SAMPLE).unwrap();
        assert_eq!(choose_iface(&ifaces, &[]).unwrap().name, "enp2s0f0");
        let on_wifi = [Ipv4Addr::new(10, 22, 0, 9)];
        assert_eq!(choose_iface(&ifaces, &on_wifi).unwrap().name, "wlp3s0");
        assert!(choose_iface(&[], &[]).is_none());
    }

    #[test]
    fn garbage_is_an_error_not_a_panic() {
        assert!(parse_ifaces("not json").is_err());
        assert!(parse_ifaces("[]").unwrap().is_empty());
    }
}
