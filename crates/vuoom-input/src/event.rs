//! Raw, QPC-stamped global input events in physical virtual-desktop pixels.
//!
//! These are produced by the platform recorder and later normalized (per captured
//! region) into [`vuoom_zoom::InputEvent`] for the auto-zoom planner.

/// Which mouse button an event refers to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
    X1,
    X2,
}

/// What a raw event represents.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RawEventKind {
    Move,
    ButtonDown(MouseButton),
    ButtonUp(MouseButton),
    /// Wheel scroll with a signed delta (wheel units).
    Scroll(i32),
    /// Key pressed (Win32 virtual-key code).
    KeyDown(u16),
    /// Key released (Win32 virtual-key code).
    KeyUp(u16),
}

/// A single raw input event: a QPC timestamp, physical coordinates, and what happened.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RawEvent {
    /// `QueryPerformanceCounter` value at the instant the event arrived.
    pub qpc: i64,
    /// Physical virtual-desktop X (may be negative on secondary monitors).
    pub x: i32,
    /// Physical virtual-desktop Y.
    pub y: i32,
    pub kind: RawEventKind,
}
