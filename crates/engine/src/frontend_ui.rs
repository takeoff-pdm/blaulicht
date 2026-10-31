//! Engine-side, read-only mirror of the view state that frontends report, and
//! the routing of plugin requests that only a frontend can act on.
//!
//! View state (which plugin windows are open, which external screens exist)
//! belongs to a frontend, not to the engine. Plugins can still ask about it
//! (`ui_is_open`, `ui_list_external_screens`), so each frontend publishes a
//! small [`FrontendUiSnapshot`] by message. Frontends never lock the mirror:
//! they only hold a [`FrontendUiPublisher`], whose updates the engine applies
//! when it next reads the mirror. Dropping the publisher disconnects the
//! frontend and clears its entries. Frontends are keyed by their
//! [`ClientId`]; a frontend's publisher lives in its
//! [`ClientConnection`](crate::hub::ClientConnection), so it connects and
//! disconnects with the client.
//!
//! Requests from plugins (open a plugin window, maximize it) go to the desk
//! frontend only, as a host-only [`DeskRequest`] addressed to the desk
//! client's hub subscription.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Mutex;

use blaulicht_shared::{ControlEvent, ExternalScreenInfo, MainUiEvent};
use crossbeam_channel::{Receiver, Sender};

use crate::hub::{ClientId, SystemHub};
use crate::msg::SystemMessage;

/// The part of a frontend's view state that the engine and plugins may read.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FrontendUiSnapshot {
    /// Plugins whose UI is open somewhere in this frontend (window, popped-out
    /// viewport or external screen tab).
    pub open_plugins: BTreeSet<u8>,
    pub external_screens: Vec<ExternalScreenInfo>,
}

enum MirrorUpdate {
    Connected(ClientId),
    Snapshot(ClientId, FrontendUiSnapshot),
    Disconnected(ClientId),
}

/// Read-only (for frontends) mirror of every connected frontend's
/// [`FrontendUiSnapshot`], keyed by [`ClientId`]. Lives in
/// [`AppState`](crate::state::AppState).
pub struct FrontendUiMirror {
    sender: Sender<MirrorUpdate>,
    receiver: Receiver<MirrorUpdate>,
    frontends: Mutex<BTreeMap<ClientId, FrontendUiSnapshot>>,
}

impl Default for FrontendUiMirror {
    fn default() -> Self {
        Self::new()
    }
}

impl FrontendUiMirror {
    pub fn new() -> Self {
        let (sender, receiver) = crossbeam_channel::unbounded();
        Self {
            sender,
            receiver,
            frontends: Mutex::new(BTreeMap::new()),
        }
    }

    /// Attach a frontend. It counts as connected (with an empty snapshot)
    /// until the returned publisher is dropped. Connecting the same id twice
    /// is not supported: the first publisher's drop would disconnect both.
    pub fn connect(&self, id: ClientId) -> FrontendUiPublisher {
        let _ = self.sender.send(MirrorUpdate::Connected(id));
        FrontendUiPublisher {
            id,
            sender: self.sender.clone(),
            last: None,
        }
    }

    /// Apply pending updates, then read the current state.
    fn read<R>(&self, f: impl FnOnce(&BTreeMap<ClientId, FrontendUiSnapshot>) -> R) -> R {
        let mut frontends = self.frontends.lock().unwrap();
        while let Ok(update) = self.receiver.try_recv() {
            match update {
                MirrorUpdate::Connected(id) => {
                    frontends.insert(id, FrontendUiSnapshot::default());
                }
                MirrorUpdate::Snapshot(id, snapshot) => {
                    // A late snapshot from a frontend that already
                    // disconnected must not resurrect it.
                    if let Some(entry) = frontends.get_mut(&id) {
                        *entry = snapshot;
                    }
                }
                MirrorUpdate::Disconnected(id) => {
                    frontends.remove(&id);
                }
            }
        }
        f(&frontends)
    }

    pub fn is_connected(&self, id: ClientId) -> bool {
        self.read(|frontends| frontends.contains_key(&id))
    }

    pub fn desk_connected(&self) -> bool {
        self.is_connected(ClientId::Desk)
    }

    /// `true` if the plugin's UI is open in any frontend.
    pub fn is_plugin_open(&self, plugin_id: u8) -> bool {
        self.read(|frontends| {
            frontends
                .values()
                .any(|snapshot| snapshot.open_plugins.contains(&plugin_id))
        })
    }

    /// The desk frontend's external screens; empty without a desk.
    pub fn desk_external_screens(&self) -> Vec<ExternalScreenInfo> {
        self.read(|frontends| {
            frontends
                .get(&ClientId::Desk)
                .map(|snapshot| snapshot.external_screens.clone())
                .unwrap_or_default()
        })
    }
}

/// A frontend's handle for publishing its [`FrontendUiSnapshot`]. Dropping it
/// disconnects the frontend.
pub struct FrontendUiPublisher {
    id: ClientId,
    sender: Sender<MirrorUpdate>,
    last: Option<FrontendUiSnapshot>,
}

impl FrontendUiPublisher {
    pub fn id(&self) -> ClientId {
        self.id
    }

    /// Publish the frontend's current snapshot. Cheap to call every frame:
    /// nothing is sent unless it changed.
    pub fn publish(&mut self, snapshot: FrontendUiSnapshot) {
        if self.last.as_ref() == Some(&snapshot) {
            return;
        }
        let _ = self
            .sender
            .send(MirrorUpdate::Snapshot(self.id, snapshot.clone()));
        self.last = Some(snapshot);
    }
}

impl Drop for FrontendUiPublisher {
    fn drop(&mut self) {
        let _ = self.sender.send(MirrorUpdate::Disconnected(self.id));
    }
}

/// A request from the engine, on behalf of a plugin, that only the desk
/// frontend can act on. Host-only: never serialized to plugins.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeskRequest {
    /// A plugin sent `MainUiEvent::SetPluginUIOpen`.
    SetPluginUiOpen { plugin_id: u8, open: bool },
    /// A plugin called `ui_maximize_screen`: open its UI popped out and
    /// maximized.
    MaximizePluginUi { plugin_id: u8 },
}

/// Send `request` to the desk client's hub subscription, or drop it with a
/// debug log if no desk is connected. Returns whether it was sent.
pub fn send_desk_request(hub: &SystemHub, request: DeskRequest) -> bool {
    let sent = hub.publish_to(ClientId::Desk, SystemMessage::DeskRequest(request));
    if !sent {
        tracing::debug!("No desk frontend connected; dropping {request:?}");
    }
    sent
}

/// Desk requests carried by a plugin-sent event (`SetPluginUIOpen`, also
/// inside a `Transaction`). The event itself still goes on the bus as before,
/// so other plugins keep seeing it.
pub fn desk_requests_in_plugin_event(event: &ControlEvent) -> Vec<DeskRequest> {
    fn collect(event: &ControlEvent, out: &mut Vec<DeskRequest>) {
        match event {
            ControlEvent::MainUi(MainUiEvent::SetPluginUIOpen { plugin_id, open }) => {
                out.push(DeskRequest::SetPluginUiOpen {
                    plugin_id: *plugin_id,
                    open: *open,
                });
            }
            ControlEvent::Transaction(events) => {
                for event in events {
                    collect(event, out);
                }
            }
            _ => {}
        }
    }
    let mut out = Vec::new();
    collect(event, &mut out);
    out
}

/// Route the desk requests in a plugin-sent event to the desk frontend.
pub fn route_plugin_event_to_desk(hub: &SystemHub, event: &ControlEvent) {
    for request in desk_requests_in_plugin_event(event) {
        send_desk_request(hub, request);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hub::HubSubscription;

    fn screen(index: u32, owner: Option<u8>) -> ExternalScreenInfo {
        ExternalScreenInfo {
            index,
            width: 1920.0,
            height: 1080.0,
            x: None,
            y: None,
            owner_plugin_id: owner,
        }
    }

    fn snapshot(open: &[u8], screens: Vec<ExternalScreenInfo>) -> FrontendUiSnapshot {
        FrontendUiSnapshot {
            open_plugins: open.iter().copied().collect(),
            external_screens: screens,
        }
    }

    fn desk_requests(subscription: &HubSubscription) -> Vec<DeskRequest> {
        std::iter::from_fn(|| subscription.try_recv())
            .filter_map(|message| match message {
                SystemMessage::DeskRequest(request) => Some(request),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn plugin_is_open_if_open_in_any_frontend() {
        let mirror = FrontendUiMirror::new();
        let mut desk = mirror.connect(ClientId::Desk);
        let mut remote = mirror.connect(ClientId::Remote(7));

        assert!(!mirror.is_plugin_open(1));
        remote.publish(snapshot(&[1], vec![]));
        assert!(mirror.is_plugin_open(1));
        desk.publish(snapshot(&[2], vec![]));
        assert!(mirror.is_plugin_open(1));
        assert!(mirror.is_plugin_open(2));
        assert!(!mirror.is_plugin_open(3));

        remote.publish(snapshot(&[], vec![]));
        assert!(!mirror.is_plugin_open(1));
        assert!(mirror.is_plugin_open(2));
    }

    #[test]
    fn disconnect_clears_the_frontends_entries() {
        let mirror = FrontendUiMirror::new();
        let mut desk = mirror.connect(ClientId::Desk);
        let mut remote = mirror.connect(ClientId::Remote(7));
        desk.publish(snapshot(&[1], vec![screen(0, None)]));
        remote.publish(snapshot(&[2], vec![]));
        assert!(mirror.is_plugin_open(2));

        drop(remote);
        assert!(!mirror.is_plugin_open(2));
        assert!(!mirror.is_connected(ClientId::Remote(7)));
        assert!(mirror.is_plugin_open(1));

        drop(desk);
        assert!(!mirror.desk_connected());
        assert!(!mirror.is_plugin_open(1));
        assert!(mirror.desk_external_screens().is_empty());
    }

    #[test]
    fn external_screens_are_the_desks_only() {
        let mirror = FrontendUiMirror::new();
        assert!(mirror.desk_external_screens().is_empty());
        let mut desk = mirror.connect(ClientId::Desk);
        let mut remote = mirror.connect(ClientId::Remote(7));
        remote.publish(snapshot(&[], vec![screen(0, None), screen(1, None)]));
        assert!(mirror.desk_external_screens().is_empty());
        desk.publish(snapshot(&[], vec![screen(0, Some(3))]));
        assert_eq!(mirror.desk_external_screens(), vec![screen(0, Some(3))]);
    }

    #[test]
    fn connecting_counts_before_the_first_snapshot_and_unchanged_snapshots_are_not_resent() {
        let mirror = FrontendUiMirror::new();
        let mut desk = mirror.connect(ClientId::Desk);
        assert!(mirror.desk_connected());

        desk.publish(snapshot(&[1], vec![]));
        desk.publish(snapshot(&[1], vec![]));
        desk.publish(snapshot(&[1], vec![]));
        assert_eq!(mirror.receiver.len(), 1);
        assert!(mirror.is_plugin_open(1));
        assert!(mirror.receiver.is_empty());
    }

    #[test]
    fn plugin_set_open_requests_go_to_the_desk() {
        let hub = SystemHub::new();
        let desk = hub.subscribe(ClientId::Desk);
        let remote = hub.subscribe(ClientId::Remote(7));

        route_plugin_event_to_desk(
            &hub,
            &ControlEvent::MainUi(MainUiEvent::SetPluginUIOpen {
                plugin_id: 4,
                open: true,
            }),
        );
        route_plugin_event_to_desk(
            &hub,
            &ControlEvent::Transaction(vec![
                ControlEvent::SelectGroup(0),
                ControlEvent::MainUi(MainUiEvent::SetPluginUIOpen {
                    plugin_id: 5,
                    open: false,
                }),
            ]),
        );
        route_plugin_event_to_desk(&hub, &ControlEvent::SelectGroup(0));

        assert_eq!(
            desk_requests(&desk),
            vec![
                DeskRequest::SetPluginUiOpen {
                    plugin_id: 4,
                    open: true
                },
                DeskRequest::SetPluginUiOpen {
                    plugin_id: 5,
                    open: false
                },
            ]
        );
        // Other frontends don't get desk requests.
        assert!(desk_requests(&remote).is_empty());
    }

    #[test]
    fn maximize_goes_to_the_desk() {
        let hub = SystemHub::new();
        let desk = hub.subscribe(ClientId::Desk);
        assert!(send_desk_request(
            &hub,
            DeskRequest::MaximizePluginUi { plugin_id: 2 }
        ));
        assert_eq!(
            desk_requests(&desk),
            vec![DeskRequest::MaximizePluginUi { plugin_id: 2 }]
        );
    }

    #[test]
    fn requests_without_a_desk_are_dropped() {
        let hub = SystemHub::new();
        // Another frontend does not receive desk requests.
        let remote = hub.subscribe(ClientId::Remote(7));

        assert!(!send_desk_request(
            &hub,
            DeskRequest::MaximizePluginUi { plugin_id: 2 }
        ));
        route_plugin_event_to_desk(
            &hub,
            &ControlEvent::MainUi(MainUiEvent::SetPluginUIOpen {
                plugin_id: 4,
                open: true,
            }),
        );
        assert!(desk_requests(&remote).is_empty());

        // A desk that disconnected gets nothing either.
        drop(hub.subscribe(ClientId::Desk));
        assert!(!send_desk_request(
            &hub,
            DeskRequest::MaximizePluginUi { plugin_id: 2 }
        ));
    }
}
