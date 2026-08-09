use bincode::{Decode, Encode, config};

#[derive(Encode, Decode, Clone, Debug)]
pub struct UdpReceived {
    pub port_id: u8,
    pub body: Vec<u8>,
}

#[derive(Encode, Decode)]
pub struct UdpCollector {
    events: Vec<UdpReceived>,
}

impl UdpCollector {
    pub fn new(events: Vec<UdpReceived>) -> Self {
        Self { events }
    }

    pub fn serialize(&self) -> Vec<u8> {
        bincode::encode_to_vec(self, config::standard()).unwrap()
    }

    pub fn deserialize(buf: &[u8]) -> Self {
        let res = bincode::decode_from_slice(buf, config::standard());

        let data = match res {
            Ok((data, _)) => data,
            Err(err) => {
                panic!("UdpCollector deserialize error: {}", err);
            }
        };

        data
    }

    pub fn events(self) -> Vec<UdpReceived> {
        self.events
    }
}
