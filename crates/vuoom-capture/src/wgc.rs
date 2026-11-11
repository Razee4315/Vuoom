//! Windows capture: Windows Graphics Capture via `windows-capture`, with DXGI Desktop
//! Duplication for displays it can record ([`crate::dxgi`]). Fills in `crate::os::run`.

use crate::capture::{
    crop_bgra, Backend, CaptureError, CaptureOptions, CaptureSource, CapturedFrame, CropRegion,
    FramePacer, Output,
};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, Sender, SyncSender, TrySendError};
use std::sync::Arc;
use std::time::Duration;
use vuoom_input::Clock;
use windows_capture::capture::{Context, GraphicsCaptureApiHandler};
use windows_capture::frame::Frame;
use windows_capture::graphics_capture_api::{GraphicsCaptureApi, InternalCaptureControl};
use windows_capture::monitor::Monitor;
use windows_capture::settings::{
    ColorFormat, CursorCaptureSettings, DirtyRegionSettings, DrawBorderSettings,
    MinimumUpdateIntervalSettings, SecondaryWindowSettings, Settings,
};
use windows_capture::window::Window;

/// The cursor setting for a capture. Where the OS can't change it (older Windows 10
/// builds), the system default applies: requesting a value there fails the session.
fn cursor_setting(show: bool) -> CursorCaptureSettings {
    if !GraphicsCaptureApi::is_cursor_settings_supported().unwrap_or(false) {
        return CursorCaptureSettings::Default;
    }
    if show {
        CursorCaptureSettings::WithCursor
    } else {
        CursorCaptureSettings::WithoutCursor
    }
}

/// The compositor-side frame throttle for a `max_fps` target (`0` = no cap).
///
/// Left at its default, WGC delivers a frame on every DWM composition: 144 or 240 per second
/// on a high-refresh display, each one a full GPU→CPU copy we then throw away downstream.
/// `MinUpdateInterval` makes the compositor itself coalesce updates, and it always hands us
/// the *latest* content once the interval passes, so nothing visible is ever lost. Measured
/// throttling undershoots (a 1/60 s interval yields ~45 fps, see windows-capture#190), so the
/// interval is set 1.5× tighter than the target to land at or just above it. Windows 10
/// lacks the API; there the setting is skipped and capture runs uncapped as before.
fn update_interval(max_fps: u32) -> MinimumUpdateIntervalSettings {
    let supported = GraphicsCaptureApi::is_minimum_update_interval_supported().unwrap_or(false);
    if max_fps == 0 || !supported {
        return MinimumUpdateIntervalSettings::Default;
    }
    let interval = 1.0 / (f64::from(max_fps) * 1.5);
    MinimumUpdateIntervalSettings::Custom(Duration::from_secs_f64(interval))
}

struct Handler {
    tx: SyncSender<CapturedFrame>,
    clock: Clock,
    stop: Arc<AtomicBool>,
    crop: Option<CropRegion>,
    /// Frames dropped because the bounded channel was full (drain couldn't keep up). Shared
    /// with the owning [`CaptureHandle`] so the caller can surface the count to the user.
    dropped: Arc<AtomicU64>,
    /// Holds the take to its frame rate. The compositor's own throttle (see
    /// [`update_interval`]) only exists on Windows 11 and lands a little above the target;
    /// on Windows 10 every screen refresh arrives here.
    pacer: FramePacer,
    /// Frame buffers the consumer is done with (see [`CaptureHandle::recycle`]).
    recycle: Receiver<Vec<u8>>,
    /// Unpacks frames that arrive with row padding. Kept for the whole take: a frame is
    /// megabytes, and a fresh buffer has every page zeroed by the OS on first touch.
    unpadded: Vec<u8>,
}

impl GraphicsCaptureApiHandler for Handler {
    type Flags = (
        SyncSender<CapturedFrame>,
        Arc<AtomicBool>,
        Option<CropRegion>,
        Arc<AtomicU64>,
        u32,
        Receiver<Vec<u8>>,
    );
    type Error = Box<dyn std::error::Error + Send + Sync>;

    fn new(ctx: Context<Self::Flags>) -> Result<Self, Self::Error> {
        let (tx, stop, crop, dropped, max_fps, recycle) = ctx.flags;
        let clock = Clock::new();
        Ok(Self {
            tx,
            clock,
            stop,
            crop,
            dropped,
            pacer: FramePacer::new(max_fps, clock.freq()),
            recycle,
            unpadded: Vec::new(),
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
        // Prefer WGC's SystemRelativeTime (the frame's real capture instant) over the callback
        // arrival time, so frames align tightly with input events. SystemRelativeTime shares the
        // QueryPerformanceCounter epoch but is expressed in 100 ns ticks, so rescale to raw QPC
        // ticks: qpc = ticks * freq / 10_000_000 (i128 avoids overflow on long uptimes).
        //
        // Defensive: if the timestamp is missing or lands implausibly far from `now` (i.e. it is
        // NOT on the QPC axis we assume, a wrong-epoch conversion would poison every frame
        // time), fall back to the callback time. `now` here is the capture instant to within the
        // WGC→callback latency, so a sane frame time is at most ~1 s away.
        let now = self.clock.now();
        // Not due yet: leave the frame on the GPU. Asking for its buffer is what copies it.
        if !self.pacer.admit(now) {
            return Ok(());
        }
        let freq = i128::from(self.clock.freq());
        let qpc = frame
            .timestamp()
            .ok()
            // 100 ns ticks → raw QPC ticks. Stay in i128 through the sanity check so a
            // wrong-epoch value can never overflow the i64 subtraction below.
            .map(|ts| (i128::from(ts.Duration) * freq) / 10_000_000)
            .filter(|&t| (t - i128::from(now)).abs() < freq)
            .map(|t| t as i64)
            .unwrap_or(now);
        let buffer = frame.buffer()?;
        let full = buffer.as_nopadding_buffer(&mut self.unpadded);
        let mut bgra = self.recycle.try_recv().unwrap_or_default();
        let (w, h) = match self.crop {
            Some(r) => crop_bgra(full, width, height, r, &mut bgra),
            None => {
                bgra.clear();
                bgra.extend_from_slice(full);
                (width, height)
            }
        };
        // Bounded, drop-newest: never block this WGC callback thread and never let full
        // BGRA frames pile up in RAM. If the drain lags we drop the newest frame (and warn);
        // if the drain has died (e.g. a disk write failed) the channel disconnects, so stop
        // capturing rather than spin producing frames nothing consumes.
        let captured = CapturedFrame {
            width: w,
            height: h,
            bgra,
            qpc,
            // Graphics Capture doesn't say what changed.
            dirty: None,
        };
        match self.tx.try_send(captured) {
            Ok(()) => {}
            Err(TrySendError::Full(_)) => {
                let n = self.dropped.fetch_add(1, Ordering::Relaxed) + 1;
                if n == 1 || n.is_multiple_of(60) {
                    tracing::warn!("frame drain can't keep up, dropped {n} frame(s) so far");
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

/// Resolve a window by HWND among the enumerable top-level windows.
fn pick_window(hwnd: isize) -> Result<Window, CaptureError> {
    let windows = Window::enumerate().map_err(|e| CaptureError::Start(e.to_string()))?;
    windows
        .into_iter()
        .find(|w| w.as_raw_hwnd() as isize == hwnd)
        .ok_or_else(|| CaptureError::Start(format!("capture window {hwnd} not found")))
}

/// Capture a top-level window's client area, **blocking** until stopped; BGRA frames go
/// to `tx`. Window frames arrive client-sized, so `crop` (when set) is client-relative.
///
/// # Errors
/// Returns [`CaptureError`] if the window or capture session cannot be started.
pub(crate) fn run_window(
    out: Output,
    stop: Arc<AtomicBool>,
    crop: Option<CropRegion>,
    hwnd: isize,
    opts: CaptureOptions,
) -> Result<(), CaptureError> {
    let window = pick_window(hwnd)?;
    let border = if GraphicsCaptureApi::is_border_settings_supported().unwrap_or(false) {
        DrawBorderSettings::WithoutBorder
    } else {
        DrawBorderSettings::Default
    };
    let settings = Settings::new(
        window,
        cursor_setting(opts.cursor),
        border,
        SecondaryWindowSettings::Default,
        update_interval(opts.max_fps),
        DirtyRegionSettings::Default,
        ColorFormat::Bgra8,
        (out.tx, stop, crop, out.dropped, opts.max_fps, out.recycle),
    );
    Handler::start(settings).map_err(|e| CaptureError::Start(e.to_string()))?;
    Ok(())
}

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
pub(crate) fn run_display(
    out: Output,
    stop: Arc<AtomicBool>,
    crop: Option<CropRegion>,
    monitor: Option<&str>,
    opts: CaptureOptions,
) -> Result<(), CaptureError> {
    let monitor = pick_monitor(monitor)?;
    // Capture without the OS "being captured" highlight where the platform allows it
    // (Windows 11+, via the `IsBorderRequired` API). On Windows 10 that API is absent and the
    // border can't be removed, so we fall back to Default, requesting a specific value there
    // makes the capture session fail to start.
    let border = if GraphicsCaptureApi::is_border_settings_supported().unwrap_or(false) {
        DrawBorderSettings::WithoutBorder
    } else {
        DrawBorderSettings::Default
    };
    let settings = Settings::new(
        monitor,
        cursor_setting(opts.cursor),
        border,
        SecondaryWindowSettings::Default,
        update_interval(opts.max_fps),
        DirtyRegionSettings::Default,
        ColorFormat::Bgra8,
        (out.tx, stop, crop, out.dropped, opts.max_fps, out.recycle),
    );
    Handler::start(settings).map_err(|e| CaptureError::Start(e.to_string()))?;
    Ok(())
}

/// Whether a recording of this display (by device name; primary if `None`) would use
/// Desktop Duplication, and so keep Vuoom's capture-excluded windows out of it wherever they
/// sit. Opens a duplication to find out, and releases it.
#[must_use]
pub fn duplication_available(monitor: Option<&str>) -> bool {
    pick_monitor(monitor).is_ok_and(crate::dxgi::available)
}

/// Capture a display, **blocking** until stopped: with Desktop Duplication when it can record
/// this display, else with WGC. `ready` hears which one runs before the first frame.
fn run_monitor(
    out: Output,
    stop: Arc<AtomicBool>,
    crop: Option<CropRegion>,
    monitor: Option<&str>,
    opts: CaptureOptions,
    ready: &Sender<Backend>,
) -> Result<(), CaptureError> {
    let picked = pick_monitor(monitor)?;
    let started = || {
        let _ = ready.send(Backend::DesktopDuplication);
    };
    match crate::dxgi::run(picked, &out, &stop, crop, opts, started) {
        Ok(()) => Ok(()),
        Err(e) => {
            tracing::info!("desktop duplication unavailable ({e}), using graphics capture");
            let _ = ready.send(Backend::GraphicsCapture);
            run_display(out, stop, crop, monitor, opts)
        }
    }
}

/// Run a capture of `source`, **blocking** until stopped; `ready` hears which API runs.
pub(crate) fn run(
    source: &CaptureSource,
    out: Output,
    stop: Arc<AtomicBool>,
    crop: Option<CropRegion>,
    opts: CaptureOptions,
    ready: &Sender<Backend>,
) -> Result<(), CaptureError> {
    match source {
        CaptureSource::Monitor { name } => {
            run_monitor(out, stop, crop, name.as_deref(), opts, ready)
        }
        CaptureSource::Window { hwnd } => {
            let _ = ready.send(Backend::GraphicsCapture);
            run_window(out, stop, crop, *hwnd, opts)
        }
    }
}
