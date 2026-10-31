//! MP4 (H.264) export via Windows Media Foundation's sink writer.
//!
//! No bundled ffmpeg: the OS H.264 encoder MFT does the work. Frames go to it as NV12, the
//! format every H.264 encoder takes natively, converted from the compositor's RGBA here
//! ([`rgba_to_nv12`]) with the BT.709 studio-range matrix, and both ends of the encoder are
//! tagged BT.709 to match. Handing the sink writer RGB instead would leave the conversion to
//! a converter it inserts on its own, whose matrix follows the frame size (BT.601 for small
//! frames) and isn't written into the file: players then guess, and colors shift.
//!
//! The encoder is asked for the High profile and average-bitrate VBR (flat stretches of a
//! screen recording cost almost nothing, and a zoom gets the bits they saved) with a
//! keyframe every two seconds. The sink writer only considers hardware encoders (NVENC,
//! Quick Sync, AMF) when the hardware-transforms attribute is set, so it is, with fallbacks
//! to Microsoft's software encoder and then to the encoder's own defaults for a driver that
//! rejects the setup. Sink-writer throttling stays on: it blocks a write while too many frames
//! wait for the encoder, which is all that keeps a 4K export from queueing frames in memory
//! faster than a software encoder takes them, until the computer runs out.
//! With a soundtrack, an AAC stream (48 kHz stereo, 192 kbps, the OS encoder again) is fed
//! 16-bit PCM interleaved with the video; if the AAC encoder is unavailable the file is
//! written without audio rather than failing.
//! Compile-verified on CI; the encode path needs a real Windows session to run.

/// BT.709 luma weights for studio range (16..=235), in 15-bit fixed point.
const Y_R: i32 = 5983;
const Y_G: i32 = 20127;
const Y_B: i32 = 2032;
/// BT.709 chroma weights for studio range (16..=240), in 15-bit fixed point.
const CB_R: i32 = -3298;
const CB_G: i32 = -11094;
const CB_B: i32 = 14392;
const CR_R: i32 = 14392;
const CR_G: i32 = -13073;
const CR_B: i32 = -1319;

fn luma(r: u8, g: u8, b: u8) -> u8 {
    let y = Y_R * i32::from(r) + Y_G * i32::from(g) + Y_B * i32::from(b);
    (((y + (1 << 14)) >> 15) + 16) as u8
}

/// One chroma sample from the SUMS of a 2x2 block's channels (hence the two extra bits).
fn chroma(wr: i32, wg: i32, wb: i32, (r, g, b): (i32, i32, i32)) -> u8 {
    (((wr * r + wg * g + wb * b + (1 << 16)) >> 17) + 128) as u8
}

/// Two rows of RGBA (`src`, `w` pixels wide) into two rows of luma and one row of
/// interleaved chroma, each chroma pair the average of its 2x2 block.
fn nv12_row_pair(src: &[u8], w: usize, y_rows: &mut [u8], uv_row: &mut [u8]) {
    let (top, bottom) = src.split_at(w * 4);
    let (y_top, y_bottom) = y_rows.split_at_mut(w);
    let (top, _) = top.as_chunks::<8>();
    let (bottom, _) = bottom.as_chunks::<8>();
    let (y_top, _) = y_top.as_chunks_mut::<2>();
    let (y_bottom, _) = y_bottom.as_chunks_mut::<2>();
    let (uv, _) = uv_row.as_chunks_mut::<2>();
    let lumas = y_top.iter_mut().zip(y_bottom);
    let blocks = top.iter().zip(bottom).zip(lumas).zip(uv);
    for (((t, b), (yt, yb)), c) in blocks {
        yt[0] = luma(t[0], t[1], t[2]);
        yt[1] = luma(t[4], t[5], t[6]);
        yb[0] = luma(b[0], b[1], b[2]);
        yb[1] = luma(b[4], b[5], b[6]);
        let sum = |i: usize| {
            i32::from(t[i]) + i32::from(t[i + 4]) + i32::from(b[i]) + i32::from(b[i + 4])
        };
        let rgb = (sum(0), sum(1), sum(2));
        c[0] = chroma(CB_R, CB_G, CB_B, rgb);
        c[1] = chroma(CR_R, CR_G, CR_B, rgb);
    }
}

/// Convert a tightly packed RGBA frame, `w` pixels wide (`w` and the height even), to NV12:
/// `y` gets the full-size luma plane and `uv` the half-height interleaved chroma plane.
/// BT.709, studio range. Row pairs are converted in parallel.
pub(crate) fn rgba_to_nv12(rgba: &[u8], w: usize, y: &mut [u8], uv: &mut [u8]) {
    use rayon::prelude::*;
    y.par_chunks_mut(w * 2)
        .zip(uv.par_chunks_mut(w))
        .zip(rgba.par_chunks(w * 8))
        .for_each(|((y_rows, uv_row), src)| nv12_row_pair(src, w, y_rows, uv_row));
}

/// The largest frame H.264 encoders take, by the High profile's level 5.2 and in practice:
/// no side over 4096 and no more pixels than 4096×2304.
const H264_MAX_SIDE: f64 = 4096.0;
const H264_MAX_PIXELS: f64 = 4096.0 * 2304.0;

/// `w`×`h` (even) as they are if an H.264 encoder takes that size, else shrunk to fit with the
/// same shape. A frame around a 4K take grows the picture past what any H.264 encoder takes,
/// and the export must still work: the framed picture comes out at the most H.264 can hold.
pub(crate) fn fit_h264(w: u32, h: u32) -> (u32, u32) {
    let (fw, fh) = (f64::from(w), f64::from(h));
    let k = (H264_MAX_SIDE / fw)
        .min(H264_MAX_SIDE / fh)
        .min((H264_MAX_PIXELS / (fw * fh)).sqrt());
    if k >= 1.0 {
        return (w, h);
    }
    let even = |v: f64| ((v * k / 2.0).floor() as u32 * 2).max(2);
    (even(fw), even(fh))
}

/// The most bits per second we ask of an encoder: 4K at 60 fps at the top of the slider.
const MAX_BITRATE: u32 = 120_000_000;

/// Map the 40-100 quality slider to an H.264 average bitrate for this size/rate.
/// Roughly 0.04-0.2 bits per pixel per frame, README-screencast territory.
pub(crate) fn bitrate(w: u32, h: u32, fps: u32, quality: u8) -> u32 {
    let q = f64::from(quality.clamp(40, 100));
    let bpp = 0.04 + (q - 40.0) / 60.0 * 0.16;
    let bits = f64::from(w) * f64::from(h) * f64::from(fps) * bpp;
    (bits as u32).clamp(1_000_000, MAX_BITRATE)
}

#[cfg(windows)]
mod imp {
    use super::{bitrate, rgba_to_nv12};
    use std::path::Path;
    use std::sync::OnceLock;
    use windows::core::{GUID, PCWSTR};
    use windows::Win32::Media::MediaFoundation::{
        CODECAPI_AVEncCommonMeanBitRate, CODECAPI_AVEncCommonRateControlMode,
        CODECAPI_AVEncMPVGOPSize, IMFAttributes, IMFByteStream, IMFMediaType, IMFSinkWriter,
        MFAudioFormat_AAC, MFAudioFormat_PCM, MFCreateAttributes, MFCreateMediaType,
        MFCreateMemoryBuffer, MFCreateSample, MFCreateSinkWriterFromURL, MFMediaType_Audio,
        MFMediaType_Video, MFStartup, MFVideoFormat_H264, MFVideoFormat_NV12,
        MFVideoInterlace_Progressive, MFSTARTUP_FULL, MF_MT_AUDIO_AVG_BYTES_PER_SECOND,
        MF_MT_AUDIO_BITS_PER_SAMPLE, MF_MT_AUDIO_BLOCK_ALIGNMENT, MF_MT_AUDIO_NUM_CHANNELS,
        MF_MT_AUDIO_SAMPLES_PER_SECOND, MF_MT_AVG_BITRATE, MF_MT_DEFAULT_STRIDE, MF_MT_FRAME_RATE,
        MF_MT_FRAME_SIZE, MF_MT_INTERLACE_MODE, MF_MT_MAJOR_TYPE, MF_MT_MPEG2_PROFILE,
        MF_MT_PIXEL_ASPECT_RATIO, MF_MT_SUBTYPE, MF_MT_TRANSFER_FUNCTION,
        MF_MT_VIDEO_NOMINAL_RANGE, MF_MT_VIDEO_PRIMARIES, MF_MT_YUV_MATRIX,
        MF_READWRITE_ENABLE_HARDWARE_TRANSFORMS, MF_VERSION,
    };
    use windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};

    /// `eAVEncH264VProfile_High`.
    const H264_PROFILE_HIGH: u32 = 100;
    /// `eAVEncCommonRateControlMode_UnconstrainedVBR`: hold the average bitrate over the
    /// whole file, spending it where the picture needs it.
    const RATE_CONTROL_VBR: u32 = 2;
    /// `MFVideoPrimaries_BT709`.
    const PRIMARIES_BT709: u32 = 2;
    /// `MFVideoTransFunc_709`.
    const TRANSFER_BT709: u32 = 5;
    /// `MFVideoTransferMatrix_BT709`.
    const MATRIX_BT709: u32 = 1;
    /// `MFNominalRange_16_235` (studio range).
    const RANGE_16_235: u32 = 2;
    /// Seconds between keyframes: a player can seek to any of them.
    const KEYFRAME_SECONDS: u32 = 2;

    /// A Media Foundation result with its error as text.
    fn ok<T>(r: windows::core::Result<T>) -> Result<T, String> {
        r.map_err(|e| e.to_string())
    }

    /// Soundtrack sample rate (the mixer renders at this rate).
    const AUDIO_RATE: u32 = vuoom_audio::OUTPUT_RATE;
    /// AAC bitrate in bytes per second (192 kbps; the encoder accepts 12/16/20/24 k).
    const AAC_BYTES_PER_SEC: u32 = 24_000;

    /// Add an AAC output stream fed 16-bit stereo PCM. Returns its stream index.
    ///
    /// # Safety
    /// `writer` must be a sink writer that has not begun writing.
    unsafe fn add_audio_stream(writer: &IMFSinkWriter) -> windows::core::Result<u32> {
        // SAFETY: guaranteed by the caller; the media types are fresh and owned here.
        unsafe {
            let out = MFCreateMediaType()?;
            out.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Audio)?;
            out.SetGUID(&MF_MT_SUBTYPE, &MFAudioFormat_AAC)?;
            out.SetUINT32(&MF_MT_AUDIO_BITS_PER_SAMPLE, 16)?;
            out.SetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND, AUDIO_RATE)?;
            out.SetUINT32(&MF_MT_AUDIO_NUM_CHANNELS, 2)?;
            out.SetUINT32(&MF_MT_AUDIO_AVG_BYTES_PER_SECOND, AAC_BYTES_PER_SEC)?;
            let stream = writer.AddStream(&out)?;

            let inp = MFCreateMediaType()?;
            inp.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Audio)?;
            inp.SetGUID(&MF_MT_SUBTYPE, &MFAudioFormat_PCM)?;
            inp.SetUINT32(&MF_MT_AUDIO_BITS_PER_SAMPLE, 16)?;
            inp.SetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND, AUDIO_RATE)?;
            inp.SetUINT32(&MF_MT_AUDIO_NUM_CHANNELS, 2)?;
            inp.SetUINT32(&MF_MT_AUDIO_BLOCK_ALIGNMENT, 4)?;
            inp.SetUINT32(&MF_MT_AUDIO_AVG_BYTES_PER_SECOND, AUDIO_RATE * 4)?;
            writer.SetInputMediaType(stream, &inp, None::<&IMFAttributes>)?;
            Ok(stream)
        }
    }

    /// `MF_MT_FRAME_SIZE` / `MF_MT_FRAME_RATE` pack two u32s into one u64 attribute.
    fn pack2(hi: u32, lo: u32) -> u64 {
        (u64::from(hi) << 32) | u64::from(lo)
    }

    /// A progressive, square-pixel video type of `subtype`, tagged BT.709 studio range (what
    /// [`rgba_to_nv12`] produces, and what the encoder then writes into the stream).
    ///
    /// # Safety
    /// Media Foundation must be started in this process (see [`ensure_mf`]).
    unsafe fn video_type(subtype: &GUID, w: u32, h: u32, fps: u32) -> Result<IMFMediaType, String> {
        // SAFETY: a fresh media type, owned here; every pointer outlives its call.
        unsafe {
            let t = ok(MFCreateMediaType())?;
            ok(t.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video))?;
            ok(t.SetGUID(&MF_MT_SUBTYPE, subtype))?;
            ok(t.SetUINT64(&MF_MT_FRAME_SIZE, pack2(w, h)))?;
            ok(t.SetUINT64(&MF_MT_FRAME_RATE, pack2(fps, 1)))?;
            ok(t.SetUINT64(&MF_MT_PIXEL_ASPECT_RATIO, pack2(1, 1)))?;
            let progressive = MFVideoInterlace_Progressive.0 as u32;
            ok(t.SetUINT32(&MF_MT_INTERLACE_MODE, progressive))?;
            ok(t.SetUINT32(&MF_MT_VIDEO_PRIMARIES, PRIMARIES_BT709))?;
            ok(t.SetUINT32(&MF_MT_TRANSFER_FUNCTION, TRANSFER_BT709))?;
            ok(t.SetUINT32(&MF_MT_YUV_MATRIX, MATRIX_BT709))?;
            ok(t.SetUINT32(&MF_MT_VIDEO_NOMINAL_RANGE, RANGE_16_235))?;
            Ok(t)
        }
    }

    /// The encoder settings the sink writer hands the H.264 encoder: VBR around `bitrate`
    /// and a keyframe every [`KEYFRAME_SECONDS`].
    ///
    /// # Safety
    /// Media Foundation must be started in this process (see [`ensure_mf`]).
    unsafe fn encoder_params(bitrate: u32, fps: u32) -> Result<IMFAttributes, String> {
        // SAFETY: a fresh attribute store, owned here; every pointer outlives its call.
        unsafe {
            let mut attrs: Option<IMFAttributes> = None;
            ok(MFCreateAttributes(&mut attrs, 3))?;
            let attrs = attrs.ok_or("encoder attributes")?;
            ok(attrs.SetUINT32(&CODECAPI_AVEncCommonRateControlMode, RATE_CONTROL_VBR))?;
            ok(attrs.SetUINT32(&CODECAPI_AVEncCommonMeanBitRate, bitrate))?;
            let gop = fps.max(1) * KEYFRAME_SECONDS;
            ok(attrs.SetUINT32(&CODECAPI_AVEncMPVGOPSize, gop))?;
            Ok(attrs)
        }
    }

    /// One way of asking for an encoder; [`Mp4Encoder::new`] tries them best first.
    #[derive(Clone, Copy, Debug)]
    struct Setup {
        /// Let the sink writer pick a hardware encoder.
        hardware: bool,
        /// Ask for the High profile, VBR and the keyframe interval (off: the encoder's
        /// own defaults, for one that rejects them).
        tuned: bool,
        /// Add the AAC soundtrack stream.
        audio: bool,
    }

    /// One-time Media Foundation startup (per process). COM init is per-thread and cheap;
    /// a `RPC_E_CHANGED_MODE` result just means the thread already has an apartment.
    fn ensure_mf() -> Result<(), String> {
        static START: OnceLock<Result<(), String>> = OnceLock::new();
        // SAFETY: standard COM/MF initialization.
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        }
        START
            .get_or_init(|| {
                // SAFETY: MFStartup with the SDK version constant.
                unsafe { MFStartup(MF_VERSION, MFSTARTUP_FULL) }.map_err(|e| e.to_string())
            })
            .clone()
    }

    /// A streaming H.264/MP4 encoder: feed RGBA frames in order, then [`Mp4Encoder::finish`].
    pub struct Mp4Encoder {
        writer: IMFSinkWriter,
        stream: u32,
        /// The AAC stream, when the file carries a soundtrack.
        audio: Option<u32>,
        w: u32,
        h: u32,
        /// Per-frame duration in 100ns units.
        frame_hns: i64,
    }

    impl Mp4Encoder {
        /// Create the sink writer for `path` and configure H.264 out / NV12 in, trying a
        /// tuned hardware encoder first, then Microsoft's software one, then that one on its
        /// defaults. With `audio`, an AAC stream is added too; if no setup accepts it, the
        /// file is video-only (see [`Self::has_audio`]).
        pub fn new(
            path: &Path,
            w: u32,
            h: u32,
            fps: u32,
            quality: u8,
            audio: bool,
        ) -> Result<Self, String> {
            ensure_mf()?;
            if w < 2 || h < 2 || !w.is_multiple_of(2) || !h.is_multiple_of(2) {
                return Err(format!("MP4 needs even dimensions, got {w}x{h}"));
            }
            let soundtracks: &[bool] = if audio { &[true, false] } else { &[false] };
            let encoders = [(true, true), (false, true), (false, false)];
            let mut last = String::new();
            for &audio in soundtracks {
                for (hardware, tuned) in encoders {
                    let setup = Setup {
                        hardware,
                        tuned,
                        audio,
                    };
                    match Self::with_setup(path, w, h, fps, quality, setup) {
                        Ok(enc) => {
                            tracing::info!("MP4 encoder ready: {w}x{h} at {fps} fps, {setup:?}");
                            return Ok(enc);
                        }
                        Err(e) => {
                            tracing::warn!("MP4 encoder setup failed ({setup:?}): {e}");
                            let _ = std::fs::remove_file(path);
                            last = e;
                        }
                    }
                }
            }
            // The usual cause on an otherwise working PC: a frame bigger than its H.264
            // encoders take (without a recent GPU they top out around 4K at 30 fps).
            if w > 1920 || h > 1088 || fps > 30 {
                return Err(format!(
                    "This computer's video encoder can't make an MP4 of {w} x {h} at {fps} fps. \
                     Try a smaller width or a lower frame rate. ({last})"
                ));
            }
            Err(last)
        }

        /// Whether the file carries a soundtrack.
        pub fn has_audio(&self) -> bool {
            self.audio.is_some()
        }

        fn with_setup(
            path: &Path,
            w: u32,
            h: u32,
            fps: u32,
            quality: u8,
            setup: Setup,
        ) -> Result<Self, String> {
            let wide: Vec<u16> = path
                .as_os_str()
                .to_string_lossy()
                .encode_utf16()
                .chain([0u16])
                .collect();
            let rate = bitrate(w, h, fps, quality);

            // SAFETY: standard sink-writer setup; all pointers outlive the calls.
            unsafe {
                let mut attrs: Option<IMFAttributes> = None;
                ok(MFCreateAttributes(&mut attrs, 1))?;
                let attrs = attrs.ok_or("sink writer attributes")?;
                let hardware = u32::from(setup.hardware);
                ok(attrs.SetUINT32(&MF_READWRITE_ENABLE_HARDWARE_TRANSFORMS, hardware))?;
                let writer = MFCreateSinkWriterFromURL(
                    PCWSTR(wide.as_ptr()),
                    None::<&IMFByteStream>,
                    &attrs,
                )
                .map_err(|e| format!("create MP4 writer: {e}"))?;

                let out = video_type(&MFVideoFormat_H264, w, h, fps)?;
                ok(out.SetUINT32(&MF_MT_AVG_BITRATE, rate))?;
                if setup.tuned {
                    ok(out.SetUINT32(&MF_MT_MPEG2_PROFILE, H264_PROFILE_HIGH))?;
                }
                let stream = writer
                    .AddStream(&out)
                    .map_err(|e| format!("H.264 stream: {e}"))?;

                let inp = video_type(&MFVideoFormat_NV12, w, h, fps)?;
                // Positive stride = top-down rows, one byte per pixel in the luma plane.
                ok(inp.SetUINT32(&MF_MT_DEFAULT_STRIDE, w))?;
                let params = if setup.tuned {
                    Some(encoder_params(rate, fps)?)
                } else {
                    None
                };
                writer
                    .SetInputMediaType(stream, &inp, params.as_ref())
                    .map_err(|e| format!("NV12 input not accepted: {e}"))?;
                let audio = if setup.audio {
                    let added = add_audio_stream(&writer);
                    Some(added.map_err(|e| format!("AAC stream: {e}"))?)
                } else {
                    None
                };

                ok(writer.BeginWriting())?;
                Ok(Self {
                    writer,
                    stream,
                    audio,
                    w,
                    h,
                    frame_hns: (10_000_000 / i64::from(fps.max(1))).max(1),
                })
            }
        }

        /// Encode one RGBA frame of exactly the encoder's size (tightly packed rows).
        pub fn write_rgba(&self, rgba: &[u8], index: u32) -> Result<(), String> {
            let (w, h) = (self.w as usize, self.h as usize);
            if rgba.len() != w * h * 4 {
                return Err("frame is not the encoder's size".into());
            }
            // NV12: a full-size luma plane, then a half-height plane of chroma pairs.
            let len = self.w * self.h * 3 / 2;
            // SAFETY: buffer is locked, filled within bounds, unlocked before use.
            unsafe {
                let buffer = ok(MFCreateMemoryBuffer(len))?;
                let mut ptr: *mut u8 = std::ptr::null_mut();
                ok(buffer.Lock(&mut ptr, None, None))?;
                let dst = std::slice::from_raw_parts_mut(ptr, len as usize);
                let (y, uv) = dst.split_at_mut(w * h);
                rgba_to_nv12(rgba, w, y, uv);
                ok(buffer.Unlock())?;
                ok(buffer.SetCurrentLength(len))?;

                let sample = ok(MFCreateSample())?;
                ok(sample.AddBuffer(&buffer))?;
                ok(sample.SetSampleTime(i64::from(index) * self.frame_hns))?;
                ok(sample.SetSampleDuration(self.frame_hns))?;
                self.writer
                    .WriteSample(self.stream, &sample)
                    .map_err(|e| format!("encode frame {index}: {e}"))?;
            }
            Ok(())
        }

        /// Encode interleaved 16-bit stereo samples (48 kHz) starting at output sample frame
        /// `start`. A no-op when the file has no soundtrack.
        pub fn write_audio(&self, samples: &[i16], start: u64) -> Result<(), String> {
            let Some(stream) = self.audio else {
                return Ok(());
            };
            if samples.is_empty() {
                return Ok(());
            }
            let Ok(len) = u32::try_from(samples.len() * 2) else {
                return Err("audio chunk too large".into());
            };
            let rate = i64::from(AUDIO_RATE);
            let hns = |frames: i64| frames * 10_000_000 / rate;
            let first = start as i64;
            let count = (samples.len() / 2) as i64;
            // SAFETY: buffer is locked, filled within bounds, unlocked before use.
            unsafe {
                let buffer = MFCreateMemoryBuffer(len).map_err(|e| e.to_string())?;
                let mut ptr: *mut u8 = std::ptr::null_mut();
                buffer
                    .Lock(&mut ptr, None, None)
                    .map_err(|e| e.to_string())?;
                let dst = std::slice::from_raw_parts_mut(ptr, len as usize);
                for (d, s) in dst.as_chunks_mut::<2>().0.iter_mut().zip(samples) {
                    *d = s.to_le_bytes();
                }
                buffer.Unlock().map_err(|e| e.to_string())?;
                buffer.SetCurrentLength(len).map_err(|e| e.to_string())?;

                let sample = MFCreateSample().map_err(|e| e.to_string())?;
                sample.AddBuffer(&buffer).map_err(|e| e.to_string())?;
                sample
                    .SetSampleTime(hns(first))
                    .map_err(|e| e.to_string())?;
                sample
                    .SetSampleDuration(hns(first + count) - hns(first))
                    .map_err(|e| e.to_string())?;
                self.writer
                    .WriteSample(stream, &sample)
                    .map_err(|e| format!("encode audio: {e}"))?;
            }
            Ok(())
        }

        /// Flush the encoder and finalize the MP4 container.
        pub fn finish(self) -> Result<(), String> {
            // SAFETY: finalizing a writer we began writing on.
            unsafe { self.writer.Finalize().map_err(|e| e.to_string()) }
        }
    }
}

#[cfg(windows)]
pub use imp::Mp4Encoder;

/// Non-Windows stub (the app is Windows-only, but keeps `cargo check` portable).
#[cfg(not(windows))]
pub struct Mp4Encoder;

#[cfg(not(windows))]
impl Mp4Encoder {
    pub fn new(
        _path: &std::path::Path,
        _w: u32,
        _h: u32,
        _fps: u32,
        _quality: u8,
        _audio: bool,
    ) -> Result<Self, String> {
        Err("MP4 export is Windows-only".into())
    }
    pub fn has_audio(&self) -> bool {
        false
    }
    pub fn write_audio(&self, _samples: &[i16], _start: u64) -> Result<(), String> {
        Err("MP4 export is Windows-only".into())
    }
    pub fn write_rgba(&self, _rgba: &[u8], _i: u32) -> Result<(), String> {
        Err("MP4 export is Windows-only".into())
    }
    pub fn finish(self) -> Result<(), String> {
        Err("MP4 export is Windows-only".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Convert a `w`x`h` frame filled with one color and return (Y, Cb, Cr) of its pixels.
    fn solid(w: usize, h: usize, rgb: [u8; 3]) -> (u8, u8, u8) {
        let rgba: Vec<u8> = (0..w * h)
            .flat_map(|_| [rgb[0], rgb[1], rgb[2], 255])
            .collect();
        let mut y = vec![0u8; w * h];
        let mut uv = vec![0u8; w * h / 2];
        rgba_to_nv12(&rgba, w, &mut y, &mut uv);
        assert!(y.iter().all(|&v| v == y[0]), "luma is flat");
        let (pairs, _) = uv.as_chunks::<2>();
        assert!(pairs.iter().all(|p| *p == pairs[0]), "chroma is flat");
        (y[0], uv[0], uv[1])
    }

    #[test]
    fn nv12_is_bt709_studio_range() {
        // The reference points of BT.709 in 8-bit studio range.
        assert_eq!(solid(4, 2, [0, 0, 0]), (16, 128, 128));
        assert_eq!(solid(4, 2, [255, 255, 255]), (235, 128, 128));
        assert_eq!(solid(6, 4, [255, 0, 0]), (63, 102, 240));
        assert_eq!(solid(6, 4, [0, 0, 255]), (32, 240, 118));
        // Grays carry no color.
        assert_eq!(solid(2, 2, [128, 128, 128]), (126, 128, 128));
    }

    #[test]
    fn nv12_chroma_averages_each_2x2_block() {
        // One 2x2 block: a red, a blue and two black pixels.
        let rgba = [
            255, 0, 0, 255, 0, 0, 255, 255, // top row
            0, 0, 0, 255, 0, 0, 0, 255, // bottom row
        ];
        let (mut y, mut uv) = ([0u8; 4], [0u8; 2]);
        rgba_to_nv12(&rgba, 2, &mut y, &mut uv);
        // Luma stays per pixel...
        assert_eq!(y, [63, 32, 16, 16]);
        // ...chroma is the block's average color (a dark magenta), not any one pixel's.
        assert!(uv[0] > 128 && uv[1] > 128, "{uv:?}");
        assert!(uv[0] < 240 && uv[1] < 240, "{uv:?}");
    }

    #[test]
    fn bitrate_follows_size_rate_and_quality() {
        let low = bitrate(1920, 1080, 30, 40);
        let high = bitrate(1920, 1080, 30, 100);
        assert!(high > low * 4);
        assert!(bitrate(1920, 1080, 60, 100).abs_diff(high * 2) <= 2);
        // Clamped at both ends.
        assert_eq!(bitrate(64, 64, 8, 40), 1_000_000);
        assert_eq!(bitrate(7680, 4320, 60, 100), 120_000_000);
        // 4K at 60 fps gets the bits its pixels call for.
        assert!(bitrate(3840, 2160, 60, 100) > 90_000_000);
    }

    /// One 4K export through the real compositor and encoder, printing what each stage costs.
    /// Writes the file to `VUOOM_PERF_OUT` for the workflow to inspect.
    #[cfg(windows)]
    fn bench_export(name: &str, padding: f64, out: &std::path::Path) -> (u32, u32) {
        use std::time::{Duration, Instant};
        use vuoom_project::{Project, SourceInfo};
        use vuoom_render::{build_scene, BgFill, Compositor};
        use vuoom_zoom::{simulate, ZoomConfig};
        const FPS: u32 = 30;
        const FRAMES: u32 = 45;
        let (sw, sh) = (3840u32, 2160u32);
        let Some(compositor) = Compositor::new() else {
            panic!("no GPU adapter on this machine");
        };
        println!("PERF adapter: {}", compositor.adapter());
        let mut project = Project::new(SourceInfo {
            path: String::new(),
            width: sw,
            height: sh,
            fps: f64::from(FPS),
            duration: f64::from(FRAMES) / f64::from(FPS),
        });
        project.frame.padding = padding;
        let cfg = ZoomConfig::default();
        let duration = project.source.duration;
        let track = simulate(&[], &[], duration, f64::from(FPS), &cfg);
        let (w, h) = project.output_dims();
        let (w, h) = fit_h264(w + w % 2, h + h % 2);
        let mut source = vec![255u8; (sw * sh * 4) as usize];
        for (i, p) in source.as_chunks_mut::<4>().0.iter_mut().enumerate() {
            p[0] = (i % 251) as u8;
            p[1] = (i / 3840 % 256) as u8;
        }
        let bg = BgFill::solid([0.1, 0.1, 0.12, 1.0]);
        let encoder = Mp4Encoder::new(out, w, h, FPS, 85, false).expect("MP4 encoder");
        let (mut drawn, mut encoded) = (Duration::ZERO, Duration::ZERO);
        let start = Instant::now();
        for i in 0..FRAMES {
            // A bar that moves, so each frame has something new to encode.
            let row = (i * 40) as usize * sw as usize * 4;
            source[row..row + sw as usize * 40].fill(i as u8 * 5);
            let t = f64::from(i) / f64::from(FPS);
            let scene = build_scene(&project, &track, w, h, t);
            let at = Instant::now();
            let rgba = compositor.composite_scene(&source, None, sw, sh, w, h, &scene, bg, None);
            drawn += at.elapsed();
            let at = Instant::now();
            encoder.write_rgba(&rgba, i).expect("encode");
            encoded += at.elapsed();
            compositor.recycle(rgba);
        }
        encoder.finish().expect("finish");
        let total = start.elapsed().as_secs_f64();
        println!(
            "PERF export {name} {w}x{h}: {:.1} fps, composite {:.0} ms, encode {:.0} ms a frame",
            f64::from(FRAMES) / total,
            drawn.as_secs_f64() * 1000.0 / f64::from(FRAMES),
            encoded.as_secs_f64() * 1000.0 / f64::from(FRAMES),
        );
        assert!(std::fs::metadata(out).is_ok_and(|m| m.len() > 1000));
        (w, h)
    }

    #[cfg(windows)]
    #[test]
    #[ignore = "a 4K benchmark, run by the engine-4k workflow"]
    fn perf_4k_export() {
        let dir = std::env::var_os("VUOOM_PERF_OUT").map_or_else(std::env::temp_dir, Into::into);
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(
            bench_export("plain", 0.0, &dir.join("plain.mp4")),
            (3840, 2160)
        );
        let (w, h) = bench_export("framed", 0.08, &dir.join("framed.mp4"));
        assert!(w <= 4096 && h <= 2304, "{w}x{h}");
    }

    #[test]
    fn frames_that_fit_h264_are_left_alone() {
        assert_eq!(fit_h264(1920, 1080), (1920, 1080));
        assert_eq!(fit_h264(3840, 2160), (3840, 2160));
        assert_eq!(fit_h264(4096, 2304), (4096, 2304));
    }

    #[test]
    fn a_framed_4k_take_shrinks_to_what_h264_holds() {
        let (w, h) = fit_h264(4454, 2506);
        assert!(w <= 4096 && h <= 4096, "{w}x{h}");
        assert!(u64::from(w) * u64::from(h) <= 4096 * 2304, "{w}x{h}");
        assert!(w % 2 == 0 && h % 2 == 0, "{w}x{h}");
        let (was, now) = (4454.0 / 2506.0, f64::from(w) / f64::from(h));
        assert!((was - now).abs() < 0.01, "shape {was} became {now}");
        // Tall, and an 8K take.
        let (w, h) = fit_h264(2160, 7680);
        assert!(h <= 4096 && w % 2 == 0, "{w}x{h}");
        let (w, h) = fit_h264(7680, 4320);
        assert!(w <= 4096 && h <= 4096, "{w}x{h}");
    }
}
