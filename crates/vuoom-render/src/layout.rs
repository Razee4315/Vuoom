//! Per-frame compositor layout math, pure, GPU-free, and unit-tested.
//!
//! Turns the camera pose + framing settings into the rectangles the wgpu shaders need:
//! which region of the source frame to sample (the zoom/pan crop) and where to draw the
//! framed recording inside the padded output. See `docs/05-Compositing-and-Preview.md`.

use vuoom_project::{CropRect, FrameStyle};
use vuoom_zoom::CameraState;

/// A normalized rectangle in `0.0..=1.0` source space.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NormRect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

/// A rectangle in output pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PxRect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

/// The soft shadow the framed recording casts on its backdrop, in output pixels.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct PxShadow {
    /// How far the shadow sits from the recording (across, then down).
    pub dx: f64,
    pub dy: f64,
    /// How far the shadow's edge fades, either side of the recording's outline.
    pub blur: f64,
    /// Darkness at its deepest, 0 (none) to 1 (black).
    pub strength: f64,
}

/// Everything the compositor needs to draw one framed, zoomed frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CompositeLayout {
    /// Region of the source frame to sample (normalized), from the camera zoom/pan.
    pub src_rect: NormRect,
    /// Where the framed recording is drawn within the output (pixels), after padding.
    pub dst_rect: PxRect,
    /// Rounded-corner radius in output pixels.
    pub corner_radius_px: f64,
    /// The recording's shadow on the backdrop (zero strength = none).
    pub shadow: PxShadow,
}

/// The visible source region for a camera pose: a centered crop of side `1/zoom`.
#[must_use]
pub fn camera_src_rect(cam: &CameraState) -> NormRect {
    let side = 1.0 / cam.zoom.max(1.0);
    let half = side / 2.0;
    NormRect {
        x: cam.center.x - half,
        y: cam.center.y - half,
        w: side,
        h: side,
    }
}

/// A recording within this many pixels of fitting the padded area at its own size is drawn
/// at exactly its own size, on whole pixels.
const SNAP_PX: f64 = 2.0;

/// Where a `content` (width, height in pixels) recording is drawn inside the output: as
/// large as fits within the padding, centered, and never stretched.
///
/// When that comes to (all but) the recording's own size, which is how
/// `Project::output_dims` sizes the output, it is drawn at exactly that size on whole
/// pixels: every source pixel lands on one output pixel, so text stays as sharp as it was
/// recorded.
#[must_use]
pub fn content_rect(out_w: u32, out_h: u32, padding: f64, content: (u32, u32)) -> PxRect {
    let (ow, oh) = (f64::from(out_w), f64::from(out_h));
    let cw = f64::from(content.0.max(1));
    let ch = f64::from(content.1.max(1));
    let pad = (padding * ow.min(oh)).max(0.0);
    let avail_w = (ow - 2.0 * pad).max(1.0);
    let avail_h = (oh - 2.0 * pad).max(1.0);
    let k = (avail_w / cw).min(avail_h / ch);
    if (k - 1.0).abs() * cw.max(ch) < SNAP_PX {
        return PxRect {
            x: ((ow - cw) / 2.0).floor(),
            y: ((oh - ch) / 2.0).floor(),
            w: cw,
            h: ch,
        };
    }
    let (w, h) = (cw * k, ch * k);
    PxRect {
        x: (ow - w) / 2.0,
        y: (oh - h) / 2.0,
        w,
        h,
    }
}

/// Map a camera source rect through the crop: the camera pans/zooms INSIDE the cropped
/// region, so the sampled area is the camera rect expressed in crop space, then clamped
/// to the crop so the camera can never reveal pixels outside it.
#[must_use]
pub fn crop_src_rect(cam: &CameraState, crop: Option<CropRect>) -> NormRect {
    let cam_src = camera_src_rect(cam);
    match crop {
        None => cam_src,
        Some(c) => {
            let mapped = NormRect {
                x: c.x + cam_src.x * c.w,
                y: c.y + cam_src.y * c.h,
                w: cam_src.w * c.w,
                h: cam_src.h * c.h,
            };
            let x0 = mapped.x.max(c.x);
            let y0 = mapped.y.max(c.y);
            let x1 = (mapped.x + mapped.w).min(c.x + c.w);
            let y1 = (mapped.y + mapped.h).min(c.y + c.h);
            NormRect {
                x: x0,
                y: y0,
                w: (x1 - x0).max(1e-4),
                h: (y1 - y0).max(1e-4),
            }
        }
    }
}

/// Compute the full per-frame layout from output size, framing, camera pose, and crop.
/// `content` is the recording's size in pixels after the crop
/// (`Project::effective_source_dims`).
#[must_use]
pub fn compute_layout(
    out_w: u32,
    out_h: u32,
    frame: &FrameStyle,
    cam: &CameraState,
    crop: Option<CropRect>,
    content: (u32, u32),
) -> CompositeLayout {
    let small = f64::from(out_w.min(out_h));
    let oh = f64::from(out_h);
    // The shadow falls on the mat, so a frame without one has nowhere to show it.
    let shadow = if frame.padding > 0.0 && frame.shadow.strength > 0.0 {
        PxShadow {
            dx: frame.shadow.offset.x * oh,
            dy: frame.shadow.offset.y * oh,
            blur: (frame.shadow.blur * oh).max(1.0),
            strength: frame.shadow.strength.clamp(0.0, 1.0),
        }
    } else {
        PxShadow::default()
    };
    CompositeLayout {
        src_rect: crop_src_rect(cam, crop),
        dst_rect: content_rect(out_w, out_h, frame.padding, content),
        corner_radius_px: (frame.corner_radius * small).max(0.0),
        shadow,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::DVec2;

    fn cam(cx: f64, cy: f64, zoom: f64) -> CameraState {
        CameraState {
            center: DVec2::new(cx, cy),
            zoom,
        }
    }

    #[test]
    fn no_zoom_samples_full_source() {
        let r = camera_src_rect(&cam(0.5, 0.5, 1.0));
        assert_eq!(
            r,
            NormRect {
                x: 0.0,
                y: 0.0,
                w: 1.0,
                h: 1.0
            }
        );
    }

    #[test]
    fn double_zoom_samples_center_quarter() {
        let r = camera_src_rect(&cam(0.5, 0.5, 2.0));
        assert_eq!(
            r,
            NormRect {
                x: 0.25,
                y: 0.25,
                w: 0.5,
                h: 0.5
            }
        );
    }

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-6
    }

    #[test]
    fn padding_insets_the_content_rect_without_stretching_it() {
        // A square recording in a square output: the padding on every side.
        let r = content_rect(1000, 1000, 0.06, (2000, 2000));
        assert!(close(r.x, 60.0) && close(r.y, 60.0));
        assert!(close(r.w, 880.0) && close(r.h, 880.0));
        // A wide recording in an output of its own size keeps its 16:9 shape: the padding
        // above and below, and more to the sides.
        let r = content_rect(1920, 1080, 0.04, (1920, 1080));
        assert!(close(r.w / r.h, 16.0 / 9.0));
        assert!(close(r.h, 1080.0 - 2.0 * 43.2));
        assert!(close(r.x, (1920.0 - r.w) / 2.0));
        assert!(close(r.y, 43.2));
    }

    #[test]
    fn a_recording_that_fits_is_drawn_pixel_for_pixel() {
        // The output `Project::output_dims` gives a 1920x1080 take with a 4% frame.
        let r = content_rect(2086, 1174, 0.04, (1920, 1080));
        assert_eq!((r.x, r.y, r.w, r.h), (83.0, 47.0, 1920.0, 1080.0));
        // No frame: the whole output.
        let r = content_rect(1920, 1080, 0.0, (1920, 1080));
        assert_eq!((r.x, r.y, r.w, r.h), (0.0, 0.0, 1920.0, 1080.0));
        // An odd-sized recording in its even output keeps its pixels and loses one line.
        let r = content_rect(1920, 1080, 0.0, (1921, 1080));
        assert_eq!((r.x, r.w), (-1.0, 1921.0));
    }

    #[test]
    fn corner_radius_scales_with_smaller_dimension() {
        let f = FrameStyle {
            corner_radius: 0.02,
            ..FrameStyle::default()
        };
        let l = compute_layout(1920, 1080, &f, &cam(0.5, 0.5, 1.0), None, (1920, 1080));
        assert!((l.corner_radius_px - 0.02 * 1080.0).abs() < 1e-9);
        assert_eq!(l.shadow, PxShadow::default());
    }

    #[test]
    fn a_framed_recording_casts_its_shadow() {
        let mut f = FrameStyle {
            padding: 0.04,
            ..FrameStyle::default()
        };
        f.shadow.strength = 0.3;
        let l = compute_layout(2086, 1174, &f, &cam(0.5, 0.5, 1.0), None, (1920, 1080));
        assert!(close(l.shadow.strength, 0.3));
        assert!(close(l.shadow.blur, 0.03 * 1174.0));
        assert!(l.shadow.dy > 0.0 && l.shadow.dx.abs() < 1e-9);
        // Without a mat there is nowhere for it to fall.
        f.padding = 0.0;
        let l = compute_layout(1920, 1080, &f, &cam(0.5, 0.5, 1.0), None, (1920, 1080));
        assert_eq!(l.shadow, PxShadow::default());
    }
}
