use crate::{AudioSource, Frequency};
use rand::Rng; // Import the trait to use .gen_range()

#[derive(Clone)]
pub struct AudioSourceNoise {
    freq_buffer: Vec<Frequency>,
    sample_rate: u32,
    chunk_size: usize,
    length_millis: usize,
}

impl AudioSource for AudioSourceNoise {
    fn get_frequencies(&mut self, now: usize) -> &[Frequency] {
        self.get_frequencies_at_time(now)
    }

    fn get_freq_buffer_size(&self) -> usize {
        self.freq_buffer.len()
    }
}

impl AudioSourceNoise {
    pub fn new(sample_rate: u32, length_millis: usize, num_samples: usize) -> Self {
        let upper_freq_range = 20000;
        let freq_step = upper_freq_range / num_samples;

        let samples = (0..num_samples)
            .map(|i| Frequency {
                volume: 0.0,
                freq: (freq_step * i) as f32,
                position: i as f32,
            })
            .collect();

        Self {
            freq_buffer: samples,
            sample_rate,
            chunk_size: 10,
            length_millis,
        }
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
        let mut rng = rand::rng(); // Create a local random generator

        self.freq_buffer.iter_mut().for_each(|freq| {
            freq.volume = rng.random_range(0.0..50.0);
        });

        &self.freq_buffer
    }
}
