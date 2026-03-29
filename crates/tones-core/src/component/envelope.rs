use crate::graph::AudioNode;
use crate::signal::param::Param;

/// The curve shape for envelope segments.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum EnvelopeCurve {
    Linear,
    Exponential,
}

/// ADSR envelope generator.
///
/// Generates an amplitude envelope that goes through Attack, Decay,
/// Sustain, and Release phases. Uses the Param automation system
/// for sample-accurate curve generation.
///
/// Port of Tone.js Envelope.
pub struct Envelope {
    /// Attack time in seconds.
    pub attack: f64,
    /// Decay time in seconds.
    pub decay: f64,
    /// Sustain level (0.0 .. 1.0).
    pub sustain: f64,
    /// Release time in seconds.
    pub release: f64,
    /// Shape of the attack curve.
    pub attack_curve: EnvelopeCurve,
    /// Shape of the decay curve.
    pub decay_curve: EnvelopeCurve,
    /// Shape of the release curve.
    pub release_curve: EnvelopeCurve,

    /// Internal parameter that generates the envelope signal.
    sig: Param,
}

impl Envelope {
    pub fn new(attack: f64, decay: f64, sustain: f64, release: f64) -> Self {
        Self {
            attack,
            decay,
            sustain,
            release,
            attack_curve: EnvelopeCurve::Linear,
            decay_curve: EnvelopeCurve::Exponential,
            release_curve: EnvelopeCurve::Exponential,
            sig: Param::new(0.0),
        }
    }

    /// Trigger the attack phase at the given time.
    /// `velocity` scales the peak level (0.0 .. 1.0).
    pub fn trigger_attack(&mut self, time: f64, velocity: f64) {
        let attack = self.attack;
        let decay = self.decay;
        let sustain = self.sustain;

        // Attack phase
        self.sig.cancel_scheduled_values(time);
        if attack < 1e-6 {
            // Instant attack
            self.sig.set_value_at_time(velocity, time);
        } else {
            match self.attack_curve {
                EnvelopeCurve::Linear => {
                    self.sig.linear_ramp_to(velocity, attack, time);
                }
                EnvelopeCurve::Exponential => {
                    self.sig.target_ramp_to(velocity, attack, time);
                }
            }
        }

        // Decay phase
        if decay > 1e-6 && sustain < 1.0 {
            let decay_value = velocity * sustain;
            let decay_start = time + attack;

            // Anchor the peak value so decay starts from the correct level
            self.sig.set_value_at_time(velocity, decay_start);

            match self.decay_curve {
                EnvelopeCurve::Linear => {
                    self.sig
                        .linear_ramp_to_value_at_time(decay_value, decay_start + decay);
                }
                EnvelopeCurve::Exponential => {
                    let time_constant = (decay + 1.0).ln() / 200.0_f64.ln();
                    self.sig
                        .set_target_at_time(decay_value, decay_start, time_constant);
                    // Ensure we reach the sustain level
                    self.sig
                        .set_value_at_time(decay_value, decay_start + decay);
                }
            }
        }
    }

    /// Trigger the release phase at the given time.
    pub fn trigger_release(&mut self, time: f64) {
        let current = self.sig.get_value_at_time(time);
        let release = self.release;

        self.sig.cancel_scheduled_values(time);

        if current <= 1e-6 {
            return;
        }

        if release < 1e-6 {
            self.sig.set_value_at_time(0.0, time);
        } else {
            match self.release_curve {
                EnvelopeCurve::Linear => {
                    self.sig.set_value_at_time(current, time);
                    self.sig.linear_ramp_to_value_at_time(0.0, time + release);
                }
                EnvelopeCurve::Exponential => {
                    self.sig.set_value_at_time(current, time);
                    let time_constant = (release + 1.0).ln() / 200.0_f64.ln();
                    self.sig.set_target_at_time(0.0, time, time_constant);
                    self.sig.set_value_at_time(0.0, time + release);
                }
            }
        }
    }

    /// Convenience: trigger attack and schedule release after `duration` seconds.
    pub fn trigger_attack_release(&mut self, time: f64, duration: f64, velocity: f64) {
        self.trigger_attack(time, velocity);
        self.trigger_release(time + duration);
    }

    /// Get the envelope value at the given time.
    pub fn get_value_at_time(&self, time: f64) -> f64 {
        self.sig.get_value_at_time(time)
    }
}

/// AmplitudeEnvelope: an Envelope that applies its output as gain to the input signal.
///
/// Equivalent to Tone.js AmplitudeEnvelope — multiplies the input by
/// the envelope's value at each sample.
pub struct AmplitudeEnvelope {
    pub envelope: Envelope,
    current_time: f64,
}

impl AmplitudeEnvelope {
    pub fn new(attack: f64, decay: f64, sustain: f64, release: f64) -> Self {
        Self {
            envelope: Envelope::new(attack, decay, sustain, release),
            current_time: 0.0,
        }
    }

    pub fn trigger_attack(&mut self, time: f64, velocity: f64) {
        self.envelope.trigger_attack(time, velocity);
    }

    pub fn trigger_release(&mut self, time: f64) {
        self.envelope.trigger_release(time);
    }

    pub fn trigger_attack_release(&mut self, time: f64, duration: f64, velocity: f64) {
        self.envelope.trigger_attack_release(time, duration, velocity);
    }
}

impl AudioNode for AmplitudeEnvelope {
    fn process(&mut self, input: &[f32], output: &mut [f32], sample_rate: u32) {
        let sample_period = 1.0 / sample_rate as f64;

        for (i, out) in output.iter_mut().enumerate() {
            let t = self.current_time + i as f64 * sample_period;
            let env_value = self.envelope.get_value_at_time(t) as f32;

            if i < input.len() {
                *out = input[i] * env_value;
            } else {
                *out = 0.0;
            }
        }

        self.current_time += output.len() as f64 * sample_period;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_envelope_attack_decay_sustain() {
        let mut env = Envelope::new(0.1, 0.1, 0.5, 0.1);
        env.trigger_attack(0.0, 1.0);

        // Debug: check values at several points
        // Before attack: 0
        assert!((env.get_value_at_time(0.0) - 0.0).abs() < 0.01);

        // Mid attack (linear): ~0.5
        let mid = env.get_value_at_time(0.05);
        assert!(mid > 0.3 && mid < 0.7, "mid attack = {mid}");

        // Near end of attack: close to 1.0
        let peak = env.get_value_at_time(0.099);
        assert!(peak > 0.9, "peak = {peak}");

        // After decay, should approach sustain level (0.5)
        let sustained = env.get_value_at_time(0.3);
        assert!(
            (sustained - 0.5).abs() < 0.05,
            "sustained = {sustained}"
        );
    }

    #[test]
    fn test_envelope_release() {
        let mut env = Envelope::new(0.01, 0.01, 0.8, 0.1);
        env.release_curve = EnvelopeCurve::Linear;
        env.trigger_attack(0.0, 1.0);
        env.trigger_release(0.1);

        // Just before release: should be near sustain (0.8)
        let before = env.get_value_at_time(0.099);
        assert!(before > 0.5, "before release = {before}");

        // After release: should be 0
        let after = env.get_value_at_time(0.3);
        assert!(after < 0.01, "after release = {after}");
    }

    #[test]
    fn test_attack_release_convenience() {
        let mut env = Envelope::new(0.01, 0.01, 1.0, 0.1);
        env.release_curve = EnvelopeCurve::Linear;
        env.trigger_attack_release(0.0, 0.1, 1.0);

        // During note: should be > 0
        assert!(env.get_value_at_time(0.05) > 0.5);

        // After release: should approach 0
        assert!(env.get_value_at_time(0.5) < 0.01);
    }

    #[test]
    fn test_amplitude_envelope_process() {
        let mut amp_env = AmplitudeEnvelope::new(0.0, 0.0, 1.0, 0.0);
        amp_env.trigger_attack(0.0, 1.0);

        let input = [1.0f32; 64];
        let mut output = [0.0f32; 64];
        amp_env.process(&input, &mut output, 44100);

        // With instant attack and sustain=1.0, output should equal input
        for (i, &sample) in output.iter().enumerate() {
            assert!(
                (sample - 1.0).abs() < 0.01,
                "sample {i} = {sample}"
            );
        }
    }
}
