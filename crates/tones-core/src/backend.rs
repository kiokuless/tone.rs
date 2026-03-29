/// Callback type for audio processing.
/// The backend calls this with a mutable buffer to fill with audio samples (mono f32).
pub type AudioCallback = Box<dyn FnMut(&mut [f32]) + Send>;

/// Platform-independent audio output abstraction.
pub trait AudioBackend {
    /// Returns the sample rate of the audio output in Hz.
    fn sample_rate(&self) -> u32;

    /// Start audio output, calling the given callback to fill buffers.
    fn start(&mut self, callback: AudioCallback);

    /// Stop audio output.
    fn stop(&mut self);
}
