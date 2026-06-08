//! Per-frame compositor layout math — pure, GPU-free, and unit-tested.
//!
//! Turns the camera pose + framing settings into the rectangles the wgpu shaders need:
//! which region of the source frame to sample (the zoom/pan crop) and where to draw the
//! framed recording inside the padded output. See `docs/05-Compositing-and-Preview.md`.

use vuoom_project::FrameStyle;
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

/// Everything the compositor needs to draw one framed, zoomed frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CompositeLayout {
    /// Region of the source frame to sample (normalized), from the camera zoom/pan.
    pub src_rect: NormRect,
    /// Where the framed recording is drawn within the output (pixels), after padding.
    pub dst_rect: PxRect,
    /// Rounded-corner radius in output pixels.
    pub corner_radius_px: f64,
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

/// The padded content rectangle inside the output (pixels).
#[must_use]
pub fn content_rect(out_w: u32, out_h: u32, padding: f64) -> PxRect {
    let small = f64::from(out_w.min(out_h));
    let pad = (padding * small).max(0.0);
    PxRect {
        x: pad,
        y: pad,
        w: (f64::from(out_w) - 2.0 * pad).max(1.0),
        h: (f64::from(out_h) - 2.0 * pad).max(1.0),
    }
}

/// Compute the full per-frame layout from output size, framing, and camera pose.
#[must_use]
pub fn compute_layout(
    out_w: u32,
    out_h: u32,
    frame: &FrameStyle,
    cam: &CameraState,
) -> CompositeLayout {
    let small = f64::from(out_w.min(out_h));
    CompositeLayout {
        src_rect: camera_src_rect(cam),
        dst_rect: content_rect(out_w, out_h, frame.padding),
        corner_radius_px: (frame.corner_radius * small).max(0.0),
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

    #[test]
    fn padding_insets_the_content_rect() {
        let r = content_rect(1000, 1000, 0.06);
        assert_eq!(
            r,
            PxRect {
                x: 60.0,
                y: 60.0,
                w: 880.0,
                h: 880.0
            }
        );
    }

    #[test]
    fn corner_radius_scales_with_smaller_dimension() {
        let f = FrameStyle {
            corner_radius: 0.02,
            ..FrameStyle::default()
        };
        let l = compute_layout(1920, 1080, &f, &cam(0.5, 0.5, 1.0));
        assert!((l.corner_radius_px - 0.02 * 1080.0).abs() < 1e-9);
    }
}
