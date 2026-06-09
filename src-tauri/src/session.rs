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

use glam::DVec2;
use serde::{Deserialize, Serialize};
use vuoom_capture::{spawn_region, CaptureHandle, CapturedFrame, CropRegion};
use vuoom_encode::{
    export_gif_native, plan_frames, read_png, swizzle_rb, write_png, GifSettings, RgbaImage,
};
use vuoom_input::{normalize, zoom_marks, CaptureRegion, Clock, InputRecorder, RawEvent};
use vuoom_preview::{pack_frame, FrameMeta, PreviewServer};
use vuoom_project::{
    ArrowAnnotation, Background, Color, FrameStyle, HighlightBox, Project, Rect, SourceInfo,
    TextAnnotation, TimeRange, ZoomConfig,
};
use vuoom_render::{build_scene, Compositor};
use vuoom_zoom::{plan_zooms, simulate, CameraTrack, InputEvent};

/// Summary returned to the UI when recording stops.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct RecordingSummary {
    pub duration: f64,
    pub frames: usize,
    pub zooms: usize,
}

/// All annotations on the project, sent to the editor overlay so it can draw selection
/// handles and hit-test for moving/resizing.
#[derive(Debug, Clone, Serialize)]
pub struct AnnotationSet {
    pub texts: Vec<TextAnnotation>,
    pub arrows: Vec<ArrowAnnotation>,
    pub highlights: Vec<HighlightBox>,
}

struct Active {
    frames_rx: Receiver<CapturedFrame>,
    capture: CaptureHandle,
    recorder: InputRecorder,
    events_rx: Receiver<RawEvent>,
    start_qpc: i64,
    region: Option<CropRegion>,
}

#[derive(Default)]
struct Edited {
    frames: Vec<CapturedFrame>,
    project: Option<Project>,
    track: Option<CameraTrack>,
    start_qpc: i64,
}

/// One entry in a saved bundle's `frames/index.json`: frame number, time from start
/// (seconds), and dimensions. The QPC epoch isn't portable, so time is stored instead.
#[derive(Serialize, Deserialize)]
struct FrameIndex {
    n: usize,
    t: f64,
    w: u32,
    h: u32,
}

/// The app's recording/editing/export engine, held as Tauri managed state.
pub struct Session {
    preview: PreviewServer,
    compositor: Option<Compositor>,
    clock: Clock,
    active: Mutex<Option<Active>>,
    edited: Mutex<Edited>,
    /// The capture region chosen by the selector for the next recording (physical px);
    /// `None` = full primary display.
    pending_region: Mutex<Option<CropRegion>>,
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
            pending_region: Mutex::new(None),
        }
    }

    /// Set the capture region (physical px) for the next recording; `None` = full display.
    pub fn set_region(&self, region: Option<CropRegion>) -> Result<(), String> {
        *self.pending_region.lock().map_err(|_| "lock poisoned")? = region;
        Ok(())
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
        let region = *self.pending_region.lock().map_err(|_| "lock poisoned")?;
        let (frames_rx, capture) = spawn_region(region);
        let (recorder, events_rx) = InputRecorder::start();
        *active = Some(Active {
            frames_rx,
            capture,
            region,
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

        // Map the cursor into the captured area. With a crop region the cursor's physical
        // virtual-desktop coords must be offset by the region origin; full-screen uses 0,0.
        let region = match session.region {
            Some(r) => CaptureRegion {
                x: r.x as i32,
                y: r.y as i32,
                w: r.w as i32,
                h: r.h as i32,
            },
            None => CaptureRegion {
                x: 0,
                y: 0,
                w: width as i32,
                h: height as i32,
            },
        };
        let freq = self.clock.freq();
        let mut events: Vec<InputEvent> = raw_events
            .iter()
            .filter_map(|e| normalize(e, &region, session.start_qpc, freq))
            .collect();
        // Manual zoom: each Ctrl+Shift+Z press becomes a deliberate zoom at the cursor.
        events.extend(zoom_marks(&raw_events, &region, session.start_qpc, freq));
        events.sort_by(|a, b| a.t().total_cmp(&b.t()));

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
        project.events = events; // persisted so a reopened project can re-simulate panning

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
        let scene = build_scene(project, track, out_w, out_h, t);
        let rgba = compositor.composite_scene(
            &frame.bgra,
            frame.width,
            frame.height,
            out_w,
            out_h,
            &scene,
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
    ///
    /// Uses the pure-Rust encoder so export works with no external tools installed.
    pub fn export_gif(
        &self,
        out_path: String,
        fps: u32,
        width: Option<u32>,
        quality: u8,
    ) -> Result<(), String> {
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
            let scene = build_scene(project, track, out_w, out_h, t);
            let rgba = compositor.composite_scene(
                &frame.bgra,
                frame.width,
                frame.height,
                out_w,
                out_h,
                &scene,
                bg,
            );
            images.push(RgbaImage::new(out_w, out_h, rgba));
        }

        let settings = GifSettings {
            fps,
            width,
            quality,
            ..GifSettings::readme()
        };
        export_gif_native(&images, &settings, Path::new(&out_path)).map_err(|e| e.to_string())
    }

    /// Add a text label at normalized `(x, y)`, visible for ~3s from time `t`. Returns its id.
    pub fn add_text(&self, text: String, x: f64, y: f64, t: f64) -> Result<u32, String> {
        let mut edited = self.edited.lock().map_err(|_| "lock poisoned")?;
        let project = edited.project.as_mut().ok_or("no recording")?;
        let id = next_id(project);
        let range = TimeRange::with_fade(t, default_end(t, project.source.duration), 0.2);
        project.texts.push(TextAnnotation {
            id,
            text,
            pos: DVec2::new(x, y),
            font_size: 0.05,
            color: Color::WHITE,
            range,
        });
        Ok(id)
    }

    /// Add an arrow between normalized points, visible for ~3s from time `t`. Returns its id.
    pub fn add_arrow(&self, fx: f64, fy: f64, tx: f64, ty: f64, t: f64) -> Result<u32, String> {
        let mut edited = self.edited.lock().map_err(|_| "lock poisoned")?;
        let project = edited.project.as_mut().ok_or("no recording")?;
        let id = next_id(project);
        let range = TimeRange::with_fade(t, default_end(t, project.source.duration), 0.2);
        project.arrows.push(ArrowAnnotation {
            id,
            from: DVec2::new(fx, fy),
            to: DVec2::new(tx, ty),
            color: Color::rgb(0.95, 0.25, 0.25),
            thickness: 0.006,
            range,
        });
        Ok(id)
    }

    /// Add a highlight box (normalized rect), visible for ~3s from time `t`. Returns its id.
    pub fn add_box(&self, x: f64, y: f64, w: f64, h: f64, t: f64) -> Result<u32, String> {
        let mut edited = self.edited.lock().map_err(|_| "lock poisoned")?;
        let project = edited.project.as_mut().ok_or("no recording")?;
        let id = next_id(project);
        let range = TimeRange::with_fade(t, default_end(t, project.source.duration), 0.2);
        project.highlights.push(HighlightBox {
            id,
            rect: Rect::new(x, y, w, h),
            color: Color::rgb(1.0, 0.82, 0.1),
            thickness: 0.005,
            filled: false,
            range,
        });
        Ok(id)
    }

    /// Snapshot every annotation for the editor overlay.
    pub fn annotations(&self) -> Result<AnnotationSet, String> {
        let edited = self.edited.lock().map_err(|_| "lock poisoned")?;
        let project = edited.project.as_ref().ok_or("no recording")?;
        Ok(AnnotationSet {
            texts: project.texts.clone(),
            arrows: project.arrows.clone(),
            highlights: project.highlights.clone(),
        })
    }

    /// Move/edit a text label. `None` fields keep their current value.
    pub fn update_text(
        &self,
        id: u32,
        x: Option<f64>,
        y: Option<f64>,
        text: Option<String>,
        font_size: Option<f32>,
    ) -> Result<(), String> {
        self.with_project(|p| {
            let a = p
                .texts
                .iter_mut()
                .find(|a| a.id == id)
                .ok_or("no such text")?;
            if let Some(x) = x {
                a.pos.x = x.clamp(0.0, 1.0);
            }
            if let Some(y) = y {
                a.pos.y = y.clamp(0.0, 1.0);
            }
            if let Some(t) = text {
                a.text = t;
            }
            if let Some(fs) = font_size {
                a.font_size = fs.clamp(0.01, 0.5);
            }
            Ok(())
        })
    }

    /// Move an arrow's endpoints.
    pub fn update_arrow(&self, id: u32, fx: f64, fy: f64, tx: f64, ty: f64) -> Result<(), String> {
        self.with_project(|p| {
            let a = p
                .arrows
                .iter_mut()
                .find(|a| a.id == id)
                .ok_or("no such arrow")?;
            a.from = DVec2::new(fx.clamp(0.0, 1.0), fy.clamp(0.0, 1.0));
            a.to = DVec2::new(tx.clamp(0.0, 1.0), ty.clamp(0.0, 1.0));
            Ok(())
        })
    }

    /// Move/resize a highlight box.
    pub fn update_box(&self, id: u32, x: f64, y: f64, w: f64, h: f64) -> Result<(), String> {
        self.with_project(|p| {
            let b = p
                .highlights
                .iter_mut()
                .find(|b| b.id == id)
                .ok_or("no such box")?;
            b.rect = Rect::new(x.clamp(0.0, 1.0), y.clamp(0.0, 1.0), w.max(0.0), h.max(0.0));
            Ok(())
        })
    }

    /// Tint any annotation (text, arrow, or box) by id.
    pub fn set_annotation_color(&self, id: u32, r: f64, g: f64, b: f64) -> Result<(), String> {
        let color = Color::rgb(r as f32, g as f32, b as f32);
        self.with_project(|p| {
            if let Some(a) = p.texts.iter_mut().find(|a| a.id == id) {
                a.color = color;
            } else if let Some(a) = p.arrows.iter_mut().find(|a| a.id == id) {
                a.color = color;
            } else if let Some(a) = p.highlights.iter_mut().find(|a| a.id == id) {
                a.color = color;
            } else {
                return Err("no such annotation".into());
            }
            Ok(())
        })
    }

    /// Delete any annotation (text, arrow, or box) by id.
    pub fn delete_annotation(&self, id: u32) -> Result<(), String> {
        self.with_project(|p| {
            p.texts.retain(|a| a.id != id);
            p.arrows.retain(|a| a.id != id);
            p.highlights.retain(|a| a.id != id);
            Ok(())
        })
    }

    /// Save the recording as a `dir.vuoom` bundle: the project manifest plus every frame
    /// as a lossless PNG and a time index. Reopenable with [`Self::open_bundle`].
    pub fn save_bundle(&self, dir: &Path) -> Result<(), String> {
        let edited = self.edited.lock().map_err(|_| "lock poisoned")?;
        let project = edited.project.as_ref().ok_or("no recording")?;
        let frames_dir = dir.join("frames");
        std::fs::create_dir_all(&frames_dir).map_err(|e| e.to_string())?;

        let mut index = Vec::with_capacity(edited.frames.len());
        for (n, f) in edited.frames.iter().enumerate() {
            // Stored as RGBA (write_png's format); capture buffers are BGRA.
            let img = RgbaImage::new(f.width, f.height, swizzle_rb(&f.bgra));
            write_png(&frames_dir.join(format!("{n:05}.png")), &img).map_err(|e| e.to_string())?;
            index.push(FrameIndex {
                n,
                t: self.clock.seconds_between(edited.start_qpc, f.qpc),
                w: f.width,
                h: f.height,
            });
        }
        std::fs::write(
            frames_dir.join("index.json"),
            serde_json::to_string(&index).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        std::fs::write(
            dir.join("project.json"),
            project.to_json().map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Open a `.vuoom` bundle saved by [`Self::save_bundle`]: decode the frames, re-simulate
    /// the camera from the persisted events, and repopulate the editor. Returns a summary.
    pub fn open_bundle(&self, dir: &Path) -> Result<RecordingSummary, String> {
        let project = Project::from_json(
            &std::fs::read_to_string(dir.join("project.json")).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        let frames_dir = dir.join("frames");
        let index: Vec<FrameIndex> = serde_json::from_str(
            &std::fs::read_to_string(frames_dir.join("index.json")).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;

        let freq = self.clock.freq();
        let base = self.clock.now(); // fresh epoch; frame qpc is re-based onto it
        let mut frames = Vec::with_capacity(index.len());
        for fi in &index {
            let img = read_png(&frames_dir.join(format!("{:05}.png", fi.n)))
                .map_err(|e| e.to_string())?;
            frames.push(CapturedFrame {
                width: fi.w,
                height: fi.h,
                bgra: swizzle_rb(&img.pixels), // RGBA on disk -> BGRA in memory
                qpc: base + (fi.t * freq as f64) as i64,
            });
        }

        let track = simulate(
            &project.events,
            &project.zooms,
            project.source.duration,
            project.source.fps.max(1.0),
            &project.zoom_config,
        );
        let summary = RecordingSummary {
            duration: project.source.duration,
            frames: frames.len(),
            zooms: project.zooms.len(),
        };
        let mut edited = self.edited.lock().map_err(|_| "lock poisoned")?;
        *edited = Edited {
            frames,
            project: Some(project),
            track: Some(track),
            start_qpc: base,
        };
        Ok(summary)
    }

    /// Run `f` against the editable project, erroring if there is no recording.
    fn with_project<F>(&self, f: F) -> Result<(), String>
    where
        F: FnOnce(&mut Project) -> Result<(), String>,
    {
        let mut edited = self.edited.lock().map_err(|_| "lock poisoned")?;
        let project = edited.project.as_mut().ok_or("no recording")?;
        f(project)
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

fn next_id(project: &Project) -> u32 {
    let mut max = 0;
    for t in &project.texts {
        max = max.max(t.id);
    }
    for a in &project.arrows {
        max = max.max(a.id);
    }
    for h in &project.highlights {
        max = max.max(h.id);
    }
    max + 1
}

fn default_end(t: f64, duration: f64) -> f64 {
    (t + 3.0).min(duration.max(t + 0.5))
}
