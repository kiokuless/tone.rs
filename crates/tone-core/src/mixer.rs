use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use crate::graph::AudioNode;

/// A track in the mixer with thread-safe parameter access.
pub struct Track {
    /// The audio source for this track.
    pub node: Box<dyn AudioNode>,
    /// Track gain stored as f32 bits for atomic access.
    gain_bits: AtomicU32,
    /// Mute this track.
    mute: AtomicBool,
    /// Solo this track (if any track is soloed, only soloed tracks play).
    solo: AtomicBool,
}

impl Track {
    pub fn new(node: Box<dyn AudioNode>) -> Self {
        Self {
            node,
            gain_bits: AtomicU32::new(1.0f32.to_bits()),
            mute: AtomicBool::new(false),
            solo: AtomicBool::new(false),
        }
    }

    pub fn gain(&self) -> f32 {
        f32::from_bits(self.gain_bits.load(Ordering::Relaxed))
    }

    pub fn set_gain(&self, gain: f32) {
        self.gain_bits.store(gain.to_bits(), Ordering::Relaxed);
    }

    pub fn is_muted(&self) -> bool {
        self.mute.load(Ordering::Relaxed)
    }

    pub fn set_mute(&self, mute: bool) {
        self.mute.store(mute, Ordering::Relaxed);
    }

    pub fn is_soloed(&self) -> bool {
        self.solo.load(Ordering::Relaxed)
    }

    pub fn set_solo(&self, solo: bool) {
        self.solo.store(solo, Ordering::Relaxed);
    }
}

/// A multi-track mixer that sums multiple AudioNode sources.
///
/// Supports per-track gain, mute, and solo. Implements AudioNode
/// so it can be placed in an AudioGraph.
pub struct Mixer {
    tracks: Vec<Track>,
    /// Master gain applied after mixing.
    pub master_gain: f32,
    /// Scratch buffer for individual track output.
    track_buffer: Vec<f32>,
}

impl Mixer {
    pub fn new() -> Self {
        Self {
            tracks: Vec::new(),
            master_gain: 1.0,
            track_buffer: Vec::new(),
        }
    }

    /// Add a track and return its index.
    pub fn add_track(&mut self, track: Track) -> usize {
        let idx = self.tracks.len();
        self.tracks.push(track);
        idx
    }

    /// Get a reference to a track by index.
    pub fn track(&self, index: usize) -> Option<&Track> {
        self.tracks.get(index)
    }

    /// Get a mutable reference to a track by index.
    pub fn track_mut(&mut self, index: usize) -> Option<&mut Track> {
        self.tracks.get_mut(index)
    }

    /// Get the number of tracks.
    pub fn track_count(&self) -> usize {
        self.tracks.len()
    }

    /// Check if any track is soloed.
    fn has_solo(&self) -> bool {
        self.tracks.iter().any(|t| t.is_soloed())
    }
}

impl Default for Mixer {
    fn default() -> Self {
        Self::new()
    }
}

impl AudioNode for Mixer {
    fn process(&mut self, _input: &[f32], output: &mut [f32], sample_rate: u32) {
        let len = output.len();
        output.fill(0.0);

        if self.track_buffer.len() < len {
            self.track_buffer.resize(len, 0.0);
        }

        let has_solo = self.has_solo();

        for track in &mut self.tracks {
            if track.is_muted() {
                continue;
            }
            if has_solo && !track.is_soloed() {
                continue;
            }

            let buf = &mut self.track_buffer[..len];
            buf.fill(0.0);
            track.node.process(&[], buf, sample_rate);

            let gain = track.gain();
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::oscillator::{Oscillator, OscillatorType};

    #[test]
    fn test_mixer_sums_tracks() {
        let mut mixer = Mixer::new();
        mixer.add_track(Track::new(Box::new(Oscillator::new(
            OscillatorType::Sine,
            440.0,
        ))));
        mixer.add_track(Track::new(Box::new(Oscillator::new(
            OscillatorType::Sine,
            880.0,
        ))));

        let mut output = [0.0f32; 256];
        mixer.process(&[], &mut output, 44100);

        let max = output.iter().fold(0.0f32, |a, &b| a.max(b.abs()));
        assert!(max > 0.5, "mixer should produce audio: max={max}");
    }

    #[test]
    fn test_mixer_mute() {
        let mut mixer = Mixer::new();
        let idx = mixer.add_track(Track::new(Box::new(Oscillator::new(
            OscillatorType::Sine,
            440.0,
        ))));
        mixer.track(idx).unwrap().set_mute(true);

        let mut output = [0.0f32; 256];
        mixer.process(&[], &mut output, 44100);

        let max = output.iter().fold(0.0f32, |a, &b| a.max(b.abs()));
        assert!(max < 0.001, "muted track should be silent");
    }

    #[test]
    fn test_mixer_solo() {
        let mut mixer = Mixer::new();

        mixer.add_track(Track::new(Box::new(Oscillator::new(
            OscillatorType::Sine,
            440.0,
        ))));
        let idx = mixer.add_track(Track::new(Box::new(Oscillator::new(
            OscillatorType::Sine,
            880.0,
        ))));
        mixer.track(idx).unwrap().set_solo(true);

        let mut output = [0.0f32; 256];
        mixer.process(&[], &mut output, 44100);

        let max = output.iter().fold(0.0f32, |a, &b| a.max(b.abs()));
        assert!(max > 0.5, "solo track should produce audio");
        assert!(max <= 1.01, "only one track should be playing: max={max}");
    }

    #[test]
    fn test_mixer_track_gain() {
        let mut mixer = Mixer::new();
        let idx = mixer.add_track(Track::new(Box::new(Oscillator::new(
            OscillatorType::Sine,
            440.0,
        ))));
        mixer.track(idx).unwrap().set_gain(0.5);

        let mut output = [0.0f32; 256];
        mixer.process(&[], &mut output, 44100);

        let max = output.iter().fold(0.0f32, |a, &b| a.max(b.abs()));
        assert!(max > 0.3 && max < 0.6, "half-gain track: max={max}");
    }

    #[test]
    fn test_mixer_thread_safe_setters() {
        let mut mixer = Mixer::new();
        let idx = mixer.add_track(Track::new(Box::new(Oscillator::new(
            OscillatorType::Sine,
            440.0,
        ))));

        // These should work through immutable reference (atomic)
        let track = mixer.track(idx).unwrap();
        track.set_gain(0.7);
        track.set_mute(true);
        track.set_solo(true);

        assert!((track.gain() - 0.7).abs() < 0.01);
        assert!(track.is_muted());
        assert!(track.is_soloed());
    }
}
