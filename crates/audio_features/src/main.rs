//! Runs the shared mel front end over raw PCM, for the training pipeline.
//!
//! Reads mono little-endian f32 PCM on stdin and writes little-endian f32
//! frames of `N_MELS` values on stdout, or with `--model` the beat and
//! downbeat probabilities of each frame. Trailing samples that do not
//! complete a hop produce no frame.

use std::{
    io::{self, Read, Write},
    path::PathBuf,
};

use blaulicht_audio_features::{
    BeatModel, MelFrontend, FEATURE_VERSION, FRAME_RATE, MEL_FMAX_HZ, MEL_FMIN_HZ, N_MELS,
};
use clap::Parser;

#[derive(Debug, Parser)]
#[command(about = "Compute log-mel frames from mono f32le PCM on stdin")]
struct Args {
    #[arg(long)]
    sample_rate: u32,
    /// Print the front-end configuration as JSON instead of processing stdin.
    #[arg(long)]
    info: bool,
    /// Run this exported beat model and write its activations instead of
    /// the mel frames.
    #[arg(long)]
    model: Option<PathBuf>,
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let mut frontend = MelFrontend::new(args.sample_rate).map_err(anyhow::Error::msg)?;
    let mut model = match &args.model {
        Some(path) => Some(BeatModel::load(path).map_err(anyhow::Error::msg)?),
        None => None,
    };

    if args.info {
        let c = frontend.config();
        let centers: Vec<String> = MelFrontend::band_centers_hz()
            .iter()
            .map(|hz| format!("{hz:.2}"))
            .collect();
        println!(
            "{{\"version\": {FEATURE_VERSION}, \"frame_rate\": {FRAME_RATE}, \"n_mels\": {N_MELS}, \
             \"fmin_hz\": {MEL_FMIN_HZ}, \"fmax_hz\": {MEL_FMAX_HZ}, \"sample_rate\": {}, \
             \"hop\": {}, \"window_len\": {}, \"fft_size\": {}, \"band_centers_hz\": [{}]}}",
            c.sample_rate,
            c.hop,
            c.window_len,
            c.fft_size,
            centers.join(", ")
        );
        return Ok(());
    }

    let mut stdin = io::stdin().lock();
    let mut stdout = io::BufWriter::new(io::stdout().lock());
    let mut bytes = vec![0u8; 1 << 16];
    let mut carry = Vec::new();
    let mut samples = Vec::new();
    let mut frames = Vec::new();
    loop {
        let n = stdin.read(&mut bytes)?;
        if n == 0 {
            break;
        }
        // Reads can split a sample; keep the partial bytes for the next read.
        carry.extend_from_slice(&bytes[..n]);
        let whole = carry.len() / 4 * 4;
        samples.clear();
        samples.extend(
            carry[..whole]
                .chunks_exact(4)
                .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])),
        );
        carry.drain(..whole);

        frames.clear();
        frontend.push(&samples, &mut frames);
        for frame in &frames {
            if let Some(model) = &mut model {
                let act = model.step(frame);
                stdout.write_all(&act.beat.to_le_bytes())?;
                stdout.write_all(&act.downbeat.to_le_bytes())?;
            } else {
                for value in frame {
                    stdout.write_all(&value.to_le_bytes())?;
                }
            }
        }
    }
    stdout.flush()?;
    Ok(())
}
