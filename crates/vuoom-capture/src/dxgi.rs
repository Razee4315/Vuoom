//! Display capture through DXGI Desktop Duplication.
//!
//! Preferred over Windows Graphics Capture for displays because it honors
//! `WDA_EXCLUDEFROMCAPTURE` on Windows 10 as well as 11: Vuoom's recording panel can sit over
//! the recorded area and still stay out of the frames. (On Windows 10, WGC records such a
//! window as a black box, while Duplication shows what's behind it; both verified on
//! hardware.) Duplication doesn't draw the pointer, so when a take keeps the real pointer it
//! is drawn here from the shape Windows reports ([`crate::pointer`]).
//!
//! Frames arrive only when the screen changes, as with WGC. Each change is copied on the GPU
//! into one staging texture kept for the whole take, and the duplication's frame goes straight
//! back to Windows. When the frame-rate cap allows the next frame, only the rows that changed
//! since the last one (from Duplication's dirty and move rectangles) are read back to the CPU,
//! so typing or a moving pointer costs a sliver of the screen, not all of it. Each frame out
//! says which rows those were ([`CapturedFrame::dirty`]), so the frame store compresses only
//! that sliver too, and is filled into a buffer the consumer handed back rather than a fresh
//! allocation.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, TrySendError};
use std::time::Duration;

use vuoom_input::Clock;
use windows::Win32::Foundation::RECT;
use windows::Win32::Graphics::Direct3D11 as d3d;
use windows::Win32::Graphics::Dxgi::Common::{
    DXGI_MODE_ROTATION_IDENTITY, DXGI_MODE_ROTATION_UNSPECIFIED,
};
use windows::Win32::Graphics::Dxgi::{
    IDXGIOutputDuplication, DXGI_OUTDUPL_FRAME_INFO, DXGI_OUTDUPL_MOVE_RECT,
    DXGI_OUTDUPL_POINTER_SHAPE_INFO,
};
use windows_capture::dxgi_duplication_api::{
    DxgiDuplicationApi, DxgiDuplicationFrame, Error as DupError,
};
use windows_capture::monitor::Monitor;

use crate::capture::{clamp_region, CaptureOptions, CapturedFrame, CropRegion, Output};
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

/// The CPU-readable copy of the recorded area: a staging texture the size of the crop, kept
/// for the whole capture. Each new desktop image copies its changed rows into it on the GPU;
/// the CPU reads back only those rows, and only when the next frame is due.
struct Readback {
    device: d3d::ID3D11Device,
    context: d3d::ID3D11DeviceContext,
    texture: d3d::ID3D11Texture2D,
    width: u32,
    height: u32,
}

impl Readback {
    fn new(frame: &DxgiDuplicationFrame<'_>, width: u32, height: u32) -> Result<Self, String> {
        let desc = d3d::D3D11_TEXTURE2D_DESC {
            Width: width,
            Height: height,
            MipLevels: 1,
            ArraySize: 1,
            Usage: d3d::D3D11_USAGE_STAGING,
            BindFlags: 0,
            CPUAccessFlags: d3d::D3D11_CPU_ACCESS_READ.0 as u32,
            MiscFlags: 0,
            ..*frame.texture_desc()
        };
        let device = frame.device().clone();
        let mut texture = None;
        // SAFETY: a plain texture description; the out-pointer lives for the call.
        unsafe { device.CreateTexture2D(&desc, None, Some(&mut texture)) }
            .map_err(|e| e.to_string())?;
        let texture = texture.ok_or("no staging texture")?;
        Ok(Self {
            context: frame.device_context().clone(),
            device,
            texture,
            width,
            height,
        })
    }

    /// Whether this readback fits a crop of `width`×`height` on `frame`'s device.
    fn fits(&self, frame: &DxgiDuplicationFrame<'_>, width: u32, height: u32) -> bool {
        self.width == width && self.height == height && self.device == *frame.device()
    }

    /// GPU copy of rows `rows` of the crop at (`x0`, `y0`) of `frame` into the readback.
    fn copy(&self, frame: &DxgiDuplicationFrame<'_>, x0: u32, y0: u32, rows: (u32, u32)) {
        let src = d3d::D3D11_BOX {
            left: x0,
            top: y0 + rows.0,
            front: 0,
            right: x0 + self.width,
            bottom: y0 + rows.1,
            back: 1,
        };
        let (dst, tex) = (&self.texture, frame.texture());
        // SAFETY: both textures live for the call and the box lies inside the source frame.
        unsafe {
            self.context
                .CopySubresourceRegion(dst, 0, 0, rows.0, 0, tex, 0, Some(&src));
        }
    }

    /// Read rows `rows` into `out` (a tightly packed BGRA image of the crop's size).
    fn read(&self, rows: (u32, u32), out: &mut [u8]) -> Result<(), String> {
        let mut mapped = d3d::D3D11_MAPPED_SUBRESOURCE::default();
        // SAFETY: the staging texture was created CPU-readable; it is unmapped below.
        unsafe {
            self.context
                .Map(&self.texture, 0, d3d::D3D11_MAP_READ, 0, Some(&mut mapped))
        }
        .map_err(|e| e.to_string())?;
        let row = self.width as usize * 4;
        let pitch = mapped.RowPitch as usize;
        for y in rows.0 as usize..rows.1 as usize {
            // SAFETY: the mapping spans `height` rows of `RowPitch` bytes, each holding at
            // least `row` bytes of pixels, and `y` is below `height`.
            let src = unsafe {
                std::slice::from_raw_parts(mapped.pData.cast::<u8>().add(y * pitch), row)
            };
            out[y * row..(y + 1) * row].copy_from_slice(src);
        }
        // SAFETY: mapped above.
        unsafe { self.context.Unmap(&self.texture, 0) };
        Ok(())
    }
}

/// The rows (top, bottom) of the crop at (`x0`, `y0`, `w`, `h`) this frame changed, from the
/// dirty and move rectangles Duplication reports. `None` when nothing inside the crop
/// changed; all rows when the rectangles can't be read.
fn changed_rows(
    dup: &IDXGIOutputDuplication,
    info: &DXGI_OUTDUPL_FRAME_INFO,
    (x0, y0, w, h): (u32, u32, u32, u32),
) -> Option<(u32, u32)> {
    let all = Some((0, h));
    let size = info.TotalMetadataBufferSize;
    if size == 0 {
        return all;
    }
    let mut spans: Vec<RECT> = Vec::new();
    let moves_len = size as usize / std::mem::size_of::<DXGI_OUTDUPL_MOVE_RECT>() + 1;
    let mut moves = vec![DXGI_OUTDUPL_MOVE_RECT::default(); moves_len];
    let mut used = 0u32;
    let bytes = (moves.len() * std::mem::size_of::<DXGI_OUTDUPL_MOVE_RECT>()) as u32;
    // SAFETY: the buffer holds `bytes` bytes; Duplication writes at most that many.
    if unsafe { dup.GetFrameMoveRects(bytes, moves.as_mut_ptr(), &mut used) }.is_err() {
        return all;
    }
    let n = used as usize / std::mem::size_of::<DXGI_OUTDUPL_MOVE_RECT>();
    spans.extend(moves.iter().take(n).map(|m| m.DestinationRect));
    let dirty_len = size as usize / std::mem::size_of::<RECT>() + 1;
    let mut dirty = vec![RECT::default(); dirty_len];
    let bytes = (dirty.len() * std::mem::size_of::<RECT>()) as u32;
    // SAFETY: as above.
    if unsafe { dup.GetFrameDirtyRects(bytes, dirty.as_mut_ptr(), &mut used) }.is_err() {
        return all;
    }
    let n = used as usize / std::mem::size_of::<RECT>();
    spans.extend(dirty.iter().take(n));
    let (left, right) = (x0 as i32, (x0 + w) as i32);
    let (top, bottom) = (y0 as i32, (y0 + h) as i32);
    let mut band: Option<(u32, u32)> = None;
    for r in spans {
        if r.right <= left || r.left >= right || r.bottom <= top || r.top >= bottom {
            continue;
        }
        let a = (r.top.max(top) - top) as u32;
        let b = (r.bottom.min(bottom) - top) as u32;
        band = Some(union(band, (a, b)));
    }
    band
}

/// Two row spans as one (the rows of both).
fn union(a: Option<(u32, u32)>, b: (u32, u32)) -> (u32, u32) {
    a.map_or(b, |(p, q)| (p.min(b.0), q.max(b.1)))
}

/// Two optional row spans as one.
fn union_opt(a: Option<(u32, u32)>, b: Option<(u32, u32)>) -> Option<(u32, u32)> {
    match b {
        Some(b) => Some(union(a, b)),
        None => a,
    }
}

/// The rows of a `height`-row frame a pointer image `rows` tall covers with its top at
/// row `y` (which may be off the frame). `None` when none of it is on the frame.
fn pointer_span(y: i32, rows: u32, height: u32) -> Option<(u32, u32)> {
    let top = i64::from(y).clamp(0, i64::from(height));
    let bottom = (i64::from(y) + i64::from(rows)).clamp(0, i64::from(height));
    (bottom > top).then_some((top as u32, bottom as u32))
}

/// What the next frame out owes for frames that were made but never delivered (the
/// consumer's queue was full): the consumer compares each frame with the last one it GOT,
/// so the rows those lost frames changed have to be reported again.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum Owed {
    #[default]
    Nothing,
    Rows((u32, u32)),
    /// A lost frame couldn't say what it changed: the next one can't either.
    Everything,
}

impl Owed {
    /// Add a lost frame's changed rows.
    fn add(self, lost: Option<(u32, u32)>) -> Self {
        match (self, lost) {
            (Self::Everything, _) | (_, None) => Self::Everything,
            (Self::Nothing, Some(rows)) => Self::Rows(rows),
            (Self::Rows(owed), Some(rows)) => Self::Rows(union(Some(owed), rows)),
        }
    }

    /// The rows a frame that changed `rows` must report, owing this.
    fn settle(self, rows: Option<(u32, u32)>) -> Option<(u32, u32)> {
        match self {
            Self::Nothing => Some(rows.unwrap_or((0, 0))),
            Self::Rows(owed) => Some(union(rows, owed)),
            Self::Everything => None,
        }
    }
}

/// A running capture's state between frames.
///
/// Every acquired desktop image is copied into [`Readback`] on the GPU right away (cheap) and
/// the duplication frame goes back to Windows at the next acquire, so Windows never waits on
/// us. A frame goes out when the frame-rate cap allows: only then are the rows that changed
/// since the last one read back to the CPU, into a clean copy of the recorded area that the
/// pointer is drawn onto.
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
    readback: Option<Readback>,
    /// The recorded area as of the last frame out, tightly packed BGRA, without the pointer.
    clean: Vec<u8>,
    /// Rows copied into `readback` but not yet read into `clean`.
    rows: Option<(u32, u32)>,
    /// Whether the pointer moved (or changed) since the last frame out.
    pointer_moved: bool,
    /// When the newest image not yet sent was presented (0 = none).
    presented: i64,
    /// The crop origin in the desktop image.
    origin: (u32, u32),
    /// The rows the pointer was drawn over in the last frame out: the next one shows the
    /// screen there again, so they changed even if the screen didn't.
    pointer_rows: Option<(u32, u32)>,
    /// Changed rows of frames the consumer never got.
    owed: Owed,
    /// Whether a frame has gone out yet (the first can't say what changed).
    sent_any: bool,
    /// A buffer to fill the next frame into, when one is at hand.
    spare: Option<Vec<u8>>,
}

impl Pacer {
    /// Wait for the next change or until a pending frame is due, and send one when it is.
    fn next(&mut self, api: &mut DxgiDuplicationApi, recycle: &Receiver<Vec<u8>>) -> Outcome {
        let wait = self.wait_ms();
        match api.acquire_next_frame(wait) {
            Ok(frame) => {
                if let Err(e) = self.take(&frame) {
                    return Outcome::Failed(e);
                }
            }
            Err(DupError::Timeout) => {}
            Err(DupError::AccessLost) => return Outcome::Lost,
            Err(e) => return Outcome::Failed(e.to_string()),
        }
        if self.pending() && self.due() {
            return self.emit(recycle);
        }
        Outcome::Nothing
    }

    /// A frame that was made couldn't be delivered: remember what it changed for the next
    /// one to report, and keep its buffer to fill again.
    fn undelivered(&mut self, frame: CapturedFrame) {
        self.owed = self.owed.add(frame.dirty);
        self.spare = Some(frame.bgra);
    }

    /// Whether something changed since the last frame out.
    fn pending(&self) -> bool {
        self.seen_image && (self.rows.is_some() || self.pointer_moved || self.last_sent.is_none())
    }

    /// Whether the frame-rate cap allows a frame now.
    fn due(&self) -> bool {
        let Some(last) = self.last_sent else {
            return true;
        };
        self.clock.seconds_between(last, self.clock.now()) >= self.interval
    }

    /// How long the next acquire may wait: until a pending frame is due, else the usual.
    fn wait_ms(&self) -> u32 {
        let Some(last) = self.last_sent.filter(|_| self.pending()) else {
            return ACQUIRE_MS;
        };
        let left = self.interval - self.clock.seconds_between(last, self.clock.now());
        ((left * 1000.0).ceil().max(0.0) as u32).min(ACQUIRE_MS)
    }

    /// Take in one acquired frame: the pointer news, and the changed rows on the GPU.
    fn take(&mut self, frame: &DxgiDuplicationFrame<'_>) -> Result<(), String> {
        let info = *frame.frame_info();
        if self.cursor {
            self.pointer.update(frame.duplication(), &info);
            self.pointer_moved |= info.LastMouseUpdateTime != 0;
        }
        if info.LastPresentTime == 0 {
            return Ok(());
        }
        self.seen_image = true;
        self.presented = info.LastPresentTime;
        let (w, h) = (frame.width(), frame.height());
        let (x0, y0, cw, ch) = match self.crop {
            Some(r) => clamp_region(r, w, h),
            None => (0, 0, w, h),
        };
        let fits = |r: &Readback| r.fits(frame, cw, ch);
        let fresh = !self.readback.as_ref().is_some_and(fits);
        if fresh {
            self.readback = Some(Readback::new(frame, cw, ch)?);
            self.clean = vec![0; cw as usize * ch as usize * 4];
            self.rows = None;
            // A new size: the next frame is new all over, whatever it reports.
            self.pointer_rows = None;
            self.owed = Owed::Everything;
        }
        let moved = self.origin != (x0, y0);
        self.origin = (x0, y0);
        let rows = if fresh || moved {
            Some((0, ch))
        } else {
            changed_rows(frame.duplication(), &info, (x0, y0, cw, ch))
        };
        if let (Some(rows), Some(rb)) = (rows, self.readback.as_ref()) {
            rb.copy(frame, x0, y0, rows);
            self.rows = Some(union(self.rows, rows));
        }
        Ok(())
    }

    /// Send the recorded area as it is now, with the pointer drawn when the take keeps it.
    fn emit(&mut self, recycle: &Receiver<Vec<u8>>) -> Outcome {
        let Some(rb) = self.readback.as_ref() else {
            return Outcome::Nothing;
        };
        let screen_rows = self.rows;
        if let Some(rows) = screen_rows {
            if let Err(e) = rb.read(rows, &mut self.clean) {
                return Outcome::Failed(e);
            }
        }
        self.rows = None;
        let (cw, ch) = (rb.width, rb.height);
        // Into a buffer the consumer is done with, when there is one: it already has its
        // memory, where a fresh one this size has every page zeroed by the OS on first touch.
        let recycled = self.spare.take().or_else(|| recycle.try_recv().ok());
        let mut bgra = recycled.unwrap_or_default();
        bgra.clear();
        bgra.extend_from_slice(&self.clean);
        let drawn = self.cursor && self.pointer.visible;
        let mut pointer_rows = None;
        if let Some(shape) = self.pointer.shape.as_ref().filter(|_| drawn) {
            let (x0, y0) = self.origin;
            let (px, py) = (self.pointer.x - x0 as i32, self.pointer.y - y0 as i32);
            pointer::draw(&mut bgra, cw, ch, px, py, shape);
            pointer_rows = pointer_span(py, shape.rows(), ch);
        }
        // What differs from the last frame out: the rows the screen changed, the rows the
        // pointer was over then (they show the screen again) and the rows it is over now.
        let was = std::mem::replace(&mut self.pointer_rows, pointer_rows);
        let changed = union_opt(union_opt(screen_rows, was), pointer_rows);
        let owed = std::mem::take(&mut self.owed);
        let dirty = if self.sent_any {
            owed.settle(changed)
        } else {
            None
        };
        self.sent_any = true;
        let now = self.clock.now();
        // The present time is read from the same performance counter; trust it only when
        // it's plausibly recent and after the last frame out.
        let after_last = self.last_sent.is_none_or(|t| self.presented > t);
        let recent = (self.presented - now).abs() < self.clock.freq();
        let qpc = if self.presented != 0 && recent && after_last {
            self.presented
        } else {
            now
        };
        self.presented = 0;
        self.pointer_moved = false;
        self.last_sent = Some(now.max(qpc));
        Outcome::Frame(CapturedFrame {
            width: cw,
            height: ch,
            bgra,
            qpc,
            dirty,
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
pub(crate) fn run(
    monitor: Monitor,
    out: &Output,
    stop: &AtomicBool,
    crop: Option<CropRegion>,
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
        readback: None,
        clean: Vec::new(),
        rows: None,
        pointer_moved: false,
        presented: 0,
        origin: (0, 0),
        pointer_rows: None,
        owed: Owed::Nothing,
        sent_any: false,
        spare: None,
    };
    let mut failures = 0u64;
    while !stop.load(Ordering::Relaxed) {
        let Some(api) = dup.as_mut() else {
            std::thread::sleep(REOPEN_WAIT);
            dup = open(monitor).ok();
            continue;
        };
        match pacer.next(api, &out.recycle) {
            Outcome::Frame(frame) => match out.tx.try_send(frame) {
                Ok(()) => {}
                Err(TrySendError::Full(frame)) => {
                    pacer.undelivered(frame);
                    let n = out.dropped.fetch_add(1, Ordering::Relaxed) + 1;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_pointer_span_is_clipped_to_the_frame() {
        assert_eq!(pointer_span(10, 32, 100), Some((10, 42)));
        // Hanging off the top and the bottom.
        assert_eq!(pointer_span(-8, 32, 100), Some((0, 24)));
        assert_eq!(pointer_span(90, 32, 100), Some((90, 100)));
        // Off the frame altogether.
        assert_eq!(pointer_span(-40, 32, 100), None);
        assert_eq!(pointer_span(100, 32, 100), None);
    }

    #[test]
    fn a_frame_reports_its_rows_and_what_lost_frames_changed() {
        // Nothing owed: just this frame's rows, or an empty span when nothing changed.
        assert_eq!(Owed::Nothing.settle(Some((5, 9))), Some((5, 9)));
        assert_eq!(Owed::Nothing.settle(None), Some((0, 0)));
        // Two frames were lost: their rows ride along with the next frame's.
        let owed = Owed::Nothing.add(Some((40, 50))).add(Some((2, 4)));
        assert_eq!(owed, Owed::Rows((2, 50)));
        assert_eq!(owed.settle(Some((60, 70))), Some((2, 70)));
        assert_eq!(owed.settle(None), Some((2, 50)));
        // A lost frame that couldn't say what changed: the next can't either, however
        // many more are lost after it.
        let owed = owed.add(None).add(Some((1, 2)));
        assert_eq!(owed, Owed::Everything);
        assert_eq!(owed.settle(Some((5, 9))), None);
    }

    #[test]
    fn row_spans_merge() {
        assert_eq!(union(None, (3, 7)), (3, 7));
        assert_eq!(union(Some((10, 20)), (3, 7)), (3, 20));
        assert_eq!(union_opt(None, None), None);
        assert_eq!(union_opt(Some((1, 2)), None), Some((1, 2)));
        assert_eq!(union_opt(Some((1, 2)), Some((8, 9))), Some((1, 9)));
    }
}
