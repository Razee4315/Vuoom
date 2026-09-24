//! The re-drawn cursor's path: where the pointer is at any moment, smoothed without lag.
//!
//! A take recorded with the pointer hidden keeps the cursor only in the input log. Between
//! logged positions the path is interpolated where the mouse was actually moving; a gap in
//! the log means the pointer sat still (the hook only fires on movement), so it holds.
//! The path is then smoothed with a centered Gaussian: export knows the future of the path,
//! so hand jitter disappears without the pointer trailing behind. A click briefly presses
//! the pointer down.

use glam::DVec2;
use vuoom_project::InputEvent;

/// Log gaps longer than this mean the pointer was resting, not moving.
const MOTION_GAP: f64 = 0.06;
/// A click's press animation: down fast, back up slower (seconds).
const PRESS_DOWN: f64 = 0.06;
const PRESS_UP: f64 = 0.22;

/// A logged pointer position: (time, position).
type Sample = (f64, DVec2);

/// The nearest positioned event at or before index `i` (walking back) and at or after it.
fn neighbors(events: &[InputEvent], i: usize) -> (Option<Sample>, Option<Sample>) {
    let before = events[..i]
        .iter()
        .rev()
        .find_map(|e| e.pos().map(|p| (e.t(), p)));
    let after = events[i..].iter().find_map(|e| e.pos().map(|p| (e.t(), p)));
    (before, after)
}

/// The recorded pointer position at `t` (normalized source space), before smoothing.
/// `None` when the log holds no positions. `events` must be sorted by time.
#[must_use]
pub fn raw_pos(events: &[InputEvent], t: f64) -> Option<DVec2> {
    let i = events.partition_point(|e| e.t() < t);
    match neighbors(events, i) {
        (Some((ta, a)), Some((tb, b))) => {
            let span = tb - ta;
            if span <= 1e-9 || t >= tb {
                Some(b)
            } else if span > MOTION_GAP {
                Some(a)
            } else {
                Some(a.lerp(b, (t - ta) / span))
            }
        }
        (Some((_, a)), None) => Some(a),
        (None, Some((_, b))) => Some(b),
        (None, None) => None,
    }
}

/// The pointer position at `t`, smoothed by a centered Gaussian of standard deviation
/// `sigma` seconds (0 = the raw path).
#[must_use]
pub fn smooth_pos(events: &[InputEvent], t: f64, sigma: f64) -> Option<DVec2> {
    if sigma <= 1e-4 {
        return raw_pos(events, t);
    }
    let mut acc = DVec2::ZERO;
    let mut total = 0.0;
    // Nine taps half a sigma apart cover ±2 sigma.
    for k in -4i32..=4 {
        let dt = f64::from(k) * sigma * 0.5;
        let w = (-(dt * dt) / (2.0 * sigma * sigma)).exp();
        if let Some(p) = raw_pos(events, t + dt) {
            acc += p * w;
            total += w;
        }
    }
    (total > 0.0).then(|| acc / total)
}

/// How far the pointer is pressed at `t` (0 = up, 1 = fully down), from the latest click.
#[must_use]
pub fn press_at(events: &[InputEvent], t: f64) -> f64 {
    let i = events.partition_point(|e| e.t() <= t);
    for e in events[..i].iter().rev() {
        let age = t - e.t();
        if age > PRESS_DOWN + PRESS_UP {
            break;
        }
        if matches!(e, InputEvent::Click { .. }) {
            return if age < PRESS_DOWN {
                age / PRESS_DOWN
            } else {
                let u = (age - PRESS_DOWN) / PRESS_UP;
                1.0 - u * u * (3.0 - 2.0 * u)
            };
        }
    }
    0.0
}

/// The classic arrow pointer as a polygon, tip at the origin, one unit tall.
pub const ARROW: [[f64; 2]; 7] = [
    [0.0, 0.0],
    [0.0, 0.80],
    [0.195, 0.625],
    [0.335, 0.93],
    [0.46, 0.875],
    [0.32, 0.575],
    [0.575, 0.565],
];

/// Triangles covering [`ARROW`] (indices into it): the body as a fan from the tip, then
/// the tail.
pub const ARROW_TRIS: [[usize; 3]; 5] = [[0, 1, 2], [0, 2, 5], [0, 5, 6], [2, 3, 4], [2, 4, 5]];

/// Offset a closed polygon's vertices along their miter normals by `d` (positive grows
/// it outward for a counter-clockwise polygon in y-down space). Miter length is capped
/// so sharp corners (the tip) don't spike.
#[must_use]
pub fn offset_polygon(poly: &[[f64; 2]], d: f64) -> Vec<[f64; 2]> {
    let n = poly.len();
    let area: f64 = (0..n)
        .map(|i| {
            let (a, b) = (poly[i], poly[(i + 1) % n]);
            a[0] * b[1] - b[0] * a[1]
        })
        .sum();
    // Normals point outward whichever way the polygon winds.
    let sign = if area >= 0.0 { 1.0 } else { -1.0 };
    let normal = |a: [f64; 2], b: [f64; 2]| {
        let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
        let len = (dx * dx + dy * dy).sqrt().max(1e-12);
        DVec2::new(dy / len, -dx / len) * sign
    };
    (0..n)
        .map(|i| {
            let prev = poly[(i + n - 1) % n];
            let cur = poly[i];
            let next = poly[(i + 1) % n];
            let n1 = normal(prev, cur);
            let n2 = normal(cur, next);
            let bis = (n1 + n2).normalize_or_zero();
            let cos = bis.dot(n1).max(0.35);
            let m = bis * (d / cos);
            [cur[0] + m.x, cur[1] + m.y]
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use vuoom_project::InputEvent as E;
    use vuoom_zoom::MouseButton;

    fn mv(t: f64, x: f64, y: f64) -> E {
        E::Move {
            t,
            pos: DVec2::new(x, y),
        }
    }

    #[test]
    fn interpolates_while_moving_and_holds_while_resting() {
        let ev = [mv(0.0, 0.0, 0.0), mv(0.02, 0.2, 0.0), mv(1.0, 0.8, 0.0)];
        // Mid-motion: halfway between the first two samples.
        assert!((raw_pos(&ev, 0.01).unwrap().x - 0.1).abs() < 1e-9);
        // A long gap: the pointer rested at 0.2 until the next logged move.
        assert!((raw_pos(&ev, 0.5).unwrap().x - 0.2).abs() < 1e-9);
        // Before the log starts / after it ends: the first / last position.
        assert!((raw_pos(&ev, -1.0).unwrap().x).abs() < 1e-9);
        assert!((raw_pos(&ev, 9.0).unwrap().x - 0.8).abs() < 1e-9);
    }

    #[test]
    fn events_without_positions_are_skipped() {
        let ev = [
            mv(0.0, 0.3, 0.3),
            E::KeyType { t: 0.01 },
            mv(0.03, 0.6, 0.3),
        ];
        assert!((raw_pos(&ev, 0.015).unwrap().x - 0.45).abs() < 1e-9);
        assert!(raw_pos(&[E::KeyType { t: 0.0 }], 0.0).is_none());
    }

    #[test]
    fn smoothing_removes_jitter_without_lag() {
        // A steady rightward drag with +/- jitter on y.
        let ev: Vec<E> = (0..=200)
            .map(|i| {
                let t = f64::from(i) * 0.005;
                let jitter = if i % 2 == 0 { 0.01 } else { -0.01 };
                mv(t, t, 0.5 + jitter)
            })
            .collect();
        let p = smooth_pos(&ev, 0.5, 0.03).unwrap();
        // Centered: no lag along the motion...
        assert!((p.x - 0.5).abs() < 0.002, "{p}");
        // ...and the jitter averages out.
        assert!((p.y - 0.5).abs() < 0.003, "{p}");
        // Zero sigma is the raw path.
        assert_eq!(smooth_pos(&ev, 0.5, 0.0), raw_pos(&ev, 0.5));
    }

    #[test]
    fn clicks_press_the_pointer_briefly() {
        let ev = [
            mv(0.0, 0.5, 0.5),
            E::Click {
                t: 1.0,
                pos: DVec2::new(0.5, 0.5),
                button: MouseButton::Left,
            },
        ];
        assert!(press_at(&ev, 0.9).abs() < 1e-9);
        assert!((press_at(&ev, 1.0 + PRESS_DOWN) - 1.0).abs() < 1e-9);
        assert!(press_at(&ev, 1.03) > 0.3 && press_at(&ev, 1.03) < 1.0);
        assert!(press_at(&ev, 1.0 + PRESS_DOWN + PRESS_UP + 0.01).abs() < 1e-9);
    }

    #[test]
    fn outline_grows_the_arrow_outward() {
        let grown = offset_polygon(&ARROW, 0.05);
        // The tip moves up-left, the far wing moves right.
        assert!(grown[0][0] < 0.0 && grown[0][1] < 0.0);
        assert!(grown[6][0] > ARROW[6][0]);
        let shrunk = offset_polygon(&ARROW, -0.02);
        assert!(shrunk[6][0] < ARROW[6][0]);
    }

    #[test]
    fn arrow_triangles_cover_the_polygon_once() {
        let tri_area = |t: &[usize; 3]| {
            let [a, b, c] = t.map(|i| ARROW[i]);
            ((b[0] - a[0]) * (c[1] - a[1]) - (c[0] - a[0]) * (b[1] - a[1])).abs() / 2.0
        };
        let sum: f64 = ARROW_TRIS.iter().map(tri_area).sum();
        let poly: f64 = (0..ARROW.len())
            .map(|i| {
                let (a, b) = (ARROW[i], ARROW[(i + 1) % ARROW.len()]);
                a[0] * b[1] - b[0] * a[1]
            })
            .sum::<f64>()
            .abs()
            / 2.0;
        assert!((sum - poly).abs() < 1e-9, "{sum} vs {poly}");
    }
}
