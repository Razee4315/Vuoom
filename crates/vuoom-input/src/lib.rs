//! Global input capture → QPC-stamped event log (the other half of M2).
//!
//! Provides the master [`Clock`] (QPC), DPI-awareness setup, the raw event types, and the
//! pure [`normalize`] bridge into [`vuoom_zoom::InputEvent`]. The platform Raw-Input
//! recorder (a dedicated thread + message-only window + `RIDEV_INPUTSINK`) lands on top of
//! these. See `docs/04-Input-and-AutoZoom.md` Part A.

mod clock;
mod dpi;
mod event;
mod normalize;

pub use clock::Clock;
pub use dpi::set_per_monitor_aware_v2;
pub use event::{MouseButton, RawEvent, RawEventKind};
pub use normalize::{normalize, CaptureRegion};
