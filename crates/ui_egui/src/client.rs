//! [`AppClient`]: the engine connection a [`crate::app::BlaulichtApp`] holds,
//! either in process (the desk) or over the websocket protocol (the web
//! frontend, or a native UI attached to a remote engine).
//!
//! `EngineClient` is not object-safe (its readers take closures
//! generically), so this is an enum that delegates every call: one match per
//! call, no allocation, lock or copy on top of the client's own. Pages use it
//! through the `EngineClient` trait like any client.

use std::collections::HashMap;

use blaulicht_audio_engine::{AudioSpectrogram, SignalCollectorParams};
use blaulicht_engine::{
    audio::defs::AudioThreadControlSignal,
    client::{EngineClient, RemoteClient, RemoteStatus},
    dmx::EngineState,
    frontend_ui::FrontendUiSnapshot,
    hub::{BusBody, BusMessage, ClientId, CommandResult, CommandSeq, SessionId},
    msg::SystemMessage,
    state::{AppHealthState, ArtNetReceiver, PluginRuntimeKind, PluginState},
    ui_ops::WasmUiOp,
};

#[cfg(not(target_arch = "wasm32"))]
use blaulicht_engine::client::LocalClient;

/// The frontend's client: [`LocalClient`] in the engine's process (never on
/// wasm32), or a [`RemoteClient`].
pub enum AppClient {
    #[cfg(not(target_arch = "wasm32"))]
    Local(LocalClient),
    Remote(RemoteClient),
}

/// Runs `$body` with `$client` bound to the inner client.
macro_rules! delegate {
    ($self:expr, $client:ident => $body:expr) => {
        match $self {
            #[cfg(not(target_arch = "wasm32"))]
            AppClient::Local($client) => $body,
            AppClient::Remote($client) => $body,
        }
    };
}

impl AppClient {
    /// Whether the engine runs elsewhere: the frontend can't reach the
    /// engine host's files, devices or windows, and its session is not the
    /// desk's.
    pub fn is_remote(&self) -> bool {
        matches!(self, AppClient::Remote(_))
    }

    /// The remote client, if this is one.
    pub fn remote(&self) -> Option<&RemoteClient> {
        match self {
            AppClient::Remote(remote) => Some(remote),
            #[cfg(not(target_arch = "wasm32"))]
            AppClient::Local(_) => None,
        }
    }

    /// Where the connection stands; a local client is always connected.
    pub fn status(&self) -> RemoteStatus {
        self.remote()
            .map_or(RemoteStatus::Connected, RemoteClient::status)
    }

    /// Replaces a remote client whose connection is gone with a new one to
    /// the same engine (a new operator session). Does nothing for a local
    /// client.
    pub fn reconnect(&mut self) -> anyhow::Result<()> {
        match self {
            AppClient::Remote(remote) => *remote = remote.reconnect()?,
            #[cfg(not(target_arch = "wasm32"))]
            AppClient::Local(_) => {}
        }
        Ok(())
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl From<LocalClient> for AppClient {
    fn from(client: LocalClient) -> Self {
        AppClient::Local(client)
    }
}

impl From<RemoteClient> for AppClient {
    fn from(client: RemoteClient) -> Self {
        AppClient::Remote(client)
    }
}

impl EngineClient for AppClient {
    fn client(&self) -> ClientId {
        delegate!(self, c => c.client())
    }

    fn session(&self) -> SessionId {
        delegate!(self, c => c.session())
    }

    fn with_show<R>(&self, f: impl FnOnce(&EngineState) -> R) -> R {
        delegate!(self, c => c.with_show(f))
    }

    fn send(&self, body: impl Into<BusBody>) -> CommandSeq {
        delegate!(self, c => c.send(body))
    }

    fn take_results(&self) -> Vec<CommandResult> {
        delegate!(self, c => c.take_results())
    }

    fn try_recv_system(&self) -> Option<SystemMessage> {
        delegate!(self, c => c.try_recv_system())
    }

    fn try_recv_bus(&self) -> Option<BusMessage> {
        delegate!(self, c => c.try_recv_bus())
    }

    fn with_dmx_universe<R>(&self, universe: usize, f: impl FnOnce(&[u8; 513]) -> R) -> Option<R> {
        delegate!(self, c => c.with_dmx_universe(universe, f))
    }

    fn with_health<R>(&self, f: impl FnOnce(&AppHealthState) -> R) -> R {
        delegate!(self, c => c.with_health(f))
    }

    fn with_spectrogram<R>(&self, f: impl FnOnce(&AudioSpectrogram) -> R) -> R {
        delegate!(self, c => c.with_spectrogram(f))
    }

    fn with_plugin_ui_ops<R>(&self, f: impl FnOnce(&HashMap<u8, Vec<WasmUiOp>>) -> R) -> R {
        delegate!(self, c => c.with_plugin_ui_ops(f))
    }

    fn with_audio_params<R>(&self, f: impl FnOnce(&SignalCollectorParams) -> R) -> R {
        delegate!(self, c => c.with_audio_params(f))
    }

    fn with_artnet_receivers<R>(&self, f: impl FnOnce(&[ArtNetReceiver]) -> R) -> R {
        delegate!(self, c => c.with_artnet_receivers(f))
    }

    fn with_plugins<R>(&self, f: impl FnOnce(&HashMap<u8, PluginState>) -> R) -> R {
        delegate!(self, c => c.with_plugins(f))
    }

    fn grand_master_percent(&self) -> u8 {
        delegate!(self, c => c.grand_master_percent())
    }

    fn mainloop_state(&self) -> AudioThreadControlSignal {
        delegate!(self, c => c.mainloop_state())
    }

    fn with_plugin_runtime_kinds<R>(
        &self,
        f: impl FnOnce(&HashMap<u8, PluginRuntimeKind>) -> R,
    ) -> R {
        delegate!(self, c => c.with_plugin_runtime_kinds(f))
    }

    fn with_animation_editor_ui_ops<R>(
        &self,
        f: impl FnOnce(&HashMap<u64, Vec<WasmUiOp>>) -> R,
    ) -> R {
        delegate!(self, c => c.with_animation_editor_ui_ops(f))
    }

    fn publish_ui(&mut self, snapshot: FrontendUiSnapshot) {
        delegate!(self, c => c.publish_ui(snapshot))
    }
}
