//! Vuoom auto-zoom: planner + spring camera.
//!
//! The heart of the product. See `docs/04-Input-and-AutoZoom.md` for the algorithm,
//! the spring math, and the default parameter table. This crate has **no GPU or OS
//! dependencies** so the planner and camera are unit-testable in isolation (M2 gate).

/// Natural log of 2, for the half-life-parameterized spring.
const LN2: f64 = 0.693_147_180_559_945_3;

/// Critically-damped spring, exact integration (frame-rate independent, no overshoot).
/// `hl` is the half-life in seconds: the time to close half the remaining distance.
/// See `docs/04-Input-and-AutoZoom.md` § "The camera".
pub fn spring_update(x: &mut f64, v: &mut f64, x_goal: f64, hl: f64, dt: f64) {
    let y = (4.0 * LN2) / hl / 2.0;
    let j0 = *x - x_goal;
    let j1 = *v + j0 * y;
    let eydt = (-y * dt).exp();
    *x = eydt * (j0 + j1 * dt) + x_goal;
    *v = eydt * (*v - j1 * y * dt);
}

/// Clamp the camera center so the zoomed viewport never reveals off-screen area.
/// `center` and the return are normalized 0..1; `zoom` >= 1.0.
pub fn clamp_camera(center: (f64, f64), zoom: f64) -> (f64, f64) {
    let half = 0.5 / zoom;
    (
        center.0.clamp(half, 1.0 - half),
        center.1.clamp(half, 1.0 - half),
    )
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
    fn camera_never_reveals_offscreen() {
        // At 2x zoom the center must stay within [0.25, 0.75] on each axis.
        let c = clamp_camera((0.0, 1.0), 2.0);
        assert_eq!(c, (0.25, 0.75));
    }
}
