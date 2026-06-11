//! Lightweight triangle geometry for flat annotation shapes (highlight boxes + arrows).
//! Generated manually (no tessellation dependency) and drawn with `shaders/shapes.wgsl`.

use crate::scene::{ResolvedArrow, ResolvedHighlight, Scene};
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

/// Segments used to approximate an ellipse — plenty for screen-sized highlights.
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
    let head = th * 3.5;
    let shaft_end = [to[0] - dir[0] * head, to[1] - dir[1] * head];
    let h = th / 2.0;

    push_quad(
        out,
        [
            [from[0] + perp[0] * h, from[1] + perp[1] * h],
            [shaft_end[0] + perp[0] * h, shaft_end[1] + perp[1] * h],
            [shaft_end[0] - perp[0] * h, shaft_end[1] - perp[1] * h],
            [from[0] - perp[0] * h, from[1] - perp[1] * h],
        ],
        color,
    );

    let hw = head * 0.6;
    out.push(ShapeVertex {
        pos: [shaft_end[0] + perp[0] * hw, shaft_end[1] + perp[1] * hw],
        color,
    });
    out.push(ShapeVertex { pos: to, color });
    out.push(ShapeVertex {
        pos: [shaft_end[0] - perp[0] * hw, shaft_end[1] - perp[1] * hw],
        color,
    });
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
    out
}
