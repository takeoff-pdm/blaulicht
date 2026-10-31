//! The show state for remote clients: the bincode bytes and FNV-1a digest
//! the plugin tick already produces for plugins, handed over without a copy.
//!
//! The plugin tick serializes the shared engine state on every tick
//! (`plugin::tick`). While a remote client is connected
//! ([`ShowSnapshots::want`]), it offers those bytes here, and at most every
//! [`OFFER_INTERVAL`] the slot keeps them if the digest changed. Without
//! remote clients the tick only checks an atomic counter. When the plugin
//! tick doesn't run (no `wasmtime` feature, main loop stopped, tests),
//! [`ShowSnapshots::latest`] serializes the state itself under a read lock.

use std::{
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex, PoisonError, RwLock,
    },
    time::{Duration, Instant},
};

use crate::dmx::EngineState;

/// How often the slot takes new bytes from the plugin tick: the fastest rate
/// at which a client gets show snapshots.
pub const OFFER_INTERVAL: Duration = Duration::from_millis(100);
/// Bytes older than this (counted from the plugin tick's last offer) are
/// stale; the server then serializes the state itself.
pub const FRESH_FOR: Duration = Duration::from_millis(250);

/// FNV-1a over the serialized engine state, used only to detect "unchanged".
pub fn fnv1a(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

/// One serialized show state.
#[derive(Clone, Debug)]
pub struct ShowBytes {
    pub digest: u64,
    /// `blaulicht_shared::EngineState::serialize()`.
    pub bytes: Arc<Vec<u8>>,
}

#[derive(Default)]
struct Slot {
    latest: Option<ShowBytes>,
    /// When `latest` was stored.
    stored_at: Option<Instant>,
    /// When the plugin tick last offered bytes (stored or not).
    offered_at: Option<Instant>,
}

/// The latest serialized show state; lives on `AppState`.
#[derive(Default)]
pub struct ShowSnapshots {
    wanted: AtomicUsize,
    slot: Mutex<Slot>,
}

/// Keeps [`ShowSnapshots::is_wanted`] true while it lives.
pub struct SnapshotInterest<'a>(&'a ShowSnapshots);

impl Drop for SnapshotInterest<'_> {
    fn drop(&mut self) {
        self.0.wanted.fetch_sub(1, Ordering::Relaxed);
    }
}

impl ShowSnapshots {
    fn slot(&self) -> std::sync::MutexGuard<'_, Slot> {
        self.slot.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Registers a consumer; the plugin tick offers its bytes while at least
    /// one is registered.
    pub fn want(&self) -> SnapshotInterest<'_> {
        self.wanted.fetch_add(1, Ordering::Relaxed);
        SnapshotInterest(self)
    }

    pub fn is_wanted(&self) -> bool {
        self.wanted.load(Ordering::Relaxed) > 0
    }

    /// The plugin tick's bytes and their digest. Kept if someone wants them,
    /// the digest differs from the kept one and the last kept bytes are at
    /// least [`OFFER_INTERVAL`] old; dropped otherwise (they are moved here,
    /// never copied).
    pub fn offer(&self, digest: u64, bytes: Vec<u8>) {
        if !self.is_wanted() {
            return;
        }
        let now = Instant::now();
        let mut slot = self.slot();
        slot.offered_at = Some(now);
        let changed = slot.latest.as_ref().map(|latest| latest.digest) != Some(digest);
        let due = slot
            .stored_at
            .map_or(true, |stored| now.duration_since(stored) >= OFFER_INTERVAL);
        if changed && due {
            slot.latest = Some(ShowBytes {
                digest,
                bytes: Arc::new(bytes),
            });
            slot.stored_at = Some(now);
        }
    }

    /// The latest show state: the plugin tick's if it offered recently,
    /// else serialized here under `engine`'s read lock.
    pub fn latest(&self, engine: &RwLock<EngineState>) -> ShowBytes {
        let now = Instant::now();
        {
            let slot = self.slot();
            let fresh = slot
                .offered_at
                .is_some_and(|offered| now.duration_since(offered) < FRESH_FOR);
            if let (true, Some(latest)) = (fresh, &slot.latest) {
                return latest.clone();
            }
        }
        let bytes = engine
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .0
            .serialize();
        let latest = ShowBytes {
            digest: fnv1a(&bytes),
            bytes: Arc::new(bytes),
        };
        let mut slot = self.slot();
        slot.latest = Some(latest.clone());
        slot.stored_at = Some(now);
        latest
    }
}

#[cfg(test)]
mod tests {
    use std::sync::RwLock;

    use super::*;

    #[test]
    fn offers_are_kept_only_while_wanted_changed_and_due() {
        let snapshots = ShowSnapshots::default();
        let engine = RwLock::new(EngineState::default());
        snapshots.offer(1, vec![1]);
        // Nobody wants them: the slot stays empty and `latest` serializes.
        let own = snapshots.latest(&engine);
        assert_eq!(own.digest, fnv1a(&own.bytes));

        let interest = snapshots.want();
        snapshots.offer(7, vec![7]);
        // Not due yet: `latest` above stored its own bytes just now.
        assert_eq!(snapshots.latest(&engine).digest, own.digest);
        std::thread::sleep(OFFER_INTERVAL);
        snapshots.offer(7, vec![7]);
        let offered = snapshots.latest(&engine);
        assert_eq!((offered.digest, offered.bytes.as_slice()), (7, &[7u8][..]));
        // Same digest: not replaced.
        std::thread::sleep(OFFER_INTERVAL);
        snapshots.offer(7, vec![8]);
        assert_eq!(snapshots.latest(&engine).bytes.as_slice(), &[7u8][..]);

        drop(interest);
        assert!(!snapshots.is_wanted());
        // Stale (no offer for a while): serialized here again.
        std::thread::sleep(FRESH_FOR);
        assert_eq!(snapshots.latest(&engine).digest, own.digest);
    }
}
