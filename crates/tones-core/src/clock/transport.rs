use std::sync::atomic::{AtomicU32, AtomicU64, AtomicU8, Ordering};
use std::sync::{Arc, Mutex};

use crate::time::notation::parse_time;

/// Playback state of the Transport.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum PlaybackState {
    Stopped = 0,
    Started = 1,
    Paused = 2,
}

impl From<u8> for PlaybackState {
    fn from(v: u8) -> Self {
        match v {
            1 => PlaybackState::Started,
            2 => PlaybackState::Paused,
            _ => PlaybackState::Stopped,
        }
    }
}

/// A scheduled event on the transport timeline.
#[derive(Clone)]
struct ScheduledEvent {
    id: u64,
    /// Time in seconds (transport time) when this event fires.
    time: f64,
    /// For repeating events: interval in seconds. None for one-shot.
    interval: Option<f64>,
    /// Whether this event should be removed after firing once.
    once: bool,
    /// The callback, shared so it can be called from the audio thread.
    callback: Arc<Mutex<Box<dyn FnMut(f64) + Send>>>,
}

/// Master transport for tempo-synced scheduling.
///
/// Provides play/pause/stop control, BPM management, and the ability
/// to schedule callbacks at specific transport times. Designed to be
/// advanced from the audio thread via `advance()`.
pub struct Transport {
    /// Current playback state (atomic for lock-free read from audio thread).
    state: AtomicU8,
    /// BPM stored as f32 bits for atomic access.
    bpm_bits: AtomicU32,
    /// Current transport position in samples (atomic for audio thread).
    position_samples: AtomicU64,
    /// Sample rate.
    sample_rate: u32,
    /// Scheduled events.
    events: Mutex<Vec<ScheduledEvent>>,
    /// Next event ID counter.
    next_id: AtomicU64,
    /// Loop enabled.
    loop_enabled: std::sync::atomic::AtomicBool,
    /// Loop start in seconds (stored as f64 bits).
    loop_start_bits: AtomicU64,
    /// Loop end in seconds (stored as f64 bits).
    loop_end_bits: AtomicU64,
}

impl Transport {
    pub fn new(sample_rate: u32) -> Self {
        Self {
            state: AtomicU8::new(PlaybackState::Stopped as u8),
            bpm_bits: AtomicU32::new(120.0f32.to_bits()),
            position_samples: AtomicU64::new(0),
            sample_rate,
            events: Mutex::new(Vec::new()),
            next_id: AtomicU64::new(0),
            loop_enabled: std::sync::atomic::AtomicBool::new(false),
            loop_start_bits: AtomicU64::new(0.0f64.to_bits()),
            loop_end_bits: AtomicU64::new(0.0f64.to_bits()),
        }
    }

    /// Start playback.
    pub fn start(&self) {
        self.state
            .store(PlaybackState::Started as u8, Ordering::Release);
    }

    /// Pause playback (position is retained).
    pub fn pause(&self) {
        self.state
            .store(PlaybackState::Paused as u8, Ordering::Release);
    }

    /// Stop playback and reset position to 0.
    pub fn stop(&self) {
        self.state
            .store(PlaybackState::Stopped as u8, Ordering::Release);
        self.position_samples.store(0, Ordering::Release);
    }

    /// Get the current playback state.
    pub fn state(&self) -> PlaybackState {
        PlaybackState::from(self.state.load(Ordering::Acquire))
    }

    /// Get the current BPM.
    pub fn bpm(&self) -> f64 {
        f32::from_bits(self.bpm_bits.load(Ordering::Relaxed)) as f64
    }

    /// Set the BPM.
    pub fn set_bpm(&self, bpm: f64) {
        self.bpm_bits
            .store((bpm as f32).to_bits(), Ordering::Relaxed);
    }

    /// Get the current transport position in seconds.
    pub fn position(&self) -> f64 {
        self.position_samples.load(Ordering::Relaxed) as f64 / self.sample_rate as f64
    }

    /// Set loop boundaries and enable looping.
    pub fn set_loop(&self, start: f64, end: f64) {
        self.loop_start_bits
            .store(start.to_bits(), Ordering::Relaxed);
        self.loop_end_bits
            .store(end.to_bits(), Ordering::Relaxed);
        self.loop_enabled
            .store(true, Ordering::Relaxed);
    }

    /// Disable looping.
    pub fn disable_loop(&self) {
        self.loop_enabled.store(false, Ordering::Relaxed);
    }

    /// Schedule a callback at a specific transport time (in seconds).
    /// Returns an event ID that can be used to cancel it.
    pub fn schedule(&self, callback: impl FnMut(f64) + Send + 'static, time: f64) -> u64 {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let event = ScheduledEvent {
            id,
            time,
            interval: None,
            once: false,
            callback: Arc::new(Mutex::new(Box::new(callback))),
        };
        self.events.lock().unwrap().push(event);
        id
    }

    /// Schedule a callback to fire once at a specific time, then auto-remove.
    pub fn schedule_once(&self, callback: impl FnMut(f64) + Send + 'static, time: f64) -> u64 {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let event = ScheduledEvent {
            id,
            time,
            interval: None,
            once: true,
            callback: Arc::new(Mutex::new(Box::new(callback))),
        };
        self.events.lock().unwrap().push(event);
        id
    }

    /// Schedule a repeating callback at a given interval.
    /// `start_time` is when the first invocation happens.
    pub fn schedule_repeat(
        &self,
        callback: impl FnMut(f64) + Send + 'static,
        interval: f64,
        start_time: f64,
    ) -> u64 {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let event = ScheduledEvent {
            id,
            time: start_time,
            interval: Some(interval),
            once: false,
            callback: Arc::new(Mutex::new(Box::new(callback))),
        };
        self.events.lock().unwrap().push(event);
        id
    }

    /// Schedule using musical time notation for both time and interval.
    pub fn schedule_repeat_notation(
        &self,
        callback: impl FnMut(f64) + Send + 'static,
        interval_notation: &str,
        start_notation: &str,
    ) -> u64 {
        let bpm = self.bpm();
        let interval = parse_time(interval_notation, bpm).unwrap_or(0.5);
        let start = parse_time(start_notation, bpm).unwrap_or(0.0);
        self.schedule_repeat(callback, interval, start)
    }

    /// Cancel a scheduled event by ID.
    pub fn clear(&self, id: u64) {
        self.events.lock().unwrap().retain(|e| e.id != id);
    }

    /// Cancel all scheduled events.
    pub fn clear_all(&self) {
        self.events.lock().unwrap().clear();
    }

    /// Advance the transport by `num_samples` and fire any due events.
    ///
    /// Call this from the audio thread once per buffer. Events whose
    /// scheduled time falls within `[current_pos, current_pos + buffer_duration)`
    /// will be fired.
    pub fn advance(&self, num_samples: u32) {
        if self.state() != PlaybackState::Started {
            return;
        }

        let pos_before = self.position();
        let buffer_duration = num_samples as f64 / self.sample_rate as f64;
        let pos_after = pos_before + buffer_duration;

        // Fire events
        if let Ok(mut events) = self.events.try_lock() {
            let mut to_remove = Vec::new();

            for event in events.iter_mut() {
                if event.time >= pos_before && event.time <= pos_after {
                    if let Ok(mut cb) = event.callback.try_lock() {
                        cb(event.time);
                    }

                    if event.once {
                        to_remove.push(event.id);
                    } else if let Some(interval) = event.interval {
                        // Schedule next occurrence
                        event.time += interval;
                    }
                }
            }

            events.retain(|e| !to_remove.contains(&e.id));
        }

        // Advance position
        let new_samples =
            self.position_samples.load(Ordering::Relaxed) + num_samples as u64;
        self.position_samples
            .store(new_samples, Ordering::Relaxed);

        // Handle looping
        if self.loop_enabled.load(Ordering::Relaxed) {
            let loop_end = f64::from_bits(self.loop_end_bits.load(Ordering::Relaxed));
            let loop_start = f64::from_bits(self.loop_start_bits.load(Ordering::Relaxed));
            if loop_end > loop_start && self.position() >= loop_end {
                let loop_start_samples = (loop_start * self.sample_rate as f64) as u64;
                self.position_samples
                    .store(loop_start_samples, Ordering::Relaxed);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicU32;

    #[test]
    fn test_transport_state() {
        let t = Transport::new(44100);
        assert_eq!(t.state(), PlaybackState::Stopped);

        t.start();
        assert_eq!(t.state(), PlaybackState::Started);

        t.pause();
        assert_eq!(t.state(), PlaybackState::Paused);

        t.stop();
        assert_eq!(t.state(), PlaybackState::Stopped);
        assert_eq!(t.position(), 0.0);
    }

    #[test]
    fn test_transport_advance() {
        let t = Transport::new(44100);
        t.start();

        // Advance 44100 samples = 1 second
        t.advance(44100);
        assert!((t.position() - 1.0).abs() < 0.001);

        // Advance another 22050 = 0.5s
        t.advance(22050);
        assert!((t.position() - 1.5).abs() < 0.001);
    }

    #[test]
    fn test_transport_schedule() {
        let t = Transport::new(44100);
        let counter = Arc::new(AtomicU32::new(0));

        let c = counter.clone();
        t.schedule(move |_time| {
            c.fetch_add(1, Ordering::Relaxed);
        }, 0.5);

        t.start();

        // Advance to 0.25s — event at 0.5s should not fire
        t.advance(11025);
        assert_eq!(counter.load(Ordering::Relaxed), 0);

        // Advance to 0.5s — event should fire
        t.advance(11025);
        assert_eq!(counter.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn test_transport_schedule_repeat() {
        let t = Transport::new(44100);
        let counter = Arc::new(AtomicU32::new(0));

        let c = counter.clone();
        t.schedule_repeat(
            move |_time| {
                c.fetch_add(1, Ordering::Relaxed);
            },
            0.5, // every 0.5s
            0.0, // starting at 0
        );

        t.start();

        // Process 4 half-second buffers (2 seconds total)
        for _ in 0..4 {
            t.advance(22050);
        }

        // Should have fired at 0.0, 0.5, 1.0, 1.5
        assert_eq!(counter.load(Ordering::Relaxed), 4);
    }

    #[test]
    fn test_transport_loop() {
        let t = Transport::new(44100);
        t.set_loop(0.0, 1.0); // loop 0-1s
        t.start();

        // Advance 1.5s
        t.advance(44100); // 1s
        // After 1s, should loop back to 0
        assert!(t.position() < 0.1);

        t.advance(22050); // 0.5s
        assert!((t.position() - 0.5).abs() < 0.01);
    }

    #[test]
    fn test_transport_schedule_once() {
        let t = Transport::new(44100);
        let counter = Arc::new(AtomicU32::new(0));

        let c = counter.clone();
        t.schedule_once(move |_time| {
            c.fetch_add(1, Ordering::Relaxed);
        }, 0.5);

        t.start();
        t.advance(22050); // 0.5s — should fire
        assert_eq!(counter.load(Ordering::Relaxed), 1);

        t.advance(22050); // 1.0s — should not fire again
        assert_eq!(counter.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn test_transport_bpm() {
        let t = Transport::new(44100);
        assert_eq!(t.bpm(), 120.0);

        t.set_bpm(140.0);
        assert!((t.bpm() - 140.0).abs() < 0.1);
    }
}
