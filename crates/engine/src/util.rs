use audio_thread_priority::promote_current_thread_to_real_time;
#[cfg(not(target_arch = "wasm32"))]
use thread_priority::ThreadPriority;

pub fn increase_thread_priority() {
    // realtime();
    conservative();
}

#[cfg(not(target_arch = "wasm32"))]
fn conservative() {
    match thread_priority::set_current_thread_priority(ThreadPriority::Max) {
        Ok(_) => tracing::info!("Raised engine thread priority"),
        Err(err) => tracing::warn!("Failed to raise engine thread priority: {err}"),
    }
}

/// wasm32 builds of the engine never run its threads (see `plugin::midi`).
#[cfg(target_arch = "wasm32")]
fn conservative() {}

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
