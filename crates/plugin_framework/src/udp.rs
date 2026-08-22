use crate::blaulicht;
use crate::error::{IoError, Result};
use crate::BufferSource;
use blaulicht_shared::{UdpCollector, UdpReceived};

const UDP_BUFFER_LEN: usize = 256 * 1024;
type UdpBufferT = u8;

static mut GLOBAL_UDP_SOURCE: BufferSource<UdpBufferT, UDP_BUFFER_LEN> = BufferSource {
    buffer: [0; UDP_BUFFER_LEN],
    current_length: 0,
};

#[no_mangle]
pub extern "C" fn __internal_get_global_udp_buffer_start_addr() -> *mut UdpBufferT {
    unsafe { &raw mut GLOBAL_UDP_SOURCE.buffer as *mut UdpBufferT }
}

#[no_mangle]
pub extern "C" fn __internal_get_global_udp_buffer_length_start_addr() -> *mut UdpBufferT {
    unsafe { &raw mut GLOBAL_UDP_SOURCE.current_length as *mut UdpBufferT }
}

fn get_udp() -> Vec<UdpReceived> {
    let curr_len = unsafe { GLOBAL_UDP_SOURCE.current_length };
    if curr_len == 0 {
        return vec![];
    }
    if curr_len as usize > UDP_BUFFER_LEN {
        return vec![];
    }

    let buf = unsafe { &GLOBAL_UDP_SOURCE.buffer[..curr_len as usize] };
    let collector = UdpCollector::deserialize(buf);
    collector.events()
}

const UDP_PORT_NOT_FOUND: u8 = u8::MAX;

#[derive(Debug, Clone, Copy)]
pub struct UdpPort {
    port_id: u8,
}

impl UdpPort {
    pub fn open(bind_port: u16) -> Result<Self> {
        let port_id = blaulicht::bl_open_udp_port_safe(bind_port);

        if port_id == UDP_PORT_NOT_FOUND {
            return Err(IoError::UdpBindFailed(bind_port).into());
        }

        Ok(Self { port_id })
    }

    /// Binds on 127.0.0.1 only (not reachable from the network).
    pub fn open_loopback(bind_port: u16) -> Result<Self> {
        let port_id = blaulicht::bl_open_udp_port_loopback_safe(bind_port);

        if port_id == UDP_PORT_NOT_FOUND {
            return Err(IoError::UdpBindFailed(bind_port).into());
        }

        Ok(Self { port_id })
    }

    pub fn poll(&self) -> Vec<UdpReceived> {
        let all_events = get_udp();
        all_events
            .into_iter()
            .filter(|e| e.port_id == self.port_id)
            .collect()
    }
}
