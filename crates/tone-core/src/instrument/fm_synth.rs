//! FM (Frequency Modulation) synthesizer.
//!
//! Two-operator FM synthesis: a modulator oscillator modulates the frequency
//! of a carrier oscillator. Port of Tone.js FMSynth.

use std::f32::consts::PI;
use std::sync::atomic::{AtomicU32, Ordering};

use crate::component::envelope::{AmplitudeEnvelope, Envelope};
use crate::graph::AudioNode;
use crate::source::oscillator::OscillatorType;
use crate::time::frequency::note_to_frequency;
use crate::time::notation::parse_time;

/// FM synthesizer with carrier + modulator architecture.
///
/// `modulator_freq = carrier_freq × harmonicity`
/// `frequency_deviation = modulator_output × carrier_freq × modulation_index`
pub struct FmSynth {
    // Carrier
    carrier_phase: f32,
    carrier_waveform: OscillatorType,
    carrier_envelope: AmplitudeEnvelope,

    // Modulator
    modulator_phase: f32,
    modulator_waveform: OscillatorType,
    modulator_envelope: Envelope,

    // Atomic parameters
    frequency_bits: AtomicU32,
    harmonicity_bits: AtomicU32,
    mod_index_bits: AtomicU32,

    // Scratch
    carrier_buf: Vec<f32>,
    current_time: f64,

    /// BPM for time notation parsing.
    pub bpm: f64,
}

#[inline]
fn generate(waveform: OscillatorType, phase: f32) -> f32 {
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

impl FmSynth {
    pub fn new() -> Self {
        Self {
            carrier_phase: 0.0,
            carrier_waveform: OscillatorType::Sine,
            carrier_envelope: AmplitudeEnvelope::new(0.01, 0.01, 1.0, 0.5),
            modulator_phase: 0.0,
            modulator_waveform: OscillatorType::Sine,
            modulator_envelope: Envelope::new(0.01, 0.0, 1.0, 0.5),
            frequency_bits: AtomicU32::new(440.0f32.to_bits()),
            harmonicity_bits: AtomicU32::new(3.0f32.to_bits()),
            mod_index_bits: AtomicU32::new(10.0f32.to_bits()),
            carrier_buf: Vec::new(),
            current_time: 0.0,
            bpm: 120.0,
        }
    }

    pub fn set_waveform(&mut self, waveform: OscillatorType) {
        self.carrier_waveform = waveform;
    }

    pub fn set_modulator_waveform(&mut self, waveform: OscillatorType) {
        self.modulator_waveform = waveform;
    }

    pub fn harmonicity(&self) -> f32 {
        f32::from_bits(self.harmonicity_bits.load(Ordering::Relaxed))
    }

    pub fn set_harmonicity(&self, h: f32) {
        self.harmonicity_bits
            .store(h.max(0.0).to_bits(), Ordering::Relaxed);
    }

    pub fn modulation_index(&self) -> f32 {
        f32::from_bits(self.mod_index_bits.load(Ordering::Relaxed))
    }

    pub fn set_modulation_index(&self, idx: f32) {
        self.mod_index_bits
            .store(idx.max(0.0).to_bits(), Ordering::Relaxed);
    }

    fn frequency(&self) -> f32 {
        f32::from_bits(self.frequency_bits.load(Ordering::Relaxed))
    }

    pub fn trigger_attack(&mut self, note: &str, time: f64, velocity: f64) {
        if let Ok(freq) = note_to_frequency(note) {
            self.frequency_bits
                .store((freq as f32).to_bits(), Ordering::Relaxed);
        }
        self.carrier_envelope.trigger_attack(time, velocity);
        self.modulator_envelope.trigger_attack(time, velocity);
    }

    pub fn trigger_release(&mut self, time: f64) {
        self.carrier_envelope.trigger_release(time);
        self.modulator_envelope.trigger_release(time);
    }

    pub fn trigger_attack_release(&mut self, note: &str, duration: &str, time: f64, velocity: f64) {
        if let Ok(freq) = note_to_frequency(note) {
            self.frequency_bits
                .store((freq as f32).to_bits(), Ordering::Relaxed);
        }
        let dur_secs = parse_time(duration, self.bpm).unwrap_or(0.5);
        self.carrier_envelope
            .trigger_attack_release(time, dur_secs, velocity);
        self.modulator_envelope
            .trigger_attack_release(time, dur_secs, velocity);
    }
}

impl Default for FmSynth {
    fn default() -> Self {
        Self::new()
    }
}

impl AudioNode for FmSynth {
    fn process(&mut self, _input: &[f32], output: &mut [f32], sample_rate: u32) {
        let len = output.len();
        if self.carrier_buf.len() < len {
            self.carrier_buf.resize(len, 0.0);
        }

        let carrier_freq = self.frequency();
        let harmonicity = self.harmonicity();
        let mod_index = self.modulation_index();
        let sr = sample_rate as f32;
        let sample_period = 1.0 / sample_rate as f64;

        let mod_freq = carrier_freq * harmonicity;

        for i in 0..len {
            let t = self.current_time + i as f64 * sample_period;

            // Modulator
            let mod_env = self.modulator_envelope.get_value_at_time(t) as f32;
            let mod_output = generate(self.modulator_waveform, self.modulator_phase) * mod_env;
            let freq_deviation = mod_output * carrier_freq * mod_index;

            // Carrier
            self.carrier_buf[i] = generate(self.carrier_waveform, self.carrier_phase);

            // Advance phases
            self.carrier_phase += (carrier_freq + freq_deviation) / sr;
            self.carrier_phase = self.carrier_phase.rem_euclid(1.0);
            self.modulator_phase += mod_freq / sr;
            self.modulator_phase = self.modulator_phase.rem_euclid(1.0);
        }

        // Apply carrier envelope
        self.carrier_envelope
            .process(&self.carrier_buf[..len], output, sample_rate);

        self.current_time += len as f64 * sample_period;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fm_synth_produces_output() {
        let mut synth = FmSynth::new();
        synth.trigger_attack_release("A4", "4n", 0.0, 1.0);

        let mut output = [0.0f32; 512];
        synth.process(&[], &mut output, 44100);

        let max = output.iter().fold(0.0f32, |a, &b| a.max(b.abs()));
        assert!(max > 0.01, "FM synth should produce audio: max={max}");
    }

    #[test]
    fn test_harmonicity_changes_output() {
        let mut synth_a = FmSynth::new();
        synth_a.set_harmonicity(1.0);
        synth_a.trigger_attack_release("A4", "4n", 0.0, 1.0);

        let mut synth_b = FmSynth::new();
        synth_b.set_harmonicity(5.0);
        synth_b.trigger_attack_release("A4", "4n", 0.0, 1.0);

        let mut out_a = [0.0f32; 512];
        let mut out_b = [0.0f32; 512];
        synth_a.process(&[], &mut out_a, 44100);
        synth_b.process(&[], &mut out_b, 44100);

        // Outputs should differ due to different harmonicity
        let diff: f32 = out_a
            .iter()
            .zip(out_b.iter())
            .map(|(a, b)| (a - b).abs())
            .sum();
        assert!(
            diff > 0.1,
            "different harmonicity should produce different output"
        );
    }

    #[test]
    fn test_modulation_index_changes_output() {
        let mut synth_a = FmSynth::new();
        synth_a.set_modulation_index(0.0);
        synth_a.trigger_attack_release("A4", "4n", 0.0, 1.0);

        let mut synth_b = FmSynth::new();
        synth_b.set_modulation_index(20.0);
        synth_b.trigger_attack_release("A4", "4n", 0.0, 1.0);

        let mut out_a = [0.0f32; 512];
        let mut out_b = [0.0f32; 512];
        synth_a.process(&[], &mut out_a, 44100);
        synth_b.process(&[], &mut out_b, 44100);

        let diff: f32 = out_a
            .iter()
            .zip(out_b.iter())
            .map(|(a, b)| (a - b).abs())
            .sum();
        assert!(
            diff > 0.1,
            "different modulation index should produce different output"
        );
    }
}
