//! macOS capture: ScreenCaptureKit (macOS 12.3+; Vuoom targets 14+). Fills in
//! `crate::os::run`.
//!
//! One `SCStream` per take. Its content filter leaves every window of this process out, so
//! the recording panel, the region border and any dialog are never recorded, wherever they
//! sit, and what is behind them shows (unlike Windows 10, no drag wall is needed). Frames are
//! asked for at the display's pixel size (Retina sharp), in BGRA and sRGB, paced by the
//! stream itself (`minimumFrameInterval`) and again by [`FramePacer`]. Each frame's pixel
//! buffer is copied out tightly packed (IOSurface rows are padded), cropped by the stream's
//! `sourceRect`, and stamped with its presentation time on the host clock, the same clock
//! [`vuoom_input::Clock`] reads.

use crate::capture::{
    clamp_region, Backend, CaptureError, CaptureOptions, CaptureSource, CapturedFrame, CropRegion,
    FramePacer, Output,
};
use block2::RcBlock;
use dispatch2::DispatchQueue;
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObject, NSObjectProtocol, ProtocolObject};
use objc2::{define_class, msg_send, AllocAnyThread, DefinedClass};
use objc2_core_foundation::{CFArray, CFRetained, CGPoint, CGRect, CGSize};
use objc2_core_graphics::{kCGColorSpaceSRGB, CGPreflightScreenCaptureAccess};
use objc2_core_media::{CMSampleBuffer, CMTime};
use objc2_core_video::{
    kCVPixelFormatType_32BGRA, CVPixelBufferGetBaseAddress, CVPixelBufferGetBytesPerRow,
    CVPixelBufferGetHeight, CVPixelBufferGetWidth, CVPixelBufferLockBaseAddress,
    CVPixelBufferLockFlags, CVPixelBufferUnlockBaseAddress,
};
use objc2_foundation::{NSArray, NSDictionary, NSError, NSNumber, NSString};
use objc2_screen_capture_kit::{
    SCCaptureResolutionType, SCContentFilter, SCDisplay, SCRunningApplication, SCShareableContent,
    SCStream, SCStreamConfiguration, SCStreamFrameInfoStatus, SCStreamOutput, SCStreamOutputType,
    SCWindow,
};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender, SyncSender, TrySendError};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use vuoom_input::Clock;

/// The error a capture reports when macOS hasn't given Vuoom the Screen Recording
/// permission. The UI turns it into a "Grant access" prompt.
pub const PERMISSION_ERROR: &str = "screen-recording-permission";

/// How long to wait for ScreenCaptureKit to answer (it is quick; this bounds a stuck daemon).
const SCK_WAIT: Duration = Duration::from_secs(6);

/// `SCFrameStatus.complete`: a frame with new pixels (others are status-only).
const FRAME_COMPLETE: isize = 0;

/// A value that may cross threads. ScreenCaptureKit objects are thread-safe to hand over;
/// the bindings just don't mark them `Send`.
struct Sendable<T>(T);
// SAFETY: only used for retained, immutable SCK objects passed from a completion handler.
unsafe impl<T> Send for Sendable<T> {}

fn describe(err: *mut NSError) -> String {
    if err.is_null() {
        return "unknown error".into();
    }
    // SAFETY: a non-null NSError handed to a completion handler is valid for its duration.
    let err = unsafe { &*err };
    err.localizedDescription().to_string()
}

/// Whether Vuoom may record the screen. Doesn't prompt.
#[must_use]
pub fn screen_access() -> bool {
    CGPreflightScreenCaptureAccess()
}

/// Ask macOS for the Screen Recording permission (shows the system prompt the first time;
/// afterwards the user grants it in System Settings). Returns whether it is granted now.
pub fn request_screen_access() -> bool {
    objc2_core_graphics::CGRequestScreenCaptureAccess()
}

/// What is on screen right now: displays, windows and apps.
fn shareable() -> Result<Retained<SCShareableContent>, CaptureError> {
    let (tx, rx) = channel::<Result<Sendable<Retained<SCShareableContent>>, String>>();
    let block = RcBlock::new(move |content: *mut SCShareableContent, err: *mut NSError| {
        // SAFETY: a non-null content pointer is a live object; retain it to keep it.
        let got = unsafe { Retained::retain(content) };
        let _ = tx.send(got.map(Sendable).ok_or_else(|| describe(err)));
    });
    // SAFETY: the block lives until the handler runs (ScreenCaptureKit copies it).
    unsafe {
        SCShareableContent::getShareableContentExcludingDesktopWindows_onScreenWindowsOnly_completionHandler(
            true, true, &block,
        );
    }
    match rx.recv_timeout(SCK_WAIT) {
        Ok(Ok(content)) => Ok(content.0),
        Ok(Err(e)) => {
            if screen_access() {
                Err(CaptureError::Start(e))
            } else {
                Err(CaptureError::Start(PERMISSION_ERROR.into()))
            }
        }
        Err(_) => Err(CaptureError::Start(
            "ScreenCaptureKit did not answer".into(),
        )),
    }
}

/// One window a take can record, for the picker.
#[derive(Debug, Clone)]
pub struct ShareableWindow {
    pub id: u32,
    pub title: String,
    pub app: String,
    /// Frame in points (global desktop space, origin top-left of the main display).
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

/// Windows worth recording: on screen, normal level, titled, not tiny, not Vuoom's own.
/// Front to back.
///
/// # Errors
/// When ScreenCaptureKit can't list them (no permission, typically).
pub fn shareable_windows() -> Result<Vec<ShareableWindow>, CaptureError> {
    let content = shareable()?;
    let own = std::process::id() as i32;
    let mut out = Vec::new();
    // SAFETY: plain property reads on live SCK objects.
    unsafe {
        for w in &content.windows() {
            if w.windowLayer() != 0 || !w.isOnScreen() {
                continue;
            }
            let app = w.owningApplication();
            if app.as_ref().is_some_and(|a| a.processID() == own) {
                continue;
            }
            let title = w.title().map(|t| t.to_string()).unwrap_or_default();
            let f = w.frame();
            if title.trim().is_empty() || f.size.width < 80.0 || f.size.height < 60.0 {
                continue;
            }
            let app_name = app
                .map(|a| a.applicationName().to_string())
                .unwrap_or_default();
            // Menu-bar extras and the Dock live on other layers; Window Server's own
            // overlays have no app name.
            if app_name.is_empty() {
                continue;
            }
            out.push(ShareableWindow {
                id: w.windowID(),
                title,
                app: app_name,
                x: f.origin.x,
                y: f.origin.y,
                w: f.size.width,
                h: f.size.height,
            });
        }
    }
    Ok(out)
}

/// The display a key names (`CGDirectDisplayID` as text), else the main display.
fn pick_display(
    content: &SCShareableContent,
    name: Option<&str>,
) -> Result<Retained<SCDisplay>, CaptureError> {
    // SAFETY: plain property reads.
    let displays = unsafe { content.displays() };
    let wanted: Option<u32> = name.and_then(|n| n.parse().ok());
    let main = objc2_core_graphics::CGMainDisplayID();
    let find = |id: u32| displays.iter().find(|d| unsafe { d.displayID() } == id);
    wanted
        .and_then(find)
        .or_else(|| find(main))
        .or_else(|| displays.firstObject())
        .ok_or_else(|| CaptureError::Start("no display to record".into()))
}

fn pick_window(content: &SCShareableContent, id: u32) -> Result<Retained<SCWindow>, CaptureError> {
    // SAFETY: plain property reads.
    unsafe { content.windows() }
        .iter()
        .find(|w| unsafe { w.windowID() } == id)
        .ok_or_else(|| CaptureError::Start(format!("capture window {id} not found")))
}

/// Pixels per point of the display with this id (2 on Retina).
fn display_scale(id: u32) -> f64 {
    vuoom_input::mac::displays()
        .into_iter()
        .find(|d| d.id == id)
        .map_or(1.0, |d| d.scale)
}

/// Pixels per point where a frame (points) sits: the display holding its center.
fn scale_for_frame(f: CGRect) -> f64 {
    let (cx, cy) = (
        f.origin.x + f.size.width / 2.0,
        f.origin.y + f.size.height / 2.0,
    );
    let list = vuoom_input::mac::displays();
    list.iter()
        .find(|d| cx >= d.x && cx < d.x + d.w && cy >= d.y && cy < d.y + d.h)
        .or_else(|| list.iter().find(|d| d.primary))
        .map_or(1.0, |d| d.scale)
}

/// Everything the frame handler needs, owned by the output object.
struct Sink {
    tx: SyncSender<CapturedFrame>,
    stop: Arc<AtomicBool>,
    dropped: Arc<AtomicU64>,
    recycle: Mutex<Receiver<Vec<u8>>>,
    pacer: Mutex<FramePacer>,
    clock: Clock,
    /// Set when the consumer hung up: the take is over.
    dead: Arc<AtomicBool>,
    /// Frames delivered so far (a screenshot waits for one).
    delivered: AtomicU64,
}

impl Sink {
    fn on_frame(&self, sample: &CMSampleBuffer) {
        if self.stop.load(Ordering::Relaxed) || self.dead.load(Ordering::Relaxed) {
            return;
        }
        if !is_complete(sample) {
            return;
        }
        let now = self.clock.now();
        // The presentation time is on the host clock, in seconds. Fall back to arrival time
        // if it is ever off that axis (more than a second away).
        // SAFETY: plain reads of a live sample buffer.
        let pts = unsafe { sample.presentation_time_stamp() };
        let stamp = if pts.flags.contains(objc2_core_media::CMTimeFlags::Valid) {
            // SAFETY: CMTimeGetSeconds is a pure conversion.
            (unsafe { pts.seconds() } * 1e9) as i64
        } else {
            now
        };
        let qpc = if (stamp - now).abs() < 1_000_000_000 {
            stamp
        } else {
            now
        };
        let admit = self.pacer.lock().map(|mut p| p.admit(qpc)).unwrap_or(true);
        if !admit {
            return;
        }
        // SAFETY: the image buffer belongs to the sample; we only read it while locked.
        let Some(image) = (unsafe { sample.image_buffer() }) else {
            return;
        };
        let mut bgra = self
            .recycle
            .lock()
            .ok()
            .and_then(|r| r.try_recv().ok())
            .unwrap_or_default();
        let (w, h) = copy_tight(&image, &mut bgra);
        if w == 0 || h == 0 {
            return;
        }
        let frame = CapturedFrame {
            width: w,
            height: h,
            bgra,
            qpc,
            // ScreenCaptureKit's dirty rects aren't relied on: the frame store diffs.
            dirty: None,
        };
        match self.tx.try_send(frame) {
            Ok(()) => {
                self.delivered.fetch_add(1, Ordering::Relaxed);
            }
            Err(TrySendError::Full(_)) => {
                let n = self.dropped.fetch_add(1, Ordering::Relaxed) + 1;
                if n == 1 || n.is_multiple_of(60) {
                    tracing::warn!("frame drain can't keep up, dropped {n} frame(s) so far");
                }
            }
            Err(TrySendError::Disconnected(_)) => self.dead.store(true, Ordering::Relaxed),
        }
    }
}

/// Whether a sample carries a complete new frame (ScreenCaptureKit also sends idle,
/// blank and suspended status samples without pixels).
fn is_complete(sample: &CMSampleBuffer) -> bool {
    // SAFETY: reads of the sample's attachment dictionaries, toll-free bridged to NSArray.
    unsafe {
        let Some(array): Option<CFRetained<CFArray>> = sample.sample_attachments_array(false)
        else {
            return false;
        };
        let array: &NSArray<NSDictionary<NSString, AnyObject>> =
            &*(std::ptr::from_ref::<CFArray>(&array).cast());
        let Some(info) = array.firstObject() else {
            return false;
        };
        let Some(status) = info.objectForKey(SCStreamFrameInfoStatus) else {
            return false;
        };
        let Ok(number) = status.downcast::<NSNumber>() else {
            return false;
        };
        number.integerValue() == FRAME_COMPLETE
    }
}

/// Copy a BGRA pixel buffer into `out`, tightly packed (no row padding). Returns its size.
fn copy_tight(image: &objc2_core_video::CVPixelBuffer, out: &mut Vec<u8>) -> (u32, u32) {
    let flags = CVPixelBufferLockFlags::ReadOnly;
    // SAFETY: lock / read / unlock of a live pixel buffer; the base address is valid for
    // `bytes_per_row * height` bytes while locked.
    unsafe {
        if CVPixelBufferLockBaseAddress(image, flags) != 0 {
            return (0, 0);
        }
        let w = CVPixelBufferGetWidth(image);
        let h = CVPixelBufferGetHeight(image);
        let stride = CVPixelBufferGetBytesPerRow(image);
        let base = CVPixelBufferGetBaseAddress(image).cast::<u8>();
        let row = w * 4;
        out.clear();
        if !base.is_null() && stride >= row {
            out.reserve(row * h);
            if stride == row {
                out.extend_from_slice(std::slice::from_raw_parts(base, row * h));
            } else {
                for y in 0..h {
                    out.extend_from_slice(std::slice::from_raw_parts(base.add(y * stride), row));
                }
            }
        }
        CVPixelBufferUnlockBaseAddress(image, flags);
        if out.len() == row * h {
            (w as u32, h as u32)
        } else {
            (0, 0)
        }
    }
}

define_class!(
    // SAFETY: NSObject has no subclassing requirements; the output only reads its ivars.
    #[unsafe(super(NSObject))]
    #[name = "VuoomScreenOutput"]
    #[ivars = Sink]
    struct ScreenOutput;

    unsafe impl NSObjectProtocol for ScreenOutput {}

    unsafe impl SCStreamOutput for ScreenOutput {
        #[unsafe(method(stream:didOutputSampleBuffer:ofType:))]
        fn stream_did_output(
            &self,
            _stream: &SCStream,
            sample: &CMSampleBuffer,
            kind: SCStreamOutputType,
        ) {
            if kind == SCStreamOutputType::Screen {
                self.ivars().on_frame(sample);
            }
        }
    }
);

impl ScreenOutput {
    fn new(sink: Sink) -> Retained<Self> {
        let this = Self::alloc().set_ivars(sink);
        // SAFETY: NSObject's designated initializer.
        unsafe { msg_send![super(this), init] }
    }
}

/// Run a completion-handler call and wait for its error (or success).
fn wait_done(call: impl FnOnce(&block2::DynBlock<dyn Fn(*mut NSError)>)) -> Result<(), String> {
    let (tx, rx) = channel::<Result<(), String>>();
    let block = RcBlock::new(move |err: *mut NSError| {
        let _ = tx.send(if err.is_null() {
            Ok(())
        } else {
            Err(describe(err))
        });
    });
    call(&block);
    rx.recv_timeout(SCK_WAIT)
        .unwrap_or_else(|_| Err("ScreenCaptureKit did not answer".into()))
}

/// What the stream records and how big its frames are.
struct Plan {
    filter: Retained<SCContentFilter>,
    /// The recorded area in points, in the filter's own space (display or window).
    area: CGSize,
    scale: f64,
}

fn plan(content: &SCShareableContent, source: &CaptureSource) -> Result<Plan, CaptureError> {
    match source {
        CaptureSource::Monitor { name } => {
            let display = pick_display(content, name.as_deref())?;
            let own = std::process::id() as i32;
            // SAFETY: SCK object construction and property reads.
            unsafe {
                let apps: Vec<Retained<SCRunningApplication>> = content
                    .applications()
                    .iter()
                    .filter(|a| a.processID() == own)
                    .collect();
                let apps = NSArray::from_retained_slice(&apps);
                let none = NSArray::<SCWindow>::new();
                // Leaving the whole app out keeps windows it opens mid-take out too.
                let filter =
                    SCContentFilter::initWithDisplay_excludingApplications_exceptingWindows(
                        SCContentFilter::alloc(),
                        &display,
                        &apps,
                        &none,
                    );
                let f = display.frame();
                Ok(Plan {
                    filter,
                    area: f.size,
                    scale: display_scale(display.displayID()),
                })
            }
        }
        CaptureSource::Window { hwnd } => {
            let id =
                u32::try_from(*hwnd).map_err(|_| CaptureError::Start("bad window id".into()))?;
            let window = pick_window(content, id)?;
            // SAFETY: SCK object construction and property reads.
            unsafe {
                let filter = SCContentFilter::initWithDesktopIndependentWindow(
                    SCContentFilter::alloc(),
                    &window,
                );
                let f = window.frame();
                Ok(Plan {
                    filter,
                    area: f.size,
                    scale: scale_for_frame(f),
                })
            }
        }
    }
}

/// The stream configuration: pixel size, crop, pixel format, colour, pacing, cursor.
fn configure(
    plan: &Plan,
    crop: Option<CropRegion>,
    opts: CaptureOptions,
    window: bool,
) -> Retained<SCStreamConfiguration> {
    let full_w = (plan.area.width * plan.scale).round().max(1.0) as u32;
    let full_h = (plan.area.height * plan.scale).round().max(1.0) as u32;
    // SAFETY: plain property writes on a fresh configuration.
    unsafe {
        let config = SCStreamConfiguration::new();
        let (w, h) = match crop {
            Some(r) => {
                let (x, y, cw, ch) = clamp_region(r, full_w, full_h);
                let s = plan.scale;
                config.setSourceRect(CGRect::new(
                    CGPoint::new(f64::from(x) / s, f64::from(y) / s),
                    CGSize::new(f64::from(cw) / s, f64::from(ch) / s),
                ));
                (cw, ch)
            }
            None => (full_w, full_h),
        };
        config.setWidth(w as usize);
        config.setHeight(h as usize);
        config.setScalesToFit(false);
        config.setPixelFormat(kCVPixelFormatType_32BGRA);
        config.setColorSpaceName(kCGColorSpaceSRGB);
        config.setShowsCursor(opts.cursor);
        config.setQueueDepth(6);
        config.setCaptureResolution(SCCaptureResolutionType::Best);
        if window {
            config.setIgnoreShadowsSingleWindow(true);
        }
        let interval = if opts.max_fps == 0 {
            CMTime::new(1, 240)
        } else {
            // A touch tighter than the target so the stream lands at or above it; the
            // pacer holds the exact rate.
            CMTime::new(1, (opts.max_fps * 2).min(240) as i32)
        };
        config.setMinimumFrameInterval(interval);
        config
    }
}

/// Capture `source`, **blocking** until stopped; BGRA frames go to `out.tx`.
pub(crate) fn run(
    source: &CaptureSource,
    out: Output,
    stop: Arc<AtomicBool>,
    crop: Option<CropRegion>,
    opts: CaptureOptions,
    ready: &Sender<Backend>,
) -> Result<(), CaptureError> {
    let _ = ready.send(Backend::ScreenCaptureKit);
    if !screen_access() {
        return Err(CaptureError::Start(PERMISSION_ERROR.into()));
    }
    let content = shareable()?;
    let plan = plan(&content, source)?;
    let window = matches!(source, CaptureSource::Window { .. });
    let config = configure(&plan, crop, opts, window);
    let clock = Clock::new();
    let dead = Arc::new(AtomicBool::new(false));
    let sink = Sink {
        tx: out.tx,
        stop: Arc::clone(&stop),
        dropped: out.dropped,
        recycle: Mutex::new(out.recycle),
        pacer: Mutex::new(FramePacer::new(opts.max_fps, clock.freq())),
        clock,
        dead: Arc::clone(&dead),
        delivered: AtomicU64::new(0),
    };
    let output = ScreenOutput::new(sink);
    let queue = DispatchQueue::new("dev.vuoom.capture", None);
    // SAFETY: SCK stream construction and control; `output` and `queue` outlive the stream.
    unsafe {
        let stream = SCStream::initWithFilter_configuration_delegate(
            SCStream::alloc(),
            &plan.filter,
            &config,
            None,
        );
        stream
            .addStreamOutput_type_sampleHandlerQueue_error(
                ProtocolObject::from_ref(&*output),
                SCStreamOutputType::Screen,
                Some(&queue),
            )
            .map_err(|e| CaptureError::Start(e.localizedDescription().to_string()))?;
        wait_done(|b| stream.startCaptureWithCompletionHandler(Some(b)))
            .map_err(CaptureError::Start)?;
        tracing::info!(
            width = config.width(),
            height = config.height(),
            fps = opts.max_fps,
            "ScreenCaptureKit stream started"
        );
        while !stop.load(Ordering::Relaxed) && !dead.load(Ordering::Relaxed) {
            std::thread::sleep(Duration::from_millis(25));
        }
        let _ = wait_done(|b| stream.stopCaptureWithCompletionHandler(Some(b)));
    }
    // The output's sender drops with it, which ends the consumer's channel.
    drop(output);
    Ok(())
}

/// ScreenCaptureKit hides Vuoom's windows on every display.
#[must_use]
pub fn duplication_available(_monitor: Option<&str>) -> bool {
    true
}
