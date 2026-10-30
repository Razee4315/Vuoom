//! Resolve a `Project` at a given time into a flat, GPU-ready draw list.
//!
//! This is the bridge between the edit model and the wgpu compositor: it evaluates the
//! camera, computes the framed layout, and resolves every annotation's pixel geometry and
//! current fade opacity. Pure and unit-tested; the compositor just consumes a [`Scene`].

use crate::cursor::{idle_opacity, press_at, smooth_pos};
use crate::layout::{compute_layout, CompositeLayout, NormRect, PxRect};
use vuoom_project::{
    caption_at, pointer_shape_at, ArrowAnnotation, ArrowStyle, CameraOverlay, CaptionPosition,
    ClickEffect, ClickStyle, Color, Corner, HighlightShape, InputEvent, PointerLook, PointerShape,
    Project, Rect,
};
use vuoom_zoom::{CameraState, CameraTrack};

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
    /// The control point of a curved arrow (a quadratic Bézier), in output pixels.
    pub ctrl: Option<[f64; 2]>,
}

/// A pen stroke resolved to output pixels.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedStroke {
    /// The path in output pixels.
    pub points: Vec<[f64; 2]>,
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
    /// Draw an ellipse inscribed in the rect instead of the rect itself.
    pub ellipse: bool,
    pub color: Color,
}

/// One rounded corner of a spotlight's clear area: the dark sliver between the square
/// corner at (`x`, `y`) and the quarter circle of radius `r` that bulges toward it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ResolvedCorner {
    /// The square corner, in output pixels.
    pub x: f64,
    pub y: f64,
    /// Which way the clear area lies from the corner: +1 or -1 on each axis.
    pub dx: f64,
    pub dy: f64,
    /// Corner radius in output pixels.
    pub r: f64,
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
    /// Which pointer to draw: what the real one looked like at this moment.
    pub shape: PointerShape,
    /// The pointer's own color (RGB), or `None` for the classic white one.
    pub color: Option<[f32; 3]>,
    /// Draw a highlight disc under the pointer.
    pub halo: bool,
    /// Vuoom's pointer, a dot, or the loaded pointer picture.
    pub look: PointerLook,
    /// For a picture: the point that clicks, as a fraction of its width and height.
    pub hotspot: [f32; 2],
    /// How opaque the whole pointer is (the style's opacity), 0 to 1. Applied to the
    /// pointer as one layer, on top of the idle fade.
    pub alpha: f64,
    /// Draw a soft shadow under it.
    pub shadow: bool,
}

impl ResolvedCursor {
    /// How opaque the pointer is drawn: its own opacity and the idle fade together.
    #[must_use]
    pub fn group_opacity(&self) -> f64 {
        (self.alpha * self.opacity).clamp(0.0, 1.0)
    }
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

/// The four dark bands around a spotlight's clear rect `r` (normalized) on an `ow`×`oh`
/// frame: above, below, left and right of it.
fn spotlight_bands(r: Rect, ow: f64, oh: f64, color: Color) -> [ResolvedHighlight; 4] {
    let x0 = r.x.clamp(0.0, 1.0) * ow;
    let y0 = r.y.clamp(0.0, 1.0) * oh;
    let x1 = (r.x + r.w).clamp(0.0, 1.0) * ow;
    let y1 = (r.y + r.h).clamp(0.0, 1.0) * oh;
    let band = |x: f64, y: f64, w: f64, h: f64| ResolvedHighlight {
        x,
        y,
        w: w.max(0.0),
        h: h.max(0.0),
        thickness_px: 0.0,
        filled: true,
        ellipse: false,
        color,
    };
    [
        band(0.0, 0.0, ow, y0),
        band(0.0, y1, ow, oh - y1),
        band(0.0, y0, x0, y1 - y0),
        band(x1, y0, ow - x1, y1 - y0),
    ]
}

/// The four rounded corners of a spotlight's clear rect `r` (normalized) on an `ow`×`oh`
/// frame, for a corner `radius` given as a fraction of the height. None when the radius
/// is too small to see; never more than half the clear rect's shorter side.
fn corners_of(r: Rect, radius: f64, ow: f64, oh: f64, color: Color) -> Vec<ResolvedCorner> {
    let x0 = r.x.clamp(0.0, 1.0) * ow;
    let y0 = r.y.clamp(0.0, 1.0) * oh;
    let x1 = (r.x + r.w).clamp(0.0, 1.0) * ow;
    let y1 = (r.y + r.h).clamp(0.0, 1.0) * oh;
    let rad = (radius * oh).min((x1 - x0) / 2.0).min((y1 - y0) / 2.0);
    if rad < 0.5 {
        return Vec::new();
    }
    let corner = |x: f64, y: f64, dx: f64, dy: f64| ResolvedCorner {
        x,
        y,
        dx,
        dy,
        r: rad,
        color,
    };
    vec![
        corner(x0, y0, 1.0, 1.0),
        corner(x1, y0, -1.0, 1.0),
        corner(x0, y1, 1.0, -1.0),
        corner(x1, y1, -1.0, -1.0),
    ]
}

/// Height of a size-1.0 pointer as a fraction of the recorded screen's height (about a
/// real pointer on a 1080p screen).
const CURSOR_BASE: f64 = 0.021;

/// How tall the recorded screen is against the part of it recorded (`content`, width and
/// height in pixels): a pointer belongs to the screen, so it is as large on a small region
/// as on the whole display. Held to 2, so a small region of a big screen isn't all pointer.
fn screen_scale(project: &Project, content: (u32, u32)) -> f64 {
    if project.screen_height == 0 || content.1 == 0 {
        return 1.0;
    }
    (f64::from(project.screen_height) / f64::from(content.1)).clamp(1.0, 2.0)
}

/// Fast at first, settling at the end.
fn ease_out(x: f64) -> f64 {
    let u = 1.0 - x.clamp(0.0, 1.0);
    1.0 - u * u * u
}

/// A circle of radius `r` around `c` (output pixels): a ring `thickness` thick, or a
/// filled disc when `thickness` is `None`.
fn circle(c: [f64; 2], r: f64, thickness: Option<f64>, color: Color) -> ResolvedHighlight {
    ResolvedHighlight {
        x: c[0] - r,
        y: c[1] - r,
        w: r * 2.0,
        h: r * 2.0,
        thickness_px: thickness.unwrap_or(0.0),
        filled: thickness.is_none(),
        ellipse: true,
        color,
    }
}

/// Dots in a click's burst.
const BURST_DOTS: u32 = 8;

/// What one click shows `p` of the way (0 to 1) through its animation, centered on `c`
/// (output pixels) in a frame `oh` pixels tall.
fn click_marks(out: &mut Vec<ResolvedHighlight>, style: ClickStyle, c: [f64; 2], p: f64, oh: f64) {
    let [r, g, b] = style.color.unwrap_or([1.0, 1.0, 1.0]);
    let base = Color::rgb(r, g, b);
    let opacity = f64::from(style.opacity);
    let color = |fade: f64| base.with_alpha((fade.max(0.0) * opacity) as f32);
    // Lengths scale with the frame and the style's size.
    let k = f64::from(style.size) * oh;
    let ring_w = (0.0035 * k).max(1.5);
    match style.effect {
        // The classic: one ring growing steadily as it fades.
        ClickEffect::Ripple => {
            let radius = (0.008 + 0.030 * p) * k;
            out.push(circle(c, radius, Some(ring_w), color(1.0 - p)));
        }
        // A second ring follows the first, a third of the way behind.
        ClickEffect::Rings => {
            for delay in [0.0, 0.3] {
                let q = (p - delay) / (1.0 - delay);
                if (0.0..1.0).contains(&q) {
                    let radius = (0.008 + 0.034 * ease_out(q)) * k;
                    out.push(circle(c, radius, Some(ring_w), color(1.0 - q)));
                }
            }
        }
        // A soft disc swells under a crisp edge.
        ClickEffect::Pulse => {
            let radius = (0.01 + 0.026 * ease_out(p)) * k;
            out.push(circle(c, radius, None, color((1.0 - p) * 0.45)));
            out.push(circle(c, radius, Some(ring_w), color(1.0 - p)));
        }
        // Dots fly outward and shrink.
        ClickEffect::Burst => {
            let reach = (0.006 + 0.032 * ease_out(p)) * k;
            let dot = 0.0045 * (1.0 - 0.6 * p) * k;
            for i in 0..BURST_DOTS {
                let a = f64::from(i) * std::f64::consts::TAU / f64::from(BURST_DOTS);
                let at = [c[0] + reach * a.cos(), c[1] + reach * a.sin()];
                out.push(circle(at, dot, None, color(1.0 - p)));
            }
        }
    }
}

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
    /// The rounded corners of spotlights, drawn with their dark bands.
    pub spot_corners: Vec<ResolvedCorner>,
    /// Pen strokes, drawn over the highlights and under the arrows.
    pub strokes: Vec<ResolvedStroke>,
    /// Click ripples (expanding fading rings), kept separate from `highlights` because
    /// the live preview clears the annotation lists (the editor overlay draws those)
    /// but ripples must still show.
    pub ripples: Vec<ResolvedHighlight>,
    /// Keystroke-overlay chip backgrounds (separate from `highlights` for the same reason).
    pub key_chips: Vec<ResolvedHighlight>,
    /// Keystroke-overlay labels (separate from `texts` for the same reason).
    pub key_texts: Vec<ResolvedText>,
    /// Hide masks. Apart from `highlights` because they are drawn in the preview too: they
    /// follow the camera, and the editor overlay can only outline them.
    pub masks: Vec<ResolvedHighlight>,
    /// Masks that pixelate (x, y, width, height in output pixels).
    pub mosaics: Vec<[f32; 4]>,
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

/// The most masks that pixelate at once; any more are drawn black.
pub const MAX_MOSAICS: usize = 8;

/// A line's width in ems, near enough to size what goes behind it: a CJK character is a
/// full em, a Latin one a little over half.
fn text_em(line: &str) -> f64 {
    line.chars()
        .map(|c| if u32::from(c) >= 0x2E80 { 1.0 } else { 0.6 })
        .sum()
}

/// The control point that bows arrow `a` by its `curve`, in output pixels. `None` if straight.
fn arrow_ctrl(a: &ArrowAnnotation, ow: f64, oh: f64) -> Option<[f64; 2]> {
    if a.curve.abs() < 1e-3 {
        return None;
    }
    let (fx, fy) = (a.from.x * ow, a.from.y * oh);
    let (tx, ty) = (a.to.x * ow, a.to.y * oh);
    let (dx, dy) = (tx - fx, ty - fy);
    // A quadratic Bézier's middle is half way to its control point, hence the 2.
    let bow = f64::from(a.curve) * 2.0;
    Some([(fx + tx) / 2.0 - dy * bow, (fy + ty) / 2.0 + dx * bow])
}

/// `rect` (output pixels) as drawn in `whole` (the picture with no zoom), moved to where the
/// same part of the recording is in `now`, and clipped to the picture. `None` once it is
/// out of view.
fn follow_camera(
    rect: [f64; 4],
    whole: &CompositeLayout,
    now: &CompositeLayout,
) -> Option<[f64; 4]> {
    let (wd, ws) = (whole.dst_rect, whole.src_rect);
    let (nd, ns) = (now.dst_rect, now.src_rect);
    let map_x = |x: f64| {
        let src = ws.x + (x - wd.x) / wd.w.max(1e-9) * ws.w;
        nd.x + (src - ns.x) / ns.w.max(1e-9) * nd.w
    };
    let map_y = |y: f64| {
        let src = ws.y + (y - wd.y) / wd.h.max(1e-9) * ws.h;
        nd.y + (src - ns.y) / ns.h.max(1e-9) * nd.h
    };
    let [x, y, w, h] = rect;
    let (x0, x1) = (map_x(x).max(nd.x), map_x(x + w).min(nd.x + nd.w));
    let (y0, y1) = (map_y(y).max(nd.y), map_y(y + h).min(nd.y + nd.h));
    (x1 > x0 && y1 > y0).then_some([x0, y0, x1 - x0, y1 - y0])
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
    let content = project.effective_source_dims();
    let before = compute_layout(out_w, out_h, &project.frame, &cam, project.crop, content);
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
    let content = project.effective_source_dims();
    let layout = compute_layout(out_w, out_h, &project.frame, &cam, project.crop, content);
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
        if ta.badge {
            // `pos` is the center of the disc; the number sits in the middle of it.
            let (cx, cy) = (ta.pos.x * ow, ta.pos.y * oh);
            let r = font_px * 0.92;
            highlights.push(ResolvedHighlight {
                x: cx - r,
                y: cy - r,
                w: r * 2.0,
                h: r * 2.0,
                thickness_px: 0.0,
                filled: true,
                ellipse: true,
                color: fade(ta.color, o),
            });
            let light = ta.color.r * 0.3 + ta.color.g * 0.6 + ta.color.b * 0.1 > 0.6;
            let ink = if light {
                Color::rgb(0.05, 0.05, 0.06)
            } else {
                Color::WHITE
            };
            texts.push(ResolvedText {
                text: ta.text.clone(),
                x: cx - text_em(&ta.text) * font_px / 2.0,
                y: cy - font_px * 0.625,
                font_px,
                color: fade(ink, o),
                bold: true,
                italic: false,
                font: ta.font.clone(),
            });
            continue;
        }
        if ta.background {
            // A translucent plate behind the glyphs (estimated text box, every line of it),
            // drawn in the shape pass so it sits under the text. Mirrors the keystroke-chip
            // backing.
            let widest = ta.text.split('\n').map(text_em).fold(0.0, f64::max);
            let lines = ta.text.split('\n').count().max(1) as f64;
            let tw = widest * font_px;
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

    let mut strokes = Vec::new();
    for s in &project.strokes {
        let o = s.range.opacity_at(t);
        if o <= 0.0 || s.points.is_empty() {
            continue;
        }
        strokes.push(ResolvedStroke {
            points: s.points.iter().map(|p| [p.x * ow, p.y * oh]).collect(),
            thickness_px: f64::from(s.thickness) * oh,
            color: fade(s.color, o),
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
            ctrl: arrow_ctrl(a, ow, oh),
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

    // A spotlight's dark bands go first, so everything else stays bright over them.
    let mut dim = Vec::new();
    let mut spot_corners = Vec::new();
    for h in &project.highlights {
        let o = h.range.opacity_at(t);
        if o <= 0.0 {
            continue;
        }
        if h.shape == HighlightShape::Spotlight {
            let color = fade(h.color, o);
            dim.extend(spotlight_bands(h.rect, ow, oh, color));
            spot_corners.extend(corners_of(h.rect, f64::from(h.radius), ow, oh, color));
            continue;
        }
        // Masks are drawn on their own below.
        if h.shape == HighlightShape::Mask {
            continue;
        }
        highlights.push(ResolvedHighlight {
            x: h.rect.x * ow,
            y: h.rect.y * oh,
            w: h.rect.w * ow,
            h: h.rect.h * oh,
            thickness_px: f64::from(h.thickness) * oh,
            filled: h.filled,
            ellipse: h.shape == HighlightShape::Ellipse,
            color: fade(h.color, o),
        });
    }
    highlights.splice(0..0, dim);

    // Hide masks stay on what they cover through a zoom, or a zoom would slide private
    // content out from under them, and never fade: a mask half way through a fade doesn't
    // hide anything.
    let mut masks = Vec::new();
    let mut mosaics = Vec::new();
    let whole = compute_layout(
        out_w,
        out_h,
        &project.frame,
        &CameraState::default(),
        project.crop,
        content,
    );
    for h in &project.highlights {
        if h.shape != HighlightShape::Mask || t < h.range.start || t >= h.range.end {
            continue;
        }
        let rect = [h.rect.x * ow, h.rect.y * oh, h.rect.w * ow, h.rect.h * oh];
        let Some([x, y, w, h_px]) = follow_camera(rect, &whole, &layout) else {
            continue;
        };
        if h.pixelate && mosaics.len() < MAX_MOSAICS {
            mosaics.push([x as f32, y as f32, w as f32, h_px as f32]);
            continue;
        }
        masks.push(ResolvedHighlight {
            x,
            y,
            w,
            h: h_px,
            thickness_px: 0.0,
            filled: true,
            ellipse: false,
            color: Color::rgb(0.04, 0.04, 0.05),
        });
    }

    let mut ripples = Vec::new();
    if project.show_clicks {
        let style = project.click_style.clamped();
        let len = f64::from(style.duration);
        let src = layout.src_rect;
        let dst = layout.dst_rect;
        for e in &project.events {
            let &InputEvent::Click { t: ct, pos, .. } = e else {
                continue;
            };
            let age = t - ct;
            if !(0.0..len).contains(&age) {
                continue;
            }
            // A click outside the recorded area (the panel, another display) shows nothing.
            if !(0.0..=1.0).contains(&pos.x) || !(0.0..=1.0).contains(&pos.y) {
                continue;
            }
            // Click positions are in source space; map through the camera crop into the
            // drawn content rect so ripples stay glued to the content while zoomed.
            let cx = dst.x + (pos.x - src.x) / src.w.max(1e-9) * dst.w;
            let cy = dst.y + (pos.y - src.y) / src.h.max(1e-9) * dst.h;
            click_marks(&mut ripples, style, [cx, cy], age / len, oh);
        }
    }

    // The re-drawn pointer follows its smoothed path through the camera like the ripples,
    // scales with the zoom (a real pointer is part of the picture), and is left out when
    // the camera has moved away from it (or it has faded out while resting).
    let cursor = project.cursor.as_ref().and_then(|style| {
        let style = style.clone().clamped();
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
        let alpha = f64::from(style.opacity);
        let shown = opacity * alpha > 0.005;
        (inside && shown).then(|| ResolvedCursor {
            x: dst.x + (pos.x - src.x) / src.w.max(1e-9) * dst.w,
            y: dst.y + (pos.y - src.y) / src.h.max(1e-9) * dst.h,
            size: f64::from(style.size) * CURSOR_BASE * screen_scale(project, content) * dst.h
                / src.h.max(1e-9),
            press: press_at(&project.events, t),
            opacity,
            shape: pointer_shape_at(&project.pointer_shapes, t),
            color: style.color,
            halo: style.halo,
            look: style.drawn_look(),
            hotspot: style.image.as_ref().map_or([0.0, 0.0], |i| i.hotspot),
            alpha,
            shadow: style.shadow,
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
        spot_corners,
        strokes,
        ripples,
        key_chips,
        key_texts,
        masks,
        mosaics,
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
    use vuoom_project::{
        CursorStyle, HighlightBox, SourceInfo, StrokeAnnotation, TextAnnotation, TimeRange,
    };

    fn project_with_text() -> Project {
        let mut p = Project::new(SourceInfo {
            path: "c.mkv".into(),
            width: 1000,
            height: 1000,
            fps: 60.0,
            duration: 5.0,
        });
        p.texts.push(TextAnnotation {
            badge: false,
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

    fn mask(pixelate: bool) -> HighlightBox {
        HighlightBox {
            pixelate,
            id: 9,
            rect: Rect::new(0.25, 0.25, 0.5, 0.5),
            color: Color::rgba(1.0, 0.0, 0.0, 0.1),
            thickness: 0.0,
            filled: true,
            shape: HighlightShape::Mask,
            radius: 0.0,
            range: TimeRange::with_fade(1.0, 3.0, 0.5),
        }
    }

    #[test]
    fn a_mask_is_solid_from_its_first_moment_to_its_last() {
        let mut p = project_with_text();
        p.highlights.push(mask(false));
        let track = vuoom_zoom::simulate(&[], &[], 5.0, 60.0, &p.zoom_config);
        for t in [1.0, 1.1, 2.9] {
            let scene = build_scene(&p, &track, 1000, 1000, t);
            assert_eq!(scene.masks.len(), 1, "at {t}");
            assert!(scene.masks[0].color.a >= 1.0, "at {t}");
            assert!(scene.highlights.is_empty(), "drawn once, as a mask");
        }
        assert!(build_scene(&p, &track, 1000, 1000, 3.0).masks.is_empty());
    }

    #[test]
    fn a_mask_stays_on_what_it_covers_through_a_zoom() {
        let whole = CompositeLayout {
            src_rect: NormRect {
                x: 0.0,
                y: 0.0,
                w: 1.0,
                h: 1.0,
            },
            dst_rect: PxRect {
                x: 0.0,
                y: 0.0,
                w: 1000.0,
                h: 1000.0,
            },
            corner_radius_px: 0.0,
            shadow: crate::layout::PxShadow::default(),
        };
        // Zoomed 2x into the top-left quarter: the picture there is twice the size.
        let zoomed = CompositeLayout {
            src_rect: NormRect {
                x: 0.0,
                y: 0.0,
                w: 0.5,
                h: 0.5,
            },
            ..whole
        };
        let moved = follow_camera([100.0, 200.0, 50.0, 50.0], &whole, &zoomed);
        assert_eq!(moved, Some([200.0, 400.0, 100.0, 100.0]));
        // Half way out of view: clipped to the picture.
        let edge = follow_camera([450.0, 0.0, 100.0, 10.0], &whole, &zoomed).unwrap();
        assert_eq!(edge[0] + edge[2], 1000.0);
        // Wholly out of view: nothing to draw.
        assert_eq!(
            follow_camera([600.0, 0.0, 100.0, 10.0], &whole, &zoomed),
            None
        );
    }

    #[test]
    fn a_pixelating_mask_goes_to_the_mosaic_list() {
        let mut p = project_with_text();
        p.highlights.push(mask(true));
        let track = vuoom_zoom::simulate(&[], &[], 5.0, 60.0, &p.zoom_config);
        let scene = build_scene(&p, &track, 1000, 1000, 2.0);
        assert!(scene.masks.is_empty());
        assert_eq!(scene.mosaics, vec![[250.0, 250.0, 500.0, 500.0]]);
    }

    #[test]
    fn a_step_marker_is_a_disc_with_its_number_in_the_middle() {
        let mut p = project_with_text();
        p.texts[0].badge = true;
        p.texts[0].text = "12".into();
        p.texts[0].pos = DVec2::new(0.5, 0.5);
        p.texts[0].color = Color::rgb(0.9, 0.1, 0.1);
        let track = vuoom_zoom::simulate(&[], &[], 5.0, 60.0, &p.zoom_config);
        let scene = build_scene(&p, &track, 1000, 1000, 2.0);
        let disc = &scene.highlights[0];
        assert!(disc.ellipse && disc.filled);
        assert!((disc.x + disc.w / 2.0 - 500.0).abs() < 1e-6);
        let number = &scene.texts[0];
        assert!(number.x < 500.0 && number.bold);
        assert_eq!(number.color.g, 1.0, "white on a dark disc");
    }

    #[test]
    fn a_bowed_arrow_has_a_control_point_to_the_side() {
        let a = ArrowAnnotation {
            id: 1,
            from: DVec2::new(0.0, 0.5),
            to: DVec2::new(1.0, 0.5),
            color: Color::WHITE,
            thickness: 0.01,
            style: ArrowStyle::Arrow,
            curve: 0.25,
            range: TimeRange::new(0.0, 1.0),
        };
        assert_eq!(arrow_ctrl(&a, 100.0, 100.0), Some([50.0, 100.0]));
        assert_eq!(
            arrow_ctrl(&ArrowAnnotation { curve: 0.0, ..a }, 100.0, 100.0),
            None
        );
    }

    #[test]
    fn chinese_text_counts_as_wide() {
        assert!(text_em("中文") > text_em("ab") * 1.5);
    }

    #[test]
    fn a_spotlight_dims_all_around_its_rect_and_goes_under_the_rest() {
        let mut p = project_with_text();
        p.texts[0].background = true;
        p.highlights.push(HighlightBox {
            pixelate: false,
            id: 3,
            rect: Rect::new(0.2, 0.3, 0.4, 0.5),
            color: Color::rgba(0.0, 0.0, 0.0, 0.6),
            thickness: 0.0,
            filled: true,
            shape: HighlightShape::Spotlight,
            radius: 0.0,
            range: TimeRange::new(1.0, 3.0),
        });
        let track = vuoom_zoom::simulate(&[], &[], 5.0, 60.0, &p.zoom_config);
        let scene = build_scene(&p, &track, 1000, 1000, 2.0);
        // Four bands, then the label's plate.
        assert_eq!(scene.highlights.len(), 5);
        let bands = &scene.highlights[..4];
        let area: f64 = bands.iter().map(|b| b.w * b.h).sum();
        assert!((area - (1_000_000.0 - 400.0 * 500.0)).abs() < 1e-6);
        assert!(bands.iter().all(|b| b.filled && b.color.a > 0.5));
        // Square corners by default.
        assert!(scene.spot_corners.is_empty());

        // Rounded: one sliver per corner, each pointing into the clear rect.
        p.highlights[0].radius = 0.05;
        let scene = build_scene(&p, &track, 1000, 1000, 2.0);
        assert_eq!(scene.spot_corners.len(), 4);
        let first = scene.spot_corners[0];
        assert!((first.x - 200.0).abs() < 1e-6 && (first.y - 300.0).abs() < 1e-6);
        assert!((first.r - 50.0).abs() < 1e-3);
        assert!(first.dx > 0.0 && first.dy > 0.0);
        // Never past half the shorter side (400 px wide: at most 200).
        p.highlights[0].radius = 0.25;
        let scene = build_scene(&p, &track, 1000, 1000, 2.0);
        assert!((scene.spot_corners[0].r - 200.0).abs() < 1e-3);
    }

    #[test]
    fn a_pen_stroke_resolves_to_pixels_while_visible() {
        let mut p = project_with_text();
        p.strokes.push(StrokeAnnotation {
            id: 2,
            points: vec![DVec2::new(0.1, 0.2), DVec2::new(0.3, 0.4)],
            color: Color::WHITE,
            thickness: 0.01,
            range: TimeRange::new(1.0, 3.0),
        });
        let track = vuoom_zoom::simulate(&[], &[], 5.0, 60.0, &p.zoom_config);
        let scene = build_scene(&p, &track, 1000, 1000, 2.0);
        assert_eq!(scene.strokes.len(), 1);
        let s = &scene.strokes[0];
        assert_eq!(s.points.len(), 2);
        assert!((s.points[1][0] - 300.0).abs() < 1e-9);
        assert!((s.points[1][1] - 400.0).abs() < 1e-9);
        assert!((s.thickness_px - 10.0).abs() < 1e-4);
        assert!(build_scene(&p, &track, 1000, 1000, 4.0).strokes.is_empty());
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
        assert_eq!(c.shape, PointerShape::Arrow);
        assert!(c.color.is_none() && !c.halo);

        // It takes the shape the real pointer had at that moment.
        p.pointer_shapes = vec![vuoom_project::PointerShapeAt {
            t: 0.5,
            shape: PointerShape::Text,
        }];
        let shape_at = |t: f64| {
            let scene = build_scene(&p, &track, 1000, 1000, t);
            scene.cursor.unwrap().shape
        };
        assert_eq!(shape_at(0.25), PointerShape::Arrow);
        assert_eq!(shape_at(1.0), PointerShape::Text);
        p.pointer_shapes.clear();

        // Hidden when idle: gone once it has rested a while.
        p.cursor = Some(CursorStyle {
            hide_idle: true,
            ..CursorStyle::default()
        });
        assert!(build_scene(&p, &track, 1000, 1000, 0.5).cursor.is_some());
        assert!(build_scene(&p, &track, 1000, 1000, 4.0).cursor.is_none());
    }

    #[test]
    fn the_pointer_carries_its_look_and_hides_when_transparent() {
        let mut p = project_with_text();
        p.events = vec![InputEvent::Move {
            t: 0.0,
            pos: DVec2::new(0.25, 0.5),
        }];
        let track = vuoom_zoom::simulate(&[], &[], 5.0, 60.0, &p.zoom_config);
        let c = |p: &Project| build_scene(p, &track, 1000, 1000, 1.0).cursor;

        p.cursor = Some(CursorStyle::default());
        let plain = c(&p).unwrap();
        assert_eq!(plain.look, PointerLook::Classic);
        assert!(plain.shadow);
        assert!((plain.group_opacity() - 1.0).abs() < 1e-9);

        p.cursor = Some(CursorStyle {
            look: PointerLook::Image,
            image: Some(vuoom_project::PointerImage {
                path: "p.png".into(),
                hotspot: [0.5, 0.25],
            }),
            opacity: 0.5,
            shadow: false,
            ..CursorStyle::default()
        });
        let styled = c(&p).unwrap();
        assert_eq!(styled.look, PointerLook::Image);
        assert_eq!(styled.hotspot, [0.5, 0.25]);
        assert!((styled.group_opacity() - 0.5).abs() < 1e-6);
        assert!(!styled.shadow);

        // No picture to draw: Vuoom's own pointer instead.
        if let Some(s) = p.cursor.as_mut() {
            s.image = None;
        }
        assert_eq!(c(&p).unwrap().look, PointerLook::Classic);

        // Fully transparent: nothing to draw at all.
        if let Some(s) = p.cursor.as_mut() {
            s.opacity = 0.0;
        }
        assert!(c(&p).is_none());
    }

    #[test]
    fn clicks_play_the_chosen_effect_for_its_duration() {
        let mut p = project_with_text();
        p.events = vec![InputEvent::Click {
            t: 1.0,
            pos: DVec2::new(0.5, 0.5),
            button: vuoom_zoom::MouseButton::Left,
        }];
        p.show_clicks = true;
        let track = vuoom_zoom::simulate(&[], &[], 5.0, 60.0, &p.zoom_config);
        let marks = |p: &Project, t: f64| build_scene(p, &track, 1000, 1000, t).ripples;

        // The classic ripple: one white ring, gone after its 0.45 s.
        let ripple = marks(&p, 1.1);
        assert_eq!(ripple.len(), 1);
        assert!(!ripple[0].filled);
        assert_eq!([ripple[0].color.r, ripple[0].color.b], [1.0, 1.0]);
        assert!(marks(&p, 0.9).is_empty());
        assert!(marks(&p, 1.5).is_empty());

        // Two rings once the second has started; colored; and longer when slowed down.
        p.click_style = ClickStyle {
            effect: ClickEffect::Rings,
            color: Some([1.0, 0.0, 0.0]),
            duration: 1.0,
            ..ClickStyle::default()
        };
        assert_eq!(marks(&p, 1.1).len(), 1);
        let rings = marks(&p, 1.5);
        assert_eq!(rings.len(), 2);
        let red = |m: &ResolvedHighlight| [m.color.r, m.color.g, m.color.b] == [1.0, 0.0, 0.0];
        assert!(rings.iter().all(red));
        // The first ring is ahead, so wider.
        assert!(rings[0].w > rings[1].w);

        p.click_style.effect = ClickEffect::Pulse;
        let pulse = marks(&p, 1.2);
        assert_eq!(pulse.len(), 2);
        assert!(pulse[0].filled && !pulse[1].filled);

        p.click_style.effect = ClickEffect::Burst;
        let burst = marks(&p, 1.2);
        assert_eq!(burst.len(), BURST_DOTS as usize);
        // The dots sit around the click, all at the same distance.
        let center = |m: &ResolvedHighlight| DVec2::new(m.x + m.w / 2.0, m.y + m.h / 2.0);
        let mid = burst.iter().map(center).sum::<DVec2>() / f64::from(BURST_DOTS);
        let d0 = center(&burst[0]).distance(mid);
        assert!(d0 > 1.0);
        let even = |m: &ResolvedHighlight| (center(m).distance(mid) - d0).abs() < 1e-6;
        assert!(burst.iter().all(even));

        // A bigger effect reaches further.
        let small = marks(&p, 1.2);
        p.click_style.size = 2.0;
        let big = marks(&p, 1.2);
        let reach = |m: &ResolvedHighlight| center(m).distance(mid);
        assert!(reach(&big[0]) > reach(&small[0]));

        // Off: nothing.
        p.show_clicks = false;
        assert!(marks(&p, 1.2).is_empty());
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
