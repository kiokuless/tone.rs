//! Dynamic range compressor.

use std::sync::atomic::{AtomicU32, Ordering};

use crate::graph::AudioNode;

/// Feed-forward dynamic range compressor with soft-knee.
pub struct Compressor {
    threshold_bits: AtomicU32,
    ratio_bits: AtomicU32,
    knee_bits: AtomicU32,
    attack_bits: AtomicU32,
    release_bits: AtomicU32,
    /// Envelope state (audio-thread only).
    envelope: f32,
}

impl Compressor {
    pub fn new() -> Self {
        Self {
            threshold_bits: AtomicU32::new((-24.0f32).to_bits()),
            ratio_bits: AtomicU32::new(12.0f32.to_bits()),
            knee_bits: AtomicU32::new(30.0f32.to_bits()),
            attack_bits: AtomicU32::new(0.003f32.to_bits()),
            release_bits: AtomicU32::new(0.25f32.to_bits()),
            envelope: 0.0,
        }
    }

    // -- getters/setters ------------------------------------------------------

    pub fn threshold(&self) -> f32 {
        f32::from_bits(self.threshold_bits.load(Ordering::Relaxed))
    }
    pub fn set_threshold(&self, v: f32) {
        self.threshold_bits.store(v.to_bits(), Ordering::Relaxed);
    }

    pub fn ratio(&self) -> f32 {
        f32::from_bits(self.ratio_bits.load(Ordering::Relaxed))
    }
    pub fn set_ratio(&self, v: f32) {
        self.ratio_bits
            .store(v.max(1.0).to_bits(), Ordering::Relaxed);
    }

    pub fn knee(&self) -> f32 {
        f32::from_bits(self.knee_bits.load(Ordering::Relaxed))
    }
    pub fn set_knee(&self, v: f32) {
        self.knee_bits
            .store(v.max(0.0).to_bits(), Ordering::Relaxed);
    }

    pub fn attack(&self) -> f32 {
        f32::from_bits(self.attack_bits.load(Ordering::Relaxed))
    }
    pub fn set_attack(&self, v: f32) {
        self.attack_bits
            .store(v.max(0.0).to_bits(), Ordering::Relaxed);
    }

    pub fn release(&self) -> f32 {
        f32::from_bits(self.release_bits.load(Ordering::Relaxed))
    }
    pub fn set_release(&self, v: f32) {
        self.release_bits
            .store(v.max(0.0).to_bits(), Ordering::Relaxed);
    }
}

impl Default for Compressor {
    fn default() -> Self {
        Self::new()
    }
}

impl AudioNode for Compressor {
    fn process(&mut self, input: &[f32], output: &mut [f32], sample_rate: u32) {
        let threshold = self.threshold();
        let ratio = self.ratio();
        let knee = self.knee();
        let attack = self.attack();
        let release = self.release();

        let sr = sample_rate as f32;
        let attack_coeff = (-1.0 / (attack * sr + 1e-10)).exp();
        let release_coeff = (-1.0 / (release * sr + 1e-10)).exp();

        let half_knee = knee * 0.5;
        let below = threshold - half_knee;
        let above = threshold + half_knee;

        for (i, out) in output.iter_mut().enumerate() {
            let inp = if i < input.len() { input[i] } else { 0.0 };
            let input_db = 20.0 * (inp.abs() + 1e-10).log10();

            // Gain reduction (dB, negative or zero)
            let gain_db = if input_db < below {
                0.0
            } else if input_db > above {
                threshold + (input_db - threshold) / ratio - input_db
            } else {
                // Soft knee — quadratic interpolation
                let x = input_db - below;
                let knee_factor = (1.0 / ratio - 1.0) / (2.0 * knee);
                x * x * knee_factor
            };

            // Envelope follower
            let coeff = if gain_db < self.envelope {
                attack_coeff
            } else {
                release_coeff
            };
            self.envelope = coeff * self.envelope + (1.0 - coeff) * gain_db;

            // Apply gain
            *out = inp * 10.0_f32.powf(self.envelope / 20.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_below_threshold_passthrough() {
        let mut comp = Compressor::new();
        comp.set_threshold(-6.0);

        // Very quiet signal (well below threshold)
        let input: Vec<f32> = (0..256).map(|i| (i as f32 * 0.1).sin() * 0.01).collect();
        let mut output = vec![0.0f32; 256];
        comp.process(&input, &mut output, 44100);

        for (inp, out) in input.iter().zip(output.iter()) {
            assert!(
                (inp - out).abs() < 0.001,
                "below threshold should pass through: {inp} vs {out}"
            );
        }
    }

    #[test]
    fn test_compression_reduces_amplitude() {
        let mut comp = Compressor::new();
        comp.set_threshold(-20.0);
        comp.set_ratio(4.0);
        comp.set_knee(0.0);
        comp.set_attack(0.0001);

        // Loud signal
        let input: Vec<f32> = (0..1024).map(|i| (i as f32 * 0.1).sin() * 0.9).collect();
        let mut output = vec![0.0f32; 1024];
        comp.process(&input, &mut output, 44100);

        let input_peak = input.iter().fold(0.0f32, |a, &b| a.max(b.abs()));
        let output_peak = output.iter().fold(0.0f32, |a, &b| a.max(b.abs()));

        assert!(
            output_peak < input_peak,
            "compressed output should be quieter: {output_peak} vs {input_peak}"
        );
    }

    #[test]
    fn test_ratio_one_no_compression() {
        let mut comp = Compressor::new();
        comp.set_ratio(1.0);

        let input: Vec<f32> = (0..256).map(|i| (i as f32 * 0.1).sin() * 0.5).collect();
        let mut output = vec![0.0f32; 256];
        comp.process(&input, &mut output, 44100);

        for (inp, out) in input.iter().zip(output.iter()) {
            assert!(
                (inp - out).abs() < 0.01,
                "ratio=1 should not compress: {inp} vs {out}"
            );
        }
    }

    #[test]
    fn test_setters() {
        let comp = Compressor::new();
        comp.set_threshold(-12.0);
        assert_eq!(comp.threshold(), -12.0);
        comp.set_ratio(8.0);
        assert_eq!(comp.ratio(), 8.0);
        comp.set_knee(10.0);
        assert_eq!(comp.knee(), 10.0);
        comp.set_attack(0.01);
        assert_eq!(comp.attack(), 0.01);
        comp.set_release(0.1);
        assert_eq!(comp.release(), 0.1);
    }
}
