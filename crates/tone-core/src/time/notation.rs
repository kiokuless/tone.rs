use std::fmt;

/// Error type for time notation parsing.
#[derive(Debug, Clone, PartialEq)]
pub enum TimeParseError {
    InvalidNotation(String),
}

impl fmt::Display for TimeParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TimeParseError::InvalidNotation(s) => write!(f, "invalid time notation: {s}"),
        }
    }
}

impl std::error::Error for TimeParseError {}

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
    let s = notation.trim();

    if s.is_empty() {
        return Err(TimeParseError::InvalidNotation(s.to_string()));
    }

    // Try plain number first
    if let Ok(v) = s.parse::<f64>() {
        return Ok(v);
    }

    // Hz notation: "2hz", "440hz"
    if let Some(hz_str) = s.strip_suffix("hz") {
        let hz: f64 = hz_str
            .parse()
            .map_err(|_| TimeParseError::InvalidNotation(s.to_string()))?;
        if hz <= 0.0 {
            return Err(TimeParseError::InvalidNotation(s.to_string()));
        }
        return Ok(1.0 / hz);
    }

    // BBS notation: "1:2:3", "1:0:0", "0:1:0"
    if s.contains(':') {
        return parse_bbs(s, bpm);
    }

    // Note value notation: "4n", "8t", "4n.", "2n"
    parse_note_value(s, bpm)
}

/// Parse bars:beats:sixteenths notation (assumes 4/4 time).
fn parse_bbs(s: &str, bpm: f64) -> Result<f64, TimeParseError> {
    let parts: Vec<&str> = s.split(':').collect();
    let err = || TimeParseError::InvalidNotation(s.to_string());

    let quarter = 60.0 / bpm;

    match parts.len() {
        2 => {
            // bars:beats
            let bars: f64 = parts[0].parse().map_err(|_| err())?;
            let beats: f64 = parts[1].parse().map_err(|_| err())?;
            Ok((bars * 4.0 + beats) * quarter)
        }
        3 => {
            // bars:beats:sixteenths
            let bars: f64 = parts[0].parse().map_err(|_| err())?;
            let beats: f64 = parts[1].parse().map_err(|_| err())?;
            let sixteenths: f64 = parts[2].parse().map_err(|_| err())?;
            Ok((bars * 4.0 + beats + sixteenths * 0.25) * quarter)
        }
        _ => Err(err()),
    }
}

/// Parse note value notation.
fn parse_note_value(s: &str, bpm: f64) -> Result<f64, TimeParseError> {
    let err = || TimeParseError::InvalidNotation(s.to_string());
    let quarter = 60.0 / bpm;

    // Check for dotted notation (trailing ".")
    let (s_trimmed, dotted) = if let Some(stripped) = s.strip_suffix('.') {
        (stripped, true)
    } else {
        (s, false)
    };

    // Check suffix: "n" for normal, "t" for triplet
    let (num_str, triplet) = if let Some(n) = s_trimmed.strip_suffix('n') {
        (n, false)
    } else if let Some(n) = s_trimmed.strip_suffix('t') {
        (n, true)
    } else {
        return Err(err());
    };

    let divisor: f64 = num_str.parse().map_err(|_| err())?;
    if divisor <= 0.0 {
        return Err(err());
    }

    // A whole note (1n) = 4 quarter notes
    let mut duration = (4.0 / divisor) * quarter;

    if triplet {
        duration *= 2.0 / 3.0;
    }
    if dotted {
        duration *= 1.5;
    }

    Ok(duration)
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
