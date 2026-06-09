//! Tunable parameters for the auto-zoom planner and camera.
//!
//! Defaults reproduce the "Screen-Studio-quality" starting point documented in
//! `docs/04-Input-and-AutoZoom.md`. Every field is exposed in the editor.

use serde::{Deserialize, Serialize};

/// Configuration for [`crate::plan_zooms`] and [`crate::simulate`].
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ZoomConfig {
    /// Zoom multiplier applied at a click (1.0 = no zoom).
    pub amount: f64,
    /// Half-life (s) of the zoom spring — how snappily zoom level changes.
    pub hl_zoom: f64,
    /// Half-life (s) of the pan spring — how snappily the camera follows the cursor.
    pub hl_pan: f64,
    /// Half-life (s) of the pre-smoothing applied to the raw cursor ("shaky -> glide").
    pub hl_cursor: f64,
    /// Seconds of inactivity after the last activity before the camera zooms back out.
    pub hold: f64,
    /// Seconds before a click that the zoom-in begins (anticipation).
    pub pre_roll: f64,
    /// Clicks within this time gap (s) of each other may merge into one zoom.
    pub merge_gap: f64,
    /// Clicks within this normalized distance of a cluster centroid merge into it.
    pub merge_radius: f64,
    /// The cursor must leave a box of this normalized half-extent around the camera
    /// center before the pan target moves (jitter rejection).
    pub dead_zone: f64,
    /// How strongly the focus is pulled toward a screen edge when the cursor is near it,
    /// so corner content is not cropped (0 = off).
    pub edge_snap_ratio: f64,
    /// Minimum seconds between the end of one zoom and the start of the next.
    pub min_rezoom_interval: f64,
    /// When `true`, every mouse click seeds a zoom (the original behaviour). When `false`,
    /// only the manual zoom hotkey ([`crate::InputEvent::ZoomMark`]) seeds a zoom.
    pub auto_zoom_on_click: bool,
}

impl Default for ZoomConfig {
    fn default() -> Self {
        Self {
            amount: 1.8,
            hl_zoom: 0.30,
            hl_pan: 0.22,
            hl_cursor: 0.12,
            hold: 1.8,
            pre_roll: 0.3,
            merge_gap: 0.8,
            merge_radius: 0.15,
            dead_zone: 0.10,
            edge_snap_ratio: 0.25,
            min_rezoom_interval: 1.0,
            auto_zoom_on_click: false,
        }
    }
}
