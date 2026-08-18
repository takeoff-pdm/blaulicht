//! Sound-reactive modulation: turning the audio analysis output into the
//! per-property scale/offset contributions an animation layer produces.
//!
//! The math here is deliberately free of engine state so the animation tick
//! and the animation editor's live preview can share it.

use blaulicht_audio_engine::{AudioBucket, CollectorOutput};
use blaulicht_shared::{
    AudioModulationBlend, AudioModulationShaping, AudioModulationSignal, AudioModulationSpec,
    AudioSourceStatus, CollectedAudioSnapshot,
};

/// A `NoFrame` source is still considered live while its last frame is this
/// fresh; anything older targets zero and decays with the configured release.
pub const MAX_FRAME_AGE_MS: u32 = 250;

/// The beat clock may only stand in for a missed onset once the tempo estimate
/// is at least this confident.
pub const MIN_BEAT_FALLBACK_CONFIDENCE: f32 = 0.5;

/// Why a layer is (or isn't) receiving usable audio, surfaced in the editor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioHealth {
    Live,
    /// Source produced no new frame but the last one is still fresh enough.
    Holding {
        frame_age_ms: u32,
    },
    Stale {
        frame_age_ms: u32,
    },
    Unavailable(AudioSourceStatus),
}

impl AudioHealth {
    pub fn is_valid(&self) -> bool {
        matches!(self, Self::Live | Self::Holding { .. })
    }

    pub fn label(&self) -> String {
        match self {
            Self::Live => "live".to_string(),
            Self::Holding { frame_age_ms } => format!("holding ({frame_age_ms} ms)"),
            Self::Stale { frame_age_ms } => format!("stale ({frame_age_ms} ms)"),
            Self::Unavailable(status) => format!("{status:?}").to_lowercase(),
        }
    }
}

/// Classifies the audio input. Only `Active` and a sufficiently fresh
/// `NoFrame` count as valid; disconnected, ended and errored sources do not.
pub fn audio_health(audio: &CollectedAudioSnapshot) -> AudioHealth {
    match audio.source_status {
        AudioSourceStatus::Active => AudioHealth::Live,
        AudioSourceStatus::NoFrame if audio.frame_age_ms <= MAX_FRAME_AGE_MS => {
            AudioHealth::Holding {
                frame_age_ms: audio.frame_age_ms,
            }
        }
        AudioSourceStatus::NoFrame => AudioHealth::Stale {
            frame_age_ms: audio.frame_age_ms,
        },
        other => AudioHealth::Unavailable(other),
    }
}

/// RMS energy of every FFT bucket overlapping `[min_hz, max_hz]`, normalized
/// to `0..1`.
///
/// Bucket bounds come from the analysis output rather than being reconstructed
/// from the bucket index — the spectrum is not binned into uniformly spaced
/// frequencies.
pub fn band_energy(column: &[AudioBucket], min_hz: f64, max_hz: f64) -> f64 {
    // Bounds are used only for comparison, so they are taken as given: a range
    // that lies outside the spectrum simply selects no buckets.
    let (low, high) = (min_hz.min(max_hz), min_hz.max(max_hz));

    let mut sum_squares = 0.0;
    let mut count = 0_usize;
    for bucket in column {
        let bucket_low = bucket.freq_bound_lower as f64;
        let bucket_high = bucket.freq_bound_upper.max(bucket.freq_bound_lower) as f64;
        // Half-open overlap so touching bounds still select the bucket.
        if bucket_high < low || bucket_low > high {
            continue;
        }
        let normalized = bucket.volume as f64 / u8::MAX as f64;
        sum_squares += normalized * normalized;
        count += 1;
    }

    if count == 0 {
        return 0.0;
    }
    (sum_squares / count as f64).sqrt().clamp(0.0, 1.0)
}

/// The raw, unshaped `0..1` value of a continuous signal. Pulse signals are
/// event-driven and handled by [`AudioModulationRuntime::poll_pulse`].
pub fn raw_continuous_signal(signal: &AudioModulationSignal, audio: &CollectorOutput) -> f64 {
    match signal {
        AudioModulationSignal::Energy => audio.snapshot.volume as f64 / u8::MAX as f64,
        AudioModulationSignal::Band {
            freq_min_hz,
            freq_max_hz,
        } => band_energy(
            &audio.current_audio_colunn,
            *freq_min_hz as f64,
            *freq_max_hz as f64,
        ),
        // Pulses carry no continuous level of their own.
        AudioModulationSignal::BeatPulse => 0.0,
        // Compatibility signals never take the shaped path.
        _ => 0.0,
    }
}

/// Applies threshold, sensitivity, inversion and the section multiplier.
pub fn shape(
    raw: f64,
    shaping: &AudioModulationShaping,
    section: blaulicht_shared::SectionState,
) -> f64 {
    let threshold = (shaping.threshold as f64).clamp(0.0, blaulicht_shared::MAX_THRESHOLD as f64);
    let span = 1.0 - threshold;
    let gated = ((raw.clamp(0.0, 1.0) - threshold) / span).clamp(0.0, 1.0);

    let sensitivity = if shaping.sensitivity.is_finite() {
        shaping.sensitivity.max(0.0) as f64
    } else {
        1.0
    };
    let mut value = (gated * sensitivity).clamp(0.0, 1.0);

    if shaping.invert {
        value = 1.0 - value;
    }

    (value * shaping.section_multiplier(section) as f64).clamp(0.0, 1.0)
}

/// The per-layer state that survives between engine ticks.
#[derive(Clone, Debug, Default)]
pub struct AudioModulationRuntime {
    envelope: f64,
    last_update_ms: u64,
    initialized: bool,
    last_onset_event_id: u64,
    last_beat_event_id: u64,
    /// Whether an onset already covered the beat the clock is about to report.
    onset_since_last_beat: bool,
    /// Generation marker used to drop runtimes for animations that went away.
    pub(crate) seen_generation: u64,
}

impl AudioModulationRuntime {
    pub fn envelope(&self) -> f64 {
        self.envelope
    }

    /// Returns whether a hybrid pulse fires this tick.
    ///
    /// Onsets fire immediately. A clock beat only stands in when no onset
    /// covered it, the tempo estimate is confident enough and the audio is
    /// fresh. Both event IDs are consumed regardless so a single event can
    /// never fire twice.
    pub fn poll_pulse(&mut self, audio: &CollectedAudioSnapshot, audio_valid: bool) -> bool {
        let mut fired = false;

        if audio.onset_event_id != 0 && audio.onset_event_id != self.last_onset_event_id {
            self.last_onset_event_id = audio.onset_event_id;
            self.onset_since_last_beat = true;
            fired |= audio_valid;
        }

        if audio.beat_event_id != 0 && audio.beat_event_id != self.last_beat_event_id {
            self.last_beat_event_id = audio.beat_event_id;
            let covered_by_onset = self.onset_since_last_beat;
            self.onset_since_last_beat = false;
            if !covered_by_onset
                && audio_valid
                && audio.bpm_confidence >= MIN_BEAT_FALLBACK_CONFIDENCE
            {
                fired = true;
            }
        }

        fired
    }

    /// Moves the envelope toward `target` using elapsed-time exponential
    /// smoothing, so the result does not depend on the engine tick rate.
    pub fn advance(&mut self, now_ms: u64, target: f64, shaping: &AudioModulationShaping) -> f64 {
        let elapsed_ms = if self.initialized {
            now_ms.saturating_sub(self.last_update_ms) as f64
        } else {
            0.0
        };
        self.initialized = true;
        self.last_update_ms = now_ms;

        let tau_ms = if target > self.envelope {
            shaping.attack_ms
        } else {
            shaping.release_ms
        } as f64;

        if tau_ms <= 0.0 {
            self.envelope = target;
        } else if elapsed_ms > 0.0 {
            let alpha = 1.0 - (-elapsed_ms / tau_ms).exp();
            self.envelope += (target - self.envelope) * alpha;
        }

        self.envelope = self.envelope.clamp(0.0, 1.0);
        self.envelope
    }

    /// Advances one tick of a non-compatibility layer and returns its envelope.
    ///
    /// Invalid audio always targets zero and decays with the configured
    /// release; inversion is deliberately not applied in that case, so a
    /// dropout fades out instead of latching to full.
    pub fn tick(
        &mut self,
        now_ms: u64,
        spec: &AudioModulationSpec,
        audio: &CollectorOutput,
    ) -> AudioModulationSample {
        let health = audio_health(&audio.snapshot);
        let valid = health.is_valid();

        let pulse = matches!(spec.signal, AudioModulationSignal::BeatPulse)
            .then(|| self.poll_pulse(&audio.snapshot, valid));

        let raw = match pulse {
            Some(fired) => f64::from(u8::from(fired)),
            None => raw_continuous_signal(&spec.signal, audio),
        };

        let target = if valid {
            shape(raw, &spec.shaping, audio.snapshot.section_state)
        } else {
            0.0
        };

        let envelope = self.advance(now_ms, target, &spec.shaping);

        AudioModulationSample {
            raw: if valid { raw } else { 0.0 },
            envelope,
            health,
        }
    }
}

/// One tick's worth of observable state for a layer, used for the editor's
/// live readouts as well as for producing output.
#[derive(Debug, Clone, Copy)]
pub struct AudioModulationSample {
    pub raw: f64,
    pub envelope: f64,
    pub health: AudioHealth,
}

/// The contribution a layer makes to one property this tick.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AudioModulationContribution {
    /// Multiplied into the property's running scale factor.
    Scale(f64),
    /// Summed into the property's running offset.
    Add(f64),
}

impl AudioModulationContribution {
    /// Signed magnitude, for display purposes.
    pub fn display_value(&self) -> f64 {
        match self {
            Self::Scale(factor) => *factor,
            Self::Add(offset) => *offset,
        }
    }
}

/// Maps an envelope through the blend mode. Returns `None` for the
/// compatibility absolute output, which is produced separately.
pub fn contribution(
    blend: &AudioModulationBlend,
    envelope: f64,
) -> Option<AudioModulationContribution> {
    match blend {
        AudioModulationBlend::Add { depth } => {
            let depth = if depth.is_finite() {
                *depth as f64
            } else {
                0.0
            };
            Some(AudioModulationContribution::Add(envelope * depth))
        }
        AudioModulationBlend::Scale { peak_percent } => {
            let peak = if peak_percent.is_finite() {
                (*peak_percent).max(0.0) as f64 / 100.0
            } else {
                1.0
            };
            Some(AudioModulationContribution::Scale(envelope * peak))
        }
        AudioModulationBlend::LegacyAbsolute => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use blaulicht_shared::SectionState;

    fn bucket(volume: u8, lower: u64, upper: u64) -> AudioBucket {
        AudioBucket {
            volume,
            freq_bound_lower: lower,
            freq_bound_upper: upper,
        }
    }

    fn snapshot(status: AudioSourceStatus) -> CollectedAudioSnapshot {
        CollectedAudioSnapshot {
            source_status: status,
            ..Default::default()
        }
    }

    fn output(snapshot: CollectedAudioSnapshot) -> CollectorOutput {
        CollectorOutput {
            snapshot,
            debug_data: Default::default(),
            current_audio_colunn: Vec::new(),
        }
    }

    #[test]
    fn health_treats_fresh_no_frame_as_valid_and_stale_as_invalid() {
        assert!(audio_health(&snapshot(AudioSourceStatus::Active)).is_valid());

        let mut no_frame = snapshot(AudioSourceStatus::NoFrame);
        no_frame.frame_age_ms = MAX_FRAME_AGE_MS;
        assert!(audio_health(&no_frame).is_valid());

        no_frame.frame_age_ms = MAX_FRAME_AGE_MS + 1;
        assert!(!audio_health(&no_frame).is_valid());

        for status in [
            AudioSourceStatus::Disconnected,
            AudioSourceStatus::Ended,
            AudioSourceStatus::Error,
        ] {
            assert!(!audio_health(&snapshot(status)).is_valid(), "{status:?}");
        }
    }

    #[test]
    fn band_energy_uses_real_bucket_bounds_not_bucket_index() {
        // Deliberately non-uniform bucket widths: an index-based selector
        // would pick the wrong buckets here.
        let column = vec![
            bucket(255, 0, 30),
            bucket(200, 30, 200),
            bucket(0, 200, 6_000),
            bucket(255, 6_000, 20_000),
        ];

        // Only the second bucket overlaps 40..180 Hz.
        let energy = band_energy(&column, 40.0, 180.0);
        assert!((energy - 200.0 / 255.0).abs() < 1e-9);

        // Reversed bounds select the same buckets.
        assert_eq!(band_energy(&column, 180.0, 40.0), energy);

        // Empty selection is silent, not a panic.
        assert_eq!(band_energy(&column, 20_001.0, 20_002.0), 0.0);
        assert_eq!(band_energy(&[], 0.0, 20_000.0), 0.0);
    }

    #[test]
    fn band_energy_is_rms_over_overlapping_buckets() {
        let column = vec![bucket(255, 0, 100), bucket(0, 100, 200)];
        // RMS of {1.0, 0.0} is sqrt(0.5), not the mean of 0.5.
        assert!((band_energy(&column, 0.0, 200.0) - 0.5_f64.sqrt()).abs() < 1e-9);
    }

    #[test]
    fn threshold_rescales_the_remaining_range() {
        let shaping = AudioModulationShaping {
            threshold: 0.5,
            ..Default::default()
        };
        assert_eq!(shape(0.5, &shaping, SectionState::ActiveBeat), 0.0);
        assert_eq!(shape(0.25, &shaping, SectionState::ActiveBeat), 0.0);
        assert!((shape(0.75, &shaping, SectionState::ActiveBeat) - 0.5).abs() < 1e-9);
        assert_eq!(shape(1.0, &shaping, SectionState::ActiveBeat), 1.0);
    }

    #[test]
    fn sensitivity_gains_then_clamps() {
        let shaping = AudioModulationShaping {
            sensitivity: 4.0,
            ..Default::default()
        };
        assert!((shape(0.1, &shaping, SectionState::ActiveBeat) - 0.4).abs() < 1e-9);
        assert_eq!(shape(0.5, &shaping, SectionState::ActiveBeat), 1.0);
    }

    #[test]
    fn inversion_mirrors_the_shaped_value() {
        let shaping = AudioModulationShaping {
            invert: true,
            ..Default::default()
        };
        assert_eq!(shape(0.0, &shaping, SectionState::ActiveBeat), 1.0);
        assert_eq!(shape(1.0, &shaping, SectionState::ActiveBeat), 0.0);
        assert!((shape(0.25, &shaping, SectionState::ActiveBeat) - 0.75).abs() < 1e-9);
    }

    #[test]
    fn section_multipliers_apply_per_section() {
        let shaping = AudioModulationShaping {
            breakdown_multiplier: 0.25,
            drop_multiplier: 2.0,
            active_beat_multiplier: 1.0,
            ..Default::default()
        };
        assert!((shape(0.4, &shaping, SectionState::Breakdown) - 0.1).abs() < 1e-9);
        // The multiplier can push the value up, but never past full scale.
        assert!((shape(0.4, &shaping, SectionState::Drop) - 0.8).abs() < 1e-9);
        assert_eq!(shape(0.6, &shaping, SectionState::Drop), 1.0);
        assert!((shape(0.4, &shaping, SectionState::ActiveBeat) - 0.4).abs() < 1e-9);
    }

    #[test]
    fn attack_and_release_depend_on_elapsed_time_not_tick_count() {
        let shaping = AudioModulationShaping {
            attack_ms: 100,
            release_ms: 100,
            ..Default::default()
        };

        let mut coarse = AudioModulationRuntime::default();
        coarse.advance(0, 1.0, &shaping);
        coarse.advance(100, 1.0, &shaping);

        let mut fine = AudioModulationRuntime::default();
        fine.advance(0, 1.0, &shaping);
        for step in 1..=10 {
            fine.advance(step * 10, 1.0, &shaping);
        }

        assert!(
            (coarse.envelope() - fine.envelope()).abs() < 1e-9,
            "{} vs {}",
            coarse.envelope(),
            fine.envelope()
        );
        // One time constant reaches 1 - 1/e.
        assert!((coarse.envelope() - (1.0 - (-1.0_f64).exp())).abs() < 1e-9);
    }

    #[test]
    fn zero_attack_snaps_and_release_decays() {
        let shaping = AudioModulationShaping {
            attack_ms: 0,
            release_ms: 100,
            ..Default::default()
        };
        let mut runtime = AudioModulationRuntime::default();
        assert_eq!(runtime.advance(0, 1.0, &shaping), 1.0);

        let after = runtime.advance(100, 0.0, &shaping);
        assert!((after - (-1.0_f64).exp()).abs() < 1e-9);
        assert!(after < 1.0 && after > 0.0);
    }

    #[test]
    fn invalid_audio_decays_toward_zero_even_when_inverted() {
        let spec = AudioModulationSpec {
            signal: AudioModulationSignal::Energy,
            shaping: AudioModulationShaping {
                invert: true,
                attack_ms: 0,
                release_ms: 100,
                ..Default::default()
            },
            blend: AudioModulationBlend::default(),
        };

        let mut runtime = AudioModulationRuntime::default();
        // Valid, silent, inverted input latches to full.
        let live = runtime.tick(0, &spec, &output(snapshot(AudioSourceStatus::Active)));
        assert_eq!(live.envelope, 1.0);

        // Losing the source decays instead of holding the inverted value.
        let dropped = runtime.tick(
            100,
            &spec,
            &output(snapshot(AudioSourceStatus::Disconnected)),
        );
        assert!(dropped.envelope < 1.0);
        assert!((dropped.envelope - (-1.0_f64).exp()).abs() < 1e-9);
        assert_eq!(dropped.raw, 0.0);
    }

    #[test]
    fn onsets_fire_immediately_and_are_deduplicated() {
        let mut runtime = AudioModulationRuntime::default();
        let mut audio = snapshot(AudioSourceStatus::Active);

        audio.onset_event_id = 7;
        assert!(runtime.poll_pulse(&audio, true));
        // Same event ID on the next tick must not fire again.
        assert!(!runtime.poll_pulse(&audio, true));

        audio.onset_event_id = 8;
        assert!(runtime.poll_pulse(&audio, true));
    }

    #[test]
    fn clock_beat_only_fires_when_no_onset_covered_it() {
        let mut runtime = AudioModulationRuntime::default();
        let mut audio = snapshot(AudioSourceStatus::Active);
        audio.bpm_confidence = 0.9;

        // Onset then beat in the same bar: exactly one fire, from the onset.
        audio.onset_event_id = 1;
        assert!(runtime.poll_pulse(&audio, true));
        audio.beat_event_id = 1;
        assert!(!runtime.poll_pulse(&audio, true));

        // A beat with no preceding onset falls back to the clock.
        audio.beat_event_id = 2;
        assert!(runtime.poll_pulse(&audio, true));
    }

    #[test]
    fn simultaneous_onset_and_beat_fire_once() {
        let mut runtime = AudioModulationRuntime::default();
        let mut audio = snapshot(AudioSourceStatus::Active);
        audio.bpm_confidence = 1.0;
        audio.onset_event_id = 4;
        audio.beat_event_id = 4;

        assert!(runtime.poll_pulse(&audio, true));
        assert!(!runtime.poll_pulse(&audio, true));
    }

    #[test]
    fn clock_fallback_is_gated_on_confidence_and_freshness() {
        let mut runtime = AudioModulationRuntime::default();
        let mut audio = snapshot(AudioSourceStatus::Active);

        audio.bpm_confidence = MIN_BEAT_FALLBACK_CONFIDENCE - 0.01;
        audio.beat_event_id = 1;
        assert!(!runtime.poll_pulse(&audio, true));

        audio.bpm_confidence = MIN_BEAT_FALLBACK_CONFIDENCE;
        audio.beat_event_id = 2;
        assert!(runtime.poll_pulse(&audio, true));

        // Stale audio never fires, but still consumes the event.
        audio.beat_event_id = 3;
        assert!(!runtime.poll_pulse(&audio, false));
        assert!(!runtime.poll_pulse(&audio, true));
    }

    #[test]
    fn blend_modes_map_the_envelope() {
        assert_eq!(
            contribution(&AudioModulationBlend::Add { depth: 200.0 }, 0.5),
            Some(AudioModulationContribution::Add(100.0))
        );
        assert_eq!(
            contribution(
                &AudioModulationBlend::Scale {
                    peak_percent: 200.0
                },
                0.5
            ),
            Some(AudioModulationContribution::Scale(1.0))
        );
        // A silent scale layer blacks the property out; that is the point.
        assert_eq!(
            contribution(
                &AudioModulationBlend::Scale {
                    peak_percent: 100.0
                },
                0.0
            ),
            Some(AudioModulationContribution::Scale(0.0))
        );
        assert_eq!(
            contribution(&AudioModulationBlend::LegacyAbsolute, 1.0),
            None
        );
    }
}
