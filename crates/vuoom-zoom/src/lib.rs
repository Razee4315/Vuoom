//! Vuoom auto-zoom — the heart of the product.
//!
//! Turns a timestamped input log into editable zoom segments ([`plan_zooms`]) and a
//! deterministic per-frame camera path ([`simulate`]). Critically-damped springs give
//! cinematic, overshoot-free motion; an off-screen clamp guarantees the viewport never
//! reveals empty area. No GPU or OS dependencies, so the whole thing is unit-tested here
//! (the M2 acceptance gate). Algorithm and defaults: `docs/04-Input-and-AutoZoom.md`.

mod camera;
mod config;
mod edit;
mod event;
mod keyframe;
mod planner;

pub use camera::{clamp_camera, simulate, spring_update, CameraState, CameraTrack};
pub use config::ZoomConfig;
pub use edit::{insert_sorted, move_to, remove, resize, sort_by_start, MIN_LEN};
pub use event::{InputEvent, MouseButton};
pub use keyframe::{ZoomKeyframe, ZoomMode};
pub use planner::plan_zooms;

/// Convenience: plan zoom segments and simulate the camera in one call.
///
/// Returns the editable segments (for the timeline) and the per-frame camera track
/// (for preview + export). Both come from the same inputs, so they always agree.
#[must_use]
pub fn plan_and_simulate(
    events: &[InputEvent],
    duration: f64,
    fps: f64,
    cfg: &ZoomConfig,
) -> (Vec<ZoomKeyframe>, CameraTrack) {
    let zooms = plan_zooms(events, duration, cfg);
    let track = simulate(events, &zooms, duration, fps, cfg);
    (zooms, track)
}

#[cfg(test)]
mod gate_tests {
    //! The spec §5.1 "definition of done" for auto-zoom, encoded as tests.

    use super::*;
    use glam::DVec2;

    fn click(t: f64, x: f64, y: f64) -> InputEvent {
        InputEvent::Click {
            t,
            pos: DVec2::new(x, y),
            button: MouseButton::Left,
        }
    }

    fn mv(t: f64, x: f64, y: f64) -> InputEvent {
        InputEvent::Move {
            t,
            pos: DVec2::new(x, y),
        }
    }

    /// A typical "click around a UI" recording: move, click a corner, move, click elsewhere.
    fn sample_recording() -> Vec<InputEvent> {
        let mut e = Vec::new();
        for i in 0..30 {
            let f = f64::from(i) / 30.0;
            e.push(mv(f, 0.5 + 0.02 * f, 0.5));
        }
        e.push(click(1.0, 0.12, 0.12)); // near top-left corner
        for i in 0..30 {
            let f = 1.0 + f64::from(i) / 30.0;
            e.push(mv(f, 0.12 + 0.02 * f, 0.12));
        }
        e.push(click(5.0, 0.85, 0.8)); // bottom-right
        e
    }

    /// §5.1 #3 — no zoom ever shows empty space outside the captured content.
    #[test]
    fn never_reveals_offscreen_area() {
        let cfg = ZoomConfig::default();
        let (_zooms, track) = plan_and_simulate(&sample_recording(), 8.0, 60.0, &cfg);
        for (i, s) in track.frames().iter().enumerate() {
            let half = 0.5 / s.zoom.max(1.0);
            let bounds = (half - 1e-9)..=(1.0 - half + 1e-9);
            assert!(
                bounds.contains(&s.center.x) && bounds.contains(&s.center.y),
                "frame {i}: center {:?} reveals off-screen area at zoom {}",
                s.center,
                s.zoom
            );
            assert!(s.zoom >= 1.0 - 1e-9, "zoom dipped below 1.0: {}", s.zoom);
        }
    }

    /// §5.1 #5 — tiny cursor movements / no clicks must not cause camera jumps.
    #[test]
    fn jitter_without_clicks_does_not_zoom() {
        let cfg = ZoomConfig::default();
        let mut events = Vec::new();
        for i in 0..120 {
            let t = f64::from(i) / 60.0;
            // sub-pixel wiggle around the middle
            let jx = 0.5 + 0.001 * f64::from(i % 3);
            events.push(mv(t, jx, 0.5));
        }
        let (zooms, track) = plan_and_simulate(&events, 2.0, 60.0, &cfg);
        assert!(zooms.is_empty(), "jitter alone must not create zooms");
        let max_zoom = track
            .frames()
            .iter()
            .map(|s| s.zoom)
            .fold(1.0_f64, f64::max);
        assert!(max_zoom < 1.01, "camera zoomed on jitter: {max_zoom}");
    }

    /// A click should actually produce a meaningful zoom-in that later releases.
    #[test]
    fn click_zooms_in_then_releases() {
        let cfg = ZoomConfig::default();
        let events = [click(1.0, 0.5, 0.5)];
        let (_zooms, track) = plan_and_simulate(&events, 6.0, 60.0, &cfg);

        let peak = track.frames().iter().map(|s| s.zoom).fold(1.0, f64::max);
        assert!(peak > 1.5, "click did not zoom in enough (peak {peak})");

        // By the end of the clip (well after the hold) it should have released.
        let end = track.at(6.0).zoom;
        assert!(end < 1.2, "camera never zoomed back out (end {end})");
    }

    /// The track is deterministic — identical inputs give identical output.
    #[test]
    fn simulation_is_deterministic() {
        let cfg = ZoomConfig::default();
        let rec = sample_recording();
        let a = plan_and_simulate(&rec, 8.0, 60.0, &cfg).1;
        let b = plan_and_simulate(&rec, 8.0, 60.0, &cfg).1;
        assert_eq!(a.frames(), b.frames());
    }
}
