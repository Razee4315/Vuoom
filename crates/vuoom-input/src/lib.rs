//! Global input capture → QPC-stamped event log (the other half of M2).
//!
//! Provides the master [`Clock`] (QPC), DPI-awareness setup, the raw event types, and the
//! pure [`normalize`] bridge into [`vuoom_zoom::InputEvent`]. The platform Raw-Input
//! recorder (a dedicated thread + message-only window + `RIDEV_INPUTSINK`) lands on top of
//! these. See `docs/04-Input-and-AutoZoom.md` Part A.

mod chords;
mod clock;
mod dpi;
mod event;
mod keys;
mod normalize;
#[cfg(windows)]
mod recorder;

#[cfg(windows)]
pub use chords::ChordWatch;
pub use chords::{set_chords, stop_chord, zoom_chord, Chord, Hotkey};
pub use clock::Clock;
pub use dpi::set_per_monitor_aware_v2;
pub use event::{MouseButton, RawEvent, RawEventKind};
pub use keys::{is_standalone, key_name, modifier, Modifier};
pub use normalize::{normalize, zoom_marks, CaptureRegion};
#[cfg(windows)]
pub use recorder::InputRecorder;
