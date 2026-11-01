//! The websocket server and `RemoteClient` against a real engine (the test
//! harness's `TestEngine`: real `DmxEngine`, bus and hub), over loopback.

use std::{
    collections::{BTreeSet, HashSet},
    net::{SocketAddr, TcpStream},
    path::PathBuf,
    sync::Arc,
    time::Duration,
};
use web_time::{Instant, SystemTime, UNIX_EPOCH};

use blaulicht_audio_engine::CollectorOutput;
use blaulicht_shared::{
    fixture::state::FixtureState, ControlEvent, ControlEventMessage, EventOriginator, LogLevel,
};
use serde_json::{json, Value};
use tungstenite::{Message, WebSocket};

use super::{
    protocol::{
        ClientMessage, Encoding, Frame, ServerMessage, ShowState, Stream, WireSystemMessage,
        PROTOCOL_VERSION,
    },
    ServerHandle, ServerOptions,
};
use crate::{
    client::{
        testing::{small_rig, TestEngine},
        EngineClient, RemoteClient, RemoteStatus, ReplyHandlers, DISCONNECTED,
    },
    config::ShowfileUiState,
    dmx::DESK_ONLY_REFUSAL,
    frontend_ui::FrontendUiSnapshot,
    hub::{BusBody, CommandOutcome, HostCommand, ModelBytes, SessionId},
    msg::SystemMessage,
    persistence::{SaveStatus, ShowfileStatus},
};

const OK: CommandOutcome = CommandOutcome::Ok { created: None };

fn loopback() -> ServerOptions {
    ServerOptions::new(SocketAddr::from(([127, 0, 0, 1], 0)))
}

fn web(event: ControlEvent) -> ControlEventMessage {
    ControlEventMessage::new(EventOriginator::Web, event)
}

/// A remote client whose first snapshots arrived.
fn connect(engine: &mut TestEngine, server: &ServerHandle, streams: &[Stream]) -> RemoteClient {
    let remote = RemoteClient::connect(server.local_addr(), streams).expect("connect");
    engine.tick_until("the first snapshots", |_| {
        remote.show_digest().is_some() && remote.with_session(|_, _| ()).is_some()
    });
    remote
}

/// Sends `body` from `remote` and ticks until its result arrives.
fn round_trip(
    engine: &mut TestEngine,
    remote: &RemoteClient,
    body: impl Into<BusBody>,
) -> CommandOutcome {
    let seq = remote.send(body);
    let mut outcome = None;
    engine.tick_until("a result", |_| {
        for result in remote.take_results() {
            assert_eq!(result.seq, seq, "a result for another command");
            outcome = Some(result.outcome);
        }
        outcome.is_some()
    });
    outcome.unwrap()
}

/// A raw websocket to the server, for what `RemoteClient` doesn't do (JSON,
/// wrong versions, never reading).
fn raw_socket(server: &ServerHandle) -> WebSocket<TcpStream> {
    let stream = TcpStream::connect(server.local_addr()).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let url = format!("ws://{}/", server.local_addr());
    tungstenite::client(url.as_str(), stream).unwrap().0
}

fn send(ws: &mut WebSocket<TcpStream>, encoding: Encoding, message: &ClientMessage) {
    let frame = match encoding {
        // What a non-Rust client writes.
        Encoding::Json => Frame::Text(serde_json::to_string(message).unwrap()),
        Encoding::Bincode => encoding.encode(message).unwrap(),
    };
    ws.send(match frame {
        Frame::Binary(bytes) => Message::Binary(bytes),
        Frame::Text(text) => Message::Text(text),
    })
    .unwrap();
}

/// The acceptance test: a client connects over loopback, gets the
/// snapshots, selects a group and sets a color in its own session; the DMX
/// output changes and the desk's selection doesn't. Disconnecting closes its
/// session and drops its view state.
#[test]
fn a_remote_client_works_in_its_own_session_and_cleans_up_on_disconnect() {
    let mut engine = TestEngine::start(small_rig);
    // Scene 0 plays, so edits to it reach the output.
    engine.edit_show(|show| show.0.current_overlay_scenes = vec![0]);
    let desk = engine.connect_desk();
    let server = engine.start_server(loopback());
    let mut remote = connect(&mut engine, &server, &[Stream::Dmx]);

    assert_ne!(remote.session(), SessionId::DESK);
    assert_eq!(remote.engine_version(), Some(env!("CARGO_PKG_VERSION")));
    let clients = server.clients();
    assert_eq!(clients.len(), 1);
    assert_eq!(
        (clients[0].client, clients[0].session, clients[0].encoding),
        (remote.client(), remote.session(), Encoding::Bincode)
    );
    // The show snapshot is the engine's.
    assert_eq!(
        remote.with_show(|show| show.0.groups.keys().copied().collect::<Vec<_>>()),
        vec![0, 1, 2]
    );

    assert_eq!(
        engine.round_trip(&desk, web(ControlEvent::SelectGroup(0))),
        OK
    );
    assert_eq!(
        round_trip(&mut engine, &remote, web(ControlEvent::SelectGroup(1))),
        OK
    );
    // The session snapshot comes before the result.
    assert_eq!(
        remote.with_session(|_, me| me.selection.group_ids.clone()),
        Some(HashSet::from([1]))
    );
    assert_eq!(
        round_trip(
            &mut engine,
            &remote,
            web(ControlEvent::SetColor((255, 0, 0)))
        ),
        OK
    );
    assert_eq!(
        round_trip(&mut engine, &remote, web(ControlEvent::SetAlpha(255))),
        OK
    );

    // Group 1's fixtures are at channels 7..=12, group 0's at 1..=6.
    let group = |dmx: &[u8; 513], gid: usize| dmx[1 + 6 * gid..7 + 6 * gid].to_vec();
    engine.tick_until("the remote's DMX stream", |_| {
        remote.with_dmx_universe(0, |dmx| group(dmx, 1)) == Some(vec![255, 0, 0, 255, 0, 0])
    });
    assert_eq!(
        desk.with_dmx_universe(0, |dmx| (group(dmx, 0), group(dmx, 1))),
        Some((vec![0; 6], vec![255, 0, 0, 255, 0, 0]))
    );
    // The desk's session is untouched.
    assert_eq!(
        desk.with_session(|_, me| me.selection.group_ids.clone()),
        Some(HashSet::from([0]))
    );
    desk.with_show(|show| {
        let untouched = format!("{:?}", FixtureState::default());
        assert_eq!(format!("{:?}", show.0.control_buffer), untouched);
    });

    // View state reported by the remote reaches the mirror ...
    remote.publish_ui(FrontendUiSnapshot {
        open_plugins: BTreeSet::from([3]),
        ..FrontendUiSnapshot::default()
    });
    let app = Arc::clone(engine.app());
    engine.tick_until("the remote's view state", |_| {
        app.frontend_ui.is_plugin_open(3)
    });

    // ... and goes with its session when it disconnects.
    let session = remote.session();
    drop(remote);
    engine.tick_until("the session to close", |engine| {
        !engine.edit_show(|show| show.has_session(session))
    });
    engine.tick_until("the server to forget the client", |_| {
        server.clients().is_empty()
    });
    assert!(!app.frontend_ui.is_plugin_open(3));
}

/// Logs go through the hub to every client: the desk and a remote one,
/// including the lines logged before the remote connected (the replay).
#[test]
fn logs_reach_the_desk_and_a_remote_client() {
    let mut engine = TestEngine::start(small_rig);
    let desk = engine.connect_desk();
    let hub = engine.hub().clone();
    crate::log::with_hub_logging(hub.clone(), || tracing::info!("before the remote"));
    let server = engine.start_server(loopback());
    let remote = connect(&mut engine, &server, &[]);
    crate::log::with_hub_logging(hub, || tracing::warn!("after the remote"));

    let logs = |client: &dyn Fn() -> Option<SystemMessage>| {
        std::iter::from_fn(client)
            .filter_map(|message| match message {
                SystemMessage::Log {
                    message,
                    level,
                    source,
                } => Some((message, level, source.into_owned())),
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    let expected = vec![
        (
            "before the remote".to_string(),
            LogLevel::Info,
            "Web".to_string(),
        ),
        (
            "after the remote".to_string(),
            LogLevel::Warn,
            "Web".to_string(),
        ),
    ];
    assert_eq!(logs(&|| desk.try_recv_system()), expected);
    let mut remote_logs = Vec::new();
    engine.tick_until("the logs at the remote", |_| {
        remote_logs.extend(logs(&|| remote.try_recv_system()));
        remote_logs.len() >= 2
    });
    assert_eq!(remote_logs, expected);
}

/// A client that stops reading doesn't slow the engine down, and what waits
/// for it stays bounded: the socket fills, the server stops producing for it,
/// its hub queue stays at its cap, and after the stall timeout it is
/// disconnected and its session closed. Another remote client keeps working
/// throughout.
#[test]
fn a_stalled_client_neither_blocks_the_engine_nor_grows_without_bound() {
    let mut engine = TestEngine::start(small_rig);
    let queue_capacity = 1500;
    let stall_timeout = Duration::from_secs(4);
    let server = engine.start_server(ServerOptions {
        queue_capacity,
        stall_timeout,
        ..loopback()
    });
    let healthy = connect(&mut engine, &server, &[]);

    // The stalled client: handshake and Hello, then it never reads again.
    let mut stalled = raw_socket(&server);
    send(
        &mut stalled,
        Encoding::Bincode,
        &ClientMessage::Hello {
            protocol_version: PROTOCOL_VERSION,
            streams: Stream::ALL.to_vec(),
        },
    );
    let stalled_info = || {
        server
            .clients()
            .into_iter()
            .find(|info| info.client != healthy.client())
    };
    engine.tick_until("the stalled client to join", |_| stalled_info().is_some());
    let stalled_session = stalled_info().unwrap().session;

    // Far more than the socket buffers hold (40 MB of log lines). Publishing
    // never waits for a subscriber.
    let line = "x".repeat(4096);
    let started = Instant::now();
    for n in 0..10_000 {
        engine.hub().publish(SystemMessage::Log {
            message: format!("{n} {line}"),
            level: LogLevel::Info,
            source: "Test".into(),
        });
        if n % 500 == 0 {
            while healthy.try_recv_system().is_some() {}
        }
    }
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "publishing took {:?}",
        started.elapsed()
    );

    engine.tick_until("the stalled client to back up", |_| {
        while healthy.try_recv_system().is_some() {}
        stalled_info().is_some_and(|info| info.backlogged)
    });
    let backed_up = stalled_info().unwrap();
    // Nothing more is buffered for it while it doesn't read ...
    std::thread::sleep(Duration::from_millis(500));
    let later = stalled_info().unwrap();
    assert!(later.backlogged);
    assert_eq!(later.sent_bytes, backed_up.sent_bytes, "{later:?}");
    // ... and its hub queue stays at its cap.
    assert!(later.queued_system <= queue_capacity, "{later:?}");

    // The engine and the other client don't notice.
    for _ in 0..20 {
        let started = Instant::now();
        assert_eq!(round_trip(&mut engine, &healthy, HostCommand::Ping), OK);
        assert!(
            started.elapsed() < Duration::from_secs(1),
            "{:?}",
            started.elapsed()
        );
        while healthy.try_recv_system().is_some() {}
    }
    let started = Instant::now();
    for _ in 0..100 {
        engine.tick();
    }
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "{:?}",
        started.elapsed()
    );

    // After the stall timeout the server drops it, and its session closes.
    engine.tick_until("the stalled client to be dropped", |engine| {
        while healthy.try_recv_system().is_some() {}
        stalled_info().is_none() && !engine.edit_show(|show| show.has_session(stalled_session))
    });
    assert!(backed_up.queued_system <= queue_capacity);
    assert!(healthy.is_connected());
    drop(stalled);
}

/// A client that writes text frames speaks JSON: its show state is an
/// object (composite map keys as JSON text), and its JSON commands work.
#[test]
fn json_clients_get_json_snapshots_and_can_send_commands() {
    let mut engine = TestEngine::start(small_rig);
    engine.edit_show(|show| {
        show.0.overrides.insert((0, 5), 77);
    });
    let server = engine.start_server(loopback());
    let mut ws = raw_socket(&server);
    ws.send(Message::Text(
        json!({"Hello": {"protocol_version": PROTOCOL_VERSION, "streams": ["Dmx"]}}).to_string(),
    ))
    .unwrap();

    let read_json = |ws: &mut WebSocket<TcpStream>| loop {
        match ws.read().unwrap() {
            Message::Text(text) => break serde_json::from_str::<Value>(&text).unwrap(),
            Message::Binary(_) => panic!("binary frame on a JSON connection"),
            _ => continue,
        }
    };
    let welcome = read_json(&mut ws);
    assert_eq!(
        welcome["Welcome"]["protocol_version"],
        json!(PROTOCOL_VERSION)
    );
    assert!(welcome["Welcome"]["client"]["Remote"].is_u64(), "{welcome}");

    ws.get_mut()
        .set_read_timeout(Some(Duration::from_millis(20)))
        .unwrap();
    let command = json!({"Command": {"seq": 7, "body": {"Control": {
        "originator": "Web", "body": {"SelectGroup": 1}
    }}}});
    ws.send(Message::Text(command.to_string())).unwrap();

    let (mut show, mut result, mut dmx) = (None, None, None);
    let deadline = Instant::now() + Duration::from_secs(5);
    while show.is_none() || result.is_none() || dmx.is_none() {
        assert!(Instant::now() < deadline, "show {show:?} result {result:?}");
        engine.tick();
        let message = match ws.read() {
            Ok(Message::Text(text)) => serde_json::from_str::<Value>(&text).unwrap(),
            Ok(_) => continue,
            Err(tungstenite::Error::Io(err))
                if matches!(
                    err.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) =>
            {
                continue
            }
            Err(err) => panic!("{err}"),
        };
        if let Some(snapshot) = message.get("Show") {
            show = Some(snapshot.clone());
        } else if let Some(reply) = message.get("Result") {
            result = Some(reply.clone());
        } else if let Some(universes) = message.get("Dmx") {
            dmx = Some(universes.clone());
        }
    }
    let show = show.unwrap();
    assert!(show["digest"].is_u64());
    assert_eq!(show["state"]["overrides"]["[0,5]"], json!(77));
    assert_eq!(show["state"]["groups"]["1"]["name"], json!("G1"));
    assert_eq!(
        result.unwrap(),
        json!({"client": welcome["Welcome"]["client"], "seq": 7, "outcome": {"Ok": {"created": null}}})
    );
    assert_eq!(dmx.unwrap()[0]["channels"].as_array().unwrap().len(), 513);
}

#[test]
fn a_client_of_another_protocol_version_is_rejected() {
    let engine = TestEngine::start(small_rig);
    let server = engine.start_server(loopback());
    let mut ws = raw_socket(&server);
    send(
        &mut ws,
        Encoding::Bincode,
        &ClientMessage::Hello {
            protocol_version: PROTOCOL_VERSION + 1,
            streams: Vec::new(),
        },
    );
    let reply = loop {
        match ws.read().unwrap() {
            Message::Binary(bytes) => {
                break Encoding::decode::<ServerMessage>(&Frame::Binary(bytes)).unwrap()
            }
            _ => continue,
        }
    };
    match reply {
        ServerMessage::Rejected {
            protocol_version,
            reason,
        } => {
            assert_eq!(protocol_version, PROTOCOL_VERSION);
            assert!(reason.contains("not supported"), "{reason}");
        }
        other => panic!("{other:?}"),
    }
    // The server closes the connection.
    assert!(matches!(ws.read(), Ok(Message::Close(_)) | Err(_)));
    assert!(server.clients().is_empty());
    let error = RemoteClient::connect("127.0.0.1:1", &[]).err().unwrap();
    assert!(error.to_string().contains("Could not connect"), "{error}");
}

/// The desk's own commands (its showfile section) are refused from remote
/// clients, local or over the wire; the desk's still goes through.
#[test]
fn desk_only_commands_are_refused_from_other_clients() {
    let mut engine = TestEngine::start(small_rig);
    let desk = engine.connect_desk();
    let local_remote = engine.connect_remote();
    let server = engine.start_server(loopback());
    let remote = connect(&mut engine, &server, &[]);
    let section = || HostCommand::SetFrontendSection {
        generation: 1,
        section: ShowfileUiState {
            main_screen_desktop_mode: None,
            external_screens: Vec::new(),
        },
    };
    let refused = CommandOutcome::Refused(DESK_ONLY_REFUSAL.to_string());
    assert_eq!(engine.round_trip(&local_remote, section()), refused);
    assert_eq!(round_trip(&mut engine, &remote, section()), refused);
    // The desk's goes to the persistence service, which takes it at once.
    assert_eq!(engine.round_trip(&desk, section()), OK);
}

/// Commands waiting for a result fail when the connection drops, and so do
/// commands sent afterwards, so reply closures never wait forever.
#[test]
fn waiting_commands_fail_when_the_connection_drops() {
    let mut engine = TestEngine::start(small_rig);
    let server = engine.start_server(loopback());
    let remote = connect(&mut engine, &server, &[]);
    assert_eq!(remote.status(), RemoteStatus::Connected);

    #[derive(Default)]
    struct Dialog {
        handlers: ReplyHandlers<Dialog>,
        error: Option<String>,
    }
    let mut dialog = Dialog::default();
    // Not ticked: no result comes back before the server stops.
    dialog.handlers.send_then(
        &remote,
        HostCommand::Ping,
        |dialog: &mut Dialog, outcome| {
            if let CommandOutcome::Refused(why) = outcome {
                dialog.error = Some(why.clone());
            }
        },
    );
    std::thread::sleep(Duration::from_millis(50));
    server.stop();
    let deadline = Instant::now() + Duration::from_secs(5);
    while remote.is_connected() {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(matches!(remote.status(), RemoteStatus::Disconnected(_)));
    let results = remote.take_results();
    ReplyHandlers::dispatch(&mut dialog, |dialog| &mut dialog.handlers, &results);
    assert_eq!(dialog.error.as_deref(), Some(DISCONNECTED));
    assert!(dialog.handlers.is_empty());

    let late = remote.send(HostCommand::Ping);
    let results = remote.take_results();
    assert_eq!(results.len(), 1);
    assert_eq!(
        (results[0].seq, &results[0].outcome),
        (late, &CommandOutcome::Refused(DISCONNECTED.to_string()))
    );
}

/// A remote client times its commands' round trips, and `reconnect` opens a
/// new connection to the same engine, in a new session, with the same
/// streams.
#[test]
fn a_remote_client_times_round_trips_and_reconnects_in_a_new_session() {
    let mut engine = TestEngine::start(small_rig);
    let server = engine.start_server(loopback());
    let remote = connect(&mut engine, &server, &[Stream::Status]);
    assert_eq!(remote.stats().round_trips, 0);

    assert_eq!(round_trip(&mut engine, &remote, HostCommand::Ping), OK);
    assert_eq!(round_trip(&mut engine, &remote, HostCommand::Ping), OK);
    let stats = remote.stats();
    assert_eq!(stats.round_trips, 2);
    assert_eq!(stats.recent_round_trips.len(), 2);
    assert!(stats
        .recent_round_trips
        .iter()
        .all(|rtt| *rtt < Duration::from_secs(5)));

    let again = remote.reconnect().expect("reconnect");
    engine.tick_until("the new session", |_| {
        again.with_session(|_, _| ()).is_some()
    });
    assert_ne!(again.session(), remote.session());
    assert_ne!(again.client(), remote.client());
    assert_eq!(again.streams(), vec![Stream::Status]);
    assert_eq!(server.clients().len(), 2);
}

/// The spectrogram stream mirrors the engine's time-stamped columns: only
/// new ones go out, also when stamps repeat and the engine trims old columns
/// by age, and a restart of the engine's spectrogram starts the mirror over.
#[test]
fn the_spectrogram_stream_mirrors_stamped_columns_through_age_trimming() {
    let mut engine = TestEngine::start(small_rig);
    let server = engine.start_server(loopback());
    let app = Arc::clone(engine.app());
    let push = |times_ms: &[u64]| {
        let mut spec = app.audio_spectrogram.write().unwrap();
        for &time_ms in times_ms {
            spec.push_data_at(time_ms, CollectorOutput::default());
        }
    };
    let engine_times = || {
        app.audio_spectrogram
            .read()
            .unwrap()
            .column_times_ms
            .clone()
    };

    app.audio_spectrogram.write().unwrap().restart_live(
        Instant::now(),
        Duration::from_millis(200),
        10_000,
    );
    push(&(0..50).map(|i| i * 20).collect::<Vec<_>>());
    let remote = connect(&mut engine, &server, &[Stream::Spectrogram]);
    engine.tick_until("the first columns", |_| {
        remote.with_spectrogram(|spec| spec.column_times_ms.clone()) == engine_times()
    });
    remote.with_spectrogram(|spec| {
        assert_eq!(spec.window, Some(Duration::from_millis(200)));
        assert!(spec.epoch.is_some());
    });

    // Batches with a repeated stamp each, while the engine keeps only the
    // last 1.2 s (window plus slack), so its ring is trimmed all along.
    for batch in 0..10u64 {
        let start = 1_000 + batch * 400;
        push(&[start, start, start + 200]);
        engine.tick_until("a batch", |_| {
            remote.with_spectrogram(|spec| spec.column_times_ms.clone()) == engine_times()
        });
    }
    let times = engine_times();
    assert_eq!(times.back(), Some(&4_800));
    assert!(times.front().copied().unwrap() >= 4_800 - 1_200);

    // The engine's spectrogram starts over at engine time 0.
    app.audio_spectrogram.write().unwrap().restart_live(
        Instant::now(),
        Duration::from_millis(200),
        10_000,
    );
    push(&[0, 20, 40]);
    engine.tick_until("the restart", |_| {
        remote.with_spectrogram(|spec| spec.column_times_ms.clone()) == [0, 20, 40]
    });
}

/// Every message type survives both encodings where a Rust client reads it,
/// and the forms a JSON client sees are the documented ones.
#[test]
fn wire_types_round_trip() {
    let commands = vec![
        HostCommand::ImportStageModel {
            file_name: "box.glb".to_string(),
            bytes: ModelBytes(Arc::from(&[1u8, 2, 3][..])),
        },
        HostCommand::SaveShowfileAs(PathBuf::from("/shows/a.json")),
        HostCommand::AddArtNetReceiver("10.0.0.5:6454".parse().unwrap()),
        HostCommand::SetFrontendSection {
            generation: 3,
            section: ShowfileUiState {
                main_screen_desktop_mode: None,
                external_screens: Vec::new(),
            },
        },
    ];
    for encoding in [Encoding::Bincode, Encoding::Json] {
        for command in &commands {
            let message = ClientMessage::Command {
                seq: 1,
                body: BusBody::Host(command.clone()),
            };
            let decoded: ClientMessage =
                Encoding::decode(&encoding.encode(&message).unwrap()).unwrap();
            assert_eq!(format!("{decoded:?}"), format!("{message:?}"), "{encoding}");
        }
    }

    // The show state is the plugin ABI's bytes in bincode.
    let mut state = blaulicht_shared::EngineState::default();
    state.overrides.insert((1, 2), 3);
    let message = ServerMessage::Show(super::protocol::ShowSnapshot {
        digest: 9,
        state: ShowState::Decoded(Box::new(state.clone())),
    });
    let Frame::Binary(bytes) = Encoding::Bincode.encode(&message).unwrap() else {
        panic!()
    };
    let abi = state.serialize();
    assert!(bytes.windows(abi.len()).any(|window| window == abi));
    match Encoding::decode::<ServerMessage>(&Frame::Binary(bytes)).unwrap() {
        ServerMessage::Show(snapshot) => {
            let decoded = snapshot.state.into_state().unwrap();
            assert_eq!(decoded.overrides.get(&(1, 2)), Some(&3));
        }
        other => panic!("{other:?}"),
    }

    // `saved_at` is UNIX milliseconds.
    let saved_at = UNIX_EPOCH + Duration::from_millis(1_700_000_000_123);
    let status = WireSystemMessage::ShowfileStatus(ShowfileStatus {
        path: Some(PathBuf::from("/shows/a.json")),
        save: SaveStatus::Clean,
        saved_at: Some(saved_at),
    });
    let Frame::Text(text) = Encoding::Json
        .encode(&ServerMessage::System(status.clone()))
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(
        serde_json::from_str::<Value>(&text).unwrap(),
        json!({"System": {"ShowfileStatus": {
            "path": "/shows/a.json", "save": "Clean", "saved_at": 1_700_000_000_123u64
        }}})
    );
    for encoding in [Encoding::Bincode, Encoding::Json] {
        let frame = encoding
            .encode(&ServerMessage::System(status.clone()))
            .unwrap();
        match Encoding::decode::<ServerMessage>(&frame).unwrap() {
            ServerMessage::System(decoded) => assert_eq!(decoded, status),
            other => panic!("{other:?}"),
        }
    }
    assert!(SystemTime::now() > saved_at);
}

/// Sends a raw HTTP `request` to the server's port and returns the status,
/// the headers (names lowercased) and the body; the server closes the
/// connection after answering.
fn http(server: &ServerHandle, request: &str) -> (u16, Vec<(String, String)>, Vec<u8>) {
    use std::io::{Read, Write};
    let mut stream = TcpStream::connect(server.local_addr()).expect("connect");
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    stream.write_all(request.as_bytes()).unwrap();
    let mut answer = Vec::new();
    stream.read_to_end(&mut answer).unwrap();
    let split = answer
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .expect("a response head");
    let head = String::from_utf8(answer[..split].to_vec()).unwrap();
    let body = answer[split + 4..].to_vec();
    let mut lines = head.split("\r\n");
    let status = lines
        .next()
        .unwrap()
        .split(' ')
        .nth(1)
        .unwrap()
        .parse()
        .unwrap();
    let headers = lines
        .filter_map(|line| line.split_once(": "))
        .map(|(name, value)| (name.to_ascii_lowercase(), value.to_string()))
        .collect();
    (status, headers, body)
}

fn header<'a>(headers: &'a [(String, String)], name: &str) -> Option<&'a str> {
    headers
        .iter()
        .find(|(key, _)| key == name)
        .map(|(_, value)| value.as_str())
}

/// Without a web bundle, the port still answers plain HTTP: `/` says how to
/// build the web UI, everything else is not there, and only GET and HEAD
/// are served.
#[test]
fn the_port_explains_a_missing_web_bundle() {
    let engine = TestEngine::start(small_rig);
    let server = engine.start_server(loopback());

    let (status, headers, body) = http(&server, "GET / HTTP/1.1\r\nHost: x\r\n\r\n");
    assert_eq!(status, 200);
    assert_eq!(
        header(&headers, "content-type"),
        Some("text/html; charset=utf-8")
    );
    assert!(String::from_utf8_lossy(&body).contains("the web UI is not built"));

    let (status, _, _) = http(&server, "GET /blaulicht_web.js HTTP/1.1\r\nHost: x\r\n\r\n");
    assert_eq!(status, 404);
    let (status, headers, _) = http(&server, "POST / HTTP/1.1\r\nHost: x\r\n\r\n");
    assert_eq!(
        (status, header(&headers, "allow")),
        (405, Some("GET, HEAD"))
    );
}

/// The port serves the web bundle to browsers (gzip when they take it,
/// revalidation by ETag, nothing outside the bundle) and remote clients on
/// the same port at the same time.
#[test]
fn the_port_serves_the_web_bundle_and_remote_clients() {
    let bundle = tempdir::TempDir::new("web-bundle").unwrap();
    let file =
        |name: &str, contents: &[u8]| std::fs::write(bundle.path().join(name), contents).unwrap();
    file("index.html", b"<html>web ui</html>");
    file("blaulicht_web.js", b"export default 1;");
    file("blaulicht_web_bg.wasm", b"\0asm raw");
    file("blaulicht_web_bg.wasm.gz", b"gzipped");
    std::fs::write(bundle.path().parent().unwrap().join("secret.txt"), b"no").unwrap();

    let mut engine = TestEngine::start(small_rig);
    let server = engine.start_server(ServerOptions {
        web_root: Some(bundle.path().to_path_buf()),
        ..loopback()
    });

    let (status, headers, body) = http(&server, "GET / HTTP/1.1\r\nHost: x\r\n\r\n");
    assert_eq!(
        (status, body.as_slice()),
        (200, b"<html>web ui</html>".as_slice())
    );
    assert_eq!(header(&headers, "cache-control"), Some("no-cache"));
    let (_, headers, _) = http(
        &server,
        "GET /blaulicht_web.js?v=1 HTTP/1.1\r\nHost: x\r\n\r\n",
    );
    assert_eq!(
        header(&headers, "content-type"),
        Some("text/javascript; charset=utf-8")
    );

    let wasm = "GET /blaulicht_web_bg.wasm HTTP/1.1\r\nHost: x\r\n";
    let (status, headers, body) = http(
        &server,
        &format!("{wasm}Accept-Encoding: deflate, gzip;q=1.0\r\n\r\n"),
    );
    assert_eq!((status, body.as_slice()), (200, b"gzipped".as_slice()));
    assert_eq!(header(&headers, "content-type"), Some("application/wasm"));
    assert_eq!(header(&headers, "content-encoding"), Some("gzip"));
    let (_, headers, body) = http(&server, &format!("{wasm}\r\n"));
    assert_eq!(body, b"\0asm raw");
    assert_eq!(header(&headers, "content-encoding"), None);

    let etag = header(&headers, "etag").expect("an etag").to_string();
    let (status, _, body) = http(&server, &format!("{wasm}If-None-Match: {etag}\r\n\r\n"));
    assert_eq!((status, body.len()), (304, 0));

    let (status, headers, body) = http(&server, "HEAD / HTTP/1.1\r\nHost: x\r\n\r\n");
    assert_eq!((status, body.len()), (200, 0));
    assert_eq!(header(&headers, "content-length"), Some("19"));

    for escape in [
        "/../secret.txt",
        "/%2e%2e/secret.txt",
        "//etc/passwd",
        "/a/../../secret.txt",
    ] {
        let (status, _, _) = http(
            &server,
            &format!("GET {escape} HTTP/1.1\r\nHost: x\r\n\r\n"),
        );
        assert_eq!(status, 404, "{escape}");
    }

    // A remote client on the same port.
    let remote = connect(&mut engine, &server, &[]);
    assert_eq!(round_trip(&mut engine, &remote, HostCommand::Ping), OK);
}

/// A file larger than the socket's send buffer arrives whole even when the
/// browser is slow to read. On macOS an accepted socket inherited the
/// listener's non-blocking mode, so the answer was cut off at the send
/// buffer (ERR_CONTENT_LENGTH_MISMATCH in the browser).
#[test]
fn the_port_serves_large_files_to_slow_readers() {
    use std::io::{Read, Write};
    let bundle = tempdir::TempDir::new("web-bundle").unwrap();
    std::fs::write(bundle.path().join("index.html"), b"<html></html>").unwrap();
    let wasm: Vec<u8> = (0..8u32 << 20).map(|i| (i % 251) as u8).collect();
    std::fs::write(bundle.path().join("blaulicht_web_bg.wasm"), &wasm).unwrap();

    let engine = TestEngine::start(small_rig);
    let server = engine.start_server(ServerOptions {
        web_root: Some(bundle.path().to_path_buf()),
        ..loopback()
    });

    let mut stream = TcpStream::connect(server.local_addr()).expect("connect");
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    stream
        .write_all(b"GET /blaulicht_web_bg.wasm HTTP/1.1\r\nHost: x\r\n\r\n")
        .unwrap();
    // Let the server fill its send buffer before anything is read.
    std::thread::sleep(Duration::from_millis(300));
    let mut answer = Vec::new();
    stream.read_to_end(&mut answer).unwrap();
    let split = answer
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .expect("a response head")
        + 4;
    assert!(answer.starts_with(b"HTTP/1.1 200"));
    assert_eq!(answer.len() - split, wasm.len(), "the whole file arrives");
    assert!(answer[split..] == wasm[..]);
}
