#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::{Arc, Mutex};

use tauri::State;
use tone_core::backend::AudioBackend;
use tone_core::clock::Transport;
use tone_core::component::gain::Gain;
use tone_core::effect::delay::Delay;
use tone_core::effect::distortion::Distortion;
use tone_core::effect::filter::{Filter, FilterType};
use tone_core::event::sequence::{Sequence, Step};
use tone_core::graph::{AudioGraph, NodeId};
use tone_core::instrument::Synth;
use tone_core::source::oscillator::OscillatorType;
use tone_cpal::CpalBackend;

struct AppState {
    graph: Arc<Mutex<AudioGraph>>,
    transport: Arc<Transport>,
    synth_id: NodeId,
    filter_id: NodeId,
    delay_id: NodeId,
    distortion_id: NodeId,
    gain_id: NodeId,
    sample_rate: u32,
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
    let mut synth = Synth::new();
    synth.set_waveform(parse_waveform(&waveform));
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
fn set_filter(state: State<AppState>, filter_type: String, cutoff: f64, q: f64, wet: f64) {
    let ft = match filter_type.as_str() {
        "highpass" => FilterType::HighPass,
        "bandpass" => FilterType::BandPass,
        _ => FilterType::LowPass,
    };
    let filter = Filter::new(ft, cutoff as f32, q as f32);
    filter.set_wet(wet as f32);
    let mut graph = state.graph.lock().unwrap();
    graph.replace_node(state.filter_id, Box::new(filter));
}

#[tauri::command]
fn set_delay(state: State<AppState>, time: f64, feedback: f64, wet: f64) {
    let delay = Delay::new(time as f32, feedback as f32, state.sample_rate);
    delay.set_wet(wet as f32);
    let mut graph = state.graph.lock().unwrap();
    graph.replace_node(state.delay_id, Box::new(delay));
}

#[tauri::command]
fn set_distortion(state: State<AppState>, drive: f64, wet: f64) {
    let dist = Distortion::new(drive as f32);
    dist.set_wet(wet as f32);
    let mut graph = state.graph.lock().unwrap();
    graph.replace_node(state.distortion_id, Box::new(dist));
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
            if n == "_" { Step::rest("8n") } else { Step::note(n, "8n") }
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
    let mut backend = CpalBackend::new();
    let sample_rate = backend.sample_rate();

    let mut graph = AudioGraph::new();

    // Signal chain: Synth → Distortion → Filter → Delay → Gain → Output
    let synth_id = graph.add_node(Box::new(Synth::new()));
    let distortion_id = graph.add_node(Box::new(Distortion::new(1.0))); // clean by default
    let filter_id = graph.add_node(Box::new(Filter::new(FilterType::LowPass, 20000.0, 1.0)));
    let delay_id = graph.add_node(Box::new(Delay::new(0.3, 0.0, sample_rate))); // off by default
    let gain_id = graph.add_node(Box::new(Gain::new(0.3)));

    graph.connect(synth_id, distortion_id);
    graph.connect(distortion_id, filter_id);
    graph.connect(filter_id, delay_id);
    graph.connect(delay_id, gain_id);
    graph.set_output(gain_id);

    let graph = Arc::new(Mutex::new(graph));
    let transport = Arc::new(Transport::new(sample_rate));

    let g = Arc::clone(&graph);
    let t = Arc::clone(&transport);
    backend.start(Box::new(move |buffer: &mut [f32]| {
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
        filter_id,
        delay_id,
        distortion_id,
        gain_id,
        sample_rate,
    };

    tauri::Builder::default()
        .manage(app_state)
        .invoke_handler(tauri::generate_handler![
            play_note,
            set_volume,
            set_filter,
            set_delay,
            set_distortion,
            play_sequence,
            stop_sequence,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
