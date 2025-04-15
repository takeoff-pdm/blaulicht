use crate::{app::FromFrontend, audio::{Signal, SystemMessage}};

pub struct AppState {
    pub from_frontend_sender: crossbeam_channel::Sender<FromFrontend>,
    pub app_signal_receiver: crossbeam_channel::Receiver<Signal>,
    pub app_system_receiver: crossbeam_channel::Receiver<SystemMessage>,
}
