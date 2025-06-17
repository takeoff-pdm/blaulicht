use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use crate::{app::FromFrontend, msg::UnifiedMessage};

pub struct AppState {
    pub from_frontend_sender: crossbeam_channel::Sender<FromFrontend>,
    pub to_frontend_consumers:
        Arc<Mutex<HashMap<String, crossbeam_channel::Sender<UnifiedMessage>>>>,
}
