use std::sync::atomic::{AtomicU32, Ordering};

use crate::graph::AudioNode;

/// A simple waveshaping distortion effect.
pub struct Distortion {
    /// Drive amount (1.0 = clean, higher = more distortion).
    drive_bits: AtomicU32,
    /// Wet/dry mix.
    wet_bits: AtomicU32,
}

impl Distortion {
    pub fn new(drive: f32) -> Self {
        Self {
            drive_bits: AtomicU32::new(drive.max(1.0).to_bits()),
            wet_bits: AtomicU32::new(1.0f32.to_bits()),
        }
    }

    pub fn set_drive(&self, drive: f32) {
        self.drive_bits
            .store(drive.max(1.0).to_bits(), Ordering::Relaxed);
    }

    pub fn set_wet(&self, wet: f32) {
        self.wet_bits
            .store(wet.clamp(0.0, 1.0).to_bits(), Ordering::Relaxed);
    }

    fn drive(&self) -> f32 {
        f32::from_bits(self.drive_bits.load(Ordering::Relaxed))
    }

    fn wet(&self) -> f32 {
        f32::from_bits(self.wet_bits.load(Ordering::Relaxed))
    }
}

impl AudioNode for Distortion {
    fn process(&mut self, input: &[f32], output: &mut [f32], _sample_rate: u32) {
        let drive = self.drive();
        let wet = self.wet();

        for (i, out) in output.iter_mut().enumerate() {
            let dry = if i < input.len() { input[i] } else { 0.0 };

            // Soft clipping via tanh waveshaping
            let distorted = (dry * drive).tanh();

            *out = dry * (1.0 - wet) + distorted * wet;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_distortion_clips() {
        let mut dist = Distortion::new(10.0);
        let input = [0.5f32; 64];
        let mut output = [0.0f32; 64];

        dist.process(&input, &mut output, 44100);

        // With high drive, tanh(0.5 * 10) ≈ 1.0
        for &s in &output {
            assert!(s > 0.9 && s <= 1.0, "distorted sample = {s}");
        }
    }

    #[test]
    fn test_distortion_clean_at_drive_1() {
        let mut dist = Distortion::new(1.0);
        let input = [0.3f32; 64];
        let mut output = [0.0f32; 64];

        dist.process(&input, &mut output, 44100);

        // tanh(0.3) ≈ 0.291 — close to original
        for &s in &output {
            assert!((s - 0.3).abs() < 0.02, "clean sample = {s}");
        }
    }
}
