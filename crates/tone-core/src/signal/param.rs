use crate::util::timeline::{Timeline, TimelineEvent};

/// Types of automation events, mirroring Tone.js Param automation.
#[derive(Debug, Clone)]
pub enum AutomationEvent {
    /// Set value instantaneously at time.
    SetValue { time: f64, value: f64 },
    /// Linear ramp ending at time with target value.
    LinearRamp { time: f64, value: f64 },
    /// Exponential ramp ending at time with target value.
    ExponentialRamp { time: f64, value: f64 },
    /// Exponential approach toward value starting at time with given time constant.
    SetTarget {
        time: f64,
        value: f64,
        time_constant: f64,
    },
}

impl TimelineEvent for AutomationEvent {
    fn time(&self) -> f64 {
        match self {
            AutomationEvent::SetValue { time, .. }
            | AutomationEvent::LinearRamp { time, .. }
            | AutomationEvent::ExponentialRamp { time, .. }
            | AutomationEvent::SetTarget { time, .. } => *time,
        }
    }
}

/// Sample-accurate automatable parameter.
///
/// Port of Tone.js Param — stores automation events on a Timeline
/// and computes the value at any point in time.
pub struct Param {
    events: Timeline<AutomationEvent>,
    initial_value: f64,
}

impl Param {
    pub fn new(initial_value: f64) -> Self {
        Self {
            events: Timeline::new(),
            initial_value,
        }
    }

    pub fn initial_value(&self) -> f64 {
        self.initial_value
    }

    /// Schedule a value change at exact time.
    pub fn set_value_at_time(&mut self, value: f64, time: f64) {
        self.events.add(AutomationEvent::SetValue { time, value });
    }

    /// Schedule a linear ramp to value, ending at time.
    pub fn linear_ramp_to_value_at_time(&mut self, value: f64, time: f64) {
        self.events.add(AutomationEvent::LinearRamp { time, value });
    }

    /// Schedule an exponential ramp to value, ending at time.
    /// Both start and end values must be positive.
    pub fn exponential_ramp_to_value_at_time(&mut self, value: f64, time: f64) {
        self.events
            .add(AutomationEvent::ExponentialRamp { time, value });
    }

    /// Approach target value exponentially starting at time.
    pub fn set_target_at_time(&mut self, value: f64, start_time: f64, time_constant: f64) {
        self.events.add(AutomationEvent::SetTarget {
            time: start_time,
            value,
            time_constant,
        });
    }

    /// Convenience: linear ramp from current value to target over duration.
    pub fn linear_ramp_to(&mut self, value: f64, duration: f64, start_time: f64) {
        let current = self.get_value_at_time(start_time);
        self.set_value_at_time(current, start_time);
        self.linear_ramp_to_value_at_time(value, start_time + duration);
    }

    /// Convenience: exponential approach to target over duration.
    /// Uses the same time constant formula as Tone.js:
    /// `time_constant = ln(duration + 1) / ln(200)`
    pub fn target_ramp_to(&mut self, value: f64, duration: f64, start_time: f64) {
        let current = self.get_value_at_time(start_time);
        self.set_value_at_time(current, start_time);
        let time_constant = (duration + 1.0).ln() / 200.0_f64.ln();
        self.set_target_at_time(value, start_time, time_constant);
        // End with a set_value to ensure we reach the target precisely
        self.set_value_at_time(value, start_time + duration);
    }

    /// Cancel all scheduled events at or after `time`.
    pub fn cancel_scheduled_values(&mut self, time: f64) {
        self.events.cancel_from(time);
    }

    /// Cancel and hold: cancel events from `time` but keep the computed value.
    pub fn cancel_and_hold_at_time(&mut self, time: f64) {
        let value = self.get_value_at_time(time);
        self.events.cancel_from(time);
        self.events.add(AutomationEvent::SetValue { time, value });
    }

    /// Compute the parameter value at the given time.
    ///
    /// Implements the same algorithm as Tone.js Param.getValueAtTime.
    pub fn get_value_at_time(&self, time: f64) -> f64 {
        let time = time.max(0.0);
        let before = self.events.get(time);
        let after = self.events.get_after(time);

        match (before, after) {
            // No events before this time
            (None, _) => self.initial_value,

            // setTargetAtTime with no ramp following (or followed by setValue)
            (
                Some(AutomationEvent::SetTarget {
                    time: t0,
                    value: target,
                    time_constant,
                    ..
                }),
                after_opt,
            ) if after_opt.is_none()
                || matches!(after_opt, Some(AutomationEvent::SetValue { .. })) =>
            {
                let previous = self.events.get_before(*t0);
                let prev_val = previous.map_or(self.initial_value, event_value);
                exponential_approach(*t0, prev_val, *target, *time_constant, time)
            }

            // No event after — hold the last value
            (Some(before_evt), None) => event_value(before_evt),

            // A ramp follows
            (Some(before_evt), Some(after_evt)) => {
                let before_time = before_evt.time();
                let mut before_val = event_value(before_evt);

                // If before is setTarget, use the value before it started
                if matches!(before_evt, AutomationEvent::SetTarget { .. }) {
                    let previous = self.events.get_before(before_time);
                    before_val = previous.map_or(self.initial_value, event_value);
                }

                match after_evt {
                    AutomationEvent::LinearRamp {
                        time: t1,
                        value: v1,
                    } => linear_interpolate(before_time, before_val, *t1, *v1, time),
                    AutomationEvent::ExponentialRamp {
                        time: t1,
                        value: v1,
                    } => exponential_interpolate(before_time, before_val, *t1, *v1, time),
                    _ => before_val,
                }
            }
        }
    }

    /// Fill a buffer with sample-accurate parameter values.
    /// `start_time` is the time of the first sample.
    pub fn fill_buffer(&self, buffer: &mut [f64], start_time: f64, sample_rate: u32) {
        let sample_period = 1.0 / sample_rate as f64;
        for (i, sample) in buffer.iter_mut().enumerate() {
            let t = start_time + i as f64 * sample_period;
            *sample = self.get_value_at_time(t);
        }
    }
}

/// Extract the value from an automation event.
fn event_value(event: &AutomationEvent) -> f64 {
    match event {
        AutomationEvent::SetValue { value, .. }
        | AutomationEvent::LinearRamp { value, .. }
        | AutomationEvent::ExponentialRamp { value, .. }
        | AutomationEvent::SetTarget { value, .. } => *value,
    }
}

/// v0 + (v1 - v0) * ((t - t0) / (t1 - t0))
fn linear_interpolate(t0: f64, v0: f64, t1: f64, v1: f64, t: f64) -> f64 {
    if (t1 - t0).abs() < f64::EPSILON {
        return v1;
    }
    v0 + (v1 - v0) * ((t - t0) / (t1 - t0))
}

/// v0 * (v1 / v0)^((t - t0) / (t1 - t0))
fn exponential_interpolate(t0: f64, v0: f64, t1: f64, v1: f64, t: f64) -> f64 {
    if (t1 - t0).abs() < f64::EPSILON {
        return v1;
    }
    if v0.abs() < f64::EPSILON || v1.abs() < f64::EPSILON {
        // Exponential interpolation requires non-zero values; fall back to linear.
        return linear_interpolate(t0, v0, t1, v1, t);
    }
    v0 * (v1 / v0).powf((t - t0) / (t1 - t0))
}

/// v1 + (v0 - v1) * e^(-(t - t0) / time_constant)
fn exponential_approach(t0: f64, v0: f64, v1: f64, time_constant: f64, t: f64) -> f64 {
    v1 + (v0 - v1) * (-(t - t0) / time_constant).exp()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_initial_value() {
        let param = Param::new(440.0);
        assert_eq!(param.get_value_at_time(0.0), 440.0);
        assert_eq!(param.get_value_at_time(1.0), 440.0);
    }

    #[test]
    fn test_set_value_at_time() {
        let mut param = Param::new(0.0);
        param.set_value_at_time(1.0, 1.0);
        assert_eq!(param.get_value_at_time(0.5), 0.0);
        assert_eq!(param.get_value_at_time(1.0), 1.0);
        assert_eq!(param.get_value_at_time(2.0), 1.0);
    }

    #[test]
    fn test_linear_ramp() {
        let mut param = Param::new(0.0);
        param.set_value_at_time(0.0, 0.0);
        param.linear_ramp_to_value_at_time(1.0, 1.0);

        assert!((param.get_value_at_time(0.0) - 0.0).abs() < 1e-10);
        assert!((param.get_value_at_time(0.5) - 0.5).abs() < 1e-10);
        assert!((param.get_value_at_time(1.0) - 1.0).abs() < 1e-10);
    }

    #[test]
    fn test_exponential_ramp() {
        let mut param = Param::new(1.0);
        param.set_value_at_time(100.0, 0.0);
        param.exponential_ramp_to_value_at_time(1000.0, 1.0);

        assert!((param.get_value_at_time(0.0) - 100.0).abs() < 1e-10);
        assert!((param.get_value_at_time(1.0) - 1000.0).abs() < 1e-6);
        // Midpoint of exponential ramp: 100 * (10)^0.5 ≈ 316.23
        let mid = param.get_value_at_time(0.5);
        assert!((mid - 316.227766).abs() < 0.01);
    }

    #[test]
    fn test_set_target_at_time() {
        let mut param = Param::new(0.0);
        param.set_value_at_time(1.0, 0.0);
        // Approach 0.0 with time_constant = 0.1
        param.set_target_at_time(0.0, 0.5, 0.1);

        assert!((param.get_value_at_time(0.3) - 1.0).abs() < 1e-10);
        // At t=0.5 (start), value should still be ~1.0
        let v = param.get_value_at_time(0.5);
        assert!((v - 1.0).abs() < 0.01);
        // After several time constants, should approach 0
        let v = param.get_value_at_time(1.5);
        assert!(v < 0.01);
    }

    #[test]
    fn test_cancel_and_hold() {
        let mut param = Param::new(0.0);
        param.set_value_at_time(0.0, 0.0);
        param.linear_ramp_to_value_at_time(1.0, 1.0);

        // Cancel at 0.5 — should hold at 0.5
        param.cancel_and_hold_at_time(0.5);
        assert!((param.get_value_at_time(0.5) - 0.5).abs() < 1e-10);
        assert!((param.get_value_at_time(1.0) - 0.5).abs() < 1e-10);
    }
}
