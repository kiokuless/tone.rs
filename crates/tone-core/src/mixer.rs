use crate::graph::AudioNode;

/// A track in the mixer.
pub struct Track {
    /// The audio source for this track.
    pub node: Box<dyn AudioNode>,
    /// Track gain (0.0-1.0).
    pub gain: f32,
    /// Mute this track.
    pub mute: bool,
    /// Solo this track (if any track is soloed, only soloed tracks play).
    pub solo: bool,
}

impl Track {
    pub fn new(node: Box<dyn AudioNode>) -> Self {
        Self {
            node,
            gain: 1.0,
            mute: false,
            solo: false,
        }
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
        self.tracks.iter().any(|t| t.solo)
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
            // Skip muted tracks
            if track.mute {
                continue;
            }
            // If any track is soloed, only play soloed tracks
            if has_solo && !track.solo {
                continue;
            }

            let buf = &mut self.track_buffer[..len];
            buf.fill(0.0);
            track.node.process(&[], buf, sample_rate);

            let gain = track.gain;
            for (out, &t) in output.iter_mut().zip(buf.iter()) {
                *out += t * gain;
            }
        }

        // Apply master gain
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
        mixer.track_mut(idx).unwrap().mute = true;

        let mut output = [0.0f32; 256];
        mixer.process(&[], &mut output, 44100);

        let max = output.iter().fold(0.0f32, |a, &b| a.max(b.abs()));
        assert!(max < 0.001, "muted track should be silent");
    }

    #[test]
    fn test_mixer_solo() {
        let mut mixer = Mixer::new();

        // Track 0: 440Hz (not soloed)
        mixer.add_track(Track::new(Box::new(Oscillator::new(
            OscillatorType::Sine,
            440.0,
        ))));
        // Track 1: 880Hz (soloed)
        let idx = mixer.add_track(Track::new(Box::new(Oscillator::new(
            OscillatorType::Sine,
            880.0,
        ))));
        mixer.track_mut(idx).unwrap().solo = true;

        let mut output = [0.0f32; 256];
        mixer.process(&[], &mut output, 44100);

        // Only the 880Hz track should play
        let max = output.iter().fold(0.0f32, |a, &b| a.max(b.abs()));
        assert!(max > 0.5, "solo track should produce audio");
        // Max should be <= 1.0 (single oscillator, not summed)
        assert!(max <= 1.01, "only one track should be playing: max={max}");
    }

    #[test]
    fn test_mixer_track_gain() {
        let mut mixer = Mixer::new();
        let idx = mixer.add_track(Track::new(Box::new(Oscillator::new(
            OscillatorType::Sine,
            440.0,
        ))));
        mixer.track_mut(idx).unwrap().gain = 0.5;

        let mut output = [0.0f32; 256];
        mixer.process(&[], &mut output, 44100);

        let max = output.iter().fold(0.0f32, |a, &b| a.max(b.abs()));
        assert!(max > 0.3 && max < 0.6, "half-gain track: max={max}");
    }
}
