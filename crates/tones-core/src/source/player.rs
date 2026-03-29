use std::io::Cursor;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;

use crate::graph::AudioNode;

/// An audio buffer decoded from a WAV file.
#[derive(Clone)]
pub struct AudioBuffer {
    /// Mono PCM samples in f32 [-1.0, 1.0].
    pub data: Arc<Vec<f32>>,
    /// Sample rate of the original file.
    pub sample_rate: u32,
}

impl AudioBuffer {
    /// Decode a WAV file from raw bytes.
    pub fn from_wav(bytes: &[u8]) -> Result<Self, String> {
        let cursor = Cursor::new(bytes);
        let reader = hound::WavReader::new(cursor).map_err(|e| format!("WAV decode error: {e}"))?;
        let spec = reader.spec();
        let sample_rate = spec.sample_rate;

        let data: Vec<f32> = match spec.sample_format {
            hound::SampleFormat::Int => {
                let max_val = (1u32 << (spec.bits_per_sample - 1)) as f32;
                reader
                    .into_samples::<i32>()
                    .filter_map(|s| s.ok())
                    .enumerate()
                    .filter(|(i, _)| {
                        // Take only the first channel for mono mixdown
                        *i as u16 % spec.channels == 0
                    })
                    .map(|(_, s)| s as f32 / max_val)
                    .collect()
            }
            hound::SampleFormat::Float => reader
                .into_samples::<f32>()
                .filter_map(|s| s.ok())
                .enumerate()
                .filter(|(i, _)| *i as u16 % spec.channels == 0)
                .map(|(_, s)| s)
                .collect(),
        };

        Ok(Self {
            data: Arc::new(data),
            sample_rate,
        })
    }

    /// Create a buffer from raw f32 samples.
    pub fn from_samples(data: Vec<f32>, sample_rate: u32) -> Self {
        Self {
            data: Arc::new(data),
            sample_rate,
        }
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }

    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    pub fn duration(&self) -> f64 {
        self.data.len() as f64 / self.sample_rate as f64
    }
}

/// A sample player that reads from an AudioBuffer.
///
/// Supports start/stop, looping, and variable playback rate.
pub struct Player {
    buffer: AudioBuffer,
    /// Fractional position for non-integer playback rates.
    position: f64,
    playing: AtomicBool,
    loop_enabled: AtomicBool,
    /// Playback rate stored as f32 bits. 1.0 = original speed.
    playback_rate_bits: AtomicU32,
}

impl Player {
    pub fn new(buffer: AudioBuffer) -> Self {
        Self {
            buffer,
            position: 0.0,
            playing: AtomicBool::new(false),
            loop_enabled: AtomicBool::new(false),
            playback_rate_bits: AtomicU32::new(1.0f32.to_bits()),
        }
    }

    pub fn start(&self) {
        self.playing.store(true, Ordering::Relaxed);
    }

    pub fn stop(&mut self) {
        self.playing.store(false, Ordering::Relaxed);
        self.position = 0.0;
    }

    pub fn set_loop(&self, enabled: bool) {
        self.loop_enabled.store(enabled, Ordering::Relaxed);
    }

    pub fn set_playback_rate(&self, rate: f32) {
        self.playback_rate_bits
            .store(rate.to_bits(), Ordering::Relaxed);
    }

    pub fn playback_rate(&self) -> f32 {
        f32::from_bits(self.playback_rate_bits.load(Ordering::Relaxed))
    }

    pub fn buffer(&self) -> &AudioBuffer {
        &self.buffer
    }
}

impl AudioNode for Player {
    fn process(&mut self, _input: &[f32], output: &mut [f32], _sample_rate: u32) {
        if !self.playing.load(Ordering::Relaxed) || self.buffer.is_empty() {
            output.fill(0.0);
            return;
        }

        let rate = self.playback_rate() as f64;
        let buf_len = self.buffer.data.len() as f64;
        let loop_enabled = self.loop_enabled.load(Ordering::Relaxed);

        for sample in output.iter_mut() {
            if self.position >= buf_len {
                if loop_enabled {
                    self.position -= buf_len;
                } else {
                    self.playing.store(false, Ordering::Relaxed);
                    *sample = 0.0;
                    continue;
                }
            }

            // Linear interpolation between samples
            let idx = self.position as usize;
            let frac = self.position - idx as f64;
            let s0 = self.buffer.data[idx];
            let s1 = if idx + 1 < self.buffer.data.len() {
                self.buffer.data[idx + 1]
            } else if loop_enabled {
                self.buffer.data[0]
            } else {
                0.0
            };

            *sample = s0 + (s1 - s0) * frac as f32;
            self.position += rate;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_buffer() -> AudioBuffer {
        // 1 second of 440Hz sine at 44100Hz
        let sr = 44100;
        let data: Vec<f32> = (0..sr)
            .map(|i| (2.0 * std::f32::consts::PI * 440.0 * i as f32 / sr as f32).sin())
            .collect();
        AudioBuffer::from_samples(data, sr as u32)
    }

    #[test]
    fn test_player_playback() {
        let buf = test_buffer();
        let mut player = Player::new(buf);
        player.start();

        let mut output = [0.0f32; 256];
        player.process(&[], &mut output, 44100);

        let max = output.iter().fold(0.0f32, |a, &b| a.max(b.abs()));
        assert!(max > 0.5, "player should produce audio: max={max}");
    }

    #[test]
    fn test_player_loop() {
        let buf = AudioBuffer::from_samples(vec![1.0, -1.0, 0.5], 44100);
        let mut player = Player::new(buf);
        player.set_loop(true);
        player.start();

        let mut output = [0.0f32; 9];
        player.process(&[], &mut output, 44100);

        // Should loop: 1.0, -1.0, 0.5, 1.0, -1.0, 0.5, 1.0, -1.0, 0.5
        assert!((output[0] - 1.0).abs() < 0.01);
        assert!((output[3] - 1.0).abs() < 0.01);
        assert!((output[6] - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_player_stops_without_loop() {
        let buf = AudioBuffer::from_samples(vec![1.0, 0.5], 44100);
        let mut player = Player::new(buf);
        player.start();

        let mut output = [0.0f32; 6];
        player.process(&[], &mut output, 44100);

        // After buffer ends, should be silence
        assert!((output[0] - 1.0).abs() < 0.01);
        assert!(output[4].abs() < 0.01);
    }

    #[test]
    fn test_player_playback_rate() {
        let data: Vec<f32> = (0..100).map(|i| i as f32 / 100.0).collect();
        let buf = AudioBuffer::from_samples(data, 44100);
        let mut player = Player::new(buf);
        player.set_playback_rate(2.0); // double speed
        player.start();

        let mut output = [0.0f32; 10];
        player.process(&[], &mut output, 44100);

        // At 2x speed, samples should be 0, 2, 4, 6... (with interpolation)
        assert!(output[1] > output[0]); // increasing
        // Second output sample should be roughly data[2] = 0.02
        assert!((output[1] - 0.02).abs() < 0.01);
    }
}
