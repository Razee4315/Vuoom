//! WGC screen capture via `windows-capture` → BGRA frames with QPC timestamps.
//!
//! Implements the crate's `GraphicsCaptureApiHandler`; each arrived frame's tightly-packed
//! BGRA buffer is copied (with a QPC stamp) and sent over a channel. A [`CaptureHandle`]
//! stops the session. See `docs/03-Capture.md`. (Compile-verified on CI; runtime needs a
//! real GPU + display.)

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use vuoom_input::Clock;
use windows_capture::capture::{Context, GraphicsCaptureApiHandler};
use windows_capture::frame::Frame;
use windows_capture::graphics_capture_api::{GraphicsCaptureApi, InternalCaptureControl};
use windows_capture::monitor::Monitor;
use windows_capture::settings::{
    ColorFormat, CursorCaptureSettings, DirtyRegionSettings, DrawBorderSettings,
    GraphicsCaptureItemType, MinimumUpdateIntervalSettings, SecondaryWindowSettings, Settings,
};
use windows_capture::window::Window;

/// One captured frame: tightly-packed BGRA8 pixels + dimensions + QPC timestamp.
pub struct CapturedFrame {
    pub width: u32,
    pub height: u32,
    pub bgra: Vec<u8>,
    pub qpc: i64,
}

/// A sub-rectangle (physical px) of the captured monitor to keep — the rest is discarded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CropRegion {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

/// What to capture: a display monitor or a specific application window.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CaptureTarget {
    /// A display monitor by Win32 device name (e.g. `\\.\DISPLAY2`); `None` = primary.
    Monitor { device_name: Option<String> },
    /// A top-level window whose title *contains* `title` (case-insensitive, best/topmost match).
    Window { title: String },
}

/// Clamp a requested crop inside the frame, guaranteeing a non-empty rect.
fn clamp_region(r: CropRegion, w: u32, h: u32) -> (u32, u32, u32, u32) {
    let cx = r.x.min(w.saturating_sub(1));
    let cy = r.y.min(h.saturating_sub(1));
    let cw = r.w.min(w - cx).max(1);
    let ch = r.h.min(h - cy).max(1);
    (cx, cy, cw, ch)
}

/// Crop a tightly-packed BGRA buffer to `region`, returning `(w, h, pixels)`.
fn crop_bgra(full: &[u8], w: u32, h: u32, region: CropRegion) -> (u32, u32, Vec<u8>) {
    let (cx, cy, cw, ch) = clamp_region(region, w, h);
    let row_bytes = (cw * 4) as usize;
    let mut out = Vec::with_capacity(row_bytes * ch as usize);
    for row in cy..cy + ch {
        let s = ((row * w + cx) * 4) as usize;
        out.extend_from_slice(&full[s..s + row_bytes]);
    }
    (cw, ch, out)
}

/// Capture errors.
#[derive(Debug, thiserror::Error)]
pub enum CaptureError {
    #[error("failed to start capture: {0}")]
    Start(String),
}

/// A handle to stop a running capture session.
#[derive(Clone)]
pub struct CaptureHandle {
    stop: Arc<AtomicBool>,
}

impl CaptureHandle {
    /// Signal the capture thread to stop on its next frame.
    pub fn stop(&self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

struct Handler {
    tx: Sender<CapturedFrame>,
    clock: Clock,
    stop: Arc<AtomicBool>,
    crop: Option<CropRegion>,
}

impl GraphicsCaptureApiHandler for Handler {
    type Flags = (Sender<CapturedFrame>, Arc<AtomicBool>, Option<CropRegion>);
    type Error = Box<dyn std::error::Error + Send + Sync>;

    fn new(ctx: Context<Self::Flags>) -> Result<Self, Self::Error> {
        let (tx, stop, crop) = ctx.flags;
        Ok(Self {
            tx,
            clock: Clock::new(),
            stop,
            crop,
        })
    }

    fn on_frame_arrived(
        &mut self,
        frame: &mut Frame,
        control: InternalCaptureControl,
    ) -> Result<(), Self::Error> {
        if self.stop.load(Ordering::Relaxed) {
            control.stop();
            return Ok(());
        }
        // Dimensions are read fresh every frame, so a window that RESIZES mid-capture is
        // handled without crashing: WGC hands us a new frame size and `clamp_region`
        // re-clamps the crop to whatever the current frame is (the crop is clamped/"letterboxed"
        // into the live frame rather than being a fixed absolute rect). Frames therefore keep
        // flowing at the new size; downstream consumers see a dimension change.
        let width = frame.width();
        let height = frame.height();
        let buffer = frame.buffer()?;
        let mut scratch: Vec<u8> = Vec::new();
        let full = buffer.as_nopadding_buffer(&mut scratch);
        let (w, h, bgra) = match self.crop {
            Some(r) => crop_bgra(full, width, height, r),
            None => (width, height, full.to_vec()),
        };
        let _ = self.tx.send(CapturedFrame {
            width: w,
            height: h,
            bgra,
            qpc: self.clock.now(),
        });
        Ok(())
    }

    fn on_closed(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
}

/// Resolve a monitor by Win32 device name (e.g. `\\.\DISPLAY2`), falling back to primary.
fn pick_monitor(name: Option<&str>) -> Result<Monitor, CaptureError> {
    if let Some(name) = name {
        if let Ok(monitors) = Monitor::enumerate() {
            for m in monitors {
                if m.device_name().is_ok_and(|n| n == name) {
                    return Ok(m);
                }
            }
        }
    }
    Monitor::primary().map_err(|e| CaptureError::Start(e.to_string()))
}

/// Resolve a window by case-insensitive title substring, picking the best/topmost match.
fn pick_window(title: &str) -> Result<Window, CaptureError> {
    let windows =
        Window::enumerate().map_err(|e| CaptureError::Start(format!("enumerate windows: {e}")))?;
    let titles: Vec<String> = windows
        .iter()
        .map(|w| w.title().unwrap_or_default())
        .collect();
    let idx = crate::windows::best_match_index(&titles, title)
        .ok_or_else(|| CaptureError::Start(format!("no window matching '{title}'")))?;
    Ok(windows[idx])
}

/// Build capture settings for `item` and run the session, **blocking** until stopped.
///
/// Generic over the capture item so the monitor and window paths share one code path (both
/// `Monitor` and `Window` implement `TryInto<GraphicsCaptureItemType>`).
fn run_capture<T: TryInto<GraphicsCaptureItemType>>(
    item: T,
    tx: Sender<CapturedFrame>,
    stop: Arc<AtomicBool>,
    crop: Option<CropRegion>,
) -> Result<(), CaptureError> {
    // Capture without the OS "being captured" highlight where the platform allows it
    // (Windows 11+, via the `IsBorderRequired` API). On Windows 10 that API is absent and the
    // border can't be removed, so we fall back to Default — requesting a specific value there
    // makes the capture session fail to start.
    let border = if GraphicsCaptureApi::is_border_settings_supported().unwrap_or(false) {
        DrawBorderSettings::WithoutBorder
    } else {
        DrawBorderSettings::Default
    };
    let settings = Settings::new(
        item,
        CursorCaptureSettings::Default,
        border,
        SecondaryWindowSettings::Default,
        MinimumUpdateIntervalSettings::Default,
        DirtyRegionSettings::Default,
        ColorFormat::Bgra8,
        (tx, stop, crop),
    );
    Handler::start(settings).map_err(|e| CaptureError::Start(e.to_string()))?;
    Ok(())
}

/// Capture a [`CaptureTarget`] (monitor or window), **blocking** until stopped; BGRA frames
/// go to `tx`. `crop`, when set, is applied relative to the captured frame (monitor- or
/// window-relative physical px) and re-clamped every frame, so a window that resizes
/// mid-capture is handled safely (see `Handler::on_frame_arrived`).
///
/// # Errors
/// Returns [`CaptureError`] if the target cannot be resolved or the session cannot start.
pub fn run_target(
    tx: Sender<CapturedFrame>,
    stop: Arc<AtomicBool>,
    crop: Option<CropRegion>,
    target: &CaptureTarget,
) -> Result<(), CaptureError> {
    match target {
        CaptureTarget::Monitor { device_name } => {
            let monitor = pick_monitor(device_name.as_deref())?;
            run_capture(monitor, tx, stop, crop)
        }
        CaptureTarget::Window { title } => {
            let window = pick_window(title)?;
            run_capture(window, tx, stop, crop)
        }
    }
}

/// Capture one display (by device name; primary if `None`), **blocking** until stopped;
/// BGRA frames go to `tx`. Thin wrapper over [`run_target`] for the monitor case.
///
/// # Errors
/// Returns [`CaptureError`] if the monitor or capture session cannot be started.
pub fn run_display(
    tx: Sender<CapturedFrame>,
    stop: Arc<AtomicBool>,
    crop: Option<CropRegion>,
    monitor: Option<&str>,
) -> Result<(), CaptureError> {
    run_target(
        tx,
        stop,
        crop,
        &CaptureTarget::Monitor {
            device_name: monitor.map(str::to_string),
        },
    )
}

/// Spawn display capture on a background thread; returns the frame receiver and a
/// [`CaptureHandle`] to stop it. When `crop` is set, frames are cropped to that
/// sub-rectangle (monitor-relative physical px) before being sent. `monitor` is a Win32
/// device name (e.g. `\\.\DISPLAY2`); `None` captures the primary display.
#[must_use]
pub fn spawn_region(
    crop: Option<CropRegion>,
    monitor: Option<String>,
) -> (Receiver<CapturedFrame>, CaptureHandle) {
    let (tx, rx) = channel();
    let stop = Arc::new(AtomicBool::new(false));
    let handle = CaptureHandle {
        stop: Arc::clone(&stop),
    };
    std::thread::spawn(move || {
        if let Err(e) = run_display(tx, stop, crop, monitor.as_deref()) {
            tracing::error!("screen capture stopped: {e}");
        }
    });
    (rx, handle)
}

/// Spawn capture of an arbitrary [`CaptureTarget`] (monitor or window) on a background thread;
/// returns the frame receiver and a [`CaptureHandle`] to stop it. When `crop` is set, frames
/// are cropped to that sub-rectangle (target-relative physical px) before being sent.
#[must_use]
pub fn spawn_target(
    target: CaptureTarget,
    crop: Option<CropRegion>,
) -> (Receiver<CapturedFrame>, CaptureHandle) {
    let (tx, rx) = channel();
    let stop = Arc::new(AtomicBool::new(false));
    let handle = CaptureHandle {
        stop: Arc::clone(&stop),
    };
    std::thread::spawn(move || {
        if let Err(e) = run_target(tx, stop, crop, &target) {
            tracing::error!("screen capture stopped: {e}");
        }
    });
    (rx, handle)
}

/// Spawn full primary-display capture (no crop). Convenience wrapper over [`spawn_region`].
#[must_use]
pub fn spawn_primary_display() -> (Receiver<CapturedFrame>, CaptureHandle) {
    spawn_region(None, None)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 4×2 BGRA with each pixel's B channel = its linear index, for easy assertions.
    fn ramp(w: u32, h: u32) -> Vec<u8> {
        let mut v = Vec::new();
        for i in 0..(w * h) {
            v.extend_from_slice(&[i as u8, 0, 0, 255]);
        }
        v
    }

    #[test]
    fn crop_extracts_subrect() {
        let full = ramp(4, 2); // indices 0..7
        let (w, h, out) = crop_bgra(
            &full,
            4,
            2,
            CropRegion {
                x: 1,
                y: 0,
                w: 2,
                h: 2,
            },
        );
        assert_eq!((w, h), (2, 2));
        // row0: idx 1,2 ; row1: idx 5,6
        assert_eq!(out[0], 1);
        assert_eq!(out[4], 2);
        assert_eq!(out[8], 5);
        assert_eq!(out[12], 6);
    }

    #[test]
    fn clamp_keeps_region_inside_frame() {
        assert_eq!(
            clamp_region(
                CropRegion {
                    x: 3,
                    y: 1,
                    w: 99,
                    h: 99
                },
                4,
                2
            ),
            (3, 1, 1, 1)
        );
        assert_eq!(
            clamp_region(
                CropRegion {
                    x: 0,
                    y: 0,
                    w: 4,
                    h: 2
                },
                4,
                2
            ),
            (0, 0, 4, 2)
        );
    }
}
