use std::cell::{Cell, RefCell};

use blaulicht_shared::{engine::EngineState, ENGINE_STATE_BUFFER_LEN};

use crate::BufferSource;

const STATE_BUFFER_LEN: usize = ENGINE_STATE_BUFFER_LEN;
type StateBufferT = u8;

static mut GLOBAL_STATE_SOURCE: BufferSource<StateBufferT, STATE_BUFFER_LEN> = BufferSource {
    buffer: [0; STATE_BUFFER_LEN],
    current_length: 0,
};

// Function called by the engine to get the location of the state buffer.
// The engine will write its state into this buffer.
#[no_mangle]
pub extern "C" fn __internal_get_global_state_buffer_start_addr() -> *mut StateBufferT {
    unsafe { &raw mut GLOBAL_STATE_SOURCE.buffer as *mut StateBufferT }
}

// Same as the above, just for the length of the buffer.
#[no_mangle]
pub extern "C" fn __internal_get_global_state_buffer_length_start_addr() -> *mut StateBufferT {
    unsafe { &raw mut GLOBAL_STATE_SOURCE.current_length as *mut StateBufferT }
}

/// Decoding the engine snapshot is by far the most expensive thing a plugin can
/// do per tick: it rebuilds every group, scene and palette out of bincode. The
/// host rewrites the buffer only when the snapshot actually changes, so the
/// decoded value is cached and reused until the bytes differ.
struct StateCache {
    /// FNV-1a digest of the bytes `state` was decoded from.
    digest: Option<u64>,
    state: EngineState,
}

thread_local! {
    static STATE_CACHE: RefCell<StateCache> = RefCell::new(StateCache {
        digest: None,
        state: EngineState::default(),
    });

    /// Whether the host buffer was already checked during this tick. It lives
    /// outside the `RefCell` so that a nested [`with_dmx`] never has to take a
    /// mutable borrow while an outer one still holds a shared borrow.
    static CHECKED_THIS_TICK: Cell<bool> = const { Cell::new(false) };
}

fn fnv1a(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

/// Called by the framework at the start of each tick so the next state access
/// re-checks the host buffer exactly once.
pub(crate) fn invalidate_state_cache() {
    CHECKED_THIS_TICK.with(|checked| checked.set(false));
}

fn current_buffer() -> Option<&'static [StateBufferT]> {
    // Sanity check for memory usage.
    let curr_len = unsafe { GLOBAL_STATE_SOURCE.current_length };
    // Initialization runs before the host publishes its first engine snapshot.
    if curr_len == 0 || curr_len as usize > STATE_BUFFER_LEN {
        return None;
    }
    Some(unsafe { &GLOBAL_STATE_SOURCE.buffer[..curr_len as usize] })
}

fn refresh(cache: &mut StateCache) {
    let Some(buf) = current_buffer() else {
        if cache.digest.is_some() {
            cache.digest = None;
            cache.state = EngineState::default();
        }
        return;
    };

    let digest = fnv1a(buf);
    if cache.digest == Some(digest) {
        return;
    }
    cache.digest = Some(digest);
    cache.state = EngineState::deserialize(buf);
}

/// Runs `f` against the current engine state without copying it.
///
/// Prefer this over [`get_dmx`] in code that runs every tick: `get_dmx` has to
/// clone the whole state so it can hand back an owned value.
pub fn with_dmx<T>(f: impl FnOnce(&EngineState) -> T) -> T {
    // The refresh takes a mutable borrow, so it has to finish before the
    // closure runs: the closure may itself call back into `with_dmx`.
    let needs_refresh = CHECKED_THIS_TICK.with(|checked| !checked.replace(true));
    if needs_refresh {
        STATE_CACHE.with(|cache| refresh(&mut cache.borrow_mut()));
    }
    STATE_CACHE.with(|cache| f(&cache.borrow().state))
}

/// Returns an owned copy of the current engine state.
///
/// This clones the entire state. In per-tick code use [`with_dmx`] instead.
pub fn get_dmx() -> EngineState {
    with_dmx(|state| state.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_initial_snapshot_returns_default_state() {
        assert_eq!(get_dmx().serialize(), EngineState::default().serialize());
    }

    #[test]
    fn repeated_reads_are_consistent() {
        invalidate_state_cache();
        let first = get_dmx().serialize();
        let second = with_dmx(|state| state.serialize());
        assert_eq!(first, second);
    }

    #[test]
    fn nested_access_does_not_panic() {
        invalidate_state_cache();
        let focus = with_dmx(|outer| {
            let inner = with_dmx(|state| state.current_scene_focus);
            assert_eq!(inner, outer.current_scene_focus);
            inner
        });
        assert_eq!(focus, EngineState::default().current_scene_focus);
    }

    #[test]
    fn digest_distinguishes_contents() {
        assert_ne!(fnv1a(b"abc"), fnv1a(b"abd"));
        assert_eq!(fnv1a(b"abc"), fnv1a(b"abc"));
    }
}
