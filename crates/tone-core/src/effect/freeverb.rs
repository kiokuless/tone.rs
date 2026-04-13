//! Freeverb reverb algorithm.
//!
//! 8 parallel feedback comb filters with one-pole lowpass dampening,
//! followed by 4 series allpass filters for diffusion.

use std::sync::atomic::{AtomicU32, Ordering};

use crate::graph::AudioNode;

// Standard Freeverb delay lengths at 44100 Hz.
const COMB_TUNINGS: [usize; 8] = [1557, 1617, 1491, 1422, 1277, 1356, 1188, 1116];
const ALLPASS_TUNINGS: [usize; 4] = [556, 441, 341, 225];

fn scale_tuning(base: usize, sample_rate: u32) -> usize {
    ((base as f64) * (sample_rate as f64) / 44100.0).round() as usize
}

// ---------------------------------------------------------------------------
// CombFilter (private)
// ---------------------------------------------------------------------------

struct CombFilter {
    buffer: Vec<f32>,
    index: usize,
    filter_store: f32,
}

impl CombFilter {
    fn new(delay_samples: usize) -> Self {
        Self {
            buffer: vec![0.0; delay_samples.max(1)],
            index: 0,
            filter_store: 0.0,
        }
    }

    #[inline]
    fn process(&mut self, input: f32, feedback: f32, damp1: f32, damp2: f32) -> f32 {
        let output = self.buffer[self.index];
        self.filter_store = output * damp1 + self.filter_store * damp2;
        self.buffer[self.index] = input + feedback * self.filter_store;
        self.index += 1;
        if self.index >= self.buffer.len() {
            self.index = 0;
        }
        output
    }
}

// ---------------------------------------------------------------------------
// AllpassFilter (private)
// ---------------------------------------------------------------------------

struct AllpassFilter {
    buffer: Vec<f32>,
    index: usize,
}

impl AllpassFilter {
    fn new(delay_samples: usize) -> Self {
        Self {
            buffer: vec![0.0; delay_samples.max(1)],
            index: 0,
        }
    }

    #[inline]
    fn process(&mut self, input: f32) -> f32 {
        let buffered = self.buffer[self.index];
        let output = -input + buffered;
        self.buffer[self.index] = input + 0.5 * buffered;
        self.index += 1;
        if self.index >= self.buffer.len() {
            self.index = 0;
        }
        output
    }
}

// ---------------------------------------------------------------------------
// Freeverb (public)
// ---------------------------------------------------------------------------

/// Freeverb reverb effect.
///
/// Classic Schroeder reverberator with 8 parallel comb filters and 4 series
/// allpass filters. All buffers are pre-allocated at construction time.
pub struct Freeverb {
    combs: [CombFilter; 8],
    allpasses: [AllpassFilter; 4],
    room_size_bits: AtomicU32,
    dampening_bits: AtomicU32,
    wet_bits: AtomicU32,
}

impl Freeverb {
    /// Create a new Freeverb with delay lines scaled to the given sample rate.
    pub fn new(sample_rate: u32) -> Self {
        let combs =
            std::array::from_fn(|i| CombFilter::new(scale_tuning(COMB_TUNINGS[i], sample_rate)));
        let allpasses = std::array::from_fn(|i| {
            AllpassFilter::new(scale_tuning(ALLPASS_TUNINGS[i], sample_rate))
        });

        Self {
            combs,
            allpasses,
            room_size_bits: AtomicU32::new(0.7f32.to_bits()),
            dampening_bits: AtomicU32::new(0.5f32.to_bits()),
            wet_bits: AtomicU32::new(1.0f32.to_bits()),
        }
    }

    pub fn room_size(&self) -> f32 {
        f32::from_bits(self.room_size_bits.load(Ordering::Relaxed))
    }
    pub fn set_room_size(&self, v: f32) {
        self.room_size_bits
            .store(v.clamp(0.0, 1.0).to_bits(), Ordering::Relaxed);
    }

    pub fn dampening(&self) -> f32 {
        f32::from_bits(self.dampening_bits.load(Ordering::Relaxed))
    }
    pub fn set_dampening(&self, v: f32) {
        self.dampening_bits
            .store(v.clamp(0.0, 1.0).to_bits(), Ordering::Relaxed);
    }

    pub fn wet(&self) -> f32 {
        f32::from_bits(self.wet_bits.load(Ordering::Relaxed))
    }
    pub fn set_wet(&self, v: f32) {
        self.wet_bits
            .store(v.clamp(0.0, 1.0).to_bits(), Ordering::Relaxed);
    }
}

impl AudioNode for Freeverb {
    fn process(&mut self, input: &[f32], output: &mut [f32], _sample_rate: u32) {
        let room_size = self.room_size();
        let dampening = self.dampening();
        let wet = self.wet();
        let dry = 1.0 - wet;

        let feedback = room_size * 0.28 + 0.7;
        let damp1 = dampening;
        let damp2 = 1.0 - dampening;

        for (i, out) in output.iter_mut().enumerate() {
            let inp = if i < input.len() { input[i] } else { 0.0 };

            // Sum 8 parallel comb filters
            let mut comb_sum = 0.0f32;
            for comb in &mut self.combs {
                comb_sum += comb.process(inp, feedback, damp1, damp2);
            }
            comb_sum /= 8.0;

            // 4 series allpass filters
            let mut sample = comb_sum;
            for ap in &mut self.allpasses {
                sample = ap.process(sample);
            }

            *out = inp * dry + sample * wet;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_reverb_tail() {
        let mut reverb = Freeverb::new(44100);
        // Feed an impulse
        let input = {
            let mut v = vec![0.0f32; 256];
            v[0] = 1.0;
            v
        };
        let mut output = vec![0.0f32; 256];
        reverb.process(&input, &mut output, 44100);

        // Process more silence — reverb tail should still have energy
        let silence = vec![0.0f32; 4096];
        let mut tail = vec![0.0f32; 4096];
        reverb.process(&silence, &mut tail, 44100);

        let energy: f32 = tail.iter().map(|s| s * s).sum();
        assert!(energy > 0.001, "reverb tail should have energy: {energy}");
    }

    #[test]
    fn test_wet_zero_passthrough() {
        let mut reverb = Freeverb::new(44100);
        reverb.set_wet(0.0);

        let input: Vec<f32> = (0..128).map(|i| (i as f32 * 0.1).sin()).collect();
        let mut output = vec![0.0f32; 128];
        reverb.process(&input, &mut output, 44100);

        for (inp, out) in input.iter().zip(output.iter()) {
            assert!(
                (inp - out).abs() < 1e-6,
                "wet=0 should pass through: {inp} vs {out}"
            );
        }
    }

    #[test]
    fn test_room_size_affects_tail() {
        // Larger room_size → more reverb energy
        let mut small = Freeverb::new(44100);
        small.set_room_size(0.2);
        let mut large = Freeverb::new(44100);
        large.set_room_size(0.9);

        let impulse = {
            let mut v = vec![0.0f32; 64];
            v[0] = 1.0;
            v
        };
        let mut out_s = vec![0.0f32; 64];
        let mut out_l = vec![0.0f32; 64];
        small.process(&impulse, &mut out_s, 44100);
        large.process(&impulse, &mut out_l, 44100);

        // Process tail
        let silence = vec![0.0f32; 8192];
        let mut tail_s = vec![0.0f32; 8192];
        let mut tail_l = vec![0.0f32; 8192];
        small.process(&silence, &mut tail_s, 44100);
        large.process(&silence, &mut tail_l, 44100);

        let energy_s: f32 = tail_s.iter().map(|s| s * s).sum();
        let energy_l: f32 = tail_l.iter().map(|s| s * s).sum();
        assert!(
            energy_l > energy_s,
            "larger room should have more tail energy: {energy_l} vs {energy_s}"
        );
    }

    #[test]
    fn test_setters() {
        let reverb = Freeverb::new(44100);
        reverb.set_room_size(0.5);
        assert_eq!(reverb.room_size(), 0.5);
        reverb.set_dampening(0.3);
        assert_eq!(reverb.dampening(), 0.3);
        reverb.set_wet(0.8);
        assert_eq!(reverb.wet(), 0.8);
    }
}
