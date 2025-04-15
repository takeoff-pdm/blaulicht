use std::{
    sync::{Arc, Mutex},
    // time::Duration,
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

use crate::{
    app::FromFrontend,
    audio::{Signal, SystemMessage},
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

#[derive(Serialize)]
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
                value,
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

    let mut stream = stream
        .aggregate_continuations()
        // aggregate continuation frames up to 1MiB
        .max_continuation_size(2_usize.pow(20));

    let data2 = data.clone();
    let mut session2 = session.clone();

    let b = Arc::new(Mutex::new(true));

    let a = b.clone();

    rt::spawn(async move {
        loop {
            {
                if !*a.lock().unwrap() {
                    println!("signal loop killed");
                    break;
                }
            }

            match data2.app_signal_receiver.try_recv() {
                Ok(signal) => {
                    // println!("app signal: ${signal:?}");
                    let ws_signal = WSSignal::from(signal);
                    session2
                        .text(serde_json::to_string(&ws_signal).unwrap())
                        .await
                        .unwrap();
                }
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => unreachable!(),
            }

            match data2.app_system_receiver.try_recv() {
                Ok(sys) => {
                    session2
                        .text(serde_json::to_string(&WSSystemMessage::from(sys)).unwrap())
                        .await
                        .unwrap();
                }
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => unreachable!(),
            }

            yield_now().await;
        }
    });

    // start task but don't wait for it
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

                    data.from_frontend_sender.send(msg.clone().into()).unwrap();
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
    });

    Ok(res)
}

pub async fn binary_ws_handler(
    req: HttpRequest,
    data: Data<AppState>,
    stream: web::Payload,
) -> Result<HttpResponse, actix_web::Error> {
    let (res, mut session, _) = actix_ws::handle(&req, stream)?;

    println!("Open websocket");

    // let mut stream = stream
    //     .aggregate_continuations()
    //     // aggregate continuation frames up to 1MiB
    //     .max_continuation_size(2_usize.pow(20));

    rt::spawn(async move {
        loop {
            match data.app_system_receiver.try_recv() {
                Ok(sys) => match sys {
                    SystemMessage::DMX(data) => {
                        let mut vec = data.to_vec();
                        vec.remove(0);
                        match session.binary(vec).await {
                            Ok(_) => {},
                            Err(_) => break,
                        }
                    }
                    _ => {}
                },
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => break,
            }

            yield_now().await;
        }
    });

    println!("Websocket was killed.");

    Ok(res)
}
