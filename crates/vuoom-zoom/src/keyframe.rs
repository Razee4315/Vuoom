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

/// Per-segment easing "feel": scales the zoom and pan spring half-lives so an
/// individual zoom can settle faster or gentler than the global defaults.
///
/// `Smooth` is the identity — it reproduces the original behaviour exactly, so it is the
/// serde default and older bundles (which lack the field) load unchanged.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ZoomStyle {
    /// Default cinematic glide — no change to the configured half-lives.
    #[default]
    Smooth,
    /// Faster settle — shorter half-lives, a punchier zoom.
    Snappy,
    /// Gentler, more cinematic — longer half-lives, a slower drift.
    Slow,
}

impl ZoomStyle {
    /// Multiplier applied to the zoom and pan spring half-lives for this style.
    ///
    /// `< 1` settles faster (snappier); `> 1` settles slower (gentler). `Smooth` is
    /// exactly `1.0`, so it leaves the spring math bit-for-bit unchanged.
    #[must_use]
    pub fn half_life_mul(self) -> f64 {
        match self {
            ZoomStyle::Smooth => 1.0,
            ZoomStyle::Snappy => 0.55,
            ZoomStyle::Slow => 1.8,
        }
    }

    /// Parse a case-insensitive label (`"smooth"`, `"snappy"`, `"slow"`) — used by the
    /// Tauri/MCP control surface, which passes the style as a plain string.
    #[must_use]
    pub fn from_label(s: &str) -> Option<ZoomStyle> {
        match s.trim().to_ascii_lowercase().as_str() {
            "smooth" => Some(ZoomStyle::Smooth),
            "snappy" => Some(ZoomStyle::Snappy),
            "slow" => Some(ZoomStyle::Slow),
            _ => None,
        }
    }
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
    /// Easing preset for this segment's spring motion. Defaults to [`ZoomStyle::Smooth`]
    /// (current behaviour) so bundles written before this field load unchanged.
    #[serde(default)]
    pub style: ZoomStyle,
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
