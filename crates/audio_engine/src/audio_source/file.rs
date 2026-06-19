use crate::{AudioSource, Frequency};
use audioviz::spectrum::{config::ProcessorConfig, processor::Processor};
use std::fs::File;
use std::ops::Range;
use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::DecoderOptions;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

#[derive(Clone)]
pub struct AudioSourceSoundfile {
    freq_buffer: Vec<Frequency>,
    samples: Vec<f32>,
    sample_rate: u32,
    chunk_size: usize,
    length_millis: usize,
}

impl AudioSource for AudioSourceSoundfile {
    fn get_frequencies(&mut self, now: usize) -> (&[Frequency], bool) {
        let freqs = self.get_frequencies_at_time(now);
        let has_new = !freqs.is_empty();
        (freqs, has_new)
    }

    fn get_freq_buffer_size(&self) -> usize {
        self.freq_buffer.len()
    }
}

impl AudioSourceSoundfile {
    pub fn new(
        file_path: &str,
        freq_buffer_len: usize,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        // Open the audio file
        let file = File::open(file_path)?;
        let mss = MediaSourceStream::new(Box::new(file), Default::default());

        // Create a hint to help the format registry
        let mut hint = Hint::new();
        if let Some(ext) = file_path.split('.').last() {
            hint.with_extension(ext);
        }

        // Probe the file to get format information
        let probed = symphonia::default::get_probe().format(
            &hint,
            mss,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )?;

        let mut format = probed.format;
        let track = format.tracks().first().ok_or("No audio track found")?;
        let track_id = track.id;
        let sample_rate = track.codec_params.sample_rate.unwrap_or(44100);

        // Create a decoder
        let mut decoder = symphonia::default::get_codecs()
            .make(&track.codec_params, &DecoderOptions::default())?;

        let mut samples = Vec::new();

        // Decode all packets
        while let Ok(packet) = format.next_packet() {
            if packet.track_id() != track_id {
                continue;
            }

            match decoder.decode(&packet) {
                Ok(decoded) => {
                    let spec = *decoded.spec();
                    let duration = decoded.capacity() as u64;
                    let mut sample_buf = SampleBuffer::<f32>::new(duration, spec);
                    sample_buf.copy_interleaved_ref(decoded);

                    // WHAT THE FUCK
                    // Take only one channel for simplicity (left channel)
                    for i in (0..sample_buf.samples().len()).step_by(spec.channels.count()) {
                        samples.push(sample_buf.samples()[i]);
                    }
                }
                Err(_) => break,
            }
        }

        // println!("Decoded {} samples at {} Hz", samples.len(), sample_rate);

        let length_millis = (samples.len() as f32 / sample_rate as f32 * 1000.0) as usize;

        Ok(Self {
            freq_buffer: vec![Frequency::default(); freq_buffer_len],
            samples,
            sample_rate,
            chunk_size: 4096,
            length_millis,
        })
    }

    /// Get the total duration of the audio in millis
    pub fn duration(&self) -> usize {
        self.length_millis
    }

    /// Return the sample rate of the decoded audio stream.
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Get frequencies at a specific time in seconds
    pub fn get_frequencies_at_time(&mut self, time_millis: usize) -> &[Frequency] {
        let sample_index = (time_millis as f32 / 1000.0 * self.sample_rate as f32) as usize;

        if sample_index >= self.samples.len() {
            return &[];
        }

        // Get chunk centered around the requested time
        let start = sample_index.saturating_sub(self.chunk_size / 2);
        let end = (start + self.chunk_size).min(self.samples.len());

        if end - start < self.chunk_size {
            return &[];
        }

        let range = start..end;
        self.compute_frequencies(range)
    }

    /// Get frequencies for a range of time in seconds
    pub fn get_frequencies_range(&mut self, start_time: f32, end_time: f32) -> &[Frequency] {
        let start_sample = (start_time * self.sample_rate as f32) as usize;
        let end_sample = (end_time * self.sample_rate as f32) as usize;

        if start_sample >= self.samples.len() || end_sample > self.samples.len() {
            return &[];
        }

        let range = start_sample..end_sample;
        if range.len() < 512 {
            return &[];
        }

        self.compute_frequencies(range)
    }

    /// Get current frequencies (returns frequencies for the entire loaded audio)
    pub fn get_frequencies(&mut self) -> &[Frequency] {
        if self.samples.len() < self.chunk_size {
            return &[];
        }

        let range = 0..self.chunk_size;
        self.compute_frequencies(range)
    }

    /// Set the FFT chunk size (must be power of 2)
    pub fn set_chunk_size(&mut self, size: usize) {
        if size.is_power_of_two() {
            self.chunk_size = size;
        }
    }

    fn compute_frequencies(&mut self, chunk_range: Range<usize>) -> &[Frequency] {
        let samples = &self.samples[chunk_range];

        if samples.is_empty() {
            return &[];
        }

        {
            let mut processor = Processor::from_raw_data(
                ProcessorConfig {
                    sampling_rate: self.sample_rate,
                    resolution: Some(samples.len() / 2),
                    ..ProcessorConfig::default()
                },
                samples.to_vec(),
            );

            processor.compute_all();

            debug_assert!(
                processor.freq_buffer.len() == self.freq_buffer.len(),
                "Soundfile buffer is {} but got {} from processor",
                self.freq_buffer.len(),
                processor.freq_buffer.len()
            );

            for (i, f) in processor.freq_buffer.iter().enumerate() {
                self.freq_buffer[i] = Frequency::from(f);
            }

            &self.freq_buffer

            // processor
            //     .freq_buffer
            //     .iter()
            //     .map(|f| Frequency::from(f))
            //     .collect()
        }
    }
}
