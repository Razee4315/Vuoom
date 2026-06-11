//! Resolve a `Project` at a given time into a flat, GPU-ready draw list.
//!
//! This is the bridge between the edit model and the wgpu compositor: it evaluates the
//! camera, computes the framed layout, and resolves every annotation's pixel geometry and
//! current fade opacity. Pure and unit-tested; the compositor just consumes a [`Scene`].

use crate::layout::{compute_layout, CompositeLayout};
use vuoom_project::{Color, Project};
use vuoom_zoom::CameraTrack;

/// A text label resolved to output pixels with fade opacity baked into its alpha.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedText {
    pub text: String,
    /// Top-left in output pixels.
    pub x: f64,
    pub y: f64,
    pub font_px: f64,
    pub color: Color,
    pub bold: bool,
    pub italic: bool,
}

/// An arrow resolved to output pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ResolvedArrow {
    pub from_x: f64,
    pub from_y: f64,
    pub to_x: f64,
    pub to_y: f64,
    pub thickness_px: f64,
    pub color: Color,
}

/// A highlight box resolved to output pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ResolvedHighlight {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    pub thickness_px: f64,
    pub filled: bool,
    pub color: Color,
}

/// Everything the compositor draws for one output frame.
#[derive(Debug, Clone, PartialEq)]
pub struct Scene {
    pub layout: CompositeLayout,
    pub texts: Vec<ResolvedText>,
    pub arrows: Vec<ResolvedArrow>,
    pub highlights: Vec<ResolvedHighlight>,
}

fn fade(color: Color, opacity: f64) -> Color {
    color.with_alpha(color.a * opacity as f32)
}

/// Build the draw list for `project` at source time `t` (seconds), at the given output size.
#[must_use]
pub fn build_scene(
    project: &Project,
    camera: &CameraTrack,
    out_w: u32,
    out_h: u32,
    t: f64,
) -> Scene {
    let cam = camera.at(t);
    let layout = compute_layout(out_w, out_h, &project.frame, &cam);
    let ow = f64::from(out_w);
    let oh = f64::from(out_h);

    let mut texts = Vec::new();
    for ta in &project.texts {
        let o = ta.range.opacity_at(t);
        if o <= 0.0 {
            continue;
        }
        texts.push(ResolvedText {
            text: ta.text.clone(),
            x: ta.pos.x * ow,
            y: ta.pos.y * oh,
            font_px: f64::from(ta.font_size) * oh,
            color: fade(ta.color, o),
            bold: ta.bold,
            italic: ta.italic,
        });
    }

    let mut arrows = Vec::new();
    for a in &project.arrows {
        let o = a.range.opacity_at(t);
        if o <= 0.0 {
            continue;
        }
        arrows.push(ResolvedArrow {
            from_x: a.from.x * ow,
            from_y: a.from.y * oh,
            to_x: a.to.x * ow,
            to_y: a.to.y * oh,
            thickness_px: f64::from(a.thickness) * oh,
            color: fade(a.color, o),
        });
    }

    let mut highlights = Vec::new();
    for h in &project.highlights {
        let o = h.range.opacity_at(t);
        if o <= 0.0 {
            continue;
        }
        highlights.push(ResolvedHighlight {
            x: h.rect.x * ow,
            y: h.rect.y * oh,
            w: h.rect.w * ow,
            h: h.rect.h * oh,
            thickness_px: f64::from(h.thickness) * oh,
            filled: h.filled,
            color: fade(h.color, o),
        });
    }

    Scene {
        layout,
        texts,
        arrows,
        highlights,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::DVec2;
    use vuoom_project::{SourceInfo, TextAnnotation, TimeRange};

    fn project_with_text() -> Project {
        let mut p = Project::new(SourceInfo {
            path: "c.mkv".into(),
            width: 1000,
            height: 1000,
            fps: 60.0,
            duration: 5.0,
        });
        p.texts.push(TextAnnotation {
            id: 1,
            text: "Hi".into(),
            pos: DVec2::new(0.1, 0.2),
            font_size: 0.05,
            color: Color::WHITE,
            bold: false,
            italic: false,
            range: TimeRange::new(1.0, 3.0),
        });
        p
    }

    #[test]
    fn visible_text_resolves_to_pixels() {
        let p = project_with_text();
        let track = vuoom_zoom::simulate(&[], &[], 5.0, 60.0, &p.zoom_config);
        let scene = build_scene(&p, &track, 1000, 1000, 2.0);
        assert_eq!(scene.texts.len(), 1);
        let t = &scene.texts[0];
        assert!((t.x - 100.0).abs() < 1e-9);
        assert!((t.y - 200.0).abs() < 1e-9);
        // font_size is f32, so allow f32->f64 rounding slack.
        assert!((t.font_px - 50.0).abs() < 1e-4);
    }

    #[test]
    fn text_outside_time_window_is_dropped() {
        let p = project_with_text();
        let track = vuoom_zoom::simulate(&[], &[], 5.0, 60.0, &p.zoom_config);
        let scene = build_scene(&p, &track, 1000, 1000, 4.0); // after the text disappears
        assert!(scene.texts.is_empty());
    }
}
