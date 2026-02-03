use crate::msg::SystemMessage;
use audio_thread_priority::{
    demote_current_thread_from_real_time, promote_current_thread_to_real_time,
};
use blaulicht_shared::LogLevel;
use crossbeam_channel::Sender;
use thread_priority::ThreadPriority;

pub fn increase_thread_priority(system_sender: Sender<SystemMessage>) {
    // realtime(system_sender);
    conservative(system_sender);
}

fn conservative(system_sender: Sender<SystemMessage>) {
    match thread_priority::set_current_thread_priority(ThreadPriority::Max) {
        Ok(_) => log::info!("SUCCESS: set thread priority"),
        Err(err) => {
            let msg = format!("FAILED: set thread priority: {err}");
            log::error!("{msg}");
            system_sender.send(SystemMessage::Log(msg, LogLevel::Warn));
        }
    }
}

fn realtime(system_sender: Sender<SystemMessage>) {
    match promote_current_thread_to_real_time(512, 44100) {
        Ok(h) => {
            log::info!("SUCCESS: set thread priority to REALTIME");
        }
        Err(e) => {
            let msg = format!("FAILED: set thread priority to REALTIME: {e}");
            log::error!("{msg}");
            system_sender.send(SystemMessage::Log(msg, LogLevel::Warn));
        }
    }

    // // Do some real-time work...
    // match demote_current_thread_from_real_time(h) {
    //     Ok(_) => {
    //         println!("this thread is now bumped back to normal.")
    //     }
    //     Err(_) => {
    //         println!("Could not bring the thread back to normal priority.")
    //     }
    // };
}
