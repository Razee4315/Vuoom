//! Per-Monitor-DPI-Aware-V2 declaration.
//!
//! Must run at startup. Without it Windows virtualizes coordinates and the cursor
//! misaligns with captured pixels on scaled monitors (see `docs/04-Input-and-AutoZoom.md`).

/// Declare the process Per-Monitor-DPI-Aware-V2. Returns `true` on success.
///
/// Safe to call more than once; only the first call has effect.
#[cfg(windows)]
#[must_use]
pub fn set_per_monitor_aware_v2() -> bool {
    use windows::Win32::UI::HiDpi::{
        SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
    };
    // SAFETY: the context constant is a valid handle; the call has no memory effects.
    unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2).is_ok() }
}

/// No-op on non-Windows targets.
#[cfg(not(windows))]
#[must_use]
pub fn set_per_monitor_aware_v2() -> bool {
    false
}
