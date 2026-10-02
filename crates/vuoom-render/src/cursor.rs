//! The re-drawn cursor's path: where the pointer is at any moment, smoothed without lag.
//!
//! A take recorded with the pointer hidden keeps the cursor only in the input log. Between
//! logged positions the path is interpolated where the mouse was actually moving; a gap in
//! the log means the pointer sat still (the hook only fires on movement), so it holds.
//! The path is then smoothed with a centered Gaussian: export knows the future of the path,
//! so hand jitter disappears without the pointer trailing behind. A click briefly presses
//! the pointer down, and a pointer set to hide when idle fades out while it rests.

use glam::DVec2;
use vuoom_project::{InputEvent, PointerShape};

/// Log gaps longer than this mean the pointer was resting, not moving.
const MOTION_GAP: f64 = 0.06;
/// A click's press animation: down fast, back up slower (seconds).
const PRESS_DOWN: f64 = 0.06;
const PRESS_UP: f64 = 0.22;
/// Stillness after which a pointer that hides when idle starts fading out, and how long
/// that takes (seconds)...
const IDLE_AFTER: f64 = 1.5;
const IDLE_FADE_OUT: f64 = 0.35;
/// ...and how long it takes to fade back in, finishing as the next movement starts: export
/// knows when that is, so the pointer is fully there the moment it moves.
const IDLE_FADE_IN: f64 = 0.2;

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

fn smoothstep(x: f64) -> f64 {
    let x = x.clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

/// How visible a pointer that hides when idle is at `t`, 0 to 1: fully shown while it
/// moves (or clicks) and for [`IDLE_AFTER`] after, fading out while it rests, and back to
/// fully shown by its next movement. Before the first logged event, the rest is counted
/// from the start of the take.
#[must_use]
pub fn idle_opacity(events: &[InputEvent], t: f64) -> f64 {
    let i = events.partition_point(|e| e.t() <= t);
    let last = events[..i]
        .iter()
        .rev()
        .find(|e| e.pos().is_some())
        .map_or(0.0, InputEvent::t);
    let next = events[i..]
        .iter()
        .find(|e| e.pos().is_some())
        .map(InputEvent::t);
    let resting = 1.0 - smoothstep((t - last - IDLE_AFTER) / IDLE_FADE_OUT);
    let arriving = next.map_or(0.0, |n| 1.0 - smoothstep((n - t) / IDLE_FADE_IN));
    resting.max(arriving)
}

/// An axis-aligned rectangle as a polygon.
const fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> [[f64; 2]; 4] {
    [[x0, y0], [x1, y0], [x1, y1], [x0, y1]]
}

/// The text beam: a stem with a serif at each end, centered on the hotspot.
const BEAM: [[[f64; 2]; 4]; 3] = [
    rect(-0.03, -0.42, 0.03, 0.42),
    rect(-0.15, -0.48, 0.15, -0.42),
    rect(-0.15, 0.42, 0.15, 0.48),
];

/// The crosshair, centered on the hotspot.
const CROSS: [[[f64; 2]; 4]; 2] = [
    rect(-0.42, -0.028, 0.42, 0.028),
    rect(-0.028, -0.42, 0.028, 0.42),
];

/// A left-right resize arrow, centered on the hotspot: its shaft and two heads.
const RESIZE_SHAFT: [[f64; 2]; 4] = rect(-0.26, -0.045, 0.26, 0.045);
const RESIZE_HEADS: [[[f64; 2]; 3]; 2] = [
    [[-0.5, 0.0], [-0.26, -0.19], [-0.26, 0.19]],
    [[0.5, 0.0], [0.26, 0.19], [0.26, -0.19]],
];
/// The square where the move pointer's two arrows cross.
const MOVE_HUB: [[f64; 2]; 4] = rect(-0.105, -0.105, 0.105, 0.105);

/// The pointing hand, hotspot at the tip of the index finger: the four fingers, the palm
/// and the thumb.
const HAND_FINGERS: [[[f64; 2]; 4]; 4] = [
    rect(-0.065, 0.0, 0.065, 0.50),
    rect(0.065, 0.27, 0.185, 0.55),
    rect(0.185, 0.31, 0.300, 0.58),
    rect(0.300, 0.36, 0.410, 0.62),
];
const HAND_PALM: [[f64; 2]; 6] = [
    [-0.065, 0.42],
    [0.41, 0.42],
    [0.41, 0.70],
    [0.33, 0.88],
    [0.02, 0.88],
    [-0.065, 0.74],
];
const HAND_THUMB: [[f64; 2]; 4] = [
    [-0.065, 0.50],
    [-0.065, 0.74],
    [-0.245, 0.585],
    [-0.185, 0.47],
];

/// `part` turned by `degrees` about the hotspot.
fn turned(part: &[[f64; 2]], degrees: f64) -> Vec<[f64; 2]> {
    let (sin, cos) = degrees.to_radians().sin_cos();
    part.iter()
        .map(|&[x, y]| [x * cos - y * sin, x * sin + y * cos])
        .collect()
}

/// The left-right resize arrow turned by `degrees`.
fn resize_arrow(degrees: f64) -> Vec<Vec<[f64; 2]>> {
    let mut parts = vec![turned(&RESIZE_SHAFT, degrees)];
    parts.extend(RESIZE_HEADS.iter().map(|head| turned(head, degrees)));
    parts
}

/// A pointer shape other than the arrow, as convex polygons in pointer units (the hotspot
/// at the origin, the arrow one unit tall). Each is filled on its own, so together they
/// may overlap. `None` for the arrow, which has its own polygon ([`ARROW`]).
#[must_use]
pub fn pointer_parts(shape: PointerShape) -> Option<Vec<Vec<[f64; 2]>>> {
    let list = |parts: &[[[f64; 2]; 4]]| -> Vec<Vec<[f64; 2]>> {
        parts.iter().map(|p| p.to_vec()).collect()
    };
    Some(match shape {
        PointerShape::Arrow => return None,
        PointerShape::Text => list(&BEAM),
        PointerShape::Cross => list(&CROSS),
        PointerShape::ResizeH => resize_arrow(0.0),
        PointerShape::ResizeV => resize_arrow(90.0),
        PointerShape::ResizeNwse => resize_arrow(45.0),
        PointerShape::ResizeNesw => resize_arrow(-45.0),
        PointerShape::Move => {
            let mut parts = resize_arrow(0.0);
            parts.extend(resize_arrow(90.0));
            parts.push(MOVE_HUB.to_vec());
            parts
        }
        PointerShape::Hand => {
            let mut parts = list(&HAND_FINGERS);
            parts.push(HAND_PALM.to_vec());
            parts.push(HAND_THUMB.to_vec());
            parts
        }
    })
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
    fn an_idle_pointer_fades_out_and_is_back_before_it_moves() {
        let ev = [mv(0.0, 0.5, 0.5), mv(0.02, 0.51, 0.5), mv(5.0, 0.6, 0.5)];
        // Moving, and for a while after.
        assert!((idle_opacity(&ev, 0.01) - 1.0).abs() < 1e-9);
        assert!((idle_opacity(&ev, 1.5) - 1.0).abs() < 1e-9);
        // Fading, then gone while it rests.
        let fading = idle_opacity(&ev, 0.02 + IDLE_AFTER + IDLE_FADE_OUT / 2.0);
        assert!(fading > 0.1 && fading < 0.9, "{fading}");
        assert!(idle_opacity(&ev, 3.0).abs() < 1e-9);
        // Coming back ahead of the next move, fully there when it starts.
        let back = idle_opacity(&ev, 5.0 - IDLE_FADE_IN / 2.0);
        assert!(back > 0.1 && back < 0.9, "{back}");
        assert!((idle_opacity(&ev, 5.0) - 1.0).abs() < 1e-9);
        // Typing doesn't count as the pointer moving.
        let typing = [mv(0.0, 0.5, 0.5), E::KeyType { t: 2.9 }, mv(9.0, 0.6, 0.5)];
        assert!(idle_opacity(&typing, 3.0).abs() < 1e-9);
    }

    #[test]
    fn a_click_wakes_a_resting_pointer() {
        let ev = [
            mv(0.0, 0.5, 0.5),
            E::Click {
                t: 4.0,
                pos: DVec2::new(0.5, 0.5),
                button: MouseButton::Left,
            },
        ];
        assert!(idle_opacity(&ev, 3.0).abs() < 1e-9);
        assert!((idle_opacity(&ev, 4.5) - 1.0).abs() < 1e-9);
    }

    #[test]
    fn every_pointer_shape_is_made_of_convex_parts_near_the_hotspot() {
        let shapes = [
            PointerShape::Text,
            PointerShape::Hand,
            PointerShape::Cross,
            PointerShape::ResizeH,
            PointerShape::ResizeV,
            PointerShape::ResizeNwse,
            PointerShape::ResizeNesw,
            PointerShape::Move,
        ];
        assert!(pointer_parts(PointerShape::Arrow).is_none());
        for shape in shapes {
            let parts = pointer_parts(shape).expect("a drawn shape");
            assert!(!parts.is_empty());
            for part in &parts {
                assert!(part.len() >= 3);
                // Convex: every corner turns the same way (so a fan from the first
                // corner fills it exactly).
                let n = part.len();
                let turns: Vec<f64> = (0..n)
                    .map(|i| {
                        let (a, b, c) = (part[i], part[(i + 1) % n], part[(i + 2) % n]);
                        (b[0] - a[0]) * (c[1] - b[1]) - (b[1] - a[1]) * (c[0] - b[0])
                    })
                    .collect();
                let one_way = turns.iter().all(|t| *t > 0.0) || turns.iter().all(|t| *t < 0.0);
                assert!(one_way, "{shape:?} has a part that isn't convex: {turns:?}");
                // No bigger than the arrow: within a unit of the hotspot.
                assert!(part.iter().all(|p| p[0].abs() <= 1.0 && p[1].abs() <= 1.0));
            }
        }
        // A turned arrow keeps its size: the diagonal's tips are as far out as the flat one's.
        let far = |parts: &[Vec<[f64; 2]>]| {
            let all = parts.iter().flatten();
            all.map(|p| p[0].hypot(p[1])).fold(0.0f64, f64::max)
        };
        let flat = far(&resize_arrow(0.0));
        assert!((far(&resize_arrow(45.0)) - flat).abs() < 1e-9);
        assert!((flat - 0.5).abs() < 1e-9);
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
