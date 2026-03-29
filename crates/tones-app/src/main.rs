#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::{Arc, Mutex};

use tauri::State;
use tones_core::backend::AudioBackend;
use tones_core::clock::Transport;
use tones_core::component::gain::Gain;
use tones_core::event::sequence::{Sequence, Step};
use tones_core::graph::{AudioGraph, NodeId};
use tones_core::instrument::Synth;
use tones_core::source::oscillator::OscillatorType;
use tones_cpal::CpalBackend;

struct AppState {
    graph: Arc<Mutex<AudioGraph>>,
    transport: Arc<Transport>,
    synth_id: NodeId,
    gain_id: NodeId,
}

fn parse_waveform(s: &str) -> OscillatorType {
    match s {
        "square" => OscillatorType::Square,
        "sawtooth" => OscillatorType::Sawtooth,
        "triangle" => OscillatorType::Triangle,
        _ => OscillatorType::Sine,
    }
}

#[tauri::command]
fn play_note(state: State<AppState>, note: String, waveform: String, duration: String) {
    let wf = parse_waveform(&waveform);

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

#[tauri::command]
fn play_sequence(state: State<AppState>, notes: Vec<String>, waveform: String, bpm: f64) {
    let transport = &state.transport;
    transport.stop();
    transport.clear_all();
    transport.set_bpm(bpm);

    let steps: Vec<Step> = notes
        .iter()
        .map(|n| {
            if n == "_" {
                Step::rest("8n")
            } else {
                Step::note(n, "8n")
            }
        })
        .collect();

    let graph = Arc::clone(&state.graph);
    let synth_id = state.synth_id;
    let wf = parse_waveform(&waveform);

    let mut seq = Sequence::new(steps);
    seq.schedule_on(transport, move |note, dur_secs, _time| {
        let mut synth = Synth::new();
        synth.set_waveform(wf);
        synth.trigger_attack_release(&note, &format!("{dur_secs}"), 0.0, 0.8);
        if let Ok(mut g) = graph.lock() {
            g.replace_node(synth_id, Box::new(synth));
        }
    });

    transport.start();
}

#[tauri::command]
fn stop_sequence(state: State<AppState>) {
    state.transport.stop();
    state.transport.clear_all();
}

fn main() {
    let mut graph = AudioGraph::new();

    let synth_id = graph.add_node(Box::new(Synth::new()));
    let gain_id = graph.add_node(Box::new(Gain::new(0.3)));

    graph.connect(synth_id, gain_id);
    graph.set_output(gain_id);

    let graph = Arc::new(Mutex::new(graph));

    let mut backend = CpalBackend::new();
    let sample_rate = backend.sample_rate();
    let transport = Arc::new(Transport::new(sample_rate));

    let g = Arc::clone(&graph);
    let t = Arc::clone(&transport);
    backend.start(Box::new(move |buffer: &mut [f32]| {
        // Advance transport timing
        t.advance(buffer.len() as u32);

        if let Ok(mut graph) = g.try_lock() {
            graph.process(buffer, sample_rate);
        } else {
            buffer.fill(0.0);
        }
    }));

    Box::leak(Box::new(backend));

    let app_state = AppState {
        graph,
        transport,
        synth_id,
        gain_id,
    };

    tauri::Builder::default()
        .manage(app_state)
        .invoke_handler(tauri::generate_handler![
            play_note,
            set_volume,
            play_sequence,
            stop_sequence,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
