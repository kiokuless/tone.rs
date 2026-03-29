# tone-rs

Rust implementation of [Tone.js](https://tonejs.github.io/) — a framework for creating interactive music and audio applications.

Designed to be embedded in native apps (iOS, Android, macOS, Windows, Linux) via FFI and compiled to WASM for browser use.

## Features

### Sources
- **Oscillator** — Sine, Square, Sawtooth, Triangle waveforms
- **Noise** — White, Pink, Brown noise generators
- **Player** — WAV file buffer playback with loop and variable playback rate
- **GrainPlayer** — Granular synthesis for pitch-preserving tempo change

### Instruments
- **Synth** — Monophonic synthesizer (Oscillator + ADSR Envelope)
- **PolySynth** — Polyphonic synthesizer with voice allocation

### Effects
- **Filter** — State-variable filter (LowPass, HighPass, BandPass)
- **Delay** — Feedback delay with wet/dry control
- **Distortion** — Tanh waveshaping distortion

### Core
- **AudioGraph** — Node-based audio processing with topological sort
- **Signal/Param** — Sample-accurate parameter automation (linearRamp, exponentialRamp, setTargetAtTime)
- **Envelope** — ADSR envelope generator with linear/exponential curves
- **Transport** — Master transport with BPM, play/pause/stop, event scheduling, loop
- **Sequence** — Step sequencer for musical event scheduling
- **Mixer** — Multi-track mixer with per-track gain, mute, solo
- **Time Notation** — Musical time parsing (`"4n"`, `"8t"`, `"4n."`, `"1:2:3"`, `"2hz"`)
- **Frequency** — Note name to frequency conversion (`"C4"` → 261.63 Hz)

## Crate Structure

```
crates/
├── tone-core    # Platform-independent DSP, scheduling, and synthesis core
├── tone-cpal    # Native audio backend (macOS, Windows, Linux) via cpal
├── tone-wasm    # Browser audio backend via Web Audio API + wasm-bindgen
└── tone-app     # Tauri desktop app for interactive testing
```

## Quick Start

### Native (cpal)

```rust
use tone_core::instrument::Synth;
use tone_core::graph::{AudioGraph, AudioNode};
use tone_core::component::gain::Gain;
use tone_cpal::CpalBackend;
use tone_core::backend::AudioBackend;
use std::sync::{Arc, Mutex};

let mut backend = CpalBackend::new();
let sample_rate = backend.sample_rate();

let mut graph = AudioGraph::new();
let mut synth = Synth::new();
synth.trigger_attack_release("C4", "8n", 0.0, 1.0);
let synth_id = graph.add_node(Box::new(synth));
let gain_id = graph.add_node(Box::new(Gain::new(0.3)));
graph.connect(synth_id, gain_id);
graph.set_output(gain_id);

let graph = Arc::new(Mutex::new(graph));
let g = Arc::clone(&graph);
backend.start(Box::new(move |buffer: &mut [f32]| {
    if let Ok(mut graph) = g.try_lock() {
        graph.process(buffer, sample_rate);
    }
}));
```

### Browser (WASM)

```bash
cd crates/tone-wasm
wasm-pack build --target web
```

```js
import init, { TonesEngine } from './pkg/tone_wasm.js';
await init();

const engine = new TonesEngine();
engine.playNote("C4", "sine", "8n");
engine.playSequence(["C4", "E4", "G4", "C5"], "sine", 120);
```

### Tauri App

```bash
cd crates/tone-app
cargo tauri dev
```

Piano keyboard UI with waveform selection, musical duration notation, sequencer presets, and real-time effects (filter, delay, distortion).

## Examples

```bash
# Play a 440Hz sine wave
cargo run --example simple_synth -p tone-cpal

# Play a note with ADSR envelope
cargo run --example envelope_demo -p tone-cpal
```

## Testing

```bash
cargo test -p tone-core
```

## Architecture

```
Oscillator/Player/Noise
        ↓
   AmplitudeEnvelope
        ↓
    Distortion → Filter → Delay
        ↓
       Gain
        ↓
   AudioBackend (cpal / Web Audio)
```

All audio processing happens through the **AudioGraph**, which topologically sorts nodes and processes them in order. Parameters are controlled via atomic values for real-time safety — no allocations or locks on the audio thread.

The **Transport** provides BPM-aware scheduling with `schedule()`, `scheduleRepeat()`, and `scheduleOnce()`, enabling tempo-synced playback of sequences and events.

## License

MIT
