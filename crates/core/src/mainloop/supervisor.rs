use crate::{
    audio::defs::AudioThreadControlSignal,
    config::Config,
    event::SystemEventBusConnectionInst,
    mainloop::{self, bg_worker},
    msg::{SystemMessage},
    state::AppState,
};
use crate::{msg::FromFrontend, utils};
use blaulicht_audio_engine::Signal;
use blaulicht_shared::LogLevel;
use cpal::{traits::DeviceTrait, Device};
use crossbeam_channel::{Receiver, Sender, TryRecvError};
use log::{error, info, warn};
use std::{
    sync::{
        atomic::{AtomicU8, Ordering},
        Arc,
    },
    thread,
    time::Duration,
};

pub fn signal_mainloop(
    audio_thread_control_signal: Arc<AtomicU8>,
    app_state: Arc<AppState>,
    signal: AudioThreadControlSignal,
) {
    audio_thread_control_signal.store(signal.into(), Ordering::Relaxed);
    *app_state.mainloop_state.write().unwrap() = signal;
}

pub fn supervisor_thread(
    from_frontend: Receiver<FromFrontend>,
    audio_thread_control_signal: Arc<AtomicU8>,
    signal_out_0: Sender<Signal>,
    system_out: Sender<SystemMessage>,
    config: Config,
    event_bus_connection_plugins: SystemEventBusConnectionInst,
    event_bus_connection_dmx: SystemEventBusConnectionInst,
    app_state: Arc<AppState>,
) {
    log::info!("[SUPERVISOR] Thread started!");

    // Start background worker.
    {
        let app_state_b = Arc::clone(&app_state);
        thread::spawn(move || {
            bg_worker::spawn_bg_worker(Arc::clone(&app_state_b));
        });
    }

    let heartbeat_delay = Duration::from_millis(1000);

    let mut audio_device: Option<Device> = None;
    let mut device_changed = false;

    // TODO: put the DMX thread under main!

    signal_mainloop(
        Arc::clone(&audio_thread_control_signal),
        Arc::clone(&app_state),
        AudioThreadControlSignal::ABORTED,
    );

    let mut seq = 0;

    let mut sent_no_device_available_log_message = false;

    loop {
        thread::sleep(heartbeat_delay);

        if system_out.send(SystemMessage::Heartbeat(seq)).is_err() {
            warn!("[SUPERVISOR] Shutting down...");
            break;
        };
        seq += 1;

        match from_frontend.try_recv() {
            Ok(FromFrontend::Reload) => {
                info!("[SUPERVISOR] Got reload request");
                if AudioThreadControlSignal::from(
                    audio_thread_control_signal.load(Ordering::Relaxed),
                ) == AudioThreadControlSignal::CONTINUE
                {
                    signal_mainloop(
                        Arc::clone(&audio_thread_control_signal),
                        Arc::clone(&app_state),
                        AudioThreadControlSignal::RELOAD,
                    );
                }
            }
            Ok(FromFrontend::SelectInputDevice(dev)) => {
                // Get device by name.
                audio_device = dev;
                device_changed = true;
            }
            Err(TryRecvError::Disconnected) => {
                log::warn!("[SUPERVISOR] Shutting down.");
                break;
            }
            Err(TryRecvError::Empty) => {}
        };

        // Check if the thread crashed and attempt to restart it.
        if AudioThreadControlSignal::from(audio_thread_control_signal.load(Ordering::Relaxed))
            == AudioThreadControlSignal::CRASHED.into()
            && audio_device.is_some()
        {
            thread::sleep(Duration::from_secs(2));
            device_changed = true;
        }

        if device_changed {
            // Update state.
            {
                let mut audio = app_state.audio.write().unwrap();
                audio.device_name = match audio_device {
                    Some(ref dev) => Some(dev.name().unwrap().to_string()),
                    None => None,
                };
            }
        }

        if audio_device.is_none() {
            let devices = utils::get_input_devices_flat();

            // TODO: add an aggregate log macro which logs to the channel and the console.

            system_out
                .send(SystemMessage::AudioDevicesView(devices))
                .unwrap();

            if !sent_no_device_available_log_message {
                system_out
                    .send(SystemMessage::Log(
                        "[audio] No audio device selected, waiting for selection...".to_string(),
                        LogLevel::Debug,
                    ))
                    .unwrap();
                sent_no_device_available_log_message = true;
            }

            device_changed = false;

            if AudioThreadControlSignal::from(audio_thread_control_signal.load(Ordering::Relaxed))
                == AudioThreadControlSignal::CONTINUE.into()
            {
                signal_mainloop(
                    Arc::clone(&audio_thread_control_signal),
                    Arc::clone(&app_state),
                    AudioThreadControlSignal::ABORT,
                );
            }
        } else if device_changed {
            // TODO: just broadcast a state-change message.
            system_out
                .send(SystemMessage::AudioSelected(audio_device.clone()))
                .unwrap();

            let (sig_0, sys) = (signal_out_0.clone(), system_out.clone());
            {
                let audio_input_device = audio_device.clone().unwrap();
                let audio_thread_control_signal = audio_thread_control_signal.clone();

                let sys = sys.clone();
                let config = config.clone();
                let bus_connection_plugins = event_bus_connection_plugins.clone();
                let bus_connection_dmx = event_bus_connection_dmx.clone();

                let app_state = Arc::clone(&app_state);

                thread::spawn(move || {
                    signal_mainloop(
                        Arc::clone(&audio_thread_control_signal),
                        Arc::clone(&app_state),
                        AudioThreadControlSignal::CONTINUE,
                    );

                    if let Err(err) = mainloop::run(
                        audio_input_device,
                        sig_0,
                        sys.clone(),
                        audio_thread_control_signal.clone(),
                        config,
                        bus_connection_plugins,
                        bus_connection_dmx,
                        Arc::clone(&app_state),
                    ) {
                        // TODO: handle the audio backend error.
                        error!("[audio] THREAD CRASH: {err}");
                        sys.send(SystemMessage::Log(format!("[audio] {err}"), LogLevel::Err))
                            .unwrap();

                        signal_mainloop(
                            Arc::clone(&audio_thread_control_signal),
                            Arc::clone(&app_state),
                            AudioThreadControlSignal::CRASHED,
                        );
                    }

                    sys.send(SystemMessage::Log(
                        "[audio] Thread died.".into(),
                        LogLevel::Warn,
                    ))
                    .unwrap();
                });
            }

            device_changed = false;
            log::info!(
                "[AUDIO] Main thread started: <{}>",
                audio_device.clone().unwrap().name().unwrap()
            );

            sys.send(SystemMessage::Log(
                "[audio] Thread started.".to_string(),
                LogLevel::Info,
            ))
            .unwrap();
        }
    }
}

// fn init_dmx(
//     midi_out_sender: Sender<MidiEvent>,
//     system_out: Sender<SystemMessage>,
// ) -> anyhow::Result<DmxUniverse> {
//     debug!("[DMX] Trying to establish hardware link...");
//     let res = DmxUniverse::new(midi_out_sender.clone(), system_out.clone());
//     let dmx_universe = match res {
//         Ok(universe) => universe,
//         Err(e) => {
//             info!("[DMX] Failed to establish hardware link: {e}, using dummy...");
//             let Ok(universe) = DmxUniverse::new_dummy(midi_out_sender, system_out.clone()) else {
//                 bail!("[DMX] Failed to create dummy universe, exiting.");
//             };

//             universe
//         }
//     };

//     Ok(dmx_universe)
// }
