use std::{
    collections::VecDeque,
    time::{self, Instant},
    u8,
};

use crossbeam_channel::Sender;

use audioviz::spectrum::Frequency;
use itertools::Itertools;


use crate::{
    audio::{capture::SignalCollector, SIGNAL_SPEED}, msg::{BpmInfo, Signal}, shift_push, signal, util
};

// Constants.
pub const BASS_FRAMES: usize = 10000;
pub const BASS_PEAK_FRAMES: usize = 800;

pub const ROLLING_AVERAGE_LOOP_ITERATIONS: usize = 100;
pub const ROLLING_AVERAGE_VOLUME_SAMPLE_SIZE: usize = ROLLING_AVERAGE_LOOP_ITERATIONS / 2;

#[inline(always)]
pub fn bass(
    now: Instant,
    time_of_last_beat_publish_parent: &mut Instant,
    signal_out_0: &Sender<Signal>,
    signal_collector: &mut SignalCollector,
    values: &[Frequency],
    bass_samples: &mut VecDeque<u8>,
    bass_modifier: u8,
    bass_peaks: &mut VecDeque<Instant>,
) -> anyhow::Result<()> {
    signal!(
        now,
        time_of_last_beat_publish_parent,
        signal_out_0,
        signal_collector,
        {
            let v = values
                .iter()
                .map(|f| f.volume as usize)
                .collect::<Vec<usize>>();

            let avg = v.iter().sum::<usize>() as f32 / v.len() as f32;
            let bass_sig = (avg * 100.0) as u8;

            // Bass samples.
            bass_samples.push_back(bass_sig);

            if bass_samples.len() >= BASS_FRAMES {
                bass_samples.pop_front();
            }

            let bass_moving_average =
                bass_samples.iter().map(|v| *v as f64).sum::<f64>() / BASS_FRAMES as f64;

            let elapsed_since_last_peak = match bass_peaks.iter().last() {
                Some(last) => last.elapsed().as_millis(),
                None => 10000,
            };

            // Must be in the upper 90% to be a peak.
            // Do not consider values under bass 10.
            let mut peaked = false;

            if bass_moving_average >= 30.0 {
                let bass_moving_average_theoretical_max =
                    (bass_moving_average * 2.0) * (bass_modifier as f64 / 100.0);

                if bass_sig >= bass_moving_average_theoretical_max as u8
                    && elapsed_since_last_peak > 300
                {
                    bass_peaks.push_back(Instant::now());
                    peaked = true;
                }
            }

            if bass_peaks.len() >= BASS_PEAK_FRAMES {
                bass_peaks.pop_front();
            }

            const SECONDS_IN_A_MINUTE: f64 = 60.0;
            const MINIMUM_BPM: f64 = 90.0;
            const MAXIMUM_BPM: f64 = 200.0;
            const MAX_BPM_TIME_BETWEEN_SECS: f64 = SECONDS_IN_A_MINUTE / MINIMUM_BPM;
            const MIN_BPM_TIME_BETWEEN_SECS: f64 = SECONDS_IN_A_MINUTE / MAXIMUM_BPM;

            let bass_peak_durations = bass_peaks.iter().tuple_windows().filter_map(|(a, b)| {
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

            let bpm = if bass_moving_average <= 30.0 {
                0.0
            } else {
                SECONDS_IN_A_MINUTE / avg_bass_peak_durations
            };

            let bpm = bpm as u8;

            &[
                Signal::Bass(bass_sig),
                Signal::Bpm(BpmInfo {
                    bpm,
                    time_between_beats_millis: (avg_bass_peak_durations * 1000.0) as u16,
                }),
                if peaked || elapsed_since_last_peak < 100 {
                    Signal::BassAvgShort(255)
                } else if bass_moving_average > 40.0 {
                    Signal::BassAvgShort((elapsed_since_last_peak / 10) as u8)
                } else {
                    Signal::BassAvgShort(0)
                },
                Signal::BassAvg(bass_moving_average as u8),
            ]
        }
    );

    Ok(())
}

#[inline(always)]
pub fn beat_volume(
    values: &[Frequency],
    time_of_last_beat_publish: &mut Instant,
    signal_out_0: &Sender<Signal>,
    signal_collector: &mut SignalCollector,
    historic: &mut VecDeque<usize>,
    long_historic: &mut VecDeque<usize>,
    rolling_average_frames: usize,
    long_historic_frames: usize,
    last_index: &mut usize,
) -> anyhow::Result<()> {
    let curr: Vec<usize> = values
        .chunks(2)
        // TODO: only look at the bass line?
        .map(|f| f.iter().map(|e| e.volume as usize).max().unwrap())
        .collect();

    let curr_unfiltered: usize = values.iter().map(|f| f.volume as usize).sum();
    shift_push!(long_historic, long_historic_frames, curr_unfiltered);

    let curr = curr.iter().max().unwrap_or(&0);
    shift_push!(historic, rolling_average_frames, *curr);

    let max = historic.iter().max().unwrap_or(&usize::MAX);
    let min = historic.iter().min().unwrap_or(&usize::MIN);

    const MAX_BEAT_VOLUME: u8 = 255;
    // TODO: use map range crate.
    let index_mapped = util::map(
        *curr as isize,
        *min as isize,
        *max as isize,
        0,
        MAX_BEAT_VOLUME as isize,
    );

    if *last_index != index_mapped {
        let now = time::Instant::now();
        signal!(
            now,
            time_of_last_beat_publish,
            signal_out_0,
            signal_collector,
            {
                *last_index = index_mapped;
                &[Signal::BeatVolume(index_mapped as u8)]
            }
        );
    }

    Ok(())
}

#[inline(always)]
pub fn volume(
    now: Instant,
    time_of_last_volume_publish: &mut Instant,
    signal_out_0: &Sender<Signal>,
    signal_collector: &mut SignalCollector,
    values: &[Frequency],
    volume_samples: &mut VecDeque<usize>,
) -> anyhow::Result<()> {
    signal!(
        now,
        time_of_last_volume_publish,
        signal_out_0,
        signal_collector,
        {
            let volume_mean = ((volume_samples.iter().sum::<usize>() as f32)
                / (volume_samples.len() as f32)
                * 10.0) as usize;

            let volume = volume_mean as u8;
            &[Signal::Volume(volume)]
        }
    );

    let curr_avg = values
        .iter()
        .max_by_key(|f| (f.volume * 10.0) as usize)
        .unwrap_or(&Frequency {
            volume: 0f32,
            freq: 0f32,
            position: 0f32,
        })
        .volume as usize;

    shift_push!(volume_samples, ROLLING_AVERAGE_VOLUME_SAMPLE_SIZE, curr_avg);

    Ok(())
}
