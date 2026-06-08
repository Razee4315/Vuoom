//! The editable output of the planner: zoom segments on the timeline.
//!
//! These are what the editor mutates (add / remove / move / resize / re-target).
//! Both the planner and the manual editor produce the same `ZoomKeyframe` list, and
//! the camera simulation consumes it. See `docs/04-Input-and-AutoZoom.md`.

use glam::DVec2;
use serde::{Deserialize, Serialize};

/// How a zoom segment chooses its focus point.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum ZoomMode {
    /// Follow the smoothed cursor path during the segment.
    Auto,
    /// Hold a fixed normalized focus point.
    Manual { pos: DVec2 },
}

/// One zoom segment on the timeline.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ZoomKeyframe {
    /// Segment start time (s), including pre-roll.
    pub start: f64,
    /// Segment end time (s), after the hold.
    pub end: f64,
    /// Zoom multiplier (1.0 = none).
    pub amount: f64,
    /// Focus behaviour.
    pub mode: ZoomMode,
    /// Edge-snap strength for this segment (copied from config at plan time, editable).
    pub edge_snap_ratio: f64,
}

impl ZoomKeyframe {
    /// Whether time `t` falls within this segment.
    #[must_use]
    pub fn contains(&self, t: f64) -> bool {
        t >= self.start && t < self.end
    }

    /// Segment duration in seconds (never negative).
    #[must_use]
    pub fn duration(&self) -> f64 {
        (self.end - self.start).max(0.0)
    }
}
