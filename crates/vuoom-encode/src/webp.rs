//! Animated WebP encoding backed by Google `libwebp` (`libwebp-sys`).
//!
//! Produces significantly smaller, full-color (no 255-color palette or dithering bands)
//! animated WebP files than GIF while streaming composited frames one at a time into the
//! WebP animation encoder so memory usage stays bounded on long clips.

use crate::error::EncodeError;
use crate::image::RgbaImage;
use crate::native::downscale_rgba;
use crate::settings::WebpSettings;
use crate::StreamFrame;
use std::ffi::CStr;
use std::fs::File;
use std::io::Write;
use std::mem::MaybeUninit;
use std::path::Path;
use std::ptr;

/// Maximum canvas side supported by the WebP bitstream (14-bit width/height minus 1).
const WEBP_MAX_DIM: u32 = 16_383;

/// RAII wrapper around `libwebp_sys::WebPAnimEncoder`.
struct OwnedAnimEncoder(*mut libwebp_sys::WebPAnimEncoder);

impl OwnedAnimEncoder {
    fn new(
        w: u32,
        h: u32,
        opts: &libwebp_sys::WebPAnimEncoderOptions,
    ) -> Result<Self, EncodeError> {
        let (wi, hi) = (
            i32::try_from(w).map_err(|_| EncodeError::Webp("width overflows i32".into()))?,
            i32::try_from(h).map_err(|_| EncodeError::Webp("height overflows i32".into()))?,
        );
        // SAFETY: `opts` was initialized with `WebPAnimEncoderOptionsInitInternal`.
        let ptr = unsafe {
            libwebp_sys::WebPAnimEncoderNewInternal(
                wi,
                hi,
                opts,
                libwebp_sys::WEBP_MUX_ABI_VERSION as i32,
            )
        };
        if ptr.is_null() {
            return Err(EncodeError::Webp(
                "could not initialize WebP animation encoder".into(),
            ));
        }
        Ok(Self(ptr))
    }

    fn last_error(&self) -> String {
        // SAFETY: `self.0` is a live `WebPAnimEncoder`.
        let msg = unsafe { libwebp_sys::WebPAnimEncoderGetError(self.0) };
        if msg.is_null() {
            return "unknown libwebp error".into();
        }
        // SAFETY: `WebPAnimEncoderGetError` returns a null-terminated static/internal string.
        unsafe { CStr::from_ptr(msg) }
            .to_string_lossy()
            .into_owned()
    }

    fn add_frame(
        &mut self,
        pic: &mut OwnedPicture,
        timestamp_ms: i32,
        config: &libwebp_sys::WebPConfig,
    ) -> Result<(), EncodeError> {
        // SAFETY: `self.0` and `pic.0` are initialized; `config` was validated.
        let ok =
            unsafe { libwebp_sys::WebPAnimEncoderAdd(self.0, &mut pic.0, timestamp_ms, config) };
        if ok == 0 {
            return Err(EncodeError::Webp(self.last_error()));
        }
        Ok(())
    }

    fn assemble(self, end_timestamp_ms: i32) -> Result<Vec<u8>, EncodeError> {
        // Flush the final frame duration by passing a null picture at the end timestamp.
        // SAFETY: `self.0` is live; passing null `frame` and null `config` terminates the stream.
        let ok = unsafe {
            libwebp_sys::WebPAnimEncoderAdd(self.0, ptr::null_mut(), end_timestamp_ms, ptr::null())
        };
        if ok == 0 {
            return Err(EncodeError::Webp(self.last_error()));
        }

        let mut data = OwnedWebPData::new();
        // SAFETY: `self.0` is live and `data.0` is initialized empty.
        let ok = unsafe { libwebp_sys::WebPAnimEncoderAssemble(self.0, &mut data.0) };
        if ok == 0 {
            return Err(EncodeError::Webp(self.last_error()));
        }
        Ok(data.as_slice().to_vec())
    }
}

impl Drop for OwnedAnimEncoder {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: `self.0` was allocated by `WebPAnimEncoderNewInternal`.
            unsafe { libwebp_sys::WebPAnimEncoderDelete(self.0) };
            self.0 = ptr::null_mut();
        }
    }
}

/// RAII wrapper around `libwebp_sys::WebPPicture`.
struct OwnedPicture(libwebp_sys::WebPPicture);

impl OwnedPicture {
    fn from_rgba(img: &RgbaImage) -> Result<Self, EncodeError> {
        if !img.is_valid() {
            return Err(EncodeError::Webp(format!(
                "pixel buffer {} != {}x{}x4",
                img.pixels.len(),
                img.width,
                img.height
            )));
        }
        let mut pic = MaybeUninit::<libwebp_sys::WebPPicture>::uninit();
        // SAFETY: must pass `WEBP_ENCODER_ABI_VERSION` (do not use `WebPPicture::new()`, which
        // passes `WEBP_DECODER_ABI_VERSION` in `libwebp-sys` 0.14).
        let ok = unsafe {
            libwebp_sys::WebPPictureInitInternal(
                pic.as_mut_ptr(),
                libwebp_sys::WEBP_ENCODER_ABI_VERSION as i32,
            )
        };
        if ok == 0 {
            return Err(EncodeError::Webp("WebPPictureInit failed".into()));
        }
        // SAFETY: `WebPPictureInitInternal` succeeded and initialized the struct.
        let mut owned = Self(unsafe { pic.assume_init() });
        owned.0.use_argb = 1;
        owned.0.width = i32::try_from(img.width)
            .map_err(|_| EncodeError::Webp("width overflows i32".into()))?;
        owned.0.height = i32::try_from(img.height)
            .map_err(|_| EncodeError::Webp("height overflows i32".into()))?;
        let stride = i32::try_from(img.width.saturating_mul(4))
            .map_err(|_| EncodeError::Webp("stride overflows i32".into()))?;
        // SAFETY: `img.is_valid()` verified `pixels.len() == width * height * 4`.
        let imported = unsafe {
            libwebp_sys::WebPPictureImportRGBA(&mut owned.0, img.pixels.as_ptr(), stride)
        };
        if imported == 0 {
            return Err(EncodeError::Webp("WebPPictureImportRGBA failed".into()));
        }
        Ok(owned)
    }
}

impl Drop for OwnedPicture {
    fn drop(&mut self) {
        // SAFETY: `self.0` was initialized by `WebPPictureInitInternal`.
        unsafe { libwebp_sys::WebPPictureFree(&mut self.0) };
    }
}

/// RAII wrapper around `libwebp_sys::WebPData`.
struct OwnedWebPData(libwebp_sys::WebPData);

impl OwnedWebPData {
    fn new() -> Self {
        let mut data = libwebp_sys::WebPData::default();
        libwebp_sys::WebPDataInit(&mut data);
        Self(data)
    }

    fn as_slice(&self) -> &[u8] {
        if self.0.bytes.is_null() || self.0.size == 0 {
            &[]
        } else {
            // SAFETY: `libwebp` guarantees `bytes` points to `size` readable bytes.
            unsafe { std::slice::from_raw_parts(self.0.bytes, self.0.size) }
        }
    }
}

impl Drop for OwnedWebPData {
    fn drop(&mut self) {
        // SAFETY: `self.0` was initialized with `WebPDataInit` and populated by libwebp.
        unsafe { libwebp_sys::WebPDataClear(&mut self.0) };
    }
}

fn build_options() -> Result<libwebp_sys::WebPAnimEncoderOptions, EncodeError> {
    let mut opts = MaybeUninit::<libwebp_sys::WebPAnimEncoderOptions>::uninit();
    // SAFETY: initializes `opts` using the matching `WEBP_MUX_ABI_VERSION`.
    let ok = unsafe {
        libwebp_sys::WebPAnimEncoderOptionsInitInternal(
            opts.as_mut_ptr(),
            libwebp_sys::WEBP_MUX_ABI_VERSION as i32,
        )
    };
    if ok == 0 {
        return Err(EncodeError::Webp(
            "WebPAnimEncoderOptionsInit failed".into(),
        ));
    }
    // SAFETY: `WebPAnimEncoderOptionsInitInternal` initialized the struct.
    let mut opts = unsafe { opts.assume_init() };
    opts.anim_params.loop_count = 0; // loop infinitely like Vuoom GIFs
    opts.allow_mixed = 0;
    opts.verbose = 0;
    Ok(opts)
}

fn build_config(settings: &WebpSettings) -> Result<libwebp_sys::WebPConfig, EncodeError> {
    let quality = f32::from(settings.quality.min(100));
    let mut cfg = MaybeUninit::<libwebp_sys::WebPConfig>::uninit();
    // SAFETY: must pass `WEBP_ENCODER_ABI_VERSION` (do not use `WebPConfig::new()`).
    let ok = unsafe {
        libwebp_sys::WebPConfigInitInternal(
            cfg.as_mut_ptr(),
            libwebp_sys::WebPPreset::WEBP_PRESET_DEFAULT,
            quality,
            libwebp_sys::WEBP_ENCODER_ABI_VERSION as i32,
        )
    };
    if ok == 0 {
        return Err(EncodeError::Webp("WebPConfigInit failed".into()));
    }
    // SAFETY: `WebPConfigInitInternal` initialized the struct.
    let mut cfg = unsafe { cfg.assume_init() };
    // Clamp VP8L lossless method <= 2 so lossless exports stay fast (method 4+ can take
    // seconds per 1080p frame on low-end CPUs).
    let method = if settings.lossless {
        settings.method.min(2)
    } else {
        settings.method.min(6)
    };
    cfg.lossless = i32::from(settings.lossless);
    cfg.quality = quality;
    cfg.method = i32::from(method);
    cfg.image_hint = libwebp_sys::WebPImageHint::WEBP_HINT_GRAPH;
    cfg.thread_level = 1;
    // Iterative sharp YUV adds a 4-8x cost on every frame; only enable on slow HQ presets.
    cfg.use_sharp_yuv = i32::from(!settings.lossless && method >= 5 && settings.quality >= 90);
    // SAFETY: `cfg` is initialized.
    if unsafe { libwebp_sys::WebPValidateConfig(&cfg) } == 0 {
        return Err(EncodeError::Webp("invalid WebP encoder config".into()));
    }
    Ok(cfg)
}

fn validate_dims(w: u32, h: u32) -> Result<(), EncodeError> {
    if w == 0 || h == 0 {
        return Err(EncodeError::Webp("zero-size frame".into()));
    }
    if w > WEBP_MAX_DIM || h > WEBP_MAX_DIM {
        return Err(EncodeError::Webp(format!(
            "frame dimensions {w}x{h} exceed WebP {WEBP_MAX_DIM}px limit"
        )));
    }
    Ok(())
}

/// Millisecond timestamp for output frame `index` at `fps`, strictly advancing past `prev_ms`
/// when `index > 0` so libwebp never rejects two frames with the same millisecond timestamp.
fn frame_timestamp_ms(index: usize, fps: u32, prev_ms: i32) -> i32 {
    let fps = u64::from(fps.max(1));
    let raw = ((index as u64 * 1000) + fps / 2) / fps;
    let ts = i32::try_from(raw).unwrap_or(i32::MAX);
    if index > 0 && ts <= prev_ms {
        prev_ms.saturating_add(1)
    } else {
        ts
    }
}

/// Encode RGBA `frames` into an animated WebP byte vector.
///
/// Honors `settings.fps`, `settings.width` (downscale cap), `settings.quality`,
/// `settings.lossless`, and `settings.method`. Loops infinitely.
///
/// # Errors
/// Returns [`EncodeError`] if there are no frames, dimensions are invalid/mismatched, or
/// libwebp rejects the encode.
pub fn encode_webp_to_vec(
    frames: &[RgbaImage],
    settings: &WebpSettings,
) -> Result<Vec<u8>, EncodeError> {
    if frames.is_empty() {
        return Err(EncodeError::NoFrames);
    }
    encode_webp_stream_to_vec(
        frames.len(),
        |i| Ok(StreamFrame::Image(frames[i].clone())),
        settings,
        &|_, _| {},
    )
}

/// Encode RGBA `frames` into an animated WebP file at `out`.
///
/// # Errors
/// Returns [`EncodeError`] on invalid inputs, encoding error, or I/O failure.
pub fn export_webp(
    frames: &[RgbaImage],
    settings: &WebpSettings,
    out: &Path,
) -> Result<(), EncodeError> {
    let bytes = encode_webp_to_vec(frames, settings)?;
    let mut file = File::create(out)?;
    file.write_all(&bytes)?;
    Ok(())
}

/// Streaming variant of [`export_webp`] for full recordings. Pulls frames one at a time from
/// `frame(i)` so full-resolution RGBA frames never accumulate in RAM.
///
/// `progress(done, total)` ticks once per frame and once after the final WebP container is
/// assembled and written.
///
/// # Errors
/// Returns [`EncodeError`] for zero frames, a `frame(i)` error, mismatched/zero-size frame
/// dimensions, or a write/encode failure.
pub fn export_webp_streaming<F>(
    count: usize,
    mut frame: F,
    settings: &WebpSettings,
    out: &Path,
    progress: &dyn Fn(usize, usize),
) -> Result<(), EncodeError>
where
    F: FnMut(usize) -> Result<RgbaImage, String>,
{
    export_webp_stream_dedup(
        count,
        |i| frame(i).map(StreamFrame::Image),
        settings,
        out,
        progress,
    )
}

/// Deduplicating streaming WebP encoder: frame 0 must yield [`StreamFrame::Image`], while any
/// later [`StreamFrame::Repeat`] extends the previous frame's timestamp without importing or
/// encoding duplicate pixels through `libwebp`.
///
/// # Errors
/// Returns [`EncodeError`] for zero frames, callback errors, mismatched/zero-size dimensions,
/// or a write/encode failure.
pub fn export_webp_stream_dedup<F>(
    count: usize,
    frame: F,
    settings: &WebpSettings,
    out: &Path,
    progress: &dyn Fn(usize, usize),
) -> Result<(), EncodeError>
where
    F: FnMut(usize) -> Result<StreamFrame, String>,
{
    let bytes = encode_webp_stream_to_vec(count, frame, settings, progress)?;
    let mut file = File::create(out)?;
    file.write_all(&bytes)?;
    progress(count + 1, count + 1);
    Ok(())
}

fn encode_webp_stream_to_vec<F>(
    count: usize,
    mut frame: F,
    settings: &WebpSettings,
    progress: &dyn Fn(usize, usize),
) -> Result<Vec<u8>, EncodeError>
where
    F: FnMut(usize) -> Result<StreamFrame, String>,
{
    if count == 0 {
        return Err(EncodeError::NoFrames);
    }

    let scale = |img: RgbaImage| match settings.width {
        Some(cap) => downscale_rgba(&img, cap),
        None => img,
    };

    let opts = build_options()?;
    let cfg = build_config(settings)?;
    let total_work = count + 1;

    let first = match frame(0).map_err(EncodeError::Webp)? {
        StreamFrame::Image(img) => scale(img),
        StreamFrame::Repeat => {
            return Err(EncodeError::Webp("first frame cannot be repeat".into()))
        }
    };
    let (w, h) = (first.width, first.height);
    validate_dims(w, h)?;

    let mut encoder = OwnedAnimEncoder::new(w, h, &opts)?;
    let mut prev_ts = frame_timestamp_ms(0, settings.fps, 0);
    let mut pic = OwnedPicture::from_rgba(&first)?;
    drop(first);
    encoder.add_frame(&mut pic, prev_ts, &cfg)?;
    drop(pic);
    progress(1, total_work);

    for i in 1..count {
        match frame(i).map_err(EncodeError::Webp)? {
            StreamFrame::Repeat => {
                // Skip encoding completely: the next distinct frame's timestamp (or final
                // flush `end_ts`) extends this held frame's duration automatically.
            }
            StreamFrame::Image(raw) => {
                let img = scale(raw);
                if img.width != w || img.height != h {
                    return Err(EncodeError::Webp("frame dimensions differ".into()));
                }
                let ts = frame_timestamp_ms(i, settings.fps, prev_ts);
                let mut pic = OwnedPicture::from_rgba(&img)?;
                drop(img);
                encoder.add_frame(&mut pic, ts, &cfg)?;
                prev_ts = ts;
            }
        }
        progress(i + 1, total_work);
    }

    // Ensure end_ts honors all trailing Repeat frames (at full `count` timeline end).
    let raw_end = ((count as u64 * 1000) + u64::from(settings.fps.max(1)) / 2)
        / u64::from(settings.fps.max(1));
    let min_end = prev_ts.saturating_add(1);
    let end_ts = i32::try_from(raw_end).unwrap_or(i32::MAX).max(min_end);
    encoder.assemble(end_ts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::native::export_gif_native;
    use crate::settings::GifSettings;

    fn solid(w: u32, h: u32, rgba: [u8; 4]) -> RgbaImage {
        let mut px = Vec::with_capacity((w * h * 4) as usize);
        for _ in 0..(w * h) {
            px.extend_from_slice(&rgba);
        }
        RgbaImage::new(w, h, px)
    }

    /// Decode an animated WebP back with libwebp's `WebPAnimDecoder` into
    /// `(info, per_frame_timestamps_ms)`.
    fn decode_webp(bytes: &[u8]) -> (libwebp_sys::WebPAnimInfo, Vec<i32>) {
        unsafe {
            let mut dec_opts = MaybeUninit::<libwebp_sys::WebPAnimDecoderOptions>::uninit();
            assert_ne!(
                libwebp_sys::WebPAnimDecoderOptionsInit(dec_opts.as_mut_ptr()),
                0
            );
            let dec_opts = dec_opts.assume_init();
            let data = libwebp_sys::WebPData {
                bytes: bytes.as_ptr(),
                size: bytes.len(),
            };
            let dec = libwebp_sys::WebPAnimDecoderNew(&data, &dec_opts);
            assert!(!dec.is_null(), "WebPAnimDecoderNew failed");
            let mut info = libwebp_sys::WebPAnimInfo::default();
            assert_ne!(libwebp_sys::WebPAnimDecoderGetInfo(dec, &mut info), 0);

            let mut timestamps = Vec::new();
            while libwebp_sys::WebPAnimDecoderHasMoreFrames(dec) > 0 {
                let mut buf = ptr::null_mut();
                let mut ts = 0;
                assert_ne!(
                    libwebp_sys::WebPAnimDecoderGetNext(dec, &mut buf, &mut ts),
                    0
                );
                timestamps.push(ts);
            }
            libwebp_sys::WebPAnimDecoderDelete(dec);
            (info, timestamps)
        }
    }

    #[test]
    fn no_frames_is_an_error() {
        let err = encode_webp_to_vec(&[], &WebpSettings::balanced());
        assert!(matches!(err, Err(EncodeError::NoFrames)));

        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("empty.webp");
        let err = export_webp_streaming(
            0,
            |_| Ok(solid(4, 4, [0, 0, 0, 255])),
            &WebpSettings::balanced(),
            &out,
            &|_, _| {},
        );
        assert!(matches!(err, Err(EncodeError::NoFrames)));
    }

    #[test]
    fn zero_size_and_mismatched_dimensions_are_errors() {
        let zero = vec![solid(0, 0, [0, 0, 0, 255])];
        assert!(matches!(
            encode_webp_to_vec(&zero, &WebpSettings::balanced()),
            Err(EncodeError::Webp(_))
        ));

        let mismatch = vec![solid(8, 8, [255, 0, 0, 255]), solid(4, 4, [0, 255, 0, 255])];
        assert!(matches!(
            encode_webp_to_vec(&mismatch, &WebpSettings::balanced()),
            Err(EncodeError::Webp(_))
        ));
    }

    #[test]
    fn writes_valid_animated_webp_and_decodes_back() {
        let frames = vec![
            solid(16, 16, [240, 40, 40, 255]),
            solid(16, 16, [40, 240, 40, 255]),
            solid(16, 16, [40, 40, 240, 255]),
        ];
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("anim.webp");
        let settings = WebpSettings {
            fps: 20,
            width: None,
            ..WebpSettings::balanced()
        };
        export_webp(&frames, &settings, &out).unwrap();

        let bytes = std::fs::read(&out).unwrap();
        assert_eq!(&bytes[0..4], b"RIFF");
        assert_eq!(&bytes[8..12], b"WEBP");

        let (info, timestamps) = decode_webp(&bytes);
        assert_eq!((info.canvas_width, info.canvas_height), (16, 16));
        assert_eq!(info.loop_count, 0);
        assert_eq!(info.frame_count, 3);
        assert_eq!(timestamps, vec![50, 100, 150]);
    }

    #[test]
    fn duplicate_frames_collapse_and_preserve_total_duration() {
        let a = solid(16, 16, [40, 80, 120, 255]);
        let b = solid(16, 16, [200, 120, 40, 255]);
        let settings = WebpSettings {
            fps: 10, // 100ms per frame -> 300ms total
            width: None,
            ..WebpSettings::balanced()
        };
        // All-identical frames collapse to a single still frame.
        let single = encode_webp_to_vec(&[a.clone(), a.clone(), a.clone()], &settings).unwrap();
        let (info, _) = decode_webp(&single);
        assert_eq!(info.frame_count, 1);

        // Leading duplicate frames collapse while preserving the 300ms timeline duration.
        let collapsed = encode_webp_to_vec(&[a.clone(), a, b], &settings).unwrap();
        let (info, timestamps) = decode_webp(&collapsed);
        assert_eq!(info.frame_count, 2);
        assert_eq!(timestamps, vec![200, 300]);
    }

    #[test]
    fn lossy_and_lossless_both_produce_valid_webp() {
        let frames = vec![
            solid(24, 16, [15, 25, 35, 255]),
            solid(24, 16, [235, 215, 195, 255]),
        ];
        for lossless in [false, true] {
            let settings = WebpSettings {
                fps: 20,
                width: None,
                quality: 80,
                lossless,
                method: 4,
            };
            let bytes = encode_webp_to_vec(&frames, &settings).unwrap();
            let (info, _) = decode_webp(&bytes);
            assert_eq!((info.canvas_width, info.canvas_height), (24, 16));
            assert_eq!(info.frame_count, 2);
        }
    }

    #[test]
    fn animated_webp_is_smaller_than_gif_on_demo_scene() {
        let (w, h, n) = (320u32, 200u32, 20usize);
        let mut bg = Vec::with_capacity((w * h * 4) as usize);
        for y in 0..h {
            for x in 0..w {
                let r = ((x * 255) / w) as u8;
                let g = ((y * 255) / h) as u8;
                let b = (((x + y) * 127) / (w + h)) as u8;
                bg.extend_from_slice(&[r, g, b, 255]);
            }
        }
        let frames: Vec<RgbaImage> = (0..n)
            .map(|f| {
                let mut px = bg.clone();
                for y in 70..130usize {
                    for x in (f * 10)..(f * 10 + 48) {
                        let o = (y * w as usize + x) * 4;
                        px[o..o + 4].copy_from_slice(&[240, 244, 248, 255]);
                    }
                }
                RgbaImage::new(w, h, px)
            })
            .collect();

        let dir = tempfile::tempdir().unwrap();
        let gif_path = dir.path().join("demo.gif");
        let webp_path = dir.path().join("demo.webp");
        export_gif_native(
            &frames,
            &GifSettings {
                fps: 15,
                width: None,
                quality: 80,
                lossy: None,
                dither: true,
            },
            &gif_path,
        )
        .unwrap();
        export_webp(
            &frames,
            &WebpSettings {
                fps: 15,
                width: None,
                quality: 80,
                lossless: false,
                method: 4,
            },
            &webp_path,
        )
        .unwrap();

        let gif_bytes = std::fs::metadata(&gif_path).unwrap().len();
        let webp_bytes = std::fs::metadata(&webp_path).unwrap().len();
        assert!(
            webp_bytes < gif_bytes,
            "expected animated WebP ({webp_bytes} B) to be smaller than GIF ({gif_bytes} B)"
        );
    }

    #[test]
    fn streaming_encoder_reports_progress_and_propagates_cancel() {
        let frames = [
            solid(12, 12, [10, 10, 10, 255]),
            solid(12, 12, [200, 200, 200, 255]),
        ];
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("stream.webp");
        let ticks = std::cell::Cell::new(0usize);
        export_webp_streaming(
            frames.len(),
            |i| Ok(frames[i].clone()),
            &WebpSettings {
                width: None,
                ..WebpSettings::balanced()
            },
            &out,
            &|done, total| {
                assert!(done <= total);
                ticks.set(ticks.get() + 1);
            },
        )
        .unwrap();
        assert_eq!(ticks.get(), frames.len() + 1);
        assert!(out.exists());

        let canceled = dir.path().join("cancel.webp");
        let err = export_webp_streaming(
            2,
            |i| {
                if i == 1 {
                    Err("export cancelled".into())
                } else {
                    Ok(frames[0].clone())
                }
            },
            &WebpSettings {
                width: None,
                ..WebpSettings::balanced()
            },
            &canceled,
            &|_, _| {},
        );
        assert!(matches!(err, Err(EncodeError::Webp(ref s)) if s.contains("export cancelled")));
        assert!(!canceled.exists());
    }

    #[test]
    fn stream_dedup_preserves_timeline_durations() {
        let a = solid(16, 16, [20, 40, 200, 255]);
        let b = solid(16, 16, [220, 60, 40, 255]);
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("dedup.webp");
        let settings = WebpSettings {
            fps: 10, // 100ms per frame -> 500ms total across 5 frames
            width: None,
            ..WebpSettings::balanced()
        };
        export_webp_stream_dedup(
            5,
            |i| match i {
                0 => Ok(StreamFrame::Image(a.clone())),
                1 | 2 | 4 => Ok(StreamFrame::Repeat),
                3 => Ok(StreamFrame::Image(b.clone())),
                _ => unreachable!(),
            },
            &settings,
            &out,
            &|_, _| {},
        )
        .unwrap();
        let bytes = std::fs::read(&out).unwrap();
        let (info, timestamps) = decode_webp(&bytes);
        assert_eq!(info.frame_count, 2);
        assert_eq!(timestamps, vec![300, 500]);
    }
}
