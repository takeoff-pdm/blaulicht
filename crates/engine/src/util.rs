use audio_thread_priority::promote_current_thread_to_real_time;
use thread_priority::ThreadPriority;

pub fn increase_thread_priority() {
    // realtime();
    conservative();
}

fn conservative() {
    match thread_priority::set_current_thread_priority(ThreadPriority::Max) {
        Ok(_) => tracing::info!("Raised engine thread priority"),
        Err(err) => tracing::warn!("Failed to raise engine thread priority: {err}"),
    }
}

#[allow(dead_code)]
fn realtime() {
    match promote_current_thread_to_real_time(512, 44100) {
        Ok(_h) => tracing::info!("Raised engine thread priority to realtime"),
        Err(e) => tracing::warn!("Failed to raise engine thread priority to realtime: {e}"),
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
