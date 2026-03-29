use tones_core::component::envelope::AmplitudeEnvelope;
use tones_core::component::gain::Gain;
use tones_core::engine::AudioEngine;
use tones_core::source::oscillator::{Oscillator, OscillatorType};
use tones_cpal::CpalBackend;

fn main() {
    let mut backend = CpalBackend::new();
    let engine = AudioEngine::new(&backend);

    // Create a 440Hz sine oscillator
    let osc_id = engine.add_node(Box::new(Oscillator::new(OscillatorType::Sine, 440.0)));

    // ADSR envelope: 50ms attack, 100ms decay, 0.6 sustain, 200ms release
    let mut amp_env = AmplitudeEnvelope::new(0.05, 0.1, 0.6, 0.2);
    // Trigger a note at time 0, hold for 0.5s
    amp_env.trigger_attack_release(0.0, 0.5, 1.0);
    let env_id = engine.add_node(Box::new(amp_env));

    // Master volume
    let gain_id = engine.add_node(Box::new(Gain::new(0.3)));

    // Oscillator -> AmplitudeEnvelope -> Gain -> Output
    engine.connect(osc_id, env_id);
    engine.connect(env_id, gain_id);
    engine.set_output(gain_id);

    engine.start(&mut backend);

    println!("Playing a note with ADSR envelope (A=50ms D=100ms S=0.6 R=200ms).");
    println!("Note duration: 0.5s. Press Enter to stop.");
    let mut input = String::new();
    std::io::stdin().read_line(&mut input).unwrap();
}
