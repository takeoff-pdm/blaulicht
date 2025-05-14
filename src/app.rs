use cpal::Device;
use serde::Deserialize;

#[derive(Deserialize, Clone, Debug)]
pub struct MatrixEvent {
    pub device: u8,
    pub x: u8,
    pub y: u8,
    pub value: bool,
}

#[derive(Deserialize, Clone, Debug)]
pub struct MidiEvent {
    pub device: u8,
    pub status: u8,
    pub data0: u8,
    pub data1: u8,
}

#[derive(Clone)]
pub enum FromFrontend {
    Reload,
    SelectInputDevice(Option<Device>),
    SelectSerialDevice(Option<String>),
    MatrixControl(MatrixEvent),
}