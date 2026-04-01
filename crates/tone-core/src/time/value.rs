//! Strongly-typed newtypes for audio time and pitch values.
//!
//! These types prevent unit-mismatch bugs at the API boundary while remaining
//! zero-cost (`#[repr(transparent)]`) so they can be freely used around DSP code.

use std::fmt;
use std::ops::{Add, Mul, Sub};

// ---------------------------------------------------------------------------
// Seconds
// ---------------------------------------------------------------------------

/// Time duration or position in seconds.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, PartialOrd, Default)]
pub struct Seconds(pub f64);

impl Seconds {
    pub const ZERO: Self = Self(0.0);

    /// Convert to sample count at the given sample rate.
    #[inline]
    pub fn to_samples(self, sample_rate: f64) -> Samples {
        Samples(self.0 * sample_rate)
    }

    /// Convert to milliseconds.
    #[inline]
    pub fn to_millis(self) -> f64 {
        self.0 * 1000.0
    }

    #[inline]
    pub fn as_f64(self) -> f64 {
        self.0
    }
}

impl Add for Seconds {
    type Output = Self;
    #[inline]
    fn add(self, rhs: Self) -> Self {
        Self(self.0 + rhs.0)
    }
}

impl Sub for Seconds {
    type Output = Self;
    #[inline]
    fn sub(self, rhs: Self) -> Self {
        Self(self.0 - rhs.0)
    }
}

impl Mul<f64> for Seconds {
    type Output = Self;
    #[inline]
    fn mul(self, rhs: f64) -> Self {
        Self(self.0 * rhs)
    }
}

impl fmt::Display for Seconds {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}s", self.0)
    }
}

// ---------------------------------------------------------------------------
// Hertz
// ---------------------------------------------------------------------------

/// Frequency in cycles per second (Hz).
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, PartialOrd, Default)]
pub struct Hertz(pub f64);

impl Hertz {
    /// A4 concert pitch (440 Hz).
    pub const A4: Self = Self(440.0);

    /// Convert frequency to its period in seconds.
    #[inline]
    pub fn as_period(self) -> Seconds {
        Seconds(1.0 / self.0)
    }

    /// Convert to MIDI note number (rounded to nearest).
    #[inline]
    pub fn to_midi(self) -> MidiNote {
        let midi = 69.0 + 12.0 * (self.0 / 440.0).log2();
        MidiNote(midi.round().clamp(0.0, 127.0) as u8)
    }

    /// Transpose by a number of semitones.
    #[inline]
    pub fn transpose(self, semitones: f64) -> Self {
        Self(self.0 * 2.0_f64.powf(semitones / 12.0))
    }

    /// Return frequencies for the given intervals (in semitones).
    pub fn harmonize(self, intervals: &[f64]) -> Vec<Self> {
        intervals.iter().map(|&i| self.transpose(i)).collect()
    }

    #[inline]
    pub fn as_f64(self) -> f64 {
        self.0
    }

    #[inline]
    pub fn as_f32(self) -> f32 {
        self.0 as f32
    }
}

impl fmt::Display for Hertz {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}hz", self.0)
    }
}

// ---------------------------------------------------------------------------
// MidiNote
// ---------------------------------------------------------------------------

/// MIDI note number (0–127).
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct MidiNote(pub u8);

impl MidiNote {
    /// Convert to frequency in Hz (A4 = MIDI 69 = 440 Hz, 12-TET).
    #[inline]
    pub fn to_hz(self) -> Hertz {
        Hertz(440.0 * 2.0_f64.powf((self.0 as f64 - 69.0) / 12.0))
    }

    /// Transpose by a number of semitones (clamped to 0–127).
    #[inline]
    pub fn transpose(self, semitones: i8) -> Self {
        let v = (self.0 as i16 + semitones as i16).clamp(0, 127);
        Self(v as u8)
    }

    #[inline]
    pub fn as_u8(self) -> u8 {
        self.0
    }
}

impl From<MidiNote> for Hertz {
    #[inline]
    fn from(m: MidiNote) -> Self {
        m.to_hz()
    }
}

impl fmt::Display for MidiNote {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "midi:{}", self.0)
    }
}

// ---------------------------------------------------------------------------
// Ticks
// ---------------------------------------------------------------------------

/// Transport tick count (PPQ-based).
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, PartialOrd, Default)]
pub struct Ticks(pub f64);

impl Ticks {
    pub const ZERO: Self = Self(0.0);

    #[inline]
    pub fn as_f64(self) -> f64 {
        self.0
    }
}

impl Add for Ticks {
    type Output = Self;
    #[inline]
    fn add(self, rhs: Self) -> Self {
        Self(self.0 + rhs.0)
    }
}

impl fmt::Display for Ticks {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}i", self.0)
    }
}

// ---------------------------------------------------------------------------
// Samples
// ---------------------------------------------------------------------------

/// Sample count.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, PartialOrd, Default)]
pub struct Samples(pub f64);

impl Samples {
    /// Convert back to seconds at the given sample rate.
    #[inline]
    pub fn to_seconds(self, sample_rate: f64) -> Seconds {
        Seconds(self.0 / sample_rate)
    }

    #[inline]
    pub fn as_f64(self) -> f64 {
        self.0
    }
}

impl fmt::Display for Samples {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}samples", self.0)
    }
}

// ---------------------------------------------------------------------------
// Beats
// ---------------------------------------------------------------------------

/// Duration or position in beats (quarter notes).
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, PartialOrd, Default)]
pub struct Beats(pub f64);

impl Beats {
    #[inline]
    pub fn as_f64(self) -> f64 {
        self.0
    }
}

impl Add for Beats {
    type Output = Self;
    #[inline]
    fn add(self, rhs: Self) -> Self {
        Self(self.0 + rhs.0)
    }
}

impl fmt::Display for Beats {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}beats", self.0)
    }
}

// ---------------------------------------------------------------------------
// Bpm
// ---------------------------------------------------------------------------

/// Tempo in beats per minute.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, PartialOrd, Default)]
pub struct Bpm(pub f64);

impl Bpm {
    /// Duration of one quarter note at this tempo.
    #[inline]
    pub fn quarter_duration(self) -> Seconds {
        Seconds(60.0 / self.0)
    }

    #[inline]
    pub fn as_f64(self) -> f64 {
        self.0
    }
}

impl fmt::Display for Bpm {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}bpm", self.0)
    }
}

// ---------------------------------------------------------------------------
// Conversion utilities
// ---------------------------------------------------------------------------

/// Convert a dB value to linear gain.
#[inline]
pub fn db_to_gain(db: f64) -> f64 {
    10.0_f64.powf(db / 20.0)
}

/// Convert linear gain to dB.
#[inline]
pub fn gain_to_db(gain: f64) -> f64 {
    20.0 * gain.log10()
}

/// Equal-power crossfade scaling (0.0–1.0).
#[inline]
pub fn equal_power_scale(percent: f64) -> f64 {
    (percent * std::f64::consts::FRAC_PI_2).sin()
}

/// Convert an interval in semitones to a frequency ratio.
#[inline]
pub fn interval_to_freq_ratio(semitones: f64) -> f64 {
    2.0_f64.powf(semitones / 12.0)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_seconds_to_samples() {
        let s = Seconds(1.0);
        assert_eq!(s.to_samples(44100.0), Samples(44100.0));
    }

    #[test]
    fn test_samples_to_seconds() {
        let s = Samples(44100.0);
        assert_eq!(s.to_seconds(44100.0), Seconds(1.0));
    }

    #[test]
    fn test_midi_to_hz() {
        let a4 = MidiNote(69);
        assert!((a4.to_hz().0 - 440.0).abs() < 0.01);

        let c4 = MidiNote(60);
        assert!((c4.to_hz().0 - 261.63).abs() < 0.01);
    }

    #[test]
    fn test_hz_to_midi() {
        let hz = Hertz(440.0);
        assert_eq!(hz.to_midi(), MidiNote(69));

        let hz = Hertz(261.63);
        assert_eq!(hz.to_midi(), MidiNote(60));
    }

    #[test]
    fn test_hertz_as_period() {
        let hz = Hertz(2.0);
        assert_eq!(hz.as_period(), Seconds(0.5));
    }

    #[test]
    fn test_hertz_transpose() {
        let a4 = Hertz(440.0);
        let a5 = a4.transpose(12.0);
        assert!((a5.0 - 880.0).abs() < 0.01);
    }

    #[test]
    fn test_hertz_harmonize() {
        let a4 = Hertz(440.0);
        let chord = a4.harmonize(&[0.0, 4.0, 7.0]); // major triad
        assert_eq!(chord.len(), 3);
        assert!((chord[0].0 - 440.0).abs() < 0.01);
    }

    #[test]
    fn test_midi_transpose() {
        assert_eq!(MidiNote(60).transpose(12), MidiNote(72));
        assert_eq!(MidiNote(60).transpose(-61), MidiNote(0)); // clamped
        assert_eq!(MidiNote(120).transpose(10), MidiNote(127)); // clamped
    }

    #[test]
    fn test_bpm_quarter_duration() {
        let bpm = Bpm(120.0);
        assert_eq!(bpm.quarter_duration(), Seconds(0.5));
    }

    #[test]
    fn test_seconds_arithmetic() {
        assert_eq!(Seconds(1.0) + Seconds(0.5), Seconds(1.5));
        assert_eq!(Seconds(1.0) - Seconds(0.3), Seconds(0.7));
        assert_eq!(Seconds(1.0) * 2.0, Seconds(2.0));
    }

    #[test]
    fn test_db_gain_conversions() {
        assert!((db_to_gain(0.0) - 1.0).abs() < 1e-10);
        assert!((db_to_gain(-20.0) - 0.1).abs() < 1e-10);
        assert!((gain_to_db(1.0) - 0.0).abs() < 1e-10);
    }

    #[test]
    fn test_interval_to_freq_ratio() {
        assert!((interval_to_freq_ratio(12.0) - 2.0).abs() < 1e-10);
        assert!((interval_to_freq_ratio(0.0) - 1.0).abs() < 1e-10);
    }

    #[test]
    fn test_display() {
        assert_eq!(format!("{}", Seconds(1.5)), "1.5s");
        assert_eq!(format!("{}", Hertz(440.0)), "440hz");
        assert_eq!(format!("{}", MidiNote(69)), "midi:69");
        assert_eq!(format!("{}", Ticks(480.0)), "480i");
        assert_eq!(format!("{}", Bpm(120.0)), "120bpm");
    }
}
