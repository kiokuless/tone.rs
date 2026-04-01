pub mod context;
pub mod expr;
pub mod frequency;
pub mod notation;
pub mod value;

// Re-export value types
pub use value::{Beats, Bpm, Hertz, MidiNote, Samples, Seconds, Ticks};

// Re-export context types
pub use context::{StaticTimeContext, TimeContext};

// Re-export expression types
pub use expr::{PitchError, PitchExpr, TimeError, TimeExpr};

// Re-export legacy API for backward compatibility
pub use frequency::{NoteParseError, frequency_to_midi, midi_to_frequency, note_to_frequency};
pub use notation::{TimeParseError, parse_time};

// Re-export conversion utilities
pub use value::{db_to_gain, equal_power_scale, gain_to_db, interval_to_freq_ratio};
