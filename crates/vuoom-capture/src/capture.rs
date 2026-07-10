//! WGC screen capture via `windows-capture` → BGRA frames with QPC timestamps.
//!
//! Implements the crate's `GraphicsCaptureApiHandler`; each arrived frame's tightly-packed
//! BGRA buffer is copied (with a QPC stamp) and sent over a channel. A [`CaptureHandle`]
//! stops the session. See `docs/03-Capture.md`. (Compile-verified on CI; runtime needs a
//! real GPU + display.)

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{sync_channel, Receiver, SyncSender, TrySendError};
use std::sync::Arc;
use vuoom_input::Clock;
use windows_capture::capture::{Context, GraphicsCaptureApiHandler};
use windows_capture::frame::Frame;
use windows_capture::graphics_capture_api::{GraphicsCaptureApi, InternalCaptureControl};
use windows_capture::monitor::Monitor;
use windows_capture::settings::{
    ColorFormat, CursorCaptureSettings, DirtyRegionSettings, DrawBorderSettings,
    MinimumUpdateIntervalSettings, SecondaryWindowSettings, Settings,
};

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
    tx: SyncSender<CapturedFrame>,
    clock: Clock,
    stop: Arc<AtomicBool>,
    crop: Option<CropRegion>,
    /// Frames dropped because the bounded channel was full (drain couldn't keep up).
    dropped: u64,
}

impl GraphicsCaptureApiHandler for Handler {
    type Flags = (SyncSender<CapturedFrame>, Arc<AtomicBool>, Option<CropRegion>);
    type Error = Box<dyn std::error::Error + Send + Sync>;

    fn new(ctx: Context<Self::Flags>) -> Result<Self, Self::Error> {
        let (tx, stop, crop) = ctx.flags;
        Ok(Self {
            tx,
            clock: Clock::new(),
            stop,
            crop,
            dropped: 0,
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
        let width = frame.width();
        let height = frame.height();
        let buffer = frame.buffer()?;
        let mut scratch: Vec<u8> = Vec::new();
        let full = buffer.as_nopadding_buffer(&mut scratch);
        let (w, h, bgra) = match self.crop {
            Some(r) => crop_bgra(full, width, height, r),
            None => (width, height, full.to_vec()),
        };
        // Bounded, drop-newest: never block this WGC callback thread and never let full
        // BGRA frames pile up in RAM. If the drain lags we drop the newest frame (and warn);
        // if the drain has died (e.g. a disk write failed) the channel disconnects, so stop
        // capturing rather than spin producing frames nothing consumes.
        let captured = CapturedFrame {
            width: w,
            height: h,
            bgra,
            qpc: self.clock.now(),
        };
        match self.tx.try_send(captured) {
            Ok(()) => {}
            Err(TrySendError::Full(_)) => {
                self.dropped += 1;
                if self.dropped == 1 || self.dropped % 60 == 0 {
                    tracing::warn!(
                        "frame drain can't keep up — dropped {} frame(s) so far",
                        self.dropped
                    );
                }
            }
            Err(TrySendError::Disconnected(_)) => control.stop(),
        }
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

/// Capture one display (by device name; primary if `None`), **blocking** until stopped;
/// BGRA frames go to `tx`.
///
/// # Errors
/// Returns [`CaptureError`] if the monitor or capture session cannot be started.
pub fn run_display(
    tx: SyncSender<CapturedFrame>,
    stop: Arc<AtomicBool>,
    crop: Option<CropRegion>,
    monitor: Option<&str>,
) -> Result<(), CaptureError> {
    let monitor = pick_monitor(monitor)?;
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
        monitor,
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

/// Frames buffered between the capture thread and the disk-drain consumer. Small on
/// purpose: it only needs to absorb brief write jitter, and each slot is a full BGRA screen
/// (several MB), so a large bound would be a large RAM ceiling.
const CHANNEL_CAP: usize = 8;

/// Spawn display capture on a background thread; returns the frame receiver and a
/// [`CaptureHandle`] to stop it. When `crop` is set, frames are cropped to that
/// sub-rectangle (monitor-relative physical px) before being sent. `monitor` is a Win32
/// device name (e.g. `\\.\DISPLAY2`); `None` captures the primary display.
#[must_use]
pub fn spawn_region(
    crop: Option<CropRegion>,
    monitor: Option<String>,
) -> (Receiver<CapturedFrame>, CaptureHandle) {
    // Bounded so a stalled/dead drain applies backpressure (drop-newest, see the handler)
    // instead of growing RAM without limit — each buffered frame is a full BGRA screen.
    let (tx, rx) = sync_channel(CHANNEL_CAP);
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
