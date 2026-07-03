//! Timestamped input events — the raw material the auto-zoom planner consumes.
//!
//! Produced by `vuoom-input` (Raw Input + cursor polling). Times are seconds,
//! frame-relative (derived from QPC). Positions are normalized `0.0..=1.0` within
//! the captured monitor/region, so the planner is resolution- and DPI-independent.

use glam::DVec2;
use serde::{Deserialize, Serialize};

/// Which mouse button produced a click.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
    Other,
}

/// A single timestamped input event in normalized capture space.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum InputEvent {
    /// Cursor moved to `pos`.
    Move { t: f64, pos: DVec2 },
    /// A mouse button was pressed at `pos`.
    Click {
        t: f64,
        pos: DVec2,
        button: MouseButton,
    },
    /// A scroll tick at `pos` (`delta` in arbitrary wheel units).
    Scroll { t: f64, pos: DVec2, delta: f64 },
    /// A drag began at `pos`.
    DragStart { t: f64, pos: DVec2 },
    /// A drag ended at `pos`.
    DragEnd { t: f64, pos: DVec2 },
    /// A key was typed. Sustains a zoom while typing. An optional `pos` (the caret /
    /// last-injection point, normalized) lets typing *steer* an `Auto` span toward the
    /// caret so text does not run off the edge; `None` behaves exactly as before (sustain
    /// only, no focus change). Absent in older saved projects (`serde` default).
    KeyType {
        t: f64,
        #[serde(default)]
        pos: Option<DVec2>,
    },
    /// The user deliberately requested a zoom at `pos` (e.g. the manual zoom hotkey).
    /// Always a zoom trigger, regardless of the click-to-zoom setting.
    ZoomMark { t: f64, pos: DVec2 },
}

impl InputEvent {
    /// The event's timestamp in seconds.
    #[must_use]
    pub fn t(&self) -> f64 {
        match *self {
            InputEvent::Move { t, .. }
            | InputEvent::Click { t, .. }
            | InputEvent::Scroll { t, .. }
            | InputEvent::DragStart { t, .. }
            | InputEvent::DragEnd { t, .. }
            | InputEvent::ZoomMark { t, .. }
            | InputEvent::KeyType { t, .. } => t,
        }
    }

    /// The event's normalized position, if it has one.
    ///
    /// `KeyType` carries a position only when a caret point was captured; a plain typed
    /// key (`pos: None`) still returns `None`, so it sustains a hold without moving focus.
    #[must_use]
    pub fn pos(&self) -> Option<DVec2> {
        match *self {
            InputEvent::Move { pos, .. }
            | InputEvent::Click { pos, .. }
            | InputEvent::Scroll { pos, .. }
            | InputEvent::DragStart { pos, .. }
            | InputEvent::ZoomMark { pos, .. }
            | InputEvent::DragEnd { pos, .. } => Some(pos),
            InputEvent::KeyType { pos, .. } => pos,
        }
    }

    /// Whether this event should *seed* a new zoom (a deliberate point of interest).
    ///
    /// Clicks and drag-starts only count when click-to-zoom is enabled; a [`ZoomMark`]
    /// (the manual hotkey) always counts. See [`crate::plan_zooms`].
    ///
    /// [`ZoomMark`]: InputEvent::ZoomMark
    #[must_use]
    pub fn is_click_trigger(&self) -> bool {
        matches!(
            self,
            InputEvent::Click { .. } | InputEvent::DragStart { .. }
        )
    }

    /// Whether this event is a deliberate manual zoom request ([`ZoomMark`]).
    ///
    /// [`ZoomMark`]: InputEvent::ZoomMark
    #[must_use]
    pub fn is_zoom_mark(&self) -> bool {
        matches!(self, InputEvent::ZoomMark { .. })
    }

    /// Whether this event counts as ongoing activity that *sustains* a zoom hold.
    #[must_use]
    pub fn is_activity(&self) -> bool {
        !matches!(self, InputEvent::Move { .. })
    }
}
