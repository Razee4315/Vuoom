//! Webcam capture through AVFoundation (macOS).
//!
//! The same shape as the Media Foundation backend: one thread per camera opens the device,
//! picks a native format with [`choose_format`], and every frame (BGRA, asked of the video
//! data output) is scaled to the stored size, encoded as a JPEG and handed to [`Shared`].
//! Frame times are the sample's presentation time (100 ns units, host clock); [`super::Pin`]
//! ties them to the recording clock exactly as on Windows.
//!
//! Needs the Camera permission (`NSCameraUsageDescription`); a denied camera fails with a
//! message the UI shows.

use super::{choose_format, downscale, fit, Camera, Format, Shared};
use crate::jpeg;
use block2::RcBlock;
use dispatch2::DispatchQueue;
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, Bool, NSObject, NSObjectProtocol, ProtocolObject};
use objc2::{define_class, msg_send, AllocAnyThread, DefinedClass};
use objc2_av_foundation::{
    AVAuthorizationStatus, AVCaptureConnection, AVCaptureDevice, AVCaptureDeviceDiscoverySession,
    AVCaptureDeviceInput, AVCaptureDevicePosition, AVCaptureDeviceTypeBuiltInWideAngleCamera,
    AVCaptureDeviceTypeContinuityCamera, AVCaptureDeviceTypeExternal, AVCaptureOutput,
    AVCaptureSession, AVCaptureVideoDataOutput, AVCaptureVideoDataOutputSampleBufferDelegate,
    AVMediaTypeVideo,
};
use objc2_core_media::{CMSampleBuffer, CMTime};
use objc2_core_video::{
    kCVPixelBufferPixelFormatTypeKey, kCVPixelFormatType_32BGRA, CVPixelBufferGetBaseAddress,
    CVPixelBufferGetBytesPerRow, CVPixelBufferGetHeight, CVPixelBufferGetWidth,
    CVPixelBufferLockBaseAddress, CVPixelBufferLockFlags, CVPixelBufferUnlockBaseAddress,
};
use objc2_foundation::{NSArray, NSDictionary, NSNumber, NSString};
use std::sync::atomic::Ordering;
use std::sync::mpsc::Sender;
use std::time::Duration;

/// The error a camera reports when macOS hasn't given Vuoom the Camera permission.
pub const PERMISSION_ERROR: &str = "camera-permission";

fn media_video() -> &'static NSString {
    // SAFETY: a framework constant, present on every supported macOS.
    unsafe { AVMediaTypeVideo }.expect("AVMediaTypeVideo")
}

fn discovered() -> Vec<Retained<AVCaptureDevice>> {
    // SAFETY: AVFoundation queries with framework constants.
    unsafe {
        let types = NSArray::from_slice(&[
            AVCaptureDeviceTypeBuiltInWideAngleCamera,
            AVCaptureDeviceTypeExternal,
            AVCaptureDeviceTypeContinuityCamera,
        ]);
        let session =
            AVCaptureDeviceDiscoverySession::discoverySessionWithDeviceTypes_mediaType_position(
                &types,
                Some(media_video()),
                AVCaptureDevicePosition::Unspecified,
            );
        session.devices().to_vec()
    }
}

pub fn list() -> Result<Vec<Camera>, String> {
    // SAFETY: plain property reads.
    Ok(discovered()
        .iter()
        .map(|d| unsafe {
            Camera {
                id: d.uniqueID().to_string(),
                name: d.localizedName().to_string(),
            }
        })
        .collect())
}

/// Whether Vuoom may use the camera, asking (and waiting for the answer) the first time.
fn authorized() -> bool {
    // SAFETY: AVFoundation authorization calls with a framework constant.
    unsafe {
        let status = AVCaptureDevice::authorizationStatusForMediaType(media_video());
        if status == AVAuthorizationStatus::Authorized {
            return true;
        }
        if status != AVAuthorizationStatus::NotDetermined {
            return false;
        }
        let (tx, rx) = std::sync::mpsc::channel::<bool>();
        let block = RcBlock::new(move |granted: Bool| {
            let _ = tx.send(granted.as_bool());
        });
        AVCaptureDevice::requestAccessForMediaType_completionHandler(media_video(), &block);
        rx.recv_timeout(Duration::from_secs(120)).unwrap_or(false)
    }
}

/// What the frame callback needs. `shared` points at the capture thread's [`Shared`], which
/// outlives every callback: `run` detaches the delegate and drains its queue before
/// returning.
struct Sink {
    shared: *const Shared,
    size: (u32, u32),
    clock: vuoom_input::Clock,
}

define_class!(
    // SAFETY: NSObject has no subclassing requirements.
    #[unsafe(super(NSObject))]
    #[name = "VuoomCameraOutput"]
    #[ivars = Sink]
    struct CameraOutput;

    unsafe impl NSObjectProtocol for CameraOutput {}

    unsafe impl AVCaptureVideoDataOutputSampleBufferDelegate for CameraOutput {
        #[unsafe(method(captureOutput:didOutputSampleBuffer:fromConnection:))]
        fn did_output(
            &self,
            _output: &AVCaptureOutput,
            sample: &CMSampleBuffer,
            _connection: &AVCaptureConnection,
        ) {
            self.ivars().frame(sample);
        }
    }
);

impl Sink {
    fn frame(&self, sample: &CMSampleBuffer) {
        // SAFETY: see the struct docs; `shared` is alive while callbacks can run.
        let shared = unsafe { &*self.shared };
        if shared.stop.load(Ordering::Relaxed) {
            return;
        }
        let arrived = self.clock.now();
        // SAFETY: reads of a live sample buffer.
        let (stamp, image) = unsafe {
            let pts: CMTime = sample.presentation_time_stamp();
            ((pts.seconds() * 1e7) as i64, sample.image_buffer())
        };
        let Some(image) = image else { return };
        let Some((w, h, px)) = copy_bgra(&image) else {
            return;
        };
        let px = if (w, h) == self.size {
            px
        } else {
            downscale(&px, w, h, self.size.0, self.size.1)
        };
        match jpeg::encode_bgra(&px, self.size.0, self.size.1) {
            Ok(j) => {
                if let Err(e) = shared.deliver(stamp, arrived, j) {
                    tracing::warn!("camera track: {e}");
                }
            }
            Err(e) => tracing::warn!("camera frame skipped: {e}"),
        }
    }
}

/// A camera frame's pixels, tightly packed BGRA.
fn copy_bgra(image: &objc2_core_video::CVPixelBuffer) -> Option<(u32, u32, Vec<u8>)> {
    let flags = CVPixelBufferLockFlags::ReadOnly;
    // SAFETY: lock / read within `stride * h` / unlock of a live pixel buffer.
    unsafe {
        if CVPixelBufferLockBaseAddress(image, flags) != 0 {
            return None;
        }
        let (w, h) = (CVPixelBufferGetWidth(image), CVPixelBufferGetHeight(image));
        let stride = CVPixelBufferGetBytesPerRow(image);
        let base = CVPixelBufferGetBaseAddress(image).cast::<u8>();
        let row = w * 4;
        let mut out = Vec::new();
        if !base.is_null() && stride >= row {
            out.reserve(row * h);
            for y in 0..h {
                out.extend_from_slice(std::slice::from_raw_parts(base.add(y * stride), row));
            }
        }
        CVPixelBufferUnlockBaseAddress(image, flags);
        (out.len() == row * h && w > 0).then_some((w as u32, h as u32, out))
    }
}

/// The device to open: by unique id, else the system default camera, else the first one.
fn device(id: Option<&str>) -> Option<Retained<AVCaptureDevice>> {
    // SAFETY: AVFoundation lookups.
    unsafe {
        if let Some(id) = id {
            if let Some(d) = AVCaptureDevice::deviceWithUniqueID(&NSString::from_str(id)) {
                return Some(d);
            }
        }
        AVCaptureDevice::defaultDeviceWithMediaType(media_video())
            .or_else(|| discovered().into_iter().next())
    }
}

/// Pick and apply a native format; returns its size.
fn apply_format(device: &AVCaptureDevice) -> Option<(u32, u32)> {
    // SAFETY: format queries and a configuration lock on an open device.
    unsafe {
        let formats = device.formats().to_vec();
        let described: Vec<Format> = formats
            .iter()
            .map(|f| {
                #[allow(deprecated)]
                let dims =
                    objc2_core_media::CMVideoFormatDescriptionGetDimensions(&f.formatDescription());
                let fps = f
                    .videoSupportedFrameRateRanges()
                    .iter()
                    .map(|r| r.maxFrameRate())
                    .fold(0.0, f64::max);
                Format {
                    width: dims.width.max(0) as u32,
                    height: dims.height.max(0) as u32,
                    fps,
                }
            })
            .collect();
        let pick = choose_format(&described)?;
        let chosen = described[pick];
        if device.lockForConfiguration().is_ok() {
            device.setActiveFormat(&formats[pick]);
            device.unlockForConfiguration();
        }
        Some((chosen.width, chosen.height))
    }
}

pub fn run(
    id: Option<&str>,
    shared: &Shared,
    ready: &Sender<Result<(u32, u32), String>>,
) -> Result<(), String> {
    let fail = |e: String| {
        let _ = ready.send(Err(e.clone()));
        Err(e)
    };
    if !authorized() {
        return fail(PERMISSION_ERROR.into());
    }
    let Some(device) = device(id) else {
        return fail("no camera found".into());
    };
    let Some((cw, ch)) = apply_format(&device) else {
        return fail("the camera offers no usable format".into());
    };
    let size = fit(cw, ch);
    let queue = DispatchQueue::new("dev.vuoom.camera", None);
    // SAFETY: AVFoundation session setup and control on this thread; the delegate holds a
    // pointer to `shared`, which stays alive until the delegate is detached and its queue
    // drained below.
    unsafe {
        let input = match AVCaptureDeviceInput::deviceInputWithDevice_error(&device) {
            Ok(i) => i,
            Err(e) => return fail(format!("camera unavailable ({})", e.localizedDescription())),
        };
        let session = AVCaptureSession::new();
        if !session.canAddInput(&input) {
            return fail("the camera is in use".into());
        }
        session.addInput(&input);
        let output = AVCaptureVideoDataOutput::new();
        let key: &NSString = &*std::ptr::from_ref(kCVPixelBufferPixelFormatTypeKey).cast();
        let format: Retained<AnyObject> =
            Retained::cast_unchecked(NSNumber::new_u32(kCVPixelFormatType_32BGRA));
        let settings = NSDictionary::<NSString, AnyObject>::from_slices(&[key], &[&*format]);
        output.setVideoSettings(Some(&settings));
        output.setAlwaysDiscardsLateVideoFrames(true);
        let delegate = {
            let this = CameraOutput::alloc().set_ivars(Sink {
                shared: std::ptr::from_ref(shared),
                size,
                clock: vuoom_input::Clock::new(),
            });
            let d: Retained<CameraOutput> = msg_send![super(this), init];
            d
        };
        output.setSampleBufferDelegate_queue(
            Some(ProtocolObject::from_ref(&*delegate)),
            Some(&queue),
        );
        if !session.canAddOutput(&output) {
            return fail("the camera can't deliver frames".into());
        }
        session.addOutput(&output);
        session.startRunning();
        let _ = ready.send(Ok(size));
        while !shared.stop.load(Ordering::Relaxed) {
            std::thread::sleep(Duration::from_millis(30));
        }
        session.stopRunning();
        output.setSampleBufferDelegate_queue(None, None);
        // Let any callback already queued finish before `shared` can go away.
        queue.exec_sync(|| {});
        drop(delegate);
    }
    Ok(())
}
