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

/// Clamp a per-span spring half-life override into the sane range the inspector uses for
/// style half-lives (0.05–1.5 s), leaving `None` untouched so the caller can fall back.
fn clamp_hl(hl: Option<f64>) -> Option<f64> {
    hl.map(|h| h.clamp(0.05, 1.5))
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
    let mut smoothed = raw_cursor;
    let mut smoothed_v = DVec2::ZERO;
    let mut center = DVec2::splat(0.5);
    let mut center_v = DVec2::ZERO;
    let mut pan_target = DVec2::splat(0.5);
    let mut zoom = 1.0_f64;
    let mut zoom_v = 0.0_f64;
    // Zoom-out half-life carried over from the most-recently-active span, so a release that
    // happens *after* a span ends still honours that span's `hl_zoom_out` override.
    let mut release_hl = cfg.hl_zoom * 0.85;

    let mut si = 0;
    let mut frames = Vec::with_capacity(frame_count);
    for i in 0..frame_count {
        let t = i as f64 * dt;

        // Advance the raw cursor to the latest sample at or before this frame.
        while si < samples.len() && samples[si].0 <= t {
            raw_cursor = samples[si].1;
            si += 1;
        }
        // Pre-smooth ("shaky -> glide").
        spring_vec(
            &mut smoothed,
            &mut smoothed_v,
            raw_cursor,
            cfg.hl_cursor,
            dt,
        );

        let active = keyframes.iter().find(|k| k.contains(t));
        let (target_zoom, focus) = match active {
            Some(k) => match k.mode {
                ZoomMode::Auto => (k.amount, snap_to_edges(smoothed, k.edge_snap_ratio)),
                ZoomMode::Manual { pos } => (k.amount, pos),
                // Fit-and-center on the rect: focus at its center; the zoom may only be
                // *reduced* (never raised) so the whole subject fits. The off-screen
                // clamp keeps the center on-frame, so no extra edge-snap is applied here.
                ZoomMode::Rect { rect } => (rect.fit_zoom(k.amount), rect.center()),
            },
            None => (1.0, DVec2::splat(0.5)),
        };

        // Jitter dead-zone: only retarget when the focus leaves a box around the center,
        // or when there is no active zoom (so we re-center cleanly on zoom-out).
        if active.is_none() || (focus - center).abs().max_element() > cfg.dead_zone {
            pan_target = focus;
        }

        // Zoom spring half-life. While a span is active, use its zoom-in override (or the
        // config default); while releasing between/after spans, use the most-recently-active
        // span's zoom-out override (or the documented `hl_zoom * 0.85` faster release).
        let default_release_hl = cfg.hl_zoom * 0.85;
        let zoom_hl = match active {
            Some(k) => {
                release_hl = clamp_hl(k.hl_zoom_out).unwrap_or(default_release_hl);
                clamp_hl(k.hl_zoom_in).unwrap_or(cfg.hl_zoom)
            }
            None => release_hl,
        };
        spring_update(&mut zoom, &mut zoom_v, target_zoom, zoom_hl, dt);
        spring_vec(&mut center, &mut center_v, pan_target, cfg.hl_pan, dt);
        center = clamp_camera(center, zoom);

        frames.push(CameraState { center, zoom });
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

    use crate::event::InputEvent;
    use crate::keyframe::NormRect;

    fn auto_kf(
        start: f64,
        end: f64,
        amount: f64,
        hl_in: Option<f64>,
        hl_out: Option<f64>,
    ) -> ZoomKeyframe {
        ZoomKeyframe {
            start,
            end,
            amount,
            mode: ZoomMode::Auto,
            edge_snap_ratio: 0.0,
            hl_zoom_in: hl_in,
            hl_zoom_out: hl_out,
        }
    }

    fn rect_kf(start: f64, end: f64, amount: f64, rect: NormRect) -> ZoomKeyframe {
        ZoomKeyframe {
            start,
            end,
            amount,
            mode: ZoomMode::Rect { rect },
            edge_snap_ratio: 0.0,
            hl_zoom_in: None,
            hl_zoom_out: None,
        }
    }

    // ---- Feature 1: rect focus ----

    #[test]
    fn rect_span_fits_and_centers() {
        let cfg = ZoomConfig::default();
        let r = NormRect { x: 0.5, y: 0.5, w: 0.3, h: 0.2 };
        let track = simulate(&[], &[rect_kf(0.0, 5.0, 3.0, r)], 5.0, 60.0, &cfg);
        let s = track.at(4.5);
        let expected_zoom = r.fit_zoom(3.0);
        // The rect reduces the span amount (3.0) down to the fit zoom so it stays visible.
        assert!(expected_zoom < 3.0, "rect should have reduced the zoom");
        assert!((s.zoom - expected_zoom).abs() < 0.05, "zoom {} != fit {expected_zoom}", s.zoom);
        let expected_center = clamp_camera(r.center(), expected_zoom);
        assert!(
            (s.center - expected_center).abs().max_element() < 0.02,
            "center {:?} not at rect center {:?}",
            s.center,
            expected_center
        );
    }

    #[test]
    fn rect_span_never_exceeds_amount() {
        let cfg = ZoomConfig::default();
        // A small rect could fit at a high zoom, but must not exceed the span amount.
        let r = NormRect { x: 0.45, y: 0.45, w: 0.03, h: 0.03 };
        let track = simulate(&[], &[rect_kf(0.0, 5.0, 1.8, r)], 5.0, 60.0, &cfg);
        for f in track.frames() {
            assert!(f.zoom <= 1.8 + 1e-6, "rect zoom exceeded amount: {}", f.zoom);
        }
        assert!((track.at(4.5).zoom - 1.8).abs() < 0.05);
    }

    #[test]
    fn degenerate_rect_behaves_like_manual() {
        let cfg = ZoomConfig::default();
        let r = NormRect { x: 0.3, y: 0.3, w: 0.0, h: 0.2 };
        let track = simulate(&[], &[rect_kf(0.0, 5.0, 2.0, r)], 5.0, 60.0, &cfg);
        let s = track.at(4.5);
        // No area to fit -> holds the full span amount, centered on the (degenerate) center.
        assert!((s.zoom - 2.0).abs() < 0.05, "degenerate rect changed zoom: {}", s.zoom);
        let expected_center = clamp_camera(r.center(), 2.0);
        assert!((s.center - expected_center).abs().max_element() < 0.02);
    }

    // ---- Feature 2: per-span envelope overrides ----

    #[test]
    fn envelope_in_override_speeds_zoom_in() {
        let cfg = ZoomConfig::default();
        let fast = simulate(&[], &[auto_kf(0.0, 3.0, 2.0, Some(0.05), None)], 3.0, 60.0, &cfg);
        let slow = simulate(&[], &[auto_kf(0.0, 3.0, 2.0, Some(1.5), None)], 3.0, 60.0, &cfg);
        assert!(
            fast.at(0.3).zoom > slow.at(0.3).zoom + 0.2,
            "fast in-override not faster: {} vs {}",
            fast.at(0.3).zoom,
            slow.at(0.3).zoom
        );
    }

    #[test]
    fn envelope_in_none_falls_back_to_config() {
        let cfg = ZoomConfig::default();
        let default_kf = simulate(&[], &[auto_kf(0.0, 3.0, 2.0, None, None)], 3.0, 60.0, &cfg);
        let explicit = simulate(
            &[],
            &[auto_kf(0.0, 3.0, 2.0, Some(cfg.hl_zoom), None)],
            3.0,
            60.0,
            &cfg,
        );
        assert_eq!(default_kf.frames(), explicit.frames());
    }

    #[test]
    fn envelope_out_override_used_on_release() {
        let cfg = ZoomConfig::default();
        // Span ends at t=1; the release afterwards must honour the span's out override.
        let fast_out = simulate(&[], &[auto_kf(0.0, 1.0, 2.0, None, Some(0.05))], 3.0, 60.0, &cfg);
        let slow_out = simulate(&[], &[auto_kf(0.0, 1.0, 2.0, None, Some(1.5))], 3.0, 60.0, &cfg);
        assert!(
            fast_out.at(1.3).zoom < slow_out.at(1.3).zoom - 0.1,
            "fast out-override did not release faster: {} vs {}",
            fast_out.at(1.3).zoom,
            slow_out.at(1.3).zoom
        );
    }

    #[test]
    fn envelope_out_none_falls_back_to_faster_release() {
        let cfg = ZoomConfig::default();
        // None out-override reproduces the documented `hl_zoom * 0.85` faster release.
        let default_kf = simulate(&[], &[auto_kf(0.0, 1.0, 2.0, None, None)], 3.0, 60.0, &cfg);
        let explicit = simulate(
            &[],
            &[auto_kf(0.0, 1.0, 2.0, None, Some(cfg.hl_zoom * 0.85))],
            3.0,
            60.0,
            &cfg,
        );
        assert_eq!(default_kf.frames(), explicit.frames());
    }

    // ---- Feature 3: caret-follow ----

    #[test]
    fn keytype_with_pos_steers_auto_focus() {
        let cfg = ZoomConfig::default();
        let kf = auto_kf(0.0, 5.0, 2.0, None, None);
        let with_pos = [
            InputEvent::Move { t: 0.0, pos: DVec2::new(0.5, 0.5) },
            InputEvent::KeyType { t: 0.5, pos: Some(DVec2::new(0.9, 0.5)) },
        ];
        let without = [
            InputEvent::Move { t: 0.0, pos: DVec2::new(0.5, 0.5) },
            InputEvent::KeyType { t: 0.5, pos: None },
        ];
        let a = simulate(&with_pos, &[kf], 5.0, 60.0, &cfg).at(4.5);
        let b = simulate(&without, &[kf], 5.0, 60.0, &cfg).at(4.5);
        assert!(a.center.x > 0.55, "caret-pos did not steer focus right: {}", a.center.x);
        assert!((b.center.x - 0.5).abs() < 1e-6, "no-pos KeyType moved focus: {}", b.center.x);
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
