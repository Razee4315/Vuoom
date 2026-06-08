//! Global input capture → QPC-stamped event log.
//!
//! Raw Input (`RIDEV_INPUTSINK`) on a dedicated thread with a message-only window,
//! plus `GetPhysicalCursorPos` polling for absolute position. All events stamped with
//! `QueryPerformanceCounter` to share one time axis with capture frames. Per-monitor
//! DPI-aware-v2; coordinates stored in physical virtual-desktop pixels.
//! See `docs/04-Input-and-AutoZoom.md` Part A.

// TODO(M2): InputEvent { Move, Click, Scroll, DragStart, DragEnd, KeyType } with qpc field.
