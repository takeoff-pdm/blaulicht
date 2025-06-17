use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};


use crate::{app::FromFrontend, audio::UnifiedMessage};

pub struct AppState {
    pub from_frontend_sender: crossbeam_channel::Sender<FromFrontend>,
    // pub app_signal_receiver: crossbeam_channel::Receiver<Signal>,
    // pub app_system_receiver: crossbeam_channel::Receiver<SystemMessage>,
    pub to_frontend_consumers: Arc<Mutex<HashMap<String, crossbeam_channel::Sender<UnifiedMessage>>>>,
}
