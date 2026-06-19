use crate::{AudioSource, Frequency};
use audioviz::io::{Input, InputController};
use audioviz::spectrum::{config::StreamConfig, stream::Stream};

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
    fn get_frequencies(&mut self, _now: usize) -> (&[Frequency], bool) {
        let mut have_new_block = false;
        if let Some(block) = self.controller.try_pull_data() {
            if !block.is_empty() {
                self.stream.push_data(block);
                self.stream.update();
                have_new_block = true;
            }
        }

        if !have_new_block {
            return (&self.freq_buffer, false);
        }

        let frequencies = self.stream.get_frequencies();
        if frequencies.is_empty() {
            return (&self.freq_buffer, false);
        }

        let mut frequencies = frequencies[0].clone();
        if frequencies.is_empty() {
            return (&self.freq_buffer, false);
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

        (&self.freq_buffer, true)
    }

    fn get_freq_buffer_size(&self) -> usize {
        self.freq_buffer.len()
    }
}
