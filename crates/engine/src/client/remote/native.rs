//! The native transport of [`RemoteClient`]: a TCP socket with `tungstenite`,
//! owned by an I/O thread that applies what arrives to the shared
//! [`RemoteState`] and writes the client's messages.

use std::{
    io::ErrorKind,
    net::{SocketAddr, TcpStream, ToSocketAddrs},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread::{self, JoinHandle},
    time::Duration,
};
use web_time::Instant;

use anyhow::Context;
use crossbeam_channel::{Receiver, Sender};
use tungstenite::{Message, WebSocket};

use super::{kind, RemoteClient, RemoteState, Welcome};
use crate::server::protocol::{
    ClientMessage, Encoding, Frame, ServerMessage, Stream, PROTOCOL_VERSION,
};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(5);
/// How long the I/O thread sleeps when there was nothing to do.
const IDLE_SLEEP: Duration = Duration::from_millis(2);

/// The I/O thread and the channel to it.
pub(super) struct Transport {
    addr: SocketAddr,
    outgoing: Sender<ClientMessage>,
    stop: Arc<AtomicBool>,
    io: Option<JoinHandle<()>>,
}

impl Transport {
    /// Hands `message` to the I/O thread; `false` if it is gone.
    pub(super) fn send(&self, message: ClientMessage) -> bool {
        self.outgoing.send(message).is_ok()
    }
}

impl Drop for Transport {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(io) = self.io.take() {
            let _ = io.join();
        }
    }
}

fn into_message(frame: Frame) -> Message {
    match frame {
        Frame::Binary(bytes) => Message::Binary(bytes),
        Frame::Text(text) => Message::Text(text),
    }
}

impl RemoteClient {
    /// Connects to the engine's server at `addr` (`"host:port"`), in a new
    /// operator session, receiving `streams` (see [`Stream`]). Blocks until
    /// the engine answered. Fails if the engine can't be reached or rejects
    /// the client (for example another [`PROTOCOL_VERSION`]).
    pub fn connect(addr: impl ToSocketAddrs, streams: &[Stream]) -> anyhow::Result<Self> {
        let addr = addr
            .to_socket_addrs()?
            .next()
            .context("No address to connect to")?;
        let stream = TcpStream::connect_timeout(&addr, CONNECT_TIMEOUT)
            .with_context(|| format!("Could not connect to {addr}"))?;
        stream.set_nodelay(true)?;
        stream.set_read_timeout(Some(HANDSHAKE_TIMEOUT))?;
        stream.set_write_timeout(Some(HANDSHAKE_TIMEOUT))?;
        let url = format!("ws://{addr}/");
        let (mut ws, _) = tungstenite::client::client_with_config(
            url.as_str(),
            stream,
            Some(crate::server::websocket_config()),
        )
        .map_err(|err| anyhow::anyhow!("Websocket handshake with {addr} failed: {err}"))?;

        let hello = Encoding::Bincode.encode(&ClientMessage::Hello {
            protocol_version: PROTOCOL_VERSION,
            streams: streams.to_vec(),
        })?;
        ws.send(into_message(hello))?;
        let welcome = loop {
            match ws.read()? {
                Message::Binary(bytes) => {
                    break Encoding::decode::<ServerMessage>(&Frame::Binary(bytes))?
                }
                Message::Close(_) => anyhow::bail!("The engine closed the connection"),
                _ => continue,
            }
        };
        let welcome = match welcome {
            ServerMessage::Welcome {
                client,
                session,
                engine_version,
                ..
            } => Welcome {
                client,
                session,
                engine_version,
            },
            ServerMessage::Rejected { reason, .. } => {
                anyhow::bail!("The engine rejected the connection: {reason}")
            }
            _ => anyhow::bail!("The engine did not answer Hello"),
        };
        ws.get_mut().set_read_timeout(None)?;
        ws.get_mut().set_write_timeout(None)?;
        ws.get_mut().set_nonblocking(true)?;

        let client = welcome.client;
        let state = Arc::new(RemoteState::new());
        let _ = state.welcome.set(welcome);
        let (outgoing, receiver) = crossbeam_channel::unbounded();
        let stop = Arc::new(AtomicBool::new(false));
        let io = {
            let state = Arc::clone(&state);
            let stop = Arc::clone(&stop);
            thread::Builder::new()
                .name(format!("remote-client-{client}"))
                .spawn(move || io_loop(ws, &state, &receiver, &stop))?
        };
        let transport = Transport {
            addr,
            outgoing,
            stop,
            io: Some(io),
        };
        Ok(Self::with_transport(state, streams, transport))
    }

    /// A new connection to the same engine with the same streams, in a new
    /// operator session (this one's is gone once it disconnected).
    pub fn reconnect(&self) -> anyhow::Result<Self> {
        Self::connect(self.transport.addr, &self.streams())
    }

    /// Waits until the first show and session snapshots arrived (`true`), or
    /// `timeout` passed or the connection dropped (`false`).
    pub fn wait_for_snapshot(&self, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        loop {
            if self.has_snapshot() {
                return true;
            }
            if Instant::now() >= deadline || !self.is_connected() {
                return false;
            }
            thread::sleep(Duration::from_millis(2));
        }
    }
}

fn io_loop(
    mut ws: WebSocket<TcpStream>,
    state: &RemoteState,
    outgoing: &Receiver<ClientMessage>,
    stop: &AtomicBool,
) {
    let reason = loop {
        if stop.load(Ordering::SeqCst) {
            let _ = ws.close(None);
            let _ = ws.flush();
            break "closed by the client".to_string();
        }
        let mut busy = false;
        let mut ended = None;
        loop {
            match ws.read() {
                Ok(Message::Binary(bytes)) => {
                    busy = true;
                    let len = bytes.len();
                    match Encoding::decode::<ServerMessage>(&Frame::Binary(bytes)) {
                        Ok(message) => {
                            state.count(kind(&message), len);
                            state.apply(message);
                        }
                        Err(err) => tracing::warn!("Remote client: bad message: {err:#}"),
                    }
                }
                Ok(Message::Close(_)) => {
                    let _ = ws.flush();
                    ended = Some("closed by the engine".to_string());
                    break;
                }
                Ok(_) => {}
                Err(tungstenite::Error::Io(err)) if err.kind() == ErrorKind::WouldBlock => break,
                Err(err) => {
                    ended = Some(format!("connection lost ({err})"));
                    break;
                }
            }
        }
        if let Some(reason) = ended {
            break reason;
        }
        while let Ok(message) = outgoing.try_recv() {
            busy = true;
            let frame = match Encoding::Bincode.encode(&message) {
                Ok(frame) => frame,
                Err(err) => {
                    tracing::warn!("Remote client: could not encode a message: {err:#}");
                    continue;
                }
            };
            match ws.write(into_message(frame)) {
                Ok(()) => {}
                Err(tungstenite::Error::Io(err)) if err.kind() == ErrorKind::WouldBlock => {}
                Err(err) => {
                    ended = Some(format!("connection lost ({err})"));
                    break;
                }
            }
        }
        if let Some(reason) = ended {
            break reason;
        }
        match ws.flush() {
            Ok(()) => {}
            Err(tungstenite::Error::Io(err)) if err.kind() == ErrorKind::WouldBlock => {}
            Err(err) => break format!("connection lost ({err})"),
        }
        if !busy {
            thread::sleep(IDLE_SLEEP);
        }
    };
    state.disconnect(&reason);
}
