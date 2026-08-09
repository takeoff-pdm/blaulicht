use crate::{
    audio::defs::AudioThreadControlSignal,
    config::Config,
    event::SystemEventBusConnectionInst,
    mainloop::{self, bg_worker},
    msg::{AudioDeviceT, SystemMessage},
    state::AppState,
    syslog,
};
use crate::{msg::FromFrontend, utils};
use blaulicht_shared::LogLevel;
use crossbeam_channel::{Receiver, Sender, TryRecvError};
use std::{
    sync::{
        atomic::{AtomicU8, Ordering},
        Arc,
    },
    thread,
    time::Duration,
};

#[cfg(feature = "audio")]
use cpal::traits::DeviceTrait;

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
    system_out: Sender<SystemMessage>,
    config: Config,
    event_bus_connection_plugins: SystemEventBusConnectionInst,
    event_bus_connection_dmx: SystemEventBusConnectionInst,
    app_state: Arc<AppState>,
) {
    tracing::info!("[SUPERVISOR] Thread started!");

    // Start background worker.
    {
        let app_state_b = Arc::clone(&app_state);
        thread::spawn(move || {
            bg_worker::spawn_bg_worker(Arc::clone(&app_state_b));
        });
    }

    let heartbeat_delay = Duration::from_millis(1000);

    let mut audio_device: Option<AudioDeviceT> = None;
    let mut device_changed = false;
    let mut audio_thread: Option<thread::JoinHandle<()>> = None;

    signal_mainloop(
        Arc::clone(&audio_thread_control_signal),
        Arc::clone(&app_state),
        AudioThreadControlSignal::ABORTED,
    );

    let mut seq = 0;

    let mut sent_no_device_available_log_message = false;

    let mut is_initial_device_changed = true;

    loop {
        if system_out.send(SystemMessage::Heartbeat(seq)).is_err() {
            tracing::warn!("[SUPERVISOR] Shutting down...");

            signal_mainloop(
                Arc::clone(&audio_thread_control_signal),
                Arc::clone(&app_state),
                AudioThreadControlSignal::ABORTED,
            );

            break;
        };
        seq += 1;

        match from_frontend.try_recv() {
            Ok(FromFrontend::Reload) => {
                tracing::info!("[SUPERVISOR] Got reload request");

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
                audio_device = dev;
                device_changed = true;
            }
            Err(TryRecvError::Disconnected) => {
                tracing::warn!("[SUPERVISOR] Shutting down.");

                signal_mainloop(
                    Arc::clone(&audio_thread_control_signal),
                    Arc::clone(&app_state),
                    AudioThreadControlSignal::ABORTED,
                );

                if let Some(handle) = audio_thread.take() {
                    let _ = handle.join();
                }

                break;
            }
            Err(TryRecvError::Empty) => {}
        };

        // Check if the thread crashed and attempt to restart it.
        if AudioThreadControlSignal::from(audio_thread_control_signal.load(Ordering::Relaxed))
            == AudioThreadControlSignal::CRASHED
            && audio_device.is_some()
        {
            if !is_initial_device_changed {
                thread::sleep(Duration::from_secs(2));
            }
            is_initial_device_changed = false;
            device_changed = true;
        }

        // Syncronize to config file.
        if device_changed {
            let mut audio = app_state.audio.write().unwrap();
            audio.device_name = audio_device.as_ref().and_then(|dev| dev.name().ok());
        }

        if audio_device.is_none() {
            let devices = utils::get_input_devices_flat();

            // TODO: add an aggregate log macro which logs to the channel and the console.

            if system_out
                .send(SystemMessage::AudioDevicesView(devices))
                .is_err()
            {
                tracing::info!("[SUPERVISOR] System channel closed; shutting down.");
                signal_mainloop(
                    Arc::clone(&audio_thread_control_signal),
                    Arc::clone(&app_state),
                    AudioThreadControlSignal::ABORTED,
                );
                break;
            }

            if !sent_no_device_available_log_message {
                syslog!(
                    system_out,
                    "[audio] No audio device selected, waiting for selection..."
                );

                sent_no_device_available_log_message = true;
            }

            device_changed = false;

            if AudioThreadControlSignal::from(audio_thread_control_signal.load(Ordering::Relaxed))
                == AudioThreadControlSignal::CONTINUE
            {
                signal_mainloop(
                    Arc::clone(&audio_thread_control_signal),
                    Arc::clone(&app_state),
                    AudioThreadControlSignal::ABORT,
                );
            }
            if let Some(handle) = audio_thread.take() {
                if handle.join().is_err() {
                    tracing::warn!("[SUPERVISOR] Audio worker did not exit cleanly");
                }
            }
        } else if device_changed {
            // TODO: just broadcast a state-change message.
            let Some(audio_input_device) = audio_device.clone() else {
                device_changed = false;
                continue;
            };

            if system_out
                .send(SystemMessage::AudioSelected(Some(
                    audio_input_device.clone(),
                )))
                .is_err()
            {
                tracing::info!("[SUPERVISOR] System channel closed; shutting down.");
                signal_mainloop(
                    Arc::clone(&audio_thread_control_signal),
                    Arc::clone(&app_state),
                    AudioThreadControlSignal::ABORTED,
                );
                break;
            }

            let sys = system_out.clone();
            if let Some(handle) = audio_thread.take() {
                signal_mainloop(
                    Arc::clone(&audio_thread_control_signal),
                    Arc::clone(&app_state),
                    AudioThreadControlSignal::ABORT,
                );
                if handle.join().is_err() {
                    tracing::warn!("[SUPERVISOR] Previous audio worker did not exit cleanly");
                }
            }
            {
                let audio_input_device = audio_input_device.clone();
                let audio_thread_control_signal = audio_thread_control_signal.clone();

                let sys = sys.clone();
                let config = config.clone();
                let bus_connection_plugins = event_bus_connection_plugins.clone();
                let bus_connection_dmx = event_bus_connection_dmx.clone();

                let app_state = Arc::clone(&app_state);

                audio_thread = Some(thread::spawn(move || {
                    signal_mainloop(
                        Arc::clone(&audio_thread_control_signal),
                        Arc::clone(&app_state),
                        AudioThreadControlSignal::CONTINUE,
                    );

                    if let Err(err) = mainloop::run(
                        audio_input_device,
                        sys.clone(),
                        audio_thread_control_signal.clone(),
                        config,
                        bus_connection_plugins,
                        bus_connection_dmx,
                        Arc::clone(&app_state),
                    ) {
                        tracing::error!("[audio] THREAD CRASH: {err}");
                        syslog!(sys, format!("[audio] {err}"), LogLevel::Err);

                        signal_mainloop(
                            Arc::clone(&audio_thread_control_signal),
                            Arc::clone(&app_state),
                            AudioThreadControlSignal::CRASHED,
                        );
                    }

                    let _ = sys.send(SystemMessage::Log(
                        "[audio] Thread died.".into(),
                        LogLevel::Warn,
                    ));
                }));
            }

            device_changed = false;
            tracing::info!(
                "[AUDIO] Main thread started: <{}>",
                audio_input_device
                    .name()
                    .unwrap_or_else(|_| "unknown".to_string())
            );

            syslog!(
                sys,
                format!(
                    "[audio] Using device \"{}\"",
                    app_state
                        .audio
                        .read()
                        .unwrap()
                        .device_name
                        .clone()
                        .unwrap_or_else(|| "None".to_string())
                )
            );
        }

        thread::sleep(heartbeat_delay);
    }
}
