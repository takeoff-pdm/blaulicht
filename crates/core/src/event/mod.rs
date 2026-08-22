use std::{
    collections::HashMap,
    fmt::Display,
    mem,
    sync::{Arc, Mutex},
};

use blaulicht_shared::ControlEventMessage;
use crossbeam_channel::{Receiver, Sender, TryRecvError, TrySendError};
use std::fmt::Debug;

pub type SystemEventBusConnectionInst = SystemEventBusConnection<ControlEventMessage>;

/// The event bus module contains the definition of the event bus and the events that are emitted by the UI or the plugin system.
#[derive(Clone)]
pub struct SystemEventBusConnection<T> {
    to_exchange_sender: Sender<T>,
    from_exchange_receiver: Receiver<T>,
}

impl<T> SystemEventBusConnection<T> {
    pub fn try_recv(&self) -> Option<T> {
        match self.from_exchange_receiver.try_recv() {
            Ok(event) => Some(event),
            Err(TryRecvError::Disconnected) => {
                tracing::debug!("[BUS] Exchange disconnected.");
                None
            }
            Err(TryRecvError::Empty) => None,
        }
    }

    pub fn send(&self, event: T) {
        match self.to_exchange_sender.try_send(event) {
            Ok(_) => {}
            Err(TrySendError::Disconnected(_)) => {
                tracing::debug!("[BUS] Exchange disconnected.");
            }
            Err(TrySendError::Full(_)) => {
                tracing::debug!("[BUS] Exchange buffer is full.")
            }
        }
    }
}

// TODO: build the actual exchange that broadcasts.
const BUS_CAPACITY: usize = 128;
type BroadCastMembers<T> = HashMap<usize, Sender<T>>;

pub struct SystemEventBus<T> {
    // Incoming messages will be sent out to all broadcast members.
    receiver: Receiver<T>,
    // Will not be used except to clone.
    sender_template: Sender<T>,
    // Broadcast members.
    broadcast_members_id_counter: usize,
    broadcast_members: Arc<Mutex<BroadCastMembers<T>>>,
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
        let (sender, receiver) = crossbeam_channel::bounded(BUS_CAPACITY);

        Self {
            receiver,
            sender_template: sender,
            // Broadcast.
            broadcast_members_id_counter: 0,
            broadcast_members: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    // If the connection struct is dropped, the receiver is also dropped and the event bus handles the disconnect error gracefully.
    pub fn new_connection(&mut self) -> SystemEventBusConnection<T> {
        let (to_connection_sender, to_connection_receiver) = crossbeam_channel::unbounded();

        let conn = SystemEventBusConnection {
            to_exchange_sender: self.sender_template.clone(),
            from_exchange_receiver: to_connection_receiver,
        };

        let id = self.alloc_id();
        let mut members = self.broadcast_members.lock().unwrap();
        members.insert(id, to_connection_sender);

        tracing::debug!("[BUS] new connection: {id}");

        conn
    }

    fn alloc_id(&mut self) -> usize {
        let count = self.broadcast_members_id_counter;
        self.broadcast_members_id_counter += 1;
        count
    }

    pub fn run(&mut self) {
        loop {
            let msg = match self.receiver.recv() {
                Ok(msg) => msg,
                Err(_) => {
                    tracing::warn!("[BUS] Exchange disconnected, stopping event bus.");
                    break;
                }
            };
            debug_assert!({
                tracing::debug!("[BUS] ---> {msg:?}");
                true
            });

            let mut members = self.broadcast_members.lock().unwrap();
            let mut members_to_remove: Option<Vec<_>> = None;

            for (id, client) in members.iter() {
                match client.try_send(msg.clone()) {
                    Ok(_) => {}
                    Err(TrySendError::Full(_)) => {
                        tracing::warn!("[BUS] System event bus buffer is full");
                    }
                    Err(TrySendError::Disconnected(_)) => match members_to_remove {
                        Some(ref mut mem) => {
                            mem.push(*id);
                        }
                        None => members_to_remove = Some(vec![*id]),
                    },
                }
            }

            if let Some(members_to_remove) = members_to_remove {
                for id in &members_to_remove {
                    members.remove(id);
                }
            }

            mem::drop(members);

            // No sleep here: `recv()` blocks when idle, and sleeping after each
            // message capped the bus at ~100 events/s, silently dropping bursts
            // once the bounded inbox filled up.
        }
    }
}
