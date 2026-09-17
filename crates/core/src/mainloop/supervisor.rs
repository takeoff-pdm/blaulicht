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
    let mut state = app_state.mainloop_state.write().unwrap();
    audio_thread_control_signal.store(signal.into(), Ordering::Relaxed);
    *state = signal;
}

/// A reload may finish after the supervisor has requested shutdown.
pub(super) fn complete_reload(signal: &AtomicU8, app_state: &AppState) {
    let mut state = app_state.mainloop_state.write().unwrap();
    if signal
        .compare_exchange(
            AudioThreadControlSignal::RELOAD.into(),
            AudioThreadControlSignal::CONTINUE.into(),
            Ordering::Relaxed,
            Ordering::Relaxed,
        )
        .is_ok()
    {
        *state = AudioThreadControlSignal::CONTINUE;
    }
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
    let mut device_changed = true;
    let mut audio_thread: Option<thread::JoinHandle<()>> = None;

    signal_mainloop(
        Arc::clone(&audio_thread_control_signal),
        Arc::clone(&app_state),
        AudioThreadControlSignal::ABORTED,
    );

    let mut seq = 0;

    let mut sent_no_device_available_log_message = false;

    let mut is_initial_device_changed = true;
    let mut auto_select_audio_device = true;

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
                auto_select_audio_device = false;
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

        if auto_select_audio_device && audio_device.is_none() {
            auto_select_audio_device = false;
            let devices = utils::get_input_devices_flat();
            let automatic_device = utils::default_input_device()
                .or_else(|| devices.first().map(|(_, device)| device.clone()));

            if system_out
                .send(SystemMessage::AudioDevicesView(devices))
                .is_err()
            {
                tracing::info!("[SUPERVISOR] System channel closed; shutting down.");
                break;
            }

            match automatic_device {
                Some(device) => {
                    let device_name = device.name().unwrap_or_else(|_| "unknown".to_owned());
                    tracing::info!("[audio] Automatically selected input device: {device_name}");
                    audio_device = Some(device);
                    device_changed = true;
                }
                None => {
                    tracing::error!("[audio] No input device is available");
                    let _ = system_out.send(SystemMessage::EngineInitializationComplete);
                }
            }
        }

        // Check if the thread crashed and attempt to restart it.
        if AudioThreadControlSignal::from(audio_thread_control_signal.load(Ordering::Relaxed))
            == AudioThreadControlSignal::CRASHED
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
        }
        if device_changed {
            let audio_input_device = audio_device.clone();

            if system_out
                .send(SystemMessage::AudioSelected(audio_input_device.clone()))
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

                // Set running before spawn, so a later ABORT cannot be overwritten.
                signal_mainloop(
                    Arc::clone(&audio_thread_control_signal),
                    Arc::clone(&app_state),
                    AudioThreadControlSignal::CONTINUE,
                );
                audio_thread = Some(thread::spawn(move || {
                    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        mainloop::run(
                            audio_input_device,
                            sys.clone(),
                            audio_thread_control_signal.clone(),
                            config,
                            bus_connection_plugins,
                            bus_connection_dmx,
                            Arc::clone(&app_state),
                        )
                    }))
                    .unwrap_or_else(|_| Err(anyhow::anyhow!("Engine thread panicked")));
                    if let Err(err) = result {
                        // `{err:#}` prints the full anyhow context chain.
                        tracing::error!("[audio] THREAD CRASH: {err:#}");
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
                    .as_ref()
                    .and_then(|device| device.name().ok())
                    .unwrap_or_else(|| "no audio input".to_string())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reload_completion_does_not_overwrite_shutdown() {
        let state = Arc::new(AppState::new(&[]));
        let signal = Arc::new(AtomicU8::new(AudioThreadControlSignal::RELOAD.into()));
        signal_mainloop(
            signal.clone(),
            state.clone(),
            AudioThreadControlSignal::ABORT,
        );
        complete_reload(&signal, &state);
        assert_eq!(
            AudioThreadControlSignal::from(signal.load(Ordering::Relaxed)),
            AudioThreadControlSignal::ABORT
        );
        assert_eq!(
            *state.mainloop_state.read().unwrap(),
            AudioThreadControlSignal::ABORT
        );
        signal_mainloop(
            signal.clone(),
            state.clone(),
            AudioThreadControlSignal::RELOAD,
        );
        complete_reload(&signal, &state);
        assert_eq!(
            AudioThreadControlSignal::from(signal.load(Ordering::Relaxed)),
            AudioThreadControlSignal::CONTINUE
        );
    }
}
