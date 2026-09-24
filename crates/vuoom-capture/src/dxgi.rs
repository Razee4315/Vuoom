//! Display capture through DXGI Desktop Duplication.
//!
//! Preferred over Windows Graphics Capture for displays because it honors
//! `WDA_EXCLUDEFROMCAPTURE` on Windows 10 as well as 11: Vuoom's recording panel can sit over
//! the recorded area and still stay out of the frames. (On Windows 10, WGC records such a
//! window as a black box, while Duplication shows what's behind it; both verified on
//! hardware.) Duplication doesn't draw the pointer, so when a take keeps the real pointer it
//! is drawn here from the shape Windows reports ([`crate::pointer`]).
//!
//! Frames arrive only when the screen changes, as with WGC. A frame that arrives before the
//! next one is due (the frame-rate cap) is held until it is: its content is kept, and the next
//! acquire gathers everything newer, so the latest change is never lost.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{SyncSender, TrySendError};
use std::time::Duration;

use vuoom_input::Clock;
use windows::Win32::Graphics::Dxgi::Common::{
    DXGI_MODE_ROTATION_IDENTITY, DXGI_MODE_ROTATION_UNSPECIFIED,
};
use windows::Win32::Graphics::Dxgi::{
    IDXGIOutputDuplication, DXGI_OUTDUPL_FRAME_INFO, DXGI_OUTDUPL_POINTER_SHAPE_INFO,
};
use windows_capture::dxgi_duplication_api::{DxgiDuplicationApi, Error as DupError};
use windows_capture::monitor::Monitor;

use crate::capture::{clamp_region, CaptureOptions, CapturedFrame, CropRegion};
use crate::pointer::{self, PointerShape, ShapeKind};

/// How long one acquire waits for a change before checking for a stop request (ms).
const ACQUIRE_MS: u32 = 100;
/// Pause before reopening a duplication that was lost (the secure desktop is up, a display
/// mode is changing).
const REOPEN_WAIT: Duration = Duration::from_millis(250);

/// Open a duplication of `monitor`, if Desktop Duplication can record it: the display must
/// hang off the device's adapter (not always so on hybrid-GPU laptops) and be unrotated
/// (a rotated display's desktop image arrives unrotated).
fn open(monitor: Monitor) -> Result<DxgiDuplicationApi, String> {
    let dup = DxgiDuplicationApi::new(monitor).map_err(|e| e.to_string())?;
    let rotation = dup.duplication_desc().Rotation;
    if rotation != DXGI_MODE_ROTATION_IDENTITY && rotation != DXGI_MODE_ROTATION_UNSPECIFIED {
        return Err("the display is rotated".into());
    }
    Ok(dup)
}

/// Whether Desktop Duplication can record `monitor` (opens a duplication and releases it).
#[must_use]
pub fn available(monitor: Monitor) -> bool {
    open(monitor).is_ok()
}

/// The pointer as the latest frames reported it.
#[derive(Default)]
struct Pointer {
    visible: bool,
    /// Top-left of the pointer image, in desktop-image pixels.
    x: i32,
    y: i32,
    shape: Option<PointerShape>,
}

impl Pointer {
    /// Take in a frame's pointer news: the position when it moved, the image when it changed.
    fn update(&mut self, dup: &IDXGIOutputDuplication, info: &DXGI_OUTDUPL_FRAME_INFO) {
        if info.LastMouseUpdateTime != 0 {
            self.visible = info.PointerPosition.Visible.as_bool();
            self.x = info.PointerPosition.Position.x;
            self.y = info.PointerPosition.Position.y;
        }
        let size = info.PointerShapeBufferSize;
        if size == 0 {
            return;
        }
        let mut data = vec![0u8; size as usize];
        let mut required = 0u32;
        let mut shape = DXGI_OUTDUPL_POINTER_SHAPE_INFO::default();
        // SAFETY: the buffer holds exactly the size this frame reported for the shape.
        let got = unsafe {
            dup.GetFramePointerShape(size, data.as_mut_ptr().cast(), &mut required, &mut shape)
        };
        if got.is_ok() {
            if let Some(kind) = ShapeKind::from_dxgi(shape.Type) {
                self.shape = Some(PointerShape {
                    kind,
                    width: shape.Width,
                    height: shape.Height,
                    pitch: shape.Pitch,
                    data,
                });
            }
        }
    }
}

/// What one attempt to take a frame came to.
enum Outcome {
    Frame(CapturedFrame),
    /// Nothing changed (or only the pointer moved while it isn't drawn).
    Nothing,
    /// The duplication must be reopened (desktop switch, display mode change).
    Lost,
    Failed(String),
}

/// A running capture's state between frames.
struct Pacer {
    clock: Clock,
    crop: Option<CropRegion>,
    cursor: bool,
    /// Seconds between frames (0 = no cap).
    interval: f64,
    last_sent: Option<i64>,
    pointer: Pointer,
    /// Whether a frame with a desktop image has arrived yet. Until one has, the duplication's
    /// image is all black: a fresh duplication often starts with pointer-only frames, and
    /// sending those gave the region picker a black backdrop and takes a black first frame.
    seen_image: bool,
}

impl Pacer {
    fn next(&mut self, api: &mut DxgiDuplicationApi) -> Outcome {
        let mut frame = match api.acquire_next_frame(ACQUIRE_MS) {
            Ok(f) => f,
            Err(DupError::Timeout) => return Outcome::Nothing,
            Err(DupError::AccessLost) => return Outcome::Lost,
            Err(e) => return Outcome::Failed(e.to_string()),
        };
        let info = *frame.frame_info();
        if self.cursor {
            self.pointer.update(frame.duplication(), &info);
        }
        let image = info.LastPresentTime != 0;
        let moved = self.cursor && info.LastMouseUpdateTime != 0;
        self.seen_image |= image;
        if !self.seen_image || (!image && !moved) {
            return Outcome::Nothing;
        }
        // Hold the frame until the cap allows the next one.
        if let Some(last) = self.last_sent {
            let wait = self.interval - self.clock.seconds_between(last, self.clock.now());
            if wait > 0.0 {
                std::thread::sleep(Duration::from_secs_f64(wait));
            }
        }
        let now = self.clock.now();
        // The present time is read from the same performance counter; trust it only when
        // it's plausibly recent.
        let qpc = if image && (info.LastPresentTime - now).abs() < self.clock.freq() {
            info.LastPresentTime
        } else {
            now
        };
        let (w, h) = (frame.width(), frame.height());
        let (x0, y0, cw, ch) = match self.crop {
            Some(r) => clamp_region(r, w, h),
            None => (0, 0, w, h),
        };
        let buffer = match frame.buffer_crop(x0, y0, x0 + cw, y0 + ch) {
            Ok(b) => b,
            Err(e) => return Outcome::Failed(e.to_string()),
        };
        let mut scratch = Vec::new();
        let mut bgra = buffer.as_nopadding_buffer(&mut scratch).to_vec();
        let drawn = self.cursor && self.pointer.visible;
        if let Some(shape) = self.pointer.shape.as_ref().filter(|_| drawn) {
            let (px, py) = (self.pointer.x - x0 as i32, self.pointer.y - y0 as i32);
            pointer::draw(&mut bgra, cw, ch, px, py, shape);
        }
        self.last_sent = Some(now);
        Outcome::Frame(CapturedFrame {
            width: cw,
            height: ch,
            bgra,
            qpc,
        })
    }
}

/// Record `monitor` until `stop`, sending frames to `tx` (cropped to `crop`, paced to
/// `opts.max_fps`, with the pointer drawn when `opts.cursor`). Fails, having sent nothing,
/// when Duplication can't record this display; the caller then falls back to WGC. Once
/// frames flow (`started` has been called) it rides out desktop switches by reopening, and
/// returns when stopped or when the receiver is gone.
///
/// # Errors
/// Returns a message if the duplication can't be opened.
pub fn run(
    monitor: Monitor,
    tx: &SyncSender<CapturedFrame>,
    stop: &AtomicBool,
    crop: Option<CropRegion>,
    dropped: &AtomicU64,
    opts: CaptureOptions,
    started: impl FnOnce(),
) -> Result<(), String> {
    let mut dup = Some(open(monitor)?);
    started();
    let interval = if opts.max_fps == 0 {
        0.0
    } else {
        1.0 / f64::from(opts.max_fps)
    };
    let mut pacer = Pacer {
        clock: Clock::new(),
        crop,
        cursor: opts.cursor,
        interval,
        last_sent: None,
        pointer: Pointer::default(),
        seen_image: false,
    };
    let mut failures = 0u64;
    while !stop.load(Ordering::Relaxed) {
        let Some(api) = dup.as_mut() else {
            std::thread::sleep(REOPEN_WAIT);
            dup = open(monitor).ok();
            continue;
        };
        match pacer.next(api) {
            Outcome::Frame(frame) => match tx.try_send(frame) {
                Ok(()) => {}
                Err(TrySendError::Full(_)) => {
                    let n = dropped.fetch_add(1, Ordering::Relaxed) + 1;
                    if n == 1 || n.is_multiple_of(60) {
                        tracing::warn!("frame drain can't keep up, dropped {n} frame(s) so far");
                    }
                }
                Err(TrySendError::Disconnected(_)) => break,
            },
            Outcome::Nothing => {}
            Outcome::Lost => {
                dup = dup.take().and_then(|d| d.recreate().ok());
            }
            Outcome::Failed(e) => {
                failures += 1;
                if failures == 1 || failures.is_multiple_of(100) {
                    tracing::warn!("desktop duplication frame failed ({failures}x): {e}");
                }
                std::thread::sleep(Duration::from_millis(20));
            }
        }
    }
    Ok(())
}
