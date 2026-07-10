//! Live recording preview — a "director's monitor" that shows the cinematic zoom in real
//! time while recording, without ever touching the actual recording pipeline.
//!
//! It runs its OWN lightweight screen capture and polls the cursor + the Ctrl+Shift+Z
//! hotkey, drives an online camera (the same critically-damped springs the final render
//! uses), crops/downscales each frame to the camera viewport, and publishes it to the
//! existing preview WebSocket. The recording path is untouched, so a hiccup here can never
//! corrupt a recording. The preview window is excluded from capture, so it never appears in
//! the recording and there is no hall-of-mirrors.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

use glam::DVec2;
use vuoom_capture::{spawn_region, CapturedFrame, CropRegion};
use vuoom_encode::{downscale_rgba, swizzle_rb, RgbaImage};
use vuoom_input::Clock;
use vuoom_preview::{pack_frame, FrameMeta, FrameSink};
use vuoom_zoom::{CameraFilter, CameraState, CameraTarget, ZoomConfig};

use windows::Win32::Foundation::POINT;
use windows::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;
use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;

/// Downscaled preview width (px) — small enough to be cheap, big enough to read.
const PREVIEW_WIDTH: u32 = 480;
/// Preview cadence (~20 fps) — independent of the capture rate so it never steals throughput.
const EMIT_INTERVAL: f64 = 0.05;

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
    /// Start streaming a live, zoom-tracked preview of `region` (full display if `None`)
    /// on `monitor` (primary if `None`) to `sink`. `origin` is the monitor's
    /// virtual-desktop origin (physical px) so the cursor maps correctly; `amount` is the
    /// chosen zoom multiplier.
    #[must_use]
    pub fn start(
        region: Option<CropRegion>,
        monitor: Option<String>,
        origin: (i32, i32),
        amount: f64,
        sink: FrameSink,
    ) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let stop_worker = Arc::clone(&stop);
        let handle =
            std::thread::spawn(move || run(region, monitor, origin, amount, sink, &stop_worker));
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

fn run(
    region: Option<CropRegion>,
    monitor: Option<String>,
    origin: (i32, i32),
    amount: f64,
    sink: FrameSink,
    stop: &AtomicBool,
) {
    let (rx, capture) = spawn_region(region, monitor);
    let cfg = ZoomConfig::default();
    let mut camera = LiveCamera::new(cfg, amount);
    let clock = Clock::new();
    let start = clock.now();
    let mut last_emit = -1.0_f64;
    let mut prev_chord = false;

    while !stop.load(Ordering::Relaxed) {
        let frame = match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(f) => f,
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        };
        let t = clock.seconds_between(start, clock.now());

        // Rising edge of Ctrl+Shift+Z toggles the zoom (mirrors the real recorder's hotkey).
        let chord = chord_down();
        if chord && !prev_chord {
            camera.toggle_zoom();
        }
        prev_chord = chord;

        let cursor = cursor_norm(region, origin, frame.width, frame.height);
        let cam = camera.step(t, cursor);

        // Throttle the actual pixel work to the preview cadence.
        if t - last_emit < EMIT_INTERVAL {
            continue;
        }
        last_emit = t;

        if let Some(packed) = render_preview(&frame, cam) {
            sink.publish(packed);
        }
    }
    capture.stop();
}

/// Crop a frame to the camera viewport, downscale, and pack it for the preview socket.
fn render_preview(frame: &CapturedFrame, cam: CameraState) -> Option<Vec<u8>> {
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
    let small = downscale_rgba(&rgba, target);

    let meta = FrameMeta {
        stride: small.width * 4,
        height: small.height,
        width: small.width,
        frame_number: 0,
        target_time_ns: 0,
    };
    Some(pack_frame(&small.pixels, meta))
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
/// from a precomputed keyframe track. Ctrl+Shift+Z is an explicit toggle — zoom in (and
/// follow the cursor) on one press, zoom back out on the next — matching the final render.
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
