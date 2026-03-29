use std::sync::atomic::{AtomicU32, Ordering};

use crate::graph::AudioNode;

/// A simple gain (volume) node that multiplies input by a gain factor.
pub struct Gain {
    /// Gain value stored as f32 bits for atomic access.
    gain_bits: AtomicU32,
}

impl Gain {
    pub fn new(gain: f32) -> Self {
        Self {
            gain_bits: AtomicU32::new(gain.to_bits()),
        }
    }

    /// Get the current gain value.
    pub fn gain(&self) -> f32 {
        f32::from_bits(self.gain_bits.load(Ordering::Relaxed))
    }

    /// Set the gain value (thread-safe).
    pub fn set_gain(&self, value: f32) {
        self.gain_bits.store(value.to_bits(), Ordering::Relaxed);
    }
}

impl AudioNode for Gain {
    fn process(&mut self, input: &[f32], output: &mut [f32], _sample_rate: u32) {
        let g = self.gain();
        for (out, &inp) in output.iter_mut().zip(input.iter()) {
            *out = inp * g;
        }
    }
}
