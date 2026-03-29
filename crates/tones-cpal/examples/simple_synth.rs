use tones_core::component::gain::Gain;
use tones_core::engine::AudioEngine;
use tones_core::source::oscillator::{Oscillator, OscillatorType};
use tones_cpal::CpalBackend;

fn main() {
    let mut backend = CpalBackend::new();
    let engine = AudioEngine::new(&backend);

    // Create a 440Hz sine wave oscillator
    let osc_id = engine.add_node(Box::new(Oscillator::new(OscillatorType::Sine, 440.0)));

    // Reduce volume to avoid clipping (0.3 = ~-10dB)
    let gain_id = engine.add_node(Box::new(Gain::new(0.3)));

    // Connect: Oscillator -> Gain -> Output
    engine.connect(osc_id, gain_id);
    engine.set_output(gain_id);

    // Start audio
    engine.start(&mut backend);

    println!("Playing 440Hz sine wave. Press Enter to stop.");
    let mut input = String::new();
    std::io::stdin().read_line(&mut input).unwrap();
}
