//! Windows screen capture → BGRA frames (the M1 capture core).
//!
//! Primary path: Windows Graphics Capture via `windows-capture` (cursor handled by the
//! compositor, `Bgra8`, frames kept tightly packed). Frames carry a QPC timestamp so they
//! align with the input event log. A DXGI Desktop Duplication fallback lands later.
//! See `docs/03-Capture.md`.

#[cfg(windows)]
mod capture;

#[cfg(windows)]
pub use capture::{run_primary_display, spawn_primary_display, CaptureError, CapturedFrame};
