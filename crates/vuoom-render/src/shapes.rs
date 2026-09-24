//! Lightweight triangle geometry for flat annotation shapes (highlight boxes + arrows) and
//! the re-drawn pointer. Generated manually (no tessellation dependency) and drawn with
//! `shaders/shapes.wgsl`.

use crate::cursor::{offset_polygon, ARROW, ARROW_TRIS};
use crate::scene::{ResolvedArrow, ResolvedCursor, ResolvedHighlight, Scene};
use vuoom_project::Color;

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

/// The pointer: a soft shadow, a dark outline, then the white body. A click presses it
/// slightly smaller toward its tip.
fn cursor(out: &mut Vec<ShapeVertex>, c: &ResolvedCursor) {
    let scale = c.size * (1.0 - 0.14 * c.press.clamp(0.0, 1.0));
    const SHADOW: [f32; 4] = [0.0, 0.0, 0.0, 0.22];
    const OUTLINE: [f32; 4] = [0.04, 0.04, 0.05, 0.95];
    const BODY: [f32; 4] = [1.0, 1.0, 1.0, 1.0];
    let shadow = offset_polygon(&ARROW, 0.09);
    let outline = offset_polygon(&ARROW, 0.06);
    fill_arrow(out, c, scale, &shadow, [0.03, 0.06], SHADOW);
    fill_arrow(out, c, scale, &outline, [0.0, 0.0], OUTLINE);
    fill_arrow(out, c, scale, &ARROW, [0.0, 0.0], BODY);
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

/// Build the triangle list for all of a scene's highlights and arrows.
#[must_use]
pub fn build_shape_vertices(scene: &Scene) -> Vec<ShapeVertex> {
    let mut out = Vec::new();
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
    for a in &scene.arrows {
        arrow(&mut out, a);
    }
    // Last, so the pointer sits above everything it points at.
    if let Some(c) = &scene.cursor {
        cursor(&mut out, c);
    }
    out
}
