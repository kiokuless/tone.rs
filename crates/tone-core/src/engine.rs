use std::sync::{Arc, Mutex};

use crate::backend::AudioBackend;
use crate::graph::{AudioGraph, AudioNode, NodeId};

/// High-level audio engine that connects an AudioGraph to an AudioBackend.
pub struct AudioEngine {
    graph: Arc<Mutex<AudioGraph>>,
    sample_rate: u32,
}

impl AudioEngine {
    /// Create a new engine with the given backend.
    /// The backend's sample rate is captured but audio doesn't start until `start()`.
    pub fn new(backend: &dyn AudioBackend) -> Self {
        Self {
            graph: Arc::new(Mutex::new(AudioGraph::new())),
            sample_rate: backend.sample_rate(),
        }
    }

    /// Add a node to the audio graph. Returns its NodeId.
    pub fn add_node(&self, node: Box<dyn AudioNode>) -> NodeId {
        self.graph.lock().unwrap().add_node(node)
    }

    /// Connect the output of `from` to the input of `to`.
    pub fn connect(&self, from: NodeId, to: NodeId) {
        self.graph.lock().unwrap().connect(from, to);
    }

    /// Set a node as the final output of the graph.
    pub fn set_output(&self, id: NodeId) {
        self.graph.lock().unwrap().set_output(id);
    }

    /// Start audio processing on the given backend.
    pub fn start(&self, backend: &mut dyn AudioBackend) {
        let graph = Arc::clone(&self.graph);
        let sample_rate = self.sample_rate;

        backend.start(Box::new(move |buffer: &mut [f32]| {
            if let Ok(mut g) = graph.try_lock() {
                g.process(buffer, sample_rate);
            } else {
                // If we can't acquire the lock, output silence.
                buffer.fill(0.0);
            }
        }));
    }
}
