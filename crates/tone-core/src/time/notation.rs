//! Legacy time notation parsing.
//!
//! This is a thin wrapper around [`TimeExpr`] and [`StaticTimeContext`].
//! Prefer using [`TimeExpr::parse`] and [`TimeExpr::to_seconds`] directly
//! in new code.

use thiserror::Error;

use super::context::StaticTimeContext;
use super::expr::{TimeError, TimeExpr};
use super::value::{Bpm, Seconds};

/// Error type for time notation parsing.
#[derive(Debug, Clone, PartialEq, Error)]
pub enum TimeParseError {
    #[error("invalid time notation: {0}")]
    InvalidNotation(String),
}

impl From<TimeError> for TimeParseError {
    fn from(e: TimeError) -> Self {
        TimeParseError::InvalidNotation(e.to_string())
    }
}

/// Parse a musical time notation string into seconds.
///
/// Supports:
/// - Plain numbers: `"1.5"` → 1.5 seconds
/// - Note values: `"1n"` (whole), `"2n"` (half), `"4n"` (quarter), `"8n"`, `"16n"`, `"32n"`, `"64n"`
/// - Triplets: `"4t"`, `"8t"` → note value × 2/3
/// - Dotted: `"4n."` → note value × 1.5
/// - Bars:beats:sixteenths: `"1:2:3"` (assumes 4/4 time)
/// - Hertz: `"2hz"` → 0.5 seconds (period)
pub fn parse_time(notation: &str, bpm: f64) -> Result<f64, TimeParseError> {
    let expr = TimeExpr::parse(notation).map_err(TimeParseError::from)?;
    let ctx = StaticTimeContext::new(Bpm(bpm), 44_100.0, 192, Seconds::ZERO);
    Ok(expr.to_seconds(&ctx).map_err(TimeParseError::from)?.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    const BPM: f64 = 120.0; // quarter = 0.5s

    #[test]
    fn test_plain_number() {
        assert_eq!(parse_time("1.5", BPM).unwrap(), 1.5);
        assert_eq!(parse_time("0", BPM).unwrap(), 0.0);
    }

    #[test]
    fn test_note_values() {
        let q = 0.5; // quarter at 120 BPM
        assert!((parse_time("1n", BPM).unwrap() - q * 4.0).abs() < 1e-10);
        assert!((parse_time("2n", BPM).unwrap() - q * 2.0).abs() < 1e-10);
        assert!((parse_time("4n", BPM).unwrap() - q).abs() < 1e-10);
        assert!((parse_time("8n", BPM).unwrap() - q * 0.5).abs() < 1e-10);
        assert!((parse_time("16n", BPM).unwrap() - q * 0.25).abs() < 1e-10);
    }

    #[test]
    fn test_triplet() {
        let q = 0.5;
        let expected = q * (2.0 / 3.0); // quarter triplet
        assert!((parse_time("4t", BPM).unwrap() - expected).abs() < 1e-10);

        let expected = q * 0.5 * (2.0 / 3.0); // eighth triplet
        assert!((parse_time("8t", BPM).unwrap() - expected).abs() < 1e-10);
    }

    #[test]
    fn test_dotted() {
        let q = 0.5;
        let expected = q * 1.5; // dotted quarter
        assert!((parse_time("4n.", BPM).unwrap() - expected).abs() < 1e-10);
    }

    #[test]
    fn test_bbs() {
        let q = 0.5;
        // 1 bar = 4 quarters = 2.0s
        assert!((parse_time("1:0:0", BPM).unwrap() - 2.0).abs() < 1e-10);
        // 0 bars, 1 beat = 0.5s
        assert!((parse_time("0:1:0", BPM).unwrap() - q).abs() < 1e-10);
        // 1 bar + 2 beats + 1 sixteenth
        let expected = (4.0 + 2.0 + 0.25) * q;
        assert!((parse_time("1:2:1", BPM).unwrap() - expected).abs() < 1e-10);
        // 2-part BBS: 1:2 = 1 bar + 2 beats
        assert!((parse_time("1:2", BPM).unwrap() - (4.0 + 2.0) * q).abs() < 1e-10);
    }

    #[test]
    fn test_hz() {
        assert!((parse_time("2hz", BPM).unwrap() - 0.5).abs() < 1e-10);
        assert!((parse_time("4hz", BPM).unwrap() - 0.25).abs() < 1e-10);
    }

    #[test]
    fn test_invalid() {
        assert!(parse_time("", BPM).is_err());
        assert!(parse_time("xyz", BPM).is_err());
        assert!(parse_time("0hz", BPM).is_err());
    }
}
