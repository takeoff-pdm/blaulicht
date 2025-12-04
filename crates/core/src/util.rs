use thread_priority::ThreadPriority;

pub fn increase_thread_priority() {
    match thread_priority::set_current_thread_priority(ThreadPriority::Max) {
        Ok(_) => log::info!("SUCCESS: set thread priority"),
        Err(err) => {
            log::error!("FAILED: set thread priority: {err}");
        }
    }
}
