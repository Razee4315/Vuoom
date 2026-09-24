//! Webcam capture through Media Foundation's source reader.
//!
//! One thread per camera. It opens the device, picks a native format ([`choose_format`]:
//! the widest at most [`MAX_WIDTH`] wide at 24 fps or better, nearest 30 fps), and asks the
//! reader for RGB32, which Media Foundation converts to from whatever the camera sends
//! (MJPG, NV12, YUY2). Every frame is scaled down if needed, encoded as a JPEG, appended to
//! the track with its time on the recording clock, and kept as the latest frame for the
//! live preview bubble.
//!
//! Times: the reader's sample timestamps are steady but count from when the camera
//! started, so [`Pin`] ties them to the recording clock by the first frame's arrival, and
//! re-ties them if they ever drift more than [`REPIN_SECS`] from arrival time.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Widest frame stored; larger camera formats are scaled down to this.
pub const MAX_WIDTH: u32 = 960;
/// Formats slower than this are a last resort.
const MIN_FPS: f64 = 24.0;
/// The frame rate aimed for among fast enough formats.
const TARGET_FPS: f64 = 30.0;
/// Timestamp drift from arrival time beyond which the clock tie is redone.
pub const REPIN_SECS: f64 = 0.25;
/// How long starting waits for the camera to open, and stopping for the thread to end.
const OPEN_TIMEOUT: Duration = Duration::from_secs(10);
const STOP_TIMEOUT: Duration = Duration::from_secs(3);
/// Frames between flushes of the track file (so a crash keeps nearly everything).
const FLUSH_EVERY: u64 = 30;

/// A camera, as the recording UI lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Camera {
    /// Stable device id (the symbolic link).
    pub id: String,
    pub name: String,
}

/// The recording's time origin on the performance-counter clock.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Clock {
    pub start_qpc: i64,
    pub freq: i64,
}

/// A native format a camera offers.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Format {
    pub width: u32,
    pub height: u32,
    pub fps: f64,
}

/// Pick the format to record from what a camera offers: fast enough (24 fps or better)
/// first, then one that fits [`MAX_WIDTH`] (the widest such, or else the narrowest of the
/// rest), then the frame rate nearest 30. Returns an index into `formats`.
#[must_use]
pub fn choose_format(formats: &[Format]) -> Option<usize> {
    let key = |f: &Format| {
        let fast = f.fps + 0.5 >= MIN_FPS;
        let fits = f.width <= MAX_WIDTH;
        let width = i64::from(f.width);
        let size = if fits { width } else { -width };
        let rate = -((f.fps - TARGET_FPS).abs() * 1000.0) as i64;
        (fast, fits, size, rate)
    };
    formats
        .iter()
        .enumerate()
        .max_by_key(|&(_, f)| key(f))
        .map(|(i, _)| i)
}

/// The stored size for frames of `w`×`h`: at most [`MAX_WIDTH`] wide with the aspect kept
/// and even dimensions.
#[must_use]
pub fn fit(w: u32, h: u32) -> (u32, u32) {
    if w <= MAX_WIDTH {
        return (w, h);
    }
    let th = u64::from(h) * u64::from(MAX_WIDTH) / u64::from(w);
    (MAX_WIDTH, ((th as u32) & !1).max(2))
}

/// Area-average a BGRA image of `w`×`h` down to `tw`×`th`.
#[must_use]
pub fn downscale(src: &[u8], w: u32, h: u32, tw: u32, th: u32) -> Vec<u8> {
    let (w, h, tw, th) = (w as usize, h as usize, tw as usize, th as usize);
    let mut out = Vec::with_capacity(tw * th * 4);
    for ty in 0..th {
        let y0 = ty * h / th;
        let y1 = ((ty + 1) * h / th).max(y0 + 1);
        for tx in 0..tw {
            let x0 = tx * w / tw;
            let x1 = ((tx + 1) * w / tw).max(x0 + 1);
            let mut acc = [0u32; 4];
            for y in y0..y1 {
                let row = &src[(y * w + x0) * 4..(y * w + x1) * 4];
                for px in row.as_chunks::<4>().0 {
                    for (a, v) in acc.iter_mut().zip(px) {
                        *a += u32::from(*v);
                    }
                }
            }
            let n = ((y1 - y0) * (x1 - x0)) as u32;
            out.extend(acc.map(|a| ((a + n / 2) / n) as u8));
        }
    }
    out
}

/// Ties a camera's sample timestamps to the recording clock.
#[derive(Debug, Default, Clone, Copy)]
pub struct Pin {
    offset: Option<f64>,
}

impl Pin {
    /// Recording time of a frame stamped `stamp` (100 ns units, any origin) that arrived
    /// `arrived` seconds into the recording.
    pub fn place(&mut self, stamp: i64, arrived: f64) -> f64 {
        let t = stamp as f64 / 1e7;
        let offset = match self.offset {
            Some(o) if (t + o - arrived).abs() <= REPIN_SECS => o,
            _ => arrived - t,
        };
        self.offset = Some(offset);
        t + offset
    }
}

/// What the capture thread shares with its owner.
struct Shared {
    stop: AtomicBool,
    frames: AtomicU64,
    latest: Mutex<Option<Arc<Vec<u8>>>>,
}

/// A running camera. Stop it with [`CameraRecorder::finish`]; dropping it stops it too.
pub struct CameraRecorder {
    shared: Arc<Shared>,
    done: Option<Receiver<Result<u64, String>>>,
    size: (u32, u32),
}

impl CameraRecorder {
    /// Start the camera `id` (`None`: the first one). With `out`, frames are recorded there
    /// as a track timed by `clock`; without, only the live preview runs.
    ///
    /// # Errors
    /// Returns a message if the camera can't be opened.
    pub fn start(id: Option<String>, out: Option<PathBuf>, clock: Clock) -> Result<Self, String> {
        let shared = Arc::new(Shared {
            stop: AtomicBool::new(false),
            frames: AtomicU64::new(0),
            latest: Mutex::new(None),
        });
        let (ready_tx, ready_rx) = mpsc::channel();
        let (done_tx, done_rx) = mpsc::channel();
        let thread_shared = Arc::clone(&shared);
        std::thread::Builder::new()
            .name("vuoom-camera".into())
            .spawn(move || {
                let r = imp::run(id.as_deref(), out, clock, &thread_shared, &ready_tx);
                let _ = done_tx.send(r);
            })
            .map_err(|e| format!("camera thread: {e}"))?;
        match ready_rx.recv_timeout(OPEN_TIMEOUT) {
            Ok(Ok(size)) => Ok(Self {
                shared,
                done: Some(done_rx),
                size,
            }),
            Ok(Err(e)) => Err(e),
            Err(_) => {
                shared.stop.store(true, Ordering::Relaxed);
                Err("the camera did not start".into())
            }
        }
    }

    /// The stored frame size.
    #[must_use]
    pub fn size(&self) -> (u32, u32) {
        self.size
    }

    /// The most recent frame as a JPEG, for the live preview.
    #[must_use]
    pub fn latest_jpeg(&self) -> Option<Arc<Vec<u8>>> {
        let latest = self.shared.latest.lock();
        latest.unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// Stop the camera and close the track. Returns the frames recorded. A camera that
    /// hangs doesn't hold the recording up: after a few seconds the frames already on disk
    /// are kept and the thread is left to finish on its own.
    ///
    /// # Errors
    /// Returns a message if capture failed or the track couldn't be closed.
    pub fn finish(mut self) -> Result<u64, String> {
        self.shared.stop.store(true, Ordering::Relaxed);
        let Some(done) = self.done.take() else {
            return Ok(0);
        };
        match done.recv_timeout(STOP_TIMEOUT) {
            Ok(result) => result,
            Err(_) => {
                tracing::warn!("the camera did not stop in time; keeping the frames written");
                Ok(self.shared.frames.load(Ordering::Relaxed))
            }
        }
    }
}

impl Drop for CameraRecorder {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::Relaxed);
    }
}

/// Cameras connected now.
///
/// # Errors
/// Returns a message if the device list can't be read.
pub fn list_cameras() -> Result<Vec<Camera>, String> {
    imp::list()
}

#[cfg(windows)]
mod imp {
    use super::{choose_format, downscale, fit, Camera, Clock, Format, Pin, Shared, FLUSH_EVERY};
    use crate::jpeg;
    use crate::store::TrackWriter;
    use std::path::PathBuf;
    use std::ptr::addr_of_mut;
    use std::sync::atomic::Ordering;
    use std::sync::mpsc::Sender;
    use std::sync::Arc;
    use windows::core::{Error, Interface, GUID, PWSTR};
    use windows::Win32::Foundation::E_FAIL;
    use windows::Win32::Media::MediaFoundation::*;
    use windows::Win32::System::Com::{CoInitializeEx, CoTaskMemFree, COINIT_MULTITHREADED};
    use windows::Win32::System::Performance::QueryPerformanceCounter;

    fn err(e: Error) -> String {
        format!("camera: {e}")
    }

    fn pack(a: u32, b: u32) -> u64 {
        (u64::from(a) << 32) | u64::from(b)
    }

    fn unpack(v: u64) -> (u32, u32) {
        ((v >> 32) as u32, v as u32)
    }

    /// COM and Media Foundation for this thread (both no-ops when already up).
    fn startup() -> Result<(), String> {
        // SAFETY: per-thread COM initialization, then Media Foundation's reference-counted
        // startup with the SDK version.
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            MFStartup(MF_VERSION, MFSTARTUP_FULL).map_err(err)
        }
    }

    /// Seconds from the recording's start to now.
    fn now(clock: Clock) -> f64 {
        let mut q = 0i64;
        // SAFETY: writes the counter into a local.
        let _ = unsafe { QueryPerformanceCounter(&mut q) };
        (q - clock.start_qpc) as f64 / clock.freq.max(1) as f64
    }

    /// A string attribute of a device, or "" when it has none.
    fn string(a: &IMFActivate, key: &GUID) -> String {
        let mut p = PWSTR::null();
        let mut len = 0u32;
        // SAFETY: on success Media Foundation allocates the string; it's copied, then freed.
        unsafe {
            if a.GetAllocatedString(key, &mut p, &mut len).is_err() {
                return String::new();
            }
            let s = p.to_string().unwrap_or_default();
            CoTaskMemFree(Some(p.0 as *const _));
            s
        }
    }

    /// Every video capture device, with its activation object.
    fn devices() -> Result<Vec<(IMFActivate, Camera)>, String> {
        startup()?;
        // SAFETY: device enumeration; each element of the returned array is taken out
        // (so nothing is released twice) and the array is then freed as the API requires.
        unsafe {
            let mut attrs: Option<IMFAttributes> = None;
            MFCreateAttributes(&mut attrs, 1).map_err(err)?;
            let attrs = attrs.ok_or("camera attributes")?;
            let kind = &MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE;
            let video = &MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_GUID;
            attrs.SetGUID(kind, video).map_err(err)?;
            let mut list: *mut Option<IMFActivate> = std::ptr::null_mut();
            let mut count = 0u32;
            let listed = MFEnumDeviceSources(&attrs, &mut list, &mut count);
            listed.map_err(err)?;
            let mut out = Vec::new();
            for i in 0..count as usize {
                if let Some(a) = (*list.add(i)).take() {
                    let id = string(&a, &MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_SYMBOLIC_LINK);
                    let name = string(&a, &MF_DEVSOURCE_ATTRIBUTE_FRIENDLY_NAME);
                    out.push((a, Camera { id, name }));
                }
            }
            if !list.is_null() {
                CoTaskMemFree(Some(list as *const _));
            }
            Ok(out)
        }
    }

    pub fn list() -> Result<Vec<Camera>, String> {
        Ok(devices()?.into_iter().map(|(_, c)| c).collect())
    }

    /// An open camera, delivering RGB32 frames of `width`×`height`.
    struct Opened {
        source: IMFMediaSource,
        reader: IMFSourceReader,
        width: u32,
        height: u32,
        /// Bytes from one row to the next when the buffer isn't 2D (negative: bottom-up).
        stride: i32,
    }

    impl Drop for Opened {
        fn drop(&mut self) {
            // SAFETY: shutting the device down releases it (and turns its light off).
            let _ = unsafe { self.source.Shutdown() };
        }
    }

    fn open(id: Option<&str>) -> Result<Opened, String> {
        let mut all = devices()?;
        let pick = match id {
            Some(id) => all.iter().position(|(_, c)| c.id == id),
            None => (!all.is_empty()).then_some(0),
        };
        let (activate, camera) = all.swap_remove(pick.ok_or("camera unavailable")?);
        let stream = MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32;
        // SAFETY: Media Foundation setup calls on live interfaces.
        unsafe {
            let source: IMFMediaSource = activate
                .ActivateObject()
                .map_err(|e| format!("{} is unavailable ({e})", camera.name))?;
            let mut attrs: Option<IMFAttributes> = None;
            MFCreateAttributes(&mut attrs, 1).map_err(err)?;
            let attrs = attrs.ok_or("camera attributes")?;
            attrs
                .SetUINT32(&MF_SOURCE_READER_ENABLE_VIDEO_PROCESSING, 1)
                .map_err(err)?;
            let made = MFCreateSourceReaderFromMediaSource(&source, &attrs);
            let reader = made.map_err(err)?;

            // What the camera can do.
            let mut formats = Vec::new();
            let mut rates = Vec::new();
            let native = |i| reader.GetNativeMediaType(stream, i).ok();
            for t in (0u32..).map_while(native) {
                let Ok(size) = t.GetUINT64(&MF_MT_FRAME_SIZE) else {
                    continue;
                };
                let rate = t.GetUINT64(&MF_MT_FRAME_RATE).unwrap_or(pack(30, 1));
                let (width, height) = unpack(size);
                let (num, den) = unpack(rate);
                let fps = f64::from(num) / f64::from(den.max(1));
                formats.push(Format { width, height, fps });
                rates.push(rate);
            }

            // Ask for RGB32 at the chosen size and rate; Media Foundation converts.
            let rgb = || -> windows::core::Result<IMFMediaType> {
                let t = MFCreateMediaType()?;
                t.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)?;
                t.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_RGB32)?;
                Ok(t)
            };
            let chosen = choose_format(&formats).is_some_and(|i| {
                let f = formats[i];
                let Ok(t) = rgb() else {
                    return false;
                };
                let sized = t
                    .SetUINT64(&MF_MT_FRAME_SIZE, pack(f.width, f.height))
                    .and_then(|()| t.SetUINT64(&MF_MT_FRAME_RATE, rates[i]));
                sized.is_ok() && reader.SetCurrentMediaType(stream, None, &t).is_ok()
            });
            if !chosen {
                // Any size the reader can convert.
                let t = rgb().map_err(err)?;
                reader
                    .SetCurrentMediaType(stream, None, &t)
                    .map_err(|e| format!("{} has no usable format ({e})", camera.name))?;
            }
            let current = reader.GetCurrentMediaType(stream).map_err(err)?;
            let size = current.GetUINT64(&MF_MT_FRAME_SIZE).map_err(err)?;
            let (width, height) = unpack(size);
            if width == 0 || height == 0 {
                return Err(format!("{} reported no frame size", camera.name));
            }
            let default_stride = current.GetUINT32(&MF_MT_DEFAULT_STRIDE).ok();
            let stride = default_stride.map_or(width as i32 * 4, |s| s as i32);
            Ok(Opened {
                source,
                reader,
                width,
                height,
                stride,
            })
        }
    }

    /// Copy `h` rows of `w` BGRX pixels, `pitch` bytes apart (negative: bottom-up), as
    /// top-down BGRA with opaque alpha.
    ///
    /// # Safety
    /// Every row `row0 + y * pitch` for `y < h` must be readable for `w * 4` bytes.
    unsafe fn copy_rows(row0: *const u8, pitch: isize, w: usize, h: usize) -> Vec<u8> {
        let mut out = Vec::with_capacity(w * h * 4);
        for y in 0..h {
            // SAFETY: guaranteed by the caller.
            let row = unsafe { std::slice::from_raw_parts(row0.offset(pitch * y as isize), w * 4) };
            let (pixels, _) = row.as_chunks::<4>();
            out.extend(pixels.iter().flat_map(|p| [p[0], p[1], p[2], 255]));
        }
        out
    }

    /// A sample's pixels as tightly packed top-down BGRA.
    fn pixels(sample: &IMFSample, cam: &Opened) -> windows::core::Result<Vec<u8>> {
        let (w, h) = (cam.width as usize, cam.height as usize);
        let row = w * 4;
        // SAFETY: each buffer stays locked while its rows are copied, and the copies stay
        // within the locked scanlines (2D) or the reported buffer length.
        unsafe {
            let buffer = sample.ConvertToContiguousBuffer()?;
            if let Ok(flat) = buffer.cast::<IMF2DBuffer>() {
                let mut row0: *mut u8 = std::ptr::null_mut();
                let mut pitch = 0i32;
                flat.Lock2D(&mut row0, &mut pitch)?;
                let px = copy_rows(row0, pitch as isize, w, h);
                let _ = flat.Unlock2D();
                return Ok(px);
            }
            let mut data: *mut u8 = std::ptr::null_mut();
            let mut len = 0u32;
            buffer.Lock(&mut data, None, Some(addr_of_mut!(len)))?;
            let pitch = (cam.stride.unsigned_abs() as usize).max(row);
            let px = if (len as usize) < pitch * (h - 1) + row {
                Vec::new()
            } else if cam.stride < 0 {
                copy_rows(data.add(pitch * (h - 1)), -(pitch as isize), w, h)
            } else {
                copy_rows(data, pitch as isize, w, h)
            };
            let _ = buffer.Unlock();
            if px.is_empty() {
                return Err(Error::from(E_FAIL));
            }
            Ok(px)
        }
    }

    /// Read frames until asked to stop.
    fn pump(
        cam: &Opened,
        writer: &mut Option<TrackWriter>,
        clock: Clock,
        shared: &Shared,
        size: (u32, u32),
    ) -> Result<(), String> {
        let stream = MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32;
        let ended = (MF_SOURCE_READERF_ERROR.0 | MF_SOURCE_READERF_ENDOFSTREAM.0) as u32;
        let mut pin = Pin::default();
        while !shared.stop.load(Ordering::Relaxed) {
            let mut flags = 0u32;
            let mut stamp = 0i64;
            let mut sample: Option<IMFSample> = None;
            // SAFETY: a synchronous read into locals that outlive the call.
            let read = unsafe {
                cam.reader.ReadSample(
                    stream,
                    0,
                    None,
                    Some(addr_of_mut!(flags)),
                    Some(addr_of_mut!(stamp)),
                    Some(addr_of_mut!(sample)),
                )
            };
            read.map_err(|e| format!("camera read: {e}"))?;
            if flags & ended != 0 {
                return Err("the camera stopped sending frames".into());
            }
            // A gap in the stream, not a frame.
            let Some(sample) = sample else {
                continue;
            };
            let arrived = now(clock);
            let Ok(px) = pixels(&sample, cam) else {
                continue;
            };
            let px = if size == (cam.width, cam.height) {
                px
            } else {
                downscale(&px, cam.width, cam.height, size.0, size.1)
            };
            let encoded = match jpeg::encode_bgra(&px, size.0, size.1) {
                Ok(j) => j,
                Err(e) => {
                    tracing::warn!("camera frame skipped: {e}");
                    continue;
                }
            };
            let t = pin.place(stamp, arrived);
            if let Some(w) = writer.as_mut() {
                if w.push(t, &encoded)? {
                    let n = shared.frames.fetch_add(1, Ordering::Relaxed) + 1;
                    if n.is_multiple_of(FLUSH_EVERY) {
                        w.flush()?;
                    }
                }
            }
            let mut latest = shared.latest.lock().unwrap_or_else(|e| e.into_inner());
            *latest = Some(Arc::new(encoded));
        }
        Ok(())
    }

    pub fn run(
        id: Option<&str>,
        out: Option<PathBuf>,
        clock: Clock,
        shared: &Shared,
        ready: &Sender<Result<(u32, u32), String>>,
    ) -> Result<u64, String> {
        let fail = |e: String| {
            let _ = ready.send(Err(e.clone()));
            Err(e)
        };
        if let Err(e) = startup() {
            return fail(e);
        }
        let cam = match open(id) {
            Ok(c) => c,
            Err(e) => return fail(e),
        };
        let mut writer = match out.map(|p| TrackWriter::create(&p)).transpose() {
            Ok(w) => w,
            Err(e) => return fail(e),
        };
        let size = fit(cam.width, cam.height);
        let _ = ready.send(Ok(size));
        let pumped = pump(&cam, &mut writer, clock, shared, size);
        drop(cam);
        let closed = writer.map_or(Ok(0), TrackWriter::finish);
        pumped.and(closed)
    }
}

#[cfg(not(windows))]
mod imp {
    use super::{Camera, Clock, Shared};
    use std::path::PathBuf;
    use std::sync::mpsc::Sender;

    pub fn list() -> Result<Vec<Camera>, String> {
        Ok(Vec::new())
    }

    pub fn run(
        _id: Option<&str>,
        _out: Option<PathBuf>,
        _clock: Clock,
        _shared: &Shared,
        ready: &Sender<Result<(u32, u32), String>>,
    ) -> Result<u64, String> {
        let e = "camera capture is Windows-only".to_string();
        let _ = ready.send(Err(e.clone()));
        Err(e)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn f(width: u32, height: u32, fps: f64) -> Format {
        Format { width, height, fps }
    }

    #[test]
    fn picks_the_widest_fitting_fast_format_near_30_fps() {
        let offered = [
            f(640, 480, 30.0),
            f(1280, 720, 30.0),
            f(960, 540, 60.0),
            f(960, 540, 30.0),
            f(1920, 1080, 5.0),
        ];
        assert_eq!(choose_format(&offered), Some(3));
        // Nothing fits: the narrowest fast one, to scale down.
        let big = [f(1920, 1080, 30.0), f(1280, 720, 30.0), f(1280, 720, 10.0)];
        assert_eq!(choose_format(&big), Some(1));
        // Only slow formats: still one of them.
        assert_eq!(choose_format(&[f(640, 480, 15.0)]), Some(0));
        assert_eq!(choose_format(&[]), None);
        // 23.976 fps counts as fast.
        let film = [f(640, 480, 23.976), f(800, 600, 15.0)];
        assert_eq!(choose_format(&film), Some(0));
    }

    #[test]
    fn large_frames_fit_the_stored_width_with_even_sides() {
        assert_eq!(fit(640, 480), (640, 480));
        assert_eq!(fit(1280, 720), (960, 540));
        assert_eq!(fit(1920, 1080), (960, 540));
        let (w, h) = fit(1000, 751);
        assert_eq!(w, MAX_WIDTH);
        assert!(h.is_multiple_of(2));
    }

    #[test]
    fn downscaling_averages_each_area() {
        // 2×2 to 1×1: the mean of the four pixels.
        let src = [
            0, 0, 0, 255, 100, 0, 0, 255, 0, 200, 0, 255, 100, 200, 40, 255,
        ];
        assert_eq!(downscale(&src, 2, 2, 1, 1), vec![50, 100, 10, 255]);
        // 4×1 to 2×1 keeps left and right apart.
        let row = [
            10, 10, 10, 255, 30, 30, 30, 255, 200, 0, 0, 255, 100, 0, 0, 255,
        ];
        assert_eq!(
            downscale(&row, 4, 1, 2, 1),
            vec![20, 20, 20, 255, 150, 0, 0, 255]
        );
    }

    #[test]
    fn timestamps_are_tied_to_the_first_arrival_and_retied_after_a_jump() {
        let mut pin = Pin::default();
        // Camera clock at 100 s; the frame arrived 0.5 s into the recording.
        assert!((pin.place(1_000_000_000, 0.5) - 0.5).abs() < 1e-9);
        // Steady stamps keep their spacing even if arrival jitters.
        assert!((pin.place(1_000_333_333, 0.56) - 0.533_333_3).abs() < 1e-6);
        // The camera clock restarted: tie again.
        assert!((pin.place(10_000, 2.0) - 2.0).abs() < 1e-9);
    }
}
