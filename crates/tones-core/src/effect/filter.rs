use std::sync::atomic::{AtomicU32, Ordering};

use crate::graph::AudioNode;

/// Filter type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterType {
    LowPass,
    HighPass,
    BandPass,
}

/// A biquad filter with atomic parameter control.
///
/// Implements a simple 2-pole state-variable filter.
pub struct Filter {
    filter_type: FilterType,
    /// Cutoff frequency in Hz (stored as f32 bits).
    cutoff_bits: AtomicU32,
    /// Resonance Q factor (stored as f32 bits).
    q_bits: AtomicU32,
    /// Wet/dry mix (0.0 = dry, 1.0 = wet).
    wet_bits: AtomicU32,
    // State variables
    low: f32,
    band: f32,
    high: f32,
}

impl Filter {
    pub fn new(filter_type: FilterType, cutoff: f32, q: f32) -> Self {
        Self {
            filter_type,
            cutoff_bits: AtomicU32::new(cutoff.to_bits()),
            q_bits: AtomicU32::new(q.to_bits()),
            wet_bits: AtomicU32::new(1.0f32.to_bits()),
            low: 0.0,
            band: 0.0,
            high: 0.0,
        }
    }

    pub fn set_cutoff(&self, freq: f32) {
        self.cutoff_bits.store(freq.to_bits(), Ordering::Relaxed);
    }

    pub fn set_q(&self, q: f32) {
        self.q_bits.store(q.to_bits(), Ordering::Relaxed);
    }

    pub fn set_wet(&self, wet: f32) {
        self.wet_bits.store(wet.to_bits(), Ordering::Relaxed);
    }

    fn cutoff(&self) -> f32 {
        f32::from_bits(self.cutoff_bits.load(Ordering::Relaxed))
    }

    fn q(&self) -> f32 {
        f32::from_bits(self.q_bits.load(Ordering::Relaxed))
    }

    fn wet(&self) -> f32 {
        f32::from_bits(self.wet_bits.load(Ordering::Relaxed))
    }
}

impl AudioNode for Filter {
    fn process(&mut self, input: &[f32], output: &mut [f32], sample_rate: u32) {
        let cutoff = self.cutoff();
        let q = self.q().max(0.5);
        let wet = self.wet().clamp(0.0, 1.0);

        // State-variable filter coefficients
        let f = 2.0 * std::f32::consts::PI * cutoff / sample_rate as f32;
        let f = f.sin().min(0.99); // clamp for stability
        let damp = 1.0 / q;

        for (i, out) in output.iter_mut().enumerate() {
            let dry = if i < input.len() { input[i] } else { 0.0 };

            // State-variable filter update
            self.low += f * self.band;
            self.high = dry - self.low - damp * self.band;
            self.band += f * self.high;

            let filtered = match self.filter_type {
                FilterType::LowPass => self.low,
                FilterType::HighPass => self.high,
                FilterType::BandPass => self.band,
            };

            *out = dry * (1.0 - wet) + filtered * wet;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lowpass_attenuates_high_freq() {
        let mut filter = Filter::new(FilterType::LowPass, 200.0, 1.0);
        let sr = 44100;

        // Generate a high frequency sine (5000Hz)
        let len = 1024;
        let input: Vec<f32> = (0..len)
            .map(|i| (2.0 * std::f32::consts::PI * 5000.0 * i as f32 / sr as f32).sin())
            .collect();
        let mut output = vec![0.0f32; len];

        filter.process(&input, &mut output, sr);

        // The output should be significantly attenuated
        let input_rms: f32 = (input.iter().map(|x| x * x).sum::<f32>() / len as f32).sqrt();
        let output_rms: f32 = (output[512..].iter().map(|x| x * x).sum::<f32>() / 512.0).sqrt();

        assert!(
            output_rms < input_rms * 0.3,
            "lowpass should attenuate 5kHz: input_rms={input_rms}, output_rms={output_rms}"
        );
    }

    #[test]
    fn test_wet_dry_mix() {
        let mut filter = Filter::new(FilterType::LowPass, 200.0, 1.0);
        filter.set_wet(0.0); // fully dry

        let input = [1.0f32; 64];
        let mut output = [0.0f32; 64];
        filter.process(&input, &mut output, 44100);

        // With wet=0, output should equal input
        for (i, &s) in output.iter().enumerate() {
            assert!(
                (s - 1.0).abs() < 0.01,
                "dry output should pass through: sample {i} = {s}"
            );
        }
    }
}
