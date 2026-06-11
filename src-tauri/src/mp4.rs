//! MP4 (H.264) export via Windows Media Foundation's sink writer.
//!
//! No bundled ffmpeg: the OS H.264 encoder MFT does the work. We feed uncompressed RGB32
//! frames (BGRA memory order, top-down via a positive `MF_MT_DEFAULT_STRIDE`) and the sink
//! writer inserts the color converter the encoder needs. Compile-verified on CI; the
//! encode path needs a real Windows session to run.

#[cfg(windows)]
mod imp {
    use std::path::Path;
    use std::sync::OnceLock;
    use windows::core::PCWSTR;
    use windows::Win32::Media::MediaFoundation::{
        IMFAttributes, IMFByteStream, IMFSinkWriter, MFCreateMediaType, MFCreateMemoryBuffer,
        MFCreateSample, MFCreateSinkWriterFromURL, MFMediaType_Video, MFStartup,
        MFVideoFormat_H264, MFVideoFormat_RGB32, MFVideoInterlace_Progressive, MFSTARTUP_FULL,
        MF_MT_AVG_BITRATE, MF_MT_DEFAULT_STRIDE, MF_MT_FRAME_RATE, MF_MT_FRAME_SIZE,
        MF_MT_INTERLACE_MODE, MF_MT_MAJOR_TYPE, MF_MT_PIXEL_ASPECT_RATIO, MF_MT_SUBTYPE,
        MF_VERSION,
    };
    use windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};

    /// `MF_MT_FRAME_SIZE` / `MF_MT_FRAME_RATE` pack two u32s into one u64 attribute.
    fn pack2(hi: u32, lo: u32) -> u64 {
        (u64::from(hi) << 32) | u64::from(lo)
    }

    /// Map the 40–100 quality slider to an H.264 average bitrate for this size/rate.
    /// Roughly 0.04–0.2 bits per pixel per frame — README-screencast territory.
    fn bitrate(w: u32, h: u32, fps: u32, quality: u8) -> u32 {
        let q = f64::from(quality.clamp(40, 100));
        let bpp = 0.04 + (q - 40.0) / 60.0 * 0.16;
        let bits = f64::from(w) * f64::from(h) * f64::from(fps) * bpp;
        (bits as u32).clamp(1_000_000, 50_000_000)
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
        w: u32,
        h: u32,
        /// Per-frame duration in 100ns units.
        frame_hns: i64,
    }

    impl Mp4Encoder {
        /// Create the sink writer for `path` and configure H.264 out / RGB32 in.
        pub fn new(path: &Path, w: u32, h: u32, fps: u32, quality: u8) -> Result<Self, String> {
            ensure_mf()?;
            let wide: Vec<u16> = path
                .as_os_str()
                .to_string_lossy()
                .encode_utf16()
                .chain([0u16])
                .collect();

            // SAFETY: standard sink-writer setup; all pointers outlive the calls.
            unsafe {
                let writer = MFCreateSinkWriterFromURL(
                    PCWSTR(wide.as_ptr()),
                    None::<&IMFByteStream>,
                    None::<&IMFAttributes>,
                )
                .map_err(|e| format!("create MP4 writer: {e}"))?;

                let out = MFCreateMediaType().map_err(|e| e.to_string())?;
                out.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)
                    .map_err(|e| e.to_string())?;
                out.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_H264)
                    .map_err(|e| e.to_string())?;
                out.SetUINT32(&MF_MT_AVG_BITRATE, bitrate(w, h, fps, quality))
                    .map_err(|e| e.to_string())?;
                out.SetUINT64(&MF_MT_FRAME_SIZE, pack2(w, h))
                    .map_err(|e| e.to_string())?;
                out.SetUINT64(&MF_MT_FRAME_RATE, pack2(fps, 1))
                    .map_err(|e| e.to_string())?;
                out.SetUINT64(&MF_MT_PIXEL_ASPECT_RATIO, pack2(1, 1))
                    .map_err(|e| e.to_string())?;
                out.SetUINT32(&MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as u32)
                    .map_err(|e| e.to_string())?;
                let stream = writer
                    .AddStream(&out)
                    .map_err(|e| format!("H.264 stream: {e}"))?;

                let inp = MFCreateMediaType().map_err(|e| e.to_string())?;
                inp.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)
                    .map_err(|e| e.to_string())?;
                inp.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_RGB32)
                    .map_err(|e| e.to_string())?;
                inp.SetUINT64(&MF_MT_FRAME_SIZE, pack2(w, h))
                    .map_err(|e| e.to_string())?;
                inp.SetUINT64(&MF_MT_FRAME_RATE, pack2(fps, 1))
                    .map_err(|e| e.to_string())?;
                inp.SetUINT64(&MF_MT_PIXEL_ASPECT_RATIO, pack2(1, 1))
                    .map_err(|e| e.to_string())?;
                inp.SetUINT32(&MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as u32)
                    .map_err(|e| e.to_string())?;
                // Positive stride = top-down rows, which is how our frames are laid out.
                inp.SetUINT32(&MF_MT_DEFAULT_STRIDE, w * 4)
                    .map_err(|e| e.to_string())?;
                writer
                    .SetInputMediaType(stream, &inp, None::<&IMFAttributes>)
                    .map_err(|e| format!("RGB32 input not accepted: {e}"))?;

                writer.BeginWriting().map_err(|e| e.to_string())?;
                Ok(Self {
                    writer,
                    stream,
                    w,
                    h,
                    frame_hns: (10_000_000 / i64::from(fps.max(1))).max(1),
                })
            }
        }

        /// Encode one RGBA frame (any size ≥ the encoder size; extra right/bottom pixels
        /// are cropped, which also handles odd-dimension downscales).
        pub fn write_rgba(
            &self,
            rgba: &[u8],
            src_w: u32,
            src_h: u32,
            index: u32,
        ) -> Result<(), String> {
            if src_w < self.w || src_h < self.h {
                return Err("frame smaller than encoder size".into());
            }
            let len = self.w * self.h * 4;
            // SAFETY: buffer is locked, filled within bounds, unlocked before use.
            unsafe {
                let buffer = MFCreateMemoryBuffer(len).map_err(|e| e.to_string())?;
                let mut ptr: *mut u8 = std::ptr::null_mut();
                buffer
                    .Lock(&mut ptr, None, None)
                    .map_err(|e| e.to_string())?;
                let dst = std::slice::from_raw_parts_mut(ptr, len as usize);
                // RGBA → RGB32 (BGRX memory order), row by row with right-edge crop.
                for y in 0..self.h as usize {
                    let src_row = &rgba[y * src_w as usize * 4..];
                    let dst_row = &mut dst[y * self.w as usize * 4..][..self.w as usize * 4];
                    for x in 0..self.w as usize {
                        dst_row[x * 4] = src_row[x * 4 + 2]; // B
                        dst_row[x * 4 + 1] = src_row[x * 4 + 1]; // G
                        dst_row[x * 4 + 2] = src_row[x * 4]; // R
                        dst_row[x * 4 + 3] = 255;
                    }
                }
                buffer.Unlock().map_err(|e| e.to_string())?;
                buffer.SetCurrentLength(len).map_err(|e| e.to_string())?;

                let sample = MFCreateSample().map_err(|e| e.to_string())?;
                sample.AddBuffer(&buffer).map_err(|e| e.to_string())?;
                sample
                    .SetSampleTime(i64::from(index) * self.frame_hns)
                    .map_err(|e| e.to_string())?;
                sample
                    .SetSampleDuration(self.frame_hns)
                    .map_err(|e| e.to_string())?;
                self.writer
                    .WriteSample(self.stream, &sample)
                    .map_err(|e| format!("encode frame {index}: {e}"))?;
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
    ) -> Result<Self, String> {
        Err("MP4 export is Windows-only".into())
    }
    pub fn write_rgba(&self, _rgba: &[u8], _w: u32, _h: u32, _i: u32) -> Result<(), String> {
        Err("MP4 export is Windows-only".into())
    }
    pub fn finish(self) -> Result<(), String> {
        Err("MP4 export is Windows-only".into())
    }
}
