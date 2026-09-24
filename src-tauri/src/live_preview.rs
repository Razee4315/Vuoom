//! Live recording preview, a "director's monitor" that shows the cinematic zoom in real
//! time while recording.
//!
//! It is fed a small copy of the recorded frames (see [`sample`]; the recording's drain hands
//! one over about 20 times a second and never waits on it), polls the cursor and the
//! Ctrl+Shift+Z hotkey, drives an online camera (the same critically damped springs the final
//! render uses), crops and downscales each frame to the camera viewport, and publishes it to
//! the preview WebSocket. Sharing the recording's frames rather than running a second capture
//! halves the capture work, and matters for Desktop Duplication, which allows one capture of a
//! display per process. When the take leaves the pointer out of the frames (the smooth
//! pointer), a small marker shows where it is. The panel showing the preview is excluded from
//! capture, so there is no hall of mirrors.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

use glam::DVec2;
use vuoom_capture::{CapturedFrame, CropRegion};
use vuoom_encode::{downscale_rgba, swizzle_rb, RgbaImage};
use vuoom_input::Clock;
use vuoom_preview::{pack_frame, FrameMeta, FrameSink};
use vuoom_zoom::{CameraFilter, CameraState, CameraTarget, ZoomConfig};

use windows::Win32::Foundation::POINT;
use windows::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;
use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;

/// Downscaled preview width (px), small enough to be cheap, big enough to read.
const PREVIEW_WIDTH: u32 = 480;
/// Preview cadence (~20 fps), independent of the capture rate so it never steals throughput.
const EMIT_INTERVAL: f64 = 0.05;
/// How often the recording's drain hands the preview a frame.
pub const TAP_EVERY: Duration = Duration::from_millis(45);
/// The widest copy handed over: enough for a 2× zoom into a 480 px preview.
const TAP_MAX_WIDTH: u32 = 1280;

/// A recorded frame, subsampled for the preview, with the full frame's size (the cursor maps
/// onto the full frame).
pub struct PreviewFrame {
    frame: CapturedFrame,
    full_w: u32,
    full_h: u32,
}

/// A copy of `frame` for the preview: every n-th pixel of every n-th row, so it is at most
/// [`TAP_MAX_WIDTH`] wide. Cheap enough to run on the recording's drain.
#[must_use]
pub fn sample(frame: &CapturedFrame) -> PreviewFrame {
    let (w, h) = (frame.width, frame.height);
    let step = w.div_ceil(TAP_MAX_WIDTH).max(1) as usize;
    let (sw, sh) = (w as usize / step, h as usize / step);
    let mut bgra = Vec::with_capacity(sw * sh * 4);
    for y in 0..sh {
        let row = y * step * w as usize * 4;
        for x in 0..sw {
            let i = row + x * step * 4;
            bgra.extend_from_slice(&frame.bgra[i..i + 4]);
        }
    }
    PreviewFrame {
        frame: CapturedFrame {
            width: sw as u32,
            height: sh as u32,
            bgra,
            qpc: frame.qpc,
        },
        full_w: w,
        full_h: h,
    }
}

// Virtual-key codes for the manual-zoom chord (Ctrl+Shift+Z).
const VK_SHIFT: i32 = 0x10;
const VK_CONTROL: i32 = 0x11;
const VK_Z: i32 = 0x5A;

/// A running live-preview worker. Dropping or calling [`LivePreview::stop`] ends it.
pub struct LivePreview {
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl LivePreview {
    /// Start streaming a live, zoom-tracked preview of the frames arriving on `frames` (the
    /// recording's, see [`sample`]) to `sink`. `region` is the recorded crop (full display if
    /// `None`) and `origin` the monitor's virtual-desktop origin (physical px), so the cursor
    /// maps correctly; `amount` is the chosen zoom multiplier; `mark_pointer` draws a pointer
    /// marker for takes whose frames leave the pointer out.
    #[must_use]
    pub fn start(
        frames: Receiver<PreviewFrame>,
        region: Option<CropRegion>,
        origin: (i32, i32),
        amount: f64,
        mark_pointer: bool,
        sink: FrameSink,
    ) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let stop_worker = Arc::clone(&stop);
        let feed = Feed {
            region,
            origin,
            amount,
            mark_pointer,
        };
        let handle = std::thread::spawn(move || run(&frames, &feed, &sink, &stop_worker));
        Self {
            stop,
            handle: Some(handle),
        }
    }

    /// Signal the worker to stop and wait for it to finish.
    pub fn stop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

impl Drop for LivePreview {
    fn drop(&mut self) {
        self.stop();
    }
}

/// What the preview shows and how.
struct Feed {
    region: Option<CropRegion>,
    origin: (i32, i32),
    amount: f64,
    mark_pointer: bool,
}

fn run(frames: &Receiver<PreviewFrame>, feed: &Feed, sink: &FrameSink, stop: &AtomicBool) {
    let cfg = ZoomConfig::default();
    let mut camera = LiveCamera::new(cfg, feed.amount);
    let clock = Clock::new();
    let start = clock.now();
    let mut last_emit = -1.0_f64;
    let mut prev_chord = false;

    while !stop.load(Ordering::Relaxed) {
        let sampled = match frames.recv_timeout(Duration::from_millis(100)) {
            Ok(f) => f,
            Err(RecvTimeoutError::Timeout) => continue,
            // The recording ended (or its drain stopped): nothing more to show.
            Err(RecvTimeoutError::Disconnected) => break,
        };
        let t = clock.seconds_between(start, clock.now());

        // Rising edge of Ctrl+Shift+Z toggles the zoom (mirrors the real recorder's hotkey).
        let chord = chord_down();
        if chord && !prev_chord {
            camera.toggle_zoom();
        }
        prev_chord = chord;

        let cursor = cursor_norm(feed.region, feed.origin, sampled.full_w, sampled.full_h);
        let cam = camera.step(t, cursor);

        // Throttle the actual pixel work to the preview cadence.
        if t - last_emit < EMIT_INTERVAL {
            continue;
        }
        last_emit = t;

        let marker = feed.mark_pointer.then_some(cursor);
        if let Some(packed) = render_preview(&sampled.frame, cam, marker) {
            sink.publish(packed);
        }
    }
}

/// Crop a frame to the camera viewport, downscale, and pack it for the preview socket.
/// `marker` (the pointer, normalized to the frame) is drawn when given.
fn render_preview(
    frame: &CapturedFrame,
    cam: CameraState,
    marker: Option<DVec2>,
) -> Option<Vec<u8>> {
    let (fw, fh) = (frame.width, frame.height);
    if fw == 0 || fh == 0 {
        return None;
    }
    // Viewport size in source px for this zoom, centered on the camera and kept on-screen.
    let zoom = cam.zoom.max(1.0);
    let vw = ((f64::from(fw) / zoom).round() as u32).clamp(1, fw);
    let vh = ((f64::from(fh) / zoom).round() as u32).clamp(1, fh);
    let cx = (cam.center.x * f64::from(fw)).round() as i64;
    let cy = (cam.center.y * f64::from(fh)).round() as i64;
    let x0 = (cx - i64::from(vw) / 2).clamp(0, i64::from(fw - vw)) as u32;
    let y0 = (cy - i64::from(vh) / 2).clamp(0, i64::from(fh - vh)) as u32;

    // Crop the tightly-packed BGRA viewport, swizzle to RGBA, downscale to the preview width.
    let row = (vw * 4) as usize;
    let mut cropped = Vec::with_capacity(row * vh as usize);
    for y in y0..y0 + vh {
        let s = ((y * fw + x0) * 4) as usize;
        cropped.extend_from_slice(&frame.bgra[s..s + row]);
    }
    let rgba = RgbaImage::new(vw, vh, swizzle_rb(&cropped));
    let target = PREVIEW_WIDTH.min(vw);
    let mut small = downscale_rgba(&rgba, target);
    if let Some(p) = marker {
        // The pointer in viewport terms, then preview pixels.
        let px = (p.x * f64::from(fw) - f64::from(x0)) / f64::from(vw) * f64::from(small.width);
        let py = (p.y * f64::from(fh) - f64::from(y0)) / f64::from(vh) * f64::from(small.height);
        mark(&mut small, px, py);
    }

    let meta = FrameMeta {
        stride: small.width * 4,
        height: small.height,
        width: small.width,
        frame_number: 0,
        target_time_ns: 0,
    };
    Some(pack_frame(&small.pixels, meta))
}

/// A small pointer marker: a white dot in a dark ring, centered on (`x`, `y`) (preview px).
/// Nothing is drawn outside the image.
fn mark(img: &mut RgbaImage, x: f64, y: f64) {
    const OUTER: f64 = 5.0;
    const INNER: f64 = 3.2;
    let (w, h) = (i64::from(img.width), i64::from(img.height));
    let (cx, cy) = (x.round() as i64, y.round() as i64);
    for yy in (cy - 6).max(0)..(cy + 7).min(h) {
        for xx in (cx - 6).max(0)..(cx + 7).min(w) {
            let d = ((xx as f64 - x).powi(2) + (yy as f64 - y).powi(2)).sqrt();
            let color = if d <= INNER {
                [255, 255, 255]
            } else if d <= OUTER {
                [20, 20, 24]
            } else {
                continue;
            };
            let i = ((yy * w + xx) * 4) as usize;
            img.pixels[i..i + 3].copy_from_slice(&color);
        }
    }
}

/// True while Ctrl AND Shift AND Z are all held.
fn chord_down() -> bool {
    key_down(VK_CONTROL) && key_down(VK_SHIFT) && key_down(VK_Z)
}

fn key_down(vk: i32) -> bool {
    // The high-order bit of GetAsyncKeyState is set while the key is down.
    (unsafe { GetAsyncKeyState(vk) } as u16 & 0x8000) != 0
}

/// The cursor position normalized into the captured `region` (full display if `None`).
/// The primary monitor's origin is (0,0) in virtual-desktop coords, and per-monitor DPI
/// awareness is enabled, so screen px map directly onto the captured frame.
fn cursor_norm(region: Option<CropRegion>, origin: (i32, i32), fw: u32, fh: u32) -> DVec2 {
    let mut p = POINT::default();
    if unsafe { GetCursorPos(&mut p) }.is_err() {
        return DVec2::splat(0.5);
    }
    // The cursor is in virtual-desktop coords; the crop is monitor-relative.
    let (mx, my) = (f64::from(origin.0), f64::from(origin.1));
    let (ox, oy, w, h) = match region {
        Some(r) => (
            mx + f64::from(r.x),
            my + f64::from(r.y),
            f64::from(r.w),
            f64::from(r.h),
        ),
        None => (mx, my, f64::from(fw), f64::from(fh)),
    };
    DVec2::new(
        ((f64::from(p.x) - ox) / w.max(1.0)).clamp(0.0, 1.0),
        ((f64::from(p.y) - oy) / h.max(1.0)).clamp(0.0, 1.0),
    )
}

/// The live-preview camera: the real recorder's [`CameraFilter`] driven online instead of
/// from a precomputed keyframe track. Ctrl+Shift+Z is an explicit toggle, zoom in (and
/// follow the cursor) on one press, zoom back out on the next, matching the final render.
/// The per-frame spring math lives in `vuoom_zoom` so the two paths stay in lock-step.
struct LiveCamera {
    cfg: ZoomConfig,
    amount: f64,
    cam: CameraFilter,
    active: bool,
    last_t: f64,
}

impl LiveCamera {
    fn new(cfg: ZoomConfig, amount: f64) -> Self {
        Self {
            cfg,
            amount,
            cam: CameraFilter::new(DVec2::splat(0.5)),
            active: false,
            last_t: 0.0,
        }
    }

    fn toggle_zoom(&mut self) {
        self.active = !self.active;
    }

    fn step(&mut self, t: f64, cursor: DVec2) -> CameraState {
        // Real wall-clock dt, clamped so a stalled frame can't launch the springs.
        let dt = (t - self.last_t).clamp(1e-4, 0.1);
        self.last_t = t;

        let target = if self.active {
            CameraTarget::Auto {
                amount: self.amount,
                edge_snap_ratio: self.cfg.edge_snap_ratio,
            }
        } else {
            CameraTarget::Idle
        };
        self.cam.step(cursor, target, &self.cfg, dt)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(w: u32, h: u32) -> CapturedFrame {
        let bgra = (0..w * h)
            .flat_map(|i| [(i % 256) as u8, (i / 256) as u8, 0, 255])
            .collect();
        CapturedFrame {
            width: w,
            height: h,
            bgra,
            qpc: 7,
        }
    }

    #[test]
    fn small_frames_pass_whole_and_big_ones_are_subsampled() {
        let small = sample(&frame(640, 360));
        assert_eq!((small.frame.width, small.frame.height), (640, 360));
        assert_eq!(small.frame.bgra.len(), 640 * 360 * 4);

        let big = sample(&frame(2560, 1440));
        assert_eq!((big.frame.width, big.frame.height), (1280, 720));
        assert_eq!((big.full_w, big.full_h), (2560, 1440));
        assert_eq!(big.frame.qpc, 7);
        // Pixel (1, 0) of the copy is pixel (2, 0) of the original.
        assert_eq!(big.frame.bgra[4], 2);
    }

    #[test]
    fn the_marker_is_drawn_in_place_and_clipped() {
        let mut img = RgbaImage::new(20, 20, vec![0; 20 * 20 * 4]);
        mark(&mut img, 10.0, 10.0);
        let at = |x: usize, y: usize| img.pixels[(y * 20 + x) * 4];
        assert_eq!(at(10, 10), 255, "white center");
        assert_eq!(at(14, 10), 20, "dark ring");
        assert_eq!(at(0, 0), 0, "untouched outside");
        // At the corner, only the part inside the image is drawn.
        mark(&mut img, 0.0, 0.0);
        assert_eq!(at(0, 0), 255);
    }
}
