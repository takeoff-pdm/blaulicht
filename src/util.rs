use thread_priority::ThreadPriority;

pub fn increase_thread_priority() {
    match thread_priority::set_current_thread_priority(ThreadPriority::Max) {
        Ok(_) => log::info!("SUCCESS: set thread priority"),
        Err(err) => {
            log::error!("FAILED: set thread priority: {err}");
        }
    }
}

pub fn map(x: isize, in_min: isize, in_max: isize, out_min: isize, out_max: isize) -> usize {
    let divisor = (in_max - in_min).max(1);
    ((x - in_min) * (out_max - out_min) / (divisor) + out_min).max(0) as usize
}
