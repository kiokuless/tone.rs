use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};

use crate::graph::AudioNode;
use crate::source::player::AudioBuffer;

/// A single grain of audio.
struct Grain {
    /// Start position in the source buffer (samples).
    buf_start: usize,
    /// Current position within the grain (fractional sample).
    position: f64,
    /// Grain size in samples.
    size: usize,
    /// Whether this grain is currently active.
    active: bool,
}

impl Grain {
    /// Get the Hann window value at the given position within the grain.
    #[inline]
    fn window(&self, pos: usize) -> f32 {
        let t = pos as f32 / self.size as f32;
        0.5 * (1.0 - (2.0 * std::f32::consts::PI * t).cos())
    }
}

/// A granular synthesis player for pitch-preserving tempo change.
///
/// Reads overlapping grains from an AudioBuffer, applying Hann windowing.
/// `playback_rate` controls the speed of position advancement (tempo)
/// while grains read at normal speed (preserving pitch).
pub struct GrainPlayer {
    buffer: AudioBuffer,
    /// Current read position in the source buffer (fractional).
    /// Updated on the audio thread, readable from any thread via AtomicU64.
    position: f64,
    /// Atomic mirror of `position` for thread-safe reads (stored as f64 bits).
    position_bits: AtomicU64,
    /// Pool of grains.
    grains: Vec<Grain>,
    /// Grain size in seconds.
    grain_size_bits: AtomicU32,
    /// Overlap factor (0.0-1.0). Higher = more overlap.
    overlap_bits: AtomicU32,
    /// Playback rate (tempo control). 1.0 = original tempo.
    playback_rate_bits: AtomicU32,
    /// Samples until next grain is spawned.
    samples_until_next_grain: usize,
    /// Whether the player is active.
    playing: bool,
    /// Loop playback.
    loop_enabled: bool,
    sample_rate: u32,
}

impl GrainPlayer {
    pub fn new(buffer: AudioBuffer, sample_rate: u32) -> Self {
        let max_grains = 8;
        Self {
            buffer,
            position: 0.0,
            position_bits: AtomicU64::new(0.0f64.to_bits()),
            grains: (0..max_grains)
                .map(|_| Grain {
                    buf_start: 0,
                    position: 0.0,
                    size: 0,
                    active: false,
                })
                .collect(),
            grain_size_bits: AtomicU32::new(0.1f32.to_bits()), // 100ms default
            overlap_bits: AtomicU32::new(0.5f32.to_bits()),
            playback_rate_bits: AtomicU32::new(1.0f32.to_bits()),
            samples_until_next_grain: 0,
            playing: false,
            loop_enabled: true,
            sample_rate,
        }
    }

    pub fn start(&mut self) {
        self.playing = true;
        self.position = 0.0;
        self.samples_until_next_grain = 0;
        for grain in &mut self.grains {
            grain.active = false;
        }
    }

    pub fn stop(&mut self) {
        self.playing = false;
    }

    pub fn set_grain_size(&self, seconds: f32) {
        self.grain_size_bits
            .store(seconds.max(0.01).to_bits(), Ordering::Relaxed);
    }

    pub fn set_overlap(&self, overlap: f32) {
        self.overlap_bits
            .store(overlap.clamp(0.1, 0.9).to_bits(), Ordering::Relaxed);
    }

    pub fn set_playback_rate(&self, rate: f32) {
        self.playback_rate_bits
            .store(rate.max(0.1).to_bits(), Ordering::Relaxed);
    }

    /// Get the current playback position in seconds (thread-safe).
    pub fn get_position_seconds(&self) -> f64 {
        let pos = f64::from_bits(self.position_bits.load(Ordering::Relaxed));
        pos / self.sample_rate as f64
    }

    pub fn set_loop(&mut self, enabled: bool) {
        self.loop_enabled = enabled;
    }

    fn grain_size(&self) -> f32 {
        f32::from_bits(self.grain_size_bits.load(Ordering::Relaxed))
    }

    fn overlap(&self) -> f32 {
        f32::from_bits(self.overlap_bits.load(Ordering::Relaxed))
    }

    fn playback_rate(&self) -> f32 {
        f32::from_bits(self.playback_rate_bits.load(Ordering::Relaxed))
    }

    fn spawn_grain(&mut self) {
        let grain_samples = (self.grain_size() * self.sample_rate as f32) as usize;
        let buf_len = self.buffer.data.len();
        if buf_len == 0 || grain_samples == 0 {
            return;
        }

        let start = (self.position as usize) % buf_len;

        // Find an inactive grain slot
        if let Some(grain) = self.grains.iter_mut().find(|g| !g.active) {
            grain.buf_start = start;
            grain.position = 0.0;
            grain.size = grain_samples;
            grain.active = true;
        }
    }
}

impl AudioNode for GrainPlayer {
    fn process(&mut self, _input: &[f32], output: &mut [f32], _sample_rate: u32) {
        if !self.playing || self.buffer.is_empty() {
            output.fill(0.0);
            return;
        }

        let rate = self.playback_rate() as f64;
        let grain_samples = (self.grain_size() * self.sample_rate as f32) as usize;
        let hop = ((1.0 - self.overlap()) * grain_samples as f32) as usize;
        let hop = hop.max(1);
        let buf_len = self.buffer.data.len();

        for sample in output.iter_mut() {
            // Spawn new grain if needed
            if self.samples_until_next_grain == 0 {
                self.spawn_grain();
                self.samples_until_next_grain = hop;
            }
            self.samples_until_next_grain -= 1;

            // Mix all active grains
            let mut sum = 0.0f32;
            for grain in &mut self.grains {
                if !grain.active {
                    continue;
                }

                let pos = grain.position as usize;
                if pos >= grain.size {
                    grain.active = false;
                    continue;
                }

                // Read from buffer with wrapping
                let buf_idx = (grain.buf_start + pos) % buf_len;
                let window = grain.window(pos);
                sum += self.buffer.data[buf_idx] * window;

                grain.position += 1.0; // grains always read at original pitch
            }

            *sample = sum;

            // Advance source position based on playback rate
            self.position += rate;
            self.position_bits
                .store(self.position.to_bits(), Ordering::Relaxed);
            if self.position >= buf_len as f64 {
                if self.loop_enabled {
                    self.position -= buf_len as f64;
                } else {
                    self.playing = false;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_buffer() -> AudioBuffer {
        let sr = 44100u32;
        let data: Vec<f32> = (0..sr)
            .map(|i| (2.0 * std::f32::consts::PI * 440.0 * i as f32 / sr as f32).sin())
            .collect();
        AudioBuffer::from_samples(data, sr)
    }

    #[test]
    fn test_grain_player_produces_output() {
        let buf = test_buffer();
        let mut gp = GrainPlayer::new(buf, 44100);
        gp.start();

        let mut output = [0.0f32; 4096];
        gp.process(&[], &mut output, 44100);

        let max = output.iter().fold(0.0f32, |a, &b| a.max(b.abs()));
        assert!(max > 0.1, "grain player should produce audio: max={max}");
    }

    #[test]
    fn test_grain_player_half_speed() {
        let buf = test_buffer();
        let mut gp = GrainPlayer::new(buf.clone(), 44100);
        gp.set_playback_rate(0.5);
        gp.start();

        let mut output = [0.0f32; 8192];
        gp.process(&[], &mut output, 44100);

        // At half speed, should still produce audio (grains still active)
        let max = output.iter().fold(0.0f32, |a, &b| a.max(b.abs()));
        assert!(
            max > 0.1,
            "half-speed grain player should produce audio: max={max}"
        );
    }

    #[test]
    fn test_grain_player_position() {
        let buf = test_buffer(); // 1 second at 44100Hz
        let mut gp = GrainPlayer::new(buf, 44100);
        assert_eq!(gp.get_position_seconds(), 0.0);

        gp.start();
        let mut output = [0.0f32; 4410]; // 0.1 seconds
        gp.process(&[], &mut output, 44100);

        let pos = gp.get_position_seconds();
        assert!(
            (pos - 0.1).abs() < 0.001,
            "position should be ~0.1s after processing 4410 samples: pos={pos}"
        );
    }

    #[test]
    fn test_grain_player_double_speed() {
        let buf = test_buffer();
        let mut gp = GrainPlayer::new(buf, 44100);
        gp.set_playback_rate(2.0);
        gp.start();

        let mut output = [0.0f32; 4096];
        gp.process(&[], &mut output, 44100);

        let max = output.iter().fold(0.0f32, |a, &b| a.max(b.abs()));
        assert!(
            max > 0.1,
            "double-speed grain player should produce audio: max={max}"
        );
    }
}
