use crate::{noise, AudioSource, Frequency};

#[derive(Clone)]
pub struct AudioSourceNoise {
    samples: Vec<f32>,
    sample_rate: u32,
    chunk_size: usize,
    length_millis: usize,
}

impl AudioSource for AudioSourceNoise {
    fn get_frequencies(&mut self, now: usize) -> Vec<Frequency> {
        self.get_frequencies_at_time(now)
    }
}

impl AudioSourceNoise {
    pub fn new(sample_rate: u32, length_millis: usize) -> Self {
        Self {
            samples: vec![],
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
    pub fn get_frequencies_at_time(&self, time_millis: usize) -> Vec<Frequency> {
        let mut base = vec![];

        for i in 0..200 {
            base.push(Frequency {
                volume: 100.0,
                freq: (i * 100) as f32,
                position: 0.0,
            });
        }

        base
    }
}
