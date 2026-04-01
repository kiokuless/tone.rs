use std::f32::consts::PI;
use std::sync::atomic::{AtomicU32, Ordering};

use crate::graph::AudioNode;

/// Oscillator waveform type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OscillatorType {
    Sine,
    Square,
    Sawtooth,
    Triangle,
}

/// Generate a single sample for the given waveform and phase (0.0 .. 1.0).
///
/// Returns a value in the range -1.0 .. 1.0. Shared by [`Oscillator`] and
/// [`super::lfo::Lfo`].
#[inline]
pub fn sample_waveform(waveform: OscillatorType, phase: f32) -> f32 {
    match waveform {
        OscillatorType::Sine => (phase * 2.0 * PI).sin(),
        OscillatorType::Square => {
            if phase < 0.5 {
                1.0
            } else {
                -1.0
            }
        }
        OscillatorType::Sawtooth => 2.0 * phase - 1.0,
        OscillatorType::Triangle => {
            if phase < 0.5 {
                4.0 * phase - 1.0
            } else {
                -4.0 * phase + 3.0
            }
        }
    }
}

/// A basic oscillator that generates periodic waveforms.
///
/// Frequency can be changed atomically from any thread.
pub struct Oscillator {
    waveform: OscillatorType,
    /// Frequency in Hz, stored as bits for atomic access.
    frequency_bits: AtomicU32,
    /// Phase accumulator (0.0 .. 1.0), only accessed from audio thread.
    phase: f32,
}

impl Oscillator {
    pub fn new(waveform: OscillatorType, frequency: f32) -> Self {
        Self {
            waveform,
            frequency_bits: AtomicU32::new(frequency.to_bits()),
            phase: 0.0,
        }
    }

    /// Get the current frequency.
    pub fn frequency(&self) -> f32 {
        f32::from_bits(self.frequency_bits.load(Ordering::Relaxed))
    }

    /// Set the frequency (thread-safe).
    pub fn set_frequency(&self, freq: f32) {
        self.frequency_bits.store(freq.to_bits(), Ordering::Relaxed);
    }

    /// Set the waveform type.
    /// Note: not thread-safe, call before starting or use with care.
    pub fn set_waveform(&mut self, waveform: OscillatorType) {
        self.waveform = waveform;
    }

    /// Generate a sample for the given phase (0.0 .. 1.0).
    #[inline]
    fn generate(&self, phase: f32) -> f32 {
        sample_waveform(self.waveform, phase)
    }
}

impl AudioNode for Oscillator {
    fn process(&mut self, _input: &[f32], output: &mut [f32], sample_rate: u32) {
        let freq = self.frequency();
        let phase_inc = freq / sample_rate as f32;

        for sample in output.iter_mut() {
            *sample = self.generate(self.phase);
            self.phase += phase_inc;
            if self.phase >= 1.0 {
                self.phase -= 1.0;
            }
        }
    }
}
