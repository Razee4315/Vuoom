//! Lightweight triangle geometry for flat annotation shapes (highlight boxes, pen strokes,
//! arrows) and the re-drawn pointer. Generated manually (no tessellation dependency) and
//! drawn with `shaders/shapes.wgsl`.

use crate::cursor::{offset_polygon, pointer_parts, ARROW, ARROW_TRIS};
use crate::scene::{
    ResolvedArrow, ResolvedCorner, ResolvedCursor, ResolvedHighlight, ResolvedStroke, Scene,
};
use std::f32::consts::{PI, TAU};
use vuoom_project::{Color, PointerLook};

/// A colored 2D vertex in output-pixel space.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct ShapeVertex {
    pub pos: [f32; 2],
    pub color: [f32; 4],
}

fn col(c: Color) -> [f32; 4] {
    [c.r, c.g, c.b, c.a]
}

/// Push a quad (corners in order) as two triangles.
fn push_quad(out: &mut Vec<ShapeVertex>, corners: [[f32; 2]; 4], color: [f32; 4]) {
    let [a, b, c, d] = corners;
    for pos in [a, b, c, a, c, d] {
        out.push(ShapeVertex { pos, color });
    }
}

fn highlight(out: &mut Vec<ShapeVertex>, h: &ResolvedHighlight) {
    let color = col(h.color);
    let (x, y, w, hh) = (h.x as f32, h.y as f32, h.w as f32, h.h as f32);
    let t = (h.thickness_px as f32).max(1.0);
    if h.filled {
        push_quad(
            out,
            [[x, y], [x + w, y], [x + w, y + hh], [x, y + hh]],
            color,
        );
    } else {
        push_quad(out, [[x, y], [x + w, y], [x + w, y + t], [x, y + t]], color);
        push_quad(
            out,
            [
                [x, y + hh - t],
                [x + w, y + hh - t],
                [x + w, y + hh],
                [x, y + hh],
            ],
            color,
        );
        push_quad(
            out,
            [[x, y], [x + t, y], [x + t, y + hh], [x, y + hh]],
            color,
        );
        push_quad(
            out,
            [
                [x + w - t, y],
                [x + w, y],
                [x + w, y + hh],
                [x + w - t, y + hh],
            ],
            color,
        );
    }
}

/// Segments used to approximate an ellipse, plenty for screen-sized highlights.
const ELLIPSE_SEGS: u32 = 48;

fn ellipse(out: &mut Vec<ShapeVertex>, h: &ResolvedHighlight) {
    let color = col(h.color);
    let cx = (h.x + h.w / 2.0) as f32;
    let cy = (h.y + h.h / 2.0) as f32;
    let rx = (h.w / 2.0) as f32;
    let ry = (h.h / 2.0) as f32;
    let t = (h.thickness_px as f32).max(1.0);
    let step = std::f32::consts::TAU / ELLIPSE_SEGS as f32;
    for i in 0..ELLIPSE_SEGS {
        let a0 = i as f32 * step;
        let a1 = a0 + step;
        let p0 = [cx + rx * a0.cos(), cy + ry * a0.sin()];
        let p1 = [cx + rx * a1.cos(), cy + ry * a1.sin()];
        if h.filled {
            // Triangle fan from the center.
            for pos in [[cx, cy], p0, p1] {
                out.push(ShapeVertex { pos, color });
            }
        } else {
            // A ring: quads between the outer ellipse and one inset by the thickness.
            let irx = (rx - t).max(0.0);
            let iry = (ry - t).max(0.0);
            let q0 = [cx + irx * a0.cos(), cy + iry * a0.sin()];
            let q1 = [cx + irx * a1.cos(), cy + iry * a1.sin()];
            push_quad(out, [p0, p1, q1, q0], color);
        }
    }
}

/// Segments in a spotlight's rounded corner (a quarter circle).
const CORNER_SEGS: u32 = 12;

/// The dark sliver outside a spotlight's rounded corner: a fan from the square corner to
/// the quarter circle, whose centre sits one radius into the clear area on both axes.
fn corner_fill(out: &mut Vec<ShapeVertex>, c: &ResolvedCorner) {
    let color = col(c.color);
    let corner = [c.x as f32, c.y as f32];
    let (rx, ry) = ((c.dx * c.r) as f32, (c.dy * c.r) as f32);
    let (cx, cy) = (corner[0] + rx, corner[1] + ry);
    let step = std::f32::consts::FRAC_PI_2 / CORNER_SEGS as f32;
    for i in 0..CORNER_SEGS {
        let a0 = i as f32 * step;
        let a1 = a0 + step;
        let p0 = [cx - rx * a0.cos(), cy - ry * a0.sin()];
        let p1 = [cx - rx * a1.cos(), cy - ry * a1.sin()];
        for pos in [corner, p0, p1] {
            out.push(ShapeVertex { pos, color });
        }
    }
}

/// Segments in a stroke's round end (half a circle).
const CAP_SEGS: u32 = 10;

/// A triangle fan around `c` of radius `r`: from angle `arc[0]`, sweeping `arc[1]`.
fn fan(out: &mut Vec<ShapeVertex>, c: [f32; 2], r: f32, arc: [f32; 2], color: [f32; 4]) {
    let segs = CAP_SEGS * if arc[1] < TAU { 1 } else { 2 };
    let step = arc[1] / segs as f32;
    for i in 0..segs {
        let a0 = arc[0] + i as f32 * step;
        let a1 = a0 + step;
        let p0 = [c[0] + r * a0.cos(), c[1] + r * a0.sin()];
        let p1 = [c[0] + r * a1.cos(), c[1] + r * a1.sin()];
        for pos in [c, p0, p1] {
            out.push(ShapeVertex { pos, color });
        }
    }
}

/// The unit normal of the segment from `a` to `b` (a quarter turn from its direction).
fn unit_normal(a: [f32; 2], b: [f32; 2]) -> [f32; 2] {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let len = dx.hypot(dy).max(1e-3);
    [-dy / len, dx / len]
}

/// The offset at a joint between segments with normals `a` and `b`: their average, scaled
/// so the stroke's sides stay parallel to both (a miter), at most twice as long on sharp
/// turns.
fn miter(a: [f32; 2], b: [f32; 2]) -> [f32; 2] {
    let m = [a[0] + b[0], a[1] + b[1]];
    let len = m[0].hypot(m[1]);
    if len < 1e-3 {
        return b;
    }
    let cos = (m[0] * b[0] + m[1] * b[1]) / len;
    let k = 1.0 / (cos.max(0.5) * len);
    [m[0] * k, m[1] * k]
}

/// A pen stroke: a strip along the path, its sides half the thickness out on each side
/// and mitered at every point, with a round cap at both ends. A single point is a dot.
fn stroke(out: &mut Vec<ShapeVertex>, s: &ResolvedStroke) {
    let color = col(s.color);
    let h = (s.thickness_px as f32).max(1.0) / 2.0;
    // Points closer than a pixel to the last kept one only add slivers.
    let mut pts: Vec<[f32; 2]> = Vec::with_capacity(s.points.len());
    for p in &s.points {
        let q = [p[0] as f32, p[1] as f32];
        match pts.last() {
            Some(l) if (q[0] - l[0]).hypot(q[1] - l[1]) < 1.0 => {}
            _ => pts.push(q),
        }
    }
    let Some(&first) = pts.first() else {
        return;
    };
    if pts.len() == 1 {
        fan(out, first, h, [0.0, TAU], color);
        return;
    }
    let mut normals = Vec::with_capacity(pts.len());
    for w in pts.windows(2) {
        normals.push(unit_normal(w[0], w[1]));
    }
    let last = normals.len() - 1;
    let mut left = Vec::with_capacity(pts.len());
    let mut right = Vec::with_capacity(pts.len());
    for (i, p) in pts.iter().enumerate() {
        let n = if i == 0 {
            normals[0]
        } else if i > last {
            normals[last]
        } else {
            miter(normals[i - 1], normals[i])
        };
        left.push([p[0] + n[0] * h, p[1] + n[1] * h]);
        right.push([p[0] - n[0] * h, p[1] - n[1] * h]);
    }
    for (l, r) in left.windows(2).zip(right.windows(2)) {
        push_quad(out, [l[0], l[1], r[1], r[0]], color);
    }
    // Round ends: half discs from one side to the other, around the outside.
    let start = normals[0][1].atan2(normals[0][0]);
    fan(out, first, h, [start, PI], color);
    let end = normals[last][1].atan2(normals[last][0]) + PI;
    fan(out, pts[pts.len() - 1], h, [end, PI], color);
}

fn arrow(out: &mut Vec<ShapeVertex>, a: &ResolvedArrow) {
    let color = col(a.color);
    let from = [a.from_x as f32, a.from_y as f32];
    let to = [a.to_x as f32, a.to_y as f32];
    let dx = to[0] - from[0];
    let dy = to[1] - from[1];
    let len = (dx * dx + dy * dy).sqrt().max(1e-3);
    let dir = [dx / len, dy / len];
    let perp = [-dir[1], dir[0]];
    let th = (a.thickness_px as f32).max(1.0);
    // A slightly larger head with a guaranteed minimum so thin arrows still read, capped
    // so it never overruns a short arrow.
    let head = (th * 4.0).max(10.0).min(len * 0.5);
    let head_hw = head * 0.55;
    let h = th / 2.0;

    // Pull the shaft in on whichever end carries a head, so the head isn't doubled over.
    let s_from = if a.head_from {
        [from[0] + dir[0] * head, from[1] + dir[1] * head]
    } else {
        from
    };
    let s_to = if a.head_to {
        [to[0] - dir[0] * head, to[1] - dir[1] * head]
    } else {
        to
    };

    push_quad(
        out,
        [
            [s_from[0] + perp[0] * h, s_from[1] + perp[1] * h],
            [s_to[0] + perp[0] * h, s_to[1] + perp[1] * h],
            [s_to[0] - perp[0] * h, s_to[1] - perp[1] * h],
            [s_from[0] - perp[0] * h, s_from[1] - perp[1] * h],
        ],
        color,
    );

    let mut head_tri = |base: [f32; 2], tip: [f32; 2]| {
        out.push(ShapeVertex {
            pos: [base[0] + perp[0] * head_hw, base[1] + perp[1] * head_hw],
            color,
        });
        out.push(ShapeVertex { pos: tip, color });
        out.push(ShapeVertex {
            pos: [base[0] - perp[0] * head_hw, base[1] - perp[1] * head_hw],
            color,
        });
    };
    if a.head_to {
        head_tri(s_to, to);
    }
    if a.head_from {
        head_tri(s_from, from);
    }
}

/// The pointer's drawn height in pixels: a click presses it slightly smaller toward its
/// tip, and fading out while idle it also shrinks a little.
#[must_use]
pub fn cursor_scale(c: &ResolvedCursor) -> f64 {
    let fade = c.opacity.clamp(0.0, 1.0);
    c.size * (1.0 - 0.14 * c.press.clamp(0.0, 1.0)) * (0.8 + 0.2 * fade)
}

/// The highlight disc under the pointer, when it has one, faded with the pointer.
fn cursor_halo(out: &mut Vec<ShapeVertex>, c: &ResolvedCursor) {
    const HALO: [f32; 4] = [1.0, 0.84, 0.2, 0.32];
    /// The highlight disc's radius, in pointer heights, around the tip.
    const HALO_RADIUS: f64 = 0.8;
    if !c.halo {
        return;
    }
    let tip = [c.x as f32, c.y as f32];
    let radius = (cursor_scale(c) * HALO_RADIUS) as f32;
    let [r, g, b, a] = HALO;
    let color = [r, g, b, a * c.group_opacity() as f32];
    fan(out, tip, radius, [0.0, TAU], color);
}

/// The dot look's radius, in pointer heights.
const DOT_RADIUS: f64 = 0.24;

/// The pointer itself, fully opaque: a soft shadow (unless turned off), an outline, then
/// the body (white unless the style names a color; a dark body gets a light outline).
/// Its opacity is applied to all of it as one layer by the compositor, so the outline
/// never shows through a see-through body. Nothing for a picture pointer, which the
/// compositor draws from its texture.
#[must_use]
pub fn pointer_vertices(c: &ResolvedCursor) -> Vec<ShapeVertex> {
    const SHADOW: [f32; 4] = [0.0, 0.0, 0.0, 0.22];
    const OUTLINE: [f32; 4] = [0.04, 0.04, 0.05, 0.95];
    const LIGHT_OUTLINE: [f32; 4] = [1.0, 1.0, 1.0, 0.95];
    const BODY: [f32; 4] = [1.0, 1.0, 1.0, 1.0];
    // Grown by this much (pointer units) for the shadow and the outline, and the shadow's
    // offset down and to the right.
    const SHADOW_GROW: f64 = 0.09;
    const OUTLINE_GROW: f64 = 0.06;
    const SHADOW_SHIFT: [f64; 2] = [0.03, 0.06];
    let mut out = Vec::new();
    let scale = cursor_scale(c);
    let body = c.color.map_or(BODY, |[r, g, b]| [r, g, b, 1.0]);
    let luma = 0.2126 * body[0] + 0.7152 * body[1] + 0.0722 * body[2];
    let outline_color = if luma < 0.4 { LIGHT_OUTLINE } else { OUTLINE };
    match c.look {
        PointerLook::Image => return out,
        PointerLook::Dot => {
            let disc = |out: &mut Vec<ShapeVertex>, grow: f64, shift: [f64; 2], color| {
                let at = [
                    (c.x + shift[0] * scale) as f32,
                    (c.y + shift[1] * scale) as f32,
                ];
                let r = ((DOT_RADIUS + grow) * scale) as f32;
                fan(out, at, r, [0.0, TAU], color);
            };
            if c.shadow {
                disc(&mut out, SHADOW_GROW, SHADOW_SHIFT, SHADOW);
            }
            disc(&mut out, OUTLINE_GROW, [0.0, 0.0], outline_color);
            disc(&mut out, 0.0, [0.0, 0.0], body);
            return out;
        }
        PointerLook::Classic => {}
    }
    let Some(parts) = pointer_parts(c.shape) else {
        if c.shadow {
            let shadow = offset_polygon(&ARROW, SHADOW_GROW);
            fill_arrow(&mut out, c, scale, &shadow, SHADOW_SHIFT, SHADOW);
        }
        let outline = offset_polygon(&ARROW, OUTLINE_GROW);
        fill_arrow(&mut out, c, scale, &outline, [0.0, 0.0], outline_color);
        fill_arrow(&mut out, c, scale, &ARROW, [0.0, 0.0], body);
        return out;
    };
    // Layer by layer across all the parts, so no part's outline lies over another's body.
    if c.shadow {
        for part in &parts {
            let shadow = offset_polygon(part, SHADOW_GROW);
            fill_convex(&mut out, c, scale, &shadow, SHADOW_SHIFT, SHADOW);
        }
    }
    for part in &parts {
        let outline = offset_polygon(part, OUTLINE_GROW);
        fill_convex(&mut out, c, scale, &outline, [0.0, 0.0], outline_color);
    }
    for part in &parts {
        fill_convex(&mut out, c, scale, part, [0.0, 0.0], body);
    }
    out
}

/// Fill a convex polygon (pointer units, the hotspot at the origin) at the pointer's
/// position, shifted by `shift` pointer units: a fan of triangles from its first corner.
fn fill_convex(
    out: &mut Vec<ShapeVertex>,
    c: &ResolvedCursor,
    scale: f64,
    poly: &[[f64; 2]],
    shift: [f64; 2],
    color: [f32; 4],
) {
    let px = |p: [f64; 2]| {
        [
            (c.x + (p[0] + shift[0]) * scale) as f32,
            (c.y + (p[1] + shift[1]) * scale) as f32,
        ]
    };
    for k in 1..poly.len().saturating_sub(1) {
        for pos in [px(poly[0]), px(poly[k]), px(poly[k + 1])] {
            out.push(ShapeVertex { pos, color });
        }
    }
}

/// Fill an arrow-shaped polygon (pointer units, tip at the origin, vertices matching
/// [`ARROW`]) at the pointer's tip, shifted by `shift` pointer units.
fn fill_arrow(
    out: &mut Vec<ShapeVertex>,
    c: &ResolvedCursor,
    scale: f64,
    poly: &[[f64; 2]],
    shift: [f64; 2],
    color: [f32; 4],
) {
    let px = |p: [f64; 2]| {
        [
            (c.x + (p[0] + shift[0]) * scale) as f32,
            (c.y + (p[1] + shift[1]) * scale) as f32,
        ]
    };
    for [a, b, t] in ARROW_TRIS {
        for pos in [px(poly[a]), px(poly[b]), px(poly[t])] {
            out.push(ShapeVertex { pos, color });
        }
    }
}

/// Build the triangle list for all of a scene's highlights, strokes and arrows, and the
/// caption's `plate` (sized by the compositor, which measures the caption's text).
#[must_use]
pub fn build_shape_vertices(scene: &Scene, plate: Option<&ResolvedHighlight>) -> Vec<ShapeVertex> {
    let mut out = Vec::new();
    // Spotlight corners go with the dark bands they round off: under everything else.
    for c in &scene.spot_corners {
        corner_fill(&mut out, c);
    }
    for h in scene
        .highlights
        .iter()
        .chain(&scene.ripples)
        .chain(&scene.key_chips)
    {
        if h.ellipse {
            ellipse(&mut out, h);
        } else {
            highlight(&mut out, h);
        }
    }
    for s in &scene.strokes {
        stroke(&mut out, s);
    }
    for a in &scene.arrows {
        arrow(&mut out, a);
    }
    if let Some(p) = plate {
        highlight(&mut out, p);
    }
    // Last, under only the pointer, which sits above everything it points at (the
    // compositor draws it next: see [`pointer_vertices`]).
    if let Some(c) = &scene.cursor {
        cursor_halo(&mut out, c);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pen(points: Vec<[f64; 2]>) -> ResolvedStroke {
        ResolvedStroke {
            points,
            thickness_px: 4.0,
            color: Color::WHITE,
        }
    }

    #[test]
    fn a_stroke_is_a_strip_with_round_ends() {
        let mut out = Vec::new();
        let s = pen(vec![[10.0, 10.0], [50.0, 10.0], [50.0, 40.0]]);
        stroke(&mut out, &s);
        // Two quads, then two half discs.
        assert_eq!(out.len(), 2 * 6 + 2 * CAP_SEGS as usize * 3);
        // Nothing reaches further than half the thickness past the path.
        for v in &out {
            let [x, y] = v.pos;
            assert!((7.99..=52.01).contains(&x), "x {x}");
            assert!((7.99..=42.01).contains(&y), "y {y}");
        }
    }

    const PLAIN: ResolvedCursor = ResolvedCursor {
        x: 100.0,
        y: 100.0,
        size: 20.0,
        press: 0.0,
        opacity: 1.0,
        shape: vuoom_project::PointerShape::Arrow,
        color: None,
        halo: false,
        look: PointerLook::Classic,
        hotspot: [0.0, 0.0],
        alpha: 1.0,
        shadow: true,
    };

    /// Triangles in a full disc drawn by [`fan`].
    const DISC: usize = 2 * CAP_SEGS as usize * 3;

    #[test]
    fn a_pointer_takes_its_color() {
        let white = pointer_vertices(&PLAIN);
        // The body is drawn last: white by default.
        assert_eq!(white.last().unwrap().color, [1.0, 1.0, 1.0, 1.0]);
        let dark = pointer_vertices(&ResolvedCursor {
            color: Some([0.0, 0.0, 0.0]),
            ..PLAIN
        });
        assert_eq!(dark.last().unwrap().color, [0.0, 0.0, 0.0, 1.0]);
        // A dark body gets a light outline.
        assert!(dark.iter().any(|v| v.color == [1.0, 1.0, 1.0, 0.95]));
        // Drawn opaque even when see-through: the compositor fades it as one layer.
        let faint = pointer_vertices(&ResolvedCursor {
            alpha: 0.3,
            ..PLAIN
        });
        assert_eq!(faint.last().unwrap().color[3], 1.0);
    }

    #[test]
    fn the_shadow_can_be_turned_off() {
        let with = pointer_vertices(&PLAIN);
        let without = pointer_vertices(&ResolvedCursor {
            shadow: false,
            ..PLAIN
        });
        // The arrow's shadow is one more copy of its triangles.
        assert_eq!(with.len(), without.len() * 3 / 2);
        assert!(with.iter().any(|v| v.color == [0.0, 0.0, 0.0, 0.22]));
        assert!(!without.iter().any(|v| v.color == [0.0, 0.0, 0.0, 0.22]));
    }

    #[test]
    fn a_dot_is_three_discs_around_the_point() {
        let dot = pointer_vertices(&ResolvedCursor {
            look: PointerLook::Dot,
            ..PLAIN
        });
        assert_eq!(dot.len(), 3 * DISC);
        // Centered on the point (the shadow is the only part pushed off it).
        let body = &dot[2 * DISC..];
        let r = (DOT_RADIUS * cursor_scale(&PLAIN)) as f32 + 0.01;
        for v in body {
            let [x, y] = v.pos;
            assert!((x - 100.0).hypot(y - 100.0) <= r, "({x}, {y})");
        }
        let no_shadow = pointer_vertices(&ResolvedCursor {
            look: PointerLook::Dot,
            shadow: false,
            ..PLAIN
        });
        assert_eq!(no_shadow.len(), 2 * DISC);
    }

    #[test]
    fn a_picture_pointer_has_no_vector_body() {
        let pic = ResolvedCursor {
            look: PointerLook::Image,
            ..PLAIN
        };
        assert!(pointer_vertices(&pic).is_empty());
    }

    #[test]
    fn the_halo_fades_with_the_pointer() {
        let mut out = Vec::new();
        cursor_halo(&mut out, &PLAIN);
        assert!(out.is_empty());
        let lit = ResolvedCursor {
            halo: true,
            alpha: 0.5,
            ..PLAIN
        };
        cursor_halo(&mut out, &lit);
        assert_eq!(out.len(), DISC);
        assert_eq!(out[0].color, [1.0, 0.84, 0.2, 0.16]);
    }

    #[test]
    fn a_rounded_corner_stays_inside_its_square() {
        let mut out = Vec::new();
        let c = ResolvedCorner {
            x: 100.0,
            y: 50.0,
            dx: -1.0,
            dy: 1.0,
            r: 20.0,
            color: Color::WHITE,
        };
        corner_fill(&mut out, &c);
        assert_eq!(out.len(), CORNER_SEGS as usize * 3);
        // Top-right corner of a clear rect: the sliver lies left of x and below y.
        for v in &out {
            let [x, y] = v.pos;
            assert!((79.99..=100.01).contains(&x), "x {x}");
            assert!((49.99..=70.01).contains(&y), "y {y}");
        }
    }

    #[test]
    fn a_single_point_is_a_dot_and_repeats_are_dropped() {
        let mut out = Vec::new();
        stroke(&mut out, &pen(vec![[5.0, 5.0], [5.2, 5.1]]));
        assert_eq!(out.len(), 2 * CAP_SEGS as usize * 3);
        let mut none = Vec::new();
        stroke(&mut none, &pen(Vec::new()));
        assert!(none.is_empty());
    }
}
