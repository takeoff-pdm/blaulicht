use crate::{
    audio::defs::AudioThreadControlSignal,
    config::{Config, SharedConfig},
    event::SystemEventBusConnectionInst,
    hub::SystemHub,
    mainloop::{self, bg_worker},
    msg::{AudioInput, DummyInput, StageStatus, StartupStage, SystemMessage},
    state::AppState,
};
use crate::{log::target, utils};
use crossbeam_channel::{Receiver, RecvTimeoutError, Sender, TryRecvError};
use std::{
    sync::{
        atomic::{AtomicU8, Ordering},
        Arc,
    },
    thread,
    time::Duration,
};
use web_time::Instant;

/// What the supervisor thread does on request. The engine forwards the
/// matching `HostCommand`s here (`DmxEngine::apply_host` runs on the main
/// loop thread, which the supervisor owns and restarts); the plugin watcher
/// sends `ReloadPlugins` too. The supervisor wakes up for a request at once
/// instead of waiting for its next heartbeat.
pub enum SupervisorRequest {
    /// Reload every plugin (and MIDI / UDP), if the engine is running.
    ReloadPlugins,
    /// Restart the main loop on `input` (`None`: [`DummyInput::Silence`])
    /// and remember it as the config's `default_audio_device`.
    SelectAudioInput(Option<AudioInput>),
}

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

fn send_stage(hub: &SystemHub, stage: StartupStage, status: StageStatus) {
    hub.publish(SystemMessage::StartupStage { stage, status });
}

/// Runs the engine: starts the main loop on the selected audio input,
/// restarts it after a crash or an input change, publishes heartbeats and the
/// audio device list, and serves [`SupervisorRequest`]s (`requests`;
/// `request_sender` is handed to each main loop's `DmxEngine`).
/// `initial_input` is the configured input, if it is available.
// The engine's parts, set up once in `main`.
#[allow(clippy::too_many_arguments)]
pub fn supervisor_thread(
    requests: Receiver<SupervisorRequest>,
    request_sender: Sender<SupervisorRequest>,
    initial_input: Option<AudioInput>,
    audio_thread_control_signal: Arc<AtomicU8>,
    hub: SystemHub,
    config: Config,
    shared_config: SharedConfig,
    event_bus_connection_plugins: SystemEventBusConnectionInst,
    event_bus_connection_dmx: SystemEventBusConnectionInst,
    app_state: Arc<AppState>,
) {
    tracing::info!("Supervisor started");

    // Start background worker.
    {
        let app_state_b = Arc::clone(&app_state);
        thread::spawn(move || {
            bg_worker::spawn_bg_worker(Arc::clone(&app_state_b));
        });
    }

    let heartbeat_delay = Duration::from_millis(1000);

    let mut auto_select_audio_device = initial_input.is_none();
    let mut audio_device: Option<AudioInput> = initial_input;
    let mut device_changed = true;
    let mut audio_thread: Option<thread::JoinHandle<()>> = None;

    signal_mainloop(
        Arc::clone(&audio_thread_control_signal),
        Arc::clone(&app_state),
        AudioThreadControlSignal::ABORTED,
    );

    let mut seq = 0;

    let mut is_initial_device_changed = true;
    let mut restarting_after_crash = false;
    let mut next_heartbeat = Instant::now();
    // A request that woke the supervisor up early.
    let mut woken_by: Option<SupervisorRequest> = None;

    loop {
        // An explicit shutdown request: with a hub, "nobody listens" is no
        // longer observable (and a headless engine has no listener anyway).
        if hub.is_shut_down() {
            tracing::warn!("Supervisor shutting down");

            signal_mainloop(
                Arc::clone(&audio_thread_control_signal),
                Arc::clone(&app_state),
                AudioThreadControlSignal::ABORTED,
            );

            break;
        }
        let now = Instant::now();
        if now >= next_heartbeat {
            hub.publish(SystemMessage::Heartbeat(seq));
            seq += 1;
            next_heartbeat = now + heartbeat_delay;
        }

        let request = match woken_by.take() {
            Some(request) => Ok(request),
            None => requests.try_recv(),
        };
        match request {
            Ok(SupervisorRequest::ReloadPlugins) => {
                tracing::info!("Supervisor got reload request");

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
            Ok(SupervisorRequest::SelectAudioInput(input)) => {
                let name = input.as_ref().map(AudioInput::name);
                if let Err(err) = shared_config.update(|config| config.default_audio_device = name)
                {
                    tracing::error!("Failed to persist audio device selection: {err}");
                }
                audio_device = Some(input.unwrap_or(AudioInput::Dummy(DummyInput::Silence)));
                device_changed = true;
                auto_select_audio_device = false;
            }
            Err(TryRecvError::Disconnected) => {
                tracing::warn!("Supervisor shutting down");

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
            send_stage(
                &hub,
                StartupStage::Audio,
                StageStatus::running("selecting device"),
            );
            let devices = utils::get_input_devices_flat();
            let automatic_device = utils::default_input_device()
                .or_else(|| devices.first().map(|(_, device)| device.clone()));

            hub.publish(SystemMessage::AudioDevicesView(utils::input_device_names(
                &devices,
            )));

            match automatic_device {
                Some(device) => {
                    let device = AudioInput::Device(device);
                    tracing::info!(target: target::AUDIO, "Automatically selected input device: {}", device.name());
                    audio_device = Some(device);
                }
                None => {
                    tracing::warn!(
                        target: target::AUDIO,
                        "No input device is available; using {}",
                        DummyInput::Silence.name()
                    );
                    audio_device = Some(AudioInput::Dummy(DummyInput::Silence));
                }
            }
            device_changed = true;
        }

        // Check if the thread crashed and attempt to restart it.
        if AudioThreadControlSignal::from(audio_thread_control_signal.load(Ordering::Relaxed))
            == AudioThreadControlSignal::CRASHED
        {
            restarting_after_crash = true;
            send_stage(
                &hub,
                StartupStage::MainLoop,
                StageStatus::restarting("restarting after crash"),
            );
            for stage in [
                StartupStage::Audio,
                StartupStage::Plugins,
                StartupStage::DmxEngine,
            ] {
                send_stage(&hub, stage, StageStatus::PENDING);
            }
            if !is_initial_device_changed {
                thread::sleep(Duration::from_secs(2));
            }
            is_initial_device_changed = false;
            device_changed = true;
        }

        // Keep the device list fresh so hot-plugged hardware can be picked
        // (a real device is busy, so the list is only refreshed when it
        // changes).
        if device_changed || matches!(audio_device, None | Some(AudioInput::Dummy(_))) {
            let devices = utils::get_input_devices_flat();
            hub.publish(SystemMessage::AudioDevicesView(utils::input_device_names(
                &devices,
            )));
        }
        if device_changed {
            let audio_input_device = audio_device
                .clone()
                .unwrap_or(AudioInput::Dummy(DummyInput::Silence));

            hub.publish(SystemMessage::AudioSelected(audio_input_device.name()));

            let sys = hub.clone();
            if let Some(handle) = audio_thread.take() {
                signal_mainloop(
                    Arc::clone(&audio_thread_control_signal),
                    Arc::clone(&app_state),
                    AudioThreadControlSignal::ABORT,
                );
                if handle.join().is_err() {
                    tracing::warn!("Previous engine thread did not exit cleanly");
                }
            }
            {
                let audio_input_device = audio_input_device.clone();
                let audio_thread_control_signal = audio_thread_control_signal.clone();

                let sys = sys.clone();
                let config = config.clone();
                let bus_connection_plugins = event_bus_connection_plugins.clone();
                let bus_connection_dmx = event_bus_connection_dmx.clone();
                let request_sender = request_sender.clone();

                let app_state = Arc::clone(&app_state);

                // After a crash the stage already shows "restarting".
                if !std::mem::take(&mut restarting_after_crash) {
                    send_stage(
                        &sys,
                        StartupStage::MainLoop,
                        StageStatus::running("starting engine"),
                    );
                }

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
                            request_sender,
                        )
                    }))
                    .unwrap_or_else(|_| Err(anyhow::anyhow!("Engine thread panicked")));
                    if let Err(err) = result {
                        // `{err:#}` prints the full anyhow context chain.
                        tracing::error!("Engine thread crashed: {err:#}");
                        send_stage(
                            &sys,
                            StartupStage::MainLoop,
                            StageStatus::failed(format!("crashed: {err:#}")),
                        );

                        signal_mainloop(
                            Arc::clone(&audio_thread_control_signal),
                            Arc::clone(&app_state),
                            AudioThreadControlSignal::CRASHED,
                        );
                    } else {
                        tracing::info!("Engine thread stopped");
                    }
                }));
            }

            device_changed = false;
            tracing::info!(
                target: target::AUDIO,
                "Engine started with audio input \"{}\"",
                audio_input_device.name()
            );
        }

        // Sleep until the next heartbeat, unless a request arrives first.
        // (A timeout, not `recv_deadline`: that takes a std `Instant`.)
        match requests.recv_timeout(next_heartbeat.saturating_duration_since(Instant::now())) {
            Ok(request) => woken_by = Some(request),
            // A closed channel is handled by `try_recv` above.
            Err(RecvTimeoutError::Timeout | RecvTimeoutError::Disconnected) => {}
        }
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
