use std::sync::{Arc, Mutex};

use crate::clock::Transport;
use crate::time::notation::parse_time;

/// A step in a sequence. `None` represents a rest.
#[derive(Debug, Clone)]
pub struct Step {
    pub note: Option<String>,
    pub duration: String,
}

impl Step {
    pub fn note(note: &str, duration: &str) -> Self {
        Self {
            note: Some(note.to_string()),
            duration: duration.to_string(),
        }
    }

    pub fn rest(duration: &str) -> Self {
        Self {
            note: None,
            duration: duration.to_string(),
        }
    }
}

/// A sequence of musical events that plays on a Transport.
///
/// Each step has a note (or rest) and a duration. The sequence
/// schedules callbacks on the transport at the appropriate times.
pub struct Sequence {
    steps: Vec<Step>,
    event_ids: Vec<u64>,
}

impl Sequence {
    pub fn new(steps: Vec<Step>) -> Self {
        Self {
            steps,
            event_ids: Vec::new(),
        }
    }

    /// Schedule all steps on the given transport.
    /// Calls `on_step(note, duration_secs, time)` for each non-rest step.
    pub fn schedule_on<F>(
        &mut self,
        transport: &Transport,
        on_step: F,
    ) where
        F: Fn(String, f64, f64) + Send + 'static,
    {
        let bpm = transport.bpm();
        let on_step = Arc::new(Mutex::new(on_step));

        let mut current_time = 0.0;

        for step in &self.steps {
            let dur_secs = parse_time(&step.duration, bpm).unwrap_or(0.5);

            if let Some(ref note) = step.note {
                let note = note.clone();
                let dur = dur_secs;
                let cb = on_step.clone();

                let id = transport.schedule_once(
                    move |time| {
                        if let Ok(f) = cb.lock() {
                            f(note.clone(), dur, time);
                        }
                    },
                    current_time,
                );
                self.event_ids.push(id);
            }

            current_time += dur_secs;
        }
    }

    /// Cancel all scheduled events for this sequence.
    pub fn cancel(&mut self, transport: &Transport) {
        for id in self.event_ids.drain(..) {
            transport.clear(id);
        }
    }

    /// Get the total duration of the sequence in seconds at the given BPM.
    pub fn duration(&self, bpm: f64) -> f64 {
        self.steps
            .iter()
            .map(|s| parse_time(&s.duration, bpm).unwrap_or(0.5))
            .sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    #[test]
    fn test_sequence_schedule() {
        let transport = Transport::new(44100);
        transport.set_bpm(120.0);

        let steps = vec![
            Step::note("C4", "4n"),
            Step::note("E4", "4n"),
            Step::rest("4n"),
            Step::note("G4", "4n"),
        ];

        let counter = Arc::new(AtomicU32::new(0));
        let c = counter.clone();

        let mut seq = Sequence::new(steps);
        seq.schedule_on(&transport, move |_note, _dur, _time| {
            c.fetch_add(1, Ordering::Relaxed);
        });

        transport.start();

        // At 120 BPM, 4n = 0.5s. Total sequence = 2.0s.
        // Process in 0.5s chunks (22050 samples)
        for _ in 0..4 {
            transport.advance(22050);
        }

        // 3 notes (one rest), all should have fired
        assert_eq!(counter.load(Ordering::Relaxed), 3);
    }

    #[test]
    fn test_sequence_duration() {
        let steps = vec![
            Step::note("C4", "4n"),
            Step::note("E4", "8n"),
            Step::rest("4n"),
        ];
        let seq = Sequence::new(steps);
        // At 120 BPM: 4n=0.5, 8n=0.25, 4n=0.5 → total=1.25
        assert!((seq.duration(120.0) - 1.25).abs() < 0.001);
    }
}
