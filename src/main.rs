use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};
use std::{mem, thread};

use actix::System;
use actix_files::Files;
use actix_web::rt::task;
use actix_web::web::{self, Data};
use actix_web::{App, HttpServer};
use anyhow::{anyhow, Context};
use blaulicht::app::FromFrontend;
use blaulicht::audio::{AudioThreadControlSignal, Signal, SystemMessage, UnifiedMessage};
use blaulicht::routes::AppState;
use blaulicht::wasm::TickInput;
use blaulicht::{config, dmx, midi, routes, wasm};
use crossbeam_channel::{Sender, TryRecvError};
use env_logger::Env;
use log::info;
use notify::event::{DataChange, ModifyKind};
use notify::{Event, RecursiveMode, Watcher};

// fn main() -> anyhow::Result<()> {
//     use std::{
//         net::UdpSocket,
//         sync::{atomic::AtomicU8, mpsc, Arc},
//         thread,
//     };
//
//     use anyhow::bail;
//     use blaulicht::{app, config};
//     // Log to stderr (if you run with `RUST_LOG=debug`).
//     use egui::TextBuffer;
//     env_logger::init();
//
//     // Load config
//     let config_path = config::config_path()?;
//     let Some(config) = config::read_config(config_path.clone())? else {
//         eprintln!(
//             "Created default config file at `{}`",
//             config_path.to_string_lossy()
//         );
//         return Ok(());
//     };
//
//     let (from_frontend_sender, from_frontend_receiver) = crossbeam_channel::unbounded();
//     let (app_signal_out, app_signal_receiver) = crossbeam_channel::unbounded();
//
//     let (system_out, _system_receiver) = crossbeam_channel::unbounded();
//     let audio_thread_control_signal = Arc::new(AtomicU8::new(AudioThreadControlSignal::CONTINUE));
//
//     {
//         // Audio recording and analysis thread.
//         let system_out = system_out.clone();
//         let audio_thread_control_signal = audio_thread_control_signal.clone();
//         thread::spawn(|| {
//             dmx::audio_thread(
//                 from_frontend_receiver,
//                 audio_thread_control_signal,
//                 app_signal_out,
//                 system_out,
//             )
//         });
//     }
//
//     let crate_name = env!("CARGO_CRATE_NAME");
//     let crate_version = env!("CARGO_PKG_VERSION");
//     BlaulichtApp::new(
//         cc,
//         from_frontend_sender,
//         audio_thread_control_signal,
//         app_signal_receiver,
//         _system_receiver,
//         config,
//     );
//
//     Ok(())
// }

#[actix_web::main]
async fn main() -> anyhow::Result<()> {
    let config_filepath = "./config.toml";

    let cfg = config::read_config(config_filepath.into())?;
    let Some(cfg) = cfg else {
        info!("Created default configuration file at {config_filepath}");
        return Ok(());
    };

    env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();

    //
    // Audio.
    //
    // let (to_frontend_sender, to_frontend_receiver) = crossbeam_channel::bounded(10);

    let (from_frontend_sender, from_frontend_receiver) = crossbeam_channel::unbounded();
    let (app_signal_out, app_signal_receiver) = crossbeam_channel::unbounded();

    let (system_out, app_system_receiver) = crossbeam_channel::unbounded();
    let audio_thread_control_signal = Arc::new(AtomicU8::new(AudioThreadControlSignal::CONTINUE));

    let (midi_in_sender, midi_in_receiver) = crossbeam_channel::bounded(100);
    let (midi_out_sender, midi_out_receiver) = crossbeam_channel::bounded(10);

    let send = midi_in_sender.clone();
    thread::spawn(move || {
        loop {
            match midi::midi(send, midi_out_receiver.clone()) {
                Ok(_) => panic!("Unreachable."),
                Err(err) => {
                    log::error!("MIDI thread crashed! {err:?}");
                    break;
                }
            }
        }

        // Debug midi thread.
        // Allows the dev to se what MIDI messages are sent to the device.
        loop {
            thread::sleep(Duration::from_millis(50));
            match midi_out_receiver.try_recv() {
                Ok(_midi) => {
                    // TODO: include if required
                    // println!("MIDI to dev: {_midi:?}")
                }
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => {
                    log::warn!("[MIDI] Shutting down.");
                    break;
                }
            }
        }
    });

    {
        // Audio recording and analysis thread.
        let system_out = system_out.clone();
        let audio_thread_control_signal = audio_thread_control_signal.clone();
        let send = midi_in_sender.clone();
        thread::spawn(move || {
            dmx::audio_thread(
                from_frontend_receiver,
                audio_thread_control_signal,
                app_signal_out,
                system_out,
                midi_in_receiver,
                send,
                midi_out_sender,
                // to_frontend_sender,
            )
        });
    }

    //
    // End audio.
    //

    let consumers: Arc<Mutex<HashMap<String, Sender<UnifiedMessage>>>> =
        Arc::new(Mutex::new(HashMap::new()));

    // app_system_receiver,
    // Spawn broadcast thread.

    let consumers2 = consumers.clone();
    thread::spawn(move || loop {
        //
        // System messages.
        //
        let system_res = app_system_receiver.try_recv();
        match system_res {
            Ok(res) => {
                match &res {
                    SystemMessage::Log(content) => log::info!("{content}"),
                    _ => {}
                }
                let consumers = consumers2.lock().unwrap();
                for c in consumers.values() {
                    if let Err(e) = c.send(UnifiedMessage::System(res.clone())) {
                        continue;
                    }
                }
            }
            Err(crossbeam_channel::TryRecvError::Empty) => {}
            Err(crossbeam_channel::TryRecvError::Disconnected) => unreachable!(),
        }

        //
        // Signal messages.
        //
        let signal_res = app_signal_receiver.try_recv();
        match signal_res {
            Ok(res) => {
                let consumers = consumers2.lock().unwrap();
                for c in consumers.values() {
                    if let Err(e) = c.send(UnifiedMessage::Signal(res.clone())) {
                        continue;
                    }
                }
            }
            Err(crossbeam_channel::TryRecvError::Empty) => {}
            Err(crossbeam_channel::TryRecvError::Disconnected) => {
                log::warn!("[BROADCAST] Shutting down.");
                break;
            }
        }
    });

    //
    // Filesystem watcher.
    //
    let (tx, rx) = mpsc::channel::<notify::Result<Event>>();

    // Use recommended_watcher() to automatically select the best implementation
    // for your platform. The `EventHandler` passed to this constructor can be a
    // closure, a `std::sync::mpsc::Sender`, a `crossbeam_channel::Sender`, or
    // another type the trait is implemented for.
    let tx = tx.clone();
    let mut watcher = notify::recommended_watcher(tx)?;

    // Add a path to be watched. All files and directories at that path and
    // below will be monitored for changes.
    watcher.watch(Path::new("./wasm/output.wasm"), RecursiveMode::NonRecursive)?;

    let send = from_frontend_sender.clone();
    thread::spawn(move || {
        // Block forever, printing out events as they come in
        for res in rx {
            match res {
                Ok(event) => match event.kind {
                    notify::EventKind::Modify(ModifyKind::Data(DataChange::Content)) => {
                        send.send(FromFrontend::Reload).unwrap();
                    }
                    _ => {}
                },
                Err(e) => println!("watch error: {:?}", e),
            }
        }
    });

    let data = Data::new(AppState {
        from_frontend_sender,
        // app_signal_receiver,
        to_frontend_consumers: consumers,
    });

    let server = HttpServer::new(move || {
        App::new()
            .app_data(data.clone())
            .service(Files::new("/assets", "./blaulicht-web/dist/assets/"))
            // HTML endpoints
            .service(routes::get_index)
            .service(routes::get_dash)
            // .service(routes::get_login)
            // .service(routes::get_dash)
            // .service(routes::get_food)
            // .service(routes::get_weight)
            // .service(routes::get_users)
            // API endpoints
            .service(routes::get_bpm)
            .service(routes::set_bpm)
            .route("/api/ws", web::get().to(routes::ws_handler))
            .route("/api/ws/sink", web::get().to(routes::binary_ws_handler))
        // .service(routes::logout)
        // .service(routes::list_users)
        // .service(routes::create_user)
        // .service(routes::modify_other_user_data)
        // .service(routes::delete_user)
        // .service(routes::get_personal_data)
        // .service(routes::modify_personal_data)
        // .service(routes::list_food)
        // .service(routes::list_starred_food)
        // .service(routes::search_food)
        // .service(routes::create_food)
        // .service(routes::star_food)
        // .service(routes::modify_food)
        // .service(routes::delete_food)
        // // weight
        // .service(routes::list_weight_history)
        // .service(routes::create_weight_measurement)
        // // has eaten
        // .service(routes::list_eaten)
        // .service(routes::eat_create)
        // .service(routes::modify_has_eaten)
        // .service(routes::delete_has_eaten)
        // // days
        // .service(routes::get_day_history)
    })
    .bind(("0.0.0.0", cfg.port))
    .with_context(|| "could not start webserver")?;

    info!("Blaulicht is running on `http://localhost:{}`", cfg.port,);

    server
        .run()
        .await
        .with_context(|| "Could not start webserver")?;

    info!("Blaulicht is shutting down...");
    mem::drop(watcher);

    let sig = audio_thread_control_signal.load(Ordering::Relaxed);
    if sig == AudioThreadControlSignal::CONTINUE {
        audio_thread_control_signal.store(AudioThreadControlSignal::ABORT, Ordering::Relaxed);
        loop {
            thread::sleep(Duration::from_secs(1));

            let sig = audio_thread_control_signal.load(Ordering::Relaxed);

            log::trace!("Waiting for audio thread to die: {sig}");

            if sig == AudioThreadControlSignal::ABORTED {
                break;
            }
        }
    }

    Ok(())
}
