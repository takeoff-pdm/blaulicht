use crate::msg::SystemMessage;
use blaulicht_shared::LogLevel;
use crossbeam_channel::Sender;
use thread_priority::ThreadPriority;

pub fn increase_thread_priority(system_sender: Sender<SystemMessage>) {
    match thread_priority::set_current_thread_priority(ThreadPriority::Max) {
        Ok(_) => log::info!("SUCCESS: set thread priority"),
        Err(err) => {
            let msg = format!("FAILED: set thread priority: {err}");
            log::error!("{msg}");
            system_sender.send(SystemMessage::Log(msg, LogLevel::Warn));
        }
    }
}
