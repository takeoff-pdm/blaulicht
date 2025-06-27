use std::{
    collections::{HashMap, HashSet},
    mem, result,
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};

use blaulicht_shared::ControlEvent;
use crossbeam_channel::{Receiver, SendError, Sender, TryRecvError};
use log::debug;

use crate::audio::SIGNAL_SPEED;

/// The event bus module contains the definition of the event bus and the events that are emitted by the UI or the plugin system.
#[derive(Clone)]
pub struct SystemEventBusConnection {
    to_exchange_sender: Sender<ControlEvent>,
    from_exchange_receiver: Receiver<ControlEvent>,
}

impl SystemEventBusConnection {
    pub fn try_recv(&self) -> Option<ControlEvent> {
        match self.from_exchange_receiver.try_recv() {
            Ok(event) => Some(event),
            Err(TryRecvError::Disconnected) => unreachable!("Cannot disconnect bus"),
            Err(TryRecvError::Empty) => None,
        }
    }

    /// Panics.
    pub fn send(&self, event: ControlEvent) {
        self.to_exchange_sender.send(event).unwrap()
    }
}

// TODO: build the actual exchange that broadcasts.
type BroadCastMembers = HashMap<usize, Sender<ControlEvent>>;
pub struct SystemEventBus {
    // Incoming messages will be sent out to all broadcast members.
    receiver: Receiver<ControlEvent>,
    // Will not be used except to clone.
    sender_template: Sender<ControlEvent>,
    // Broadcast members.
    broadcast_members_id_counter: usize,
    broadcast_members: Arc<Mutex<BroadCastMembers>>,
}

impl SystemEventBus {
    pub fn new() -> Self {
        let (sender, receiver) = crossbeam_channel::unbounded();

        Self {
            receiver,
            sender_template: sender,
            // Broadcast.
            broadcast_members_id_counter: 0,
            broadcast_members: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    // If the connection struct is dropped, the receiver is also dropped and the event bus handles the disconnect error gracefully.
    pub fn new_connection(&mut self) -> SystemEventBusConnection {
        let (to_connection_sender, to_connection_receiver) = crossbeam_channel::unbounded();

        let conn = SystemEventBusConnection {
            to_exchange_sender: self.sender_template.clone(),
            from_exchange_receiver: to_connection_receiver,
        };

        let id = self.alloc_id();
        let mut members = self.broadcast_members.lock().unwrap();
        members.insert(id, to_connection_sender);

        debug!("[BUS] new connection: {id}");

        conn
    }

    fn alloc_id(&mut self) -> usize {
        let count = self.broadcast_members_id_counter;
        self.broadcast_members_id_counter += 1;
        count
    }

    pub fn run(&mut self) -> ! {
        loop {
            let msg = self.receiver.recv().unwrap();
            debug_assert!({
                debug!("[BUS] ---> {msg:?}");
                true
            });

            let mut members = self.broadcast_members.lock().unwrap();
            let mut members_to_remove: Option<Vec<_>> = None;

            for (id, client) in members.iter() {
                if client.send(msg).is_err() {
                    match members_to_remove {
                        Some(ref mut mem) => {
                            mem.push(*id);
                        }
                        None => members_to_remove = Some(vec![*id]),
                    }
                }
            }

            if let Some(members_to_remove) = members_to_remove {
                for id in &members_to_remove {
                    members.remove(id);
                }
            }

            mem::drop(members);
            thread::sleep(Duration::from_millis(10));
        }
    }
}
