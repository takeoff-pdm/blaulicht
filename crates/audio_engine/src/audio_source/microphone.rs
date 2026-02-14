use crate::{AudioSource, Frequency};
use audioviz::io::{Input, InputController};
use audioviz::spectrum::{config::StreamConfig, stream::Stream};
use std::time::Instant;

//
// MICROPHONE SOURCE.
//

pub struct AudioSourceMicrophone {
    input: Input,
    controller: InputController,
    stream: Stream,
    pub freq_buffer: Vec<Frequency>,
}

impl AudioSourceMicrophone {
    pub fn new(
        // TODO: fork audioviz / open issue to allow device pass thru
        // OR: use the nth-device option in the Device:: enum.
        _device: cpal::Device,
        config: StreamConfig,
        freq_buffer_size: usize,
    ) -> anyhow::Result<Self> {
        let stream = Stream::new(config);

        let latency = 10;
        let frames_10ms_at_48k = stream.config.processor.sampling_rate * latency / 1000;
        let buffer_size = Some(frames_10ms_at_48k);

        // set up capture on the same thread; only the CPAL callback runs elsewhere
        let mut input = Input::new();
        let (_channels, _sample_rate, controller) = input
            .init(&audioviz::io::Device::DefaultInput, buffer_size)
            .map_err(|err| anyhow::anyhow!("failed to init audio input: {:?}", err))?;

        Ok(Self {
            freq_buffer: vec![Frequency::default(); freq_buffer_size],
            input,
            controller,
            stream,
        })
    }
}

impl AudioSource for AudioSourceMicrophone {
    fn get_frequencies(&mut self, _now: usize) -> &[Frequency] {
        let start = Instant::now();
        let mut pulled_at: Option<Instant> = None;
        let mut updated_at: Option<Instant> = None;
        let mut got_freqs_at: Option<Instant> = None;

        // loop {
        // blocks until CPAL callback pushes a block into the channel
        if let Some(block) = self.controller.try_pull_data() {
            if !block.is_empty() {
                pulled_at = Some(Instant::now());
                self.stream.push_data(block);
                self.stream.update(); // FFT + post-processing on the main thread

                updated_at = Some(Instant::now());
            }
        }

        let frequencies = self.stream.get_frequencies();
        got_freqs_at = Some(Instant::now());

        // Note: use last available freqs (like interpolation but worse)
        if frequencies.is_empty() {
            return &self.freq_buffer;
        }

        let mut frequencies = frequencies[0].clone();

        if frequencies.is_empty() {
            return &self.freq_buffer;
        }

        while frequencies.len() > self.freq_buffer.len() {
            frequencies.pop();
        }

        self.freq_buffer.clear();
        self.freq_buffer
            .extend(frequencies.iter().map(|f| Frequency {
                volume: f.volume * 10.0,
                freq: f.freq,
                position: f.position,
            }));

        if let (Some(pulled), Some(updated), Some(got)) = (pulled_at, updated_at, got_freqs_at) {
            println!(
                "get_freqs timings (ms): pull_wait={}, update={}, get_freqs={}, copy={}",
                pulled.duration_since(start).as_millis(),
                updated.duration_since(pulled).as_millis(),
                got.duration_since(updated).as_millis(),
                Instant::now().duration_since(got).as_millis()
            );
        }

        &self.freq_buffer
    }

    fn get_freq_buffer_size(&self) -> usize {
        self.freq_buffer.len()
    }
}
