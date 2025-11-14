//
// Provides analysis on the audio.
//
use crate::{
    audio::collector::{CollectorScratch, SignalCollector},
    msg::{BpmInfo, Signal},
    shift_push, signal, util,
};
use audioviz::spectrum::Frequency;
use crossbeam_channel::Sender;
use itertools::Itertools;
use std::{
    cmp::Ordering,
    collections::VecDeque,
    time::{self, Instant},
    u8,
};

// Constants.
pub const BASS_FRAMES: usize = 10000;
pub const BASS_PEAK_FRAMES: usize = 800;
pub const BASS_MODIFIER: usize = 60;

// TODO: what is this constant even
pub const ROLLING_AVERAGE_VOLUME_SAMPLE_SIZE: usize = 100;
pub const ROLLING_AVERAGE_FRAMES: usize = 100;
pub const LONG_HISTORIC_FRAMES: usize = ROLLING_AVERAGE_FRAMES * 1000;

impl<const NUM_OUTPUTS: usize> SignalCollector<NUM_OUTPUTS> {
    #[inline(always)]
    pub fn bass(&mut self, now: Instant) -> anyhow::Result<()> {
        signal!(self, {
            const USES_BASS: bool = true;

            let (v, lower_volume_limit) = match USES_BASS {
                true => (
                    self.freqs
                        .iter()
                        .filter(|f| f.freq > 20.0 && f.freq < 250.0)
                        .map(|f| f.volume as usize)
                        .collect::<Vec<usize>>(),
                    100.0,
                ),
                false => (
                    self.freqs
                        .iter()
                        .map(|f| f.volume as usize)
                        .collect::<Vec<usize>>(),
                    10.0,
                ),
            };

            let avg = v.iter().sum::<usize>() as f32 / v.len() as f32;
            let bass_sig = (avg * 100.0) as u8;

            // Bass samples.
            self.scratch.bass_samples.push_back(bass_sig);

            if self.scratch.bass_samples.len() >= BASS_FRAMES {
                self.scratch.bass_samples.pop_front();
            }

            let bass_moving_average = self
                .scratch
                .bass_samples
                .iter()
                .map(|v| *v as f64)
                .sum::<f64>()
                / BASS_FRAMES as f64;

            let elapsed_since_last_peak = match self.scratch.bass_peaks.iter().last() {
                Some(last) => last.elapsed().as_millis(),
                None => 10000,
            };

            // Must be in the upper 90% to be a peak.
            // Do not consider values under bass 10.
            let mut peaked = false;

            if bass_moving_average >= lower_volume_limit {
                let bass_signal_threshold_for_a_peak =
                    (bass_moving_average * 2.0) * (BASS_MODIFIER as f64 / 100.0);
                if bass_sig >= bass_signal_threshold_for_a_peak as u8
                    && elapsed_since_last_peak > 200
                {
                    self.scratch.bass_peaks.push_back(Instant::now());
                    peaked = true;
                }
            }

            if self.scratch.bass_peaks.len() >= BASS_PEAK_FRAMES {
                self.scratch.bass_peaks.pop_front();
            }

            const SECONDS_IN_A_MINUTE: f64 = 60.0;
            const MINIMUM_BPM: f64 = 90.0;
            const MAXIMUM_BPM: f64 = 200.0;
            const MAX_BPM_TIME_BETWEEN_SECS: f64 = SECONDS_IN_A_MINUTE / MINIMUM_BPM;
            const MIN_BPM_TIME_BETWEEN_SECS: f64 = SECONDS_IN_A_MINUTE / MAXIMUM_BPM;

            let bass_peak_durations =
                self.scratch
                    .bass_peaks
                    .iter()
                    .tuple_windows()
                    .filter_map(|(a, b)| {
                        let d = (b.duration_since(*a).as_millis() as f64) / 1000.0;
                        if d > MIN_BPM_TIME_BETWEEN_SECS && d < MAX_BPM_TIME_BETWEEN_SECS {
                            Some(d)
                        } else {
                            None
                        }
                    });

            let bass_len = bass_peak_durations
                .clone()
                .filter(|v| *v > MIN_BPM_TIME_BETWEEN_SECS && *v < MAX_BPM_TIME_BETWEEN_SECS)
                .count();

            let bass_peak_sum = bass_peak_durations.sum::<f64>();
            let avg_bass_peak_durations = bass_peak_sum / (bass_len as f64);

            let bpm = if bass_moving_average <= lower_volume_limit {
                0.0
            } else {
                SECONDS_IN_A_MINUTE / avg_bass_peak_durations
            };

            let bpm = bpm as u8;
            let time_between_beats_millis = ((avg_bass_peak_durations * 1000.0) as i16) as u16;

            let beat_marker_elapsed = now
                .duration_since(self.scratch.time_of_last_bpm_marker)
                .as_millis() as u32;

            if self.scratch.is_on_beat
                && bpm > 0
                && (beat_marker_elapsed >= time_between_beats_millis as u32
                    || self.scratch.beat_needs_sync)
            {
                // Is initial beat: Wait for actual beat.
                if beat_marker_elapsed > 1000 || self.scratch.beat_needs_sync {
                    if peaked {
                        self.scratch.is_on_beat = true;
                        self.scratch.time_of_last_bpm_marker = Instant::now();
                        self.scratch.beat_needs_sync = false;
                    } else {
                        // println!("waiting for first actual beat for sync.");
                    }
                } else {
                    self.scratch.is_on_beat = true;
                    self.scratch.time_of_last_bpm_marker = Instant::now();
                    // println!("is_on_beat = {is_on_beat}");
                }
            }

            let is_bass_avg_short = peaked || elapsed_since_last_peak < 50; // cross-tick mitigation
            if self.scratch.is_on_beat && !is_bass_avg_short {
                self.scratch.num_beat_mismatches += 1;
                // println!("drift = {}", *num_beat_mismatches);
            } else if self.scratch.is_on_beat && is_bass_avg_short {
                self.scratch.num_beat_mismatches = 0;
            }

            if self.scratch.num_beat_mismatches > 3 && bpm > 0 {
                // println!("Mismatch drift, resetting...");

                // Check if we are too early or too late.

                // let time_of_last_actual_peak = bass_peaks.iter().last().unwrap();

                // let diff_ms: i64 = if time_of_last_actual_peak > time_of_last_bpm_marker {
                //     time_of_last_actual_peak
                //         .duration_since(*time_of_last_bpm_marker)
                //         .as_millis() as i64
                // } else {
                //     -(time_of_last_bpm_marker
                //         .duration_since(*time_of_last_actual_peak)
                //         .as_millis() as i64)
                // };

                self.scratch.beat_needs_sync = true;
            }

            &[
                Signal::BeatTrigger(self.scratch.is_on_beat),
                Signal::Bass(bass_sig),
                Signal::Bpm(BpmInfo {
                    bpm,
                    time_between_beats_millis,
                }),
                if is_bass_avg_short {
                    Signal::BassAvgShort(255)
                } else {
                    Signal::BassAvgShort(0)
                },
                Signal::BassAvg(bass_moving_average as u8),
            ]
        });

        Ok(())
    }

    #[inline(always)]
    pub fn beat_volume(&mut self) -> anyhow::Result<()> {
        let curr: Vec<usize> = self
            .freqs
            .chunks(2)
            // TODO: only look at the bass line?
            .map(|f| f.iter().map(|e| e.volume as usize).max().unwrap())
            .collect();

        let curr_unfiltered: usize = self.freqs.iter().map(|f| f.volume as usize).sum();
        shift_push!(
            self.scratch.long_historic,
            LONG_HISTORIC_FRAMES,
            curr_unfiltered
        );

        let curr = curr.iter().max().unwrap_or(&0);
        shift_push!(self.scratch.historic, ROLLING_AVERAGE_FRAMES, *curr);

        let max = self.scratch.historic.iter().max().unwrap_or(&usize::MAX);
        let min = self.scratch.historic.iter().min().unwrap_or(&usize::MIN);

        const MAX_BEAT_VOLUME: u8 = 255;
        // TODO: use map range crate.
        let index_mapped = util::map(
            *curr as isize,
            *min as isize,
            *max as isize,
            0,
            MAX_BEAT_VOLUME as isize,
        );

        if self.scratch.last_index != index_mapped {
            signal!(self, {
                self.scratch.last_index = index_mapped;
                &[Signal::BeatVolume(index_mapped as u8)]
            });
        }

        Ok(())
    }

    #[inline(always)]
    pub fn volume(&mut self) -> anyhow::Result<()> {
        signal!(self, {
            let volume_mean = ((self.scratch.volume_samples.iter().sum::<usize>() as f32)
                / (self.scratch.volume_samples.len() as f32)
                * 10.0) as usize;

            let volume = volume_mean as u8;
            &[Signal::Volume(volume)]
        });

        let curr_avg = self
            .freqs
            .iter()
            .max_by_key(|f| (f.volume * 10.0) as usize)
            .unwrap_or(&Frequency {
                volume: 0f32,
                freq: 0f32,
                position: 0f32,
            })
            .volume as usize;

        // TODO: this is fake, this is not even the average.

        shift_push!(
            self.scratch.volume_samples,
            ROLLING_AVERAGE_VOLUME_SAMPLE_SIZE,
            curr_avg
        );

        Ok(())
    }
}
