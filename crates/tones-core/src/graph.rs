use std::sync::atomic::{AtomicU64, Ordering};

/// Unique identifier for a node in the audio graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NodeId(u64);

static NEXT_NODE_ID: AtomicU64 = AtomicU64::new(0);

impl NodeId {
    pub fn new() -> Self {
        Self(NEXT_NODE_ID.fetch_add(1, Ordering::Relaxed))
    }
}

/// Trait for all audio processing nodes.
///
/// Nodes generate or process audio one buffer at a time.
/// `process` is called on the real-time audio thread — implementations
/// must not allocate, lock, or perform I/O.
pub trait AudioNode: Send {
    /// Fill or transform `buffer` with audio samples.
    /// `input` contains mixed audio from upstream nodes (empty slice if no inputs).
    fn process(&mut self, input: &[f32], output: &mut [f32], sample_rate: u32);
}

/// A connection between two nodes in the graph.
#[derive(Debug, Clone)]
struct Edge {
    from: NodeId,
    to: NodeId,
}

/// Entry for a node in the graph, holding the node and its ID.
struct NodeEntry {
    id: NodeId,
    node: Box<dyn AudioNode>,
}

/// Audio processing graph.
///
/// Manages a set of audio nodes and their connections, processing
/// them in topologically sorted order.
pub struct AudioGraph {
    nodes: Vec<NodeEntry>,
    edges: Vec<Edge>,
    /// Processing order (indices into `nodes`), updated when topology changes.
    processing_order: Vec<usize>,
    /// Scratch buffers, one per node, reused across process calls.
    /// Allocated once when topology changes to avoid real-time allocation.
    node_buffers: Vec<Vec<f32>>,
    /// Tracks whether the graph topology changed and needs resorting.
    dirty: bool,
    /// The final output node (if set).
    output_node: Option<NodeId>,
}

impl AudioGraph {
    pub fn new() -> Self {
        Self {
            nodes: Vec::new(),
            edges: Vec::new(),
            processing_order: Vec::new(),
            node_buffers: Vec::new(),
            dirty: true,
            output_node: None,
        }
    }

    /// Add a node to the graph. Returns its NodeId.
    pub fn add_node(&mut self, node: Box<dyn AudioNode>) -> NodeId {
        let id = NodeId::new();
        self.nodes.push(NodeEntry { id, node });
        self.dirty = true;
        id
    }

    /// Connect the output of `from` to the input of `to`.
    pub fn connect(&mut self, from: NodeId, to: NodeId) {
        self.edges.push(Edge { from, to });
        self.dirty = true;
    }

    /// Set which node's output is the final audio output.
    pub fn set_output(&mut self, id: NodeId) {
        self.output_node = Some(id);
    }

    /// Process the graph, writing the final output into `output`.
    /// Called from the audio thread — must not allocate when `!dirty`.
    pub fn process(&mut self, output: &mut [f32], sample_rate: u32) {
        let buffer_size = output.len();

        if self.dirty {
            self.rebuild(buffer_size);
            self.dirty = false;
        }

        // Ensure buffers match the requested size
        for buf in &mut self.node_buffers {
            if buf.len() != buffer_size {
                buf.resize(buffer_size, 0.0);
            }
        }

        // Clear all node buffers
        for buf in &mut self.node_buffers {
            buf.fill(0.0);
        }

        // Process nodes in topological order
        for &node_idx in &self.processing_order {
            let id = self.nodes[node_idx].id;

            // Mix inputs from upstream nodes into a temporary input buffer.
            // We reuse the node's own buffer area for reading input first.
            // Collect input by summing all connected source buffers.
            //
            // Safety: we split borrows carefully — read from node_buffers of
            // upstream nodes, then write to node_buffers[node_idx].
            // Since processing_order is topological, all upstream buffers
            // are already filled.

            // First, gather input into output[..buffer_size] as scratch space.
            output[..buffer_size].fill(0.0);
            let mut has_input = false;
            for edge in &self.edges {
                if edge.to == id {
                    if let Some(src_idx) = self.node_index(edge.from) {
                        for (i, sample) in output[..buffer_size].iter_mut().enumerate() {
                            *sample += self.node_buffers[src_idx][i];
                        }
                        has_input = true;
                    }
                }
            }

            let input = if has_input {
                &output[..buffer_size] as &[f32]
            } else {
                &[] as &[f32]
            };

            // We need to split the borrow: node from self.nodes, buffer from self.node_buffers.
            // Use raw pointer to avoid double borrow.
            let buf_ptr = self.node_buffers[node_idx].as_mut_ptr();
            let buf_slice = unsafe { std::slice::from_raw_parts_mut(buf_ptr, buffer_size) };
            self.nodes[node_idx].node.process(input, buf_slice, sample_rate);
        }

        // Copy output node's buffer to final output
        if let Some(out_id) = self.output_node {
            if let Some(out_idx) = self.node_index(out_id) {
                output.copy_from_slice(&self.node_buffers[out_idx][..buffer_size]);
                return;
            }
        }

        output.fill(0.0);
    }

    fn node_index(&self, id: NodeId) -> Option<usize> {
        self.nodes.iter().position(|n| n.id == id)
    }

    /// Rebuild processing order (topological sort) and pre-allocate buffers.
    fn rebuild(&mut self, buffer_size: usize) {
        let n = self.nodes.len();

        // Build adjacency and in-degree
        let mut in_degree = vec![0usize; n];
        let mut adj: Vec<Vec<usize>> = vec![Vec::new(); n];

        for edge in &self.edges {
            if let (Some(from_idx), Some(to_idx)) =
                (self.node_index(edge.from), self.node_index(edge.to))
            {
                adj[from_idx].push(to_idx);
                in_degree[to_idx] += 1;
            }
        }

        // Kahn's algorithm
        let mut queue: Vec<usize> = in_degree
            .iter()
            .enumerate()
            .filter(|&(_, d)| *d == 0)
            .map(|(i, _)| i)
            .collect();
        let mut order = Vec::with_capacity(n);

        while let Some(node) = queue.pop() {
            order.push(node);
            for &neighbor in &adj[node] {
                in_degree[neighbor] -= 1;
                if in_degree[neighbor] == 0 {
                    queue.push(neighbor);
                }
            }
        }

        self.processing_order = order;

        // Pre-allocate buffers
        self.node_buffers.resize_with(n, || vec![0.0; buffer_size]);
        for buf in &mut self.node_buffers {
            buf.resize(buffer_size, 0.0);
        }
    }
}
