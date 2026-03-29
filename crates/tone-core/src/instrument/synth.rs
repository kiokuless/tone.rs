use crate::component::envelope::AmplitudeEnvelope;
use crate::graph::AudioNode;
use crate::source::oscillator::{Oscillator, OscillatorType};
use crate::time::frequency::note_to_frequency;
use crate::time::notation::parse_time;

/// A basic synthesizer instrument.
///
/// Combines an Oscillator and AmplitudeEnvelope into a single AudioNode.
/// Port of Tone.js Synth.
pub struct Synth {
    oscillator: Oscillator,
    envelope: AmplitudeEnvelope,
    /// Scratch buffer for oscillator output, pre-allocated to avoid RT allocation.
    osc_buffer: Vec<f32>,
    /// BPM used for time notation parsing.
    pub bpm: f64,
}

impl Synth {
    /// Create a new Synth with default ADSR (A=0.02, D=0.1, S=0.6, R=0.3).
    pub fn new() -> Self {
        Self {
            oscillator: Oscillator::new(OscillatorType::Sine, 440.0),
            envelope: AmplitudeEnvelope::new(0.02, 0.1, 0.6, 0.3),
            osc_buffer: Vec::new(),
            bpm: 120.0,
        }
    }

    /// Create a Synth with custom ADSR values.
    pub fn with_adsr(attack: f64, decay: f64, sustain: f64, release: f64) -> Self {
        Self {
            oscillator: Oscillator::new(OscillatorType::Sine, 440.0),
            envelope: AmplitudeEnvelope::new(attack, decay, sustain, release),
            osc_buffer: Vec::new(),
            bpm: 120.0,
        }
    }

    /// Set the oscillator waveform type.
    pub fn set_waveform(&mut self, waveform: OscillatorType) {
        self.oscillator.set_waveform(waveform);
    }

    /// Set the oscillator frequency directly in Hz.
    pub fn set_frequency(&self, freq: f32) {
        self.oscillator.set_frequency(freq);
    }

    /// Trigger the attack phase with a note name (e.g., "C4", "A#3").
    pub fn trigger_attack(&mut self, note: &str, time: f64, velocity: f64) {
        if let Ok(freq) = note_to_frequency(note) {
            self.oscillator.set_frequency(freq as f32);
        }
        self.envelope.trigger_attack(time, velocity);
    }

    /// Trigger the release phase.
    pub fn trigger_release(&mut self, time: f64) {
        self.envelope.trigger_release(time);
    }

    /// Trigger attack and release with a note name and duration string.
    ///
    /// `duration` can be a time notation string like `"8n"`, `"4n"`, `"0.5"`, etc.
    pub fn trigger_attack_release(&mut self, note: &str, duration: &str, time: f64, velocity: f64) {
        if let Ok(freq) = note_to_frequency(note) {
            self.oscillator.set_frequency(freq as f32);
        }
        let dur_secs = parse_time(duration, self.bpm).unwrap_or(0.5);
        self.envelope
            .trigger_attack_release(time, dur_secs, velocity);
    }
}

impl Default for Synth {
    fn default() -> Self {
        Self::new()
    }
}

impl AudioNode for Synth {
    fn process(&mut self, _input: &[f32], output: &mut [f32], sample_rate: u32) {
        let len = output.len();

        // Ensure scratch buffer is large enough (only resizes if needed)
        if self.osc_buffer.len() < len {
            self.osc_buffer.resize(len, 0.0);
        }

        // Generate oscillator into scratch buffer
        let osc_out = &mut self.osc_buffer[..len];
        self.oscillator.process(&[], osc_out, sample_rate);

        // Apply envelope
        self.envelope.process(osc_out, output, sample_rate);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_synth_trigger_attack_release() {
        let mut synth = Synth::new();
        synth.bpm = 120.0;
        synth.trigger_attack_release("A4", "4n", 0.0, 1.0);

        // At 120 BPM, "4n" = 0.5s. Process some audio.
        let mut output = [0.0f32; 256];
        synth.process(&[], &mut output, 44100);

        // Output should not be all zeros (envelope is active)
        let max = output.iter().fold(0.0f32, |a, &b| a.max(b.abs()));
        assert!(max > 0.01, "synth output is silent, max={max}");
    }

    #[test]
    fn test_synth_note_frequency() {
        let mut synth = Synth::new();
        synth.trigger_attack("C4", 0.0, 1.0);
        assert!((synth.oscillator.frequency() - 261.63).abs() < 0.1);

        synth.trigger_attack("A4", 0.0, 1.0);
        assert!((synth.oscillator.frequency() - 440.0).abs() < 0.1);
    }

    #[test]
    fn test_synth_with_different_durations() {
        let mut synth = Synth::new();
        synth.bpm = 120.0;

        // "8n" at 120 BPM = 0.25s
        synth.trigger_attack_release("E4", "8n", 0.0, 1.0);
        let mut output = [0.0f32; 256];
        synth.process(&[], &mut output, 44100);

        let max = output.iter().fold(0.0f32, |a, &b| a.max(b.abs()));
        assert!(max > 0.01, "synth output is silent");
    }
}
