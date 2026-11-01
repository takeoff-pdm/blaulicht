//! The browser transport of [`RemoteClient`] (wasm32): the browser's
//! `WebSocket`, binary frames (bincode). There are no threads; the socket's
//! callbacks apply what arrives to the shared [`RemoteState`] between frames
//! and then call the frontend's waker (an egui repaint request).

use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::Arc,
};

use js_sys::{ArrayBuffer, Uint8Array};
use wasm_bindgen::{closure::Closure, JsCast};
use web_sys::{BinaryType, CloseEvent, Event, MessageEvent, WebSocket};

use super::{kind, RemoteClient, RemoteState, Welcome};
use crate::server::protocol::{
    ClientMessage, Encoding, Frame, ServerMessage, Stream, PROTOCOL_VERSION,
};

/// Called after the socket delivered something (or closed).
type Waker = Rc<dyn Fn()>;

/// The socket and what waits for it to open.
struct Link {
    socket: WebSocket,
    /// Frames sent before the socket opened, in order. They follow `Hello`.
    waiting: RefCell<Vec<Vec<u8>>>,
    open: Cell<bool>,
}

impl Link {
    fn send_bytes(&self, bytes: &[u8]) -> bool {
        self.socket.send_with_u8_array(bytes).is_ok()
    }
}

/// The socket and its callbacks (which must live as long as it does).
pub(super) struct Transport {
    url: String,
    waker: Waker,
    link: Rc<Link>,
    _on_open: Closure<dyn FnMut()>,
    _on_message: Closure<dyn FnMut(MessageEvent)>,
    _on_close: Closure<dyn FnMut(CloseEvent)>,
    _on_error: Closure<dyn FnMut(Event)>,
}

fn encode(message: &ClientMessage) -> Option<Vec<u8>> {
    match Encoding::Bincode.encode(message) {
        Ok(Frame::Binary(bytes)) => Some(bytes),
        Ok(Frame::Text(_)) => None,
        Err(err) => {
            tracing::warn!("Remote client: could not encode a message: {err:#}");
            None
        }
    }
}

impl Transport {
    /// Sends `message`, or keeps it until the socket opened; `false` if the
    /// socket is closing or closed.
    pub(super) fn send(&self, message: ClientMessage) -> bool {
        let Some(bytes) = encode(&message) else {
            return false;
        };
        if self.link.open.get() {
            return self.link.send_bytes(&bytes);
        }
        if self.link.socket.ready_state() > WebSocket::OPEN {
            return false;
        }
        self.link.waiting.borrow_mut().push(bytes);
        true
    }
}

impl Drop for Transport {
    fn drop(&mut self) {
        let socket = &self.link.socket;
        socket.set_onopen(None);
        socket.set_onmessage(None);
        socket.set_onclose(None);
        socket.set_onerror(None);
        let _ = socket.close();
    }
}

impl RemoteClient {
    /// Opens a websocket to the engine at `url` (`ws://host:port/`), in a new
    /// operator session, receiving `streams`. Returns at once: the client is
    /// [`super::RemoteStatus::Connecting`] until the engine's `Welcome`, and
    /// commands sent meanwhile wait. `waker` runs whenever something arrived
    /// (request a repaint there). Fails only if the browser refuses the URL.
    pub fn connect_web(
        url: &str,
        streams: &[Stream],
        waker: impl Fn() + 'static,
    ) -> anyhow::Result<Self> {
        Self::open_web(url.to_string(), streams, Rc::new(waker))
    }

    /// A new connection to the same engine with the same streams and waker,
    /// in a new operator session (this one's is gone once it disconnected).
    pub fn reconnect(&self) -> anyhow::Result<Self> {
        Self::open_web(
            self.transport.url.clone(),
            &self.streams(),
            Rc::clone(&self.transport.waker),
        )
    }

    fn open_web(url: String, streams: &[Stream], waker: Waker) -> anyhow::Result<Self> {
        let socket = WebSocket::new(&url)
            .map_err(|err| anyhow::anyhow!("Could not open a websocket to {url}: {err:?}"))?;
        socket.set_binary_type(BinaryType::Arraybuffer);
        let link = Rc::new(Link {
            socket,
            waiting: RefCell::default(),
            open: Cell::new(false),
        });
        let state = Arc::new(RemoteState::new());

        let on_open = {
            let link = Rc::clone(&link);
            let hello = encode(&ClientMessage::Hello {
                protocol_version: PROTOCOL_VERSION,
                streams: streams.to_vec(),
            });
            Closure::<dyn FnMut()>::new(move || {
                if let Some(hello) = &hello {
                    link.send_bytes(hello);
                }
                link.open.set(true);
                for bytes in link.waiting.take() {
                    link.send_bytes(&bytes);
                }
            })
        };
        let on_message = {
            let link = Rc::clone(&link);
            let state = Arc::clone(&state);
            let waker = Rc::clone(&waker);
            Closure::<dyn FnMut(MessageEvent)>::new(move |event: MessageEvent| {
                let Ok(buffer) = event.data().dyn_into::<ArrayBuffer>() else {
                    return;
                };
                let bytes = Uint8Array::new(&buffer).to_vec();
                let len = bytes.len();
                match Encoding::decode::<ServerMessage>(&Frame::Binary(bytes)) {
                    Ok(ServerMessage::Welcome {
                        client,
                        session,
                        engine_version,
                        ..
                    }) => {
                        tracing::info!(
                            "Connected to the engine {engine_version} as {client} ({session})"
                        );
                        let _ = state.welcome.set(Welcome {
                            client,
                            session,
                            engine_version,
                        });
                    }
                    Ok(ServerMessage::Rejected { reason, .. }) => {
                        state.disconnect(&format!("the engine rejected the connection: {reason}"));
                        let _ = link.socket.close();
                    }
                    Ok(message) => {
                        state.count(kind(&message), len);
                        state.apply(message);
                    }
                    Err(err) => tracing::warn!("Remote client: bad message: {err:#}"),
                }
                waker();
            })
        };
        let on_close = {
            let state = Arc::clone(&state);
            let waker = Rc::clone(&waker);
            Closure::<dyn FnMut(CloseEvent)>::new(move |event: CloseEvent| {
                let reason = match (event.code(), event.reason()) {
                    (_, reason) if !reason.is_empty() => format!("closed by the engine: {reason}"),
                    (1000 | 1001, _) => "closed by the engine".to_string(),
                    // 1006: the browser saw the socket go without a close
                    // frame.
                    (1006, _) => "the engine stopped or the network dropped".to_string(),
                    (code, _) => format!("connection lost (code {code})"),
                };
                state.disconnect(&reason);
                waker();
            })
        };
        let on_error = Closure::<dyn FnMut(Event)>::new(move |_event: Event| {
            // The browser tells no details; the close event follows.
            tracing::debug!("Remote client: websocket error");
        });
        link.socket
            .set_onopen(Some(on_open.as_ref().unchecked_ref()));
        link.socket
            .set_onmessage(Some(on_message.as_ref().unchecked_ref()));
        link.socket
            .set_onclose(Some(on_close.as_ref().unchecked_ref()));
        link.socket
            .set_onerror(Some(on_error.as_ref().unchecked_ref()));

        let transport = Transport {
            url,
            waker,
            link,
            _on_open: on_open,
            _on_message: on_message,
            _on_close: on_close,
            _on_error: on_error,
        };
        Ok(Self::with_transport(state, streams, transport))
    }
}
