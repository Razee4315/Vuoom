//! The editable output of the planner: zoom segments on the timeline.
//!
//! These are what the editor mutates (add / remove / move / resize / re-target).
//! Both the planner and the manual editor produce the same `ZoomKeyframe` list, and
//! the camera simulation consumes it. See `docs/04-Input-and-AutoZoom.md`.

use glam::DVec2;
use serde::{Deserialize, Serialize};

/// Padding factor applied when fitting a [`ZoomMode::Rect`] subject to the viewport:
/// the rect's larger side is inflated by this much before choosing the fit zoom, so the
/// subject never touches the crop edge (~12% breathing room).
pub const RECT_FIT_PADDING: f64 = 1.12;

/// A normalized rectangle in `0.0..=1.0` capture space (`x`,`y` = top-left corner).
///
/// Defined here (not imported from `vuoom-render`) because `vuoom-render` depends on
/// `vuoom-zoom`, so the dependency may only point one way.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct NormRect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl NormRect {
    /// The rect's center point in normalized space.
    #[must_use]
    pub fn center(&self) -> DVec2 {
        DVec2::new(self.x + self.w * 0.5, self.y + self.h * 0.5)
    }

    /// A rect with a non-positive side has no area to fit — callers fall back to a plain
    /// point focus at [`Self::center`].
    #[must_use]
    pub fn is_degenerate(&self) -> bool {
        self.w <= 0.0 || self.h <= 0.0
    }

    /// The zoom multiplier that fits this rect inside the (square) viewport with
    /// [`RECT_FIT_PADDING`] breathing room, **never exceeding** `amount` and never below
    /// `1.0`: the rect may only *reduce* the span zoom so the whole subject stays visible.
    ///
    /// A degenerate rect returns `amount` unchanged (behave like a fixed point focus).
    #[must_use]
    pub fn fit_zoom(&self, amount: f64) -> f64 {
        if self.is_degenerate() {
            return amount;
        }
        let fit = 1.0 / (self.w.max(self.h) * RECT_FIT_PADDING);
        amount.min(fit).clamp(1.0, amount)
    }
}

/// How a zoom segment chooses its focus point.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum ZoomMode {
    /// Follow the smoothed cursor path during the segment.
    Auto,
    /// Hold a fixed normalized focus point.
    Manual { pos: DVec2 },
    /// Fit and center on a normalized rectangle: focus at the rect center, with the zoom
    /// reduced (if needed) so the whole rect fits with padding. See [`NormRect::fit_zoom`].
    Rect { rect: NormRect },
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
    /// Optional per-span override for the zoom-*in* spring half-life (seconds), same units
    /// as [`crate::ZoomConfig::hl_zoom`]. `None` falls back to the config default. Clamped
    /// to a sane range by the camera. Absent in older saved projects (`serde` default).
    #[serde(default)]
    pub hl_zoom_in: Option<f64>,
    /// Optional per-span override for the zoom-*out* (release) spring half-life (seconds).
    /// `None` falls back to the current release behaviour (`hl_zoom * 0.85`). Clamped to a
    /// sane range by the camera. Absent in older saved projects (`serde` default).
    #[serde(default)]
    pub hl_zoom_out: Option<f64>,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x: f64, y: f64, w: f64, h: f64) -> NormRect {
        NormRect { x, y, w, h }
    }

    #[test]
    fn rect_center_is_the_midpoint() {
        assert_eq!(rect(0.2, 0.4, 0.4, 0.2).center(), DVec2::new(0.4, 0.5));
    }

    #[test]
    fn fit_zoom_fits_the_rect_with_padding() {
        // A 0.4-wide rect: fit = 1 / (0.4 * 1.12) ≈ 2.232. With a generous span amount the
        // effective zoom is the fit value, so the padded rect exactly fills the viewport.
        let r = rect(0.3, 0.3, 0.4, 0.3);
        let z = r.fit_zoom(10.0);
        let expected = 1.0 / (0.4 * RECT_FIT_PADDING);
        assert!((z - expected).abs() < 1e-12, "fit zoom {z} != {expected}");
        // The viewport side (1/z) must exceed the rect's larger side by the padding factor.
        let viewport_side = 1.0 / z;
        assert!(
            viewport_side >= 0.4 * RECT_FIT_PADDING - 1e-12,
            "rect does not fit inside viewport: side {viewport_side}"
        );
    }

    #[test]
    fn fit_zoom_never_exceeds_span_amount() {
        // A tiny rect *could* fit at a huge zoom, but the span amount caps it.
        let r = rect(0.45, 0.45, 0.02, 0.02);
        let amount = 1.8;
        assert!(
            r.fit_zoom(amount) <= amount + 1e-12,
            "fit zoom exceeded span amount"
        );
        assert!((r.fit_zoom(amount) - amount).abs() < 1e-12);
    }

    #[test]
    fn fit_zoom_only_reduces_never_below_one() {
        // A rect nearly filling the frame forces the zoom down to 1.0 (whole frame).
        let r = rect(0.02, 0.02, 0.96, 0.96);
        assert!((r.fit_zoom(1.8) - 1.0).abs() < 1e-12, "should clamp to 1.0");
    }

    #[test]
    fn degenerate_rect_returns_amount_unchanged() {
        assert!(rect(0.4, 0.4, 0.0, 0.3).is_degenerate());
        assert!(rect(0.4, 0.4, 0.3, -0.1).is_degenerate());
        assert_eq!(rect(0.4, 0.4, 0.0, 0.3).fit_zoom(2.5), 2.5);
    }

    #[test]
    fn old_keyframe_json_without_new_fields_deserializes() {
        // A project saved before the envelope + rect fields existed.
        let old = r#"{
            "start": 1.0,
            "end": 3.0,
            "amount": 1.8,
            "mode": "Auto",
            "edge_snap_ratio": 0.25
        }"#;
        let kf: ZoomKeyframe = serde_json::from_str(old).unwrap();
        assert_eq!(kf.mode, ZoomMode::Auto);
        assert_eq!(kf.hl_zoom_in, None);
        assert_eq!(kf.hl_zoom_out, None);
    }

    #[test]
    fn old_manual_mode_json_still_deserializes() {
        // Externally-tagged Manual variant from an old project file.
        let old = r#"{"Manual":{"pos":[0.4,0.6]}}"#;
        let mode: ZoomMode = serde_json::from_str(old).unwrap();
        assert_eq!(
            mode,
            ZoomMode::Manual {
                pos: DVec2::new(0.4, 0.6)
            }
        );
    }

    #[test]
    fn zoom_mode_round_trips_including_rect() {
        for mode in [
            ZoomMode::Auto,
            ZoomMode::Manual {
                pos: DVec2::new(0.1, 0.9),
            },
            ZoomMode::Rect {
                rect: rect(0.1, 0.2, 0.3, 0.4),
            },
        ] {
            let json = serde_json::to_string(&mode).unwrap();
            let back: ZoomMode = serde_json::from_str(&json).unwrap();
            assert_eq!(mode, back, "round-trip changed the mode: {json}");
        }
    }

    #[test]
    fn keyframe_with_overrides_round_trips() {
        let kf = ZoomKeyframe {
            start: 0.5,
            end: 2.5,
            amount: 2.0,
            mode: ZoomMode::Rect {
                rect: rect(0.2, 0.2, 0.5, 0.4),
            },
            edge_snap_ratio: 0.25,
            hl_zoom_in: Some(0.4),
            hl_zoom_out: Some(0.2),
        };
        let json = serde_json::to_string(&kf).unwrap();
        let back: ZoomKeyframe = serde_json::from_str(&json).unwrap();
        assert_eq!(kf, back);
    }
}
