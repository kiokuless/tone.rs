#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::{Arc, Mutex};
use std::thread;

use tauri::State;
use tones_core::backend::AudioBackend;
use tones_core::component::envelope::AmplitudeEnvelope;
use tones_core::component::gain::Gain;
use tones_core::graph::{AudioGraph, NodeId};
use tones_core::source::oscillator::{Oscillator, OscillatorType};
use tones_cpal::CpalBackend;

struct AppState {
    graph: Arc<Mutex<AudioGraph>>,
    osc_id: NodeId,
    env_id: NodeId,
    gain_id: NodeId,
}

#[tauri::command]
fn play_note(state: State<AppState>, frequency: f64, waveform: String, duration: f64) {
    let wf = match waveform.as_str() {
        "square" => OscillatorType::Square,
        "sawtooth" => OscillatorType::Sawtooth,
        "triangle" => OscillatorType::Triangle,
        _ => OscillatorType::Sine,
    };

    let mut graph = state.graph.lock().unwrap();
    graph.replace_node(
        state.osc_id,
        Box::new(Oscillator::new(wf, frequency as f32)),
    );

    let mut env = AmplitudeEnvelope::new(0.02, 0.1, 0.6, 0.3);
    env.trigger_attack_release(0.0, duration, 1.0);
    graph.replace_node(state.env_id, Box::new(env));
}

#[tauri::command]
fn set_volume(state: State<AppState>, volume: f64) {
    let mut graph = state.graph.lock().unwrap();
    graph.replace_node(state.gain_id, Box::new(Gain::new(volume as f32)));
}

fn main() {
    let mut graph = AudioGraph::new();

    let osc_id = graph.add_node(Box::new(Oscillator::new(OscillatorType::Sine, 440.0)));
    let env_id = graph.add_node(Box::new(AmplitudeEnvelope::new(0.02, 0.1, 0.6, 0.3)));
    let gain_id = graph.add_node(Box::new(Gain::new(0.3)));

    graph.connect(osc_id, env_id);
    graph.connect(env_id, gain_id);
    graph.set_output(gain_id);

    let graph = Arc::new(Mutex::new(graph));

    // Start audio on a dedicated thread (cpal Stream is !Send)
    {
        let graph = Arc::clone(&graph);
        thread::spawn(move || {
            let mut backend = CpalBackend::new();
            let sample_rate = backend.sample_rate();
            let g = graph;
            backend.start(Box::new(move |buffer: &mut [f32]| {
                if let Ok(mut graph) = g.try_lock() {
                    graph.process(buffer, sample_rate);
                } else {
                    buffer.fill(0.0);
                }
            }));
            // Keep the thread alive so the stream isn't dropped
            loop {
                thread::park();
            }
        });
    }

    let app_state = AppState {
        graph,
        osc_id,
        env_id,
        gain_id,
    };

    tauri::Builder::default()
        .manage(app_state)
        .invoke_handler(tauri::generate_handler![play_note, set_volume])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
