//! Recording → project → preview/export orchestration — the engine glue.
//!
//! Ties the pieces together: capture + global input → auto-zoom plan + camera track →
//! composite → preview stream / GIF export. Frames stream to a disk-backed
//! [`FrameStore`] while recording, so clip length is bounded by disk, not RAM — and a
//! crashed session can be recovered on the next launch. See `docs/02-Architecture.md`.
//!
//! Runtime behaviour (capture/GPU/input) is verified by running on a real Windows machine;
//! CI verifies it compiles.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Receiver;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use crate::frame_store::{self, FrameStore, FrameWriter};
use crate::live_preview::LivePreview;
use crate::zoom_chord::ZoomChordPoller;
use base64::Engine;
use glam::DVec2;
use serde::{Deserialize, Serialize};
use vuoom_capture::{spawn_region, CaptureHandle, CapturedFrame, CropRegion};
use vuoom_encode::{
    downscale_rgba, encode_png_to_vec, estimate_delta_total_bytes, export_gif_native, read_png,
    swizzle_rb, write_png, GifSettings, RgbaImage,
};
use vuoom_input::{normalize, zoom_marks, CaptureRegion, Clock, InputRecorder, RawEvent};
use vuoom_preview::{pack_frame, FrameMeta, PreviewServer};
use vuoom_project::{
    output_duration, output_to_source, ArrowAnnotation, ArrowStyle, Background, Color, FrameStyle,
    HighlightBox, HighlightShape, KeyTap, Project, Rect, Shadow, SourceInfo, SpeedRegion,
    TextAnnotation, TimeRange, Trim, ZoomConfig, ZoomKeyframe,
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

/// The monitor the next recording captures: its Win32 device name (e.g. `\\.\DISPLAY2`)
/// and virtual-desktop origin in physical px (for mapping global cursor coordinates).
#[derive(Debug, Clone)]
pub struct MonitorInfo {
    pub name: String,
    pub x: i32,
    pub y: i32,
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
    pub cuts: Vec<Trim>,
    pub zooms: Vec<ZoomKeyframe>,
    pub show_clicks: bool,
    pub show_keys: bool,
    /// Active framing preset name, derived from the padding (the editor's frame picker).
    pub frame_preset: String,
}

struct Active {
    /// Streams frames from the capture channel to the disk store (see `frame_store`).
    drain: Option<JoinHandle<Result<FrameStore, String>>>,
    /// Tells the drain thread to stop waiting for further frames.
    drain_stop: Arc<AtomicBool>,
    capture: CaptureHandle,
    recorder: InputRecorder,
    events_rx: Receiver<RawEvent>,
    start_qpc: i64,
    region: Option<CropRegion>,
    /// Virtual-desktop origin (physical px) of the captured monitor.
    mon_origin: (i32, i32),
    /// Poll-based Ctrl+Shift+Z recorder — catches chord presses the keyboard hook misses
    /// (e.g. while an elevated window has focus). Merged with hook marks at stop time.
    zoom_poll: ZoomChordPoller,
    /// Pause spans `(start_qpc, end_qpc)` — an open span means "currently paused".
    /// Converted to cuts at stop time, so pauses stay editable in the timeline.
    pauses: Vec<(i64, Option<i64>)>,
    /// Decoupled live "director's monitor" — dropped (and stopped) when recording ends.
    _preview: LivePreview,
}

#[derive(Default)]
struct Edited {
    frames: Option<FrameStore>,
    project: Option<Project>,
    track: Option<CameraTrack>,
    start_qpc: i64,
    /// Undo history: `(coalesce_tag, project_before_the_edit)`. The tag lets rapid-fire
    /// edits (typing, slider drags) collapse into one undo step.
    undo: Vec<(String, Project)>,
    redo: Vec<Project>,
}

/// Undo history depth (project snapshots are small — frames are not copied).
const UNDO_CAP: usize = 100;

/// Record the current project state before a mutation. Consecutive snapshots carrying the
/// same non-empty `tag` coalesce: only the first keeps its pre-state, so a typing run or a
/// slider drag undoes as a single step. Any snapshot clears the redo branch.
fn snapshot(edited: &mut Edited, tag: &str) {
    let Some(p) = edited.project.as_ref() else {
        return;
    };
    edited.redo.clear();
    if !tag.is_empty() && edited.undo.last().is_some_and(|(t, _)| t == tag) {
        return;
    }
    edited.undo.push((tag.to_string(), p.clone()));
    if edited.undo.len() > UNDO_CAP {
        edited.undo.remove(0);
    }
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
    /// The capture region chosen by the selector for the next recording (physical px,
    /// monitor-relative); `None` = the full monitor.
    pending_region: Mutex<Option<CropRegion>>,
    /// The monitor the next recording captures; `None` = primary.
    pending_monitor: Mutex<Option<MonitorInfo>>,
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
            pending_monitor: Mutex::new(None),
            pending_zoom: Mutex::new(ZoomConfig::default().amount),
        })
    }

    /// Set the capture region (physical px) for the next recording; `None` = full display.
    pub fn set_region(&self, region: Option<CropRegion>) -> Result<(), String> {
        *self.pending_region.lock().map_err(|_| "lock poisoned")? = region;
        Ok(())
    }

    /// Set the monitor the next recording (and its selector screenshot) captures.
    pub fn set_monitor(&self, monitor: Option<MonitorInfo>) -> Result<(), String> {
        *self.pending_monitor.lock().map_err(|_| "lock poisoned")? = monitor;
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
        let monitor = self
            .pending_monitor
            .lock()
            .map_err(|_| "lock poisoned")?
            .as_ref()
            .map(|m| m.name.clone());
        let (rx, handle) = spawn_region(None, monitor);
        let frame = rx.recv_timeout(std::time::Duration::from_secs(3));
        // Stop the capture thread on every path — including a timeout — so a slow or failed
        // grab can't leak a live capture session and its GPU/duplication resources.
        handle.stop();
        let frame = frame.map_err(|e| format!("screenshot capture failed: {e}"))?;
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
        let monitor = self
            .pending_monitor
            .lock()
            .map_err(|_| "lock poisoned")?
            .clone();
        let mon_name = monitor.as_ref().map(|m| m.name.clone());
        let mon_origin = monitor.as_ref().map_or((0, 0), |m| (m.x, m.y));
        let amount = *self.pending_zoom.lock().map_err(|_| "lock poisoned")?;

        // Drop the previous clip BEFORE recreating the store files: its open handles point
        // at the same recovery directory this recording is about to replace.
        *self.edited.lock().map_err(|_| "lock poisoned")? = Edited::default();
        let writer = FrameWriter::create(frame_store::recovery_dir())?;

        let (frames_rx, capture) = spawn_region(region, mon_name.clone());
        let (recorder, events_rx) = InputRecorder::start();
        // Independent live preview — its own capture, so it can never disturb the recording.
        let preview = LivePreview::start(region, mon_name, mon_origin, amount, self.preview.sink());

        // Stream frames straight to disk so recording length is bounded by disk, not RAM.
        let drain_stop = Arc::new(AtomicBool::new(false));
        let stop_flag = Arc::clone(&drain_stop);
        let drain = std::thread::spawn(move || -> Result<FrameStore, String> {
            let mut writer = writer;
            loop {
                match frames_rx.recv_timeout(Duration::from_millis(200)) {
                    Ok(f) => writer.push(&f)?,
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                        if stop_flag.load(Ordering::Relaxed) {
                            break;
                        }
                    }
                    Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
                }
            }
            writer.finish()
        });

        *active = Some(Active {
            drain: Some(drain),
            drain_stop,
            capture,
            region,
            mon_origin,
            recorder,
            events_rx,
            start_qpc: self.clock.now(),
            zoom_poll: ZoomChordPoller::start(),
            pauses: Vec::new(),
            _preview: preview,
        });
        Ok(())
    }

    /// Pause / resume the running recording. Capture keeps running; the paused span is
    /// turned into a cut at stop time, so it never appears in the output (and can be
    /// restored in the editor if the pause was a mistake).
    pub fn set_record_paused(&self, paused: bool) -> Result<(), String> {
        let mut active = self.active.lock().map_err(|_| "lock poisoned")?;
        let session = active.as_mut().ok_or("not recording")?;
        let now = self.clock.now();
        let open = session.pauses.last().is_some_and(|(_, e)| e.is_none());
        if paused && !open {
            session.pauses.push((now, None));
        } else if !paused && open {
            if let Some((_, e)) = session.pauses.last_mut() {
                *e = Some(now);
            }
        }
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

        // Let the drain thread flush remaining frames and hand back the disk store.
        session.drain_stop.store(true, Ordering::Relaxed);
        let store = session
            .drain
            .take()
            .ok_or("recording already stopped")?
            .join()
            .map_err(|_| "frame drain thread panicked")??;
        let raw_events: Vec<RawEvent> = session.events_rx.try_iter().collect();

        // No frames means the screen capture never started (or was stopped instantly) — fail
        // loudly so the editor shows a clear message instead of a silent, empty player.
        if store.is_empty() {
            return Err("No frames were captured — screen capture failed to start.".into());
        }

        let (width, height) = store.recs().first().map_or((1920, 1080), |r| (r.w, r.h));
        let duration = store.recs().last().map_or(0.0, |r| {
            self.clock.seconds_between(session.start_qpc, r.qpc)
        });

        // Map the cursor into the captured area. Cursor events are in virtual-desktop
        // physical coords; the crop is monitor-relative, so offset by the monitor origin.
        let (mx, my) = session.mon_origin;
        let region = match session.region {
            Some(r) => CaptureRegion {
                x: mx + r.x as i32,
                y: my + r.y as i32,
                w: r.w as i32,
                h: r.h as i32,
            },
            None => CaptureRegion {
                x: mx,
                y: my,
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

        // Merge in poll-detected chord presses the hook missed (e.g. elevated-window
        // focus) — without this, the live preview can show a zoom that the final edit
        // silently drops. Hook marks within 0.3s win to avoid duplicates.
        let hook_mark_times: Vec<f64> = events
            .iter()
            .filter(|e| e.is_zoom_mark())
            .map(InputEvent::t)
            .collect();
        for m in session.zoom_poll.finish() {
            let t = self.clock.seconds_between(session.start_qpc, m.qpc);
            if t < 0.0 || t > duration {
                continue;
            }
            if hook_mark_times.iter().any(|&h| (h - t).abs() < 0.3) {
                continue;
            }
            let pos = DVec2::new(
                (f64::from(m.x - region.x) / f64::from(region.w.max(1))).clamp(0.0, 1.0),
                (f64::from(m.y - region.y) / f64::from(region.h.max(1))).clamp(0.0, 1.0),
            );
            events.push(InputEvent::ZoomMark { t, pos });
        }
        events.sort_by(|a, b| a.t().total_cmp(&b.t()));

        let amount = *self.pending_zoom.lock().map_err(|_| "lock poisoned")?;
        let cfg = ZoomConfig {
            amount,
            ..ZoomConfig::default()
        };
        let zooms = plan_zooms(&events, duration, &cfg);
        let fps = if duration > 0.0 {
            store.len() as f64 / duration
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
        let frame_count = store.len();
        project.zoom_config = cfg; // so a reopened project re-simulates at the same zoom level
        project.zooms = zooms;
        project.events = events; // persisted so a reopened project can re-simulate panning

        // Shortcut/special key taps for the optional keystroke overlay. Plain typing is
        // deliberately never labeled (privacy + noise) — see vuoom_input::keys.
        project.key_taps = extract_key_taps(&raw_events, self.clock, session.start_qpc, duration);

        // Paused spans become ordinary cuts: skipped by playback/export, but visible and
        // restorable in the editor if a pause was hit by mistake.
        let mut cuts: Vec<Trim> = session
            .pauses
            .iter()
            .filter_map(|&(s, e)| {
                let start = self.clock.seconds_between(session.start_qpc, s).max(0.0);
                let end = e
                    .map_or(duration, |e| {
                        self.clock.seconds_between(session.start_qpc, e)
                    })
                    .min(duration);
                (end - start > 0.05).then_some(Trim { start, end })
            })
            .collect();
        sort_cuts(&mut cuts);
        project.cuts = cuts;

        // Persist the manifest next to the on-disk frames: together they make the
        // recording recoverable if the app crashes or is closed before exporting.
        if let Ok(json) = project.to_json() {
            let _ = std::fs::write(
                frame_store::project_path(&frame_store::recovery_dir()),
                json,
            );
        }

        let mut edited = self.edited.lock().map_err(|_| "lock poisoned")?;
        // Fresh clip → fresh (empty) undo history.
        *edited = Edited {
            frames: Some(store),
            project: Some(project),
            track: Some(track),
            start_qpc: session.start_qpc,
            ..Edited::default()
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
        let store = edited.frames.as_ref().ok_or("no frames")?;
        let idx = nearest_idx(store, self.clock, edited.start_qpc, t).ok_or("no frames")?;
        let frame = store.frame(idx)?;

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

    /// Composite the output timeline and encode an H.264 MP4 to `out_path`, streaming one
    /// frame at a time (no full-clip RAM spike, unlike GIF which needs a global palette).
    pub fn export_mp4(
        &self,
        out_path: String,
        fps: u32,
        width: Option<u32>,
        quality: u8,
        progress: &dyn Fn(u32, u32),
    ) -> Result<(), String> {
        let edited = self.edited.lock().map_err(|_| "lock poisoned")?;
        let compositor = self.compositor.as_ref().ok_or("no GPU compositor")?;
        let project = edited.project.as_ref().ok_or("no recording")?;
        let track = edited.track.as_ref().ok_or("no recording")?;
        let store = edited.frames.as_ref().ok_or("no frames")?;
        if store.is_empty() {
            return Err("no frames".into());
        }

        let (out_w, out_h) = project.output_dims();
        let bg = background_color(&project.frame);
        let (t0, span, regions, cuts) = out_mapping(project);
        let d_out = output_duration(span, &regions, &cuts);
        let total = ((d_out * f64::from(fps)).ceil() as usize).max(1);

        // Optional max-width downscale; H.264 wants even dimensions, so floor to even and
        // let the encoder crop the stray right/bottom line.
        let scale_w = width.filter(|&w| w > 0 && w < out_w);
        let (enc_src_w, enc_src_h) = match scale_w {
            Some(w) => (
                w,
                ((u64::from(out_h) * u64::from(w)) / u64::from(out_w)).max(1) as u32,
            ),
            None => (out_w, out_h),
        };
        let (enc_w, enc_h) = ((enc_src_w & !1).max(2), (enc_src_h & !1).max(2));

        let encoder =
            crate::mp4::Mp4Encoder::new(Path::new(&out_path), enc_w, enc_h, fps, quality)?;
        // Track the first frame error instead of `?`-ing out mid-stream, so a failure can
        // delete the half-written file rather than leaving a corrupt .mp4 at the user's path.
        let mut frame_err: Option<String> = None;
        for i in 0..total {
            let t_out = (i as f64 / f64::from(fps)).min(d_out);
            let t_src = t0 + output_to_source(t_out, span, &regions, &cuts);
            let idx = match nearest_idx(store, self.clock, edited.start_qpc, t_src) {
                Some(idx) => idx,
                None => {
                    frame_err = Some("no frames".into());
                    break;
                }
            };
            let frame = match store.frame(idx) {
                Ok(frame) => frame,
                Err(e) => {
                    frame_err = Some(e);
                    break;
                }
            };
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
            let img = RgbaImage::new(out_w, out_h, rgba);
            let img = if scale_w.is_some() {
                downscale_rgba(&img, enc_src_w)
            } else {
                img
            };
            if let Err(e) = encoder.write_rgba(&img.pixels, img.width, img.height, i as u32) {
                frame_err = Some(e);
                break;
            }
            progress(i as u32 + 1, total as u32 + 1);
        }
        let result = match frame_err {
            Some(e) => Err(e),
            None => encoder.finish(),
        };
        if result.is_err() {
            let _ = std::fs::remove_file(&out_path);
        }
        result?;
        progress(total as u32 + 1, total as u32 + 1);
        Ok(())
    }

    /// Estimate the export size (bytes) by encoding contiguous sample windows of the
    /// output frames at the chosen settings and extrapolating (see `docs/06-Export.md` —
    /// GIF has no closed-form size formula).
    ///
    /// Windows must be contiguous: the encoder delta-compresses consecutive frames, so a
    /// strided sample would see artificially large frame-to-frame changes and wildly
    /// overestimate. A 1-frame encode per window isolates keyframe cost from delta cost.
    pub fn estimate_gif(&self, fps: u32, width: Option<u32>, quality: u8) -> Result<u64, String> {
        /// Sample windows can still miss the clip's busiest stretch; nudge up.
        const MOTION_FUDGE: f64 = 1.15;
        const WINDOW: usize = 12;

        let edited = self.edited.lock().map_err(|_| "lock poisoned")?;
        let total = self.output_frame_count(&edited, fps)?;
        let win = WINDOW.min(total);
        // One mid-clip window for short clips, two spread out for longer ones.
        let starts: Vec<usize> = if total >= 4 * win {
            vec![total / 5, total * 3 / 5]
        } else {
            vec![(total - win) / 2]
        };

        let settings = GifSettings {
            fps,
            width,
            quality,
            ..GifSettings::readme()
        };
        let mut windows: Vec<(u64, u64, usize)> = Vec::with_capacity(starts.len());
        for (k, &start) in starts.iter().enumerate() {
            let start = start.min(total - win);
            let indices: Vec<usize> = (start..start + win).collect();
            let frames = self.composite_indices(&edited, fps, &indices, &|_, _| {})?;
            if frames.is_empty() {
                continue;
            }
            let window_bytes = encode_sample_bytes(&frames, &settings, k, "win")?;
            let key_bytes = encode_sample_bytes(&frames[0..1], &settings, k, "key")?;
            windows.push((window_bytes, key_bytes, frames.len()));
        }
        Ok(estimate_delta_total_bytes(&windows, total, MOTION_FUDGE))
    }

    /// Number of frames the output timeline (after trim + speed) emits at `fps`.
    fn output_frame_count(&self, edited: &Edited, fps: u32) -> Result<usize, String> {
        let project = edited.project.as_ref().ok_or("no recording")?;
        let (_, span, regions, cuts) = out_mapping(project);
        let d_out = output_duration(span, &regions, &cuts);
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
        let total = self.output_frame_count(edited, fps)?;
        let indices: Vec<usize> = (0..total).step_by(stride.max(1)).collect();
        self.composite_indices(edited, fps, &indices, progress)
    }

    /// Composite specific output-timeline frame indices (honoring trim + speed regions).
    fn composite_indices(
        &self,
        edited: &Edited,
        fps: u32,
        indices: &[usize],
        progress: &dyn Fn(u32, u32),
    ) -> Result<Vec<RgbaImage>, String> {
        let compositor = self.compositor.as_ref().ok_or("no GPU compositor")?;
        let project = edited.project.as_ref().ok_or("no recording")?;
        let track = edited.track.as_ref().ok_or("no recording")?;
        let store = edited.frames.as_ref().ok_or("no frames")?;
        if store.is_empty() {
            return Err("no frames".into());
        }

        let (out_w, out_h) = project.output_dims();
        let bg = background_color(&project.frame);
        let (t0, span, regions, cuts) = out_mapping(project);
        let d_out = output_duration(span, &regions, &cuts);

        let mut images = Vec::with_capacity(indices.len());
        for (done, &i) in indices.iter().enumerate() {
            let t_out = (i as f64 / f64::from(fps)).min(d_out);
            let t_src = t0 + output_to_source(t_out, span, &regions, &cuts);
            let idx = nearest_idx(store, self.clock, edited.start_qpc, t_src).ok_or("no frames")?;
            let frame = store.frame(idx)?;
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
            progress(done as u32 + 1, indices.len() as u32);
        }
        Ok(images)
    }

    /// Add a text label at normalized `(x, y)`, visible for ~3s from time `t`. Returns its id.
    pub fn add_text(&self, text: String, x: f64, y: f64, t: f64) -> Result<u32, String> {
        let mut edited = self.edited.lock().map_err(|_| "lock poisoned")?;
        snapshot(&mut edited, "");
        let project = edited.project.as_mut().ok_or("no recording")?;
        let id = next_id(project);
        let range = TimeRange::with_fade(t, default_end(t, project.source.duration), 0.2);
        project.texts.push(TextAnnotation {
            id,
            text,
            pos: DVec2::new(x, y),
            font_size: 0.05,
            color: Color::WHITE,
            bold: true, // labels over video read best bold; toggleable in the inspector
            italic: false,
            background: false,
            font: String::new(),
            range,
        });
        Ok(id)
    }

    /// Add an arrow between normalized points, visible for ~3s from time `t`. Returns its id.
    pub fn add_arrow(&self, fx: f64, fy: f64, tx: f64, ty: f64, t: f64) -> Result<u32, String> {
        let mut edited = self.edited.lock().map_err(|_| "lock poisoned")?;
        snapshot(&mut edited, "");
        let project = edited.project.as_mut().ok_or("no recording")?;
        let id = next_id(project);
        let range = TimeRange::with_fade(t, default_end(t, project.source.duration), 0.2);
        project.arrows.push(ArrowAnnotation {
            id,
            from: DVec2::new(fx, fy),
            to: DVec2::new(tx, ty),
            color: Color::rgb(0.95, 0.25, 0.25),
            thickness: 0.006,
            style: ArrowStyle::Arrow,
            range,
        });
        Ok(id)
    }

    /// Add a highlight box (normalized rect), visible for ~3s from time `t`. Returns its id.
    pub fn add_box(&self, x: f64, y: f64, w: f64, h: f64, t: f64) -> Result<u32, String> {
        self.add_highlight(x, y, w, h, t, HighlightShape::Rect)
    }

    /// Add an ellipse highlight inscribed in the normalized rect. Returns its id.
    pub fn add_ellipse(&self, x: f64, y: f64, w: f64, h: f64, t: f64) -> Result<u32, String> {
        self.add_highlight(x, y, w, h, t, HighlightShape::Ellipse)
    }

    fn add_highlight(
        &self,
        x: f64,
        y: f64,
        w: f64,
        h: f64,
        t: f64,
        shape: HighlightShape,
    ) -> Result<u32, String> {
        let mut edited = self.edited.lock().map_err(|_| "lock poisoned")?;
        snapshot(&mut edited, "");
        let project = edited.project.as_mut().ok_or("no recording")?;
        let id = next_id(project);
        let range = TimeRange::with_fade(t, default_end(t, project.source.duration), 0.2);
        project.highlights.push(HighlightBox {
            id,
            rect: Rect::new(x, y, w, h),
            color: Color::rgb(1.0, 0.82, 0.1),
            thickness: 0.005,
            filled: false,
            shape,
            range,
        });
        Ok(id)
    }

    /// Add a translucent filled highlighter (marker-style) rectangle. Returns its id.
    pub fn add_highlighter(&self, x: f64, y: f64, w: f64, h: f64, t: f64) -> Result<u32, String> {
        let mut edited = self.edited.lock().map_err(|_| "lock poisoned")?;
        snapshot(&mut edited, "");
        let project = edited.project.as_mut().ok_or("no recording")?;
        let id = next_id(project);
        let range = TimeRange::with_fade(t, default_end(t, project.source.duration), 0.2);
        project.highlights.push(HighlightBox {
            id,
            rect: Rect::new(x, y, w, h),
            // Warm marker yellow at low opacity so content shows through.
            color: Color::rgba(1.0, 0.86, 0.18, 0.35),
            thickness: 0.005,
            filled: true,
            shape: HighlightShape::Rect,
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
            cuts: project.cuts.clone(),
            zooms: project.zooms.clone(),
            show_clicks: project.show_clicks,
            show_keys: project.show_keys,
            frame_preset: match project.frame.padding {
                p if p <= 0.0 => "none",
                p if p < 0.06 => "subtle",
                _ => "studio",
            }
            .into(),
        })
    }

    /// Toggle click ripples (drawn in both the preview and the exported GIF).
    pub fn set_show_clicks(&self, on: bool) -> Result<(), String> {
        self.with_project("", |p| {
            p.show_clicks = on;
            Ok(())
        })
    }

    /// Toggle the keystroke overlay (shortcut chips at the bottom of the frame).
    pub fn set_show_keys(&self, on: bool) -> Result<(), String> {
        self.with_project("", |p| {
            p.show_keys = on;
            Ok(())
        })
    }

    /// Apply a framing preset: `"none"` (edge-to-edge), `"subtle"` (slim dark mat), or
    /// `"studio"` (generous light mat + shadow). The compositor renders padding, rounded
    /// corners and shadow; preview and export both honor it.
    pub fn set_frame_preset(&self, preset: &str) -> Result<(), String> {
        self.with_project("", |p| {
            p.frame = match preset {
                "subtle" => FrameStyle {
                    background: Background::Solid(Color::rgb(0.10, 0.11, 0.12)),
                    padding: 0.04,
                    corner_radius: 0.012,
                    shadow: Shadow {
                        strength: 0.3,
                        ..Shadow::default()
                    },
                },
                "studio" => FrameStyle {
                    background: Background::Solid(Color::rgb(0.90, 0.89, 0.87)),
                    padding: 0.075,
                    corner_radius: 0.02,
                    shadow: Shadow {
                        strength: 0.5,
                        ..Shadow::default()
                    },
                },
                _ => FrameStyle::default(),
            };
            Ok(())
        })
    }

    /// Set the trim range (seconds). A range covering the whole clip clears the trim.
    pub fn set_trim(&self, start: f64, end: f64) -> Result<(), String> {
        self.with_project("", |p| {
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
        snapshot(&mut edited, "");
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
        self.with_project("", |p| {
            p.speed_regions.clear();
            Ok(())
        })
    }

    /// Manually mark `[start, end]` to play at `factor`×. Returns the updated, sorted list.
    pub fn add_speed_region(
        &self,
        start: f64,
        end: f64,
        factor: f64,
    ) -> Result<Vec<SpeedRegion>, String> {
        let mut edited = self.edited.lock().map_err(|_| "lock poisoned")?;
        snapshot(&mut edited, "");
        let project = edited.project.as_mut().ok_or("no recording")?;
        let d = project.source.duration;
        let s = start.min(end).clamp(0.0, (d - 0.2).max(0.0));
        let e = end.max(start).clamp(s + 0.2, d.max(s + 0.2));
        project.speed_regions.push(SpeedRegion {
            start: s,
            end: e,
            factor: factor.clamp(1.25, 16.0),
        });
        sort_speed(&mut project.speed_regions);
        Ok(project.speed_regions.clone())
    }

    /// Retime / re-level the speed region at `index`. Returns the updated, sorted list.
    pub fn update_speed_region(
        &self,
        index: usize,
        start: f64,
        end: f64,
        factor: f64,
    ) -> Result<Vec<SpeedRegion>, String> {
        let mut edited = self.edited.lock().map_err(|_| "lock poisoned")?;
        snapshot(&mut edited, "");
        let project = edited.project.as_mut().ok_or("no recording")?;
        let d = project.source.duration;
        let s = start.min(end).clamp(0.0, (d - 0.2).max(0.0));
        let e = end.max(start).clamp(s + 0.2, d.max(s + 0.2));
        let r = project
            .speed_regions
            .get_mut(index)
            .ok_or("no such speed region")?;
        *r = SpeedRegion {
            start: s,
            end: e,
            factor: factor.clamp(1.25, 16.0),
        };
        sort_speed(&mut project.speed_regions);
        Ok(project.speed_regions.clone())
    }

    /// Delete the speed region at `index`. Returns the updated list.
    pub fn delete_speed_region(&self, index: usize) -> Result<Vec<SpeedRegion>, String> {
        let mut edited = self.edited.lock().map_err(|_| "lock poisoned")?;
        snapshot(&mut edited, "");
        let project = edited.project.as_mut().ok_or("no recording")?;
        if index >= project.speed_regions.len() {
            return Err("no such speed region".into());
        }
        project.speed_regions.remove(index);
        Ok(project.speed_regions.clone())
    }

    /// Remove `[start, end]` from the output entirely. Returns the updated, sorted list.
    pub fn add_cut(&self, start: f64, end: f64) -> Result<Vec<Trim>, String> {
        let mut edited = self.edited.lock().map_err(|_| "lock poisoned")?;
        snapshot(&mut edited, "");
        let project = edited.project.as_mut().ok_or("no recording")?;
        let d = project.source.duration;
        let s = start.min(end).clamp(0.0, (d - 0.1).max(0.0));
        let e = end.max(start).clamp(s + 0.1, d.max(s + 0.1));
        project.cuts.push(Trim { start: s, end: e });
        sort_cuts(&mut project.cuts);
        Ok(project.cuts.clone())
    }

    /// Retime the cut at `index`. Returns the updated, sorted list.
    pub fn update_cut(&self, index: usize, start: f64, end: f64) -> Result<Vec<Trim>, String> {
        let mut edited = self.edited.lock().map_err(|_| "lock poisoned")?;
        snapshot(&mut edited, "");
        let project = edited.project.as_mut().ok_or("no recording")?;
        let d = project.source.duration;
        let s = start.min(end).clamp(0.0, (d - 0.1).max(0.0));
        let e = end.max(start).clamp(s + 0.1, d.max(s + 0.1));
        let c = project.cuts.get_mut(index).ok_or("no such cut")?;
        *c = Trim { start: s, end: e };
        sort_cuts(&mut project.cuts);
        Ok(project.cuts.clone())
    }

    /// Restore the cut at `index` (the section plays again). Returns the updated list.
    pub fn delete_cut(&self, index: usize) -> Result<Vec<Trim>, String> {
        let mut edited = self.edited.lock().map_err(|_| "lock poisoned")?;
        snapshot(&mut edited, "");
        let project = edited.project.as_mut().ok_or("no recording")?;
        if index >= project.cuts.len() {
            return Err("no such cut".into());
        }
        project.cuts.remove(index);
        Ok(project.cuts.clone())
    }

    /// Insert a manual zoom segment at time `t` and re-simulate the camera.
    /// Returns the updated segment list.
    pub fn add_zoom(&self, t: f64) -> Result<Vec<ZoomKeyframe>, String> {
        const DEFAULT_LEN: f64 = 2.0;
        let mut edited = self.edited.lock().map_err(|_| "lock poisoned")?;
        snapshot(&mut edited, "");
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
        snapshot(&mut edited, "");
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

    /// Set how the zoom segment at `index` picks its focus: `Some((x, y))` holds a fixed
    /// normalized point, `None` follows the cursor. Returns the updated segment list.
    pub fn set_zoom_focus(
        &self,
        index: usize,
        focus: Option<(f64, f64)>,
    ) -> Result<Vec<ZoomKeyframe>, String> {
        let mut edited = self.edited.lock().map_err(|_| "lock poisoned")?;
        snapshot(&mut edited, "");
        let project = edited.project.as_mut().ok_or("no recording")?;
        let kf = project.zooms.get_mut(index).ok_or("no such zoom segment")?;
        kf.mode = match focus {
            Some((x, y)) => ZoomMode::Manual {
                pos: DVec2::new(x.clamp(0.0, 1.0), y.clamp(0.0, 1.0)),
            },
            None => ZoomMode::Auto,
        };
        let zooms = project.zooms.clone();
        resimulate(&mut edited);
        Ok(zooms)
    }

    /// Delete the zoom segment at `index` and re-simulate the camera.
    /// Returns the updated segment list.
    pub fn delete_zoom(&self, index: usize) -> Result<Vec<ZoomKeyframe>, String> {
        let mut edited = self.edited.lock().map_err(|_| "lock poisoned")?;
        snapshot(&mut edited, "");
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
    #[allow(clippy::too_many_arguments)]
    #[allow(clippy::too_many_arguments)]
    pub fn update_text(
        &self,
        id: u32,
        x: Option<f64>,
        y: Option<f64>,
        text: Option<String>,
        font_size: Option<f32>,
        bold: Option<bool>,
        italic: Option<bool>,
        background: Option<bool>,
        font: Option<String>,
    ) -> Result<(), String> {
        // Typing and the size slider fire per keystroke / per tick — coalesce each run
        // into one undo step. Geometry / style commits stay discrete.
        let tag = if text.is_some() {
            format!("text:{id}")
        } else if font_size.is_some() {
            format!("font:{id}")
        } else {
            String::new()
        };
        self.with_project(&tag, |p| {
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
            if let Some(b) = bold {
                a.bold = b;
            }
            if let Some(i) = italic {
                a.italic = i;
            }
            if let Some(bg) = background {
                a.background = bg;
            }
            if let Some(f) = font {
                a.font = f;
            }
            Ok(())
        })
    }

    /// Move an arrow's endpoints.
    pub fn update_arrow(&self, id: u32, fx: f64, fy: f64, tx: f64, ty: f64) -> Result<(), String> {
        self.with_project("", |p| {
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
        self.with_project("", |p| {
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
        let (r, g, b) = (r as f32, g as f32, b as f32);
        // The color picker streams values while dragging — coalesce into one undo step.
        // Preserve each element's current alpha so recoloring a highlighter keeps its opacity.
        self.with_project(&format!("color:{id}"), |p| {
            if let Some(a) = p.texts.iter_mut().find(|a| a.id == id) {
                a.color = Color::rgba(r, g, b, a.color.a);
            } else if let Some(a) = p.arrows.iter_mut().find(|a| a.id == id) {
                a.color = Color::rgba(r, g, b, a.color.a);
            } else if let Some(a) = p.highlights.iter_mut().find(|a| a.id == id) {
                a.color = Color::rgba(r, g, b, a.color.a);
            } else {
                return Err("no such annotation".into());
            }
            Ok(())
        })
    }

    /// Set the alpha (0..1) of any annotation's color — backs the opacity slider.
    pub fn set_annotation_opacity(&self, id: u32, a: f64) -> Result<(), String> {
        let alpha = (a as f32).clamp(0.0, 1.0);
        self.with_project(&format!("opacity:{id}"), |p| {
            if let Some(x) = p.texts.iter_mut().find(|x| x.id == id) {
                x.color = x.color.with_alpha(alpha);
            } else if let Some(x) = p.arrows.iter_mut().find(|x| x.id == id) {
                x.color = x.color.with_alpha(alpha);
            } else if let Some(x) = p.highlights.iter_mut().find(|x| x.id == id) {
                x.color = x.color.with_alpha(alpha);
            } else {
                return Err("no such annotation".into());
            }
            Ok(())
        })
    }

    /// Switch a highlight between rectangle and ellipse.
    pub fn set_highlight_shape(&self, id: u32, ellipse: bool) -> Result<(), String> {
        self.with_project(&format!("shape:{id}"), |p| {
            let b = p
                .highlights
                .iter_mut()
                .find(|b| b.id == id)
                .ok_or("no such highlight")?;
            b.shape = if ellipse {
                HighlightShape::Ellipse
            } else {
                HighlightShape::Rect
            };
            Ok(())
        })
    }

    /// Set an arrow's head style: "arrow" (single), "line" (none), or "double" (both ends).
    pub fn set_arrow_style(&self, id: u32, style: &str) -> Result<(), String> {
        self.with_project(&format!("arrowstyle:{id}"), |p| {
            let a = p
                .arrows
                .iter_mut()
                .find(|a| a.id == id)
                .ok_or("no such arrow")?;
            a.style = match style {
                "line" => ArrowStyle::Line,
                "double" => ArrowStyle::DoubleArrow,
                _ => ArrowStyle::Arrow,
            };
            Ok(())
        })
    }

    /// Restyle an arrow or highlight: stroke thickness (fraction of output height) and,
    /// for highlights, filled vs outlined. `None` fields keep their current value.
    pub fn set_annotation_style(
        &self,
        id: u32,
        thickness: Option<f64>,
        filled: Option<bool>,
    ) -> Result<(), String> {
        // The thickness slider streams values while dragging — coalesce per element.
        self.with_project(&format!("style:{id}"), |p| {
            let th = thickness.map(|t| (t as f32).clamp(0.001, 0.05));
            if let Some(a) = p.arrows.iter_mut().find(|a| a.id == id) {
                if let Some(t) = th {
                    a.thickness = t;
                }
                return Ok(());
            }
            if let Some(b) = p.highlights.iter_mut().find(|b| b.id == id) {
                if let Some(t) = th {
                    b.thickness = t;
                }
                if let Some(f) = filled {
                    b.filled = f;
                }
                return Ok(());
            }
            Err("no such arrow or highlight".into())
        })
    }

    /// Retime any annotation (text, arrow, or box): set when it appears/disappears.
    pub fn update_annotation_range(&self, id: u32, start: f64, end: f64) -> Result<(), String> {
        self.with_project("", |p| {
            let d = p.source.duration;
            let s = start.clamp(0.0, (d - 0.1).max(0.0));
            let e = end.clamp(s + 0.1, d.max(s + 0.1));
            let range = p
                .texts
                .iter_mut()
                .find(|a| a.id == id)
                .map(|a| &mut a.range)
                .or_else(|| {
                    p.arrows
                        .iter_mut()
                        .find(|a| a.id == id)
                        .map(|a| &mut a.range)
                })
                .or_else(|| {
                    p.highlights
                        .iter_mut()
                        .find(|a| a.id == id)
                        .map(|a| &mut a.range)
                })
                .ok_or("no such annotation")?;
            range.start = s;
            range.end = e;
            // Keep fades sane if the window shrank below the fade lengths.
            let span = e - s;
            range.fade_in = range.fade_in.min(span / 2.0);
            range.fade_out = range.fade_out.min(span / 2.0);
            Ok(())
        })
    }

    /// Duplicate any annotation: same style and timing, nudged down-right so the copy is
    /// visible next to the original. Returns the new id.
    pub fn duplicate_annotation(&self, id: u32) -> Result<u32, String> {
        const NUDGE: f64 = 0.03;
        let mut edited = self.edited.lock().map_err(|_| "lock poisoned")?;
        snapshot(&mut edited, "");
        let project = edited.project.as_mut().ok_or("no recording")?;
        let new_id = next_id(project);
        if let Some(t) = project.texts.iter().find(|t| t.id == id).cloned() {
            let mut c = t;
            c.id = new_id;
            c.pos = (c.pos + DVec2::splat(NUDGE)).clamp(DVec2::ZERO, DVec2::ONE);
            project.texts.push(c);
            return Ok(new_id);
        }
        if let Some(a) = project.arrows.iter().find(|a| a.id == id).copied() {
            let mut c = a;
            c.id = new_id;
            c.from = (c.from + DVec2::splat(NUDGE)).clamp(DVec2::ZERO, DVec2::ONE);
            c.to = (c.to + DVec2::splat(NUDGE)).clamp(DVec2::ZERO, DVec2::ONE);
            project.arrows.push(c);
            return Ok(new_id);
        }
        if let Some(b) = project.highlights.iter().find(|b| b.id == id).copied() {
            let mut c = b;
            c.id = new_id;
            c.rect.x = (c.rect.x + NUDGE).clamp(0.0, 1.0);
            c.rect.y = (c.rect.y + NUDGE).clamp(0.0, 1.0);
            project.highlights.push(c);
            return Ok(new_id);
        }
        Err("no such annotation".into())
    }

    /// Delete any annotation (text, arrow, or box) by id.
    pub fn delete_annotation(&self, id: u32) -> Result<(), String> {
        self.with_project("", |p| {
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
        let store = edited.frames.as_ref().ok_or("no recording")?;
        let frames_dir = dir.join("frames");
        std::fs::create_dir_all(&frames_dir).map_err(|e| e.to_string())?;

        let mut index = Vec::with_capacity(store.len());
        for n in 0..store.len() {
            let f = store.frame(n)?;
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

        // Decode into the recovery-dir frame store: one frame in memory at a time, and the
        // opened project becomes the recoverable session like a fresh recording would.
        // Drop the current clip first — its store handles point at the same files.
        *self.edited.lock().map_err(|_| "lock poisoned")? = Edited::default();
        let mut writer = FrameWriter::create(frame_store::recovery_dir())?;
        for fi in &index {
            let img = read_png(&frames_dir.join(format!("{:05}.png", fi.n)))
                .map_err(|e| e.to_string())?;
            writer.push(&CapturedFrame {
                width: fi.w,
                height: fi.h,
                bgra: swizzle_rb(&img.pixels), // RGBA on disk -> BGRA in memory
                qpc: base + (fi.t * freq as f64) as i64,
            })?;
        }
        let store = writer.finish()?;
        if let Ok(json) = project.to_json() {
            let _ = std::fs::write(
                frame_store::project_path(&frame_store::recovery_dir()),
                json,
            );
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
            frames: store.len(),
            zooms: project.zooms.len(),
        };
        let mut edited = self.edited.lock().map_err(|_| "lock poisoned")?;
        // Fresh clip → fresh (empty) undo history.
        *edited = Edited {
            frames: Some(store),
            project: Some(project),
            track: Some(track),
            start_qpc: base,
            ..Edited::default()
        };
        Ok(summary)
    }

    /// Whether a recoverable session (frames + manifest from a crash or accidental close)
    /// is sitting in the recovery directory. Returns its duration in seconds.
    pub fn recovery_available(&self) -> Option<f64> {
        let dir = frame_store::recovery_dir();
        let json = std::fs::read_to_string(frame_store::project_path(&dir)).ok()?;
        let project = Project::from_json(&json).ok()?;
        let store = FrameStore::open(&dir).ok()?;
        (!store.is_empty()).then_some(project.source.duration)
    }

    /// Reload the session left in the recovery directory (last recording + its edits as
    /// of stop time). Returns a summary like `stop_recording`.
    pub fn recover_session(&self) -> Result<RecordingSummary, String> {
        let dir = frame_store::recovery_dir();
        let project = Project::from_json(
            &std::fs::read_to_string(frame_store::project_path(&dir))
                .map_err(|e| format!("no recoverable session: {e}"))?,
        )
        .map_err(|e| e.to_string())?;
        let store = FrameStore::open(&dir)?;
        if store.is_empty() {
            return Err("no recoverable session".into());
        }

        // The stored QPC stamps all come from the crashed session, so they stay mutually
        // consistent; anchoring the epoch on the first frame reproduces the timeline
        // (within the first frame's capture latency).
        let start_qpc = store.recs().first().map_or(0, |r| r.qpc);

        let track = simulate(
            &project.events,
            &project.zooms,
            project.source.duration,
            project.source.fps.max(1.0),
            &project.zoom_config,
        );
        let summary = RecordingSummary {
            duration: project.source.duration,
            frames: store.len(),
            zooms: project.zooms.len(),
        };
        let mut edited = self.edited.lock().map_err(|_| "lock poisoned")?;
        *edited = Edited {
            frames: Some(store),
            project: Some(project),
            track: Some(track),
            start_qpc,
            ..Edited::default()
        };
        Ok(summary)
    }

    /// Run `f` against the editable project, recording an undo snapshot first.
    /// `tag` controls undo coalescing — see [`snapshot`].
    fn with_project<F>(&self, tag: &str, f: F) -> Result<(), String>
    where
        F: FnOnce(&mut Project) -> Result<(), String>,
    {
        let mut edited = self.edited.lock().map_err(|_| "lock poisoned")?;
        snapshot(&mut edited, tag);
        let project = edited.project.as_mut().ok_or("no recording")?;
        f(project)
    }

    /// Revert the most recent edit. Returns `false` if there is nothing to undo.
    pub fn undo(&self) -> Result<bool, String> {
        let mut edited = self.edited.lock().map_err(|_| "lock poisoned")?;
        let Some((_, prev)) = edited.undo.pop() else {
            return Ok(false);
        };
        if let Some(cur) = edited.project.replace(prev) {
            edited.redo.push(cur);
        }
        resimulate(&mut edited);
        Ok(true)
    }

    /// Re-apply the most recently undone edit. Returns `false` if there is nothing to redo.
    pub fn redo(&self) -> Result<bool, String> {
        let mut edited = self.edited.lock().map_err(|_| "lock poisoned")?;
        let Some(next) = edited.redo.pop() else {
            return Ok(false);
        };
        if let Some(cur) = edited.project.replace(next) {
            edited.undo.push((String::new(), cur));
        }
        resimulate(&mut edited);
        Ok(true)
    }
}

/// Encode `frames` to a throwaway GIF in the temp dir and return its byte size — the
/// measurement step of the sample-and-extrapolate size estimate.
fn encode_sample_bytes(
    frames: &[RgbaImage],
    settings: &GifSettings,
    window: usize,
    tag: &str,
) -> Result<u64, String> {
    let tmp = std::env::temp_dir().join(format!("vuoom-size-estimate-{window}-{tag}.gif"));
    export_gif_native(frames, settings, &tmp).map_err(|e| e.to_string())?;
    let bytes = std::fs::metadata(&tmp).map_err(|e| e.to_string())?.len();
    let _ = std::fs::remove_file(&tmp);
    Ok(bytes)
}

/// Index of the stored frame whose timestamp is closest to `t` (metadata only — no disk).
fn nearest_idx(store: &FrameStore, clock: Clock, start_qpc: i64, t: f64) -> Option<usize> {
    store
        .recs()
        .iter()
        .enumerate()
        .min_by(|(_, a), (_, b)| {
            let da = (clock.seconds_between(start_qpc, a.qpc) - t).abs();
            let db = (clock.seconds_between(start_qpc, b.qpc) - t).abs();
            da.total_cmp(&db)
        })
        .map(|(i, _)| i)
}

/// Turn the raw key log into overlay-worthy taps: modifier chords (`Ctrl+Shift+P`) and
/// standalone special keys (Enter, Esc, F-keys, arrows). Auto-repeat is coalesced.
fn extract_key_taps(raw: &[RawEvent], clock: Clock, start_qpc: i64, duration: f64) -> Vec<KeyTap> {
    use vuoom_input::{is_standalone, key_name, modifier, Modifier, RawEventKind};

    let (mut shift, mut ctrl, mut alt, mut win) = (false, false, false, false);
    let mut taps: Vec<KeyTap> = Vec::new();
    for e in raw {
        let down = match e.kind {
            RawEventKind::KeyDown(_) => true,
            RawEventKind::KeyUp(_) => false,
            _ => continue,
        };
        let vk = match e.kind {
            RawEventKind::KeyDown(vk) | RawEventKind::KeyUp(vk) => vk,
            _ => continue,
        };
        if let Some(m) = modifier(vk) {
            match m {
                Modifier::Shift => shift = down,
                Modifier::Ctrl => ctrl = down,
                Modifier::Alt => alt = down,
                Modifier::Win => win = down,
            }
            continue;
        }
        if !down {
            continue;
        }
        let t = clock.seconds_between(start_qpc, e.qpc);
        if t < 0.0 || t > duration {
            continue;
        }
        let Some(name) = key_name(vk) else { continue };
        let chord = ctrl || alt || win;
        if !chord && !is_standalone(vk) {
            continue; // plain typing — never labeled
        }
        let mut label = String::new();
        if win {
            label.push_str("Win+");
        }
        if ctrl {
            label.push_str("Ctrl+");
        }
        if alt {
            label.push_str("Alt+");
        }
        if shift && chord {
            label.push_str("Shift+");
        }
        label.push_str(name);
        // Coalesce key auto-repeat into the original press.
        if taps
            .last()
            .is_some_and(|p| p.label == label && t - p.t < 0.5)
        {
            continue;
        }
        taps.push(KeyTap { t, label });
    }
    taps
}

/// Keep speed regions in timeline order (the editor identifies them by sorted index).
fn sort_speed(regions: &mut [SpeedRegion]) {
    regions.sort_by(|a, b| a.start.total_cmp(&b.start));
}

/// Keep cuts in timeline order (the editor identifies them by sorted index).
fn sort_cuts(cuts: &mut [Trim]) {
    cuts.sort_by(|a, b| a.start.total_cmp(&b.start));
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
/// speed regions + cuts clipped and shifted into trim-local coordinates. An output time
/// maps to source time via `t0 + output_to_source(t_out, span, &regions, &cuts)`.
fn out_mapping(project: &Project) -> (f64, f64, Vec<SpeedRegion>, Vec<Trim>) {
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
    let cuts = project
        .cuts
        .iter()
        .filter_map(|c| {
            let s = c.start.max(t0);
            let e = c.end.min(t1);
            (e > s).then_some(Trim {
                start: s - t0,
                end: e - t0,
            })
        })
        .collect();
    (t0, span, regions, cuts)
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
