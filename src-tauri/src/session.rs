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

use crate::live_preview::LivePreview;
use base64::Engine;
use glam::DVec2;
use serde::{Deserialize, Serialize};
use vuoom_capture::{spawn_region, CaptureHandle, CapturedFrame, CropRegion};
use vuoom_encode::{
    encode_png_to_vec, estimate_total_bytes, export_gif_native, read_png, swizzle_rb, write_png,
    GifSettings, RgbaImage,
};
use vuoom_input::{normalize, zoom_marks, CaptureRegion, Clock, InputRecorder, RawEvent};
use vuoom_preview::{pack_frame, FrameMeta, PreviewServer};
use vuoom_project::{
    output_duration, output_to_source, ArrowAnnotation, Background, Color, FrameStyle,
    HighlightBox, Project, Rect, SourceInfo, SpeedRegion, TextAnnotation, TimeRange, Trim,
    ZoomConfig, ZoomKeyframe,
};
use vuoom_render::{build_scene, Compositor};
use vuoom_zoom::{plan_zooms, simulate, CameraTrack, InputEvent, ZoomMode};

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

/// Everything the editor timeline binds to: duration, trim, speed regions, zoom segments.
#[derive(Debug, Clone, Serialize)]
pub struct ClipState {
    pub duration: f64,
    pub trim: Option<Trim>,
    pub speed_regions: Vec<SpeedRegion>,
    pub zooms: Vec<ZoomKeyframe>,
}

struct Active {
    frames_rx: Receiver<CapturedFrame>,
    capture: CaptureHandle,
    recorder: InputRecorder,
    events_rx: Receiver<RawEvent>,
    start_qpc: i64,
    region: Option<CropRegion>,
    /// Decoupled live "director's monitor" — dropped (and stopped) when recording ends.
    _preview: LivePreview,
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
    /// The zoom multiplier chosen for the next recording (1.0 = no zoom).
    pending_zoom: Mutex<f64>,
}

impl Session {
    /// Start the preview server and GPU compositor.
    ///
    /// # Errors
    /// Returns a message if the preview WebSocket server cannot bind.
    pub fn new() -> Result<Self, String> {
        let preview = tauri::async_runtime::block_on(PreviewServer::start())
            .map_err(|e| format!("preview server: {e}"))?;
        Ok(Self {
            preview,
            compositor: Compositor::new(),
            clock: Clock::new(),
            active: Mutex::new(None),
            edited: Mutex::new(Edited::default()),
            pending_region: Mutex::new(None),
            pending_zoom: Mutex::new(ZoomConfig::default().amount),
        })
    }

    /// Set the capture region (physical px) for the next recording; `None` = full display.
    pub fn set_region(&self, region: Option<CropRegion>) -> Result<(), String> {
        *self.pending_region.lock().map_err(|_| "lock poisoned")? = region;
        Ok(())
    }

    /// Set the zoom multiplier for the next recording (clamped to a sane range).
    pub fn set_zoom_amount(&self, amount: f64) -> Result<(), String> {
        *self.pending_zoom.lock().map_err(|_| "lock poisoned")? = amount.clamp(1.0, 4.0);
        Ok(())
    }

    /// Grab a single full-display frame and return it as a `data:image/png;base64,…` URL —
    /// the still backdrop the region selector draws on (no transparent window needed).
    pub fn screenshot(&self) -> Result<String, String> {
        let (rx, handle) = spawn_region(None);
        let frame = rx
            .recv_timeout(std::time::Duration::from_secs(3))
            .map_err(|e| format!("screenshot capture failed: {e}"))?;
        handle.stop();
        let img = RgbaImage::new(frame.width, frame.height, swizzle_rb(&frame.bgra));
        let png = encode_png_to_vec(&img).map_err(|e| e.to_string())?;
        let b64 = base64::engine::general_purpose::STANDARD.encode(&png);
        Ok(format!("data:image/png;base64,{b64}"))
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
        let amount = *self.pending_zoom.lock().map_err(|_| "lock poisoned")?;
        let (frames_rx, capture) = spawn_region(region);
        let (recorder, events_rx) = InputRecorder::start();
        // Independent live preview — its own capture, so it can never disturb the recording.
        let preview = LivePreview::start(region, amount, self.preview.sink());
        *active = Some(Active {
            frames_rx,
            capture,
            region,
            recorder,
            events_rx,
            start_qpc: self.clock.now(),
            _preview: preview,
        });
        Ok(())
    }

    /// Stop capturing and build the editable project (plan zooms, simulate the camera).
    pub fn stop_recording(&self) -> Result<RecordingSummary, String> {
        let mut active = self.active.lock().map_err(|_| "lock poisoned")?;
        let Some(mut session) = active.take() else {
            return Err("not recording".into());
        };
        session._preview.stop(); // tear down the live monitor before post-processing
        session.capture.stop();
        session.recorder.stop();

        let frames: Vec<CapturedFrame> = session.frames_rx.try_iter().collect();
        let raw_events: Vec<RawEvent> = session.events_rx.try_iter().collect();

        // No frames means the screen capture never started (or was stopped instantly) — fail
        // loudly so the editor shows a clear message instead of a silent, empty player.
        if frames.is_empty() {
            return Err("No frames were captured — screen capture failed to start.".into());
        }

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

        let amount = *self.pending_zoom.lock().map_err(|_| "lock poisoned")?;
        let cfg = ZoomConfig {
            amount,
            ..ZoomConfig::default()
        };
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
        project.zoom_config = cfg; // so a reopened project re-simulates at the same zoom level
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
        let mut scene = build_scene(project, track, out_w, out_h, t);
        // Annotations are drawn live by the editor's interactive SVG overlay, not baked into
        // the preview — a baked copy would lag the overlay during a drag and look glitchy.
        // They ARE baked into the final GIF at export time.
        scene.texts.clear();
        scene.arrows.clear();
        scene.highlights.clear();
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

    /// Composite the output-timeline frames (honoring trim + speed regions) and export an
    /// optimized GIF to `out_path`. `progress(done, total)` is called as frames composite
    /// and once more when encoding finishes.
    ///
    /// Uses the pure-Rust encoder so export works with no external tools installed.
    pub fn export_gif(
        &self,
        out_path: String,
        fps: u32,
        width: Option<u32>,
        quality: u8,
        progress: &dyn Fn(u32, u32),
    ) -> Result<(), String> {
        let edited = self.edited.lock().map_err(|_| "lock poisoned")?;
        let images = self.composite_output_frames(&edited, fps, 1, &|done, total| {
            // Reserve the last tick for the encode step.
            progress(done, total + 1);
        })?;
        let total = images.len() as u32 + 1;

        let settings = GifSettings {
            fps,
            width,
            quality,
            ..GifSettings::readme()
        };
        export_gif_native(&images, &settings, Path::new(&out_path)).map_err(|e| e.to_string())?;
        progress(total, total);
        Ok(())
    }

    /// Estimate the export size (bytes) by encoding a strided sample of the output frames
    /// at the chosen settings and extrapolating (see `docs/06-Export.md` — GIF has no
    /// closed-form size formula).
    pub fn estimate_gif(&self, fps: u32, width: Option<u32>, quality: u8) -> Result<u64, String> {
        /// Sampled frames are strided (not consecutive), so inter-frame deltas — and GIF
        /// temporal compression — are slightly pessimistic; nudge rather than multiply.
        const MOTION_FUDGE: f64 = 1.1;
        const MAX_SAMPLES: usize = 8;

        let edited = self.edited.lock().map_err(|_| "lock poisoned")?;
        let total = self.output_frame_count(&edited, fps)?;
        let stride = (total / MAX_SAMPLES).max(1);
        let samples = self.composite_output_frames(&edited, fps, stride, &|_, _| {})?;
        if samples.is_empty() {
            return Ok(0);
        }

        let settings = GifSettings {
            fps,
            width,
            quality,
            ..GifSettings::readme()
        };
        let tmp = std::env::temp_dir().join("vuoom-size-estimate.gif");
        export_gif_native(&samples, &settings, &tmp).map_err(|e| e.to_string())?;
        let bytes = std::fs::metadata(&tmp).map_err(|e| e.to_string())?.len();
        let _ = std::fs::remove_file(&tmp);
        Ok(estimate_total_bytes(
            bytes,
            samples.len(),
            total,
            MOTION_FUDGE,
        ))
    }

    /// Number of frames the output timeline (after trim + speed) emits at `fps`.
    fn output_frame_count(&self, edited: &Edited, fps: u32) -> Result<usize, String> {
        let project = edited.project.as_ref().ok_or("no recording")?;
        let (_, span, regions) = out_mapping(project);
        let d_out = output_duration(span, &regions);
        Ok(((d_out * f64::from(fps)).ceil() as usize).max(1))
    }

    /// Composite every `stride`-th output-timeline frame (honoring trim + speed regions).
    fn composite_output_frames(
        &self,
        edited: &Edited,
        fps: u32,
        stride: usize,
        progress: &dyn Fn(u32, u32),
    ) -> Result<Vec<RgbaImage>, String> {
        let compositor = self.compositor.as_ref().ok_or("no GPU compositor")?;
        let project = edited.project.as_ref().ok_or("no recording")?;
        let track = edited.track.as_ref().ok_or("no recording")?;
        if edited.frames.is_empty() {
            return Err("no frames".into());
        }

        let (out_w, out_h) = project.output_dims();
        let bg = background_color(&project.frame);
        let (t0, span, regions) = out_mapping(project);
        let d_out = output_duration(span, &regions);
        let total = ((d_out * f64::from(fps)).ceil() as usize).max(1);

        let count = total.div_ceil(stride.max(1));
        let mut images = Vec::with_capacity(count);
        for (done, i) in (0..total).step_by(stride.max(1)).enumerate() {
            let t_out = (i as f64 / f64::from(fps)).min(d_out);
            let t_src = t0 + output_to_source(t_out, span, &regions);
            let frame = nearest_frame(&edited.frames, self.clock, edited.start_qpc, t_src)
                .ok_or("no frames")?;
            let scene = build_scene(project, track, out_w, out_h, t_src);
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
            progress(done as u32 + 1, count as u32);
        }
        Ok(images)
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

    /// Snapshot everything the editor timeline binds to.
    pub fn clip_state(&self) -> Result<ClipState, String> {
        let edited = self.edited.lock().map_err(|_| "lock poisoned")?;
        let project = edited.project.as_ref().ok_or("no recording")?;
        Ok(ClipState {
            duration: project.source.duration,
            trim: project.trim,
            speed_regions: project.speed_regions.clone(),
            zooms: project.zooms.clone(),
        })
    }

    /// Set the trim range (seconds). A range covering the whole clip clears the trim.
    pub fn set_trim(&self, start: f64, end: f64) -> Result<(), String> {
        self.with_project(|p| {
            let d = p.source.duration;
            let s = start.clamp(0.0, d);
            let e = end.clamp(0.0, d);
            if e - s < 0.2 {
                return Err("trim range too short".into());
            }
            p.trim = if s <= 1e-6 && e >= d - 1e-6 {
                None
            } else {
                Some(Trim { start: s, end: e })
            };
            Ok(())
        })
    }

    /// Detect idle stretches (no clicks/keys/scrolls for `MIN_GAP` seconds) and mark them
    /// to play at `factor`×. Replaces any existing speed regions; returns the new list.
    pub fn auto_speed(&self, factor: f64) -> Result<Vec<SpeedRegion>, String> {
        /// An idle gap must be at least this long (s) to be worth skimming.
        const MIN_GAP: f64 = 2.5;
        /// Keep normal speed for a beat after the last action / before the next one.
        const LEAD: f64 = 0.6;
        const TAIL: f64 = 0.4;

        let mut edited = self.edited.lock().map_err(|_| "lock poisoned")?;
        let project = edited.project.as_mut().ok_or("no recording")?;
        let d = project.source.duration;
        let factor = factor.clamp(1.5, 16.0);

        let mut marks: Vec<f64> = project
            .events
            .iter()
            .filter(|e| e.is_activity())
            .map(InputEvent::t)
            .collect();
        marks.sort_by(f64::total_cmp);

        let mut regions = Vec::new();
        let mut prev = 0.0_f64;
        for m in marks.into_iter().chain(std::iter::once(d)) {
            if m - prev >= MIN_GAP {
                let start = (prev + LEAD).max(0.0);
                let end = (m - TAIL).min(d);
                if end - start > 0.5 {
                    regions.push(SpeedRegion { start, end, factor });
                }
            }
            prev = prev.max(m);
        }
        project.speed_regions = regions.clone();
        Ok(regions)
    }

    /// Remove all speed regions (play everything at 1×).
    pub fn clear_speed(&self) -> Result<(), String> {
        self.with_project(|p| {
            p.speed_regions.clear();
            Ok(())
        })
    }

    /// Insert a manual zoom segment at time `t` and re-simulate the camera.
    /// Returns the updated segment list.
    pub fn add_zoom(&self, t: f64) -> Result<Vec<ZoomKeyframe>, String> {
        const DEFAULT_LEN: f64 = 2.0;
        let mut edited = self.edited.lock().map_err(|_| "lock poisoned")?;
        let project = edited.project.as_mut().ok_or("no recording")?;
        let d = project.source.duration;
        let start = t.clamp(0.0, (d - vuoom_zoom::MIN_LEN).max(0.0));
        let end = (start + DEFAULT_LEN).clamp(
            start + vuoom_zoom::MIN_LEN,
            d.max(start + vuoom_zoom::MIN_LEN),
        );
        // If zoom was recorded "off" (amount 1.0), a manual segment still needs a real zoom.
        let amount = if project.zoom_config.amount > 1.05 {
            project.zoom_config.amount
        } else {
            1.8
        };
        let kf = ZoomKeyframe {
            start,
            end,
            amount,
            mode: ZoomMode::Auto,
            edge_snap_ratio: project.zoom_config.edge_snap_ratio,
        };
        vuoom_zoom::insert_sorted(&mut project.zooms, kf);
        let zooms = project.zooms.clone();
        resimulate(&mut edited);
        Ok(zooms)
    }

    /// Retime / re-level the zoom segment at `index` and re-simulate the camera.
    /// Returns the updated segment list.
    pub fn update_zoom(
        &self,
        index: usize,
        start: f64,
        end: f64,
        amount: f64,
    ) -> Result<Vec<ZoomKeyframe>, String> {
        let mut edited = self.edited.lock().map_err(|_| "lock poisoned")?;
        let project = edited.project.as_mut().ok_or("no recording")?;
        let d = project.source.duration;
        if !vuoom_zoom::resize(&mut project.zooms, index, start, end, d) {
            return Err("no such zoom segment".into());
        }
        if let Some(kf) = project.zooms.get_mut(index) {
            kf.amount = amount.clamp(1.1, 4.0);
        }
        vuoom_zoom::sort_by_start(&mut project.zooms);
        let zooms = project.zooms.clone();
        resimulate(&mut edited);
        Ok(zooms)
    }

    /// Delete the zoom segment at `index` and re-simulate the camera.
    /// Returns the updated segment list.
    pub fn delete_zoom(&self, index: usize) -> Result<Vec<ZoomKeyframe>, String> {
        let mut edited = self.edited.lock().map_err(|_| "lock poisoned")?;
        let project = edited.project.as_mut().ok_or("no recording")?;
        vuoom_zoom::remove(&mut project.zooms, index).ok_or("no such zoom segment")?;
        let zooms = project.zooms.clone();
        resimulate(&mut edited);
        Ok(zooms)
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
        let mut project = Project::from_json(
            &std::fs::read_to_string(dir.join("project.json")).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        // The border/framing feature was removed from the product; bundles saved by older
        // builds may still carry a decorated FrameStyle — normalize so nothing is baked in.
        project.frame = FrameStyle::default();
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

/// Recompute the camera track from the project's persisted events + (edited) zoom
/// segments, so preview and export reflect zoom edits immediately.
fn resimulate(edited: &mut Edited) {
    if let Some(p) = edited.project.as_ref() {
        edited.track = Some(simulate(
            &p.events,
            &p.zooms,
            p.source.duration,
            p.source.fps.max(1.0),
            &p.zoom_config,
        ));
    }
}

/// The output-timeline mapping inputs: the trim start `t0`, the trimmed span, and the
/// speed regions clipped + shifted into trim-local coordinates. An output time maps to
/// source time via `t0 + output_to_source(t_out, span, &regions)`.
fn out_mapping(project: &Project) -> (f64, f64, Vec<SpeedRegion>) {
    let (t0, t1) = project.active_range();
    let span = (t1 - t0).max(1e-6);
    let regions = project
        .speed_regions
        .iter()
        .filter_map(|r| {
            let s = r.start.max(t0);
            let e = r.end.min(t1);
            (e > s).then_some(SpeedRegion {
                start: s - t0,
                end: e - t0,
                factor: r.factor,
            })
        })
        .collect();
    (t0, span, regions)
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
