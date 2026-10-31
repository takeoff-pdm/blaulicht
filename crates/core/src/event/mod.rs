//! The event bus: every message sent by one connection is broadcast to every
//! connection, the sender included.
//!
//! Connections can be added (with an [`EventBusHandle`]) and removed (by
//! dropping the last clone of a [`SystemEventBusConnection`]) while the bus
//! runs. Each connection has its own bounded queue. When a connection doesn't
//! keep up, the bus drops that connection's oldest messages and logs it once,
//! so a slow or stuck client never blocks the engine or the other clients.

use std::{
    collections::BTreeMap,
    fmt::{Debug, Display},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex, PoisonError, Weak,
    },
};

use crossbeam_channel::{Receiver, Sender, TryRecvError, TrySendError};

use crate::hub::BusMessage;

pub type SystemEventBusConnectionInst = SystemEventBusConnection<BusMessage>;

/// Capacity of the bus inbox shared by all senders.
const BUS_CAPACITY: usize = 128;
/// Most messages a connection's queue holds before its oldest are dropped.
/// The queue only allocates what it holds, so this can be generous: it covers
/// minutes of traffic for a desk window that stops repainting while hidden.
pub const CONNECTION_CAPACITY: usize = 16384;

/// One connection to the bus. Clones share the same queue (so only one of them
/// should read). The connection is removed from the bus when its last clone is
/// dropped.
pub struct SystemEventBusConnection<T> {
    to_exchange_sender: Sender<T>,
    from_exchange_receiver: Receiver<T>,
    _membership: Arc<Membership<T>>,
}

impl<T> Clone for SystemEventBusConnection<T> {
    fn clone(&self) -> Self {
        Self {
            to_exchange_sender: self.to_exchange_sender.clone(),
            from_exchange_receiver: self.from_exchange_receiver.clone(),
            _membership: Arc::clone(&self._membership),
        }
    }
}

impl<T> SystemEventBusConnection<T> {
    pub fn try_recv(&self) -> Option<T> {
        match self.from_exchange_receiver.try_recv() {
            Ok(event) => Some(event),
            Err(TryRecvError::Disconnected) => {
                tracing::debug!("Event bus: Exchange disconnected.");
                None
            }
            Err(TryRecvError::Empty) => None,
        }
    }

    pub fn send(&self, event: T) {
        match self.to_exchange_sender.try_send(event) {
            Ok(_) => {}
            Err(TrySendError::Disconnected(_)) => {
                tracing::debug!("Event bus: Exchange disconnected.");
            }
            Err(TrySendError::Full(_)) => {
                tracing::debug!("Event bus: Exchange buffer is full.")
            }
        }
    }

    /// Number of messages waiting in this connection's queue.
    pub fn len(&self) -> usize {
        self.from_exchange_receiver.len()
    }

    pub fn is_empty(&self) -> bool {
        self.from_exchange_receiver.is_empty()
    }
}

/// Removes its connection from the bus when the last clone of the connection
/// is dropped.
struct Membership<T> {
    id: u64,
    shared: Weak<BusShared<T>>,
}

impl<T> Drop for Membership<T> {
    fn drop(&mut self) {
        if let Some(shared) = self.shared.upgrade() {
            shared.lock_members().remove(&self.id);
            tracing::debug!("Event bus: connection {} closed", self.id);
        }
    }
}

struct Member<T> {
    /// Unbounded channel; [`Member::push`] enforces the capacity, so nothing
    /// is allocated up front.
    sender: Sender<T>,
    /// A second receiver of the connection's queue, used only to drop the
    /// oldest message when the queue is full.
    oldest: Receiver<T>,
    /// Set on the first overflow, so it is logged once.
    overflowed: bool,
}

impl<T> Member<T> {
    /// Queues `message`, first dropping the oldest queued message if the
    /// queue holds `capacity` messages. Never blocks. Returns `true` on this
    /// member's first overflow.
    fn push(&mut self, message: T, capacity: usize) -> bool {
        let mut first_overflow = false;
        // Only the bus thread pushes; the reader only shrinks the queue.
        if self.sender.len() >= capacity {
            let _ = self.oldest.try_recv();
            first_overflow = !std::mem::replace(&mut self.overflowed, true);
        }
        // Cannot fail: `oldest` keeps the channel connected.
        let _ = self.sender.send(message);
        first_overflow
    }
}

struct BusShared<T> {
    /// Sender into the bus inbox, cloned into every connection.
    inbox: Sender<T>,
    members: Mutex<BTreeMap<u64, Member<T>>>,
    next_id: AtomicU64,
    capacity: usize,
}

impl<T> BusShared<T> {
    fn lock_members(&self) -> std::sync::MutexGuard<'_, BTreeMap<u64, Member<T>>> {
        self.members.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// Adds connections to a bus, also while it runs. Cheap to clone.
pub struct EventBusHandle<T> {
    shared: Arc<BusShared<T>>,
}

impl<T> Clone for EventBusHandle<T> {
    fn clone(&self) -> Self {
        Self {
            shared: Arc::clone(&self.shared),
        }
    }
}

impl<T> EventBusHandle<T> {
    /// Adds a connection. It receives every message sent after this call.
    pub fn connect(&self) -> SystemEventBusConnection<T> {
        let (sender, receiver) = crossbeam_channel::unbounded();
        let id = self.shared.next_id.fetch_add(1, Ordering::Relaxed);
        self.shared.lock_members().insert(
            id,
            Member {
                sender,
                oldest: receiver.clone(),
                overflowed: false,
            },
        );
        tracing::debug!("Event bus: new connection {id}");

        SystemEventBusConnection {
            to_exchange_sender: self.shared.inbox.clone(),
            from_exchange_receiver: receiver,
            _membership: Arc::new(Membership {
                id,
                shared: Arc::downgrade(&self.shared),
            }),
        }
    }

    /// Number of open connections.
    pub fn connection_count(&self) -> usize {
        self.shared.lock_members().len()
    }
}

pub struct SystemEventBus<T> {
    // Incoming messages will be sent out to all connections.
    receiver: Receiver<T>,
    handle: EventBusHandle<T>,
}

impl<T> Default for SystemEventBus<T>
where
    T: Clone + Display + Debug,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<T> SystemEventBus<T>
where
    T: Clone + Display + Debug,
{
    pub fn new() -> Self {
        Self::with_connection_capacity(CONNECTION_CAPACITY)
    }

    /// A bus whose connections each queue at most `capacity` messages.
    pub fn with_connection_capacity(capacity: usize) -> Self {
        let (sender, receiver) = crossbeam_channel::bounded(BUS_CAPACITY);

        Self {
            receiver,
            handle: EventBusHandle {
                shared: Arc::new(BusShared {
                    inbox: sender,
                    members: Mutex::new(BTreeMap::new()),
                    next_id: AtomicU64::new(0),
                    capacity: capacity.max(1),
                }),
            },
        }
    }

    /// A handle that adds connections, also after the bus moved into its
    /// thread.
    pub fn handle(&self) -> EventBusHandle<T> {
        self.handle.clone()
    }

    /// Shorthand for `self.handle().connect()`.
    pub fn new_connection(&self) -> SystemEventBusConnection<T> {
        self.handle.connect()
    }

    /// Delivers one inbox message to every connection.
    fn broadcast(&self, msg: T) {
        let capacity = self.handle.shared.capacity;
        let mut overflowed = Vec::new();
        {
            let mut members = self.handle.shared.lock_members();
            for (id, member) in members.iter_mut() {
                if member.push(msg.clone(), capacity) {
                    overflowed.push(*id);
                }
            }
        }
        for id in overflowed {
            tracing::warn!(
                "Event bus: connection {id} is not keeping up; dropping its oldest messages (logged once)"
            );
        }
    }

    pub fn run(&mut self) {
        loop {
            let msg = match self.receiver.recv() {
                Ok(msg) => msg,
                Err(_) => {
                    tracing::warn!("Event bus: Exchange disconnected, stopping event bus.");
                    break;
                }
            };
            debug_assert!({
                tracing::trace!("Event bus ---> {msg:?}");
                true
            });

            self.broadcast(msg);

            // No sleep here: `recv()` blocks when idle, and sleeping after each
            // message capped the bus at ~100 events/s, silently dropping bursts
            // once the bounded inbox filled up.
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    fn run_in_background(mut bus: SystemEventBus<u32>) {
        std::thread::spawn(move || bus.run());
    }

    fn recv_within(conn: &SystemEventBusConnection<u32>, timeout: Duration) -> Option<u32> {
        let deadline = Instant::now() + timeout;
        loop {
            if let Some(msg) = conn.try_recv() {
                return Some(msg);
            }
            if Instant::now() > deadline {
                return None;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    #[test]
    fn connections_can_be_added_and_removed_while_the_bus_runs() {
        let bus = SystemEventBus::<u32>::new();
        let handle = bus.handle();
        let first = handle.connect();
        run_in_background(bus);

        // Added after the bus started.
        let second = handle.connect();
        first.send(1);
        assert_eq!(recv_within(&first, Duration::from_secs(2)), Some(1));
        assert_eq!(recv_within(&second, Duration::from_secs(2)), Some(1));
        assert_eq!(handle.connection_count(), 2);

        // A clone keeps the connection; dropping the last one removes it.
        let second_clone = second.clone();
        drop(second);
        assert_eq!(handle.connection_count(), 2);
        drop(second_clone);
        assert_eq!(handle.connection_count(), 1);

        first.send(2);
        assert_eq!(recv_within(&first, Duration::from_secs(2)), Some(2));
    }

    #[test]
    fn a_full_connection_drops_its_oldest_messages_without_blocking_the_others() {
        let bus = SystemEventBus::<u32>::with_connection_capacity(4);
        let handle = bus.handle();
        let stuck = handle.connect();
        let reader = handle.connect();
        run_in_background(bus);

        // `reader` keeps up, `stuck` never reads.
        for i in 0..100 {
            reader.send(i);
            assert_eq!(recv_within(&reader, Duration::from_secs(2)), Some(i));
        }
        let kept: Vec<u32> = std::iter::from_fn(|| stuck.try_recv()).collect();
        assert_eq!(kept, vec![96, 97, 98, 99]);
    }
}
