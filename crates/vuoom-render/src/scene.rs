//! Resolve a `Project` at a given time into a flat, GPU-ready draw list.
//!
//! This is the bridge between the edit model and the wgpu compositor: it evaluates the
//! camera, computes the framed layout, and resolves every annotation's pixel geometry and
//! current fade opacity. Pure and unit-tested; the compositor just consumes a [`Scene`].

use crate::cursor::{idle_opacity, press_at, smooth_pos};
use crate::layout::{compute_layout, CompositeLayout, NormRect, PxRect};
use vuoom_project::{
    caption_at, ArrowStyle, CameraOverlay, CaptionPosition, Color, Corner, HighlightShape,
    InputEvent, Project,
};
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
    /// Font family name; empty = the default sans-serif.
    pub font: String,
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
    /// Draw a head at the `from` end / the `to` end.
    pub head_from: bool,
    pub head_to: bool,
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
    /// Draw an ellipse inscribed in the rect instead of the rect itself.
    pub ellipse: bool,
    pub color: Color,
}

/// The caption on screen, placed on the framed recording (it doesn't follow the zoom). The
/// compositor wraps the text to `max_w`, centers each line on `center_x` and draws a plate
/// behind it, grown up from `edge_y` (or down from it, for captions at the top).
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedCaption {
    pub text: String,
    pub center_x: f64,
    /// The caption block's outer edge: its bottom, or its top when `from_top`.
    pub edge_y: f64,
    pub from_top: bool,
    pub font_px: f64,
    /// Widest a line may run before it wraps.
    pub max_w: f64,
}

impl ResolvedCaption {
    /// Line height as a multiple of the font size.
    pub const LINE: f64 = 1.25;
    /// The plate's padding around the text, as a multiple of the font size.
    pub const PAD_X: f64 = 0.5;
    pub const PAD_Y: f64 = 0.25;

    /// A rough height of the caption block with its plate, for laying out around it before
    /// the compositor has measured the text.
    #[must_use]
    pub fn estimated_height(&self) -> f64 {
        let text_w = self.text.chars().count() as f64 * self.font_px * 0.55;
        let lines = (text_w / self.max_w.max(1.0)).ceil().clamp(1.0, 3.0);
        lines * Self::LINE * self.font_px + 2.0 * Self::PAD_Y * self.font_px
    }
}

/// The caption showing at source time `t`, placed on the framed recording `dst` at output
/// height `oh`.
fn place_caption(project: &Project, dst: PxRect, oh: f64, t: f64) -> Option<ResolvedCaption> {
    let style = project.caption_style.clamped();
    if !style.visible {
        return None;
    }
    let cue = caption_at(&project.captions, t)?;
    let text = cue.text.trim();
    if text.is_empty() {
        return None;
    }
    let font_px = f64::from(style.size) * oh;
    let margin = font_px * 0.8;
    let from_top = style.position == CaptionPosition::Top;
    Some(ResolvedCaption {
        text: text.to_string(),
        center_x: dst.x + dst.w / 2.0,
        edge_y: if from_top {
            dst.y + margin
        } else {
            dst.y + dst.h - margin
        },
        from_top,
        font_px,
        max_w: dst.w * 0.86,
    })
}

/// The re-drawn pointer resolved to output pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ResolvedCursor {
    /// Tip position in output pixels.
    pub x: f64,
    pub y: f64,
    /// Pointer height in output pixels.
    pub size: f64,
    /// Click press depth, 0 (up) to 1 (down).
    pub press: f64,
    /// 1 = fully shown; lower while a pointer that hides when idle fades.
    pub opacity: f64,
}

/// The webcam bubble resolved to output pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ResolvedCamera {
    /// Bubble rect in output pixels.
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    /// Corner radius in pixels (half the height on a square bubble: a circle).
    pub radius: f64,
    pub mirror: bool,
    /// The camera track's time to show (the frame's source time, less the track offset).
    pub t: f64,
}

/// Place the webcam bubble in its corner of the framed recording `dst`, at output height
/// `oh`, for source time `t`.
fn place_camera(o: CameraOverlay, dst: PxRect, oh: f64, t: f64) -> ResolvedCamera {
    let o = o.clamped();
    let h = f64::from(o.size) * oh;
    let w = h * o.shape.aspect();
    let m = CameraOverlay::MARGIN * oh;
    let left = matches!(o.corner, Corner::TopLeft | Corner::BottomLeft);
    let top = matches!(o.corner, Corner::TopLeft | Corner::TopRight);
    ResolvedCamera {
        x: if left {
            dst.x + m
        } else {
            dst.x + dst.w - m - w
        },
        y: if top {
            dst.y + m
        } else {
            dst.y + dst.h - m - h
        },
        w,
        h,
        radius: h * o.shape.radius(),
        mirror: o.mirror,
        t: t - o.offset,
    }
}

/// Height of a size-1.0 pointer as a fraction of the recorded screen's height (about a
/// real pointer on a 1080p screen).
const CURSOR_BASE: f64 = 0.021;

/// Motion blur's exposure: the camera movement over this much time before a frame is
/// smeared across it (a 180 degree shutter at 30 fps).
pub const BLUR_EXPOSURE: f64 = 1.0 / 60.0;
/// Camera movement over the exposure below this many output pixels isn't blurred.
const BLUR_MIN_PX: f64 = 0.5;

/// Everything the compositor draws for one output frame.
#[derive(Debug, Clone, PartialEq)]
pub struct Scene {
    pub layout: CompositeLayout,
    pub texts: Vec<ResolvedText>,
    pub arrows: Vec<ResolvedArrow>,
    pub highlights: Vec<ResolvedHighlight>,
    /// Click ripples (expanding fading rings), kept separate from `highlights` because
    /// the live preview clears the annotation lists (the editor overlay draws those)
    /// but ripples must still show.
    pub ripples: Vec<ResolvedHighlight>,
    /// Keystroke-overlay chip backgrounds (separate from `highlights` for the same reason).
    pub key_chips: Vec<ResolvedHighlight>,
    /// Keystroke-overlay labels (separate from `texts` for the same reason).
    pub key_texts: Vec<ResolvedText>,
    /// The re-drawn pointer, when the project has one and it's in view.
    pub cursor: Option<ResolvedCursor>,
    /// The webcam bubble, when the take has one and it's shown.
    pub camera: Option<ResolvedCamera>,
    /// The caption on screen, if any. Kept apart from `texts`, which the live preview
    /// clears (the editor overlay draws those).
    pub caption: Option<ResolvedCaption>,
    /// Motion blur: the source crop one exposure ago, while the camera is moving visibly.
    /// The compositor smears the picture from there to `layout.src_rect`.
    pub blur_from: Option<NormRect>,
}

fn fade(color: Color, opacity: f64) -> Color {
    color.with_alpha(color.a * opacity as f32)
}

/// Where the camera's crop was one [`BLUR_EXPOSURE`] before `t`, if motion blur is on and
/// the move since then shows (some corner moved at least [`BLUR_MIN_PX`] in the output).
fn blur_origin(
    project: &Project,
    camera: &CameraTrack,
    layout: &CompositeLayout,
    out_w: u32,
    out_h: u32,
    t: f64,
) -> Option<NormRect> {
    if !project.motion_blur {
        return None;
    }
    let cam = camera.at((t - BLUR_EXPOSURE).max(0.0));
    let before = compute_layout(out_w, out_h, &project.frame, &cam, project.crop);
    let (prev, cur, dst) = (before.src_rect, layout.src_rect, layout.dst_rect);
    // Output pixels per unit of normalized source, across and down.
    let kx = dst.w / cur.w.max(1e-9);
    let ky = dst.h / cur.h.max(1e-9);
    let moved = [
        (prev.x - cur.x) * kx,
        (prev.y - cur.y) * ky,
        (prev.x + prev.w - cur.x - cur.w) * kx,
        (prev.y + prev.h - cur.y - cur.h) * ky,
    ];
    let most = moved.iter().fold(0.0f64, |m, d| m.max(d.abs()));
    (most >= BLUR_MIN_PX).then_some(prev)
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
    let layout = compute_layout(out_w, out_h, &project.frame, &cam, project.crop);
    let blur_from = blur_origin(project, camera, &layout, out_w, out_h, t);
    let ow = f64::from(out_w);
    let oh = f64::from(out_h);

    let mut texts = Vec::new();
    let mut highlights = Vec::new();
    for ta in &project.texts {
        let o = ta.range.opacity_at(t);
        if o <= 0.0 {
            continue;
        }
        let font_px = f64::from(ta.font_size) * oh;
        if ta.background {
            // A translucent plate behind the glyphs (estimated text box, every line of it),
            // drawn in the shape pass so it sits under the text. Mirrors the keystroke-chip
            // backing.
            let widest = ta.text.split('\n').map(|l| l.chars().count()).max();
            let lines = ta.text.split('\n').count().max(1) as f64;
            let tw = widest.unwrap_or(0) as f64 * font_px * 0.6;
            let pad_x = font_px * 0.3;
            let pad_y = font_px * 0.16;
            highlights.push(ResolvedHighlight {
                x: ta.pos.x * ow - pad_x,
                y: ta.pos.y * oh - pad_y,
                w: tw + pad_x * 2.0,
                h: font_px * 1.25 * lines + pad_y * 2.0,
                thickness_px: 0.0,
                filled: true,
                ellipse: false,
                color: fade(Color::rgb(0.05, 0.05, 0.06).with_alpha(0.7), o),
            });
        }
        texts.push(ResolvedText {
            text: ta.text.clone(),
            x: ta.pos.x * ow,
            y: ta.pos.y * oh,
            font_px,
            color: fade(ta.color, o),
            bold: ta.bold,
            italic: ta.italic,
            font: ta.font.clone(),
        });
    }

    let mut arrows = Vec::new();
    for a in &project.arrows {
        let o = a.range.opacity_at(t);
        if o <= 0.0 {
            continue;
        }
        let (head_from, head_to) = match a.style {
            ArrowStyle::Arrow => (false, true),
            ArrowStyle::Line => (false, false),
            ArrowStyle::DoubleArrow => (true, true),
        };
        arrows.push(ResolvedArrow {
            from_x: a.from.x * ow,
            from_y: a.from.y * oh,
            to_x: a.to.x * ow,
            to_y: a.to.y * oh,
            thickness_px: f64::from(a.thickness) * oh,
            color: fade(a.color, o),
            head_from,
            head_to,
        });
    }

    for h in &project.highlights {
        let o = h.range.opacity_at(t);
        if o <= 0.0 {
            continue;
        }
        // A mask is an OPAQUE redaction block: the compositor forces a near-black fill
        // and ignores the stored color/thickness so masked content can never leak.
        let is_mask = h.shape == HighlightShape::Mask;
        highlights.push(ResolvedHighlight {
            x: h.rect.x * ow,
            y: h.rect.y * oh,
            w: h.rect.w * ow,
            h: h.rect.h * oh,
            thickness_px: if is_mask {
                0.0
            } else {
                f64::from(h.thickness) * oh
            },
            filled: h.filled || is_mask,
            ellipse: h.shape == HighlightShape::Ellipse,
            color: if is_mask {
                fade(Color::rgb(0.04, 0.04, 0.05), o)
            } else {
                fade(h.color, o)
            },
        });
    }

    let mut ripples = Vec::new();
    if project.show_clicks {
        /// How long one ripple lives (s).
        const RIPPLE_LEN: f64 = 0.45;
        let src = layout.src_rect;
        let dst = layout.dst_rect;
        for e in &project.events {
            let &InputEvent::Click { t: ct, pos, .. } = e else {
                continue;
            };
            let age = t - ct;
            if !(0.0..RIPPLE_LEN).contains(&age) {
                continue;
            }
            let p = age / RIPPLE_LEN;
            // Click positions are in source space; map through the camera crop into the
            // drawn content rect so ripples stay glued to the content while zoomed.
            let cx = dst.x + (pos.x - src.x) / src.w.max(1e-9) * dst.w;
            let cy = dst.y + (pos.y - src.y) / src.h.max(1e-9) * dst.h;
            let r = (0.008 + 0.030 * p) * oh;
            ripples.push(ResolvedHighlight {
                x: cx - r,
                y: cy - r,
                w: r * 2.0,
                h: r * 2.0,
                thickness_px: (0.0035 * oh).max(1.5),
                filled: false,
                ellipse: true,
                color: Color::rgb(1.0, 1.0, 1.0).with_alpha((1.0 - p) as f32 * 0.9),
            });
        }
    }

    // The re-drawn pointer follows its smoothed path through the camera like the ripples,
    // scales with the zoom (a real pointer is part of the picture), and is left out when
    // the camera has moved away from it (or it has faded out while resting).
    let cursor = project.cursor.and_then(|style| {
        let style = style.clamped();
        let pos = smooth_pos(&project.events, t, f64::from(style.smoothing))?;
        let opacity = if style.hide_idle {
            idle_opacity(&project.events, t)
        } else {
            1.0
        };
        let src = layout.src_rect;
        let dst = layout.dst_rect;
        let inside =
            pos.x >= src.x && pos.y >= src.y && pos.x <= src.x + src.w && pos.y <= src.y + src.h;
        (inside && opacity > 0.005).then(|| ResolvedCursor {
            x: dst.x + (pos.x - src.x) / src.w.max(1e-9) * dst.w,
            y: dst.y + (pos.y - src.y) / src.h.max(1e-9) * dst.h,
            size: f64::from(style.size) * CURSOR_BASE * dst.h / src.h.max(1e-9),
            press: press_at(&project.events, t),
            opacity,
        })
    });

    let caption = place_caption(project, layout.dst_rect, oh, t);
    // Keystroke chips stack up from here: above a caption at the bottom, if one shows.
    let mut keys_floor = oh * 0.94;
    if let Some(c) = caption.as_ref().filter(|c| !c.from_top) {
        let caption_top = c.edge_y - c.estimated_height();
        keys_floor = keys_floor.min(caption_top - 0.012 * oh);
    }

    // Keystroke overlay: the latest few shortcut chips, stacked above the bottom edge.
    let mut key_chips = Vec::new();
    let mut key_texts = Vec::new();
    if project.show_keys {
        /// How long a chip stays up (s) and how long it fades at the end.
        const SHOW: f64 = 1.1;
        const FADE: f64 = 0.25;
        let visible: Vec<_> = project
            .key_taps
            .iter()
            .filter(|k| t >= k.t && t < k.t + SHOW)
            .collect();
        for (slot, k) in visible.iter().rev().take(3).enumerate() {
            let age = t - k.t;
            let o = if age > SHOW - FADE {
                ((SHOW - age) / FADE).clamp(0.0, 1.0)
            } else {
                1.0
            };
            let font = 0.034 * oh;
            let pad_x = font * 0.7;
            let chip_h = font * 1.8;
            let text_w = k.label.chars().count() as f64 * font * 0.62;
            let chip_w = text_w + pad_x * 2.0;
            let y = keys_floor - chip_h - slot as f64 * (chip_h + 0.012 * oh);
            key_chips.push(ResolvedHighlight {
                x: ow / 2.0 - chip_w / 2.0,
                y,
                w: chip_w,
                h: chip_h,
                thickness_px: 0.0,
                filled: true,
                ellipse: false,
                color: fade(Color::rgb(0.04, 0.04, 0.05).with_alpha(0.82), o),
            });
            key_texts.push(ResolvedText {
                text: k.label.clone(),
                x: ow / 2.0 - text_w / 2.0,
                y: y + (chip_h - font * 1.25) / 2.0,
                font_px: font,
                color: fade(Color::WHITE, o),
                bold: true,
                italic: false,
                font: String::new(),
            });
        }
    }

    // The webcam bubble sits on the framed recording; it doesn't follow the zoom.
    let webcam = project
        .camera
        .filter(|c| c.visible)
        .map(|c| place_camera(c, layout.dst_rect, oh, t));

    Scene {
        layout,
        texts,
        arrows,
        highlights,
        ripples,
        key_chips,
        key_texts,
        cursor,
        camera: webcam,
        caption,
        blur_from,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::DVec2;
    use vuoom_project::{CursorStyle, SourceInfo, TextAnnotation, TimeRange};

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
            background: false,
            font: String::new(),
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
    fn redrawn_pointer_follows_the_log_only_when_enabled() {
        let mut p = project_with_text();
        p.events = vec![InputEvent::Move {
            t: 0.0,
            pos: DVec2::new(0.25, 0.5),
        }];
        let track = vuoom_zoom::simulate(&[], &[], 5.0, 60.0, &p.zoom_config);
        assert!(build_scene(&p, &track, 1000, 1000, 1.0).cursor.is_none());

        p.cursor = Some(CursorStyle::default());
        let scene = build_scene(&p, &track, 1000, 1000, 1.0);
        let c = scene.cursor.unwrap();
        let (src, dst) = (scene.layout.src_rect, scene.layout.dst_rect);
        assert!((c.x - (dst.x + (0.25 - src.x) / src.w * dst.w)).abs() < 1e-6);
        assert!((c.y - (dst.y + (0.5 - src.y) / src.h * dst.h)).abs() < 1e-6);
        let expected = f64::from(CursorStyle::DEFAULT_SIZE) * CURSOR_BASE * dst.h / src.h;
        assert!((c.size - expected).abs() < 1e-6);
        assert!(c.press.abs() < 1e-9);
        assert!((c.opacity - 1.0).abs() < 1e-9);

        // Hidden when idle: gone once it has rested a while.
        p.cursor = Some(CursorStyle {
            hide_idle: true,
            ..CursorStyle::default()
        });
        assert!(build_scene(&p, &track, 1000, 1000, 0.5).cursor.is_some());
        assert!(build_scene(&p, &track, 1000, 1000, 4.0).cursor.is_none());
    }

    #[test]
    fn motion_blur_follows_the_moving_camera_only() {
        let mut p = project_with_text();
        let zoom = vuoom_zoom::ZoomKeyframe {
            start: 1.0,
            end: 4.0,
            amount: 2.0,
            mode: vuoom_zoom::ZoomMode::Manual {
                pos: DVec2::new(0.3, 0.3),
            },
            edge_snap_ratio: 0.0,
            style: vuoom_zoom::ZoomStyle::default(),
        };
        let track = vuoom_zoom::simulate(&[], &[zoom], 5.0, 60.0, &p.zoom_config);
        let at = |p: &Project, t: f64| build_scene(p, &track, 1000, 1000, t);
        // Still before the zoom starts: nothing to blur.
        assert!(at(&p, 0.5).blur_from.is_none());
        // Mid zoom-in: blurred from a wider crop an exposure ago.
        let moving = (10..40)
            .map(|k| f64::from(k) * 0.05)
            .map(|t| at(&p, t))
            .find(|s| s.blur_from.is_some())
            .expect("the camera moves during the zoom");
        let from = moving.blur_from.unwrap();
        assert!(
            from.w > moving.layout.src_rect.w,
            "zooming in: the crop was wider"
        );
        // Off: never.
        p.motion_blur = false;
        let still = |k: i32| at(&p, f64::from(k) * 0.05).blur_from.is_none();
        assert!((10..40).all(still));
    }

    #[test]
    fn the_camera_bubble_sits_in_its_corner() {
        let mut p = project_with_text();
        let track = vuoom_zoom::simulate(&[], &[], 5.0, 60.0, &p.zoom_config);
        assert!(build_scene(&p, &track, 1000, 1000, 1.0).camera.is_none());
        p.camera = Some(CameraOverlay {
            offset: -0.25,
            ..CameraOverlay::default()
        });
        let scene = build_scene(&p, &track, 1000, 1000, 1.0);
        let c = scene.camera.unwrap();
        let dst = scene.layout.dst_rect;
        let m = CameraOverlay::MARGIN * 1000.0;
        // Bottom right, a circle of the default size.
        assert!((c.x + c.w - (dst.x + dst.w - m)).abs() < 1e-6);
        assert!((c.y + c.h - (dst.y + dst.h - m)).abs() < 1e-6);
        let size = f64::from(CameraOverlay::DEFAULT_SIZE) * 1000.0;
        assert!((c.h - size).abs() < 1e-6);
        assert!((c.w - c.h).abs() < 1e-9);
        assert!((c.radius - c.h / 2.0).abs() < 1e-9);
        // The camera's own clock runs behind by the offset.
        assert!((c.t - 1.25).abs() < 1e-9);
        // Hidden: no bubble.
        p.camera = Some(CameraOverlay {
            visible: false,
            ..CameraOverlay::default()
        });
        assert!(build_scene(&p, &track, 1000, 1000, 1.0).camera.is_none());
    }

    #[test]
    fn the_caption_on_screen_sits_on_the_frame() {
        use vuoom_project::{Caption, KeyTap};
        let mut p = project_with_text();
        p.captions.push(Caption {
            id: 1,
            text: " Hello there. ".into(),
            range: TimeRange::new(1.0, 2.0),
        });
        let track = vuoom_zoom::simulate(&[], &[], 5.0, 60.0, &p.zoom_config);
        assert!(build_scene(&p, &track, 1000, 1000, 0.5).caption.is_none());
        let scene = build_scene(&p, &track, 1000, 1000, 1.5);
        let c = scene.caption.unwrap();
        let dst = scene.layout.dst_rect;
        assert_eq!(c.text, "Hello there.");
        assert!(!c.from_top);
        assert!((c.center_x - (dst.x + dst.w / 2.0)).abs() < 1e-9);
        assert!(c.edge_y < dst.y + dst.h);
        assert!((c.font_px - 45.0).abs() < 1e-3);

        // Keystroke chips move up out of its way.
        p.show_keys = true;
        p.key_taps.push(KeyTap {
            t: 1.4,
            label: "Ctrl+S".into(),
        });
        let scene = build_scene(&p, &track, 1000, 1000, 1.5);
        let chip = scene.key_chips[0];
        let c = scene.caption.unwrap();
        assert!(chip.y + chip.h < c.edge_y - c.estimated_height());

        // At the top, and hidden.
        p.caption_style.position = CaptionPosition::Top;
        let c = build_scene(&p, &track, 1000, 1000, 1.5).caption.unwrap();
        assert!(c.from_top);
        assert!(c.edge_y > dst.y && c.edge_y < dst.y + dst.h / 2.0);
        p.caption_style.visible = false;
        assert!(build_scene(&p, &track, 1000, 1000, 1.5).caption.is_none());
    }

    #[test]
    fn text_outside_time_window_is_dropped() {
        let p = project_with_text();
        let track = vuoom_zoom::simulate(&[], &[], 5.0, 60.0, &p.zoom_config);
        let scene = build_scene(&p, &track, 1000, 1000, 4.0); // after the text disappears
        assert!(scene.texts.is_empty());
    }
}
