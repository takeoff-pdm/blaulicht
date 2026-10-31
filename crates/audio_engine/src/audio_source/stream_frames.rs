use crate::Frequency;
use audioviz::spectrum::{config::StreamConfig, stream::Stream};

/// Turns interleaved PCM blocks into analysis frames the same way for the live
/// microphone and for offline replays of recorded audio, so a replay measures
/// exactly what the engine sees on stage.
pub struct StreamFrames {
    stream: Stream,
    channel_count: usize,
    captured_mono: Vec<f32>,
    freq_buffer: Vec<Frequency>,
}

impl StreamFrames {
    /// `channel_count` and `sample_rate` describe the PCM that will actually be
    /// pushed; they override the configured values, which only hold defaults.
    pub fn new(
        mut config: StreamConfig,
        channel_count: usize,
        sample_rate: u32,
        freq_buffer_size: usize,
    ) -> Self {
        // `.max(1)`: a zero channel count would panic in `chunks()`.
        let channel_count = channel_count.max(1);
        // A mismatched channel count makes audioviz de-interleave mono input
        // into fake channels (halving the effective sample rate), and a
        // mismatched rate shifts every bin's frequency label.
        config.channel_count = channel_count as u16;
        config.processor.sampling_rate = sample_rate;
        Self {
            stream: Stream::new(config),
            channel_count,
            captured_mono: Vec::new(),
            freq_buffer: vec![Frequency::default(); freq_buffer_size],
        }
    }

    pub fn frequencies(&self) -> &[Frequency] {
        &self.freq_buffer
    }

    /// Feed one interleaved block. Returns whether a new frame is available.
    pub fn push_block(&mut self, block: Vec<f32>) -> bool {
        if block.is_empty() {
            return false;
        }

        self.captured_mono.extend(
            block
                .chunks(self.channel_count)
                .map(|frame| frame.iter().copied().sum::<f32>() / frame.len() as f32),
        );
        self.stream.push_data(block);
        self.stream.update();

        let frequencies = self.stream.get_frequencies();
        let Some(frequencies) = frequencies.first().filter(|f| !f.is_empty()) else {
            return false;
        };

        // Write in place so the buffer keeps its configured size: shrinking it
        // to a short/partial frame would permanently truncate the spectrum for
        // every later frame.
        let n = frequencies.len().min(self.freq_buffer.len());
        for (slot, f) in self.freq_buffer.iter_mut().zip(frequencies.iter().take(n)) {
            *slot = Frequency {
                volume: f.volume * 10.0,
                freq: f.freq,
                position: f.position,
            };
        }
        for slot in self.freq_buffer.iter_mut().skip(n) {
            *slot = Frequency::default();
        }

        true
    }

    /// Move captured mono PCM into `output` for secondary analyzers.
    pub fn drain_samples(&mut self, output: &mut Vec<f32>) {
        output.append(&mut self.captured_mono);
    }
}
