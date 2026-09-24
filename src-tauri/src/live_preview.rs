//! Live recording preview, a "director's monitor" that shows the cinematic zoom in real
//! time while recording.
//!
//! It is fed a small copy of the recorded frames (see [`sample`]; the recording's drain hands
//! one over up to 60 times a second and never waits on it). On its own clock, about 120 times
//! a second whether or not the screen changed, it polls the cursor and the Ctrl+Shift+Z
//! hotkey and steps an online camera (the same critically damped springs the final render
//! uses), so a zoom starts the moment the keys go down. Whenever the picture changed (a new
//! frame, the camera moving, the pointer marker moving) it crops the newest frame to the
//! camera viewport and downscales it in one pass, up to 60 times a second, and publishes it
//! to the preview WebSocket. Sharing the recording's frames rather than running a second capture
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
use vuoom_encode::RgbaImage;
use vuoom_input::Clock;
use vuoom_preview::{pack_frame, FrameMeta, FrameSink};
use vuoom_zoom::{CameraFilter, CameraState, CameraTarget, ZoomConfig};

use windows::Win32::Foundation::POINT;
use windows::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;
use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;

/// Downscaled preview width (px): sharp in the large panel, still cheap to make.
const PREVIEW_WIDTH: u32 = 640;
/// Fastest the preview repaints (60 fps). It repaints only when the picture changed.
const EMIT_INTERVAL: f64 = 1.0 / 60.0;
/// How often the camera and the zoom hotkey are checked, whether or not a frame arrived.
const TICK: Duration = Duration::from_millis(8);
/// How often the recording's drain hands the preview a frame (60 fps).
pub const TAP_EVERY: Duration = Duration::from_millis(16);
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

/// What the last published preview showed, to skip repainting an unchanged picture.
#[derive(Clone, Copy, PartialEq)]
struct Shown {
    cam: CameraState,
    marker: Option<DVec2>,
}

impl Shown {
    /// Whether `other` looks different on screen.
    fn differs(self, other: Self) -> bool {
        let moved = (self.cam.center - other.cam.center).length() > 1e-5;
        let zoomed = (self.cam.zoom - other.cam.zoom).abs() > 1e-4;
        let marker = match (self.marker, other.marker) {
            (Some(a), Some(b)) => (a - b).length() > 1e-4,
            (a, b) => a.is_some() != b.is_some(),
        };
        moved || zoomed || marker
    }
}

fn run(frames: &Receiver<PreviewFrame>, feed: &Feed, sink: &FrameSink, stop: &AtomicBool) {
    let cfg = ZoomConfig::default();
    let mut camera = LiveCamera::new(cfg, feed.amount);
    let clock = Clock::new();
    let start = clock.now();
    let mut last_emit = -1.0_f64;
    let mut prev_chord = false;
    let mut latest: Option<PreviewFrame> = None;
    let mut fresh = false;
    let mut shown: Option<Shown> = None;

    while !stop.load(Ordering::Relaxed) {
        // The newest frame, waiting at most one tick for it.
        match frames.recv_timeout(TICK) {
            Ok(f) => {
                latest = Some(f);
                fresh = true;
            }
            Err(RecvTimeoutError::Timeout) => {}
            // The recording ended (or its drain stopped): nothing more to show.
            Err(RecvTimeoutError::Disconnected) => break,
        }
        while let Ok(f) = frames.try_recv() {
            latest = Some(f);
            fresh = true;
        }
        let t = clock.seconds_between(start, clock.now());

        // Rising edge of Ctrl+Shift+Z toggles the zoom (mirrors the real recorder's hotkey).
        let chord = chord_down();
        if chord && !prev_chord {
            camera.toggle_zoom();
        }
        prev_chord = chord;

        let Some(frame) = latest.as_ref() else {
            continue;
        };
        let cursor = cursor_norm(feed.region, feed.origin, frame.full_w, frame.full_h);
        let now = Shown {
            cam: camera.step(t, cursor),
            marker: feed.mark_pointer.then_some(cursor),
        };
        let changed = fresh || shown.is_none_or(|s| s.differs(now));
        if !changed || t - last_emit < EMIT_INTERVAL {
            continue;
        }
        last_emit = t;
        fresh = false;
        shown = Some(now);
        if let Some(packed) = render_preview(&frame.frame, now.cam, now.marker) {
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

    let viewport = (x0, y0, vw, vh);
    let mut small = shrink(frame, viewport, PREVIEW_WIDTH.min(vw));
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

/// The `viewport` (x, y, w, h) of `frame` (BGRA) as an RGBA image `width` px wide, in one
/// pass: each output pixel is the average of the source pixels it covers.
fn shrink(frame: &CapturedFrame, viewport: (u32, u32, u32, u32), width: u32) -> RgbaImage {
    let (x0, y0, vw, vh) = viewport;
    let (dw, stride) = (width.max(1) as usize, frame.width as usize);
    let dh = ((u64::from(vh) * dw as u64) / u64::from(vw.max(1))).max(1) as usize;
    let (vw, vh) = (vw as usize, vh as usize);
    // Each output column's source span, worked out once for all rows.
    let cols: Vec<(usize, usize)> = (0..dw)
        .map(|dx| {
            let a = dx * vw / dw;
            (a, ((dx + 1) * vw / dw).clamp(a + 1, vw))
        })
        .collect();
    let mut out = vec![0u8; dw * dh * 4];
    for (dy, out_row) in out.chunks_mut(dw * 4).enumerate() {
        let a = dy * vh / dh;
        let b = ((dy + 1) * vh / dh).clamp(a + 1, vh);
        let (out_px, _) = out_row.as_chunks_mut::<4>();
        for (&(c0, c1), px) in cols.iter().zip(out_px) {
            let mut sum = [0u32; 3];
            for sy in a..b {
                let row = (y0 as usize + sy) * stride + x0 as usize;
                let span = &frame.bgra[(row + c0) * 4..(row + c1) * 4];
                let (src, _) = span.as_chunks::<4>();
                for p in src {
                    sum[0] += u32::from(p[0]);
                    sum[1] += u32::from(p[1]);
                    sum[2] += u32::from(p[2]);
                }
            }
            let n = ((b - a) * (c1 - c0)) as u32;
            let avg = |c: u32| (c / n) as u8;
            // BGRA in, RGBA out.
            *px = [avg(sum[2]), avg(sum[1]), avg(sum[0]), 255];
        }
    }
    RgbaImage::new(dw as u32, dh as u32, out)
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
    fn shrink_averages_and_swaps_to_rgba() {
        // 4×2 BGRA: the left half blue, the right half red.
        let mut f = frame(4, 2);
        let (pixels, _) = f.bgra.as_chunks_mut::<4>();
        for (i, px) in pixels.iter_mut().enumerate() {
            *px = if i % 4 < 2 {
                [200, 0, 0, 255]
            } else {
                [0, 0, 100, 255]
            };
        }
        let img = shrink(&f, (0, 0, 4, 2), 2);
        assert_eq!((img.width, img.height), (2, 1));
        assert_eq!(img.pixels[..4], [0, 0, 200, 255]);
        assert_eq!(img.pixels[4..], [100, 0, 0, 255]);
        // A viewport: only the right half, one output pixel.
        let img = shrink(&f, (2, 0, 2, 2), 1);
        assert_eq!(img.pixels, [100, 0, 0, 255]);
    }

    #[test]
    fn the_marker_is_drawn_in_place_and_clipped() {
        let mut img = RgbaImage::new(20, 20, vec![0; 20 * 20 * 4]);
        let at = |img: &RgbaImage, x: usize, y: usize| img.pixels[(y * 20 + x) * 4];
        mark(&mut img, 10.0, 10.0);
        assert_eq!(at(&img, 10, 10), 255, "white center");
        assert_eq!(at(&img, 14, 10), 20, "dark ring");
        assert_eq!(at(&img, 0, 0), 0, "untouched outside");
        // At the corner, only the part inside the image is drawn.
        mark(&mut img, 0.0, 0.0);
        assert_eq!(at(&img, 0, 0), 255);
    }
}
