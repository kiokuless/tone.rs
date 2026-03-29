use thiserror::Error;

/// Error type for note name parsing.
#[derive(Debug, Clone, PartialEq, Error)]
pub enum NoteParseError {
    #[error("invalid note: {0}")]
    InvalidNote(String),
}

/// Convert a MIDI note number to frequency in Hz.
/// A4 (MIDI 69) = 440 Hz, 12-tone equal temperament.
pub fn midi_to_frequency(midi: u8) -> f64 {
    440.0 * 2.0_f64.powf((midi as f64 - 69.0) / 12.0)
}

/// Convert a frequency in Hz to the nearest MIDI note number.
pub fn frequency_to_midi(freq: f64) -> u8 {
    let midi = 69.0 + 12.0 * (freq / 440.0).log2();
    midi.round().clamp(0.0, 127.0) as u8
}

/// Parse a note name string to frequency in Hz.
///
/// Supports formats like `"C4"`, `"A#4"`, `"Bb3"`, `"F#5"`.
/// Octave range: 0-9. Middle C = C4.
pub fn note_to_frequency(note: &str) -> Result<f64, NoteParseError> {
    let midi = note_to_midi(note)?;
    Ok(midi_to_frequency(midi))
}

/// Parse a note name string to a MIDI note number.
///
/// C4 = 60, A4 = 69.
pub fn note_to_midi(note: &str) -> Result<u8, NoteParseError> {
    let err = || NoteParseError::InvalidNote(note.to_string());
    let bytes = note.as_bytes();

    if bytes.is_empty() {
        return Err(err());
    }

    // Parse pitch class (C, D, E, F, G, A, B)
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

    // Parse optional accidental (# or b) and octave
    let (accidental, rest) = if bytes.len() > 1 {
        match bytes[1] {
            b'#' => (1i8, &note[2..]),
            b'b' => (-1i8, &note[2..]),
            _ => (0i8, &note[1..]),
        }
    } else {
        return Err(err()); // Need at least pitch + octave
    };

    // Parse octave number
    let octave: i8 = rest.parse().map_err(|_| err())?;
    if !(0..=9).contains(&octave) {
        return Err(err());
    }

    // MIDI note: C4 = 60
    // C-1 = 0 in some standards, but we use C0 = 12
    let midi = (octave as i16 + 1) * 12 + base_semitone as i16 + accidental as i16;

    if !(0..=127).contains(&midi) {
        return Err(err());
    }

    Ok(midi as u8)
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
