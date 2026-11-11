//! Windows screen capture → BGRA frames (the M1 capture core).
//!
//! Primary path: Windows Graphics Capture via `windows-capture` (cursor handled by the
//! compositor, `Bgra8`, frames kept tightly packed). Frames carry a QPC timestamp so they
//! align with the input event log. A DXGI Desktop Duplication fallback lands later.
//! See `docs/03-Capture.md`.

#[cfg(any(windows, target_os = "macos"))]
mod capture;
#[cfg(windows)]
mod dxgi;
pub mod pointer;
#[cfg(target_os = "macos")]
mod sck;
#[cfg(windows)]
mod wgc;

/// The running platform's capture backend.
#[cfg(target_os = "macos")]
use sck as os;
#[cfg(windows)]
use wgc as os;

#[cfg(any(windows, target_os = "macos"))]
pub use capture::{
    spawn_capture, spawn_primary_display, spawn_region, Backend, CaptureError, CaptureHandle,
    CaptureOptions, CaptureSource, CapturedFrame, CropRegion,
};
#[cfg(any(windows, target_os = "macos"))]
pub use os::duplication_available;
#[cfg(target_os = "macos")]
pub use sck::{
    request_screen_access, screen_access, shareable_windows, ShareableWindow, PERMISSION_ERROR,
};
