//! Audio file to mono PCM, with the title and artist tags.

use std::{fs::File, path::Path};

use anyhow::{anyhow, Context};
use symphonia::core::{
    audio::SampleBuffer,
    codecs::{DecoderOptions, CODEC_TYPE_NULL},
    errors::Error as SymphoniaError,
    formats::FormatOptions,
    io::MediaSourceStream,
    meta::{MetadataOptions, MetadataRevision, StandardTagKey},
    probe::Hint,
};

pub struct Decoded {
    pub samples: Vec<f32>,
    pub sample_rate: u32,
    pub title: Option<String>,
    pub artist: Option<String>,
}

pub fn decode(path: &Path) -> anyhow::Result<Decoded> {
    let file = File::open(path).with_context(|| format!("opening {}", path.display()))?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }
    let mut probed = symphonia::default::get_probe()
        .format(
            &hint,
            mss,
            &FormatOptions {
                enable_gapless: true,
                ..Default::default()
            },
            &MetadataOptions::default(),
        )
        .with_context(|| format!("{}: unsupported format", path.display()))?;

    let (mut title, mut artist) = (None, None);
    let mut read_tags = |rev: &MetadataRevision| {
        for tag in rev.tags() {
            let value = tag.value.to_string().trim().to_string();
            if value.is_empty() {
                continue;
            }
            match tag.std_key {
                Some(StandardTagKey::TrackTitle) => title = Some(value),
                Some(StandardTagKey::Artist) => artist = Some(value),
                _ => {}
            }
        }
    };
    // ID3 sits outside the container (probe), Vorbis comments inside it.
    if let Some(rev) = probed.metadata.get().as_ref().and_then(|m| m.current().cloned()) {
        read_tags(&rev);
    }
    let mut format = probed.format;
    if let Some(rev) = format.metadata().current() {
        read_tags(rev);
    }

    let track = format
        .tracks()
        .iter()
        .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
        .ok_or_else(|| anyhow!("{}: no audio track", path.display()))?;
    let track_id = track.id;
    let sample_rate = track
        .codec_params
        .sample_rate
        .ok_or_else(|| anyhow!("{}: unknown sample rate", path.display()))?;
    let mut decoder =
        symphonia::default::get_codecs().make(&track.codec_params, &DecoderOptions::default())?;

    let mut samples = Vec::new();
    // Reused while packets keep their layout; holds its capacity in frames.
    let mut buffer: Option<(SampleBuffer<f32>, u64, symphonia::core::audio::SignalSpec)> = None;
    loop {
        let packet = match format.next_packet() {
            Ok(packet) => packet,
            Err(SymphoniaError::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                break
            }
            Err(SymphoniaError::ResetRequired) => break,
            Err(e) => return Err(e).context(format!("{}: reading", path.display())),
        };
        if packet.track_id() != track_id {
            continue;
        }
        let decoded = match decoder.decode(&packet) {
            Ok(decoded) => decoded,
            // A corrupt frame: skip it, as players do.
            Err(SymphoniaError::DecodeError(_)) => continue,
            Err(e) => return Err(e).context(format!("{}: decoding", path.display())),
        };
        let spec = *decoded.spec();
        let channels = spec.channels.count().max(1);
        let frames = decoded.capacity() as u64;
        let buf = match &mut buffer {
            Some((buf, cap, old)) if *cap >= frames && *old == spec => buf,
            slot => &mut slot.insert((SampleBuffer::new(frames, spec), frames, spec)).0,
        };
        buf.copy_interleaved_ref(decoded);
        samples.extend(
            buf.samples()
                .chunks_exact(channels)
                .map(|frame| frame.iter().sum::<f32>() / channels as f32),
        );
    }
    anyhow::ensure!(!samples.is_empty(), "{}: no audio decoded", path.display());

    Ok(Decoded {
        samples,
        sample_rate,
        title,
        artist,
    })
}

/// Linear resampling, for the rare file whose rate the mel front end cannot
/// take (it needs a multiple of 100 Hz of at least 22 kHz).
pub fn resample(samples: &[f32], from: u32, to: u32) -> Vec<f32> {
    if from == to {
        return samples.to_vec();
    }
    let ratio = from as f64 / to as f64;
    let len = (samples.len() as f64 / ratio) as usize;
    (0..len)
        .map(|i| {
            let x = i as f64 * ratio;
            let j = x as usize;
            let frac = (x - j as f64) as f32;
            let a = samples[j];
            let b = samples.get(j + 1).copied().unwrap_or(a);
            a + (b - a) * frac
        })
        .collect()
}
