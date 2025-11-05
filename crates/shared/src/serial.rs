use bincode::{Decode, Encode, config};
use serde::Deserialize;

#[derive(Encode, Decode, Deserialize, Clone, Debug)]
pub struct SerialReceived {
    pub device: u8,
    pub body: Vec<u8>,
}

//
//
//

#[derive(Encode, Decode)]
pub struct SerialCollector {
    events: Vec<SerialReceived>,
}

impl SerialCollector {
    pub fn new(events: Vec<SerialReceived>) -> Self {
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
                // Panic handler is usually not registered yet.
                panic!("Deserialize error: {}", err);
            }
        };

        data
    }

    pub fn events(self) -> Vec<SerialReceived> {
        self.events
    }
}
