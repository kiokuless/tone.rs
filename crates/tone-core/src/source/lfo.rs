//! Low Frequency Oscillator for parameter modulation.
//!
//! The LFO generates a control signal mapped to a configurable `min..max`
//! range. It reuses the waveform generation logic from [`super::oscillator`].

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use crate::graph::AudioNode;
use crate::source::oscillator::{OscillatorType, sample_waveform};

/// A low-frequency oscillator that outputs a control signal in a
/// configurable `min..max` range.
///
/// # Signal flow
///
/// ```text
/// waveform(-1..1) → × amplitude → map to min..max → output
/// ```
///
/// When stopped, the LFO outputs the value corresponding to its initial
/// phase offset (matching Tone.js behaviour).
pub struct Lfo {
    waveform: OscillatorType,
    /// Frequency in Hz (atomic for thread-safe control).
    frequency_bits: AtomicU32,
    /// Minimum output value.
    min_bits: AtomicU32,
    /// Maximum output value.
    max_bits: AtomicU32,
    /// Amplitude scaling (0.0–1.0). Controls how much of the min–max range
    /// the oscillation actually covers.
    amplitude_bits: AtomicU32,
    /// Whether the LFO is running.
    running: AtomicBool,
    /// Phase accumulator (0.0 .. 1.0), audio-thread only.
    phase: f32,
    /// Initial phase offset (0.0 .. 1.0). `start()` resets `phase` to this.
    phase_offset: f32,
}

impl Lfo {
    /// Create a new LFO.
    ///
    /// * `waveform` – oscillator shape
    /// * `frequency` – rate in Hz
    /// * `min` / `max` – output range
    pub fn new(waveform: OscillatorType, frequency: f32, min: f32, max: f32) -> Self {
        Self {
            waveform,
            frequency_bits: AtomicU32::new(frequency.to_bits()),
            min_bits: AtomicU32::new(min.to_bits()),
            max_bits: AtomicU32::new(max.to_bits()),
            amplitude_bits: AtomicU32::new(1.0f32.to_bits()),
            running: AtomicBool::new(false),
            phase: 0.0,
            phase_offset: 0.0,
        }
    }

    // -- control --------------------------------------------------------------

    /// Start the LFO. Resets the phase to `phase_offset`.
    pub fn start(&mut self) {
        self.phase = self.phase_offset;
        self.running.store(true, Ordering::Relaxed);
    }

    /// Stop the LFO. Output will hold the stopped value.
    pub fn stop(&self) {
        self.running.store(false, Ordering::Relaxed);
    }

    /// Whether the LFO is currently running.
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::Relaxed)
    }

    // -- parameter accessors (all thread-safe) --------------------------------

    pub fn frequency(&self) -> f32 {
        f32::from_bits(self.frequency_bits.load(Ordering::Relaxed))
    }

    pub fn set_frequency(&self, hz: f32) {
        self.frequency_bits.store(hz.to_bits(), Ordering::Relaxed);
    }

    pub fn min(&self) -> f32 {
        f32::from_bits(self.min_bits.load(Ordering::Relaxed))
    }

    pub fn set_min(&self, v: f32) {
        self.min_bits.store(v.to_bits(), Ordering::Relaxed);
    }

    pub fn max(&self) -> f32 {
        f32::from_bits(self.max_bits.load(Ordering::Relaxed))
    }

    pub fn set_max(&self, v: f32) {
        self.max_bits.store(v.to_bits(), Ordering::Relaxed);
    }

    pub fn amplitude(&self) -> f32 {
        f32::from_bits(self.amplitude_bits.load(Ordering::Relaxed))
    }

    pub fn set_amplitude(&self, v: f32) {
        self.amplitude_bits
            .store(v.clamp(0.0, 1.0).to_bits(), Ordering::Relaxed);
    }

    pub fn set_waveform(&mut self, waveform: OscillatorType) {
        self.waveform = waveform;
    }

    /// Set the initial phase offset (0.0 .. 1.0).
    pub fn set_phase_offset(&mut self, phase: f32) {
        self.phase_offset = phase.rem_euclid(1.0);
    }

    // -- internal -------------------------------------------------------------

    /// Map a bipolar value (-1..1) to the output range, scaled by amplitude.
    ///
    /// `center ± bipolar * amplitude * half_range`
    #[inline]
    fn map_bipolar(&self, bipolar: f32) -> f32 {
        let min = self.min();
        let max = self.max();
        let amp = self.amplitude();
        let center = (min + max) * 0.5;
        let half_range = (max - min) * 0.5;
        center + bipolar * amp * half_range
    }

    /// Value output when the LFO is stopped (waveform at initial phase).
    #[inline]
    fn stopped_value(&self) -> f32 {
        self.map_bipolar(sample_waveform(self.waveform, self.phase_offset))
    }
}

impl AudioNode for Lfo {
    fn process(&mut self, _input: &[f32], output: &mut [f32], sample_rate: u32) {
        if !self.running.load(Ordering::Relaxed) {
            output.fill(self.stopped_value());
            return;
        }

        let phase_inc = self.frequency() / sample_rate as f32;

        for sample in output.iter_mut() {
            let bipolar = sample_waveform(self.waveform, self.phase);
            *sample = self.map_bipolar(bipolar);

            self.phase += phase_inc;
            if self.phase >= 1.0 {
                self.phase -= self.phase.floor();
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lfo_sine_range() {
        let mut lfo = Lfo::new(OscillatorType::Sine, 1.0, 0.0, 1.0);
        lfo.start();

        // Process one full cycle at 64 Hz sample rate (64 samples = 1 cycle at 1 Hz)
        let mut output = [0.0f32; 64];
        lfo.process(&[], &mut output, 64);

        let min = output.iter().fold(f32::INFINITY, |a, &b| a.min(b));
        let max = output.iter().fold(f32::NEG_INFINITY, |a, &b| a.max(b));

        // Sine should reach close to 0.0 and 1.0
        assert!(min < 0.05, "min should be near 0.0, got {min}");
        assert!(max > 0.95, "max should be near 1.0, got {max}");
    }

    #[test]
    fn test_lfo_custom_range() {
        let mut lfo = Lfo::new(OscillatorType::Sine, 1.0, -10.0, 30.0);
        lfo.start();

        let mut output = [0.0f32; 256];
        lfo.process(&[], &mut output, 256);

        let min = output.iter().fold(f32::INFINITY, |a, &b| a.min(b));
        let max = output.iter().fold(f32::NEG_INFINITY, |a, &b| a.max(b));

        assert!(min > -10.1, "below range: {min}");
        assert!(max < 30.1, "above range: {max}");
    }

    #[test]
    fn test_lfo_amplitude_zero_outputs_center() {
        let mut lfo = Lfo::new(OscillatorType::Sine, 1.0, -10.0, 30.0);
        lfo.set_amplitude(0.0);
        lfo.start();

        let mut output = [0.0f32; 64];
        lfo.process(&[], &mut output, 64);

        let center = (-10.0 + 30.0) * 0.5; // 10.0
        for &s in &output {
            assert!(
                (s - center).abs() < 1e-6,
                "expected center {center}, got {s}"
            );
        }
    }

    #[test]
    fn test_lfo_min_equals_max() {
        let mut lfo = Lfo::new(OscillatorType::Sine, 1.0, 5.0, 5.0);
        lfo.start();

        let mut output = [0.0f32; 64];
        lfo.process(&[], &mut output, 64);

        for &s in &output {
            assert!((s - 5.0).abs() < 1e-6, "expected 5.0, got {s}");
        }
    }

    #[test]
    fn test_lfo_stopped_outputs_initial_value() {
        let mut lfo = Lfo::new(OscillatorType::Sine, 2.0, 0.0, 1.0);
        // Don't start — should output stopped value at phase_offset=0.0
        // sine(0) = 0 → mapped to center (0.5)
        let mut output = [0.0f32; 16];
        lfo.process(&[], &mut output, 64);

        let expected = 0.5; // center + 0.0 * half_range
        for &s in &output {
            assert!((s - expected).abs() < 1e-6, "expected {expected}, got {s}");
        }
    }

    #[test]
    fn test_lfo_stopped_with_phase_offset() {
        let mut lfo = Lfo::new(OscillatorType::Sine, 2.0, 0.0, 1.0);
        lfo.set_phase_offset(0.25); // sine(0.25) = sin(π/2) = 1.0 → mapped to 1.0

        let mut output = [0.0f32; 16];
        lfo.process(&[], &mut output, 64);

        let expected = 1.0;
        for &s in &output {
            assert!((s - expected).abs() < 1e-5, "expected {expected}, got {s}");
        }
    }

    #[test]
    fn test_lfo_start_resets_phase() {
        let mut lfo = Lfo::new(OscillatorType::Sawtooth, 1.0, 0.0, 1.0);
        lfo.set_phase_offset(0.5);
        lfo.start();

        // First sample should be at phase 0.5 → sawtooth = 2*0.5-1 = 0.0 → mapped to 0.5
        let mut output = [0.0f32; 1];
        lfo.process(&[], &mut output, 44100);

        assert!(
            (output[0] - 0.5).abs() < 0.01,
            "expected ~0.5, got {}",
            output[0]
        );
    }

    #[test]
    fn test_lfo_phase_continuity_across_buffers() {
        let mut lfo = Lfo::new(OscillatorType::Sine, 1.0, 0.0, 1.0);
        lfo.start();

        let mut buf1 = [0.0f32; 32];
        let mut buf2 = [0.0f32; 32];
        lfo.process(&[], &mut buf1, 64);
        lfo.process(&[], &mut buf2, 64);

        // Concatenated should be smooth — check that buf2[0] follows buf1[31]
        let diff = (buf2[0] - buf1[31]).abs();
        // At 1 Hz / 64 Hz sample rate, each sample moves 1/64 of a cycle.
        // Adjacent samples should differ by at most ~0.1 for sine.
        assert!(diff < 0.15, "phase discontinuity: diff={diff}");
    }

    #[test]
    fn test_lfo_square_wave() {
        let mut lfo = Lfo::new(OscillatorType::Square, 1.0, 0.0, 10.0);
        lfo.start();

        let mut output = [0.0f32; 100];
        lfo.process(&[], &mut output, 100);

        // Square wave should only produce min (0.0) or max (10.0)
        for &s in &output {
            assert!(
                (s - 0.0).abs() < 1e-6 || (s - 10.0).abs() < 1e-6,
                "expected 0.0 or 10.0, got {s}"
            );
        }
    }

    #[test]
    fn test_lfo_setters() {
        let lfo = Lfo::new(OscillatorType::Sine, 5.0, 0.0, 1.0);
        assert_eq!(lfo.frequency(), 5.0);

        lfo.set_frequency(10.0);
        assert_eq!(lfo.frequency(), 10.0);

        lfo.set_min(-1.0);
        assert_eq!(lfo.min(), -1.0);

        lfo.set_max(2.0);
        assert_eq!(lfo.max(), 2.0);

        lfo.set_amplitude(0.5);
        assert_eq!(lfo.amplitude(), 0.5);
    }
}
