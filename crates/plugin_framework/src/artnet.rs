use crate::blaulicht;
use blaulicht_shared::ArtNetReceiverInfo;

/// RAII handle for a plugin-owned Art-Net receiver.
///
/// While alive, the core's DMX engine ships all rendered universes to `addr` on
/// every tick. When dropped, the receiver is unregistered from the core.
///
/// The host-side cleanup (on plugin crash / reload) is the real safety net —
/// this `Drop` impl just handles graceful, in-plugin lifecycle cases.
#[derive(Debug)]
pub struct ArtNetReceiverHandle {
    handle: u32,
    addr: String,
}

impl ArtNetReceiverHandle {
    /// Register `addr` (format: `"ip:port"`, IPv4) as an Art-Net receiver owned
    /// by this plugin. Returns `None` if registration failed (bad address,
    /// duplicate registration, etc.).
    pub fn register(addr: impl Into<String>) -> Option<Self> {
        let addr = addr.into();
        let handle = blaulicht::bl_artnet_register_receiver_safe(&addr);
        if handle == 0 {
            None
        } else {
            Some(Self { handle, addr })
        }
    }

    pub fn handle(&self) -> u32 {
        self.handle
    }

    pub fn addr(&self) -> &str {
        &self.addr
    }

    /// Enumerate every Art-Net receiver the host currently knows about — both
    /// user-created and plugin-owned. Useful for sanity-checking your own
    /// registrations or inspecting host state.
    pub fn enumerate_all() -> Vec<ArtNetReceiverInfo> {
        blaulicht::bl_artnet_enumerate_receivers_safe()
    }
}

impl Drop for ArtNetReceiverHandle {
    fn drop(&mut self) {
        let _ = blaulicht::bl_artnet_unregister_receiver_safe(self.handle);
    }
}
