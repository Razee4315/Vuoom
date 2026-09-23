//! WASAPI capture of a microphone or of everything the speakers play (loopback).
//!
//! Each capture runs on its own thread in shared mode, polling the endpoint every 10 ms.
//! Every packet carries the performance-counter time of its first sample, which is the same
//! clock the video frames and input events are stamped with, so the WAV is laid out on the
//! recording timeline directly: sample 0 sits at the recording's start instant, gaps (a
//! loopback endpoint delivers nothing while the system is silent) become silence, and
//! overlaps are dropped. Small clock drift is left alone; only jumps beyond
//! [`RESYNC_SECS`] are corrected, so a long take never gets periodic clicks.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;

/// Which endpoint to record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    /// A microphone by endpoint id; `None` is the system default input.
    Mic(Option<String>),
    /// Everything the default output device plays.
    System,
}

impl Source {
    /// Channels stored on disk: voice is mono, system sound keeps its stereo image.
    #[must_use]
    pub fn out_channels(&self) -> u16 {
        match self {
            Self::Mic(_) => 1,
            Self::System => 2,
        }
    }
}

/// The recording's time origin on the performance-counter clock.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Anchor {
    pub start_qpc: i64,
    pub freq: i64,
}

impl Anchor {
    /// Seconds from the anchor to a packet time given in 100 ns units (how WASAPI reports
    /// the counter).
    #[must_use]
    pub fn seconds_to(&self, qpc_100ns: u64) -> f64 {
        qpc_100ns as f64 / 1e7 - self.start_qpc as f64 / self.freq.max(1) as f64
    }
}

/// An input device for the picker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Device {
    pub id: String,
    pub name: String,
    pub is_default: bool,
}

/// Timing jumps larger than this are corrected with silence or by dropping overlap.
pub const RESYNC_SECS: f64 = 0.06;

/// Where the next packet's samples go relative to what's already written: frames of
/// silence to insert first, and frames to drop from the packet's start.
#[must_use]
pub fn placement(target: i64, written: u64, first: bool, rate: u32) -> (u64, u64) {
    let written = written as i64;
    if target < 0 {
        // Captured before the recording started: drop what precedes time zero.
        return (0, target.unsigned_abs());
    }
    let tol = if first {
        0
    } else {
        (RESYNC_SECS * f64::from(rate)) as i64
    };
    let diff = target - written;
    if diff > tol {
        (diff as u64, 0)
    } else if diff < -tol {
        (0, diff.unsigned_abs())
    } else {
        (0, 0)
    }
}

/// A running capture. Dropping it stops the capture (and finalizes the file).
pub struct Recorder {
    stop: Arc<AtomicBool>,
    level: Arc<AtomicU32>,
    thread: Option<JoinHandle<Result<u64, String>>>,
}

impl Recorder {
    /// Start capturing `source`. With `out`, audio is written there as a 16-bit WAV whose
    /// first sample sits at `anchor`; without, only the level meter runs (a mic check).
    ///
    /// # Errors
    /// Returns a message if the endpoint can't be opened or started.
    pub fn start(source: Source, out: Option<PathBuf>, anchor: Anchor) -> Result<Self, String> {
        let stop = Arc::new(AtomicBool::new(false));
        let level = Arc::new(AtomicU32::new(0));
        let (ready_tx, ready_rx) = std::sync::mpsc::channel();
        let thread = {
            let stop = Arc::clone(&stop);
            let level = Arc::clone(&level);
            std::thread::Builder::new()
                .name("vuoom-audio".into())
                .spawn(move || imp::run(&source, out, anchor, &stop, &level, &ready_tx))
                .map_err(|e| format!("audio thread: {e}"))?
        };
        match ready_rx.recv_timeout(std::time::Duration::from_secs(5)) {
            Ok(Ok(())) => Ok(Self {
                stop,
                level,
                thread: Some(thread),
            }),
            Ok(Err(e)) => {
                let _ = thread.join();
                Err(e)
            }
            Err(_) => {
                stop.store(true, Ordering::Relaxed);
                Err("audio device did not start".into())
            }
        }
    }

    /// Peak level (0..1) since the previous call.
    #[must_use]
    pub fn level(&self) -> f32 {
        f32::from_bits(self.level.swap(0, Ordering::Relaxed))
    }

    /// Stop capturing and finalize the file. Returns the frames written.
    ///
    /// # Errors
    /// Returns a message if the capture failed or the file couldn't be finalized.
    pub fn finish(mut self) -> Result<u64, String> {
        self.stop.store(true, Ordering::Relaxed);
        match self.thread.take() {
            Some(t) => t.join().map_err(|_| "audio thread panicked".to_string())?,
            None => Ok(0),
        }
    }
}

impl Drop for Recorder {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

/// Record a packet's peak into the shared meter (monotonic max until read).
fn bump_level(level: &AtomicU32, peak: f32) {
    // Non-negative f32 bit patterns order like the numbers, so an integer max works.
    level.fetch_max(peak.max(0.0).to_bits(), Ordering::Relaxed);
}

/// Sample layouts WASAPI shared mode hands out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SampleKind {
    F32,
    I16,
    I24,
    I32,
}

impl SampleKind {
    #[must_use]
    pub fn bytes(self) -> usize {
        match self {
            Self::I16 => 2,
            Self::I24 => 3,
            Self::F32 | Self::I32 => 4,
        }
    }

    fn read(self, b: &[u8]) -> f32 {
        match self {
            Self::F32 => f32::from_le_bytes([b[0], b[1], b[2], b[3]]),
            Self::I16 => f32::from(i16::from_le_bytes([b[0], b[1]])) / 32768.0,
            Self::I24 => (i32::from_le_bytes([0, b[0], b[1], b[2]]) >> 8) as f32 / 8_388_608.0,
            Self::I32 => i32::from_le_bytes([b[0], b[1], b[2], b[3]]) as f32 / 2_147_483_648.0,
        }
    }
}

/// Convert one packet of device-format audio to 16-bit samples with `out_ch` channels
/// (1: average of all inputs; 2: front left/right with any center mixed in).
#[must_use]
pub fn convert(data: &[u8], kind: SampleKind, in_ch: usize, out_ch: u16) -> Vec<i16> {
    let in_ch = in_ch.max(1);
    let frame = kind.bytes() * in_ch;
    let mut out = Vec::with_capacity(data.len() / frame.max(1) * usize::from(out_ch));
    for f in data.chunks_exact(frame) {
        let ch = |c: usize| kind.read(&f[c * kind.bytes()..]);
        if out_ch == 1 {
            let sum: f32 = (0..in_ch).map(ch).sum();
            out.push(crate::mix::to_i16(sum / in_ch as f32));
        } else if in_ch == 1 {
            let v = crate::mix::to_i16(ch(0));
            out.extend([v, v]);
        } else {
            let center = if in_ch >= 3 { ch(2) * 0.707 } else { 0.0 };
            out.push(crate::mix::to_i16(ch(0) + center));
            out.push(crate::mix::to_i16(ch(1) + center));
        }
    }
    out
}

#[cfg(windows)]
mod imp {
    use super::{bump_level, convert, placement, Anchor, Device, SampleKind, Source};
    use crate::wav::WavWriter;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
    use std::sync::mpsc::Sender;
    use std::time::Duration;
    use windows::core::{GUID, PCWSTR};
    use windows::Win32::Devices::FunctionDiscovery::PKEY_Device_FriendlyName;
    use windows::Win32::Media::Audio::{
        eCapture, eConsole, eRender, IAudioCaptureClient, IAudioClient, IMMDevice,
        IMMDeviceEnumerator, MMDeviceEnumerator, AUDCLNT_BUFFERFLAGS_SILENT,
        AUDCLNT_BUFFERFLAGS_TIMESTAMP_ERROR, AUDCLNT_SHAREMODE_SHARED,
        AUDCLNT_STREAMFLAGS_LOOPBACK, DEVICE_STATE_ACTIVE, WAVEFORMATEX, WAVEFORMATEXTENSIBLE,
    };
    use windows::Win32::System::Com::StructuredStorage::{
        PropVariantClear, PropVariantToStringAlloc,
    };
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoTaskMemFree, CLSCTX_ALL, COINIT_MULTITHREADED,
        STGM_READ,
    };

    const FORMAT_PCM: u16 = 1;
    const FORMAT_FLOAT: u16 = 3;
    const FORMAT_EXTENSIBLE: u16 = 0xFFFE;
    const SUBTYPE_PCM: GUID = GUID::from_u128(0x0000_0001_0000_0010_8000_00aa_0038_9b71);
    const SUBTYPE_FLOAT: GUID = GUID::from_u128(0x0000_0003_0000_0010_8000_00aa_0038_9b71);
    /// Endpoint buffer: generous, so a 10 ms poll can never overflow it.
    const BUFFER_HNS: i64 = 10_000_000;

    fn com() {
        // SAFETY: per-thread COM init; an already-initialized apartment is fine.
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        }
    }

    fn enumerator() -> Result<IMMDeviceEnumerator, String> {
        com();
        // SAFETY: plain COM activation of the system device enumerator.
        let en = unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) };
        en.map_err(|e| format!("audio devices: {e}"))
    }

    fn device_id(d: &IMMDevice) -> Option<String> {
        // SAFETY: GetId returns a CoTaskMem string we own and free.
        unsafe {
            let p = d.GetId().ok()?;
            let s = p.to_string().ok();
            CoTaskMemFree(Some(p.0 as *const _));
            s
        }
    }

    fn device_name(d: &IMMDevice) -> Option<String> {
        // SAFETY: the property value is cleared and the string freed after use.
        unsafe {
            let store = d.OpenPropertyStore(STGM_READ).ok()?;
            let mut v = store.GetValue(&PKEY_Device_FriendlyName).ok()?;
            let name = PropVariantToStringAlloc(&v).ok().and_then(|p| {
                let s = p.to_string().ok();
                CoTaskMemFree(Some(p.0 as *const _));
                s
            });
            let _ = PropVariantClear(&mut v);
            name
        }
    }

    pub fn list_inputs() -> Result<Vec<Device>, String> {
        let en = enumerator()?;
        // SAFETY: COM calls on a live enumerator; every returned interface is owned.
        unsafe {
            let default = en
                .GetDefaultAudioEndpoint(eCapture, eConsole)
                .ok()
                .and_then(|d| device_id(&d));
            let all = en
                .EnumAudioEndpoints(eCapture, DEVICE_STATE_ACTIVE)
                .map_err(|e| format!("audio devices: {e}"))?;
            let count = all.GetCount().map_err(|e| e.to_string())?;
            let mut out = Vec::new();
            for i in 0..count {
                let Ok(d) = all.Item(i) else { continue };
                let Some(id) = device_id(&d) else { continue };
                let name = device_name(&d).unwrap_or_else(|| "Microphone".into());
                let is_default = default.as_deref() == Some(id.as_str());
                out.push(Device {
                    id,
                    name,
                    is_default,
                });
            }
            Ok(out)
        }
    }

    pub fn has_output() -> bool {
        enumerator().is_ok_and(|en| {
            // SAFETY: query only.
            let device = unsafe { en.GetDefaultAudioEndpoint(eRender, eConsole) };
            device.is_ok()
        })
    }

    fn open(source: &Source) -> Result<IMMDevice, String> {
        let en = enumerator()?;
        // SAFETY: COM calls on a live enumerator; the id buffer outlives GetDevice.
        let device = unsafe {
            match source {
                Source::System => en.GetDefaultAudioEndpoint(eRender, eConsole),
                Source::Mic(None) => en.GetDefaultAudioEndpoint(eCapture, eConsole),
                Source::Mic(Some(id)) => {
                    let wide: Vec<u16> = id.encode_utf16().chain([0]).collect();
                    en.GetDevice(PCWSTR(wide.as_ptr()))
                }
            }
        };
        device.map_err(|e| match source {
            Source::System => format!("no output device for system audio ({e})"),
            Source::Mic(_) => format!("microphone unavailable ({e})"),
        })
    }

    /// Read a device mix format (the struct is packed, so it's copied out unaligned).
    ///
    /// # Safety
    /// `p` must point at a valid `WAVEFORMATEX` (extended when its tag says so).
    unsafe fn read_format(p: *const WAVEFORMATEX) -> (WAVEFORMATEX, Option<GUID>) {
        // SAFETY: guaranteed by the caller.
        unsafe {
            let f = std::ptr::read_unaligned(p);
            let tag = f.wFormatTag;
            let sub = (tag == FORMAT_EXTENSIBLE).then(|| {
                let ext = std::ptr::read_unaligned(p.cast::<WAVEFORMATEXTENSIBLE>());
                ext.SubFormat
            });
            (f, sub)
        }
    }

    /// The sample layout of a mix format: rate, channels, sample kind.
    fn parse_format(
        f: WAVEFORMATEX,
        sub: Option<GUID>,
    ) -> Result<(u32, usize, SampleKind), String> {
        let tag = f.wFormatTag;
        let bits = f.wBitsPerSample;
        let rate = f.nSamplesPerSec;
        let channels = usize::from(f.nChannels);
        let float = match (tag, sub) {
            (FORMAT_FLOAT, _) => true,
            (FORMAT_PCM, _) => false,
            (FORMAT_EXTENSIBLE, Some(s)) if s == SUBTYPE_FLOAT => true,
            (FORMAT_EXTENSIBLE, Some(s)) if s == SUBTYPE_PCM => false,
            _ => return Err("unsupported audio format".into()),
        };
        let kind = match (float, bits) {
            (true, 32) => SampleKind::F32,
            (false, 16) => SampleKind::I16,
            (false, 24) => SampleKind::I24,
            (false, 32) => SampleKind::I32,
            _ => return Err(format!("unsupported audio sample size ({bits} bit)")),
        };
        if rate == 0 || channels == 0 {
            return Err("invalid audio format".into());
        }
        Ok((rate, channels, kind))
    }

    struct Stream {
        client: IAudioClient,
        capture: IAudioCaptureClient,
        rate: u32,
        channels: usize,
        kind: SampleKind,
    }

    fn start_stream(source: &Source) -> Result<Stream, String> {
        let device = open(source)?;
        let flags = if *source == Source::System {
            AUDCLNT_STREAMFLAGS_LOOPBACK
        } else {
            0
        };
        // SAFETY: standard shared-mode WASAPI setup; the mix format is freed after use.
        unsafe {
            let client: IAudioClient = device
                .Activate(CLSCTX_ALL, None)
                .map_err(|e| format!("audio client: {e}"))?;
            let fmt = client
                .GetMixFormat()
                .map_err(|e| format!("audio format: {e}"))?;
            let (head, sub) = read_format(fmt);
            let parsed = parse_format(head, sub);
            let init = client.Initialize(AUDCLNT_SHAREMODE_SHARED, flags, BUFFER_HNS, 0, fmt, None);
            CoTaskMemFree(Some(fmt as *const _));
            let (rate, channels, kind) = parsed?;
            init.map_err(|e| format!("audio init: {e}"))?;
            let capture: IAudioCaptureClient = client
                .GetService()
                .map_err(|e| format!("audio capture: {e}"))?;
            client.Start().map_err(|e| format!("audio start: {e}"))?;
            Ok(Stream {
                client,
                capture,
                rate,
                channels,
                kind,
            })
        }
    }

    pub fn run(
        source: &Source,
        out: Option<PathBuf>,
        anchor: Anchor,
        stop: &AtomicBool,
        level: &AtomicU32,
        ready: &Sender<Result<(), String>>,
    ) -> Result<u64, String> {
        com();
        let stream = match start_stream(source) {
            Ok(s) => s,
            Err(e) => {
                let _ = ready.send(Err(e.clone()));
                return Err(e);
            }
        };
        let out_ch = source.out_channels();
        let mut writer = match out.map(|p| WavWriter::create(&p, stream.rate, out_ch)) {
            Some(Ok(w)) => Some(w),
            Some(Err(e)) => {
                // SAFETY: stopping a started client.
                let _ = unsafe { stream.client.Stop() };
                let _ = ready.send(Err(e.clone()));
                return Err(e);
            }
            None => None,
        };
        let _ = ready.send(Ok(()));

        let silent = AUDCLNT_BUFFERFLAGS_SILENT.0 as u32;
        let bad_time = AUDCLNT_BUFFERFLAGS_TIMESTAMP_ERROR.0 as u32;
        let frame_bytes = stream.kind.bytes() * stream.channels;
        let mut first = true;
        let mut failure: Option<String> = None;
        'capture: loop {
            let stopping = stop.load(Ordering::Relaxed);
            std::thread::sleep(Duration::from_millis(10));
            loop {
                // SAFETY: the buffer is read within its reported length and released.
                let packet = unsafe {
                    match stream.capture.GetNextPacketSize() {
                        Ok(0) => break,
                        Ok(_) => {}
                        Err(e) => {
                            // Device unplugged or invalidated: keep what was captured.
                            tracing::warn!("audio capture ended: {e}");
                            break 'capture;
                        }
                    }
                    let mut data = std::ptr::null_mut();
                    let mut frames = 0u32;
                    let mut flags = 0u32;
                    let mut qpc = 0u64;
                    if let Err(e) = stream.capture.GetBuffer(
                        &mut data,
                        &mut frames,
                        &mut flags,
                        None,
                        Some(std::ptr::addr_of_mut!(qpc)),
                    ) {
                        tracing::warn!("audio capture ended: {e}");
                        break 'capture;
                    }
                    let len = frames as usize * frame_bytes;
                    let samples = if flags & silent != 0 || data.is_null() {
                        vec![0i16; frames as usize * usize::from(out_ch)]
                    } else {
                        let bytes = std::slice::from_raw_parts(data, len);
                        convert(bytes, stream.kind, stream.channels, out_ch)
                    };
                    let _ = stream.capture.ReleaseBuffer(frames);
                    (samples, flags & bad_time == 0, qpc)
                };
                let (samples, timed, qpc) = packet;
                bump_level(level, crate::mix::peak(&samples));
                let Some(w) = writer.as_mut() else { continue };
                let mut from = 0usize;
                if timed {
                    let t = anchor.seconds_to(qpc);
                    let target = (t * f64::from(stream.rate)).round() as i64;
                    let (pad, skip) = placement(target, w.frames(), first, stream.rate);
                    if pad > 0 {
                        if let Err(e) = w.write_silence(pad) {
                            failure = Some(e);
                            break 'capture;
                        }
                    }
                    from = (skip as usize * usize::from(out_ch)).min(samples.len());
                }
                if from < samples.len() {
                    first = false;
                    if let Err(e) = w.write(&samples[from..]) {
                        failure = Some(e);
                        break 'capture;
                    }
                }
            }
            if stopping {
                break;
            }
        }
        // SAFETY: stopping a started client.
        let _ = unsafe { stream.client.Stop() };
        let frames = match writer {
            Some(w) => w.finish()?,
            None => 0,
        };
        match failure {
            Some(e) => Err(e),
            None => Ok(frames),
        }
    }
}

#[cfg(not(windows))]
mod imp {
    use super::{Anchor, Device, Source};
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicBool, AtomicU32};
    use std::sync::mpsc::Sender;

    pub fn list_inputs() -> Result<Vec<Device>, String> {
        Ok(Vec::new())
    }

    pub fn has_output() -> bool {
        false
    }

    pub fn run(
        _source: &Source,
        _out: Option<PathBuf>,
        _anchor: Anchor,
        _stop: &AtomicBool,
        _level: &AtomicU32,
        ready: &Sender<Result<(), String>>,
    ) -> Result<u64, String> {
        let e = "audio capture is Windows-only".to_string();
        let _ = ready.send(Err(e.clone()));
        Err(e)
    }
}

/// Active microphones, default first.
///
/// # Errors
/// Returns a message if the device list can't be read.
pub fn list_inputs() -> Result<Vec<Device>, String> {
    let mut all = imp::list_inputs()?;
    all.sort_by_key(|d| !d.is_default);
    Ok(all)
}

/// Whether a default output device exists (system audio needs one).
#[must_use]
pub fn has_output() -> bool {
    imp::has_output()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_packet_is_placed_exactly() {
        // 30 ms after the start at 1 kHz: pad 30 frames, even though that's under the
        // resync tolerance.
        assert_eq!(placement(30, 0, true, 1_000), (30, 0));
        assert_eq!(placement(30, 0, false, 1_000), (0, 0));
    }

    #[test]
    fn audio_from_before_the_start_is_dropped() {
        assert_eq!(placement(-25, 0, true, 1_000), (0, 25));
    }

    #[test]
    fn gaps_become_silence_and_overlaps_are_dropped() {
        // Loopback delivered nothing for a second.
        assert_eq!(placement(2_000, 1_000, false, 1_000), (1_000, 0));
        // The packet overlaps 100 ms already written.
        assert_eq!(placement(900, 1_000, false, 1_000), (0, 100));
        // Small drift is tolerated.
        assert_eq!(placement(1_040, 1_000, false, 1_000), (0, 0));
    }

    #[test]
    fn anchor_converts_counter_time() {
        let a = Anchor {
            start_qpc: 20_000_000,
            freq: 10_000_000,
        };
        assert!((a.seconds_to(25_000_000) - 0.5).abs() < 1e-9);
    }

    fn f32_bytes(v: &[f32]) -> Vec<u8> {
        v.iter().flat_map(|x| x.to_le_bytes()).collect()
    }

    #[test]
    fn converts_float_stereo_to_mono_and_back() {
        let data = f32_bytes(&[0.5, -0.5, 1.0, 1.0]);
        assert_eq!(convert(&data, SampleKind::F32, 2, 1), vec![0, 32_767]);
        let mono = f32_bytes(&[0.5]);
        assert_eq!(convert(&mono, SampleKind::F32, 1, 2), vec![16_384, 16_384]);
    }

    #[test]
    fn converts_integer_layouts() {
        let i16s: Vec<u8> = [16_384i16].iter().flat_map(|x| x.to_le_bytes()).collect();
        assert_eq!(convert(&i16s, SampleKind::I16, 1, 1), vec![16_384]);
        // 24-bit half scale: 0x400000.
        assert_eq!(convert(&[0, 0, 0x40], SampleKind::I24, 1, 1), vec![16_384]);
        let i32s = (1i32 << 30).to_le_bytes();
        assert_eq!(convert(&i32s, SampleKind::I32, 1, 1), vec![16_384]);
    }

    #[test]
    fn surround_folds_center_into_both_sides() {
        let data = f32_bytes(&[0.1, 0.2, 0.3, 0.0, 0.0, 0.0]);
        let out = convert(&data, SampleKind::F32, 6, 2);
        assert_eq!(out.len(), 2);
        assert!(out[0] > 3_276 && out[1] > 6_553);
    }

    #[test]
    fn meter_keeps_the_max_until_read() {
        let level = AtomicU32::new(0);
        bump_level(&level, 0.3);
        bump_level(&level, 0.1);
        assert!((f32::from_bits(level.load(Ordering::Relaxed)) - 0.3).abs() < 1e-6);
    }
}
