//! Parseable time and pitch expressions.
//!
//! [`TimeExpr`] and [`PitchExpr`] represent *unresolved* musical expressions
//! that were parsed from strings like `"4n"`, `"+8n"`, `"@4n"`, `"C4"`, etc.
//! They are resolved into concrete [`Seconds`] / [`Hertz`] values by calling
//! [`TimeExpr::to_seconds`] or [`PitchExpr::to_hz`] with a [`TimeContext`].

use thiserror::Error;

use super::context::TimeContext;
use super::value::{Hertz, MidiNote, Samples, Seconds, Ticks};

// ---------------------------------------------------------------------------
// Error types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Error)]
pub enum TimeError {
    #[error("invalid time notation: {0}")]
    InvalidNotation(String),
}

#[derive(Debug, Clone, PartialEq, Error)]
pub enum PitchError {
    #[error("invalid pitch: {0}")]
    InvalidPitch(String),
}

// ---------------------------------------------------------------------------
// TimeExpr
// ---------------------------------------------------------------------------

/// An unresolved time expression parsed from a string.
///
/// The expression carries enough information to resolve to [`Seconds`] given a
/// [`TimeContext`] (BPM, PPQ, sample rate, current position).
#[derive(Clone, Debug, PartialEq)]
pub enum TimeExpr {
    /// Absolute time in seconds (e.g. `"1.5"`, `"0.25s"`).
    Seconds(Seconds),

    /// Musical note value (e.g. `"4n"`, `"8t"`, `"4n."`).
    NoteValue {
        divisor: f64,
        dotted: bool,
        triplet: bool,
    },

    /// Bars:Beats:Sixteenths notation (e.g. `"1:2:3"`).
    BarsBeatsSixteenths {
        bars: f64,
        beats: f64,
        sixteenths: f64,
    },

    /// Frequency-as-period (e.g. `"2hz"` → 0.5 seconds).
    Hertz(Hertz),

    /// Tick count (e.g. `"480i"`).
    Ticks(Ticks),

    /// Sample count (e.g. `"44100samples"`).
    Samples(Samples),

    /// Now-relative: current transport position + inner expression (e.g. `"+4n"`).
    NowPlus(Box<TimeExpr>),

    /// Quantise to the nearest grid subdivision (e.g. `"@4n"`).
    Quantized(Box<TimeExpr>),
}

impl TimeExpr {
    /// Parse a time notation string into a `TimeExpr`.
    pub fn parse(s: &str) -> Result<Self, TimeError> {
        let s = s.trim();
        if s.is_empty() {
            return Err(TimeError::InvalidNotation(s.to_string()));
        }

        // Now-relative prefix "+"
        if let Some(rest) = s.strip_prefix('+') {
            let inner = Self::parse(rest)?;
            return Ok(TimeExpr::NowPlus(Box::new(inner)));
        }

        // Quantize prefix "@"
        if let Some(rest) = s.strip_prefix('@') {
            let grid = Self::parse(rest)?;
            return Ok(TimeExpr::Quantized(Box::new(grid)));
        }

        // Explicit seconds suffix "s"
        if let Some(num_str) = s.strip_suffix('s')
            && let Ok(v) = num_str.parse::<f64>()
        {
            return Ok(TimeExpr::Seconds(Seconds(v)));
        }

        // Samples suffix "samples"
        if let Some(num_str) = s.strip_suffix("samples")
            && let Ok(v) = num_str.parse::<f64>()
        {
            return Ok(TimeExpr::Samples(Samples(v)));
        }

        // Tick suffix "i"
        if s.ends_with('i')
            && !s.ends_with("mi")
            && let Some(num_str) = s.strip_suffix('i')
            && let Ok(v) = num_str.parse::<f64>()
        {
            return Ok(TimeExpr::Ticks(Ticks(v)));
        }

        // Plain number → seconds
        if let Ok(v) = s.parse::<f64>() {
            return Ok(TimeExpr::Seconds(Seconds(v)));
        }

        // Hz notation: "2hz", "440hz"
        if let Some(hz_str) = s.strip_suffix("hz") {
            let hz: f64 = hz_str
                .parse()
                .map_err(|_| TimeError::InvalidNotation(s.to_string()))?;
            if hz <= 0.0 {
                return Err(TimeError::InvalidNotation(s.to_string()));
            }
            return Ok(TimeExpr::Hertz(Hertz(hz)));
        }

        // BBS notation: "1:2:3", "1:0:0", "0:1:0"
        if s.contains(':') {
            return Self::parse_bbs(s);
        }

        // Note value notation: "4n", "8t", "4n.", "2n"
        Self::parse_note_value(s)
    }

    fn parse_bbs(s: &str) -> Result<Self, TimeError> {
        let parts: Vec<&str> = s.split(':').collect();
        let err = || TimeError::InvalidNotation(s.to_string());

        match parts.len() {
            2 => {
                let bars: f64 = parts[0].parse().map_err(|_| err())?;
                let beats: f64 = parts[1].parse().map_err(|_| err())?;
                Ok(TimeExpr::BarsBeatsSixteenths {
                    bars,
                    beats,
                    sixteenths: 0.0,
                })
            }
            3 => {
                let bars: f64 = parts[0].parse().map_err(|_| err())?;
                let beats: f64 = parts[1].parse().map_err(|_| err())?;
                let sixteenths: f64 = parts[2].parse().map_err(|_| err())?;
                Ok(TimeExpr::BarsBeatsSixteenths {
                    bars,
                    beats,
                    sixteenths,
                })
            }
            _ => Err(err()),
        }
    }

    fn parse_note_value(s: &str) -> Result<Self, TimeError> {
        let err = || TimeError::InvalidNotation(s.to_string());

        let (s_trimmed, dotted) = if let Some(stripped) = s.strip_suffix('.') {
            (stripped, true)
        } else {
            (s, false)
        };

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

        Ok(TimeExpr::NoteValue {
            divisor,
            dotted,
            triplet,
        })
    }

    /// Resolve this expression to [`Seconds`] using the given [`TimeContext`].
    pub fn to_seconds(&self, ctx: &impl TimeContext) -> Result<Seconds, TimeError> {
        match self {
            TimeExpr::Seconds(v) => Ok(*v),

            TimeExpr::NoteValue {
                divisor,
                dotted,
                triplet,
            } => {
                let quarter = ctx.bpm().quarter_duration().0;
                let mut duration = (4.0 / divisor) * quarter;
                if *triplet {
                    duration *= 2.0 / 3.0;
                }
                if *dotted {
                    duration *= 1.5;
                }
                Ok(Seconds(duration))
            }

            TimeExpr::BarsBeatsSixteenths {
                bars,
                beats,
                sixteenths,
            } => {
                let quarter = ctx.bpm().quarter_duration().0;
                let (num, _den) = ctx.time_signature();
                Ok(Seconds(
                    (bars * num as f64 + beats + sixteenths * 0.25) * quarter,
                ))
            }

            TimeExpr::Hertz(hz) => Ok(hz.as_period()),

            TimeExpr::Ticks(ticks) => Ok(ctx.ticks_to_seconds(*ticks)),

            TimeExpr::Samples(samples) => Ok(ctx.samples_to_seconds(*samples)),

            TimeExpr::NowPlus(inner) => {
                let now = ctx.now_seconds();
                let delta = inner.to_seconds(ctx)?;
                Ok(now + delta)
            }

            TimeExpr::Quantized(grid) => {
                let now = ctx.now_seconds().0;
                let g = grid.to_seconds(ctx)?.0;
                if g <= 0.0 {
                    return Err(TimeError::InvalidNotation(
                        "quantize grid must be positive".into(),
                    ));
                }
                Ok(Seconds((now / g).ceil() * g))
            }
        }
    }
}

// ---------------------------------------------------------------------------
// PitchExpr
// ---------------------------------------------------------------------------

/// An unresolved pitch expression parsed from a string.
#[derive(Clone, Debug, PartialEq)]
pub enum PitchExpr {
    /// Frequency in Hz (e.g. `"440hz"`, `"440"`).
    Hertz(Hertz),
    /// MIDI note number (e.g. `"69midi"`).
    Midi(MidiNote),
    /// Note name (e.g. `"C4"`, `"A#3"`, `"Bb5"`).
    NoteName(String),
}

impl PitchExpr {
    /// Parse a pitch string into a `PitchExpr`.
    pub fn parse(s: &str) -> Result<Self, PitchError> {
        let s = s.trim();
        if s.is_empty() {
            return Err(PitchError::InvalidPitch(s.to_string()));
        }

        // MIDI suffix: "69midi"
        if let Some(num_str) = s.strip_suffix("midi") {
            if let Ok(v) = num_str.parse::<u8>()
                && v <= 127
            {
                return Ok(PitchExpr::Midi(MidiNote(v)));
            }
            return Err(PitchError::InvalidPitch(s.to_string()));
        }

        // Hz suffix: "440hz"
        if let Some(hz_str) = s.strip_suffix("hz") {
            if let Ok(v) = hz_str.parse::<f64>()
                && v > 0.0
            {
                return Ok(PitchExpr::Hertz(Hertz(v)));
            }
            return Err(PitchError::InvalidPitch(s.to_string()));
        }

        // Plain number → Hz
        if let Ok(v) = s.parse::<f64>() {
            if v > 0.0 {
                return Ok(PitchExpr::Hertz(Hertz(v)));
            }
            return Err(PitchError::InvalidPitch(s.to_string()));
        }

        // Note name (validated on resolve)
        if s.len() >= 2 && s.as_bytes()[0].to_ascii_uppercase().is_ascii_alphabetic() {
            return Ok(PitchExpr::NoteName(s.to_string()));
        }

        Err(PitchError::InvalidPitch(s.to_string()))
    }

    /// Resolve to [`Hertz`].
    pub fn to_hz(&self) -> Result<Hertz, PitchError> {
        match self {
            PitchExpr::Hertz(hz) => Ok(*hz),
            PitchExpr::Midi(m) => Ok(m.to_hz()),
            PitchExpr::NoteName(name) => {
                let midi = parse_note_name_to_midi(name)?;
                Ok(midi.to_hz())
            }
        }
    }

    /// Resolve to [`MidiNote`].
    pub fn to_midi(&self) -> Result<MidiNote, PitchError> {
        match self {
            PitchExpr::Midi(m) => Ok(*m),
            PitchExpr::Hertz(hz) => Ok(hz.to_midi()),
            PitchExpr::NoteName(name) => parse_note_name_to_midi(name),
        }
    }
}

/// Parse a note name (e.g. "C4", "A#3", "Bb5") to a [`MidiNote`].
fn parse_note_name_to_midi(note: &str) -> Result<MidiNote, PitchError> {
    let err = || PitchError::InvalidPitch(note.to_string());
    let bytes = note.as_bytes();

    if bytes.is_empty() {
        return Err(err());
    }

    let base_semitone = match bytes[0].to_ascii_uppercase() {
        b'C' => 0,
        b'D' => 2,
        b'E' => 4,
        b'F' => 5,
        b'G' => 7,
        b'A' => 9,
        b'B' => 11,
        _ => return Err(err()),
    };

    let (accidental, rest) = if bytes.len() > 1 {
        match bytes[1] {
            b'#' => (1i8, &note[2..]),
            b'b' => (-1i8, &note[2..]),
            _ => (0i8, &note[1..]),
        }
    } else {
        return Err(err());
    };

    let octave: i8 = rest.parse().map_err(|_| err())?;
    if !(0..=9).contains(&octave) {
        return Err(err());
    }

    let midi = (octave as i16 + 1) * 12 + base_semitone as i16 + accidental as i16;
    if !(0..=127).contains(&midi) {
        return Err(err());
    }

    Ok(MidiNote(midi as u8))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::time::context::StaticTimeContext;
    use crate::time::value::Bpm;

    fn ctx() -> StaticTimeContext {
        StaticTimeContext::default() // 120 BPM, 44100 Hz, ppq=192
    }

    // -- TimeExpr parsing ---------------------------------------------------

    #[test]
    fn test_parse_plain_number() {
        assert_eq!(
            TimeExpr::parse("1.5").unwrap(),
            TimeExpr::Seconds(Seconds(1.5))
        );
        assert_eq!(
            TimeExpr::parse("0").unwrap(),
            TimeExpr::Seconds(Seconds(0.0))
        );
    }

    #[test]
    fn test_parse_explicit_seconds() {
        assert_eq!(
            TimeExpr::parse("2.5s").unwrap(),
            TimeExpr::Seconds(Seconds(2.5))
        );
    }

    #[test]
    fn test_parse_note_values() {
        assert_eq!(
            TimeExpr::parse("4n").unwrap(),
            TimeExpr::NoteValue {
                divisor: 4.0,
                dotted: false,
                triplet: false
            }
        );
        assert_eq!(
            TimeExpr::parse("8t").unwrap(),
            TimeExpr::NoteValue {
                divisor: 8.0,
                dotted: false,
                triplet: true
            }
        );
        assert_eq!(
            TimeExpr::parse("4n.").unwrap(),
            TimeExpr::NoteValue {
                divisor: 4.0,
                dotted: true,
                triplet: false
            }
        );
    }

    #[test]
    fn test_parse_bbs() {
        assert_eq!(
            TimeExpr::parse("1:2:3").unwrap(),
            TimeExpr::BarsBeatsSixteenths {
                bars: 1.0,
                beats: 2.0,
                sixteenths: 3.0
            }
        );
        assert_eq!(
            TimeExpr::parse("1:2").unwrap(),
            TimeExpr::BarsBeatsSixteenths {
                bars: 1.0,
                beats: 2.0,
                sixteenths: 0.0
            }
        );
    }

    #[test]
    fn test_parse_hz() {
        assert_eq!(TimeExpr::parse("2hz").unwrap(), TimeExpr::Hertz(Hertz(2.0)));
    }

    #[test]
    fn test_parse_ticks() {
        assert_eq!(
            TimeExpr::parse("480i").unwrap(),
            TimeExpr::Ticks(Ticks(480.0))
        );
    }

    #[test]
    fn test_parse_samples() {
        assert_eq!(
            TimeExpr::parse("44100samples").unwrap(),
            TimeExpr::Samples(Samples(44100.0))
        );
    }

    #[test]
    fn test_parse_now_plus() {
        let expr = TimeExpr::parse("+4n").unwrap();
        assert_eq!(
            expr,
            TimeExpr::NowPlus(Box::new(TimeExpr::NoteValue {
                divisor: 4.0,
                dotted: false,
                triplet: false
            }))
        );
    }

    #[test]
    fn test_parse_quantized() {
        let expr = TimeExpr::parse("@4n").unwrap();
        assert_eq!(
            expr,
            TimeExpr::Quantized(Box::new(TimeExpr::NoteValue {
                divisor: 4.0,
                dotted: false,
                triplet: false
            }))
        );
    }

    #[test]
    fn test_parse_invalid() {
        assert!(TimeExpr::parse("").is_err());
        assert!(TimeExpr::parse("xyz").is_err());
        assert!(TimeExpr::parse("0hz").is_err());
    }

    // -- TimeExpr resolution ------------------------------------------------

    #[test]
    fn test_resolve_seconds() {
        let expr = TimeExpr::parse("1.5").unwrap();
        assert_eq!(expr.to_seconds(&ctx()).unwrap(), Seconds(1.5));
    }

    #[test]
    fn test_resolve_note_values() {
        let c = ctx(); // 120 BPM → quarter = 0.5s
        let q = 0.5;

        let expr = TimeExpr::parse("4n").unwrap();
        assert!((expr.to_seconds(&c).unwrap().0 - q).abs() < 1e-10);

        let expr = TimeExpr::parse("8n").unwrap();
        assert!((expr.to_seconds(&c).unwrap().0 - q * 0.5).abs() < 1e-10);

        let expr = TimeExpr::parse("1n").unwrap();
        assert!((expr.to_seconds(&c).unwrap().0 - q * 4.0).abs() < 1e-10);
    }

    #[test]
    fn test_resolve_triplet() {
        let c = ctx();
        let q = 0.5;
        let expr = TimeExpr::parse("4t").unwrap();
        assert!((expr.to_seconds(&c).unwrap().0 - q * 2.0 / 3.0).abs() < 1e-10);
    }

    #[test]
    fn test_resolve_dotted() {
        let c = ctx();
        let q = 0.5;
        let expr = TimeExpr::parse("4n.").unwrap();
        assert!((expr.to_seconds(&c).unwrap().0 - q * 1.5).abs() < 1e-10);
    }

    #[test]
    fn test_resolve_bbs() {
        let c = ctx();
        let q = 0.5;

        let expr = TimeExpr::parse("1:0:0").unwrap();
        assert!((expr.to_seconds(&c).unwrap().0 - 4.0 * q).abs() < 1e-10);

        let expr = TimeExpr::parse("1:2:1").unwrap();
        let expected = (4.0 + 2.0 + 0.25) * q;
        assert!((expr.to_seconds(&c).unwrap().0 - expected).abs() < 1e-10);
    }

    #[test]
    fn test_resolve_hz() {
        let c = ctx();
        let expr = TimeExpr::parse("2hz").unwrap();
        assert!((expr.to_seconds(&c).unwrap().0 - 0.5).abs() < 1e-10);
    }

    #[test]
    fn test_resolve_ticks() {
        let c = ctx(); // ppq=192, 120 BPM
        let expr = TimeExpr::parse("192i").unwrap();
        // 192 ticks = 1 beat = 0.5s at 120 BPM
        assert!((expr.to_seconds(&c).unwrap().0 - 0.5).abs() < 1e-10);
    }

    #[test]
    fn test_resolve_samples() {
        let c = ctx(); // 44100 Hz
        let expr = TimeExpr::parse("44100samples").unwrap();
        assert!((expr.to_seconds(&c).unwrap().0 - 1.0).abs() < 1e-10);
    }

    #[test]
    fn test_resolve_now_plus() {
        let c = StaticTimeContext::new(Bpm(120.0), 44100.0, 192, Seconds(1.0));
        let expr = TimeExpr::parse("+4n").unwrap();
        // now(1.0) + 4n(0.5) = 1.5
        assert!((expr.to_seconds(&c).unwrap().0 - 1.5).abs() < 1e-10);
    }

    #[test]
    fn test_resolve_quantized() {
        let c = StaticTimeContext::new(Bpm(120.0), 44100.0, 192, Seconds(0.3));
        let expr = TimeExpr::parse("@4n").unwrap();
        // now=0.3, grid=0.5 → ceil(0.3/0.5)*0.5 = 0.5
        assert!((expr.to_seconds(&c).unwrap().0 - 0.5).abs() < 1e-10);
    }

    // -- PitchExpr ----------------------------------------------------------

    #[test]
    fn test_parse_pitch_hz() {
        assert_eq!(
            PitchExpr::parse("440hz").unwrap(),
            PitchExpr::Hertz(Hertz(440.0))
        );
        assert_eq!(
            PitchExpr::parse("440").unwrap(),
            PitchExpr::Hertz(Hertz(440.0))
        );
    }

    #[test]
    fn test_parse_pitch_midi() {
        assert_eq!(
            PitchExpr::parse("69midi").unwrap(),
            PitchExpr::Midi(MidiNote(69))
        );
    }

    #[test]
    fn test_parse_pitch_note_name() {
        assert_eq!(
            PitchExpr::parse("C4").unwrap(),
            PitchExpr::NoteName("C4".to_string())
        );
        assert_eq!(
            PitchExpr::parse("A#3").unwrap(),
            PitchExpr::NoteName("A#3".to_string())
        );
    }

    #[test]
    fn test_pitch_to_hz() {
        let p = PitchExpr::parse("A4").unwrap();
        assert!((p.to_hz().unwrap().0 - 440.0).abs() < 0.01);

        let p = PitchExpr::parse("69midi").unwrap();
        assert!((p.to_hz().unwrap().0 - 440.0).abs() < 0.01);

        let p = PitchExpr::parse("440hz").unwrap();
        assert!((p.to_hz().unwrap().0 - 440.0).abs() < 0.01);
    }

    #[test]
    fn test_pitch_to_midi() {
        let p = PitchExpr::parse("C4").unwrap();
        assert_eq!(p.to_midi().unwrap(), MidiNote(60));

        let p = PitchExpr::parse("440hz").unwrap();
        assert_eq!(p.to_midi().unwrap(), MidiNote(69));
    }

    #[test]
    fn test_pitch_invalid() {
        assert!(PitchExpr::parse("").is_err());
        assert!(PitchExpr::parse("0hz").is_err());
        assert!(PitchExpr::parse("200midi").is_err());
    }
}
