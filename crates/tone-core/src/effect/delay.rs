use std::sync::atomic::{AtomicU32, Ordering};

use crate::graph::AudioNode;

/// A simple feedback delay effect.
///
/// Stores a circular buffer and mixes delayed signal back into the output.
pub struct Delay {
    buffer: Vec<f32>,
    write_pos: usize,
    /// Delay time in seconds (stored as f32 bits).
    delay_time_bits: AtomicU32,
    /// Feedback amount 0.0-1.0 (stored as f32 bits).
    feedback_bits: AtomicU32,
    /// Wet/dry mix (stored as f32 bits).
    wet_bits: AtomicU32,
    sample_rate: u32,
}

impl Delay {
    /// Create a new delay with the given delay time and feedback.
    /// `max_delay` sets the maximum delay time in seconds (determines buffer size).
    pub fn new(delay_time: f32, feedback: f32, sample_rate: u32) -> Self {
        let max_delay = 2.0; // 2 seconds max
        let buf_size = (max_delay * sample_rate as f32) as usize;
        Self {
            buffer: vec![0.0; buf_size],
            write_pos: 0,
            delay_time_bits: AtomicU32::new(delay_time.to_bits()),
            feedback_bits: AtomicU32::new(feedback.to_bits()),
            wet_bits: AtomicU32::new(0.0f32.to_bits()),
            sample_rate,
        }
    }

    pub fn set_delay_time(&self, time: f32) {
        self.delay_time_bits
            .store(time.to_bits(), Ordering::Relaxed);
    }

    pub fn set_feedback(&self, feedback: f32) {
        self.feedback_bits
            .store(feedback.clamp(0.0, 0.95).to_bits(), Ordering::Relaxed);
    }

    pub fn set_wet(&self, wet: f32) {
        self.wet_bits
            .store(wet.clamp(0.0, 1.0).to_bits(), Ordering::Relaxed);
    }

    fn delay_time(&self) -> f32 {
        f32::from_bits(self.delay_time_bits.load(Ordering::Relaxed))
    }

    fn feedback(&self) -> f32 {
        f32::from_bits(self.feedback_bits.load(Ordering::Relaxed))
    }

    fn wet(&self) -> f32 {
        f32::from_bits(self.wet_bits.load(Ordering::Relaxed))
    }
}

impl AudioNode for Delay {
    fn process(&mut self, input: &[f32], output: &mut [f32], _sample_rate: u32) {
        let delay_samples =
            (self.delay_time() * self.sample_rate as f32) as usize;
        let delay_samples = delay_samples.min(self.buffer.len() - 1).max(1);
        let feedback = self.feedback();
        let wet = self.wet();
        let buf_len = self.buffer.len();

        for (i, out) in output.iter_mut().enumerate() {
            let dry = if i < input.len() { input[i] } else { 0.0 };

            // Read from delay buffer
            let read_pos = (self.write_pos + buf_len - delay_samples) % buf_len;
            let delayed = self.buffer[read_pos];

            // Write to delay buffer (input + feedback)
            self.buffer[self.write_pos] = dry + delayed * feedback;

            self.write_pos = (self.write_pos + 1) % buf_len;

            *out = dry * (1.0 - wet) + delayed * wet;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_delay_echo() {
        let sr = 44100;
        let mut delay = Delay::new(0.01, 0.0, sr); // 10ms delay, no feedback
        delay.set_wet(1.0);

        // Send an impulse
        let mut input = vec![0.0f32; 1024];
        input[0] = 1.0;
        let mut output = vec![0.0f32; 1024];

        delay.process(&input, &mut output, sr);

        // The impulse should appear at ~441 samples (10ms at 44100Hz)
        let delay_samples = (0.01 * sr as f32) as usize;
        assert!(
            output[delay_samples].abs() > 0.5,
            "delayed impulse at sample {delay_samples} = {}",
            output[delay_samples]
        );
        // Original position should be silent (wet=1.0)
        assert!(output[0].abs() < 0.01);
    }

    #[test]
    fn test_delay_feedback() {
        let sr = 44100;
        let mut delay = Delay::new(0.01, 0.5, sr); // 10ms, 50% feedback
        delay.set_wet(1.0);

        let mut input = vec![0.0f32; 2048];
        input[0] = 1.0;
        let mut output = vec![0.0f32; 2048];

        delay.process(&input, &mut output, sr);

        let d = (0.01 * sr as f32) as usize;
        // First echo
        assert!(output[d].abs() > 0.5);
        // Second echo (should be ~0.5 of first)
        assert!(output[d * 2].abs() > 0.2);
    }
}
