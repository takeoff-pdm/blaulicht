//
// Provides analysis on the audio.
//
// use crate::{
//     audio::collector::{CollectorScratch, SignalCollector},
//     msg::{BpmInfo, Signal},
//     shift_push, signal, util,
// };
// use audioviz::spectrum::Frequency;

#[derive(Clone, Copy, Debug, Default)]
pub struct Frequency {
    pub volume: f32,

    /// Actual frequency in hz, can range from 0 to `config.sample_rate` / 2
    ///
    /// Accuracy can vary and is not guaranteed
    pub freq: f32,

    /// Relative position of single frequency in range (0..=1)
    ///
    /// Used to make lower freqs occupy more space than higher ones, to mimic human hearing
    ///
    /// Should not be Important, except when distributing freqs manually
    ///
    /// To do this manually set `config.interpolation` equal to `Interpolation::None`
    pub position: f32,
}

#[cfg(feature = "stream_in")]
impl From<audioviz::spectrum::Frequency> for Frequency {
    fn from(value: audioviz::spectrum::Frequency) -> Self {
        Self {
            volume: value.volume,
            freq: value.freq,
            position: value.position,
        }
    }
}

#[cfg(feature = "stream_in")]
impl From<&audioviz::spectrum::Frequency> for Frequency {
    fn from(value: &audioviz::spectrum::Frequency) -> Self {
        Self {
            volume: value.volume,
            freq: value.freq,
            position: value.position,
        }
    }
}

use crossbeam_channel::Sender;
use itertools::Itertools;
use map_range::MapRange;
use std::{
    cmp::Ordering,
    collections::VecDeque,
    time::{self, Instant},
    u8,
};

use crate::{AudioSource, BpmInfo, Signal, SignalCollector};

// Constants.
pub const BASS_FRAMES: usize = 10000;
pub const BASS_PEAK_FRAMES: usize = 800;
pub const BASS_MODIFIER: usize = 60;

// TODO: what is this constant even
pub const ROLLING_AVERAGE_VOLUME_SAMPLE_SIZE: usize = 100;
pub const ROLLING_AVERAGE_FRAMES: usize = 100;
pub const LONG_HISTORIC_FRAMES: usize = ROLLING_AVERAGE_FRAMES * 1000;

///
/// Vector push operations.
///

#[macro_export]
macro_rules! shift_push {
    ($vector:expr,$capacity:ident,$item:expr) => {
        $vector.push_back($item);
        if $vector.len() > $capacity {
            $vector.pop_front();
        }
    };
}

impl<const NUM_OUTPUTS: usize, SourceT> SignalCollector<NUM_OUTPUTS, SourceT>
where
    SourceT: AudioSource,
{
    #[inline(always)]
    pub fn bass(&mut self, now: usize) -> anyhow::Result<()> {
        let signals = {
            const USES_BASS: bool = true;

            let (v, lower_volume_limit) = match USES_BASS {
                true => (
                    self.freq_buffer
                        .iter()
                        .filter(|f| f.freq > 20.0 && f.freq < 250.0)
                        .map(|f| f.volume as usize)
                        .collect::<Vec<usize>>(),
                    100.0,
                ),
                false => (
                    self.freq_buffer
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
                Some(last) => now - last,
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
                    self.scratch.bass_peaks.push_back(now);
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
                        // panic!("a = {a} | b = {b}");
                        let d = (*b as f64 - *a as f64) / 1000.0;
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

            let beat_marker_elapsed = (now - self.scratch.time_of_last_bpm_marker) as u32;
            // now
            // .duration_since(self.scratch.time_of_last_bpm_marker)
            // .as_millis() as u32;

            if !self.scratch.is_on_beat
                && bpm > 0
                && (beat_marker_elapsed >= time_between_beats_millis as u32
                    || self.scratch.beat_needs_sync)
            {
                // println!("beat detection logic called");
                // Is initial beat: Wait for actual beat.
                if beat_marker_elapsed > 1000 || self.scratch.beat_needs_sync {
                    if peaked {
                        self.scratch.is_on_beat = true;
                        self.scratch.time_of_last_bpm_marker = now;
                        self.scratch.beat_needs_sync = false;
                    } else {
                        // println!("waiting for first actual beat for sync.");
                    }
                } else {
                    self.scratch.is_on_beat = true;
                    self.scratch.time_of_last_bpm_marker = now;
                }
            }

            let is_bass_avg_short = peaked || elapsed_since_last_peak < 50; // cross-tick mitigation
            if self.scratch.is_on_beat && !is_bass_avg_short {
                self.scratch.num_beat_mismatches += 1;
            } else if self.scratch.is_on_beat && is_bass_avg_short {
                self.scratch.num_beat_mismatches = 0;
            }

            if self.scratch.num_beat_mismatches > 3 && bpm > 0 {
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
        };

        self.send_signals(signals);

        Ok(())
    }

    #[inline(always)]
    pub fn beat_volume(&mut self) -> anyhow::Result<()> {
        // 1. Clear buffer 2. Fill it up again
        self.scratch.beat_volume_volume_samples_buffer.clear();
        self.scratch.beat_volume_volume_samples_buffer.extend(
            self.freq_buffer
                .chunks(2)
                .map(|f| f.iter().map(|e| e.volume as usize).max().unwrap_or(0)),
        );

        // let curr: Vec<usize> = self
        //     .freq_buffer
        //     .chunks(2)
        //     // TODO: only look at the bass line?
        //     .map(|f| f.iter().map(|e| e.volume as usize).max().unwrap())
        //     .collect();

        let curr_unfiltered: usize = self.freq_buffer.iter().map(|f| f.volume as usize).sum();
        shift_push!(
            self.scratch.long_historic,
            LONG_HISTORIC_FRAMES,
            curr_unfiltered
        );

        let curr_max = self
            .scratch
            .beat_volume_volume_samples_buffer
            .iter()
            .max()
            .unwrap_or(&0);
        shift_push!(self.scratch.historic, ROLLING_AVERAGE_FRAMES, *curr_max);

        let min = self
            .scratch
            .historic
            .iter()
            .min()
            .unwrap_or(&usize::MIN)
            .to_owned();

        let max = self
            .scratch
            .historic
            .iter()
            .max()
            .unwrap_or(&usize::MAX)
            .to_owned()
            .max(min + 1);

        const MAX_BEAT_VOLUME: u8 = 255;

        let index_mapped = curr_max.map_range(min..max, 0..MAX_BEAT_VOLUME as usize);

        if self.scratch.last_index != index_mapped {
            self.scratch.last_index = index_mapped;
            self.send_signals(&[Signal::BeatVolume(index_mapped as u8)]);
        }

        Ok(())
    }

    #[inline(always)]
    pub fn volume(&mut self) -> anyhow::Result<()> {
        self.send_signals({
            let volume_mean = ((self.scratch.volume_samples.iter().sum::<usize>() as f32)
                / (self.scratch.volume_samples.len() as f32)
                * 10.0) as usize;

            // let volume_sum = self.freqs.iter().map(|f| f.volume).sum::<f32>() * 10.0;
            // let volume_avg = volume_sum / self.freqs.len() as f32;

            let volume = volume_mean as u8;
            &[Signal::Volume(volume)]
        });

        let curr_max = (self
            .freq_buffer
            .iter()
            // .max_by_key(|f| (f.volume * 10.0) as usize)
            // .unwrap_or(&Frequency {
            //     volume: 0f32,
            //     freq: 0f32,
            //     position: 0f32,
            // })
            .map(|f| f.volume)
            .sum::<f32>()
            * 10.0
            / self.freq_buffer.len() as f32) as usize;
        // .volume as usize;

        // TODO: this is fake, this is not even the average.

        shift_push!(
            self.scratch.volume_samples,
            ROLLING_AVERAGE_VOLUME_SAMPLE_SIZE,
            curr_max
        );

        Ok(())
    }
}
