/// A sorted collection of time-stamped events with efficient lookup.
///
/// Events are stored in ascending time order. Binary search is used for
/// O(log n) queries.
#[derive(Debug, Clone)]
pub struct Timeline<T: TimelineEvent> {
    events: Vec<T>,
}

/// Trait for events that can be placed on a Timeline.
pub trait TimelineEvent: Clone {
    fn time(&self) -> f64;
}

impl<T: TimelineEvent> Timeline<T> {
    pub fn new() -> Self {
        Self { events: Vec::new() }
    }

    /// Insert an event, maintaining sorted order by time.
    /// Events with the same time are kept in insertion order.
    pub fn add(&mut self, event: T) {
        let time = event.time();
        let pos = self
            .events
            .partition_point(|e| e.time() <= time);
        self.events.insert(pos, event);
    }

    /// Get the last event at or before `time`.
    pub fn get(&self, time: f64) -> Option<&T> {
        let pos = self.events.partition_point(|e| e.time() <= time);
        if pos > 0 { Some(&self.events[pos - 1]) } else { None }
    }

    /// Get the first event strictly after `time`.
    pub fn get_after(&self, time: f64) -> Option<&T> {
        let pos = self.events.partition_point(|e| e.time() <= time);
        self.events.get(pos)
    }

    /// Get the last event strictly before `time`.
    pub fn get_before(&self, time: f64) -> Option<&T> {
        let pos = self.events.partition_point(|e| e.time() < time);
        if pos > 0 { Some(&self.events[pos - 1]) } else { None }
    }

    /// Remove all events at or after `time`.
    pub fn cancel_from(&mut self, time: f64) {
        let pos = self.events.partition_point(|e| e.time() < time);
        self.events.truncate(pos);
    }

    /// Remove all events.
    pub fn clear(&mut self) {
        self.events.clear();
    }

    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    pub fn len(&self) -> usize {
        self.events.len()
    }
}

impl<T: TimelineEvent> Default for Timeline<T> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone)]
    struct TestEvent {
        time: f64,
        value: f64,
    }

    impl TimelineEvent for TestEvent {
        fn time(&self) -> f64 {
            self.time
        }
    }

    #[test]
    fn test_add_and_get() {
        let mut tl = Timeline::new();
        tl.add(TestEvent { time: 1.0, value: 10.0 });
        tl.add(TestEvent { time: 3.0, value: 30.0 });
        tl.add(TestEvent { time: 2.0, value: 20.0 });

        // get at 2.5 should return the event at 2.0
        let e = tl.get(2.5).unwrap();
        assert_eq!(e.value, 20.0);

        // get_after 2.0 should return the event at 3.0
        let e = tl.get_after(2.0).unwrap();
        assert_eq!(e.value, 30.0);

        // get_before 2.0 should return the event at 1.0
        let e = tl.get_before(2.0).unwrap();
        assert_eq!(e.value, 10.0);
    }

    #[test]
    fn test_cancel_from() {
        let mut tl = Timeline::new();
        tl.add(TestEvent { time: 1.0, value: 10.0 });
        tl.add(TestEvent { time: 2.0, value: 20.0 });
        tl.add(TestEvent { time: 3.0, value: 30.0 });

        tl.cancel_from(2.0);
        assert_eq!(tl.len(), 1);
        assert!(tl.get(1.0).is_some());
        assert!(tl.get_after(1.0).is_none());
    }
}
