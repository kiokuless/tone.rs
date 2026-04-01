//! Audio time context for BPM-dependent conversions.
//!
//! Types that need to resolve musical time notation (e.g. "4n") into concrete
//! seconds require a [`TimeContext`] that provides BPM, sample rate, and other
//! transport-level state.

use super::value::{Beats, Bpm, Samples, Seconds, Ticks};

/// Trait providing the tempo/transport state needed to resolve musical time
/// expressions into concrete values.
///
/// Implemented by `Transport` and by the lightweight [`StaticTimeContext`] for
/// contexts where no live transport is available (tests, offline rendering).
pub trait TimeContext {
    /// Current tempo.
    fn bpm(&self) -> Bpm;

    /// Output sample rate in Hz.
    fn sample_rate(&self) -> f64;

    /// Pulses (ticks) per quarter note. Default is 192.
    fn ppq(&self) -> u32 {
        192
    }

    /// Current transport position in seconds. Returns `Seconds::ZERO` when
    /// there is no live transport.
    fn now_seconds(&self) -> Seconds {
        Seconds::ZERO
    }

    /// Time signature as (numerator, denominator). Default is (4, 4).
    fn time_signature(&self) -> (u8, u8) {
        (4, 4)
    }

    // -- derived helpers (default impls) ------------------------------------

    /// Duration of one quarter note at the current tempo.
    fn quarter_duration(&self) -> Seconds {
        self.bpm().quarter_duration()
    }

    /// Convert beats (quarter notes) to seconds.
    fn beats_to_seconds(&self, beats: Beats) -> Seconds {
        Seconds(beats.0 * self.quarter_duration().0)
    }

    /// Convert seconds to beats (quarter notes).
    fn seconds_to_beats(&self, secs: Seconds) -> Beats {
        Beats(secs.0 / self.quarter_duration().0)
    }

    /// Convert seconds to ticks.
    fn seconds_to_ticks(&self, secs: Seconds) -> Ticks {
        let beats = self.seconds_to_beats(secs);
        Ticks(beats.0 * self.ppq() as f64)
    }

    /// Convert ticks to seconds.
    fn ticks_to_seconds(&self, ticks: Ticks) -> Seconds {
        let beats = Beats(ticks.0 / self.ppq() as f64);
        self.beats_to_seconds(beats)
    }

    /// Convert seconds to sample count.
    fn seconds_to_samples(&self, secs: Seconds) -> Samples {
        secs.to_samples(self.sample_rate())
    }

    /// Convert sample count to seconds.
    fn samples_to_seconds(&self, samples: Samples) -> Seconds {
        samples.to_seconds(self.sample_rate())
    }
}

/// A simple, immutable [`TimeContext`] for use in tests and offline scenarios.
#[derive(Clone, Debug)]
pub struct StaticTimeContext {
    pub bpm: Bpm,
    pub sample_rate: f64,
    pub ppq: u32,
    pub now: Seconds,
    pub time_signature: (u8, u8),
}

impl StaticTimeContext {
    pub fn new(bpm: Bpm, sample_rate: f64, ppq: u32, now: Seconds) -> Self {
        Self {
            bpm,
            sample_rate,
            ppq,
            now,
            time_signature: (4, 4),
        }
    }
}

impl Default for StaticTimeContext {
    fn default() -> Self {
        Self {
            bpm: Bpm(120.0),
            sample_rate: 44_100.0,
            ppq: 192,
            now: Seconds::ZERO,
            time_signature: (4, 4),
        }
    }
}

impl TimeContext for StaticTimeContext {
    fn bpm(&self) -> Bpm {
        self.bpm
    }
    fn sample_rate(&self) -> f64 {
        self.sample_rate
    }
    fn ppq(&self) -> u32 {
        self.ppq
    }
    fn now_seconds(&self) -> Seconds {
        self.now
    }
    fn time_signature(&self) -> (u8, u8) {
        self.time_signature
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_static_context_defaults() {
        let ctx = StaticTimeContext::default();
        assert_eq!(ctx.bpm(), Bpm(120.0));
        assert_eq!(ctx.sample_rate(), 44_100.0);
        assert_eq!(ctx.ppq(), 192);
    }

    #[test]
    fn test_beats_to_seconds() {
        let ctx = StaticTimeContext::default(); // 120 BPM → quarter = 0.5s
        assert_eq!(ctx.beats_to_seconds(Beats(1.0)), Seconds(0.5));
        assert_eq!(ctx.beats_to_seconds(Beats(4.0)), Seconds(2.0));
    }

    #[test]
    fn test_seconds_to_ticks() {
        let ctx = StaticTimeContext::default(); // 120 BPM, ppq=192
        // 0.5s = 1 beat = 192 ticks
        let ticks = ctx.seconds_to_ticks(Seconds(0.5));
        assert!((ticks.0 - 192.0).abs() < 1e-10);
    }

    #[test]
    fn test_ticks_to_seconds() {
        let ctx = StaticTimeContext::default();
        let secs = ctx.ticks_to_seconds(Ticks(192.0));
        assert!((secs.0 - 0.5).abs() < 1e-10);
    }

    #[test]
    fn test_seconds_to_samples() {
        let ctx = StaticTimeContext::default();
        let samples = ctx.seconds_to_samples(Seconds(1.0));
        assert_eq!(samples, Samples(44_100.0));
    }
}
