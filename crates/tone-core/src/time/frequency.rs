//! Legacy frequency/MIDI conversion functions.
//!
//! These are thin wrappers around the new type system in [`super::value`] and
//! [`super::expr`]. Prefer using [`Hertz`], [`MidiNote`], and [`PitchExpr`]
//! directly in new code.

use thiserror::Error;

use super::expr::{PitchError, PitchExpr};
use super::value::{Hertz, MidiNote};

/// Error type for note name parsing.
#[derive(Debug, Clone, PartialEq, Error)]
pub enum NoteParseError {
    #[error("invalid note: {0}")]
    InvalidNote(String),
}

impl From<PitchError> for NoteParseError {
    fn from(e: PitchError) -> Self {
        NoteParseError::InvalidNote(e.to_string())
    }
}

/// Convert a MIDI note number to frequency in Hz.
/// A4 (MIDI 69) = 440 Hz, 12-tone equal temperament.
#[inline]
pub fn midi_to_frequency(midi: u8) -> f64 {
    MidiNote(midi).to_hz().0
}

/// Convert a frequency in Hz to the nearest MIDI note number.
#[inline]
pub fn frequency_to_midi(freq: f64) -> u8 {
    Hertz(freq).to_midi().0
}

/// Parse a note name string to frequency in Hz.
///
/// Supports formats like `"C4"`, `"A#4"`, `"Bb3"`, `"F#5"`.
/// Octave range: 0-9. Middle C = C4.
pub fn note_to_frequency(note: &str) -> Result<f64, NoteParseError> {
    let expr = PitchExpr::parse(note).map_err(NoteParseError::from)?;
    Ok(expr.to_hz().map_err(NoteParseError::from)?.0)
}

/// Parse a note name string to a MIDI note number.
///
/// C4 = 60, A4 = 69.
pub fn note_to_midi(note: &str) -> Result<u8, NoteParseError> {
    let expr = PitchExpr::parse(note).map_err(NoteParseError::from)?;
    Ok(expr.to_midi().map_err(NoteParseError::from)?.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_midi_to_frequency() {
        assert!((midi_to_frequency(69) - 440.0).abs() < 0.01); // A4
        assert!((midi_to_frequency(60) - 261.63).abs() < 0.01); // C4
        assert!((midi_to_frequency(57) - 220.0).abs() < 0.01); // A3
        assert!((midi_to_frequency(81) - 880.0).abs() < 0.01); // A5
    }

    #[test]
    fn test_frequency_to_midi() {
        assert_eq!(frequency_to_midi(440.0), 69); // A4
        assert_eq!(frequency_to_midi(261.63), 60); // C4
        assert_eq!(frequency_to_midi(880.0), 81); // A5
    }

    #[test]
    fn test_note_to_midi() {
        assert_eq!(note_to_midi("C4").unwrap(), 60);
        assert_eq!(note_to_midi("A4").unwrap(), 69);
        assert_eq!(note_to_midi("C#4").unwrap(), 61);
        assert_eq!(note_to_midi("Db4").unwrap(), 61);
        assert_eq!(note_to_midi("B3").unwrap(), 59);
        assert_eq!(note_to_midi("C0").unwrap(), 12);
        assert_eq!(note_to_midi("G#5").unwrap(), 80);
    }

    #[test]
    fn test_note_to_frequency() {
        assert!((note_to_frequency("A4").unwrap() - 440.0).abs() < 0.01);
        assert!((note_to_frequency("C4").unwrap() - 261.63).abs() < 0.01);
        assert!((note_to_frequency("E4").unwrap() - 329.63).abs() < 0.01);
        assert!((note_to_frequency("G4").unwrap() - 392.00).abs() < 0.01);
    }

    #[test]
    fn test_invalid_notes() {
        assert!(note_to_frequency("").is_err());
        assert!(note_to_frequency("X4").is_err());
        assert!(note_to_frequency("C").is_err());
        assert!(note_to_frequency("C-1").is_err());
    }
}
