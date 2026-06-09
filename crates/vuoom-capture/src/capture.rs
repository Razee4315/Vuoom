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

/// Whether the OS supports toggling the capture border (Windows 11+). On Windows 10 the
/// border is always drawn and cannot be turned off, so the editor hides the toggle there.
#[must_use]
pub fn border_toggle_supported() -> bool {
    GraphicsCaptureApi::is_border_settings_supported().unwrap_or(false)
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

/// Capture the primary display, **blocking** until stopped; BGRA frames go to `tx`.
///
/// # Errors
/// Returns [`CaptureError`] if the monitor or capture session cannot be started.
pub fn run_primary_display(
    tx: Sender<CapturedFrame>,
    stop: Arc<AtomicBool>,
    crop: Option<CropRegion>,
    show_border: bool,
) -> Result<(), CaptureError> {
    let monitor = Monitor::primary().map_err(|e| CaptureError::Start(e.to_string()))?;
    // The OS-drawn "this is being captured" highlight around the recorded area. Toggling it
    // needs the `IsBorderRequired` API, which is Windows 11+ — on Windows 10 it is absent and
    // requesting any non-Default value makes the capture session FAIL to start (no frames).
    // So only request a specific border when the platform actually supports it; otherwise
    // fall back to Default (Win10 always draws the border — it can't be disabled there).
    let border_supported = GraphicsCaptureApi::is_border_settings_supported().unwrap_or(false);
    let border = if !border_supported {
        DrawBorderSettings::Default
    } else if show_border {
        DrawBorderSettings::WithBorder
    } else {
        DrawBorderSettings::WithoutBorder
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

/// Spawn primary-display capture on a background thread; returns the frame receiver and a
/// [`CaptureHandle`] to stop it. When `crop` is set, frames are cropped to that
/// sub-rectangle (physical px) before being sent. `show_border` toggles the OS capture
/// highlight around the recorded area.
#[must_use]
pub fn spawn_region(
    crop: Option<CropRegion>,
    show_border: bool,
) -> (Receiver<CapturedFrame>, CaptureHandle) {
    let (tx, rx) = channel();
    let stop = Arc::new(AtomicBool::new(false));
    let handle = CaptureHandle {
        stop: Arc::clone(&stop),
    };
    std::thread::spawn(move || {
        if let Err(e) = run_primary_display(tx, stop, crop, show_border) {
            tracing::error!("screen capture stopped: {e}");
        }
    });
    (rx, handle)
}

/// Spawn full primary-display capture (no crop). Convenience wrapper over [`spawn_region`].
#[must_use]
pub fn spawn_primary_display() -> (Receiver<CapturedFrame>, CaptureHandle) {
    spawn_region(None, true)
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
