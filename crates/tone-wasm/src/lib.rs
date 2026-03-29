use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;
use web_sys::{AudioContext, ScriptProcessorNode};

use tone_core::clock::Transport;
use tone_core::component::gain::Gain;
use tone_core::effect::delay::Delay;
use tone_core::effect::distortion::Distortion;
use tone_core::effect::filter::{Filter, FilterType};
use tone_core::event::sequence::{Sequence, Step};
use tone_core::graph::{AudioGraph, AudioNode, NodeId};
use tone_core::instrument::Synth;
use tone_core::source::oscillator::OscillatorType;

/// The main tone-rs synth engine for use in the browser via WASM.
#[wasm_bindgen]
pub struct TonesEngine {
    graph: Arc<Mutex<AudioGraph>>,
    transport: Arc<Transport>,
    synth_id: NodeId,
    filter_id: NodeId,
    delay_id: NodeId,
    distortion_id: NodeId,
    gain_id: NodeId,
    sample_rate: u32,
    _context: AudioContext,
    _processor: ScriptProcessorNode,
}

#[wasm_bindgen]
impl TonesEngine {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Result<TonesEngine, JsValue> {
        let context = AudioContext::new()?;
        let sample_rate = context.sample_rate() as u32;

        let mut graph = AudioGraph::new();

        let synth_id = graph.add_node(Box::new(Synth::new()));
        let distortion_id = graph.add_node(Box::new(Distortion::new(1.0)));
        let filter_id = graph.add_node(Box::new(Filter::new(FilterType::LowPass, 20000.0, 1.0)));
        let delay_id = graph.add_node(Box::new(Delay::new(0.3, 0.0, sample_rate)));
        let gain_id = graph.add_node(Box::new(Gain::new(0.3)));

        graph.connect(synth_id, distortion_id);
        graph.connect(distortion_id, filter_id);
        graph.connect(filter_id, delay_id);
        graph.connect(delay_id, gain_id);
        graph.set_output(gain_id);

        let graph = Arc::new(Mutex::new(graph));
        let transport = Arc::new(Transport::new(sample_rate));

        let processor = context.create_script_processor_with_buffer_size_and_number_of_input_channels_and_number_of_output_channels(
            1024, 0, 1,
        )?;

        let g = Arc::clone(&graph);
        let t = Arc::clone(&transport);
        let sr = sample_rate;

        let callback = Closure::wrap(Box::new(move |event: web_sys::AudioProcessingEvent| {
            let output_buffer = event.output_buffer().unwrap();
            let mut channel_data = output_buffer.get_channel_data(0).unwrap();
            t.advance(channel_data.len() as u32);
            if let Ok(mut graph) = g.try_lock() {
                graph.process(&mut channel_data, sr);
            }
            output_buffer.copy_to_channel(&channel_data, 0).unwrap();
        }) as Box<dyn FnMut(web_sys::AudioProcessingEvent)>);

        processor.set_onaudioprocess(Some(callback.as_ref().unchecked_ref()));
        callback.forget();
        processor.connect_with_audio_node(&context.destination())?;

        Ok(TonesEngine {
            graph,
            transport,
            synth_id,
            filter_id,
            delay_id,
            distortion_id,
            gain_id,
            sample_rate,
            _context: context,
            _processor: processor,
        })
    }

    #[wasm_bindgen(js_name = playNote)]
    pub fn play_note(&self, note: &str, waveform: &str, duration: &str) {
        let wf = parse_waveform(waveform);
        let mut synth = Synth::new();
        synth.set_waveform(wf);
        synth.trigger_attack_release(note, duration, 0.0, 1.0);
        if let Ok(mut graph) = self.graph.lock() {
            graph.replace_node(self.synth_id, Box::new(synth));
        }
    }

    #[wasm_bindgen(js_name = setVolume)]
    pub fn set_volume(&self, volume: f64) {
        if let Ok(mut graph) = self.graph.lock() {
            graph.replace_node(self.gain_id, Box::new(Gain::new(volume as f32)));
        }
    }

    #[wasm_bindgen(js_name = setFilter)]
    pub fn set_filter(&self, filter_type: &str, cutoff: f64, q: f64, wet: f64) {
        let ft = match filter_type {
            "highpass" => FilterType::HighPass,
            "bandpass" => FilterType::BandPass,
            _ => FilterType::LowPass,
        };
        let filter = Filter::new(ft, cutoff as f32, q as f32);
        filter.set_wet(wet as f32);
        if let Ok(mut graph) = self.graph.lock() {
            graph.replace_node(self.filter_id, Box::new(filter));
        }
    }

    #[wasm_bindgen(js_name = setDelay)]
    pub fn set_delay(&self, time: f64, feedback: f64, wet: f64) {
        let delay = Delay::new(time as f32, feedback as f32, self.sample_rate);
        delay.set_wet(wet as f32);
        if let Ok(mut graph) = self.graph.lock() {
            graph.replace_node(self.delay_id, Box::new(delay));
        }
    }

    #[wasm_bindgen(js_name = setDistortion)]
    pub fn set_distortion(&self, drive: f64, wet: f64) {
        let dist = Distortion::new(drive as f32);
        dist.set_wet(wet as f32);
        if let Ok(mut graph) = self.graph.lock() {
            graph.replace_node(self.distortion_id, Box::new(dist));
        }
    }

    #[wasm_bindgen(js_name = playSequence)]
    pub fn play_sequence(&self, notes: Vec<JsValue>, waveform: &str, bpm: f64) {
        self.transport.stop();
        self.transport.clear_all();
        self.transport.set_bpm(bpm);

        let steps: Vec<Step> = notes
            .iter()
            .filter_map(|v| v.as_string())
            .map(|n| {
                if n == "_" {
                    Step::rest("8n")
                } else {
                    Step::note(&n, "8n")
                }
            })
            .collect();

        let graph = Arc::clone(&self.graph);
        let synth_id = self.synth_id;
        let wf = parse_waveform(waveform);

        let mut seq = Sequence::new(steps);
        seq.schedule_on(&self.transport, move |note, dur_secs, _time| {
            let mut synth = Synth::new();
            synth.set_waveform(wf);
            synth.trigger_attack_release(&note, &format!("{dur_secs}"), 0.0, 0.8);
            if let Ok(mut g) = graph.lock() {
                g.replace_node(synth_id, Box::new(synth));
            }
        });

        self.transport.start();
    }

    #[wasm_bindgen(js_name = stopSequence)]
    pub fn stop_sequence(&self) {
        self.transport.stop();
        self.transport.clear_all();
    }
}

// ---- Multi-track GrainPlayer engine ----

/// A track entry holding a GrainPlayer and per-track gain/mute state.
struct TrackEntry {
    player: tone_core::source::grain_player::GrainPlayer,
    gain: AtomicU32,
    mute: std::sync::atomic::AtomicBool,
}

/// Shared state for multi-track audio processing.
struct MultiTrackState {
    tracks: Vec<TrackEntry>,
    master_gain: f32,
    /// Scratch buffer reused across process calls.
    track_buffer: Vec<f32>,
}

impl MultiTrackState {
    fn new() -> Self {
        Self {
            tracks: Vec::new(),
            master_gain: 1.0,
            track_buffer: Vec::new(),
        }
    }

    fn process(&mut self, output: &mut [f32], sample_rate: u32) {
        let len = output.len();
        output.fill(0.0);

        if self.track_buffer.len() < len {
            self.track_buffer.resize(len, 0.0);
        }

        for track in &mut self.tracks {
            if track.mute.load(Ordering::Relaxed) {
                continue;
            }

            let buf = &mut self.track_buffer[..len];
            buf.fill(0.0);
            track.player.process(&[], buf, sample_rate);

            let gain = f32::from_bits(track.gain.load(Ordering::Relaxed));
            for (out, &t) in output.iter_mut().zip(buf.iter()) {
                *out += t * gain;
            }
        }

        let mg = self.master_gain;
        for sample in output.iter_mut() {
            *sample *= mg;
        }
    }
}

/// Multi-track GrainPlayer engine for synchronized playback.
#[wasm_bindgen]
pub struct MultiTrackEngine {
    state: Arc<Mutex<MultiTrackState>>,
    sample_rate: u32,
    _context: AudioContext,
    _processor: ScriptProcessorNode,
}

#[wasm_bindgen]
impl MultiTrackEngine {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Result<MultiTrackEngine, JsValue> {
        let context = AudioContext::new()?;
        let sample_rate = context.sample_rate() as u32;

        let state = Arc::new(Mutex::new(MultiTrackState::new()));

        let processor = context
            .create_script_processor_with_buffer_size_and_number_of_input_channels_and_number_of_output_channels(1024, 0, 1)?;

        let s = Arc::clone(&state);
        let sr = sample_rate;

        let callback = Closure::wrap(Box::new(move |event: web_sys::AudioProcessingEvent| {
            let output_buffer = event.output_buffer().unwrap();
            let mut channel_data = output_buffer.get_channel_data(0).unwrap();
            if let Ok(mut state) = s.try_lock() {
                state.process(&mut channel_data, sr);
            }
            output_buffer.copy_to_channel(&channel_data, 0).unwrap();
        }) as Box<dyn FnMut(web_sys::AudioProcessingEvent)>);

        processor.set_onaudioprocess(Some(callback.as_ref().unchecked_ref()));
        callback.forget();
        processor.connect_with_audio_node(&context.destination())?;

        Ok(MultiTrackEngine {
            state,
            sample_rate,
            _context: context,
            _processor: processor,
        })
    }

    /// Load a WAV file from URL and add as a GrainPlayer track. Returns track index.
    #[wasm_bindgen(js_name = loadTrack)]
    pub async fn load_track(&self, url: &str) -> Result<usize, JsValue> {
        let window = web_sys::window().ok_or("no window")?;
        let resp_value = JsFuture::from(window.fetch_with_str(url)).await?;
        let resp: web_sys::Response = resp_value.dyn_into()?;

        if !resp.ok() {
            return Err(JsValue::from_str(&format!(
                "fetch failed: {} {}",
                resp.status(),
                resp.status_text()
            )));
        }

        let array_buffer = JsFuture::from(resp.array_buffer()?).await?;
        let uint8_array = js_sys::Uint8Array::new(&array_buffer);
        let bytes = uint8_array.to_vec();

        let audio_buffer = tone_core::source::player::AudioBuffer::from_wav(&bytes)
            .map_err(|e| JsValue::from_str(&e.to_string()))?;

        let player =
            tone_core::source::grain_player::GrainPlayer::new(audio_buffer, self.sample_rate);

        let entry = TrackEntry {
            player,
            gain: AtomicU32::new(1.0f32.to_bits()),
            mute: std::sync::atomic::AtomicBool::new(false),
        };

        let mut state = self.state.lock().unwrap();
        let idx = state.tracks.len();
        state.tracks.push(entry);
        Ok(idx)
    }

    /// Start all tracks from the given offset (seconds).
    #[wasm_bindgen]
    pub fn play(&self, offset: f64) {
        if let Ok(mut state) = self.state.lock() {
            for track in &mut state.tracks {
                track.player.start_at(offset);
            }
        }
    }

    /// Stop all tracks.
    #[wasm_bindgen]
    pub fn stop(&self) {
        if let Ok(mut state) = self.state.lock() {
            for track in &mut state.tracks {
                track.player.stop();
            }
        }
    }

    /// Set playback rate for all tracks (tempo change, pitch preserved).
    #[wasm_bindgen(js_name = setPlaybackRate)]
    pub fn set_playback_rate(&self, rate: f64) {
        if let Ok(state) = self.state.lock() {
            for track in &state.tracks {
                track.player.set_playback_rate(rate as f32);
            }
        }
    }

    /// Get the current playback position in seconds (from first track).
    #[wasm_bindgen(js_name = getPosition)]
    pub fn get_position(&self) -> f64 {
        if let Ok(state) = self.state.lock() {
            if let Some(track) = state.tracks.first() {
                return track.player.get_position_seconds();
            }
        }
        0.0
    }

    /// Get the duration of the longest track in seconds.
    #[wasm_bindgen(js_name = getDuration)]
    pub fn get_duration(&self) -> f64 {
        if let Ok(state) = self.state.lock() {
            return state
                .tracks
                .iter()
                .map(|t| t.player.duration())
                .fold(0.0f64, f64::max);
        }
        0.0
    }

    /// Set gain for a specific track (0.0-1.0).
    #[wasm_bindgen(js_name = setTrackGain)]
    pub fn set_track_gain(&self, index: usize, gain: f64) {
        if let Ok(state) = self.state.lock() {
            if let Some(track) = state.tracks.get(index) {
                track.gain.store((gain as f32).to_bits(), Ordering::Relaxed);
            }
        }
    }

    /// Set mute for a specific track.
    #[wasm_bindgen(js_name = setTrackMute)]
    pub fn set_track_mute(&self, index: usize, mute: bool) {
        if let Ok(state) = self.state.lock() {
            if let Some(track) = state.tracks.get(index) {
                track.mute.store(mute, Ordering::Relaxed);
            }
        }
    }
}

// ---- Standalone WAV loader ----

/// Fetch a WAV file from a URL and decode it into an AudioBuffer.
#[wasm_bindgen(js_name = loadWav)]
pub async fn load_wav(url: &str) -> Result<JsValue, JsValue> {
    let window = web_sys::window().ok_or("no window")?;
    let resp_value = JsFuture::from(window.fetch_with_str(url)).await?;
    let resp: web_sys::Response = resp_value.dyn_into()?;

    if !resp.ok() {
        return Err(JsValue::from_str(&format!(
            "fetch failed: {} {}",
            resp.status(),
            resp.status_text()
        )));
    }

    let array_buffer = JsFuture::from(resp.array_buffer()?).await?;
    let uint8_array = js_sys::Uint8Array::new(&array_buffer);
    let bytes = uint8_array.to_vec();

    let audio_buffer = tone_core::source::player::AudioBuffer::from_wav(&bytes)
        .map_err(|e| JsValue::from_str(&e.to_string()))?;

    let result = js_sys::Object::new();
    let data = js_sys::Float32Array::from(audio_buffer.data.as_slice());
    js_sys::Reflect::set(&result, &"data".into(), &data)?;
    js_sys::Reflect::set(
        &result,
        &"sampleRate".into(),
        &audio_buffer.sample_rate.into(),
    )?;
    js_sys::Reflect::set(&result, &"duration".into(), &audio_buffer.duration().into())?;

    Ok(result.into())
}

fn parse_waveform(s: &str) -> OscillatorType {
    match s {
        "square" => OscillatorType::Square,
        "sawtooth" => OscillatorType::Sawtooth,
        "triangle" => OscillatorType::Triangle,
        _ => OscillatorType::Sine,
    }
}
