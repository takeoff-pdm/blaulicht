use cpal::Device;
use serde::Deserialize;

#[derive(Deserialize, Clone, Debug)]
pub struct WSMatrixControlBody {
    pub x: u8,
    pub y: u8,
    pub value: bool,
}

#[derive(Clone)]
pub enum FromFrontend {
    Reload,
    SelectInputDevice(Option<Device>),
    SelectSerialDevice(Option<String>),
    MatrixControl(WSMatrixControlBody),
}