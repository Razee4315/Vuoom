//! Session-side camera: the device list, the camera kept open while the user frames the
//! shot (for its live bubble) and handed to the take on record, crash recovery and bundles,
//! and the decoded frames preview and export draw. See `docs/17-Camera.md`.

use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard};

use serde::Serialize;
use vuoom_camera::{CameraRecorder, Clock, TrackReader, FILE_NAME};
use vuoom_project::{CameraOverlay, Project};
use vuoom_render::{CameraImage, Scene};

/// Sidecar holding the take's clock origin (for re-aligning a recovered take).
const CAMERA_META: &str = "camera.json";

/// One camera, as the recording UI lists it.
#[derive(Debug, Clone, Serialize)]
pub struct CameraDevice {
    pub id: String,
    pub name: String,
}

/// Cameras connected now.
pub fn devices() -> Result<Vec<CameraDevice>, String> {
    let devices = vuoom_camera::list_cameras()?
        .into_iter()
        .map(|c| CameraDevice {
            id: c.id,
            name: c.name,
        })
        .collect();
    Ok(devices)
}

/// Which camera the next recording captures.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CameraChoice {
    pub on: bool,
    /// Device id; `None` = the first camera.
    pub device: Option<String>,
}

/// An open camera and which device it is.
struct Live {
    device: Option<String>,
    recorder: CameraRecorder,
}

/// The camera the recording UI keeps open for its bubble, handed to the take on record.
#[derive(Default)]
pub struct Camera {
    live: Mutex<Option<Live>>,
}

impl Camera {
    fn live(&self) -> MutexGuard<'_, Option<Live>> {
        self.live.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Open `device` for the live bubble (keeping it if it's already open), or close the
    /// camera with `None`. Opening blocks for a moment.
    pub fn preview(&self, device: Option<Option<String>>) -> Result<(), String> {
        let Some(device) = device else {
            *self.live() = None;
            return Ok(());
        };
        if self.live().as_ref().is_some_and(|l| l.device == device) {
            return Ok(());
        }
        // A device can't be open twice: close the old one first.
        *self.live() = None;
        let recorder = CameraRecorder::open(device.clone())?;
        *self.live() = Some(Live { device, recorder });
        Ok(())
    }

    /// The live bubble's latest frame, as a JPEG.
    pub fn latest(&self) -> Option<Arc<Vec<u8>>> {
        self.live().as_ref()?.recorder.latest_jpeg()
    }

    /// Start recording the chosen camera into the take stored in `dir`. The camera already
    /// open for the bubble is used when it's the chosen one, so the take doesn't wait for
    /// the device; otherwise it's opened now. A camera that can't start is skipped: the
    /// take goes on without it, and the returned note says why.
    pub fn start(
        &self,
        choice: &CameraChoice,
        dir: &Path,
        clock: Clock,
    ) -> (Option<CameraRecorder>, Option<String>) {
        let open = self.live().take();
        if !choice.on {
            return (None, None);
        }
        let recorder = match open.filter(|l| l.device == choice.device) {
            Some(live) => Ok(live.recorder),
            None => CameraRecorder::open(choice.device.clone()),
        };
        let started = recorder.and_then(|r| {
            r.record(&dir.join(FILE_NAME), clock)?;
            Ok(r)
        });
        match started {
            Ok(r) => {
                let meta = serde_json::json!({ "start_qpc": clock.start_qpc, "freq": clock.freq });
                let _ = std::fs::write(dir.join(CAMERA_META), meta.to_string());
                (Some(r), None)
            }
            Err(e) => {
                tracing::warn!("camera not recorded: {e}");
                (None, Some(format!("The camera was not recorded ({e}).")))
            }
        }
    }
}

/// Frames in the camera track in `dir` (0 when there is none).
fn frame_count(dir: &Path) -> usize {
    let path = dir.join(FILE_NAME);
    TrackReader::open(&path).map_or(0, |t| t.len())
}

/// Stop a take's camera. Returns the overlay the project gets (when frames were recorded)
/// and a note for the user if something went wrong.
pub fn finish(
    recorder: Option<CameraRecorder>,
    dir: &Path,
) -> (Option<CameraOverlay>, Option<String>) {
    let Some(recorder) = recorder else {
        return (None, None);
    };
    let failure = recorder.finish().err();
    let frames = frame_count(dir);
    let overlay = (frames > 0).then(CameraOverlay::default);
    let note = match (failure, frames) {
        (Some(e), 0) => Some(format!("The camera was not recorded ({e}).")),
        (None, 0) => Some("The camera was not recorded (it sent no frames).".into()),
        (Some(e), _) => Some(format!("The camera stopped early ({e}).")),
        (None, _) => None,
    };
    if let Some(n) = &note {
        tracing::warn!("{n}");
    }
    (overlay, note)
}

/// Re-attach a recovered take's camera. Like its audio, the track is shifted: a recovered
/// timeline starts at the first frame rather than at the recording's start instant.
pub fn recover(dir: &Path, project: &mut Project, first_frame_qpc: i64) {
    let frames = frame_count(dir);
    if frames == 0 {
        project.camera = None;
        return;
    }
    let mut overlay = project.camera.unwrap_or_default();
    let meta = std::fs::read_to_string(dir.join(CAMERA_META))
        .ok()
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok());
    let clock = meta.and_then(|m| {
        Some(Clock {
            start_qpc: m["start_qpc"].as_i64()?,
            freq: m["freq"].as_i64()?,
        })
    });
    if let Some(clock) = clock.filter(|c| c.freq > 0) {
        overlay.offset = -clock.seconds(first_frame_qpc);
    }
    project.camera = Some(overlay);
}

/// Copy a take's camera track from one directory to another (bundle save and open).
pub fn copy_track(from: &Path, to: &Path) -> Result<(), String> {
    let src = from.join(FILE_NAME);
    if !src.is_file() {
        return Ok(());
    }
    std::fs::create_dir_all(to)
        .and_then(|()| std::fs::copy(&src, to.join(FILE_NAME)))
        .map(|_| ())
        .map_err(|e| format!("camera: {e}"))
}

/// A decoded camera frame.
pub struct Decoded {
    width: u32,
    height: u32,
    bgra: Vec<u8>,
}

impl Decoded {
    /// The frame as the compositor takes it.
    pub fn image(&self) -> CameraImage<'_> {
        CameraImage {
            bgra: &self.bgra,
            width: self.width,
            height: self.height,
        }
    }
}

/// A take's camera frames for preview and export. Consecutive output frames usually show
/// the same camera frame, so the last one decoded is kept.
pub struct Frames {
    track: TrackReader,
    last: Mutex<Option<(usize, Arc<Decoded>)>>,
}

impl Frames {
    /// The camera track in `dir`, if the take has one with frames.
    pub fn open(dir: &Path) -> Option<Self> {
        let track = TrackReader::open(&dir.join(FILE_NAME)).ok()?;
        (!track.is_empty()).then(|| Self {
            track,
            last: Mutex::new(None),
        })
    }

    /// The frame showing at camera time `t`; `None` (logged) if it can't be read.
    pub fn at(&self, t: f64) -> Option<Arc<Decoded>> {
        let i = self.track.index_at(t)?;
        let mut last = self.last.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((_, d)) = last.as_ref().filter(|(j, _)| *j == i) {
            return Some(Arc::clone(d));
        }
        let decoded = self
            .track
            .read(i)
            .and_then(|bytes| vuoom_camera::jpeg::decode_bgra(&bytes));
        let (width, height, bgra) = match decoded {
            Ok(frame) => frame,
            Err(e) => {
                tracing::warn!("camera frame {i} skipped: {e}");
                return None;
            }
        };
        let d = Arc::new(Decoded {
            width,
            height,
            bgra,
        });
        *last = Some((i, Arc::clone(&d)));
        Some(d)
    }
}

/// The camera frames for `project`, stored in `dir`, when it has a camera.
pub fn frames_for(project: &Project, dir: Option<&Path>) -> Option<Frames> {
    project.camera?;
    Frames::open(dir?)
}

/// The camera frame a scene's bubble shows, if it has one.
pub fn frame_for(scene: &Scene, frames: Option<&Frames>) -> Option<Arc<Decoded>> {
    let bubble = scene.camera?;
    frames?.at(bubble.t)
}

#[cfg(test)]
mod tests {
    use super::*;
    use vuoom_camera::TrackWriter;
    use vuoom_project::SourceInfo;

    fn project() -> Project {
        Project::new(SourceInfo {
            path: String::new(),
            width: 100,
            height: 100,
            fps: 30.0,
            duration: 4.0,
        })
    }

    fn write_track(dir: &Path, frames: usize) {
        let mut w = TrackWriter::create(&dir.join(FILE_NAME)).unwrap();
        for i in 0..frames {
            w.push(i as f64 / 30.0, b"jpeg").unwrap();
        }
        w.finish().unwrap();
    }

    #[test]
    fn recovery_attaches_the_camera_and_shifts_it() {
        let dir = tempfile::tempdir().unwrap();
        let mut p = project();
        recover(dir.path(), &mut p, 1_050);
        assert!(p.camera.is_none(), "no track, no bubble");

        write_track(dir.path(), 3);
        std::fs::write(
            dir.path().join(CAMERA_META),
            r#"{"start_qpc": 1000, "freq": 1000}"#,
        )
        .unwrap();
        // The first frame landed 50 ms after the recording's start.
        recover(dir.path(), &mut p, 1_050);
        let overlay = p.camera.unwrap();
        assert!((overlay.offset + 0.05).abs() < 1e-9);
        assert!(overlay.visible);
    }

    #[test]
    fn no_camera_means_no_bubble() {
        let dir = tempfile::tempdir().unwrap();
        write_track(dir.path(), 0);
        let (overlay, note) = finish(None, dir.path());
        assert!(overlay.is_none() && note.is_none());
        assert!(Frames::open(dir.path()).is_none());
    }

    #[test]
    fn tracks_travel_with_bundles() {
        let from = tempfile::tempdir().unwrap();
        let to = tempfile::tempdir().unwrap();
        copy_track(from.path(), &to.path().join("camera")).unwrap();
        assert!(!to.path().join("camera").exists(), "nothing to copy");
        write_track(from.path(), 2);
        copy_track(from.path(), &to.path().join("camera")).unwrap();
        let copied = TrackReader::open(&to.path().join("camera").join(FILE_NAME)).unwrap();
        assert_eq!(copied.len(), 2);
    }
}
