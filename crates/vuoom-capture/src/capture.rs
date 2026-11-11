//! Screen capture → BGRA frames with clock timestamps: the platform-neutral half.
//!
//! The frame type, crop and pacing helpers, the stop handle and [`spawn_capture`] live here;
//! each OS fills in `crate::os::run` (Windows: [`crate::wgc`] with DXGI Desktop Duplication,
//! macOS: ScreenCaptureKit in `crate::sck`). See `docs/03-Capture.md` and
//! `docs/19-macOS-Port-Plan.md`.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{channel, sync_channel, Receiver, SyncSender};
use std::sync::Arc;
use std::time::Duration;

/// One captured frame: tightly-packed BGRA8 pixels + dimensions + QPC timestamp.
pub struct CapturedFrame {
    pub width: u32,
    pub height: u32,
    pub bgra: Vec<u8>,
    pub qpc: i64,
    /// The rows `(top, bottom)`, bottom exclusive, that may differ from the frame this
    /// capture sent just before this one; every other row is identical to it. An empty span
    /// means nothing changed. `None` when the capture can't tell (any row may differ): the
    /// first frame, and every frame from a capture that isn't told what changed.
    ///
    /// Typing or a moving pointer changes a sliver of the screen; knowing which lets the
    /// frame store compress that sliver instead of the whole frame.
    pub dirty: Option<(u32, u32)>,
}

/// Admits frames at a steady rate from a source that offers them faster: the compositor
/// offers one per screen refresh (60, 144, 240 a second), and each one taken is a full
/// copy from the GPU.
///
/// It keeps a schedule rather than a minimum gap, so the rate comes out exact whatever the
/// refresh rate is (a "wait at least 1/60 s" rule would take every THIRD frame of a 144 Hz
/// screen, 48 fps), and frames that arrive a little early (jitter) still count.
#[derive(Debug, Clone, Copy)]
pub(crate) struct FramePacer {
    /// Ticks between frames (0 = take every frame).
    interval: i64,
    /// When the next frame is due.
    next_due: i64,
}

impl FramePacer {
    /// A pacer for `max_fps` frames a second (`0` = no cap) on a clock of `freq` ticks a
    /// second.
    pub(crate) fn new(max_fps: u32, freq: i64) -> Self {
        let interval = if max_fps == 0 {
            0
        } else {
            freq / i64::from(max_fps)
        };
        Self {
            interval,
            next_due: i64::MIN / 2,
        }
    }

    /// Whether to take the frame offered at `now` (ticks).
    pub(crate) fn admit(&mut self, now: i64) -> bool {
        if self.interval == 0 {
            return true;
        }
        // A quarter of an interval early still counts as on time.
        if now + self.interval / 4 < self.next_due {
            return false;
        }
        // Keep to the schedule, unless it has fallen behind (the screen was still for a
        // while): then start a new one from now.
        let behind = self.next_due + self.interval < now;
        let base = if behind { now } else { self.next_due };
        self.next_due = base + self.interval;
        true
    }
}

/// A sub-rectangle (physical px) of the captured monitor to keep, the rest is discarded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CropRegion {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

/// Clamp a requested crop inside the frame, guaranteeing a non-empty rect.
pub(crate) fn clamp_region(r: CropRegion, w: u32, h: u32) -> (u32, u32, u32, u32) {
    let cx = r.x.min(w.saturating_sub(1));
    let cy = r.y.min(h.saturating_sub(1));
    let cw = r.w.min(w - cx).max(1);
    let ch = r.h.min(h - cy).max(1);
    (cx, cy, cw, ch)
}

/// Crop a tightly-packed BGRA buffer to `region`, into `out` (replacing what it held),
/// returning the crop's `(w, h)`.
#[cfg_attr(not(windows), allow(dead_code))] // macOS crops in the stream (`sourceRect`)
pub(crate) fn crop_bgra(
    full: &[u8],
    w: u32,
    h: u32,
    region: CropRegion,
    out: &mut Vec<u8>,
) -> (u32, u32) {
    let (cx, cy, cw, ch) = clamp_region(region, w, h);
    let row_bytes = (cw * 4) as usize;
    out.clear();
    out.reserve(row_bytes * ch as usize);
    for row in cy..cy + ch {
        let s = ((row * w + cx) * 4) as usize;
        out.extend_from_slice(&full[s..s + row_bytes]);
    }
    (cw, ch)
}

/// Per-session capture options.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CaptureOptions {
    /// Frame-rate cap delivered by the compositor (`0` = every composition).
    pub max_fps: u32,
    /// Whether the mouse cursor is drawn into the captured frames.
    pub cursor: bool,
}

impl Default for CaptureOptions {
    fn default() -> Self {
        Self {
            max_fps: 0,
            cursor: true,
        }
    }
}

/// Capture errors.
#[derive(Debug, thiserror::Error)]
pub enum CaptureError {
    #[error("failed to start capture: {0}")]
    Start(String),
}

/// Which capture API delivers a session's frames.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Backend {
    /// DXGI Desktop Duplication (Windows displays).
    DesktopDuplication,
    /// Windows Graphics Capture (windows, and displays Duplication can't record).
    GraphicsCapture,
    /// ScreenCaptureKit (macOS). Its content filter leaves Vuoom's own windows out and
    /// shows what is behind them, wherever they sit.
    ScreenCaptureKit,
}

impl Backend {
    /// Whether Vuoom's capture-excluded windows (the recording panel) stay out of the frames
    /// even where they overlap the captured area. Duplication honors the exclusion everywhere;
    /// WGC does on Windows 11 but records such a window as a black box on Windows 10, so it
    /// isn't counted on.
    #[must_use]
    pub fn hides_excluded_windows(self) -> bool {
        matches!(self, Self::DesktopDuplication | Self::ScreenCaptureKit)
    }

    /// What a session reports when the platform never said which API it opened.
    #[cfg(target_os = "macos")]
    pub(crate) const DEFAULT: Self = Self::ScreenCaptureKit;
    #[cfg(not(target_os = "macos"))]
    pub(crate) const DEFAULT: Self = Self::GraphicsCapture;
}

/// A handle to stop a running capture session.
#[derive(Clone)]
pub struct CaptureHandle {
    stop: Arc<AtomicBool>,
    /// Shared with the capture handler: total frames dropped because the bounded channel was
    /// full (the drain couldn't keep up). Read by the caller at stop time to warn the user
    /// that the take is choppy, see [`CaptureHandle::dropped`].
    dropped: Arc<AtomicU64>,
    backend: Backend,
    /// Frame buffers the consumer is done with, on their way back to the capture.
    recycle: SyncSender<Vec<u8>>,
}

impl CaptureHandle {
    /// The API recording this session.
    #[must_use]
    pub fn backend(&self) -> Backend {
        self.backend
    }

    /// Signal the capture thread to stop on its next frame.
    pub fn stop(&self) {
        self.stop.store(true, Ordering::Relaxed);
    }

    /// Frames dropped so far because the bounded channel was full (drain couldn't keep up).
    /// Read after `stop` (once capture has wound down) for the final count.
    #[must_use]
    pub fn dropped(&self) -> u64 {
        self.dropped.load(Ordering::Relaxed)
    }

    /// Hand back the pixel buffer of a frame that is done with, for the capture to fill
    /// again. A frame is megabytes, and a fresh allocation that size is zeroed page by page
    /// by the OS on first touch; reusing one skips that for every frame. Buffers the capture
    /// has no use for are simply freed.
    pub fn recycle(&self, buffer: Vec<u8>) {
        let _ = self.recycle.try_send(buffer);
    }
}

/// Where a capture's frames go, and what comes back.
pub(crate) struct Output {
    pub tx: SyncSender<CapturedFrame>,
    /// Frames dropped because `tx` was full.
    pub dropped: Arc<AtomicU64>,
    /// Buffers of frames the consumer is done with (see [`CaptureHandle::recycle`]).
    pub recycle: Receiver<Vec<u8>>,
}

/// Resolve a monitor by Win32 device name (e.g. `\\.\DISPLAY2`), falling back to primary.
/// What to capture: a display (by Win32 device name) or a top-level window (by HWND).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CaptureSource {
    /// A display by its key: the Win32 device name on Windows, the `CGDirectDisplayID`
    /// (as text) on macOS. `None` is the main display.
    Monitor { name: Option<String> },
    /// A window by its id: the HWND on Windows, the `CGWindowID` on macOS.
    Window { hwnd: isize },
}

/// How long `spawn_capture` waits to hear which backend runs (opening a display is quick;
/// this only bounds a stuck driver).
const BACKEND_WAIT: Duration = Duration::from_secs(3);

/// Recycled frame buffers waiting for the capture to pick them up. One comes back per
/// frame sent, so a couple is plenty.
const RECYCLE_CAP: usize = 4;

/// Frames buffered between the capture thread and the disk-drain consumer. Small on
/// purpose: it only needs to absorb brief write jitter, and each slot is a full BGRA screen
/// (several MB), so a large bound would be a large RAM ceiling.
const CHANNEL_CAP: usize = 8;

/// Spawn display capture on a background thread; returns the frame receiver and a
/// [`CaptureHandle`] to stop it. When `crop` is set, frames are cropped to that
/// sub-rectangle (monitor-relative physical px) before being sent. `monitor` is a Win32
/// device name (e.g. `\\.\DISPLAY2`); `None` captures the primary display. `opts` caps how
/// often the compositor delivers frames and whether the cursor is drawn into them.
#[must_use]
pub fn spawn_capture(
    crop: Option<CropRegion>,
    source: &CaptureSource,
    opts: CaptureOptions,
) -> (Receiver<CapturedFrame>, CaptureHandle) {
    // Bounded so a stalled/dead drain applies backpressure (drop-newest, see the handler)
    // instead of growing RAM without limit, each buffered frame is a full BGRA screen.
    let (tx, rx) = sync_channel(CHANNEL_CAP);
    let stop = Arc::new(AtomicBool::new(false));
    // Shared drop counter: the handler increments it whenever the bounded channel is full and
    // the caller reads it back through the returned `CaptureHandle` to warn about a choppy take.
    let dropped = Arc::new(AtomicU64::new(0));
    let (stop_handle, dropped_handle) = (Arc::clone(&stop), Arc::clone(&dropped));
    let (ready_tx, ready_rx) = channel();
    let (recycle_tx, recycle) = sync_channel(RECYCLE_CAP);
    let source = source.clone();
    std::thread::spawn(move || {
        let out = Output {
            tx,
            dropped,
            recycle,
        };
        let result = crate::os::run(&source, out, stop, crop, opts, &ready_tx);
        if let Err(e) = result {
            tracing::error!("screen capture stopped: {e}");
        }
    });
    let backend = ready_rx
        .recv_timeout(BACKEND_WAIT)
        .unwrap_or(Backend::DEFAULT);
    let handle = CaptureHandle {
        stop: stop_handle,
        dropped: dropped_handle,
        backend,
        recycle: recycle_tx,
    };
    (rx, handle)
}

/// Spawn display capture on a background thread. Legacy wrapper over [`spawn_capture`].
#[must_use]
pub fn spawn_region(
    crop: Option<CropRegion>,
    monitor: Option<String>,
) -> (Receiver<CapturedFrame>, CaptureHandle) {
    spawn_capture(
        crop,
        &CaptureSource::Monitor { name: monitor },
        CaptureOptions::default(),
    )
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
        let mut out = vec![9; 3]; // What a reused buffer still holds doesn't matter.
        let (w, h) = crop_bgra(
            &full,
            4,
            2,
            CropRegion {
                x: 1,
                y: 0,
                w: 2,
                h: 2,
            },
            &mut out,
        );
        assert_eq!(out.len(), 16);
        assert_eq!((w, h), (2, 2));
        // row0: idx 1,2 ; row1: idx 5,6
        assert_eq!(out[0], 1);
        assert_eq!(out[4], 2);
        assert_eq!(out[8], 5);
        assert_eq!(out[12], 6);
    }

    /// Frames admitted in ten seconds of a source offering `offered` a second (each a
    /// little early or late), paced to `fps`.
    fn admitted(offered: f64, fps: u32) -> usize {
        const FREQ: i64 = 10_000_000;
        let mut pacer = FramePacer::new(fps, FREQ);
        let step = FREQ as f64 / offered;
        (0..(offered * 10.0) as usize)
            .filter(|&k| {
                // A deterministic wobble of up to a tenth of a refresh either way.
                let wobble = ((k * 7919) % 21) as f64 / 100.0 - 0.1;
                pacer.admit(1_000_000 + ((k as f64 + wobble) * step) as i64)
            })
            .count()
    }

    #[test]
    fn the_pacer_holds_the_frame_rate_on_any_screen() {
        for (offered, fps) in [
            (60.0, 60),
            (60.0, 30),
            (60.0, 24),
            (75.0, 60),
            (144.0, 60),
            (144.0, 30),
            (240.0, 60),
        ] {
            let got = admitted(offered, fps) as f64 / 10.0;
            let want = f64::from(fps);
            assert!(
                (got - want).abs() <= want * 0.02,
                "{offered} Hz paced to {fps}: got {got} fps"
            );
        }
        // A screen slower than the cap gives all it has; no cap takes everything.
        assert_eq!(admitted(30.0, 60), 300);
        assert_eq!(admitted(144.0, 0), 1440);
    }

    #[test]
    fn the_pacer_restarts_its_schedule_after_a_still_stretch() {
        let mut pacer = FramePacer::new(10, 1000); // a frame every 100 ticks
        assert!(pacer.admit(0));
        assert!(!pacer.admit(50));
        assert!(pacer.admit(100));
        // Nothing for a long while, then two frames close together: only one is taken.
        assert!(pacer.admit(5000));
        assert!(!pacer.admit(5040));
        assert!(pacer.admit(5100));
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
