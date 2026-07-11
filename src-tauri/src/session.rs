//! Recording → project → preview/export orchestration — the engine glue.
//!
//! Ties the pieces together: capture + global input → auto-zoom plan + camera track →
//! composite → preview stream / GIF export. Frames stream to a disk-backed
//! [`FrameStore`] while recording, so clip length is bounded by disk, not RAM — and a
//! crashed session can be recovered on the next launch. See `docs/02-Architecture.md`.
//!
//! Runtime behaviour (capture/GPU/input) is verified by running on a real Windows machine;
//! CI verifies it compiles.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Receiver;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use crate::frame_store::{self, FrameRec, FrameStore, FrameWriter};
use crate::live_preview::LivePreview;
use crate::zoom_chord::{ChordMark, ZoomChordPoller};
use base64::Engine;
use glam::DVec2;
use serde::{Deserialize, Serialize};
use vuoom_capture::{spawn_region, CaptureHandle, CapturedFrame, CropRegion};
use vuoom_encode::{
    downscale_rgba, encode_png_to_vec, estimate_delta_total_bytes, export_gif_native,
    export_gif_native_streaming, read_png, swizzle_rb, write_png, GifSettings, RgbaImage,
};
use vuoom_input::{normalize, zoom_marks, CaptureRegion, Clock, InputRecorder, RawEvent};
use vuoom_preview::{pack_frame, FrameMeta, PreviewServer};
use vuoom_project::{
    output_duration, output_to_source, ArrowAnnotation, ArrowStyle, Background, Color, FrameStyle,
    HighlightBox, HighlightShape, KeyTap, Project, Rect, Shadow, SourceInfo, SpeedRegion,
    TextAnnotation, TimeRange, Trim, ZoomConfig, ZoomKeyframe,
};
use vuoom_render::{build_scene, BgFill, Compositor};
use vuoom_zoom::{plan_zooms, simulate, CameraTrack, InputEvent, ZoomMode, ZoomStyle};

/// Summary returned to the UI when recording stops.
#[derive(Debug, Clone, Serialize)]
pub struct RecordingSummary {
    pub duration: f64,
    pub frames: usize,
    pub zooms: usize,
    /// Set when the recording was truncated (e.g. the disk filled mid-capture): the clip keeps
    /// every frame written before the failure, and this message explains the shortfall so the
    /// editor can warn the user instead of the whole take failing.
    pub warning: Option<String>,
}

/// The monitor the next recording captures: its Win32 device name (e.g. `\\.\DISPLAY2`),
/// virtual-desktop origin in physical px (for mapping global cursor coordinates) and its
/// physical size (for validating a requested capture region against its bounds).
#[derive(Debug, Clone)]
pub struct MonitorInfo {
    pub name: String,
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
}

/// All annotations on the project, sent to the editor overlay so it can draw selection
/// handles and hit-test for moving/resizing.
#[derive(Debug, Clone, Serialize)]
pub struct AnnotationSet {
    pub texts: Vec<TextAnnotation>,
    pub arrows: Vec<ArrowAnnotation>,
    pub highlights: Vec<HighlightBox>,
}

/// One item in a paste payload: a full annotation snapshot from the frontend clipboard,
/// internally tagged by `kind` (`"text"`/`"arrow"`/`"box"`) so all three shapes ride one
/// ordered list — the order is preserved so the first item pasted becomes the primary
/// selection. The clipboard is self-contained (deep copies), so a paste does not depend on
/// the originals still existing.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum PasteItem {
    Text(TextAnnotation),
    Arrow(ArrowAnnotation),
    Box(HighlightBox),
}

/// A reference to one freshly-pasted annotation (its kind + new id), returned so the UI can
/// select the pastes (primary = first, the rest as extras).
#[derive(Debug, Clone, Serialize)]
pub struct PastedRef {
    pub kind: String,
    pub id: u32,
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
    /// Active backdrop preset name (`Background::preset_name`), or `""` if the backdrop is a
    /// custom one that matches no preset. Drives the background swatch picker's selection.
    pub background_preset: String,
}

/// What the drain thread hands back at stop: the disk store, an optional warning describing
/// a mid-recording disk-write truncation (frames written before the failure are kept), and a
/// flag set when the capture channel disconnected before stop was requested (capture ended
/// on its own — e.g. the monitor was unplugged).
type DrainOutcome = Result<(FrameStore, Option<String>, bool), String>;

struct Active {
    /// Streams frames from the capture channel to the disk store (see `frame_store`).
    drain: Option<JoinHandle<DrainOutcome>>,
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
    /// The rotated recovery subdir this take streams into. The final manifest is written here
    /// at stop time (not the shared root), so each take is independently recoverable.
    recovery_dir: PathBuf,
    /// Pause spans `(start_qpc, end_qpc)` — an open span means "currently paused".
    /// Converted to cuts at stop time, so pauses stay editable in the timeline.
    pauses: Vec<(i64, Option<i64>)>,
    /// Set at record start when free disk space was low (but above the hard floor): surfaced
    /// as the stop-time warning so the user knows the take started on a nearly-full disk.
    space_warning: Option<String>,
    /// Decoupled live "director's monitor" — dropped (and stopped) when recording ends.
    _preview: LivePreview,
}

#[derive(Default)]
struct Edited {
    /// `Arc` so an export can clone a handle under a short lock and then composite/encode
    /// (minutes of work) without holding the `edited` mutex — keeping scrub/edit/record
    /// responsive during export. The store reads frames from disk on demand.
    frames: Option<Arc<FrameStore>>,
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
    /// The rotated recovery subdir backing the currently-loaded clip (the active recording or
    /// an opened bundle's scratch store). Recovery scanning skips it, so we offer the
    /// *previous* unsaved session rather than the one already in the editor.
    current_recovery: Mutex<Option<PathBuf>>,
    /// Set by `cancel_export` (Cancel button / window-close) to abort an in-flight export.
    /// The GIF/MP4 loops poll it every frame and bail early, deleting the partial file. Reset
    /// to `false` at the start of each export. Only one export runs at a time from the UI, so a
    /// single flag is enough — no per-export token needed.
    export_cancel: AtomicBool,
}

impl Session {
    /// Start the preview server and GPU compositor.
    ///
    /// # Errors
    /// Returns a message if the preview WebSocket server cannot bind.
    pub fn new() -> Result<Self, String> {
        let preview = tauri::async_runtime::block_on(PreviewServer::start()).map_err(|e| {
            let msg = format!("preview server: {e}");
            tracing::error!("engine boot failed: {msg}");
            msg
        })?;
        // Clear any scratch store left by a previous run's bundle open — its gigabytes would
        // otherwise linger. Recorded sessions are pruned per-take (see `new_session_dir`).
        let _ = std::fs::remove_dir_all(frame_store::scratch_dir());
        // The GPU compositor backs both preview and export. If it can't be created (no adapter,
        // driver failure) every seek/export downstream returns "no GPU compositor" — log it
        // once here at the source instead of leaving those failures unexplained.
        let compositor = Compositor::new();
        if compositor.is_none() {
            tracing::error!("no GPU compositor available — preview and export will not work");
        }
        Ok(Self {
            preview,
            compositor,
            clock: Clock::new(),
            active: Mutex::new(None),
            edited: Mutex::new(Edited::default()),
            pending_region: Mutex::new(None),
            pending_monitor: Mutex::new(None),
            pending_zoom: Mutex::new(ZoomConfig::default().amount),
            current_recovery: Mutex::new(None),
            export_cancel: AtomicBool::new(false),
        })
    }

    /// Set the capture region (monitor-relative physical px) for the next recording;
    /// `None` = full display.
    ///
    /// # Errors
    /// Rejects an empty region, or one that falls outside the target monitor's bounds, so a
    /// bad rect fails loudly instead of being silently clamped to a 1px sliver at capture time.
    pub fn set_region(&self, region: Option<CropRegion>) -> Result<(), String> {
        if let Some(r) = region {
            // Smallest region worth recording (matches the selector's own minimum). Below this
            // the crop would be a useless sliver.
            const MIN_PX: u32 = 8;
            if r.w < MIN_PX || r.h < MIN_PX {
                return Err("capture region is empty".into());
            }
            // When the target monitor's size is known, the region must land on it. We reject by
            // how much actually falls INSIDE the monitor, not by a strict edge test: an origin
            // outside (or barely inside) the monitor is what `clamp_region` collapses to a 1px
            // sliver, whereas a rect that merely overhangs the far edge by a pixel (rounding at
            // the selector) still clamps to a sensible crop and is fine. The monitor may be
            // unset (e.g. a full-primary target that never went through the selector) — then we
            // can only reject a degenerate rect, not an out-of-bounds one.
            if let Some(m) = self
                .pending_monitor
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .as_ref()
            {
                if m.w > 0 && m.h > 0 {
                    let visible_w = m.w.saturating_sub(r.x).min(r.w);
                    let visible_h = m.h.saturating_sub(r.y).min(r.h);
                    if visible_w < MIN_PX || visible_h < MIN_PX {
                        return Err(format!(
                            "capture region {}×{} at ({}, {}) is outside the {}×{} monitor",
                            r.w, r.h, r.x, r.y, m.w, m.h
                        ));
                    }
                }
            }
        }
        *self
            .pending_region
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = region;
        Ok(())
    }

    /// Set the monitor the next recording (and its selector screenshot) captures.
    pub fn set_monitor(&self, monitor: Option<MonitorInfo>) -> Result<(), String> {
        *self
            .pending_monitor
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = monitor;
        Ok(())
    }

    /// Set the zoom multiplier for the next recording (clamped to a sane range).
    pub fn set_zoom_amount(&self, amount: f64) -> Result<(), String> {
        *self.pending_zoom.lock().unwrap_or_else(|e| e.into_inner()) = amount.clamp(1.0, 4.0);
        Ok(())
    }

    /// Grab a single full-display frame and return it as a `data:image/png;base64,…` URL —
    /// the still backdrop the region selector draws on (no transparent window needed).
    pub fn screenshot(&self) -> Result<String, String> {
        let monitor = self
            .pending_monitor
            .lock()
            .unwrap_or_else(|e| e.into_inner())
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

    /// The per-session auth token the webview must present in the preview WS URL. Without it
    /// the server refuses the upgrade, so another local process cannot read the preview.
    #[must_use]
    pub fn preview_token(&self) -> String {
        self.preview.token().to_string()
    }

    /// Whether the GPU compositor initialized. When `false`, seek/preview/export cannot
    /// work — but recording still does: capture, the live monitor, input hooks and the
    /// disk-backed frame store never touch the compositor. The frontend queries this once
    /// at boot to warn up front instead of failing every operation with a cryptic string.
    #[must_use]
    pub fn has_gpu(&self) -> bool {
        self.compositor.is_some()
    }

    /// Begin capturing the primary display + global input.
    pub fn start_recording(&self) -> Result<(), String> {
        let mut active = self.active.lock().unwrap_or_else(|e| e.into_inner());
        if active.is_some() {
            return Err("already recording".into());
        }
        let region = *self
            .pending_region
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let monitor = self
            .pending_monitor
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        let mon_name = monitor.as_ref().map(|m| m.name.clone());
        let mon_origin = monitor.as_ref().map_or((0, 0), |m| (m.x, m.y));
        let amount = *self.pending_zoom.lock().unwrap_or_else(|e| e.into_inner());

        // Guard the recording volume BEFORE creating anything: raw uncompressed BGRA streams to
        // disk at ~250 MB/s (1080p) to ~1 GB/s (4K), so without a check a take can silently fill
        // a system disk in minutes. Estimate the write size from the capture dimensions (region,
        // else monitor, else a 1080p fallback) and block if free space is under the hard floor;
        // warn (surfaced at stop) if it's above the floor but only a few minutes' worth.
        let (est_w, est_h) = region
            .map(|r| (r.w, r.h))
            .or_else(|| monitor.as_ref().map(|m| (m.w, m.h)))
            .unwrap_or((1920, 1080));
        let _ = std::fs::create_dir_all(frame_store::recovery_root());
        let space_warning = match frame_store::free_space_bytes(&frame_store::recovery_root()) {
            // A probe failure means "unknown" — don't block the user on it.
            None => None,
            Some(free) => check_free_space(free, est_w, est_h).map_err(|e| {
                tracing::error!("refusing to start recording: {e}");
                e
            })?,
        };

        // Rotate: this take streams into its own fresh recovery subdir, so the previous
        // session's store survives until a later recording ages it out. `new_session_dir`
        // prunes older sessions first, keeping disk use bounded.
        let recovery_dir = frame_store::new_session_dir();
        // Drop the previous clip BEFORE creating the new store: its open handles may point at
        // a store we might otherwise touch.
        *self.edited.lock().unwrap_or_else(|e| e.into_inner()) = Edited::default();
        let writer = FrameWriter::create(recovery_dir.clone()).map_err(|e| {
            tracing::error!("could not open frame store for recording: {e}");
            e
        })?;
        *self
            .current_recovery
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = Some(recovery_dir.clone());

        // Persist a minimal manifest up front so a hard crash mid-recording leaves a
        // *detectable* session — recovery keys off `project.json` existing. The real
        // dimensions/fps/duration are rebuilt from the incrementally-written frame index at
        // recover time (see `recover_session`); this placeholder just has to parse. Without
        // it, a crash before stop would strand the on-disk frames with nothing to open them.
        let (rw, rh) = region.map_or((0, 0), |r| (r.w, r.h));
        if let Ok(json) = Project::new(SourceInfo {
            path: String::new(),
            width: rw,
            height: rh,
            fps: 0.0,
            duration: 0.0,
        })
        .to_json()
        {
            let _ = std::fs::write(frame_store::project_path(&recovery_dir), json);
        }

        // Latch the recording epoch BEFORE spawning capture/input, so no frame or event that
        // arrives during startup can be stamped earlier than the epoch (a negative time would
        // otherwise slip into normalization / zoom planning).
        let start_qpc = self.clock.now();
        let (frames_rx, capture) = spawn_region(region, mon_name.clone());
        let (recorder, events_rx) = InputRecorder::start();
        // Independent live preview — its own capture, so it can never disturb the recording.
        let preview = LivePreview::start(region, mon_name, mon_origin, amount, self.preview.sink());

        // Stream frames straight to disk so recording length is bounded by disk, not RAM.
        let drain_stop = Arc::new(AtomicBool::new(false));
        let stop_flag = Arc::clone(&drain_stop);
        let probe_dir = recovery_dir.clone();
        let drain = std::thread::spawn(move || -> DrainOutcome {
            let mut writer = writer;
            // If a disk write fails mid-recording, stop writing but keep draining the
            // channel (so capture never blocks and RAM stays bounded) and remember why —
            // at stop we finalize with the frames already on disk instead of erroring the
            // whole take out.
            let mut write_err: Option<String> = None;
            // Set if the capture channel disconnects before a stop was requested — i.e. the
            // capture ended on its own (monitor unplugged, WGC session died). We keep the
            // frames already on disk and surface a warning instead of ending silently.
            let mut ended_early = false;
            // Proactive low-disk guard: re-probe free space every `DISK_CHECK_EVERY` frames
            // (cheap — a few times a second at most, never per frame) and stop writing before
            // the volume actually fills. Reuses the salvage + warning path, so we keep the
            // frames already on disk instead of hard-filling the user's disk.
            let mut since_disk_check: u32 = 0;
            loop {
                match frames_rx.recv_timeout(Duration::from_millis(200)) {
                    Ok(f) => {
                        if write_err.is_none() {
                            since_disk_check += 1;
                            if since_disk_check >= DISK_CHECK_EVERY {
                                since_disk_check = 0;
                                if let Some(free) = frame_store::free_space_bytes(&probe_dir) {
                                    if free < DRAIN_STOP_FLOOR_BYTES {
                                        write_err = Some(format!(
                                            "low on space, ~{} MB left",
                                            free / (1024 * 1024)
                                        ));
                                    }
                                }
                            }
                        }
                        if write_err.is_none() {
                            if let Err(e) = writer.push(f) {
                                write_err = Some(e);
                            }
                        }
                    }
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                        if stop_flag.load(Ordering::Relaxed) {
                            break;
                        }
                    }
                    Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                        ended_early = !stop_flag.load(Ordering::Relaxed);
                        break;
                    }
                }
            }
            let store = if write_err.is_some() {
                writer.finish_salvage()?
            } else {
                writer.finish()?
            };
            Ok((store, write_err, ended_early))
        });

        *active = Some(Active {
            drain: Some(drain),
            drain_stop,
            capture,
            region,
            mon_origin,
            recorder,
            events_rx,
            start_qpc,
            zoom_poll: ZoomChordPoller::start(),
            recovery_dir,
            pauses: Vec::new(),
            space_warning,
            _preview: preview,
        });
        Ok(())
    }

    /// Pause / resume the running recording. Capture keeps running; the paused span is
    /// turned into a cut at stop time, so it never appears in the output (and can be
    /// restored in the editor if the pause was a mistake).
    pub fn set_record_paused(&self, paused: bool) -> Result<(), String> {
        let mut active = self.active.lock().unwrap_or_else(|e| e.into_inner());
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
        let mut active = self.active.lock().unwrap_or_else(|e| e.into_inner());
        let Some(mut session) = active.take() else {
            return Err("not recording".into());
        };
        // Mark the intended stop BEFORE tearing down capture: stopping the capture drops the
        // channel sender, which the drain sees as a disconnect. With the flag already set, that
        // self-inflicted disconnect isn't misread as the capture ending on its own.
        session.drain_stop.store(true, Ordering::Relaxed);
        session._preview.stop(); // tear down the live monitor before post-processing
        session.capture.stop();
        session.recorder.stop();

        // Let the drain thread flush remaining frames and hand back the disk store. A disk
        // write that failed mid-recording comes back as a warning (not an error): the frames
        // written before the failure are kept and the take truncates gracefully. The trailing
        // flag is set when capture disconnected before we asked it to (monitor unplugged etc.).
        let (store, write_warning, capture_ended_early) = session
            .drain
            .take()
            .ok_or("recording already stopped")?
            .join()
            .map_err(|_| {
                tracing::error!("frame drain thread panicked");
                "frame drain thread panicked"
            })??;
        // Capture has wound down by now (its sender is dropped and the drain has joined), so the
        // shared drop counter is settled. A non-zero count means the bounded channel overflowed —
        // frames the drain couldn't keep up with — which we may surface as a warning below.
        let dropped = session.capture.dropped();
        let raw_events: Vec<RawEvent> = session.events_rx.try_iter().collect();

        // No frames means the screen capture never started (or was stopped instantly) — fail
        // loudly so the editor shows a clear message instead of a silent, empty player.
        if store.is_empty() {
            tracing::error!("recording stopped with no frames — screen capture never produced any");
            return Err("No frames were captured — screen capture failed to start.".into());
        }

        let (width, height) = store.recs().first().map_or((1920, 1080), |r| (r.w, r.h));
        let duration = store.recs().last().map_or(0.0, |r| {
            self.clock.seconds_between(session.start_qpc, r.qpc)
        });

        // Map the cursor into the captured area. Cursor events are in virtual-desktop
        // physical coords; the crop is monitor-relative, so offset by the monitor origin.
        let region = compose_capture_region(session.mon_origin, session.region, width, height);
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
        let poll_marks = session.zoom_poll.finish();
        events.extend(merge_poll_chord_marks(
            &poll_marks,
            &hook_mark_times,
            &region,
            self.clock,
            session.start_qpc,
            duration,
        ));
        events.sort_by(|a, b| a.t().total_cmp(&b.t()));

        let amount = *self.pending_zoom.lock().unwrap_or_else(|e| e.into_inner());
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
        project.cuts = pauses_to_cuts(&session.pauses, self.clock, session.start_qpc, duration);

        // Persist the manifest next to the on-disk frames (in this take's own recovery
        // subdir): together they make the recording recoverable if the app crashes or is
        // closed before exporting.
        if let Ok(json) = project.to_json() {
            let _ = std::fs::write(frame_store::project_path(&session.recovery_dir), json);
        }

        let mut edited = self.edited.lock().unwrap_or_else(|e| e.into_inner());
        // Fresh clip → fresh (empty) undo history.
        *edited = Edited {
            frames: Some(Arc::new(store)),
            project: Some(project),
            track: Some(track),
            start_qpc: session.start_qpc,
            ..Edited::default()
        };

        // Any drop is worth a log line (helps diagnose choppy takes after the fact); only a
        // material shortfall becomes a user-facing warning below.
        if dropped > 0 {
            tracing::warn!(
                "capture dropped {dropped} frame(s) during recording; kept {frame_count}"
            );
        }

        // Surface the first thing that went wrong, if anything: a mid-recording disk stop
        // (an actual write failure, or the drain's proactive low-space cutoff) is the most
        // specific cause; then a capture that ended on its own; then a take that came out
        // choppy because capture outran the drain; finally, if the take completed cleanly, the
        // heads-up that it *started* on a low-space disk.
        let warning = if let Some(e) = write_warning {
            tracing::warn!("recording truncated mid-capture to protect the disk ({e}); kept {frame_count} frames");
            Some(format!(
                "Recording was cut short to protect your disk — {e}. Kept the {frame_count} frames captured before that."
            ))
        } else if capture_ended_early {
            tracing::warn!("capture ended before stop was requested (monitor disconnected?); kept {frame_count} frames");
            Some(
                "Capture ended unexpectedly (monitor disconnected?). Kept the frames recorded up to that point."
                    .to_string(),
            )
        } else if let Some(w) = dropped_frames_warning(dropped, frame_count) {
            Some(w)
        } else {
            session.space_warning.take()
        };

        Ok(RecordingSummary {
            duration,
            frames: frame_count,
            zooms: zoom_count,
            warning,
        })
    }

    /// Composite the frame at time `t` (seconds) and publish it to the preview.
    pub fn seek(&self, t: f64) -> Result<(), String> {
        // Snapshot the consistent (project, track, frames, epoch) tuple under a short lock, then
        // release it before the disk read + GPU composite + readback (~50-150ms at 4K) so a
        // scrub never serializes with edits. Cloning the four together preserves a coherent
        // snapshot the same way the export paths do.
        let (project, track, store, start_qpc) = {
            let edited = self.edited.lock().unwrap_or_else(|e| e.into_inner());
            (
                edited.project.as_ref().ok_or("no recording")?.clone(),
                edited.track.as_ref().ok_or("no recording")?.clone(),
                Arc::clone(edited.frames.as_ref().ok_or("no frames")?),
                edited.start_qpc,
            )
        };
        let compositor = self.compositor.as_ref().ok_or("no GPU compositor")?;
        let idx = nearest_idx(store.recs(), self.clock, start_qpc, t).ok_or("no frames")?;
        let frame = store.frame(idx)?;

        let (out_w, out_h) = project.output_dims();
        let mut scene = build_scene(&project, &track, out_w, out_h, t);
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
            background_fill(&project.frame),
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

    /// Request that any in-flight GIF/MP4 export abort at the next frame boundary. Idempotent
    /// and safe to call when no export is running — the flag is reset at the start of each
    /// export. The aborting loop deletes its partial output file (see `export_gif_impl` /
    /// `export_mp4_impl`).
    pub fn cancel_export(&self) {
        self.export_cancel.store(true, Ordering::SeqCst);
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
        // Log every failure exit once at this seam (missing compositor/frames, encode error,
        // disk-full mid-write) — the frontend only sees the string, so without this the cause
        // never reaches the log.
        self.export_gif_impl(out_path, fps, width, quality, progress)
            .map_err(|e| {
                tracing::error!("GIF export failed: {e}");
                e
            })
    }

    fn export_gif_impl(
        &self,
        out_path: String,
        fps: u32,
        width: Option<u32>,
        quality: u8,
        progress: &dyn Fn(u32, u32),
    ) -> Result<(), String> {
        // Clear any stale cancel request from a prior export before we begin (see
        // `cancel_export`): the flag is process-global and one export runs at a time.
        self.export_cancel.store(false, Ordering::SeqCst);
        // Snapshot the minimal state under a short lock, then release it so the (minutes-long)
        // encode never blocks scrubbing/editing/starting a new recording. The frame store is
        // shared via `Arc` and reads from disk on demand — frames are never all resident, so
        // even an hour-long 1080p export stays within a bounded memory budget.
        let (project, track, store, start_qpc) = {
            let edited = self.edited.lock().unwrap_or_else(|e| e.into_inner());
            (
                edited.project.as_ref().ok_or("no recording")?.clone(),
                edited.track.as_ref().ok_or("no recording")?.clone(),
                Arc::clone(edited.frames.as_ref().ok_or("no frames")?),
                edited.start_qpc,
            )
        };
        if store.is_empty() {
            return Err("no frames".into());
        }
        let compositor = self.compositor.as_ref().ok_or("no GPU compositor")?;

        let (out_w, out_h) = project.output_dims();
        let bg = background_fill(&project.frame);
        let (t0, span, regions, cuts) = out_mapping(&project);
        let d_out = output_duration(span, &regions, &cuts);
        let count = ((d_out * f64::from(fps)).ceil() as usize).max(1);

        let settings = GifSettings {
            fps,
            width,
            quality,
            ..GifSettings::readme()
        };

        // Composite one output frame on demand — the streaming encoder pulls these and keeps
        // only the current + previous frame resident. Checked once per frame so a cancel
        // request (Cancel button / window-close) aborts within one composite instead of
        // running the full multi-minute encode to completion.
        let compose = |i: usize| -> Result<RgbaImage, String> {
            if self.export_cancel.load(Ordering::SeqCst) {
                return Err("export cancelled".into());
            }
            let t_out = (i as f64 / f64::from(fps)).min(d_out);
            let t_src = t0 + output_to_source(t_out, span, &regions, &cuts);
            let idx = nearest_idx(store.recs(), self.clock, start_qpc, t_src).ok_or("no frames")?;
            let frame = store.frame(idx)?;
            let scene = build_scene(&project, &track, out_w, out_h, t_src);
            let rgba = compositor.composite_scene(
                &frame.bgra,
                frame.width,
                frame.height,
                out_w,
                out_h,
                &scene,
                bg,
            );
            Ok(RgbaImage::new(out_w, out_h, rgba))
        };
        let result = export_gif_native_streaming(
            count,
            compose,
            &settings,
            Path::new(&out_path),
            &|done, total| {
                progress(done as u32, total as u32);
            },
        )
        .map_err(|e| e.to_string());
        // On cancellation or any encode error, best-effort delete the partial .gif so the user
        // is never left with a truncated file at their chosen path.
        if result.is_err() {
            let _ = std::fs::remove_file(&out_path);
        }
        // A cancel bails out through the compose closure, so the streaming encoder surfaces it
        // wrapped as "gif encoding failed: export cancelled". Normalize it back to the bare
        // sentinel the frontend matches on.
        if self.export_cancel.load(Ordering::SeqCst) {
            return Err("export cancelled".into());
        }
        result
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
        // Log every failure exit once at this seam (see `export_gif`).
        self.export_mp4_impl(out_path, fps, width, quality, progress)
            .map_err(|e| {
                tracing::error!("MP4 export failed: {e}");
                e
            })
    }

    fn export_mp4_impl(
        &self,
        out_path: String,
        fps: u32,
        width: Option<u32>,
        quality: u8,
        progress: &dyn Fn(u32, u32),
    ) -> Result<(), String> {
        // Clear any stale cancel request before we begin (see `cancel_export` / `export_gif`).
        self.export_cancel.store(false, Ordering::SeqCst);
        // Snapshot under a short lock, then release it so the long encode doesn't freeze
        // scrub/edit/record (see `export_gif`). MP4 already streams frame-by-frame.
        let (project, track, store, start_qpc) = {
            let edited = self.edited.lock().unwrap_or_else(|e| e.into_inner());
            (
                edited.project.as_ref().ok_or("no recording")?.clone(),
                edited.track.as_ref().ok_or("no recording")?.clone(),
                Arc::clone(edited.frames.as_ref().ok_or("no frames")?),
                edited.start_qpc,
            )
        };
        if store.is_empty() {
            return Err("no frames".into());
        }
        let compositor = self.compositor.as_ref().ok_or("no GPU compositor")?;

        let (out_w, out_h) = project.output_dims();
        let bg = background_fill(&project.frame);
        let (t0, span, regions, cuts) = out_mapping(&project);
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
            // Poll once per frame so a cancel request (Cancel button / window-close) aborts
            // mid-encode; the shared cleanup below then deletes the half-written .mp4.
            if self.export_cancel.load(Ordering::SeqCst) {
                frame_err = Some("export cancelled".into());
                break;
            }
            let t_out = (i as f64 / f64::from(fps)).min(d_out);
            let t_src = t0 + output_to_source(t_out, span, &regions, &cuts);
            let idx = match nearest_idx(store.recs(), self.clock, start_qpc, t_src) {
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
            let scene = build_scene(&project, &track, out_w, out_h, t_src);
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

        // Detach the state the sampling needs under a short lock, then drop the real lock so the
        // GPU compositing + throwaway GIF encodes below run unlocked (they only read, never
        // mutate `edited`). `undo`/`redo` stay empty — the helpers read only project/track/
        // frames/start_qpc. The Arc-shared frame store reads from disk on demand.
        let edited = {
            let guard = self.edited.lock().unwrap_or_else(|e| e.into_inner());
            Edited {
                frames: guard.frames.as_ref().map(Arc::clone),
                project: guard.project.clone(),
                track: guard.track.clone(),
                start_qpc: guard.start_qpc,
                ..Edited::default()
            }
        };
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
        let bg = background_fill(&project.frame);
        let (t0, span, regions, cuts) = out_mapping(project);
        let d_out = output_duration(span, &regions, &cuts);

        let mut images = Vec::with_capacity(indices.len());
        for (done, &i) in indices.iter().enumerate() {
            let t_out = (i as f64 / f64::from(fps)).min(d_out);
            let t_src = t0 + output_to_source(t_out, span, &regions, &cuts);
            let idx = nearest_idx(store.recs(), self.clock, edited.start_qpc, t_src)
                .ok_or("no frames")?;
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
        let mut edited = self.edited.lock().unwrap_or_else(|e| e.into_inner());
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
        let mut edited = self.edited.lock().unwrap_or_else(|e| e.into_inner());
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
        let mut edited = self.edited.lock().unwrap_or_else(|e| e.into_inner());
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
        let mut edited = self.edited.lock().unwrap_or_else(|e| e.into_inner());
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
        let edited = self.edited.lock().unwrap_or_else(|e| e.into_inner());
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
            background_preset: project
                .frame
                .background
                .preset_name()
                .unwrap_or_default()
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
            // The frame preset owns padding/corners/shadow; the backdrop is chosen separately
            // (`set_background_preset`). Preserve whatever backdrop the user picked across frame
            // switches — but when they first enable a frame on a still-default black backdrop,
            // seed a tasteful graphite gradient so the padded area doesn't read as a black void.
            let keep_bg = p.frame.background.clone();
            let default_bg =
                Background::preset("graphite").unwrap_or(Background::Solid(Color::BLACK));
            let bg_for_framed = if keep_bg == Background::Solid(Color::BLACK) {
                default_bg
            } else {
                keep_bg.clone()
            };
            p.frame = match preset {
                "subtle" => FrameStyle {
                    background: bg_for_framed,
                    padding: 0.04,
                    corner_radius: 0.012,
                    shadow: Shadow {
                        strength: 0.3,
                        ..Shadow::default()
                    },
                },
                "studio" => FrameStyle {
                    background: bg_for_framed,
                    padding: 0.075,
                    corner_radius: 0.02,
                    shadow: Shadow {
                        strength: 0.5,
                        ..Shadow::default()
                    },
                },
                // No frame: edge-to-edge. Keep the backdrop field for round-tripping, but it's
                // never visible (zero padding = the recording fills the whole output).
                _ => FrameStyle {
                    background: keep_bg,
                    ..FrameStyle::default()
                },
            };
            Ok(())
        })
    }

    /// Set the backdrop behind/around a framed recording to a named preset (see
    /// [`Background::preset`] — gradients like `graphite`/`slate`/`teal`, plus `solid`). The
    /// compositor renders it into both the preview and the export; one undo step per pick.
    pub fn set_background_preset(&self, name: &str) -> Result<(), String> {
        let bg = Background::preset(name).ok_or("unknown background preset")?;
        self.with_project("", |p| {
            p.frame.background = bg;
            Ok(())
        })
    }

    /// Set the trim range (seconds). A range covering the whole clip clears the trim.
    pub fn set_trim(&self, start: f64, end: f64) -> Result<(), String> {
        // Handle drags stream this per pointer event — coalesce a run into one undo step.
        self.with_project("trim", |p| {
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

        let mut edited = self.edited.lock().unwrap_or_else(|e| e.into_inner());
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
        let mut edited = self.edited.lock().unwrap_or_else(|e| e.into_inner());
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
        let mut edited = self.edited.lock().unwrap_or_else(|e| e.into_inner());
        // Drags/scrubs can stream this per pointer event — coalesce a run into one undo step.
        snapshot(&mut edited, &format!("speed:{index}"));
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
        let mut edited = self.edited.lock().unwrap_or_else(|e| e.into_inner());
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
        let mut edited = self.edited.lock().unwrap_or_else(|e| e.into_inner());
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
        let mut edited = self.edited.lock().unwrap_or_else(|e| e.into_inner());
        // Drags/scrubs can stream this per pointer event — coalesce a run into one undo step.
        snapshot(&mut edited, &format!("cut:{index}"));
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
        let mut edited = self.edited.lock().unwrap_or_else(|e| e.into_inner());
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
        let mut edited = self.edited.lock().unwrap_or_else(|e| e.into_inner());
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
            style: ZoomStyle::default(),
        };
        if vuoom_zoom::insert_sorted(&mut project.zooms, kf).is_none() {
            return Err("no room for a zoom segment here".into());
        }
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
        let mut edited = self.edited.lock().unwrap_or_else(|e| e.into_inner());
        // Drags/scrubs can stream this per pointer event — coalesce a run into one undo step.
        snapshot(&mut edited, &format!("zoom:{index}"));
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
        let mut edited = self.edited.lock().unwrap_or_else(|e| e.into_inner());
        // Focus dragging streams per pointer event — coalesce a run into one undo step.
        snapshot(&mut edited, &format!("focus:{index}"));
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

    /// Set the easing "feel" preset of the zoom segment at `index` and re-simulate.
    /// Returns the updated segment list. Preset picks are discrete (one undo step each).
    pub fn set_zoom_style(
        &self,
        index: usize,
        style: ZoomStyle,
    ) -> Result<Vec<ZoomKeyframe>, String> {
        let mut edited = self.edited.lock().unwrap_or_else(|e| e.into_inner());
        // A preset click is a single deliberate change — one undo step, never coalesced.
        snapshot(&mut edited, "");
        let project = edited.project.as_mut().ok_or("no recording")?;
        let kf = project.zooms.get_mut(index).ok_or("no such zoom segment")?;
        kf.style = style;
        let zooms = project.zooms.clone();
        resimulate(&mut edited);
        Ok(zooms)
    }

    /// Delete the zoom segment at `index` and re-simulate the camera.
    /// Returns the updated segment list.
    pub fn delete_zoom(&self, index: usize) -> Result<Vec<ZoomKeyframe>, String> {
        let mut edited = self.edited.lock().unwrap_or_else(|e| e.into_inner());
        snapshot(&mut edited, "");
        let project = edited.project.as_mut().ok_or("no recording")?;
        vuoom_zoom::remove(&mut project.zooms, index).ok_or("no such zoom segment")?;
        let zooms = project.zooms.clone();
        resimulate(&mut edited);
        Ok(zooms)
    }

    /// Snapshot every annotation for the editor overlay.
    pub fn annotations(&self) -> Result<AnnotationSet, String> {
        let edited = self.edited.lock().unwrap_or_else(|e| e.into_inner());
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
        // Typing, the size slider, and position drags all fire per keystroke / per pointer
        // event — coalesce each run into one undo step. Style toggles stay discrete.
        let tag = if text.is_some() {
            format!("text:{id}")
        } else if font_size.is_some() {
            format!("font:{id}")
        } else if x.is_some() || y.is_some() {
            format!("geo:text:{id}")
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
        // Geometry edits can stream during a drag — coalesce a run into one undo step.
        self.with_project(&format!("geo:arrow:{id}"), |p| {
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
        // Geometry edits can stream during a drag — coalesce a run into one undo step.
        self.with_project(&format!("geo:box:{id}"), |p| {
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
        // Timeline band drags stream this per pointer event — coalesce into one undo step.
        self.with_project(&format!("range:{id}"), |p| {
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
        let mut edited = self.edited.lock().unwrap_or_else(|e| e.into_inner());
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

    /// Paste a set of copied annotation snapshots at time `at`. The set is re-anchored so the
    /// EARLIEST copied annotation starts at `at`, preserving every item's relative time offset
    /// and its duration (each item's end is clamped to the clip end, so a paste near the tail
    /// only shortens what overflows). Geometry and style are kept verbatim. Fresh ids are
    /// assigned; the whole paste is one snapshot → one undo step. Returns the new items
    /// (kind + id) in paste order so the UI can select them.
    pub fn paste_annotations(
        &self,
        items: Vec<PasteItem>,
        at: f64,
    ) -> Result<Vec<PastedRef>, String> {
        if items.is_empty() {
            return Ok(Vec::new());
        }
        let mut edited = self.edited.lock().unwrap_or_else(|e| e.into_inner());
        snapshot(&mut edited, "");
        let project = edited.project.as_mut().ok_or("no recording")?;
        let d = project.source.duration;
        // The earliest copied start anchors to the playhead; every other item keeps its offset
        // relative to that earliest, so the set's internal timing is preserved.
        let earliest = items
            .iter()
            .map(|it| match it {
                PasteItem::Text(t) => t.range.start,
                PasteItem::Arrow(a) => a.range.start,
                PasteItem::Box(b) => b.range.start,
            })
            .fold(f64::INFINITY, f64::min);
        // Re-anchor a range onto [0, d]: preserve duration, clamp the end to the clip, and keep
        // fades sane if the window ended up shorter than them.
        let reanchor = |range: &mut TimeRange| {
            let dur = (range.end - range.start).max(0.1);
            let s = (at + (range.start - earliest)).clamp(0.0, (d - 0.1).max(0.0));
            let e = (s + dur).min(d.max(s + 0.1));
            range.start = s;
            range.end = e;
            let span = e - s;
            range.fade_in = range.fade_in.min(span / 2.0);
            range.fade_out = range.fade_out.min(span / 2.0);
        };
        let mut refs = Vec::with_capacity(items.len());
        for item in items {
            let id = next_id(project);
            match item {
                PasteItem::Text(mut t) => {
                    t.id = id;
                    reanchor(&mut t.range);
                    project.texts.push(t);
                    refs.push(PastedRef {
                        kind: "text".into(),
                        id,
                    });
                }
                PasteItem::Arrow(mut a) => {
                    a.id = id;
                    reanchor(&mut a.range);
                    project.arrows.push(a);
                    refs.push(PastedRef {
                        kind: "arrow".into(),
                        id,
                    });
                }
                PasteItem::Box(mut b) => {
                    b.id = id;
                    reanchor(&mut b.range);
                    project.highlights.push(b);
                    refs.push(PastedRef {
                        kind: "box".into(),
                        id,
                    });
                }
            }
        }
        Ok(refs)
    }

    /// Move an annotation within its own type's Vec, changing its stacking order. `dir` is one
    /// of `"forward"` / `"backward"` / `"front"` / `"back"`.
    ///
    /// Stacking is per-type in BOTH renderers (highlights below arrows below texts, fixed), so
    /// this only reorders an item relative to its own kind — the order the canvas overlay and
    /// the export compositor both honour by iterating each Vec front-to-back (later = on top).
    /// A no-op at a boundary (already front/back) succeeds without recording an undo step.
    pub fn reorder_annotation(&self, id: u32, dir: &str) -> Result<(), String> {
        // Locate the item's Vec + current index without holding the snapshot yet, so a no-op
        // move at a boundary leaves the undo history untouched.
        fn index_of<T>(v: &[T], id: u32, get: impl Fn(&T) -> u32) -> Option<usize> {
            v.iter().position(|x| get(x) == id)
        }
        // Compute the destination index for `dir` within a Vec of length `len`, or `None` when
        // the move is a no-op (already at the boundary).
        fn target(dir: &str, i: usize, len: usize) -> Result<Option<usize>, String> {
            match dir {
                "forward" => Ok((i + 1 < len).then_some(i + 1)),
                "backward" => Ok((i > 0).then_some(i - 1)),
                "front" => Ok((i + 1 < len).then_some(len - 1)),
                "back" => Ok((i > 0).then_some(0)),
                other => Err(format!("bad reorder dir: {other}")),
            }
        }
        let mut edited = self.edited.lock().unwrap_or_else(|e| e.into_inner());
        let project = edited.project.as_ref().ok_or("no recording")?;
        // Find which Vec owns the id and where the move lands. `None` target → boundary no-op.
        let plan: Option<(u8, usize, usize)> =
            if let Some(i) = index_of(&project.texts, id, |t| t.id) {
                target(dir, i, project.texts.len())?.map(|j| (0, i, j))
            } else if let Some(i) = index_of(&project.arrows, id, |a| a.id) {
                target(dir, i, project.arrows.len())?.map(|j| (1, i, j))
            } else if let Some(i) = index_of(&project.highlights, id, |h| h.id) {
                target(dir, i, project.highlights.len())?.map(|j| (2, i, j))
            } else {
                return Err("no such annotation".into());
            };
        let Some((kind, i, j)) = plan else {
            return Ok(()); // already at the boundary — nothing to do
        };
        snapshot(&mut edited, "");
        let project = edited.project.as_mut().ok_or("no recording")?;
        // Remove-then-insert moves the item to `j` and shifts everything between, preserving the
        // relative order of the rest (a plain swap would only work for adjacent moves).
        match kind {
            0 => {
                let item = project.texts.remove(i);
                project.texts.insert(j, item);
            }
            1 => {
                let item = project.arrows.remove(i);
                project.arrows.insert(j, item);
            }
            _ => {
                let item = project.highlights.remove(i);
                project.highlights.insert(j, item);
            }
        }
        Ok(())
    }

    /// Delete any annotation (text, arrow, or box) by id. `tag` is the undo-coalesce key
    /// (see [`snapshot`]): empty for a discrete single delete, a shared non-empty value to
    /// fold a whole group delete into one undo step.
    pub fn delete_annotation(&self, id: u32, tag: &str) -> Result<(), String> {
        self.with_project(tag, |p| {
            p.texts.retain(|a| a.id != id);
            p.arrows.retain(|a| a.id != id);
            p.highlights.retain(|a| a.id != id);
            Ok(())
        })
    }

    /// Save the recording as a `dir.vuoom` bundle: the project manifest plus every frame
    /// as a lossless PNG and a time index. Reopenable with [`Self::open_bundle`].
    pub fn save_bundle(&self, dir: &Path) -> Result<(), String> {
        self.save_bundle_impl(dir).map_err(|e| {
            tracing::error!(dir = %dir.display(), "saving bundle failed: {e}");
            e
        })
    }

    fn save_bundle_impl(&self, dir: &Path) -> Result<(), String> {
        // Snapshot the project + frame-store handle under a short lock, then release it so the
        // per-frame disk read + PNG encode below (minutes for a long clip) never freezes
        // scrubbing/editing/recording. An edit that lands mid-save just means the bundle captures
        // the pre-edit project — an acceptable, self-consistent snapshot of that instant.
        let (project, store, start_qpc) = {
            let edited = self.edited.lock().unwrap_or_else(|e| e.into_inner());
            (
                edited.project.as_ref().ok_or("no recording")?.clone(),
                Arc::clone(edited.frames.as_ref().ok_or("no recording")?),
                edited.start_qpc,
            )
        };
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
                t: self.clock.seconds_between(start_qpc, f.qpc),
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
        self.open_bundle_impl(dir).map_err(|e| {
            tracing::error!(dir = %dir.display(), "opening bundle failed: {e}");
            e
        })
    }

    fn open_bundle_impl(&self, dir: &Path) -> Result<RecordingSummary, String> {
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

        // Decode into the dedicated scratch store — NOT a rotated recording session — so
        // opening a bundle never buries the last recording's recoverable take. One frame in
        // memory at a time. Drop the current clip first: its store handles may point at the
        // scratch files we're about to truncate.
        let scratch = frame_store::scratch_dir();
        *self.edited.lock().unwrap_or_else(|e| e.into_inner()) = Edited::default();
        let mut writer = FrameWriter::create(scratch.clone())?;
        for fi in &index {
            let img = read_png(&frames_dir.join(format!("{:05}.png", fi.n)))
                .map_err(|e| e.to_string())?;
            writer.push(CapturedFrame {
                width: fi.w,
                height: fi.h,
                bgra: swizzle_rb(&img.pixels), // RGBA on disk -> BGRA in memory
                qpc: base + (fi.t * freq as f64) as i64,
            })?;
        }
        let store = writer.finish()?;
        if let Ok(json) = project.to_json() {
            let _ = std::fs::write(frame_store::project_path(&scratch), json);
        }
        *self
            .current_recovery
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = Some(scratch);

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
            warning: None,
        };
        let mut edited = self.edited.lock().unwrap_or_else(|e| e.into_inner());
        // Fresh clip → fresh (empty) undo history.
        *edited = Edited {
            frames: Some(Arc::new(store)),
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
        // The most recent recoverable session that isn't the one already loaded in the editor.
        let active = self
            .current_recovery
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        let dir = frame_store::latest_recoverable(active.as_deref())?;
        let json = std::fs::read_to_string(frame_store::project_path(&dir)).ok()?;
        let project = Project::from_json(&json).ok()?;
        let store = FrameStore::open(&dir).ok()?;
        if store.is_empty() {
            return None;
        }
        // A crash mid-recording leaves only the startup placeholder manifest (duration 0);
        // derive the real length from the frames that survived so the recents card is honest.
        Some(if project.source.duration > 0.0 {
            project.source.duration
        } else {
            store_duration(&store, self.clock)
        })
    }

    /// Reload the session left in the recovery directory (last recording + its edits as
    /// of stop time). Returns a summary like `stop_recording`.
    pub fn recover_session(&self) -> Result<RecordingSummary, String> {
        self.recover_session_impl().map_err(|e| {
            tracing::error!("recovering session failed: {e}");
            e
        })
    }

    fn recover_session_impl(&self) -> Result<RecordingSummary, String> {
        let active = self
            .current_recovery
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        let dir =
            frame_store::latest_recoverable(active.as_deref()).ok_or("no recoverable session")?;
        let mut project = Project::from_json(
            &std::fs::read_to_string(frame_store::project_path(&dir))
                .map_err(|e| format!("no recoverable session: {e}"))?,
        )
        .map_err(|e| e.to_string())?;
        let store = FrameStore::open(&dir)?;
        let first = *store.recs().first().ok_or("no recoverable session")?;

        // The stored QPC stamps all come from the crashed session, so they stay mutually
        // consistent; anchoring the epoch on the first frame reproduces the timeline
        // (within the first frame's capture latency).
        let start_qpc = first.qpc;

        // A take recovered from a hard crash carries only the startup placeholder manifest —
        // no real dimensions/fps/duration (and no post-processed events/zooms). Rebuild the
        // source metadata from the frames that survived so the clip is actually openable. A
        // cleanly stopped session already has these filled in, so leave those untouched.
        if project.source.duration <= 0.0 || project.source.width == 0 {
            let duration = store_duration(&store, self.clock);
            project.source.width = first.w;
            project.source.height = first.h;
            project.source.duration = duration;
            project.source.fps = if duration > 0.0 {
                store.len() as f64 / duration
            } else {
                60.0
            };
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
            warning: None,
        };
        // The recovered take is now the loaded clip, so later recovery checks skip it and
        // surface the *previous* session instead.
        *self
            .current_recovery
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = Some(dir);
        let mut edited = self.edited.lock().unwrap_or_else(|e| e.into_inner());
        *edited = Edited {
            frames: Some(Arc::new(store)),
            project: Some(project),
            track: Some(track),
            start_qpc,
            ..Edited::default()
        };
        Ok(summary)
    }

    /// Bytes held under the recovery root and how many recording sessions those bytes back —
    /// for the storage readout. Cheap (walks the two-or-three recovery dirs).
    pub fn recovery_storage(&self) -> (u64, usize) {
        frame_store::recovery_usage()
    }

    /// Delete all stored recovery data except the store backing the currently-loaded clip.
    /// Returns the bytes freed. Rejected while a recording is running — the active take streams
    /// into its recovery dir, so deleting anything mid-record risks corrupting it.
    pub fn clear_recovery_storage(&self) -> Result<u64, String> {
        if self
            .active
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .is_some()
        {
            return Err("Can't clear storage while recording.".into());
        }
        // Keep the current clip's store (the opened bundle's scratch dir or a recovered take),
        // so clearing never pulls the frames out from under the editor.
        let keep = self
            .current_recovery
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        Ok(frame_store::clear_recovery(keep.as_deref()))
    }

    /// Run `f` against the editable project, recording an undo snapshot first.
    /// `tag` controls undo coalescing — see [`snapshot`].
    fn with_project<F>(&self, tag: &str, f: F) -> Result<(), String>
    where
        F: FnOnce(&mut Project) -> Result<(), String>,
    {
        let mut edited = self.edited.lock().unwrap_or_else(|e| e.into_inner());
        snapshot(&mut edited, tag);
        let project = edited.project.as_mut().ok_or("no recording")?;
        f(project)
    }

    /// Revert the most recent edit. Returns `false` if there is nothing to undo.
    pub fn undo(&self) -> Result<bool, String> {
        let mut edited = self.edited.lock().unwrap_or_else(|e| e.into_inner());
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
        let mut edited = self.edited.lock().unwrap_or_else(|e| e.into_inner());
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

/// Recording length (s) implied by the stored frames' QPC span. Rebuilds the source
/// duration when recovering a crashed take whose manifest is only the startup placeholder.
fn store_duration(store: &FrameStore, clock: Clock) -> f64 {
    match (store.recs().first(), store.recs().last()) {
        (Some(a), Some(b)) => clock.seconds_between(a.qpc, b.qpc),
        _ => 0.0,
    }
}

/// Index of the stored frame whose timestamp is closest to `t` (metadata only — no disk).
/// Index of the stored frame whose capture time is nearest `t` seconds from `start_qpc`.
///
/// Frames are stored in capture order — ascending QPC — and time is linear in QPC, so this
/// binary-searches instead of scanning every frame. That matters during export, which calls
/// it once per output frame: the old linear scan made export O(frames × output_frames).
fn nearest_idx(recs: &[FrameRec], clock: Clock, start_qpc: i64, t: f64) -> Option<usize> {
    if recs.is_empty() {
        return None;
    }
    let target = start_qpc as f64 + t * clock.freq() as f64;
    let hi = recs.partition_point(|r| (r.qpc as f64) < target);
    if hi == 0 {
        return Some(0);
    }
    if hi >= recs.len() {
        return Some(recs.len() - 1);
    }
    let prev = hi - 1;
    let d_prev = (recs[prev].qpc as f64 - target).abs();
    let d_hi = (recs[hi].qpc as f64 - target).abs();
    Some(if d_hi < d_prev { hi } else { prev })
}

/// Vuoom's own control chords — these drive the app, not the demo, so they must never render
/// as keystroke-overlay chips. Kept here next to `extract_key_taps` (the layer that builds
/// chips) and cross-referenced to their definitions so a future chord change updates both:
///   - `Ctrl+Shift+X` — the stop-recording hotkey (`hotkey.rs`).
///   - `Ctrl+Shift+Z` — the manual zoom chord (`zoom_chord.rs` / `normalize.rs`).
///
/// Matched on `Ctrl && Shift && key` (ignoring Alt/Win), mirroring the actual triggers, which
/// key off exactly those modifiers. Suppressing the whole chord leaves no stray `Ctrl+Shift`
/// chip because bare modifiers never emit a tap on their own (they only set flags below).
const VK_X: u16 = 0x58;
const VK_Z: u16 = 0x5A;
fn is_app_control_chord(ctrl: bool, shift: bool, vk: u16) -> bool {
    ctrl && shift && (vk == VK_X || vk == VK_Z)
}

/// Turn the raw key log into overlay-worthy taps: modifier chords (`Ctrl+Shift+P`) and
/// standalone special keys (Enter, Esc, F-keys, arrows). Auto-repeat is coalesced. Vuoom's
/// own control chords (see `is_app_control_chord`) are dropped — they're app control, not
/// demo content.
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
        if is_app_control_chord(ctrl, shift, vk) {
            continue; // Vuoom's own stop / zoom chord — not demo content
        }
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

/// Place the capture region on the virtual desktop: offset the monitor-relative crop by the
/// captured monitor's origin, so cursor events (in virtual-desktop physical px, negative on
/// monitors left/above the primary) map into the same coordinate space. `None` crop = the
/// full monitor (`full_w`×`full_h`) at its origin.
fn compose_capture_region(
    mon_origin: (i32, i32),
    region: Option<CropRegion>,
    full_w: u32,
    full_h: u32,
) -> CaptureRegion {
    let (mx, my) = mon_origin;
    match region {
        Some(r) => CaptureRegion {
            x: mx + r.x as i32,
            y: my + r.y as i32,
            w: r.w as i32,
            h: r.h as i32,
        },
        None => CaptureRegion {
            x: mx,
            y: my,
            w: full_w as i32,
            h: full_h as i32,
        },
    }
}

/// Turn poll-detected Ctrl+Shift+Z presses into normalized zoom marks, merging them with the
/// hook-detected marks: presses outside `0.0..=duration` are dropped, and any poll press within
/// 0.3s of a `hook_mark_times` entry is discarded as a duplicate (the hook mark wins). Each
/// kept press maps its physical cursor position into the region's normalized `0.0..=1.0` space
/// (clamped when the cursor sat outside the captured region).
fn merge_poll_chord_marks(
    marks: &[ChordMark],
    hook_mark_times: &[f64],
    region: &CaptureRegion,
    clock: Clock,
    start_qpc: i64,
    duration: f64,
) -> Vec<InputEvent> {
    let mut out = Vec::new();
    for m in marks {
        let t = clock.seconds_between(start_qpc, m.qpc);
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
        out.push(InputEvent::ZoomMark { t, pos });
    }
    out
}

/// Convert recorded pause spans (`(start_qpc, Option<end_qpc>)`; an open span = still paused
/// at stop) into sorted timeline cuts. Each span is clamped to the recording bounds
/// (`0.0..=duration`), an open span runs to `duration`, and spans shorter than 0.05s (a
/// pause/resume fat-finger) are dropped.
fn pauses_to_cuts(
    pauses: &[(i64, Option<i64>)],
    clock: Clock,
    start_qpc: i64,
    duration: f64,
) -> Vec<Trim> {
    let mut cuts: Vec<Trim> = pauses
        .iter()
        .filter_map(|&(s, e)| {
            let start = clock.seconds_between(start_qpc, s).max(0.0);
            let end = e
                .map_or(duration, |e| clock.seconds_between(start_qpc, e))
                .min(duration);
            (end - start > 0.05).then_some(Trim { start, end })
        })
        .collect();
    sort_cuts(&mut cuts);
    cuts
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

/// Resolve the project's backdrop into the compositor's [`BgFill`] (a linear 2-stop gradient;
/// a solid fill is the degenerate `color2 == color` case). Image/Blur backdrops aren't
/// rendered yet, so they fall back to a flat neutral dark.
fn background_fill(frame: &FrameStyle) -> BgFill {
    let rgba = |c: Color| [c.r, c.g, c.b, c.a];
    match &frame.background {
        Background::Solid(c) => BgFill::solid(rgba(*c)),
        Background::Gradient {
            from,
            to,
            angle_deg,
        } => {
            // Gradient axis as a unit vector in output UV space (y down). The compositor
            // projects each pixel onto it and normalizes across the frame, so `from` lands on
            // the corner the axis points away from and `to` on the corner it points toward.
            let rad = angle_deg.to_radians();
            BgFill {
                color: rgba(*from),
                color2: rgba(*to),
                dir: [rad.cos() as f32, rad.sin() as f32],
            }
        }
        Background::Image { .. } | Background::Blur { .. } => {
            BgFill::solid([0.08, 0.08, 0.09, 1.0])
        }
    }
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

// ── disk free-space guard ───────────────────────────────────────────────────────
// Raw uncompressed BGRA streams straight to disk, so the store grows at `w*h*4` bytes per
// captured frame. Without a guard a full-screen/4K take can silently fill a system disk in
// minutes, so `start_recording` checks free space up front and the drain re-checks while
// recording (see `frame_store::free_space_bytes`).

/// Conservative capture rate (fps) used to size the raw-BGRA write estimate. Real capture is
/// usually 30–60 fps; picking the low end keeps the estimate from over-reserving space.
const ESTIMATE_FPS: u64 = 30;
/// Absolute minimum free space to start any recording, regardless of dimensions — leaves
/// headroom so even a tiny capture can't creep a nearly-full disk to zero.
const MIN_FREE_BYTES: u64 = 2 * 1024 * 1024 * 1024;
/// Also require at least this many seconds of capture to fit, so a large/4K take that would
/// blow past the 2 GB floor in seconds is still blocked up front.
const MIN_FREE_SECONDS: u64 = 30;
/// Above the floor but below this many seconds of capture: start, but warn the user.
const LOW_FREE_SECONDS: u64 = 5 * 60;
/// How often (in frames) the drain re-checks free space — ~2 s at 30 fps, far from per-frame.
const DISK_CHECK_EVERY: u32 = 60;
/// The drain stops writing (salvaging the take) once free space drops below this, so a runaway
/// raw stream can't fully fill the volume before an actual write error would hit.
const DRAIN_STOP_FLOOR_BYTES: u64 = 512 * 1024 * 1024;

/// Bytes per second the raw-BGRA store grows at for a `w`×`h` capture (`w*h*4` × a
/// conservative fps). Enormous by design — ~250 MB/s at 1080p, ~1 GB/s at 4K — which is
/// exactly why free space is guarded before and during recording.
fn raw_write_rate_bps(w: u32, h: u32) -> u64 {
    u64::from(w) * u64::from(h) * 4 * ESTIMATE_FPS
}

/// A dropped frame is only worth warning about once it's a material fraction of the take: more
/// than this share of all captured frames …
const DROP_WARN_FRACTION: f64 = 0.02;
/// … or more than this many frames outright. A handful of drops at capture start-up (before the
/// drain warms up) is normal and stays silent.
const DROP_WARN_MIN_FRAMES: u64 = 30;

/// Build the "capture couldn't keep up" warning if `dropped` is a material shortfall relative to
/// the `kept` frames written to disk. Percentage is measured against everything the capture
/// produced (`kept + dropped`). Returns `None` for a negligible number of drops.
fn dropped_frames_warning(dropped: u64, kept: usize) -> Option<String> {
    if dropped == 0 {
        return None;
    }
    let total = dropped + kept as u64;
    let pct = if total > 0 {
        dropped as f64 / total as f64 * 100.0
    } else {
        0.0
    };
    if dropped > DROP_WARN_MIN_FRAMES || pct > DROP_WARN_FRACTION * 100.0 {
        Some(format!(
            "Capture couldn't keep up — dropped {dropped} frames ({pct:.1}%). \
             Try a smaller region or a less busy disk."
        ))
    } else {
        None
    }
}

/// Decide whether a recording may start given `free_bytes` on the recording volume and the
/// capture dimensions. `Err` blocks the take (not enough space); `Ok(Some(_))` starts it but
/// carries a heads-up warning; `Ok(None)` is plenty of space.
fn check_free_space(free_bytes: u64, w: u32, h: u32) -> Result<Option<String>, String> {
    let rate = raw_write_rate_bps(w, h).max(1);
    let floor = MIN_FREE_BYTES.max(rate.saturating_mul(MIN_FREE_SECONDS));
    let free_gb = free_bytes as f64 / 1e9;
    if free_bytes < floor {
        let need_gb = floor as f64 / 1e9;
        let mbps = rate / (1024 * 1024);
        return Err(format!(
            "Not enough disk space to record: only {free_gb:.1} GB free, need at least {need_gb:.1} GB. \
             Vuoom records raw video (~{mbps} MB/s at this size) — free up space and try again."
        ));
    }
    if free_bytes < rate.saturating_mul(LOW_FREE_SECONDS) {
        let minutes = free_bytes / rate / 60;
        return Ok(Some(format!(
            "Heads up: your disk was low on space when recording started — about {minutes} min fits \
             ({free_gb:.1} GB free). Vuoom stops automatically before the disk fills."
        )));
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use vuoom_input::RawEventKind;

    fn rec(qpc: i64) -> FrameRec {
        FrameRec {
            qpc,
            w: 2,
            h: 2,
            offset: 0,
            len: 16,
        }
    }

    #[test]
    fn nearest_idx_picks_closest_frame_by_time() {
        let clock = Clock::new();
        let f = clock.freq();
        let start = 1000;
        // Frames captured 1s apart at t = 0, 1, 2, 3.
        let recs: Vec<FrameRec> = (0..4i64).map(|i| rec(start + i * f)).collect();

        assert_eq!(nearest_idx(&[], clock, start, 1.0), None);
        // Exact hits.
        assert_eq!(nearest_idx(&recs, clock, start, 0.0), Some(0));
        assert_eq!(nearest_idx(&recs, clock, start, 2.0), Some(2));
        // Out of range clamps to the ends.
        assert_eq!(nearest_idx(&recs, clock, start, -5.0), Some(0));
        assert_eq!(nearest_idx(&recs, clock, start, 99.0), Some(3));
        // Rounds to the nearer neighbour.
        assert_eq!(nearest_idx(&recs, clock, start, 1.4), Some(1));
        assert_eq!(nearest_idx(&recs, clock, start, 1.6), Some(2));
    }

    // --- multi-monitor origin composition ---

    #[test]
    fn capture_region_offsets_crop_by_monitor_origin() {
        // A crop offset within a secondary monitor whose virtual-desktop origin is negative
        // (to the left of / above the primary).
        let r = CropRegion {
            x: 100,
            y: 50,
            w: 800,
            h: 600,
        };
        let region = compose_capture_region((-1920, -100), Some(r), 1920, 1080);
        assert_eq!(
            (region.x, region.y, region.w, region.h),
            (-1820, -50, 800, 600)
        );
    }

    #[test]
    fn capture_region_without_crop_is_full_monitor_at_origin() {
        let full = compose_capture_region((-1920, -100), None, 1920, 1080);
        assert_eq!((full.x, full.y, full.w, full.h), (-1920, -100, 1920, 1080));
    }

    // --- poll-detected chord merge (dedup + normalized mapping) ---

    fn chord(qpc: i64, x: i32, y: i32) -> ChordMark {
        ChordMark { qpc, x, y }
    }

    #[test]
    fn poll_chord_marks_dedup_filter_and_map_negative_origin() {
        let clock = Clock::new();
        let f = clock.freq();
        let start = 0i64;
        let q = |t: f64| start + (t * f as f64) as i64;
        let duration = 10.0;
        // Secondary monitor to the LEFT of the primary: negative virtual-desktop origin.
        let region = CaptureRegion {
            x: -1920,
            y: 0,
            w: 1000,
            h: 500,
        };
        let hook_times = [2.0]; // a hook-detected zoom already sits at t = 2.0

        let marks = [
            chord(q(2.1), -1920 + 500, 250), // within 0.3s of the hook mark -> deduped away
            chord(q(5.0), -1920 + 500, 250), // region center -> (0.5, 0.5)
            chord(q(-1.0), 0, 0),            // before the epoch -> filtered
            chord(q(11.0), 0, 0),            // past the duration -> filtered
            chord(q(7.0), -1920 - 100, 999), // outside the region -> clamped to (0, 1)
        ];
        let out = merge_poll_chord_marks(&marks, &hook_times, &region, clock, start, duration);
        assert_eq!(out.len(), 2);
        match out[0] {
            InputEvent::ZoomMark { t, pos } => {
                assert!((t - 5.0).abs() < 1e-3);
                assert!((pos.x - 0.5).abs() < 1e-6 && (pos.y - 0.5).abs() < 1e-6);
            }
            _ => panic!("expected ZoomMark"),
        }
        match out[1] {
            // x left of the region -> negative ratio clamps to 0; y below the region -> clamps to 1.
            InputEvent::ZoomMark { pos, .. } => {
                assert!((pos.x - 0.0).abs() < 1e-9 && (pos.y - 1.0).abs() < 1e-9);
            }
            _ => panic!("expected ZoomMark"),
        }
    }

    #[test]
    fn poll_chord_mark_far_from_any_hook_mark_is_kept() {
        let clock = Clock::new();
        let f = clock.freq();
        let q = |t: f64| (t * f as f64) as i64;
        let region = CaptureRegion {
            x: 0,
            y: 0,
            w: 100,
            h: 100,
        };
        // Hook mark at 1.0; poll press 0.4s away (> 0.3s) is a distinct zoom, not a dup.
        let out = merge_poll_chord_marks(&[chord(q(1.4), 50, 50)], &[1.0], &region, clock, 0, 5.0);
        assert_eq!(out.len(), 1);
    }

    // --- pause -> cut conversion ---

    #[test]
    fn pauses_convert_to_sorted_clamped_cuts() {
        let clock = Clock::new();
        let f = clock.freq();
        let start = 5_000i64;
        let q = |t: f64| start + (t * f as f64) as i64;
        let duration = 10.0;
        let pauses = [
            (q(6.0), Some(q(7.0))),  // closed 6..7
            (q(2.0), Some(q(3.0))),  // closed 2..3 (earlier — must sort first among closed)
            (q(8.0), None),          // open at stop -> runs to the duration
            (q(4.0), Some(q(4.01))), // ~10ms span -> below the 0.05s floor, dropped
            (q(-1.0), Some(q(0.5))), // starts before the epoch -> start clamps to 0.0
        ];
        let cuts = pauses_to_cuts(&pauses, clock, start, duration);
        let got: Vec<(f64, f64)> = cuts.iter().map(|c| (c.start, c.end)).collect();

        assert_eq!(cuts.len(), 4); // the tiny span is dropped
        assert!(got.windows(2).all(|w| w[0].0 <= w[1].0)); // sorted by start
                                                           // Earliest span had a pre-epoch start clamped to 0.0.
        assert!((got[0].0 - 0.0).abs() < 1e-3 && (got[0].1 - 0.5).abs() < 1e-3);
        // The open span ends at the recording duration.
        let last = *got.last().unwrap();
        assert!((last.0 - 8.0).abs() < 1e-3 && (last.1 - 10.0).abs() < 1e-3);
    }

    #[test]
    fn pause_open_past_duration_clamps_to_duration() {
        let clock = Clock::new();
        let f = clock.freq();
        let q = |t: f64| (t * f as f64) as i64;
        // A closed span whose end overshoots the recording is clamped back to `duration`.
        let cuts = pauses_to_cuts(&[(q(1.0), Some(q(99.0)))], clock, 0, 4.0);
        assert_eq!(cuts.len(), 1);
        assert!((cuts[0].start - 1.0).abs() < 1e-3 && (cuts[0].end - 4.0).abs() < 1e-3);
    }

    #[test]
    fn zero_duration_pause_is_dropped() {
        let clock = Clock::new();
        // A pause and resume at the same instant produces no cut.
        assert!(pauses_to_cuts(&[(1_000, Some(1_000))], clock, 0, 5.0).is_empty());
    }

    // --- shortcut/special key tap extraction ---

    fn rawk(qpc: i64, kind: RawEventKind) -> RawEvent {
        RawEvent {
            qpc,
            x: 0,
            y: 0,
            kind,
        }
    }

    #[test]
    fn key_taps_label_chords_and_specials_but_not_plain_typing() {
        use vuoom_input::RawEventKind::{KeyDown, KeyUp};
        let clock = Clock::new();
        let f = clock.freq();
        let q = |t: f64| (t * f as f64) as i64;
        let duration = 10.0;

        let raw = [
            // Ctrl+Shift+P chord.
            rawk(q(1.0), KeyDown(0x11)),  // Ctrl
            rawk(q(1.01), KeyDown(0x10)), // Shift
            rawk(q(1.02), KeyDown(0x50)), // P
            rawk(q(1.03), KeyUp(0x50)),
            rawk(q(1.04), KeyUp(0x10)),
            rawk(q(1.05), KeyUp(0x11)),
            // Plain typing (no modifier, not a standalone special) -> never labeled.
            rawk(q(2.0), KeyDown(0x41)), // 'A'
            rawk(q(2.01), KeyUp(0x41)),
            // Standalone Enter -> labeled on its own.
            rawk(q(3.0), KeyDown(0x0D)),
            rawk(q(3.01), KeyUp(0x0D)),
            // Enter auto-repeat shortly after -> coalesced into the press above.
            rawk(q(3.2), KeyDown(0x0D)),
            // Past the recording end -> filtered.
            rawk(q(20.0), KeyDown(0x0D)),
        ];
        let taps = extract_key_taps(&raw, clock, 0, duration);
        let labels: Vec<&str> = taps.iter().map(|t| t.label.as_str()).collect();
        assert_eq!(labels, vec!["Ctrl+Shift+P", "Enter"]);
        assert!((taps[0].t - 1.02).abs() < 1e-3);
    }

    #[test]
    fn vuoom_control_chords_are_suppressed() {
        use vuoom_input::RawEventKind::{KeyDown, KeyUp};
        let clock = Clock::new();
        let f = clock.freq();
        let q = |t: f64| (t * f as f64) as i64;

        let raw = [
            // Ctrl+Shift+X — the stop hotkey — must not appear.
            rawk(q(1.0), KeyDown(0x11)),
            rawk(q(1.01), KeyDown(0x10)),
            rawk(q(1.02), KeyDown(0x58)), // X
            rawk(q(1.03), KeyUp(0x58)),
            rawk(q(1.04), KeyUp(0x10)),
            rawk(q(1.05), KeyUp(0x11)),
            // Ctrl+Shift+Z — the zoom chord — must not appear either.
            rawk(q(2.0), KeyDown(0x11)),
            rawk(q(2.01), KeyDown(0x10)),
            rawk(q(2.02), KeyDown(0x5A)), // Z
            rawk(q(2.03), KeyUp(0x5A)),
            rawk(q(2.04), KeyUp(0x10)),
            rawk(q(2.05), KeyUp(0x11)),
            // A real user chord (Ctrl+Shift+C) still renders.
            rawk(q(3.0), KeyDown(0x11)),
            rawk(q(3.01), KeyDown(0x10)),
            rawk(q(3.02), KeyDown(0x43)), // C
        ];
        let taps = extract_key_taps(&raw, clock, 0, 10.0);
        let labels: Vec<&str> = taps.iter().map(|t| t.label.as_str()).collect();
        assert_eq!(labels, vec!["Ctrl+Shift+C"]);
    }

    // --- disk free-space guard ---

    const GB: u64 = 1_000_000_000;

    #[test]
    fn raw_write_rate_scales_with_pixel_area() {
        // 4K has 4× the pixels of 1080p, so 4× the byte rate.
        assert_eq!(
            raw_write_rate_bps(3840, 2160),
            4 * raw_write_rate_bps(1920, 1080)
        );
        // 1080p at 30 fps: 1920*1080*4*30 bytes/s.
        assert_eq!(raw_write_rate_bps(1920, 1080), 1920 * 1080 * 4 * 30);
    }

    #[test]
    fn free_space_blocks_below_absolute_floor() {
        // 1 GB is under the 2 GB hard floor even for a tiny capture.
        let err = check_free_space(GB, 640, 480).unwrap_err();
        assert!(err.contains("Not enough disk space"));
    }

    #[test]
    fn free_space_blocks_when_under_thirty_seconds_of_capture() {
        // 5 GB clears the 2 GB floor, but 1080p burns ~250 MB/s, so 30 s needs ~7.5 GB.
        assert!(check_free_space(5 * GB, 1920, 1080).is_err());
    }

    #[test]
    fn free_space_warns_when_low_but_sufficient() {
        // 20 GB at 1080p: above the ~7.5 GB / 30 s floor but below the ~75 GB / 5 min warn line.
        let warn = check_free_space(20 * GB, 1920, 1080)
            .expect("should start")
            .expect("should warn");
        assert!(warn.contains("low on space"));
    }

    #[test]
    fn free_space_ok_when_plenty() {
        // 500 GB at 1080p clears even the 5 min warn line — no warning.
        assert_eq!(check_free_space(500 * GB, 1920, 1080), Ok(None));
    }
}
