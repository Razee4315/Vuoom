//! macOS audio capture: microphones through Core Audio (`cpal`), everything the Mac plays
//! through ScreenCaptureKit's audio output (macOS 13+, needs the Screen Recording
//! permission, and leaves Vuoom's own sounds out).
//!
//! Same contract as the WASAPI backend: every packet carries the host-clock time of its
//! first sample (the clock the frames and input events use), and the WAV is laid out on the
//! recording timeline with [`super::placement`], so gaps become silence and overlaps are
//! dropped.

use super::{bump_level, convert, placement, Anchor, Device, SampleKind, Source};
use crate::wav::WavWriter;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::time::Duration;

/// One block of captured audio: interleaved f32 samples and the host-clock time (ns) of
/// its first frame.
struct Packet {
    samples: Vec<f32>,
    channels: usize,
    ns: i64,
}

/// What keeps a capture running; dropping it stops the capture (never read, only held).
#[allow(dead_code)]
enum Guard {
    Mic(cpal::Stream),
    System(sck::SystemAudio),
}

pub fn list_inputs() -> Result<Vec<Device>, String> {
    use cpal::traits::{DeviceTrait, HostTrait};
    let host = cpal::default_host();
    let default = host.default_input_device().and_then(|d| d.name().ok());
    let devices = host
        .input_devices()
        .map_err(|e| format!("audio devices: {e}"))?;
    Ok(devices
        .filter_map(|d| d.name().ok())
        .map(|name| Device {
            is_default: default.as_deref() == Some(name.as_str()),
            id: name.clone(),
            name,
        })
        .collect())
}

/// System sound comes from ScreenCaptureKit, which doesn't need an output device.
pub fn has_output() -> bool {
    true
}

fn mic(id: Option<&str>, tx: Sender<Packet>) -> Result<(Guard, u32), String> {
    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
    let host = cpal::default_host();
    let device = match id {
        Some(id) => host
            .input_devices()
            .map_err(|e| format!("audio devices: {e}"))?
            .find(|d| d.name().is_ok_and(|n| n == id))
            .or_else(|| host.default_input_device()),
        None => host.default_input_device(),
    }
    .ok_or("no microphone found")?;
    let config = device
        .default_input_config()
        .map_err(|e| format!("microphone format: {e}"))?;
    let rate = config.sample_rate().0;
    let channels = usize::from(config.channels());
    let clock = vuoom_input::Clock::new();
    // The capture instant: now, less how long ago the device took the first sample.
    let stamp = move |info: &cpal::InputCallbackInfo| {
        let t = info.timestamp();
        let latency = t.callback.duration_since(&t.capture).unwrap_or_default();
        clock.now() - latency.as_nanos() as i64
    };
    let err = |e: cpal::StreamError| tracing::warn!("microphone stream: {e}");
    let stream_config = config.config();
    let stream = match config.sample_format() {
        cpal::SampleFormat::F32 => device.build_input_stream(
            &stream_config,
            move |data: &[f32], info: &_| {
                let _ = tx.send(Packet {
                    samples: data.to_vec(),
                    channels,
                    ns: stamp(info),
                });
            },
            err,
            None,
        ),
        cpal::SampleFormat::I16 => device.build_input_stream(
            &stream_config,
            move |data: &[i16], info: &_| {
                let _ = tx.send(Packet {
                    samples: data.iter().map(|&s| f32::from(s) / 32768.0).collect(),
                    channels,
                    ns: stamp(info),
                });
            },
            err,
            None,
        ),
        cpal::SampleFormat::I32 => device.build_input_stream(
            &stream_config,
            move |data: &[i32], info: &_| {
                let _ = tx.send(Packet {
                    samples: data.iter().map(|&s| s as f32 / 2_147_483_648.0).collect(),
                    channels,
                    ns: stamp(info),
                });
            },
            err,
            None,
        ),
        other => return Err(format!("unsupported microphone format ({other})")),
    }
    .map_err(|e| format!("microphone unavailable ({e})"))?;
    stream
        .play()
        .map_err(|e| format!("microphone start: {e}"))?;
    Ok((Guard::Mic(stream), rate))
}

pub fn run(
    source: &Source,
    out: Option<PathBuf>,
    anchor: Anchor,
    stop: &AtomicBool,
    level: &AtomicU32,
    ready: &Sender<Result<(), String>>,
) -> Result<u64, String> {
    let (tx, rx) = channel::<Packet>();
    let started = match source {
        Source::Mic(id) => mic(id.as_deref(), tx),
        Source::System => sck::SystemAudio::start(tx).map(|s| (Guard::System(s), sck::RATE)),
    };
    let (guard, rate) = match started {
        Ok(s) => s,
        Err(e) => {
            let _ = ready.send(Err(e.clone()));
            return Err(e);
        }
    };
    let out_ch = source.out_channels();
    let mut writer = match out.map(|p| WavWriter::create(&p, rate, out_ch)) {
        Some(Ok(w)) => Some(w),
        Some(Err(e)) => {
            let _ = ready.send(Err(e.clone()));
            return Err(e);
        }
        None => None,
    };
    let _ = ready.send(Ok(()));
    let failure = pump(&rx, stop, level, writer.as_mut(), anchor, rate, out_ch);
    drop(guard);
    let frames = match writer {
        Some(w) => w.finish()?,
        None => 0,
    };
    match failure {
        Some(e) => Err(e),
        None => Ok(frames),
    }
}

/// Lay packets onto the timeline until `stop`. Returns why writing failed, if it did.
fn pump(
    rx: &Receiver<Packet>,
    stop: &AtomicBool,
    level: &AtomicU32,
    mut writer: Option<&mut WavWriter>,
    anchor: Anchor,
    rate: u32,
    out_ch: u16,
) -> Option<String> {
    let mut first = true;
    loop {
        let stopping = stop.load(Ordering::Relaxed);
        // Take whatever has arrived; after a stop, drain what's left and finish.
        let packet = match rx.recv_timeout(Duration::from_millis(10)) {
            Ok(p) => p,
            Err(_) if stopping => return None,
            Err(_) => continue,
        };
        let bytes: Vec<u8> = packet
            .samples
            .iter()
            .flat_map(|s| s.to_le_bytes())
            .collect();
        let samples = convert(&bytes, SampleKind::F32, packet.channels, out_ch);
        bump_level(level, crate::mix::peak(&samples));
        let Some(w) = writer.as_deref_mut() else {
            continue;
        };
        // Anchor times are in 100 ns units of the counter (how WASAPI reports it).
        let t = anchor.seconds_to((packet.ns.max(0) / 100) as u64);
        let target = (t * f64::from(rate)).round() as i64;
        let (pad, skip) = placement(target, w.frames(), first, rate);
        if pad > 0 {
            if let Err(e) = w.write_silence(pad) {
                return Some(e);
            }
        }
        let from = (skip as usize * usize::from(out_ch)).min(samples.len());
        if from < samples.len() {
            first = false;
            if let Err(e) = w.write(&samples[from..]) {
                return Some(e);
            }
        }
    }
}

/// System sound through a ScreenCaptureKit stream that only asks for audio.
mod sck {
    use super::Packet;
    use block2::RcBlock;
    use dispatch2::DispatchQueue;
    use objc2::rc::Retained;
    use objc2::runtime::{NSObject, NSObjectProtocol, ProtocolObject};
    use objc2::{define_class, msg_send, AllocAnyThread, DefinedClass};
    use objc2_core_audio_types::{AudioBuffer, AudioStreamBasicDescription};
    use objc2_core_media::{CMBlockBuffer, CMSampleBuffer, CMTime};
    use objc2_foundation::{NSArray, NSError};
    use objc2_screen_capture_kit::{
        SCContentFilter, SCShareableContent, SCStream, SCStreamConfiguration, SCStreamOutput,
        SCStreamOutputType, SCWindow,
    };
    use std::sync::mpsc::{channel, Sender};
    use std::sync::Mutex;
    use std::time::Duration;

    /// The rate ScreenCaptureKit is asked for (the mixer's output rate: no resampling).
    pub const RATE: u32 = 48_000;
    const NON_INTERLEAVED: u32 = 1 << 5; // kAudioFormatFlagIsNonInterleaved
    const FLOAT: u32 = 1 << 0; // kAudioFormatFlagIsFloat

    struct Sendable<T>(T);
    // SAFETY: retained SCK objects handed over from a completion handler.
    unsafe impl<T> Send for Sendable<T> {}

    fn describe(err: *mut NSError) -> String {
        if err.is_null() {
            "unknown error".into()
        } else {
            // SAFETY: a live NSError for the handler's duration.
            unsafe { &*err }.localizedDescription().to_string()
        }
    }

    /// An `AudioBufferList` with room for up to eight channels.
    #[repr(C)]
    struct Buffers {
        count: u32,
        buffers: [AudioBuffer; 8],
    }

    define_class!(
        // SAFETY: NSObject has no subclassing requirements.
        #[unsafe(super(NSObject))]
        #[name = "VuoomSystemAudioOutput"]
        #[ivars = Mutex<Sender<Packet>>]
        struct AudioOutput;

        unsafe impl NSObjectProtocol for AudioOutput {}

        unsafe impl SCStreamOutput for AudioOutput {
            #[unsafe(method(stream:didOutputSampleBuffer:ofType:))]
            fn stream_did_output(
                &self,
                _stream: &SCStream,
                sample: &CMSampleBuffer,
                kind: SCStreamOutputType,
            ) {
                if kind != SCStreamOutputType::Audio {
                    return;
                }
                if let Some(packet) = read(sample) {
                    if let Ok(tx) = self.ivars().lock() {
                        let _ = tx.send(packet);
                    }
                }
            }
        }
    );

    /// Decode one audio sample buffer (float, interleaved or planar) to a packet.
    fn read(sample: &CMSampleBuffer) -> Option<Packet> {
        // SAFETY: reads of a live sample buffer; the block buffer keeps the data alive
        // while the slices are read, and is released at the end.
        unsafe {
            let format = sample.format_description()?;
            #[allow(deprecated)]
            let asbd: *const AudioStreamBasicDescription =
                objc2_core_media::CMAudioFormatDescriptionGetStreamBasicDescription(&format);
            let asbd = asbd.as_ref()?;
            if asbd.mFormatFlags & FLOAT == 0 || asbd.mBitsPerChannel != 32 {
                return None;
            }
            let channels = asbd.mChannelsPerFrame.max(1) as usize;
            let mut list = Buffers {
                count: 0,
                buffers: [AudioBuffer {
                    mNumberChannels: 0,
                    mDataByteSize: 0,
                    mData: std::ptr::null_mut(),
                }; 8],
            };
            // CoreMedia wants the list's exact size: ask first, then fill.
            let mut needed = 0usize;
            let _ = sample.audio_buffer_list_with_retained_block_buffer(
                &mut needed,
                std::ptr::null_mut(),
                0,
                None,
                None,
                0,
                std::ptr::null_mut(),
            );
            if needed == 0 || needed > std::mem::size_of::<Buffers>() {
                return None;
            }
            let mut block: *mut CMBlockBuffer = std::ptr::null_mut();
            let status = sample.audio_buffer_list_with_retained_block_buffer(
                std::ptr::null_mut(),
                std::ptr::from_mut(&mut list).cast(),
                needed,
                None,
                None,
                0,
                &mut block,
            );
            if status != 0 {
                return None;
            }
            let block = std::ptr::NonNull::new(block)
                .map(|b| objc2_core_foundation::CFRetained::from_raw(b));
            let n = (list.count as usize).min(8);
            let slice = |b: &AudioBuffer| {
                if b.mData.is_null() {
                    &[][..]
                } else {
                    std::slice::from_raw_parts(b.mData.cast::<f32>(), b.mDataByteSize as usize / 4)
                }
            };
            let samples = if asbd.mFormatFlags & NON_INTERLEAVED != 0 && n > 1 {
                let planes: Vec<&[f32]> = list.buffers[..n].iter().map(slice).collect();
                let frames = planes.iter().map(|p| p.len()).min().unwrap_or(0);
                let mut out = Vec::with_capacity(frames * n);
                for f in 0..frames {
                    out.extend(planes.iter().map(|p| p[f]));
                }
                out
            } else {
                slice(&list.buffers[0]).to_vec()
            };
            drop(block);
            let channels = if asbd.mFormatFlags & NON_INTERLEAVED != 0 {
                n.max(1)
            } else {
                channels
            };
            let pts: CMTime = sample.presentation_time_stamp();
            let ns = (pts.seconds() * 1e9) as i64;
            Some(Packet {
                samples,
                channels,
                ns,
            })
        }
    }

    /// A running system-sound capture; dropping it stops the stream.
    pub struct SystemAudio {
        stream: Retained<SCStream>,
        _output: Retained<AudioOutput>,
        _queue: dispatch2::DispatchRetained<DispatchQueue>,
    }

    fn wait(call: impl FnOnce(&block2::DynBlock<dyn Fn(*mut NSError)>)) -> Result<(), String> {
        let (tx, rx) = channel::<Result<(), String>>();
        let block = RcBlock::new(move |err: *mut NSError| {
            let _ = tx.send(if err.is_null() {
                Ok(())
            } else {
                Err(describe(err))
            });
        });
        call(&block);
        rx.recv_timeout(Duration::from_secs(6))
            .unwrap_or_else(|_| Err("ScreenCaptureKit did not answer".into()))
    }

    impl SystemAudio {
        pub fn start(tx: Sender<Packet>) -> Result<Self, String> {
            if !objc2_core_graphics::CGPreflightScreenCaptureAccess() {
                return Err(
                    "Recording the Mac's sound needs the Screen Recording permission".into(),
                );
            }
            let (ctx, crx) = channel::<Result<Sendable<Retained<SCShareableContent>>, String>>();
            let block = RcBlock::new(move |c: *mut SCShareableContent, e: *mut NSError| {
                // SAFETY: a live content object when non-null.
                let got = unsafe { Retained::retain(c) };
                let _ = ctx.send(got.map(Sendable).ok_or_else(|| describe(e)));
            });
            // SAFETY: the block outlives the call (SCK copies it).
            unsafe {
                SCShareableContent::getShareableContentExcludingDesktopWindows_onScreenWindowsOnly_completionHandler(
                    true, true, &block,
                );
            }
            let content = crx
                .recv_timeout(Duration::from_secs(6))
                .map_err(|_| "ScreenCaptureKit did not answer".to_string())??
                .0;
            // SAFETY: SCK construction and control; the output and queue outlive the stream.
            unsafe {
                let display = content
                    .displays()
                    .firstObject()
                    .ok_or("no display to attach system sound to")?;
                let none = NSArray::<SCWindow>::new();
                let filter = SCContentFilter::initWithDisplay_excludingWindows(
                    SCContentFilter::alloc(),
                    &display,
                    &none,
                );
                let config = SCStreamConfiguration::new();
                // Audio is all that's wanted: the smallest, slowest picture possible.
                config.setWidth(2);
                config.setHeight(2);
                config.setMinimumFrameInterval(CMTime::new(1, 1));
                config.setQueueDepth(3);
                config.setCapturesAudio(true);
                config.setSampleRate(RATE as isize);
                config.setChannelCount(2);
                config.setExcludesCurrentProcessAudio(true);
                let output = {
                    let this = AudioOutput::alloc().set_ivars(Mutex::new(tx));
                    let o: Retained<AudioOutput> = msg_send![super(this), init];
                    o
                };
                let queue = DispatchQueue::new("dev.vuoom.system-audio", None);
                let stream = SCStream::initWithFilter_configuration_delegate(
                    SCStream::alloc(),
                    &filter,
                    &config,
                    None,
                );
                // A screen output too, so the stream has somewhere to drop its frames
                // instead of logging each one.
                for kind in [SCStreamOutputType::Audio, SCStreamOutputType::Screen] {
                    stream
                        .addStreamOutput_type_sampleHandlerQueue_error(
                            ProtocolObject::from_ref(&*output),
                            kind,
                            Some(&queue),
                        )
                        .map_err(|e| e.localizedDescription().to_string())?;
                }
                wait(|b| stream.startCaptureWithCompletionHandler(Some(b)))?;
                Ok(Self {
                    stream,
                    _output: output,
                    _queue: queue,
                })
            }
        }
    }

    impl Drop for SystemAudio {
        fn drop(&mut self) {
            // SAFETY: stopping our own running stream.
            let _ = wait(|b| unsafe { self.stream.stopCaptureWithCompletionHandler(Some(b)) });
        }
    }
}
