use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};
use std::{mem, thread};

use actix_files::Files;
use actix_web::web::{self, Data};
use actix_web::{App, HttpServer};
use actix_web_actors::ws::start;
use anyhow::Context;
use blaulicht::app::FromFrontend;
use blaulicht::audio::defs::AudioThreadControlSignal;
use blaulicht::msg::{SystemMessage, UnifiedMessage};
use blaulicht::routes::AppState;
use blaulicht::{config, dmx, midi, routes};
use crossbeam_channel::{Sender, TryRecvError};
use env_logger::Env;
use log::info;
use notify::event::{DataChange, ModifyKind};
use notify::{Event, RecursiveMode, Watcher};

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

    let (from_frontend_sender, from_frontend_receiver) = crossbeam_channel::unbounded();
    let (app_signal_out, app_signal_receiver) = crossbeam_channel::unbounded();

    let (system_out, app_system_receiver) = crossbeam_channel::unbounded();
    let audio_thread_control_signal = Arc::new(AtomicU8::new(AudioThreadControlSignal::CONTINUE));

    let (midi_in_sender, midi_in_receiver) = crossbeam_channel::bounded(100);
    let (midi_out_sender, midi_out_receiver) = crossbeam_channel::bounded(10);

    let send = midi_in_sender.clone();
    thread::spawn(move || {
        match midi::midi(send, midi_out_receiver.clone()) {
            Ok(_) => panic!("Unreachable."),
            Err(err) => {
                log::error!("MIDI thread crashed! {err:?}");
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
            )
        });
    }

    //
    // End audio.
    //

    let consumers: Arc<Mutex<HashMap<String, Sender<UnifiedMessage>>>> =
        Arc::new(Mutex::new(HashMap::new()));

    let consumers2 = consumers.clone();
    thread::spawn(move || loop {
        //
        // System messages.
        //
        let system_res = app_system_receiver.try_recv();
        match system_res {
            Ok(res) => {
                if let SystemMessage::Log(content) = &res {
                    log::info!("{content}")
                }
                let consumers = consumers2.lock().unwrap();
                for c in consumers.values() {
                    if c.send(UnifiedMessage::System(res.clone())).is_err() {
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
                    if c.send(UnifiedMessage::Signal(res.clone())).is_err() {
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
    // Filesystem firmware watcher.
    //
    const ENABLE_FS_WATCHER: bool = true;
    let (firmware_file_watcher_tx, firmware_file_watcher_rx) =
        mpsc::channel::<notify::Result<Event>>();

    let potential_watcher = match ENABLE_FS_WATCHER {
        true => {
            let tx = firmware_file_watcher_tx.clone();
            let mut watcher = notify::recommended_watcher(tx)?;

            // TODO: make path configurable via env or config file.
            watcher.watch(Path::new("./wasm/output.wasm"), RecursiveMode::NonRecursive)?;
            Some(watcher)
        }
        false => None,
    };

    let send = from_frontend_sender.clone();
    thread::spawn(move || {
        // Block forever, printing out events as they come in
        for res in firmware_file_watcher_rx {
            match res {
                Ok(event) => {
                    if let notify::EventKind::Modify(ModifyKind::Data(DataChange::Content)) =
                        event.kind
                    {
                        send.send(FromFrontend::Reload).unwrap();
                    }
                }
                Err(e) => println!("watch error: {e:?}"),
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
            // API endpoints
            .route("/api/ws", web::get().to(routes::ws_handler))
            .route("/api/ws/sink", web::get().to(routes::binary_ws_handler))
    })
    .bind(("0.0.0.0", cfg.port))
    .with_context(|| "could not start webserver")?;

    info!("Blaulicht is running on `http://localhost:{}`", cfg.port,);

    server
        .run()
        .await
        .with_context(|| "Could not start webserver")?;

    info!("Blaulicht is shutting down...");
    mem::drop(potential_watcher);

    let sig = audio_thread_control_signal.load(Ordering::Relaxed);
    let start_shutdown = Instant::now();
    if sig == AudioThreadControlSignal::CONTINUE {
        audio_thread_control_signal.store(AudioThreadControlSignal::ABORT, Ordering::Relaxed);
        loop {
            thread::sleep(Duration::from_secs(1));

            let sig = audio_thread_control_signal.load(Ordering::Relaxed);

            log::trace!("Waiting for audio thread to die: {sig}");

            if start_shutdown.elapsed() > Duration::from_secs(5) {
                log::warn!("Shutdown timeout");
                break;
            }

            if sig == AudioThreadControlSignal::ABORTED {
                break;
            }
        }
    }

    Ok(())
}
