use std::sync::atomic::AtomicU8;
use std::sync::Arc;
use std::thread;
use std::time::Instant;

use actix_files::Files;
use actix_web::web::{self, Data};
use actix_web::{App, HttpServer};
use anyhow::{anyhow, Context};
use blaulicht::audio::AudioThreadControlSignal;
use blaulicht::routes::AppState;
use blaulicht::wasm::TickInput;
use blaulicht::{config, dmx, midi, routes, wasm};
use env_logger::Env;
use log::info;

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

    {
        // Audio recording and analysis thread.
        let system_out = system_out.clone();
        let audio_thread_control_signal = audio_thread_control_signal.clone();
        thread::spawn(|| {
            dmx::audio_thread(
                from_frontend_receiver,
                audio_thread_control_signal,
                app_signal_out,
                system_out,
                // to_frontend_sender,
            )
        });
    }

    //
    // End audio.
    //

    let data = Data::new(AppState {
        from_frontend_sender,
        app_signal_receiver,
        app_system_receiver,
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

    Ok(())
}
