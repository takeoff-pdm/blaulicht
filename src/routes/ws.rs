use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::Duration, // time::Duration,
};

// use actix::{Actor, StreamHandler};
use actix_web::{
    rt::{self, task::yield_now},
    web::{self, Data},
    HttpRequest, HttpResponse, Result,
};
// use actix_web_actors::ws;
use actix_ws::AggregatedMessage;
use cpal::traits::DeviceTrait;
use crossbeam_channel::TryRecvError;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    app::{FromFrontend, WSMatrixControlBody},
    audio::{Signal, SystemMessage, UnifiedMessage, SIGNAL_SPEED},
    utils::device_from_name,
};

use super::AppState;

// struct MyWebSocket {
//     app_state: web::Data<AppState>,
// }

//
// From frontend message.
//

#[derive(Deserialize, Clone, Debug)]
pub enum WSFromFrontendKind {
    SelectAudioDevice,
    SelectSerialDevice,
    Reload,
    MatrixControl,
}

#[derive(Deserialize, Clone, Debug)]
pub struct WSFromFrontend {
    kind: WSFromFrontendKind,
    value: serde_json::Value,
}

// TODO: json deserial.
impl From<WSFromFrontend> for FromFrontend {
    fn from(value: WSFromFrontend) -> Self {
        match value.kind {
            WSFromFrontendKind::Reload => Self::Reload,
            WSFromFrontendKind::MatrixControl => {
                let control: WSMatrixControlBody = serde_json::from_value(value.value).unwrap();
                Self::MatrixControl(control)
            }
            WSFromFrontendKind::SelectAudioDevice => {
                if value.value == serde_json::Value::Null {
                    Self::SelectInputDevice(None)
                } else {
                    let serde_json::Value::String(device_name) = value.value else {
                        panic!("not a string");
                    };

                    println!("select device convert: {}", &device_name);
                    let device = device_from_name(device_name);
                    println!("device is some: {}", device.is_some());
                    Self::SelectInputDevice(device)
                }
            }
            WSFromFrontendKind::SelectSerialDevice => {
                if value.value == serde_json::Value::Null {
                    Self::SelectSerialDevice(None)
                } else {
                    let device = value.value.to_string();
                    Self::SelectSerialDevice(Some(device))
                }
            }
        }
    }
}

//
// To frontend signal.
//

#[derive(Serialize, Hash, Eq, PartialEq, Clone, Copy)]
pub enum WSSignalKind {
    Bpm,
    BeatVolume,
    Bass,
    BassAvg,
    Volume,
    DMX,
}

#[derive(Serialize)]
pub struct WSSignal {
    kind: WSSignalKind,
    value: u8,
}

impl From<Signal> for WSSignal {
    fn from(value: Signal) -> Self {
        match value {
            Signal::Bpm(value) => Self {
                kind: WSSignalKind::Bpm,
                value: value.bpm,
            },
            Signal::BeatVolume(value) => Self {
                kind: WSSignalKind::BeatVolume,
                value,
            },
            Signal::Bass(value) => Self {
                kind: WSSignalKind::Bass,
                value,
            },
            Signal::BassAvg(value) => Self {
                kind: WSSignalKind::BassAvg,
                value,
            },
            Signal::Volume(value) => Self {
                kind: WSSignalKind::Volume,
                value,
            },
        }
    }
}

//
// To frontent message.
//

#[derive(Serialize)]
pub enum WSSystemMessageKind {
    Heartbeat,
    Log,
    WasmLog,
    WasmControlsLog,
    WasmControlsSet,
    WasmControlsConfig,
    TickSpeed,
    LoopSpeed,
    AudioSelected,
    AudioDevicesView,
    SerialSelected,
    SerialDevicesView,
    Dmx,
}

#[derive(Serialize)]
pub struct WSSystemMessage {
    kind: WSSystemMessageKind,
    value: serde_json::Value,
}

impl From<SystemMessage> for WSSystemMessage {
    fn from(value: SystemMessage) -> Self {
        match value {
            SystemMessage::Heartbeat(seq) => Self {
                kind: WSSystemMessageKind::Heartbeat,
                value: serde_json::to_value(seq).unwrap(),
            },
            SystemMessage::Log(msg) => Self {
                kind: WSSystemMessageKind::Log,
                value: serde_json::to_value(msg).unwrap(),
            },
            SystemMessage::WasmLog(msg) => Self {
                kind: WSSystemMessageKind::WasmLog,
                value: serde_json::to_value(msg).unwrap(),
            },
            SystemMessage::WasmControlsLog(msg) => Self {
                kind: WSSystemMessageKind::WasmControlsLog,
                value: serde_json::to_value(msg).unwrap(),
            },
            SystemMessage::WasmControlsSet(msg) => Self {
                kind: WSSystemMessageKind::WasmControlsSet,
                value: serde_json::to_value(msg).unwrap(),
            },
            SystemMessage::WasmControlsConfig(msg) => Self {
                kind: WSSystemMessageKind::WasmControlsConfig,
                value: serde_json::to_value(msg).unwrap(),
            },
            SystemMessage::TickSpeed(duration) => Self {
                kind: WSSystemMessageKind::TickSpeed,
                value: serde_json::to_value(duration.as_micros() as u64).unwrap(),
            },
            SystemMessage::LoopSpeed(duration) => Self {
                kind: WSSystemMessageKind::LoopSpeed,
                value: serde_json::to_value(duration.as_micros() as u64).unwrap(),
            },
            SystemMessage::AudioSelected(device) => Self {
                kind: WSSystemMessageKind::AudioDevicesView,
                value: match device {
                    Some(d) => serde_json::to_value(d.name().unwrap()).unwrap(),
                    None => serde_json::Value::Null,
                },
            },
            SystemMessage::AudioDevicesView(devs) => Self {
                kind: WSSystemMessageKind::AudioDevicesView,
                value: serde_json::to_value(
                    devs.iter()
                        .map(|d| d.1.name().unwrap())
                        .collect::<Vec<String>>(),
                )
                .unwrap(),
            },
            SystemMessage::SerialSelected(serial_port_info) => Self {
                kind: WSSystemMessageKind::SerialSelected,
                value: match serial_port_info {
                    Some(d) => serde_json::to_value(d.port_name).unwrap(),
                    None => serde_json::Value::Null,
                },
            },

            SystemMessage::SerialDevicesView(devs) => Self {
                kind: WSSystemMessageKind::AudioDevicesView,
                value: serde_json::to_value(
                    devs.iter()
                        .map(|d| d.port_name.clone())
                        .collect::<Vec<String>>(),
                )
                .unwrap(),
            },
            SystemMessage::DMX(chans) => Self {
                kind: WSSystemMessageKind::Dmx,
                value: serde_json::to_value(chans.to_vec()).unwrap(),
            },
        }
    }
}

pub async fn ws_handler(
    req: HttpRequest,
    data: Data<AppState>,
    stream: web::Payload,
) -> Result<HttpResponse, actix_web::Error> {
    let (res, mut session, stream) = actix_ws::handle(&req, stream)?;

    println!("Open websocket");

    // add consumer:
    let (unified_sender, unified_receiver) = crossbeam_channel::unbounded();
    let ip = req.connection_info().peer_addr().unwrap().to_string();
    let id = Uuid::new_v4().to_string();
    println!("[ws] new ip connected: {ip}: {id}");
    {
        let mut consumers = data.to_frontend_consumers.lock().unwrap();
        consumers.insert(id.clone(), unified_sender);
    }

    let mut stream = stream
        .aggregate_continuations()
        // aggregate continuation frames up to 1MiB
        .max_continuation_size(2_usize.pow(20));

    let mut session2 = session.clone();

    let b = Arc::new(Mutex::new(true));

    let a = b.clone();

    rt::spawn(async move {
        // let mut last_sent_value: HashMap<WSSignalKind, u8> = HashMap::new();

        loop {
            {
                if !*a.lock().unwrap() {
                    println!("signal loop killed");
                    break;
                }
            }

            match unified_receiver.try_recv() {
                Ok(sys) => match sys {
                    UnifiedMessage::Signal(signal) => {
                        let ws_signal = WSSignal::from(signal);

                        // if let Some(prev) = last_sent_value.get(&ws_signal.kind) {
                        // // Bigger change.
                        // if (*prev as i16 - ws_signal.value as i16).abs() > 5 {
                        //     last_sent_value.insert(ws_signal.kind, ws_signal.value);

                        session2
                            .text(serde_json::to_string(&ws_signal).unwrap())
                            .await
                            .unwrap();
                        // }
                        // }
                    }
                    UnifiedMessage::System(SystemMessage::DMX(_)) => {}
                    UnifiedMessage::System(system_message) => {
                        let ws_system = WSSystemMessage::from(system_message);
                        session2
                            .text(serde_json::to_string(&ws_system).unwrap())
                            .await
                            .unwrap();
                    }
                },
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => unreachable!(),
            }

            yield_now().await;
        }
    });

    // start task but don't wait for it
    let from_frontend_sender = data.from_frontend_sender.clone();
    rt::spawn(async move {
        // receive messages from websocket
        println!("waiting for message...");
        while let Some(msg) = stream.recv().await {
            match msg {
                Ok(AggregatedMessage::Text(text)) => {
                    // echo text message
                    session.text(text.clone()).await.unwrap();

                    let msg: WSFromFrontend =
                        serde_json::from_str(text.to_string().as_str()).unwrap();

                    from_frontend_sender.send(msg.clone().into()).unwrap();
                    println!("recv ws: {msg:?}");
                }

                Ok(AggregatedMessage::Binary(bin)) => {
                    // echo binary message
                    session.binary(bin).await.unwrap();
                }

                Ok(AggregatedMessage::Ping(msg)) => {
                    // respond to PING frame with PONG frame
                    session.pong(&msg).await.unwrap();
                }
                Ok(AggregatedMessage::Close(e)) => {
                    println!("close: {e:?}");
                    break;
                }
                Ok(AggregatedMessage::Pong(_)) => {}
                Err(e) => {
                    println!("error: {e:?}");
                    break;
                }
            }
        }

        println!("Websocket was killed.");
        let mut a = b.lock().unwrap();
        *a = false;

        {
            let mut consumers = data.to_frontend_consumers.lock().unwrap();
            consumers.remove(&id);
        }
    });

    Ok(res)
}

pub async fn binary_ws_handler(
    req: HttpRequest,
    data: Data<AppState>,
    stream: web::Payload,
) -> Result<HttpResponse, actix_web::Error> {
    let (res, mut session, stream) = actix_ws::handle(&req, stream)?;

    println!("Open websocket");

    // add consumer:
    let (unified_sender, unified_receiver) = crossbeam_channel::unbounded();
    let ip = req.connection_info().peer_addr().unwrap().to_string();
    let id = Uuid::new_v4().to_string();
    println!("[ws] new ip connected: {ip}: {id}");
    {
        let mut consumers = data.to_frontend_consumers.lock().unwrap();
        consumers.insert(id.clone(), unified_sender);
    }

    let mut stream = stream
        .aggregate_continuations()
        // aggregate continuation frames up to 1MiB
        .max_continuation_size(2_usize.pow(20));

    let mut session2 = session.clone();

    let b = Arc::new(Mutex::new(true));

    let a = b.clone();

    rt::spawn(async move {
        loop {
            {
                if !*a.lock().unwrap() {
                    println!("DMX ws loop killed");
                    break;
                }
            }

            match unified_receiver.try_recv() {
                Ok(sys) => match sys {
                    UnifiedMessage::System(SystemMessage::DMX(dat)) => {
                        let mut vec = dat.to_vec();
                        vec.remove(0);
                        match session2.binary(vec).await {
                            Ok(_) => {}
                            Err(_) => break,
                        }
                    }
                    _ => {}
                },
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => unreachable!(),
            }

            yield_now().await;
        }
    });

    rt::spawn(async move {
        // receive messages from websocket
        println!("waiting for message...");
        while let Some(msg) = stream.recv().await {
            match msg {
                Ok(AggregatedMessage::Ping(msg)) => {
                    // respond to PING frame with PONG frame
                    session.pong(&msg).await.unwrap();
                }
                Ok(AggregatedMessage::Close(e)) => {
                    println!("close: {e:?}");
                    break;
                }
                Ok(_) => {}
                Err(e) => {
                    println!("error: {e:?}");
                    break;
                }
            }
        }

        println!("Websocket was killed.");
        let mut a = b.lock().unwrap();
        *a = false;

        {
            let mut consumers = data.to_frontend_consumers.lock().unwrap();
            consumers.remove(&id);
        }
    });

    Ok(res)
}
