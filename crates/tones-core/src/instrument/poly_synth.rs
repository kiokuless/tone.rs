use crate::component::envelope::AmplitudeEnvelope;
use crate::graph::AudioNode;
use crate::source::oscillator::{Oscillator, OscillatorType};
use crate::time::frequency::note_to_frequency;
use crate::time::notation::parse_time;

/// A single voice in the polyphonic synthesizer.
struct Voice {
    oscillator: Oscillator,
    envelope: AmplitudeEnvelope,
    osc_buffer: Vec<f32>,
    active: bool,
}

impl Voice {
    fn new(waveform: OscillatorType) -> Self {
        Self {
            oscillator: Oscillator::new(waveform, 440.0),
            envelope: AmplitudeEnvelope::new(0.02, 0.1, 0.6, 0.3),
            osc_buffer: Vec::new(),
            active: false,
        }
    }

    fn process(&mut self, output: &mut [f32], sample_rate: u32) {
        let len = output.len();
        if self.osc_buffer.len() < len {
            self.osc_buffer.resize(len, 0.0);
        }

        let osc_out = &mut self.osc_buffer[..len];
        self.oscillator.process(&[], osc_out, sample_rate);
        self.envelope.process(osc_out, output, sample_rate);
    }
}

/// A polyphonic synthesizer that can play multiple notes simultaneously.
///
/// Manages a pool of voices and allocates them to incoming notes.
pub struct PolySynth {
    voices: Vec<Voice>,
    waveform: OscillatorType,
    mix_buffer: Vec<f32>,
    pub bpm: f64,
}

impl PolySynth {
    /// Create a new PolySynth with the given maximum polyphony.
    pub fn new(max_voices: usize) -> Self {
        let voices = (0..max_voices)
            .map(|_| Voice::new(OscillatorType::Sine))
            .collect();
        Self {
            voices,
            waveform: OscillatorType::Sine,
            mix_buffer: Vec::new(),
            bpm: 120.0,
        }
    }

    /// Find an inactive voice index, or steal the first one.
    fn find_voice(&self) -> usize {
        self.voices
            .iter()
            .position(|v| !v.active)
            .unwrap_or(0)
    }

    pub fn set_waveform(&mut self, waveform: OscillatorType) {
        self.waveform = waveform;
    }

    /// Trigger a note on an available voice.
    pub fn trigger_attack(&mut self, note: &str, time: f64, velocity: f64) {
        let freq = match note_to_frequency(note) {
            Ok(f) => f as f32,
            Err(_) => return,
        };

        let idx = self.find_voice();
        let voice = &mut self.voices[idx];
        voice.oscillator = Oscillator::new(self.waveform, freq);
        voice.envelope = AmplitudeEnvelope::new(0.02, 0.1, 0.6, 0.3);
        voice.envelope.trigger_attack(time, velocity);
        voice.active = true;
    }

    /// Trigger attack and release for a note.
    pub fn trigger_attack_release(
        &mut self,
        note: &str,
        duration: &str,
        time: f64,
        velocity: f64,
    ) {
        let freq = match note_to_frequency(note) {
            Ok(f) => f as f32,
            Err(_) => return,
        };

        let dur_secs = parse_time(duration, self.bpm).unwrap_or(0.5);

        let idx = self.find_voice();
        let voice = &mut self.voices[idx];
        voice.oscillator = Oscillator::new(self.waveform, freq);
        voice.envelope = AmplitudeEnvelope::new(0.02, 0.1, 0.6, 0.3);
        voice.envelope.trigger_attack_release(time, dur_secs, velocity);
        voice.active = true;
    }
}

impl AudioNode for PolySynth {
    fn process(&mut self, _input: &[f32], output: &mut [f32], sample_rate: u32) {
        let len = output.len();
        output.fill(0.0);

        if self.mix_buffer.len() < len {
            self.mix_buffer.resize(len, 0.0);
        }

        for voice in &mut self.voices {
            if !voice.active {
                continue;
            }

            let buf = &mut self.mix_buffer[..len];
            buf.fill(0.0);
            voice.process(buf, sample_rate);

            // Mix into output
            for (out, &v) in output.iter_mut().zip(buf.iter()) {
                *out += v;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_poly_synth_multiple_notes() {
        let mut synth = PolySynth::new(4);
        synth.trigger_attack_release("C4", "4n", 0.0, 1.0);
        synth.trigger_attack_release("E4", "4n", 0.0, 1.0);
        synth.trigger_attack_release("G4", "4n", 0.0, 1.0);

        let mut output = [0.0f32; 256];
        synth.process(&[], &mut output, 44100);

        let max = output.iter().fold(0.0f32, |a, &b| a.max(b.abs()));
        assert!(max > 0.1, "poly synth should produce audio: max={max}");
    }

    #[test]
    fn test_poly_synth_voice_stealing() {
        let mut synth = PolySynth::new(2);
        // Trigger 3 notes on 2 voices — third should steal
        synth.trigger_attack_release("C4", "4n", 0.0, 1.0);
        synth.trigger_attack_release("E4", "4n", 0.0, 1.0);
        synth.trigger_attack_release("G4", "4n", 0.0, 1.0);

        let mut output = [0.0f32; 256];
        synth.process(&[], &mut output, 44100);

        let max = output.iter().fold(0.0f32, |a, &b| a.max(b.abs()));
        assert!(max > 0.1, "should still produce audio after voice stealing");
    }
}
