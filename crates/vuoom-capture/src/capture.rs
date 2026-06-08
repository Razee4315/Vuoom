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
use windows_capture::graphics_capture_api::InternalCaptureControl;
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
}

impl GraphicsCaptureApiHandler for Handler {
    type Flags = (Sender<CapturedFrame>, Arc<AtomicBool>);
    type Error = Box<dyn std::error::Error + Send + Sync>;

    fn new(ctx: Context<Self::Flags>) -> Result<Self, Self::Error> {
        let (tx, stop) = ctx.flags;
        Ok(Self {
            tx,
            clock: Clock::new(),
            stop,
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
        let bgra = buffer.as_nopadding_buffer(&mut scratch).to_vec();
        let _ = self.tx.send(CapturedFrame {
            width,
            height,
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
) -> Result<(), CaptureError> {
    let monitor = Monitor::primary().map_err(|e| CaptureError::Start(e.to_string()))?;
    let settings = Settings::new(
        monitor,
        CursorCaptureSettings::Default,
        DrawBorderSettings::Default,
        SecondaryWindowSettings::Default,
        MinimumUpdateIntervalSettings::Default,
        DirtyRegionSettings::Default,
        ColorFormat::Bgra8,
        (tx, stop),
    );
    Handler::start(settings).map_err(|e| CaptureError::Start(e.to_string()))?;
    Ok(())
}

/// Spawn primary-display capture on a background thread; returns the frame receiver and a
/// [`CaptureHandle`] to stop it.
#[must_use]
pub fn spawn_primary_display() -> (Receiver<CapturedFrame>, CaptureHandle) {
    let (tx, rx) = channel();
    let stop = Arc::new(AtomicBool::new(false));
    let handle = CaptureHandle {
        stop: Arc::clone(&stop),
    };
    std::thread::spawn(move || {
        if let Err(e) = run_primary_display(tx, stop) {
            tracing::error!("screen capture stopped: {e}");
        }
    });
    (rx, handle)
}
