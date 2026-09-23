//! MP4 (H.264) export via Windows Media Foundation's sink writer.
//!
//! No bundled ffmpeg: the OS H.264 encoder MFT does the work. We feed uncompressed RGB32
//! frames (BGRA memory order, top-down via a positive `MF_MT_DEFAULT_STRIDE`) and the sink
//! writer inserts the color converter the encoder needs. The sink writer only considers
//! hardware encoders (NVENC, Quick Sync, AMF) when the hardware-transforms attribute is set,
//! so it is, with a software-encoder fallback for GPUs whose MFT rejects the setup.
//! With a soundtrack, an AAC stream (48 kHz stereo, 192 kbps, the OS encoder again) is fed
//! 16-bit PCM interleaved with the video; if the AAC encoder is unavailable the file is
//! written without audio rather than failing.
//! Compile-verified on CI; the encode path needs a real Windows session to run.

#[cfg(windows)]
mod imp {
    use std::path::Path;
    use std::sync::OnceLock;
    use windows::core::PCWSTR;
    use windows::Win32::Media::MediaFoundation::{
        IMFAttributes, IMFByteStream, IMFSinkWriter, MFAudioFormat_AAC, MFAudioFormat_PCM,
        MFCreateAttributes, MFCreateMediaType, MFCreateMemoryBuffer, MFCreateSample,
        MFCreateSinkWriterFromURL, MFMediaType_Audio, MFMediaType_Video, MFStartup,
        MFVideoFormat_H264, MFVideoFormat_RGB32, MFVideoInterlace_Progressive, MFSTARTUP_FULL,
        MF_MT_AUDIO_AVG_BYTES_PER_SECOND, MF_MT_AUDIO_BITS_PER_SAMPLE, MF_MT_AUDIO_BLOCK_ALIGNMENT,
        MF_MT_AUDIO_NUM_CHANNELS, MF_MT_AUDIO_SAMPLES_PER_SECOND, MF_MT_AVG_BITRATE,
        MF_MT_DEFAULT_STRIDE, MF_MT_FRAME_RATE, MF_MT_FRAME_SIZE, MF_MT_INTERLACE_MODE,
        MF_MT_MAJOR_TYPE, MF_MT_PIXEL_ASPECT_RATIO, MF_MT_SUBTYPE,
        MF_READWRITE_ENABLE_HARDWARE_TRANSFORMS, MF_SINK_WRITER_DISABLE_THROTTLING, MF_VERSION,
    };
    use windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};

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

    /// Map the 40-100 quality slider to an H.264 average bitrate for this size/rate.
    /// Roughly 0.04-0.2 bits per pixel per frame, README-screencast territory.
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
        /// The AAC stream, when the file carries a soundtrack.
        audio: Option<u32>,
        w: u32,
        h: u32,
        /// Per-frame duration in 100ns units.
        frame_hns: i64,
    }

    impl Mp4Encoder {
        /// Create the sink writer for `path` and configure H.264 out / RGB32 in, preferring a
        /// hardware encoder and falling back to Microsoft's software one. With `audio`, an
        /// AAC stream is added too; if no setup accepts it, the file is video-only (see
        /// [`Self::has_audio`]).
        pub fn new(
            path: &Path,
            w: u32,
            h: u32,
            fps: u32,
            quality: u8,
            audio: bool,
        ) -> Result<Self, String> {
            ensure_mf()?;
            let attempt = |hardware: bool, audio: bool| {
                let r = Self::with_hardware(path, w, h, fps, quality, hardware, audio);
                if r.is_err() {
                    let _ = std::fs::remove_file(path);
                }
                r
            };
            let first = match attempt(true, audio) {
                Ok(enc) => return Ok(enc),
                Err(e) => e,
            };
            tracing::warn!("hardware H.264 setup failed ({first}), using software");
            match attempt(false, audio) {
                Ok(enc) => Ok(enc),
                Err(e) if audio => {
                    tracing::warn!("MP4 with audio failed ({e}), writing video only");
                    attempt(false, false)
                }
                Err(e) => Err(e),
            }
        }

        /// Whether the file carries a soundtrack.
        pub fn has_audio(&self) -> bool {
            self.audio.is_some()
        }

        fn with_hardware(
            path: &Path,
            w: u32,
            h: u32,
            fps: u32,
            quality: u8,
            hardware: bool,
            audio: bool,
        ) -> Result<Self, String> {
            let wide: Vec<u16> = path
                .as_os_str()
                .to_string_lossy()
                .encode_utf16()
                .chain([0u16])
                .collect();

            // SAFETY: standard sink-writer setup; all pointers outlive the calls.
            unsafe {
                let mut attrs: Option<IMFAttributes> = None;
                MFCreateAttributes(&mut attrs, 2).map_err(|e| e.to_string())?;
                let attrs = attrs.ok_or("sink writer attributes")?;
                attrs
                    .SetUINT32(
                        &MF_READWRITE_ENABLE_HARDWARE_TRANSFORMS,
                        u32::from(hardware),
                    )
                    .map_err(|e| e.to_string())?;
                // We feed frames as fast as we can composite them (offline export), so let the
                // writer accept samples without pacing them to real time.
                attrs
                    .SetUINT32(&MF_SINK_WRITER_DISABLE_THROTTLING, 1)
                    .map_err(|e| e.to_string())?;
                let writer = MFCreateSinkWriterFromURL(
                    PCWSTR(wide.as_ptr()),
                    None::<&IMFByteStream>,
                    &attrs,
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
                let audio = if audio {
                    let added = add_audio_stream(&writer);
                    Some(added.map_err(|e| format!("AAC stream: {e}"))?)
                } else {
                    None
                };

                writer.BeginWriting().map_err(|e| e.to_string())?;
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
    pub fn write_rgba(&self, _rgba: &[u8], _w: u32, _h: u32, _i: u32) -> Result<(), String> {
        Err("MP4 export is Windows-only".into())
    }
    pub fn finish(self) -> Result<(), String> {
        Err("MP4 export is Windows-only".into())
    }
}
