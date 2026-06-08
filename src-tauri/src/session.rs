//! Recording → project → preview/export orchestration — the engine glue.
//!
//! Ties the pieces together: capture + global input → auto-zoom plan + camera track →
//! composite → preview stream / GIF export. Frame storage is in-memory for v1 (a disk
//! intermediate is the documented next step). See `docs/02-Architecture.md`.
//!
//! Runtime behaviour (capture/GPU/input) is verified by running on a real Windows machine;
//! CI verifies it compiles.

use std::path::Path;
use std::sync::mpsc::Receiver;
use std::sync::Mutex;

use serde::Serialize;
use vuoom_capture::{spawn_primary_display, CaptureHandle, CapturedFrame};
use vuoom_encode::{export_gif as encode_gif, plan_frames, GifSettings, RgbaImage};
use vuoom_input::{normalize, CaptureRegion, Clock, InputRecorder, RawEvent};
use vuoom_preview::{pack_frame, FrameMeta, PreviewServer};
use vuoom_project::{Background, Color, FrameStyle, Project, SourceInfo, ZoomConfig};
use vuoom_render::{compute_layout, Compositor};
use vuoom_zoom::{plan_zooms, simulate, CameraTrack, InputEvent};

/// Summary returned to the UI when recording stops.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct RecordingSummary {
    pub duration: f64,
    pub frames: usize,
    pub zooms: usize,
}

struct Active {
    frames_rx: Receiver<CapturedFrame>,
    capture: CaptureHandle,
    recorder: InputRecorder,
    events_rx: Receiver<RawEvent>,
    start_qpc: i64,
}

#[derive(Default)]
struct Edited {
    frames: Vec<CapturedFrame>,
    project: Option<Project>,
    track: Option<CameraTrack>,
    start_qpc: i64,
}

/// The app's recording/editing/export engine, held as Tauri managed state.
pub struct Session {
    preview: PreviewServer,
    compositor: Option<Compositor>,
    clock: Clock,
    active: Mutex<Option<Active>>,
    edited: Mutex<Edited>,
}

impl Session {
    /// Start the preview server and GPU compositor.
    #[must_use]
    pub fn new() -> Self {
        let preview = tauri::async_runtime::block_on(PreviewServer::start())
            .expect("failed to start preview server");
        Self {
            preview,
            compositor: Compositor::new(),
            clock: Clock::new(),
            active: Mutex::new(None),
            edited: Mutex::new(Edited::default()),
        }
    }

    /// The localhost port the webview connects to for the live preview.
    #[must_use]
    pub fn preview_port(&self) -> u16 {
        self.preview.port()
    }

    /// Begin capturing the primary display + global input.
    pub fn start_recording(&self) -> Result<(), String> {
        let mut active = self.active.lock().map_err(|_| "lock poisoned")?;
        if active.is_some() {
            return Err("already recording".into());
        }
        let (frames_rx, capture) = spawn_primary_display();
        let (recorder, events_rx) = InputRecorder::start();
        *active = Some(Active {
            frames_rx,
            capture,
            recorder,
            events_rx,
            start_qpc: self.clock.now(),
        });
        Ok(())
    }

    /// Stop capturing and build the editable project (plan zooms, simulate the camera).
    pub fn stop_recording(&self) -> Result<RecordingSummary, String> {
        let mut active = self.active.lock().map_err(|_| "lock poisoned")?;
        let Some(mut session) = active.take() else {
            return Err("not recording".into());
        };
        session.capture.stop();
        session.recorder.stop();

        let frames: Vec<CapturedFrame> = session.frames_rx.try_iter().collect();
        let raw_events: Vec<RawEvent> = session.events_rx.try_iter().collect();

        let (width, height) = frames.first().map_or((1920, 1080), |f| (f.width, f.height));
        let duration = frames.last().map_or(0.0, |f| {
            self.clock.seconds_between(session.start_qpc, f.qpc)
        });

        let region = CaptureRegion {
            x: 0,
            y: 0,
            w: width as i32,
            h: height as i32,
        };
        let freq = self.clock.freq();
        let events: Vec<InputEvent> = raw_events
            .iter()
            .filter_map(|e| normalize(e, &region, session.start_qpc, freq))
            .collect();

        let cfg = ZoomConfig::default();
        let zooms = plan_zooms(&events, duration, &cfg);
        let fps = if duration > 0.0 {
            frames.len() as f64 / duration
        } else {
            60.0
        };
        let track = simulate(&events, &zooms, duration, fps.max(1.0), &cfg);

        let mut project = Project::new(SourceInfo {
            path: String::new(),
            width,
            height,
            fps,
            duration,
        });
        let zoom_count = zooms.len();
        let frame_count = frames.len();
        project.zooms = zooms;

        let mut edited = self.edited.lock().map_err(|_| "lock poisoned")?;
        *edited = Edited {
            frames,
            project: Some(project),
            track: Some(track),
            start_qpc: session.start_qpc,
        };

        Ok(RecordingSummary {
            duration,
            frames: frame_count,
            zooms: zoom_count,
        })
    }

    /// Composite the frame at time `t` (seconds) and publish it to the preview.
    pub fn seek(&self, t: f64) -> Result<(), String> {
        let edited = self.edited.lock().map_err(|_| "lock poisoned")?;
        let compositor = self.compositor.as_ref().ok_or("no GPU compositor")?;
        let project = edited.project.as_ref().ok_or("no recording")?;
        let track = edited.track.as_ref().ok_or("no recording")?;
        let frame =
            nearest_frame(&edited.frames, self.clock, edited.start_qpc, t).ok_or("no frames")?;

        let (out_w, out_h) = project.output_dims();
        let cam = track.at(t);
        let layout = compute_layout(out_w, out_h, &project.frame, &cam);
        let rgba = compositor.composite(
            &frame.bgra,
            frame.width,
            frame.height,
            out_w,
            out_h,
            &layout,
            background_color(&project.frame),
        );
        let meta = FrameMeta {
            stride: out_w * 4,
            height: out_h,
            width: out_w,
            frame_number: 0,
            target_time_ns: (t * 1e9) as u64,
        };
        self.preview.sink().publish(pack_frame(&rgba, meta));
        Ok(())
    }

    /// Composite every emitted frame and export an optimized GIF to `out_path`.
    pub fn export_gif(&self, out_path: String, fps: u32, width: Option<u32>) -> Result<(), String> {
        let edited = self.edited.lock().map_err(|_| "lock poisoned")?;
        let compositor = self.compositor.as_ref().ok_or("no GPU compositor")?;
        let project = edited.project.as_ref().ok_or("no recording")?;
        let track = edited.track.as_ref().ok_or("no recording")?;

        let (out_w, out_h) = project.output_dims();
        let bg = background_color(&project.frame);
        let plan = plan_frames(edited.frames.len(), project.source.fps, f64::from(fps));

        let mut images = Vec::with_capacity(plan.len());
        for emitted in &plan {
            let frame = &edited.frames[emitted.source_index];
            let t = self.clock.seconds_between(edited.start_qpc, frame.qpc);
            let cam = track.at(t);
            let layout = compute_layout(out_w, out_h, &project.frame, &cam);
            let rgba = compositor.composite(
                &frame.bgra,
                frame.width,
                frame.height,
                out_w,
                out_h,
                &layout,
                bg,
            );
            images.push(RgbaImage::new(out_w, out_h, rgba));
        }

        let settings = GifSettings {
            fps,
            width,
            ..GifSettings::readme()
        };
        // TODO: resolve the bundled gifski sidecar path; "gifski" relies on PATH for now.
        encode_gif(
            &images,
            &settings,
            Path::new("gifski"),
            None,
            Path::new(&out_path),
        )
        .map_err(|e| e.to_string())
    }
}

impl Default for Session {
    fn default() -> Self {
        Self::new()
    }
}

fn nearest_frame(
    frames: &[CapturedFrame],
    clock: Clock,
    start_qpc: i64,
    t: f64,
) -> Option<&CapturedFrame> {
    frames.iter().min_by(|a, b| {
        let da = (clock.seconds_between(start_qpc, a.qpc) - t).abs();
        let db = (clock.seconds_between(start_qpc, b.qpc) - t).abs();
        da.total_cmp(&db)
    })
}

fn background_color(frame: &FrameStyle) -> [f32; 4] {
    let c = match &frame.background {
        Background::Solid(c) => *c,
        Background::Gradient { from, .. } => *from,
        Background::Image { .. } | Background::Blur { .. } => Color::rgb(0.08, 0.08, 0.09),
    };
    [c.r, c.g, c.b, c.a]
}
