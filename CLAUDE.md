# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build & Test Commands

```bash
# Run all core tests (57 tests)
cargo test -p tone-core

# Build entire workspace (all 4 crates)
cargo build

# Build WASM target
cd crates/tone-wasm && wasm-pack build --target web

# Run Tauri app
cd crates/tone-app && cargo tauri dev

# Run examples
cargo run --example simple_synth -p tone-cpal
cargo run --example envelope_demo -p tone-cpal

# Run a single test
cargo test -p tone-core test_name_here

# WASM compile check (without wasm-pack)
cargo build -p tone-wasm --target wasm32-unknown-unknown
```

## Architecture

Rust port of [Tone.js](https://tonejs.github.io/). Four crates in a workspace:

- **tone-core** — Platform-independent DSP engine. No external deps except `hound` for WAV decoding. All synthesis, effects, scheduling, and graph processing live here.
- **tone-cpal** — Native audio backend using cpal. Implements `AudioBackend` trait.
- **tone-wasm** — Browser audio backend using ScriptProcessorNode via wasm-bindgen/web-sys. Exports `TonesEngine` to JS.
- **tone-app** — Tauri v2 desktop app for interactive testing. Requires `withGlobalTauri: true` in tauri.conf.json and a `capabilities/default.json` for IPC permissions.

### Audio Processing Pipeline

The `AudioGraph` (graph.rs) manages a DAG of `AudioNode` implementations. On each audio callback:

1. Transport advances position and fires scheduled events
2. Graph processes nodes in topologically-sorted order (Kahn's algorithm)
3. Each node reads mixed upstream output, writes to its pre-allocated buffer
4. Output node's buffer is copied to the audio backend's output

### Real-Time Safety Contract

The `AudioNode::process()` method runs on the audio thread. Implementations **must not** allocate, lock, or perform I/O. The codebase enforces this through:

- **Atomic parameters**: All runtime-adjustable values stored as `AtomicU32` (f32 bits) with `Ordering::Relaxed`. Pattern: `store(val.to_bits())` / `f32::from_bits(load())`
- **Pre-allocated buffers**: Graph scratch buffers allocated once on topology change, reused across callbacks
- **try_lock()**: Audio callback uses `try_lock()` on the graph mutex — outputs silence on contention rather than blocking

### Node Replacement Pattern

The Tauri app and WASM engine replace nodes by creating a fresh instance and calling `graph.replace_node(id, new_node)`. This swaps the node in-place without changing graph topology (no `dirty` flag triggered). This is how note triggers work — each `play_note` creates a new Synth with a pre-scheduled envelope.

### Param Automation (signal/param.rs)

Implements Tone.js's Param system with a sorted `Timeline` of automation events. `get_value_at_time(t)` resolves the value by finding surrounding events and interpolating. Key formulas:
- Linear: `v0 + (v1 - v0) * ((t - t0) / (t1 - t0))`
- Exponential: `v0 * (v1 / v0)^((t - t0) / (t1 - t0))`
- SetTarget approach: `v1 + (v0 - v1) * e^(-(t - t0) / timeConstant)`

### Timeline Insertion Order

Events at the same time are inserted in **append order** (`partition_point(|e| e.time() <= time)`). This matters for the Envelope, where a LinearRamp at t=0.1 must come before a SetTarget at t=0.1 for correct decay behavior.

## Workspace

- Edition 2024 (Rust 2024 — note: pattern matching rules differ from 2021)
- Resolver v2
- `crates/*` glob for workspace members
