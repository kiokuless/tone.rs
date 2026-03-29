#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::{Arc, Mutex};

use tauri::State;
use tones_core::backend::AudioBackend;
use tones_core::component::gain::Gain;
use tones_core::graph::{AudioGraph, NodeId};
use tones_core::instrument::Synth;
use tones_core::source::oscillator::OscillatorType;
use tones_cpal::CpalBackend;

struct AppState {
    graph: Arc<Mutex<AudioGraph>>,
    synth_id: NodeId,
    gain_id: NodeId,
}

#[tauri::command]
fn play_note(state: State<AppState>, note: String, waveform: String, duration: String) {
    let wf = match waveform.as_str() {
        "square" => OscillatorType::Square,
        "sawtooth" => OscillatorType::Sawtooth,
        "triangle" => OscillatorType::Triangle,
        _ => OscillatorType::Sine,
    };

    let mut synth = Synth::new();
    synth.set_waveform(wf);
    synth.trigger_attack_release(&note, &duration, 0.0, 1.0);

    let mut graph = state.graph.lock().unwrap();
    graph.replace_node(state.synth_id, Box::new(synth));
}

#[tauri::command]
fn set_volume(state: State<AppState>, volume: f64) {
    let mut graph = state.graph.lock().unwrap();
    graph.replace_node(state.gain_id, Box::new(Gain::new(volume as f32)));
}

fn main() {
    let mut graph = AudioGraph::new();

    let synth_id = graph.add_node(Box::new(Synth::new()));
    let gain_id = graph.add_node(Box::new(Gain::new(0.3)));

    graph.connect(synth_id, gain_id);
    graph.set_output(gain_id);

    let graph = Arc::new(Mutex::new(graph));

    // Start audio on the main thread.
    // cpal's Stream is !Send, so it must live on the main thread on macOS.
    let mut backend = CpalBackend::new();
    let sample_rate = backend.sample_rate();

    let g = Arc::clone(&graph);
    backend.start(Box::new(move |buffer: &mut [f32]| {
        if let Ok(mut graph) = g.try_lock() {
            graph.process(buffer, sample_rate);
        } else {
            buffer.fill(0.0);
        }
    }));

    // Leak the backend so the cpal Stream lives for the entire program
    Box::leak(Box::new(backend));

    let app_state = AppState {
        graph,
        synth_id,
        gain_id,
    };

    tauri::Builder::default()
        .manage(app_state)
        .invoke_handler(tauri::generate_handler![play_note, set_volume])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
