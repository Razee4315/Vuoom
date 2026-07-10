//! The virtual camera: critically-damped springs + an off-screen clamp, simulated over
//! the whole clip into a per-frame track that scrubbing and export both read.
//!
//! Math and defaults: `docs/04-Input-and-AutoZoom.md`. Pure logic, fully unit-tested.

use crate::config::ZoomConfig;
use crate::event::InputEvent;
use crate::keyframe::{ZoomKeyframe, ZoomMode};
use glam::DVec2;
use serde::{Deserialize, Serialize};
use std::f64::consts::LN_2;

/// Camera pose for one frame: normalized `center` and `zoom` multiplier.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CameraState {
    pub center: DVec2,
    pub zoom: f64,
}

impl Default for CameraState {
    fn default() -> Self {
        Self {
            center: DVec2::splat(0.5),
            zoom: 1.0,
        }
    }
}

/// Critically-damped spring, exact integration — frame-rate independent, no overshoot.
///
/// `hl` is the half-life in seconds (time to close half the remaining distance).
pub fn spring_update(x: &mut f64, v: &mut f64, goal: f64, hl: f64, dt: f64) {
    // Guard against a zero/negative half-life: it would make `y` infinite and poison the
    // whole track with NaN (every `hl_*` is user-editable in the inspector).
    let hl = hl.max(1e-4);
    // Critical damping derived from the half-life: y = 2*ln2 / hl.
    let y = 2.0 * LN_2 / hl;
    let j0 = *x - goal;
    let j1 = *v + j0 * y;
    let eydt = (-y * dt).exp();
    *x = eydt * (j0 + j1 * dt) + goal;
    *v = eydt * (*v - j1 * y * dt);
}

fn spring_vec(x: &mut DVec2, v: &mut DVec2, goal: DVec2, hl: f64, dt: f64) {
    spring_update(&mut x.x, &mut v.x, goal.x, hl, dt);
    spring_update(&mut x.y, &mut v.y, goal.y, hl, dt);
}

/// Clamp the camera center so the zoomed viewport never reveals area outside the frame.
#[must_use]
pub fn clamp_camera(center: DVec2, zoom: f64) -> DVec2 {
    let half = 0.5 / zoom.max(1.0);
    DVec2::new(
        center.x.clamp(half, 1.0 - half),
        center.y.clamp(half, 1.0 - half),
    )
}

/// Bias a focus point toward a screen edge when the cursor is near it, so corner
/// content is not cropped once the camera clamp kicks in.
fn snap_to_edges(p: DVec2, ratio: f64) -> DVec2 {
    DVec2::new(snap_axis(p.x, ratio), snap_axis(p.y, ratio))
}

fn snap_axis(v: f64, ratio: f64) -> f64 {
    if ratio <= 0.0 {
        v
    } else if v < ratio {
        (v - (ratio - v)).max(0.0)
    } else if v > 1.0 - ratio {
        (v + (v - (1.0 - ratio))).min(1.0)
    } else {
        v
    }
}

/// What the camera should aim for on a single frame, before smoothing and clamping.
///
/// This is the one place the offline planner (keyframe-driven) and the live preview
/// (hotkey-toggle-driven) differ — they resolve *what* to look at from different inputs,
/// then hand it to the same [`CameraFilter::step`] so the motion stays identical.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CameraTarget {
    /// No active zoom: settle back to the centered, unzoomed pose.
    Idle,
    /// Auto-follow: zoom to `amount`, focus the edge-snapped smoothed cursor.
    Auto { amount: f64, edge_snap_ratio: f64 },
    /// Manual: zoom to `amount`, hold a fixed normalized focus point.
    Manual { amount: f64, focus: DVec2 },
}

/// The online spring state behind the camera: pre-smooth, zoom, and pan springs plus the
/// jitter dead-zone. Shared verbatim by the offline [`simulate`] and the live preview so
/// the two can never drift out of lock-step. Call [`CameraFilter::step`] once per frame.
#[derive(Debug, Clone, Copy)]
pub struct CameraFilter {
    smoothed: DVec2,
    smoothed_v: DVec2,
    center: DVec2,
    center_v: DVec2,
    pan_target: DVec2,
    zoom: f64,
    zoom_v: f64,
}

impl CameraFilter {
    /// Start from a resting camera (centered, unzoomed) with the pre-smoother primed to
    /// `cursor` so the first frame does not spring in from an arbitrary point.
    #[must_use]
    pub fn new(cursor: DVec2) -> Self {
        Self {
            smoothed: cursor,
            smoothed_v: DVec2::ZERO,
            center: DVec2::splat(0.5),
            center_v: DVec2::ZERO,
            pan_target: DVec2::splat(0.5),
            zoom: 1.0,
            zoom_v: 0.0,
        }
    }

    /// Advance the camera by `dt` seconds toward `target`, feeding in the latest raw cursor.
    ///
    /// Pre-smooths the cursor, resolves the target zoom/focus (with edge-snap and the jitter
    /// dead-zone), integrates the zoom and pan springs, and clamps the viewport on-screen.
    pub fn step(
        &mut self,
        raw_cursor: DVec2,
        target: CameraTarget,
        cfg: &ZoomConfig,
        dt: f64,
    ) -> CameraState {
        // Pre-smooth the raw cursor ("shaky -> glide").
        spring_vec(
            &mut self.smoothed,
            &mut self.smoothed_v,
            raw_cursor,
            cfg.hl_cursor,
            dt,
        );

        let (target_zoom, focus, active) = match target {
            CameraTarget::Idle => (1.0, DVec2::splat(0.5), false),
            CameraTarget::Auto {
                amount,
                edge_snap_ratio,
            } => (amount, snap_to_edges(self.smoothed, edge_snap_ratio), true),
            CameraTarget::Manual { amount, focus } => (amount, focus, true),
        };

        // Jitter dead-zone: only retarget when the focus leaves a box around the center,
        // or when there is no active zoom (so we re-center cleanly on zoom-out).
        if !active || (focus - self.center).abs().max_element() > cfg.dead_zone {
            self.pan_target = focus;
        }

        // Zoom-out a touch faster than zoom-in (matches the documented feel).
        let zoom_hl = if target_zoom < self.zoom {
            cfg.hl_zoom * 0.85
        } else {
            cfg.hl_zoom
        };
        spring_update(&mut self.zoom, &mut self.zoom_v, target_zoom, zoom_hl, dt);
        spring_vec(
            &mut self.center,
            &mut self.center_v,
            self.pan_target,
            cfg.hl_pan,
            dt,
        );
        self.center = clamp_camera(self.center, self.zoom);

        CameraState {
            center: self.center,
            zoom: self.zoom,
        }
    }
}

/// A precomputed per-frame camera path. Deterministic: the same inputs always produce
/// the same track, so scrubbing and GIF export share one source of truth.
#[derive(Debug, Clone)]
pub struct CameraTrack {
    fps: f64,
    frames: Vec<CameraState>,
}

impl CameraTrack {
    /// Frames per second this track was simulated at.
    #[must_use]
    pub fn fps(&self) -> f64 {
        self.fps
    }

    /// The raw per-frame states.
    #[must_use]
    pub fn frames(&self) -> &[CameraState] {
        &self.frames
    }

    /// Camera pose at an arbitrary time `t` (seconds), linearly interpolated.
    #[must_use]
    pub fn at(&self, t: f64) -> CameraState {
        if self.frames.is_empty() {
            return CameraState::default();
        }
        let last = self.frames.len() - 1;
        let f = (t * self.fps).clamp(0.0, last as f64);
        let i = f.floor() as usize;
        let frac = f - i as f64;
        if i < last {
            let a = self.frames[i];
            let b = self.frames[i + 1];
            CameraState {
                center: a.center.lerp(b.center, frac),
                zoom: a.zoom + (b.zoom - a.zoom) * frac,
            }
        } else {
            self.frames[last]
        }
    }
}

/// Simulate the camera over the whole clip, producing a [`CameraTrack`].
///
/// Per frame: pre-smooth the raw cursor, pick the active segment's target zoom and focus
/// (with edge-snap and a jitter dead-zone), integrate the zoom and pan springs, and clamp.
#[must_use]
pub fn simulate(
    events: &[InputEvent],
    keyframes: &[ZoomKeyframe],
    duration: f64,
    fps: f64,
    cfg: &ZoomConfig,
) -> CameraTrack {
    // A zero/negative fps would make `dt` infinite and NaN-poison the whole track.
    let fps = fps.max(1.0);
    let frame_count = (duration * fps).ceil().max(1.0) as usize + 1;
    let dt = 1.0 / fps;

    // Raw cursor samples (anything with a position), in time order.
    let mut samples: Vec<(f64, DVec2)> = events
        .iter()
        .filter_map(|e| e.pos().map(|p| (e.t(), p)))
        .collect();
    samples.sort_by(|a, b| a.0.total_cmp(&b.0));

    let mut raw_cursor = samples.first().map_or(DVec2::splat(0.5), |s| s.1);
    let mut cam = CameraFilter::new(raw_cursor);

    let mut si = 0;
    let mut frames = Vec::with_capacity(frame_count);
    for i in 0..frame_count {
        let t = i as f64 * dt;

        // Advance the raw cursor to the latest sample at or before this frame.
        while si < samples.len() && samples[si].0 <= t {
            raw_cursor = samples[si].1;
            si += 1;
        }

        // Resolve this frame's target from the active keyframe, then run the shared step.
        // A per-zoom style scales the spring half-lives for the span that zoom drives;
        // idle frames (between zooms) keep the base config so zoom-out feel is uniform.
        let (target, hl_mul) = match keyframes.iter().find(|k| k.contains(t)) {
            Some(k) => {
                let target = match k.mode {
                    ZoomMode::Auto => CameraTarget::Auto {
                        amount: k.amount,
                        edge_snap_ratio: k.edge_snap_ratio,
                    },
                    ZoomMode::Manual { pos } => CameraTarget::Manual {
                        amount: k.amount,
                        focus: pos,
                    },
                };
                (target, k.style.half_life_mul())
            }
            None => (CameraTarget::Idle, 1.0),
        };
        // Scale the zoom & pan half-lives for this segment's feel. `Smooth` yields a
        // multiplier of exactly 1.0, so `* 1.0` leaves the values (and output) unchanged.
        let mut frame_cfg = *cfg;
        frame_cfg.hl_zoom *= hl_mul;
        frame_cfg.hl_pan *= hl_mul;
        frames.push(cam.step(raw_cursor, target, &frame_cfg, dt));
    }

    CameraTrack { fps, frames }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spring_settles_toward_goal() {
        let (mut x, mut v) = (0.0, 0.0);
        for _ in 0..240 {
            spring_update(&mut x, &mut v, 1.0, 0.25, 1.0 / 60.0);
        }
        assert!((x - 1.0).abs() < 1e-3, "spring did not settle: x={x}");
    }

    #[test]
    fn spring_never_overshoots() {
        let (mut x, mut v) = (0.0, 0.0);
        let mut max = 0.0_f64;
        for _ in 0..600 {
            spring_update(&mut x, &mut v, 1.0, 0.25, 1.0 / 120.0);
            max = max.max(x);
        }
        assert!(
            max <= 1.0 + 1e-6,
            "critically-damped spring overshot to {max}"
        );
    }

    #[test]
    fn clamp_keeps_viewport_in_bounds() {
        let c = clamp_camera(DVec2::new(0.0, 1.0), 2.0);
        assert_eq!(c, DVec2::new(0.25, 0.75));
        // At zoom 1.0 the only valid center is the middle.
        assert_eq!(clamp_camera(DVec2::new(0.0, 0.0), 1.0), DVec2::splat(0.5));
    }

    #[test]
    fn spring_zero_half_life_does_not_nan() {
        // hl=0 used to make `y` infinite -> NaN; the guard must keep it finite.
        let (mut x, mut v) = (0.0, 0.0);
        spring_update(&mut x, &mut v, 1.0, 0.0, 1.0 / 60.0);
        assert!(x.is_finite() && v.is_finite(), "x={x} v={v}");
    }

    #[test]
    fn zoom_style_changes_settle_speed() {
        use crate::keyframe::{ZoomKeyframe, ZoomMode, ZoomStyle};
        let cfg = ZoomConfig::default();
        let seg = |style| ZoomKeyframe {
            start: 0.0,
            end: 6.0,
            amount: 2.0,
            mode: ZoomMode::Auto,
            edge_snap_ratio: 0.0,
            style,
        };
        // How far the zoom has risen shortly after the segment starts: a shorter
        // half-life closes the gap faster, so snappy leads smooth leads slow.
        let sample = |style| simulate(&[], &[seg(style)], 6.0, 60.0, &cfg).at(0.4).zoom;
        let snappy = sample(ZoomStyle::Snappy);
        let smooth = sample(ZoomStyle::Smooth);
        let slow = sample(ZoomStyle::Slow);
        assert!(snappy > smooth, "snappy {snappy} should lead smooth {smooth}");
        assert!(smooth > slow, "smooth {smooth} should lead slow {slow}");
    }

    #[test]
    fn simulate_zero_fps_produces_finite_track() {
        // fps=0 used to make `dt` infinite -> a NaN-poisoned track.
        let track = simulate(&[], &[], 1.0, 0.0, &ZoomConfig::default());
        assert!(!track.frames().is_empty());
        assert!(track
            .frames()
            .iter()
            .all(|f| f.zoom.is_finite() && f.center.x.is_finite() && f.center.y.is_finite()));
    }
}
