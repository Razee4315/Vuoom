//! Windows screen capture → BGRA frames (the M1 capture core).
//!
//! Primary path: Windows Graphics Capture via `windows-capture` (cursor handled by the
//! compositor, `Bgra8`, frames kept tightly packed). Frames carry a QPC timestamp so they
//! align with the input event log. A DXGI Desktop Duplication fallback lands later.
//! See `docs/03-Capture.md`.

#[cfg(windows)]
mod capture;

/// Top-level window enumeration + bounds (window-targeted capture support).
pub mod windows;

#[cfg(windows)]
pub use capture::{
    run_display, run_target, spawn_primary_display, spawn_region, spawn_target, CaptureError,
    CaptureHandle, CaptureTarget, CapturedFrame, CropRegion,
};

pub use windows::{find_window_bounds, list_windows, WindowInfo};
