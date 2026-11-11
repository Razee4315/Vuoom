//! MP4 (H.264 + AAC) export on macOS through `AVAssetWriter`.
//!
//! The same surface as the Windows Media Foundation encoder, and the same picture: frames
//! are converted to NV12 with the shared BT.709 studio-range [`super::rgba_to_nv12`] and the
//! file is tagged BT.709, so an export looks identical on both platforms. VideoToolbox does
//! the encoding (the hardware encoder on Apple Silicon): High profile, average bitrate from
//! [`super::bitrate`], a keyframe every two seconds. The soundtrack is AAC, 48 kHz stereo,
//! 192 kbps, fed 16-bit PCM; if the writer won't take it the file is written without one.
//!
//! The writer is not real-time: every append waits for its input to be ready, which is
//! what keeps a 4K export from queueing frames in memory faster than they are encoded.
//! It also interleaves the tracks, holding video back until about a second more audio has
//! arrived; the export feeds the soundtrack [`super::AUDIO_LEAD_SECS`] ahead and calls
//! [`Mp4Encoder::end_audio`] once it is all written, so neither track waits on the other.

use super::{bitrate, rgba_to_nv12};
use block2::RcBlock;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2_av_foundation::{
    AVAssetWriter, AVAssetWriterInput, AVAssetWriterInputPixelBufferAdaptor, AVAssetWriterStatus,
    AVFileTypeMPEG4, AVMediaTypeAudio, AVMediaTypeVideo, AVVideoAllowFrameReorderingKey,
    AVVideoAverageBitRateKey, AVVideoCodecKey, AVVideoCodecTypeH264, AVVideoColorPrimariesKey,
    AVVideoColorPrimaries_ITU_R_709_2, AVVideoColorPropertiesKey, AVVideoCompressionPropertiesKey,
    AVVideoExpectedSourceFrameRateKey, AVVideoHeightKey, AVVideoMaxKeyFrameIntervalKey,
    AVVideoProfileLevelH264HighAutoLevel, AVVideoProfileLevelKey, AVVideoTransferFunctionKey,
    AVVideoTransferFunction_ITU_R_709_2, AVVideoWidthKey, AVVideoYCbCrMatrixKey,
    AVVideoYCbCrMatrix_ITU_R_709_2,
};
use objc2_core_audio_types::AudioStreamBasicDescription;
use objc2_core_foundation::CFRetained;
use objc2_core_media::{
    CMAudioFormatDescription, CMBlockBuffer, CMFormatDescription, CMSampleBuffer, CMTime,
};
use objc2_core_video::{
    kCVPixelBufferHeightKey, kCVPixelBufferPixelFormatTypeKey, kCVPixelBufferWidthKey,
    kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange, CVPixelBuffer, CVPixelBufferLockFlags,
};
use objc2_foundation::{NSDictionary, NSNumber, NSString, NSURL};
use std::cell::RefCell;
use std::path::Path;
use std::ptr::NonNull;
use std::time::{Duration, Instant};

/// The soundtrack's sample rate (the mixer's output rate).
const AUDIO_RATE: u32 = 48_000;

/// How long an append may wait for its input to be ready before the export gives up.
const READY_WAIT: Duration = Duration::from_secs(20);

fn ns(s: &str) -> Retained<NSString> {
    NSString::from_str(s)
}

/// A framework string constant as an `NSString` key (they are all present on macOS 14+).
fn key(k: Option<&'static NSString>) -> &'static NSString {
    k.expect("AVFoundation constant missing")
}

/// Any object as a plain `AnyObject` (for dictionary values).
fn any<T: objc2::Message>(r: Retained<T>) -> Retained<AnyObject> {
    // SAFETY: every Objective-C object is an AnyObject.
    unsafe { Retained::cast_unchecked(r) }
}

fn num(v: f64) -> Retained<AnyObject> {
    any(NSNumber::new_f64(v))
}

fn int(v: i64) -> Retained<AnyObject> {
    any(NSNumber::new_i64(v))
}

fn obj(s: &NSString) -> Retained<AnyObject> {
    any(NSString::from_str(&s.to_string()))
}

fn dict(
    pairs: Vec<(&NSString, Retained<AnyObject>)>,
) -> Retained<NSDictionary<NSString, AnyObject>> {
    let keys: Vec<&NSString> = pairs.iter().map(|(k, _)| *k).collect();
    let values: Vec<&AnyObject> = pairs.iter().map(|(_, v)| &**v).collect();
    NSDictionary::from_slices(&keys, &values)
}

fn dict_obj(d: Retained<NSDictionary<NSString, AnyObject>>) -> Retained<AnyObject> {
    any(d)
}

fn writer_error(writer: &AVAssetWriter) -> String {
    // SAFETY: a plain property read.
    unsafe { writer.error() }.map_or_else(
        || "the writer failed".into(),
        |e| e.localizedDescription().to_string(),
    )
}

/// A streaming H.264/MP4 encoder: feed RGBA frames in order, then [`Mp4Encoder::finish`].
pub struct Mp4Encoder {
    writer: Retained<AVAssetWriter>,
    video: Retained<AVAssetWriterInput>,
    adaptor: Retained<AVAssetWriterInputPixelBufferAdaptor>,
    audio: Option<(
        Retained<AVAssetWriterInput>,
        CFRetained<CMAudioFormatDescription>,
    )>,
    w: u32,
    h: u32,
    fps: u32,
    /// Whether the soundtrack has been marked finished (see [`Self::end_audio`]).
    audio_ended: std::cell::Cell<bool>,
    /// Tight NV12 planes, reused frame to frame.
    planes: RefCell<(Vec<u8>, Vec<u8>)>,
}

impl Mp4Encoder {
    /// Create the writer for `path`: H.264 out, NV12 in, and with `audio` an AAC track (the
    /// file is video-only if that can't be set up, see [`Self::has_audio`]).
    pub fn new(
        path: &Path,
        w: u32,
        h: u32,
        fps: u32,
        quality: u8,
        audio: bool,
    ) -> Result<Self, String> {
        if w < 2 || h < 2 || !w.is_multiple_of(2) || !h.is_multiple_of(2) {
            return Err(format!("MP4 needs even dimensions, got {w}x{h}"));
        }
        // AVAssetWriter refuses to replace a file.
        let _ = std::fs::remove_file(path);
        let url = NSURL::fileURLWithPath(&ns(&path.to_string_lossy()));
        let fps = fps.max(1);
        // SAFETY: AVFoundation object construction with well-formed settings dictionaries.
        unsafe {
            let writer =
                AVAssetWriter::assetWriterWithURL_fileType_error(&url, key(AVFileTypeMPEG4))
                    .map_err(|e| e.localizedDescription().to_string())?;
            writer.setShouldOptimizeForNetworkUse(true);

            let compression = dict(vec![
                (
                    key(AVVideoAverageBitRateKey),
                    int(i64::from(bitrate(w, h, fps, quality))),
                ),
                (
                    key(AVVideoProfileLevelKey),
                    obj(key(AVVideoProfileLevelH264HighAutoLevel)),
                ),
                (key(AVVideoMaxKeyFrameIntervalKey), int(i64::from(fps * 2))),
                (key(AVVideoExpectedSourceFrameRateKey), int(i64::from(fps))),
                // No B-frames: VideoToolbox then holds a few frames instead of dozens, and
                // the writer's audio/video interleaving never waits on the encoder.
                (
                    key(AVVideoAllowFrameReorderingKey),
                    any(NSNumber::new_bool(false)),
                ),
            ]);
            let color = dict(vec![
                (
                    key(AVVideoColorPrimariesKey),
                    obj(key(AVVideoColorPrimaries_ITU_R_709_2)),
                ),
                (
                    key(AVVideoTransferFunctionKey),
                    obj(key(AVVideoTransferFunction_ITU_R_709_2)),
                ),
                (
                    key(AVVideoYCbCrMatrixKey),
                    obj(key(AVVideoYCbCrMatrix_ITU_R_709_2)),
                ),
            ]);
            let settings = dict(vec![
                (key(AVVideoCodecKey), obj(key(AVVideoCodecTypeH264))),
                (key(AVVideoWidthKey), int(i64::from(w))),
                (key(AVVideoHeightKey), int(i64::from(h))),
                (key(AVVideoCompressionPropertiesKey), dict_obj(compression)),
                (key(AVVideoColorPropertiesKey), dict_obj(color)),
            ]);
            let video = AVAssetWriterInput::assetWriterInputWithMediaType_outputSettings(
                key(AVMediaTypeVideo),
                Some(&settings),
            );
            video.setExpectsMediaDataInRealTime(false);
            let cf = |k: &objc2_core_foundation::CFString| -> &NSString {
                // SAFETY: CFString and NSString are toll-free bridged.
                &*std::ptr::from_ref(k).cast::<NSString>()
            };
            let source = dict(vec![
                (
                    cf(kCVPixelBufferPixelFormatTypeKey),
                    int(i64::from(kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange)),
                ),
                (cf(kCVPixelBufferWidthKey), int(i64::from(w))),
                (cf(kCVPixelBufferHeightKey), int(i64::from(h))),
            ]);
            let adaptor = AVAssetWriterInputPixelBufferAdaptor::assetWriterInputPixelBufferAdaptorWithAssetWriterInput_sourcePixelBufferAttributes(
                &video,
                Some(&source),
            );
            if !writer.canAddInput(&video) {
                return Err("the MP4 writer won't take an H.264 track of this size".into());
            }
            writer.addInput(&video);

            let audio = if audio {
                Self::audio_input(&writer)
            } else {
                None
            };
            if !writer.startWriting() {
                return Err(writer_error(&writer));
            }
            writer.startSessionAtSourceTime(CMTime::new(0, 1));
            tracing::info!(
                "MP4 encoder ready: {w}x{h} at {fps} fps, audio: {}",
                audio.is_some()
            );
            let luma = (w * h) as usize;
            Ok(Self {
                writer,
                video,
                adaptor,
                audio,
                w,
                h,
                fps,
                audio_ended: std::cell::Cell::new(false),
                planes: RefCell::new((vec![0; luma], vec![0; luma / 2])),
            })
        }
    }

    /// The AAC input and the PCM format fed to it, or `None` if the writer won't take it.
    unsafe fn audio_input(
        writer: &AVAssetWriter,
    ) -> Option<(
        Retained<AVAssetWriterInput>,
        CFRetained<CMAudioFormatDescription>,
    )> {
        const AAC: u32 = u32::from_be_bytes(*b"aac ");
        let settings = dict(vec![
            (&ns("AVFormatIDKey"), int(i64::from(AAC))),
            (&ns("AVSampleRateKey"), num(f64::from(AUDIO_RATE))),
            (&ns("AVNumberOfChannelsKey"), int(2)),
            (&ns("AVEncoderBitRateKey"), int(192_000)),
        ]);
        // SAFETY: as in `new`.
        unsafe {
            let input = AVAssetWriterInput::assetWriterInputWithMediaType_outputSettings(
                key(AVMediaTypeAudio),
                Some(&settings),
            );
            input.setExpectsMediaDataInRealTime(false);
            if !writer.canAddInput(&input) {
                tracing::warn!("MP4 writer won't take an AAC track; exporting without sound");
                return None;
            }
            let format = pcm_format()?;
            writer.addInput(&input);
            Some((input, format))
        }
    }

    /// Whether the file carries a soundtrack.
    pub fn has_audio(&self) -> bool {
        self.audio.is_some()
    }

    /// Wait until `input` takes more data (or the writer fails).
    fn wait_ready(&self, input: &AVAssetWriterInput) -> Result<(), String> {
        let start = Instant::now();
        // SAFETY: plain property reads.
        unsafe {
            while !input.isReadyForMoreMediaData() {
                if self.writer.status() == AVAssetWriterStatus::Failed {
                    return Err(writer_error(&self.writer));
                }
                if start.elapsed() > READY_WAIT {
                    return Err("the video encoder stopped taking frames".into());
                }
                std::thread::sleep(Duration::from_millis(1));
            }
        }
        Ok(())
    }

    /// Encode one RGBA frame of exactly the encoder's size (tightly packed rows).
    pub fn write_rgba(&self, rgba: &[u8], index: u32) -> Result<(), String> {
        let (w, h) = (self.w as usize, self.h as usize);
        if rgba.len() != w * h * 4 {
            return Err("frame is not the encoder's size".into());
        }
        let mut planes = self.planes.borrow_mut();
        let (y, uv) = &mut *planes;
        rgba_to_nv12(rgba, w, y, uv);
        self.wait_ready(&self.video)?;
        // SAFETY: the pool's buffer is locked while its planes are written within bounds.
        unsafe {
            let pool = self
                .adaptor
                .pixelBufferPool()
                .ok_or_else(|| writer_error(&self.writer))?;
            let mut out: *mut CVPixelBuffer = std::ptr::null_mut();
            let status = objc2_core_video::CVPixelBufferPool::create_pixel_buffer(
                None,
                &pool,
                NonNull::from(&mut out),
            );
            if status != 0 || out.is_null() {
                return Err(format!("no pixel buffer for frame {index} ({status})"));
            }
            let buffer: CFRetained<CVPixelBuffer> =
                CFRetained::from_raw(NonNull::new_unchecked(out));
            let flags = CVPixelBufferLockFlags(0);
            objc2_core_video::CVPixelBufferLockBaseAddress(&buffer, flags);
            copy_plane(&buffer, 0, y, w, h);
            copy_plane(&buffer, 1, uv, w, h / 2);
            objc2_core_video::CVPixelBufferUnlockBaseAddress(&buffer, flags);
            tag_bt709(&buffer);
            let at = CMTime::new(i64::from(index), self.fps as i32);
            if !self
                .adaptor
                .appendPixelBuffer_withPresentationTime(&buffer, at)
            {
                return Err(format!(
                    "encode frame {index}: {}",
                    writer_error(&self.writer)
                ));
            }
        }
        Ok(())
    }

    /// Encode interleaved 16-bit stereo samples (48 kHz) starting at output sample frame
    /// `start`. A no-op when the file has no soundtrack.
    pub fn write_audio(&self, samples: &[i16], start: u64) -> Result<(), String> {
        let Some((input, format)) = &self.audio else {
            return Ok(());
        };
        if samples.is_empty() {
            return Ok(());
        }
        let frames = samples.len() / 2;
        let bytes: Vec<u8> = samples.iter().flat_map(|s| s.to_le_bytes()).collect();
        self.wait_ready(input)?;
        // SAFETY: the block buffer owns a copy of `bytes`; the sample buffer retains it.
        unsafe {
            let block = block_buffer(&bytes)?;
            let mut out: *mut CMSampleBuffer = std::ptr::null_mut();
            let at = CMTime::new(start as i64, AUDIO_RATE as i32);
            let format: &CMFormatDescription = format;
            #[allow(deprecated)]
            let status = objc2_core_media::CMAudioSampleBufferCreateReadyWithPacketDescriptions(
                None,
                &block,
                format,
                frames as isize,
                at,
                std::ptr::null(),
                NonNull::from(&mut out),
            );
            if status != 0 || out.is_null() {
                return Err(format!("audio buffer ({status})"));
            }
            let sample: CFRetained<CMSampleBuffer> =
                CFRetained::from_raw(NonNull::new_unchecked(out));
            if !input.appendSampleBuffer(&sample) {
                return Err(format!("encode audio: {}", writer_error(&self.writer)));
            }
        }
        Ok(())
    }

    /// The whole soundtrack has been written. AVAssetWriter interleaves its tracks and
    /// holds video back while it expects more audio; once there is none to come, saying so
    /// lets the last frames through.
    pub fn end_audio(&self) {
        if let Some((input, _)) = &self.audio {
            if !self.audio_ended.replace(true) {
                // SAFETY: finishing an input of a writer that is writing.
                unsafe { input.markAsFinished() };
            }
        }
    }

    /// Flush the encoder and finalize the MP4 container.
    pub fn finish(self) -> Result<(), String> {
        let (tx, rx) = std::sync::mpsc::channel::<()>();
        self.end_audio();
        // SAFETY: finishing a writer we began writing on.
        unsafe {
            self.video.markAsFinished();
            let done = RcBlock::new(move || {
                let _ = tx.send(());
            });
            self.writer.finishWritingWithCompletionHandler(&done);
            let _ = rx.recv_timeout(Duration::from_secs(60));
            if self.writer.status() == AVAssetWriterStatus::Completed {
                Ok(())
            } else {
                Err(writer_error(&self.writer))
            }
        }
    }
}

/// Copy a tight plane (`row` bytes × `rows`) into plane `index` of a locked pixel buffer,
/// whose rows may be padded.
unsafe fn copy_plane(buffer: &CVPixelBuffer, index: usize, src: &[u8], row: usize, rows: usize) {
    let base = objc2_core_video::CVPixelBufferGetBaseAddressOfPlane(buffer, index).cast::<u8>();
    let stride = objc2_core_video::CVPixelBufferGetBytesPerRowOfPlane(buffer, index);
    if base.is_null() || stride < row {
        return;
    }
    for r in 0..rows {
        // SAFETY: row `r` of the plane holds `stride >= row` bytes; `src` holds `row * rows`.
        unsafe {
            std::ptr::copy_nonoverlapping(src.as_ptr().add(r * row), base.add(r * stride), row);
        }
    }
}

/// Mark a frame BT.709 (primaries, transfer, matrix) so the encoder and players agree on
/// its colours.
unsafe fn tag_bt709(buffer: &CVPixelBuffer) {
    use objc2_core_video::{
        kCVImageBufferColorPrimariesKey, kCVImageBufferColorPrimaries_ITU_R_709_2,
        kCVImageBufferTransferFunctionKey, kCVImageBufferTransferFunction_ITU_R_709_2,
        kCVImageBufferYCbCrMatrixKey, kCVImageBufferYCbCrMatrix_ITU_R_709_2, CVAttachmentMode,
    };
    // SAFETY: framework constants set as attachments on a live buffer.
    unsafe {
        for (k, v) in [
            (
                kCVImageBufferColorPrimariesKey,
                kCVImageBufferColorPrimaries_ITU_R_709_2,
            ),
            (
                kCVImageBufferTransferFunctionKey,
                kCVImageBufferTransferFunction_ITU_R_709_2,
            ),
            (
                kCVImageBufferYCbCrMatrixKey,
                kCVImageBufferYCbCrMatrix_ITU_R_709_2,
            ),
        ] {
            buffer.set_attachment(k, v, CVAttachmentMode::ShouldPropagate);
        }
    }
}

/// 48 kHz stereo, 16-bit signed interleaved PCM: what the mixer renders.
fn pcm_format() -> Option<CFRetained<CMAudioFormatDescription>> {
    const LPCM: u32 = u32::from_be_bytes(*b"lpcm");
    const SIGNED_INT: u32 = 1 << 2; // kAudioFormatFlagIsSignedInteger
    const PACKED: u32 = 1 << 3; // kAudioFormatFlagIsPacked
    let asbd = AudioStreamBasicDescription {
        mSampleRate: f64::from(AUDIO_RATE),
        mFormatID: LPCM,
        mFormatFlags: SIGNED_INT | PACKED,
        mBytesPerPacket: 4,
        mFramesPerPacket: 1,
        mBytesPerFrame: 4,
        mChannelsPerFrame: 2,
        mBitsPerChannel: 16,
        mReserved: 0,
    };
    let mut out: *const CMAudioFormatDescription = std::ptr::null();
    // SAFETY: a valid ASBD; no layout, cookie or extensions.
    #[allow(deprecated)]
    let status = unsafe {
        objc2_core_media::CMAudioFormatDescriptionCreate(
            None,
            NonNull::from(&asbd),
            0,
            std::ptr::null(),
            0,
            std::ptr::null(),
            None,
            NonNull::from(&mut out),
        )
    };
    if status != 0 || out.is_null() {
        tracing::warn!("PCM format description failed ({status}); exporting without sound");
        return None;
    }
    // SAFETY: a +1 object from a Create call.
    Some(unsafe { CFRetained::from_raw(NonNull::new_unchecked(out.cast_mut())) })
}

/// A block buffer holding a copy of `bytes`.
unsafe fn block_buffer(bytes: &[u8]) -> Result<CFRetained<CMBlockBuffer>, String> {
    const ASSURE_MEMORY_NOW: u32 = 1; // kCMBlockBufferAssureMemoryNowFlag
    let mut out: *mut CMBlockBuffer = std::ptr::null_mut();
    // SAFETY: CoreMedia allocates the memory block itself (NULL block, default allocators).
    #[allow(deprecated)]
    let status = unsafe {
        objc2_core_media::CMBlockBufferCreateWithMemoryBlock(
            None,
            std::ptr::null_mut(),
            bytes.len(),
            None,
            std::ptr::null(),
            0,
            bytes.len(),
            ASSURE_MEMORY_NOW,
            NonNull::from(&mut out),
        )
    };
    if status != 0 || out.is_null() {
        return Err(format!("audio block ({status})"));
    }
    // SAFETY: a +1 object from a Create call.
    let block: CFRetained<CMBlockBuffer> =
        unsafe { CFRetained::from_raw(NonNull::new_unchecked(out)) };
    // SAFETY: copies `bytes.len()` bytes into the block allocated just above.
    #[allow(deprecated)]
    let status = unsafe {
        objc2_core_media::CMBlockBufferReplaceDataBytes(
            NonNull::new_unchecked(bytes.as_ptr().cast_mut().cast()),
            &block,
            0,
            bytes.len(),
        )
    };
    if status != 0 {
        return Err(format!("audio copy ({status})"));
    }
    Ok(block)
}
