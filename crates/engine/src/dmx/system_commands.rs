//! The engine side of the Audio and System pages' host commands (see
//! [`HostCommand`]): the audio input, audio parameters, plugin reload and
//! Art-Net receivers. Each runs in `DmxEngine::apply_host` on the main loop
//! thread.
//!
//! The audio input and the plugin reload are carried out by the supervisor
//! thread, which owns the main loop (an input change restarts it, so it
//! can't happen on the main loop thread itself). The engine checks the
//! command, answers it, and forwards a [`SupervisorRequest`]; the supervisor
//! wakes up for it at once. The supervisor also writes the config, because
//! the engine holds the show state's write lock here and the desk frontend
//! takes the config lock before that lock (until 052).
//!
//! [`HostCommand`]: crate::hub::HostCommand

use std::{net::SocketAddr, sync::PoisonError};

use super::DmxEngine;
use crate::{
    hub::{AudioParam, CommandOutcome},
    mainloop::SupervisorRequest,
    state::ArtNetReceiver,
    utils,
};

const OK: CommandOutcome = CommandOutcome::Ok { created: None };

fn refused(reason: impl Into<String>) -> CommandOutcome {
    CommandOutcome::Refused(reason.into())
}

const NO_SUPERVISOR: &str = "The engine supervisor is not running";
const RECEIVER_NOT_FOUND: &str = "Art-Net receiver not found";

impl DmxEngine {
    fn send_to_supervisor(&self, request: SupervisorRequest) -> CommandOutcome {
        match &self.supervisor {
            Some(supervisor) if supervisor.send(request).is_ok() => OK,
            _ => refused(NO_SUPERVISOR),
        }
    }

    /// `HostCommand::SelectAudioInput`. Resolving a device name enumerates
    /// the audio hosts; on success the main loop is restarted right after,
    /// so the pause doesn't matter.
    pub(super) fn select_audio_input_command(&self, name: Option<String>) -> CommandOutcome {
        let input = match name {
            None => None,
            Some(name) => match utils::input_from_name(name) {
                Some(input) => Some(input),
                None => return refused("Audio input not found"),
            },
        };
        self.send_to_supervisor(SupervisorRequest::SelectAudioInput(input))
    }

    /// `HostCommand::ReloadPlugins`.
    pub(super) fn reload_plugins_command(&self) -> CommandOutcome {
        self.send_to_supervisor(SupervisorRequest::ReloadPlugins)
    }

    /// `HostCommand::SetAudioParam`: writes the parameter and sets `changed`,
    /// which the main loop's spectrogram tick hands to the collector.
    pub(super) fn set_audio_param_command(&self, param: AudioParam) -> CommandOutcome {
        if let AudioParam::ManualBpm(Some(bpm)) = param {
            if !bpm.is_finite() || bpm <= 0.0 {
                return refused("BPM must be positive");
            }
        }
        let mut params = self
            .state_ref
            .audio_params
            .write()
            .unwrap_or_else(PoisonError::into_inner);
        match param {
            AudioParam::Volume(volume) => params.volume = volume,
            AudioParam::Gate(gate) => params.gate = gate,
            AudioParam::Boost(boost) => params.boost = boost.filter(|boost| *boost > 0),
            AudioParam::AutoCalibrate(on) => params.auto_calibrate = on,
            AudioParam::ManualBpm(bpm) => params.manual_bpm = bpm,
        }
        params.changed = true;
        OK
    }

    //
    // Art-Net receivers. The show state's lock is held here, so this takes
    // `artnet_output` after it, as showfile loading does.
    //

    fn with_artnet_receivers(
        &self,
        f: impl FnOnce(&mut Vec<ArtNetReceiver>) -> CommandOutcome,
    ) -> CommandOutcome {
        let mut output = self
            .state_ref
            .artnet_output
            .write()
            .unwrap_or_else(PoisonError::into_inner);
        f(&mut output.receivers)
    }

    /// `HostCommand::AddArtNetReceiver`.
    pub(super) fn add_artnet_receiver_command(&self, address: SocketAddr) -> CommandOutcome {
        if !address.is_ipv4() {
            return refused("Art-Net receiver must be IPv4");
        }
        self.with_artnet_receivers(|receivers| {
            if receivers.iter().any(|existing| existing.address == address) {
                return refused(format!("Art-Net receiver {address} already exists"));
            }
            receivers.push(ArtNetReceiver::new(address));
            tracing::info!(target: crate::log::target::DMX, "Added Art-Net receiver {address}");
            OK
        })
    }

    /// `HostCommand::RemoveArtNetReceiver`.
    pub(super) fn remove_artnet_receiver_command(&self, address: SocketAddr) -> CommandOutcome {
        self.with_artnet_receivers(|receivers| {
            let Some(index) = receivers
                .iter()
                .position(|entry| entry.address == address && entry.owner_plugin_id.is_none())
            else {
                return refused(RECEIVER_NOT_FOUND);
            };
            receivers.remove(index);
            tracing::info!(target: crate::log::target::DMX, "Removed Art-Net receiver {address}");
            OK
        })
    }

    /// `HostCommand::SetArtNetReceiverEnabled`.
    pub(super) fn set_artnet_receiver_enabled_command(
        &self,
        address: SocketAddr,
        owner: Option<u8>,
        enabled: bool,
    ) -> CommandOutcome {
        self.with_artnet_receivers(|receivers| {
            match receivers
                .iter_mut()
                .find(|entry| entry.address == address && entry.owner_plugin_id == owner)
            {
                Some(entry) => {
                    entry.enabled = enabled;
                    OK
                }
                None => refused(RECEIVER_NOT_FOUND),
            }
        })
    }

    /// `HostCommand::SetArtNetReceiverUniverses`.
    pub(super) fn set_artnet_receiver_universes_command(
        &self,
        address: SocketAddr,
        first: u8,
        last: u8,
    ) -> CommandOutcome {
        self.with_artnet_receivers(|receivers| {
            match receivers
                .iter_mut()
                .find(|entry| entry.address == address && entry.owner_plugin_id.is_none())
            {
                Some(entry) => {
                    entry.set_universe_range(first, last);
                    OK
                }
                None => refused(RECEIVER_NOT_FOUND),
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use std::net::SocketAddr;

    use crate::{
        client::{
            testing::{small_rig, TestEngine},
            EngineClient,
        },
        hub::{AudioParam, CommandOutcome, HostCommand},
        mainloop::SupervisorRequest,
        msg::{AudioInput, DummyInput},
        state::ArtNetReceiver,
    };

    const OK: CommandOutcome = CommandOutcome::Ok { created: None };

    fn refused(reason: &str) -> CommandOutcome {
        CommandOutcome::Refused(reason.to_string())
    }

    fn addr(text: &str) -> SocketAddr {
        text.parse().unwrap()
    }

    /// `(address, enabled, first, last, owner)` of every receiver.
    fn receivers(client: &impl EngineClient) -> Vec<(SocketAddr, bool, u8, u8, Option<u8>)> {
        client.with_artnet_receivers(|receivers| {
            receivers
                .iter()
                .map(|r| {
                    (
                        r.address,
                        r.enabled,
                        r.first_universe,
                        r.last_universe,
                        r.owner_plugin_id,
                    )
                })
                .collect()
        })
    }

    fn selected(request: SupervisorRequest) -> Option<String> {
        match request {
            SupervisorRequest::SelectAudioInput(input) => input.as_ref().map(AudioInput::name),
            SupervisorRequest::ReloadPlugins => panic!("expected an input selection"),
        }
    }

    #[test]
    fn an_audio_input_is_selected_by_name_through_the_supervisor() {
        let mut engine = TestEngine::start(small_rig);
        let desk = engine.connect_desk();
        let remote = engine.connect_remote();

        // Without a supervisor (a headless test engine) nothing can switch.
        assert_eq!(
            engine.round_trip(
                &desk,
                HostCommand::SelectAudioInput {
                    name: Some(DummyInput::Noise.name().to_string())
                }
            ),
            refused("The engine supervisor is not running")
        );

        let supervisor = engine.connect_supervisor();
        for (client, name) in [
            (&desk, Some(DummyInput::Noise.name())),
            (&remote, Some(DummyInput::Silence.name())),
            (&desk, None),
        ] {
            let name = name.map(str::to_string);
            assert_eq!(
                engine.round_trip(client, HostCommand::SelectAudioInput { name: name.clone() }),
                OK
            );
            assert_eq!(selected(supervisor.try_recv().unwrap()), name);
        }

        assert_eq!(engine.round_trip(&remote, HostCommand::ReloadPlugins), OK);
        assert!(matches!(
            supervisor.try_recv(),
            Ok(SupervisorRequest::ReloadPlugins)
        ));
        assert!(supervisor.try_recv().is_err());
    }

    /// Without the `audio` feature every name resolves to the mock device.
    #[cfg(feature = "audio")]
    #[test]
    fn an_unknown_audio_input_is_refused() {
        let mut engine = TestEngine::start(small_rig);
        let desk = engine.connect_desk();
        let supervisor = engine.connect_supervisor();
        assert_eq!(
            engine.round_trip(
                &desk,
                HostCommand::SelectAudioInput {
                    name: Some("No such device 0x1f".to_string())
                }
            ),
            refused("Audio input not found")
        );
        assert!(supervisor.try_recv().is_err());
    }

    #[test]
    fn audio_params_are_set_one_at_a_time_and_flagged_for_the_collector() {
        let mut engine = TestEngine::start(small_rig);
        let desk = engine.connect_desk();
        let remote = engine.connect_remote();

        for (client, param) in [
            (&desk, AudioParam::Volume(150)),
            (&remote, AudioParam::Gate(30)),
            (&desk, AudioParam::Boost(Some(40))),
            (&remote, AudioParam::AutoCalibrate(false)),
            (&desk, AudioParam::ManualBpm(Some(128.0))),
        ] {
            assert_eq!(
                engine.round_trip(client, HostCommand::SetAudioParam(param)),
                OK
            );
        }
        let params = desk.with_audio_params(|params| *params);
        assert_eq!(params.volume, 150);
        assert_eq!(params.gate, 30);
        assert_eq!(params.boost, Some(40));
        assert!(!params.auto_calibrate);
        assert_eq!(params.manual_bpm, Some(128.0));
        assert!(params.changed, "the main loop hands changed params on");

        // The main loop takes them.
        engine.app().audio_params.write().unwrap().changed = false;
        assert_eq!(
            engine.round_trip(
                &desk,
                HostCommand::SetAudioParam(AudioParam::Boost(Some(0)))
            ),
            OK
        );
        assert_eq!(
            engine.round_trip(
                &desk,
                HostCommand::SetAudioParam(AudioParam::ManualBpm(None))
            ),
            OK
        );
        let params = desk.with_audio_params(|params| *params);
        assert_eq!((params.boost, params.manual_bpm), (None, None));
        assert!(params.changed);
        assert_eq!(params.volume, 150, "other params stay");

        for bpm in [0.0, -1.0, f32::NAN, f32::INFINITY] {
            assert_eq!(
                engine.round_trip(
                    &desk,
                    HostCommand::SetAudioParam(AudioParam::ManualBpm(Some(bpm)))
                ),
                refused("BPM must be positive")
            );
        }
        assert_eq!(desk.with_audio_params(|params| params.manual_bpm), None);
    }

    #[test]
    fn user_artnet_receivers_are_added_edited_and_removed() {
        let mut engine = TestEngine::start(small_rig);
        let desk = engine.connect_desk();
        let remote = engine.connect_remote();
        let a = addr("192.0.2.10:6454");
        let b = addr("192.0.2.11:6454");

        assert_eq!(
            engine.round_trip(&desk, HostCommand::AddArtNetReceiver(a)),
            OK
        );
        assert_eq!(
            engine.round_trip(&remote, HostCommand::AddArtNetReceiver(b)),
            OK
        );
        assert_eq!(
            engine.round_trip(&remote, HostCommand::AddArtNetReceiver(a)),
            refused("Art-Net receiver 192.0.2.10:6454 already exists")
        );
        assert_eq!(
            engine.round_trip(
                &desk,
                HostCommand::AddArtNetReceiver(addr("[2001:db8::1]:6454"))
            ),
            refused("Art-Net receiver must be IPv4")
        );
        assert_eq!(
            receivers(&desk),
            [(a, true, 0, 9, None), (b, true, 0, 9, None)]
        );

        assert_eq!(
            engine.round_trip(
                &remote,
                HostCommand::SetArtNetReceiverEnabled {
                    address: a,
                    owner: None,
                    enabled: false
                }
            ),
            OK
        );
        // Inverted and out-of-range bounds are fixed, as in the UI before.
        assert_eq!(
            engine.round_trip(
                &desk,
                HostCommand::SetArtNetReceiverUniverses {
                    address: b,
                    first: 200,
                    last: 3
                }
            ),
            OK
        );
        assert_eq!(
            receivers(&desk),
            [(a, false, 0, 9, None), (b, true, 3, 9, None)]
        );

        assert_eq!(
            engine.round_trip(&desk, HostCommand::RemoveArtNetReceiver(a)),
            OK
        );
        assert_eq!(
            engine.round_trip(&desk, HostCommand::RemoveArtNetReceiver(a)),
            refused("Art-Net receiver not found")
        );
        assert_eq!(receivers(&remote), [(b, true, 3, 9, None)]);
    }

    #[test]
    fn a_plugin_receiver_can_only_be_turned_on_and_off() {
        let mut engine = TestEngine::start(small_rig);
        let desk = engine.connect_desk();
        let address = addr("192.0.2.20:6454");
        let handle = engine
            .app()
            .artnet_output
            .write()
            .unwrap()
            .register_plugin_receiver(4, address);
        assert_ne!(handle, 0);

        assert_eq!(
            engine.round_trip(
                &desk,
                HostCommand::SetArtNetReceiverEnabled {
                    address,
                    owner: Some(4),
                    enabled: false
                }
            ),
            OK
        );
        let not_found = refused("Art-Net receiver not found");
        for command in [
            // The wrong owner.
            HostCommand::SetArtNetReceiverEnabled {
                address,
                owner: None,
                enabled: true,
            },
            HostCommand::SetArtNetReceiverUniverses {
                address,
                first: 1,
                last: 2,
            },
            HostCommand::RemoveArtNetReceiver(address),
        ] {
            assert_eq!(engine.round_trip(&desk, command), not_found);
        }
        assert_eq!(
            receivers(&desk),
            [(address, false, 0, ArtNetReceiver::MAX_UNIVERSE, Some(4))]
        );
    }

    #[test]
    fn only_user_receiver_edits_mark_the_show() {
        let address = addr("192.0.2.30:6454");
        let enabled = |owner| HostCommand::SetArtNetReceiverEnabled {
            address,
            owner,
            enabled: true,
        };
        assert!(enabled(None).edits_show());
        assert!(!enabled(Some(1)).edits_show());
        assert!(HostCommand::AddArtNetReceiver(address).edits_show());
        assert!(HostCommand::RemoveArtNetReceiver(address).edits_show());
        assert!(!HostCommand::SetAudioParam(AudioParam::Gate(1)).edits_show());
        assert!(!HostCommand::SelectAudioInput { name: None }.edits_show());
        assert!(!HostCommand::ReloadPlugins.edits_show());
    }
}
