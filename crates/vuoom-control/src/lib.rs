//! Control protocol for Vuoom's AI Demo Director.
//!
//! Vuoom runs a small localhost control server (in `src-tauri`) that an AI agent drives
//! through the standalone [`vuoom-mcp`](../vuoom_mcp/index.html) sidecar. This crate is the
//! contract between them: the request/response types plus a tiny blocking TCP [`Client`].
//!
//! The wire format is **newline-delimited JSON**: each request is one line, each response is
//! one line. That keeps the server trivial (read a line → dispatch → write a line) and the
//! whole crate dependency-light (just serde), so it compiles fast and is fully
//! unit-testable without a GPU/display. See `docs/13-AI-Demo-Director-Research.md`.
//!
//! **Authentication.** Loopback is not a trust boundary — any local process could otherwise
//! drive input injection. The server generates a random [`generate_token`] at startup,
//! publishes it in the discovery file next to the port, and requires it as the first line
//! of every connection before any request is served.

#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use std::io::{self, BufRead, BufReader, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// How long the client waits for a connection to be accepted.
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(3);
/// How long the client waits for a single reply. Generous because `get_frames` composites
/// on the GPU; exports are asynchronous jobs precisely so they never hit this.
pub const READ_TIMEOUT: Duration = Duration::from_secs(120);

/// A mouse button for an injected click.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Button {
    /// The primary (left) button.
    #[default]
    Left,
    /// The secondary (right) button.
    Right,
    /// The middle (wheel) button.
    Middle,
}

/// A command sent from the agent (via the MCP sidecar) to Vuoom's control server.
///
/// All screen coordinates are **virtual-desktop physical pixels** — the same space the
/// capture and auto-zoom planner work in — so an injected click lands exactly where the
/// agent saw it and the zoom centres on the right spot. Use [`ControlRequest::ScreenGeometry`]
/// to learn the desktop bounds first. Clip/edit times are seconds; unless stated otherwise
/// they are **output-timeline** for sampling and **source-timeline** for edit ops (matching
/// the editor UI).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum ControlRequest {
    /// Liveness check — the server answers [`ControlResponse::Ok`].
    Ping,
    /// Ask for the virtual-desktop bounds (so the agent can reason in screen pixels).
    ScreenGeometry,
    /// Capture the screen **right now** (independent of any recording) so the agent can see
    /// the current state and locate what to click. Answered with [`ControlResponse::Shot`].
    Screenshot {
        /// Optional max width (px) to downscale the returned PNG.
        width: Option<u32>,
    },
    /// Set the capture region (physical px) for the next recording. Omit all fields (or pass
    /// a zero-area rect) to capture the full screen.
    SetRegion {
        /// Left edge (physical px); `None` = full screen.
        x: Option<u32>,
        /// Top edge (physical px).
        y: Option<u32>,
        /// Width (physical px).
        w: Option<u32>,
        /// Height (physical px).
        h: Option<u32>,
    },
    /// Set the zoom multiplier (1.0 = no zoom) applied to the next recording.
    SetZoomAmount {
        /// Multiplier in `[1.0, 4.0]` (clamped server-side).
        amount: f64,
    },
    /// Tune the auto-zoom *feel* for the next recording. `None` fields keep the defaults.
    SetZoomStyle {
        /// Seconds of inactivity before the camera zooms back out.
        hold: Option<f64>,
        /// Seconds of anticipation before a click that the zoom-in begins.
        pre_roll: Option<f64>,
        /// Half-life (s) of the zoom spring — smaller = snappier zoom.
        hl_zoom: Option<f64>,
        /// Half-life (s) of the pan spring — smaller = snappier follow.
        hl_pan: Option<f64>,
        /// Clicks within this gap (s) may merge into one zoom (clamped `0.1..=5.0`).
        merge_gap: Option<f64>,
        /// Clicks within this normalized distance of a cluster merge into it
        /// (clamped `0.02..=1.0`).
        merge_radius: Option<f64>,
        /// Minimum seconds between the end of one zoom and the start of the next
        /// (clamped `0.0..=10.0`).
        min_rezoom_interval: Option<f64>,
        /// Normalized half-extent the focus must leave before the pan retargets — jitter
        /// rejection (clamped `0.0..=0.5`).
        dead_zone: Option<f64>,
    },
    /// Begin capturing the screen + global input. `auto_zoom_on_click` defaults to `true`
    /// for agent recordings (the agent drives via clicks); the flag applies to **this**
    /// recording only and never leaks into interactive use.
    StartRecording {
        /// `true` (default) = every click seeds a cinematic zoom; `false` = manual only.
        auto_zoom_on_click: Option<bool>,
    },
    /// Stop capturing and build the editable project; answered with [`ControlResponse::Recording`].
    StopRecording,
    /// Stop capturing and **discard** the take (no project is built).
    CancelRecording,
    /// Pause or resume the running recording (a paused span becomes a cut).
    SetPaused {
        /// `true` to pause, `false` to resume.
        paused: bool,
    },
    /// Ask what the engine is doing; answered with [`ControlResponse::Status`].
    Status,
    /// Glide the cursor to `(x, y)` without clicking (minimum-jerk path).
    MoveCursor {
        /// Target x (physical px).
        x: i32,
        /// Target y (physical px).
        y: i32,
        /// Glide time in ms; `None` = distance-scaled, `0` = instant warp.
        duration_ms: Option<u32>,
    },
    /// Glide to `(x, y)`, settle, and click. These synthetic clicks flow through Vuoom's
    /// input hook exactly like real ones, so they drive both the target app and the
    /// cinematic auto-zoom.
    Click {
        /// Click x (physical px).
        x: i32,
        /// Click y (physical px).
        y: i32,
        /// Which button (defaults to left).
        #[serde(default)]
        button: Button,
        /// `true` for a double-click.
        #[serde(default)]
        double: bool,
        /// Glide time in ms; `None` = distance-scaled, `0` = instant warp.
        glide_ms: Option<u32>,
    },
    /// Press-drag from `(x1, y1)` to `(x2, y2)` — sliders, drag-and-drop, text selection.
    Drag {
        /// Drag start x (physical px).
        x1: i32,
        /// Drag start y (physical px).
        y1: i32,
        /// Drag end x (physical px).
        x2: i32,
        /// Drag end y (physical px).
        y2: i32,
        /// Which button to hold (defaults to left).
        #[serde(default)]
        button: Button,
        /// Duration of the dragging portion in ms; `None` = distance-scaled.
        duration_ms: Option<u32>,
    },
    /// Type a string of Unicode text into the focused control at a human cadence.
    TypeText {
        /// The text to type. `\n`/`\t` press real Enter/Tab.
        text: String,
        /// Characters per second; `None` = ~15 with natural jitter.
        cps: Option<f64>,
    },
    /// Press a key chord, e.g. `["ctrl", "c"]` or `["enter"]`. Modifiers are held while the
    /// final key is tapped, then released in reverse order.
    KeyChord {
        /// Key names (modifiers first); see the injection key table for accepted names.
        keys: Vec<String>,
    },
    /// Scroll the wheel at `(x, y)`; positive `delta` scrolls up, one notch per step.
    Scroll {
        /// Pointer x (physical px).
        x: i32,
        /// Pointer y (physical px).
        y: i32,
        /// Wheel delta in notches (positive = up).
        delta: i32,
        /// Gap between notches in ms; `None` = 40, `0` = all at once.
        step_ms: Option<u32>,
    },
    /// Composite and publish the preview frame at time `t` (seconds, output timeline).
    Seek {
        /// Time from the clip start, in seconds.
        t: f64,
    },
    /// Ask for the full editable state of the clip (durations, zoom/cut/speed spans).
    ClipState,
    /// Composite the given **output-timeline** times and return them as PNGs so the agent
    /// can *see* exactly what the export will produce, and critique it. Sample sparsely
    /// (e.g. around each zoom span from [`ControlRequest::ClipState`]) to keep cost down.
    GetFrames {
        /// Times (seconds, output timeline) to sample.
        times: Vec<f64>,
        /// Optional max width (px) to downscale each returned frame.
        width: Option<u32>,
    },
    /// Insert a zoom segment starting at source time `t`; answered with the updated list.
    /// Provide all four `rect_*` fields to frame a normalized region (fit-and-centre);
    /// omit them for a plain cursor-following segment.
    AddZoom {
        /// Source-timeline start (seconds); the segment gets a default length.
        t: f64,
        /// Rect left in `[0, 1]` (all four `rect_*` required together → Rect mode).
        rect_x: Option<f64>,
        /// Rect top in `[0, 1]`.
        rect_y: Option<f64>,
        /// Rect width in `[0, 1]`.
        rect_w: Option<f64>,
        /// Rect height in `[0, 1]`.
        rect_h: Option<f64>,
        /// Per-span zoom-*in* spring half-life (s); `None` = config default.
        hl_zoom_in: Option<f64>,
        /// Per-span zoom-*out* (release) spring half-life (s); `None` = default release.
        hl_zoom_out: Option<f64>,
    },
    /// Retime / re-level the zoom at `index`; answered with the updated list.
    UpdateZoom {
        /// Index into the sorted zoom list (see [`ControlRequest::ClipState`]).
        index: usize,
        /// New start (source seconds).
        start: f64,
        /// New end (source seconds).
        end: f64,
        /// New zoom multiplier (clamped to `[1.1, 4.0]`).
        amount: f64,
        /// Per-span zoom-*in* half-life (s): `None` leaves it unchanged, a value `> 0`
        /// sets it, and a value `<= 0` clears it back to the config default.
        hl_zoom_in: Option<f64>,
        /// Per-span zoom-*out* (release) half-life (s): same convention as `hl_zoom_in`.
        hl_zoom_out: Option<f64>,
    },
    /// Re-centre the zoom at `index`: frame a normalized rect, hold a fixed focus, or follow
    /// the cursor again. Precedence: all four `rect_*` → Rect mode; else `x`+`y` → Manual;
    /// else Auto (follow the cursor).
    SetZoomFocus {
        /// Index into the sorted zoom list.
        index: usize,
        /// Normalized focus x in `[0, 1]`; omit both `x`/`y` (and the rect) to follow the cursor.
        x: Option<f64>,
        /// Normalized focus y in `[0, 1]`.
        y: Option<f64>,
        /// Rect left in `[0, 1]` (all four `rect_*` required together → Rect mode).
        rect_x: Option<f64>,
        /// Rect top in `[0, 1]`.
        rect_y: Option<f64>,
        /// Rect width in `[0, 1]`.
        rect_w: Option<f64>,
        /// Rect height in `[0, 1]`.
        rect_h: Option<f64>,
    },
    /// Delete the zoom at `index`; answered with the updated list.
    RemoveZoom {
        /// Index into the sorted zoom list.
        index: usize,
    },
    /// Remove `[start, end]` (source seconds) from the output; answered with updated cuts.
    AddCut {
        /// Cut start (source seconds).
        start: f64,
        /// Cut end (source seconds).
        end: f64,
    },
    /// Retime the cut at `index`; answered with the updated list.
    UpdateCut {
        /// Index into the sorted cut list.
        index: usize,
        /// New start (source seconds).
        start: f64,
        /// New end (source seconds).
        end: f64,
    },
    /// Restore the cut at `index`; answered with the updated list.
    RemoveCut {
        /// Index into the sorted cut list.
        index: usize,
    },
    /// Detect idle stretches and mark them to play at `factor`× (replaces existing speed
    /// regions); answered with the new list.
    AutoSpeed {
        /// Speed-up factor (clamped to `[1.5, 16.0]`).
        factor: f64,
    },
    /// Remove all speed regions (play everything at 1×).
    ClearSpeed,
    /// Trim the clip to `[start, end]` (source seconds); the full range clears the trim.
    SetTrim {
        /// Trim start (source seconds).
        start: f64,
        /// Trim end (source seconds).
        end: f64,
    },
    /// Start exporting the edited clip to an animated GIF. Answered immediately with
    /// [`ControlResponse::Job`]; poll with [`ControlRequest::ExportStatus`].
    ExportGif {
        /// Absolute output path.
        path: String,
        /// Output frame rate.
        fps: u32,
        /// Optional max width (px).
        width: Option<u32>,
        /// Quality 1–100.
        quality: u8,
    },
    /// Start exporting the edited clip to an H.264 MP4 (job-based, like `ExportGif`).
    ExportMp4 {
        /// Absolute output path.
        path: String,
        /// Output frame rate.
        fps: u32,
        /// Optional max width (px).
        width: Option<u32>,
        /// Quality 1–100.
        quality: u8,
    },
    /// Ask how an export job is doing; answered with [`ControlResponse::Export`].
    ExportStatus {
        /// The id returned by `ExportGif`/`ExportMp4`.
        id: u64,
    },
    /// Estimate the GIF export size (bytes) for the given settings.
    EstimateGif {
        /// Output frame rate.
        fps: u32,
        /// Optional max width (px).
        width: Option<u32>,
        /// Quality 1–100.
        quality: u8,
    },
}

/// The virtual-desktop bounds in physical pixels (origin can be negative with multi-monitor).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScreenGeometry {
    /// Left edge of the virtual desktop.
    pub x: i32,
    /// Top edge of the virtual desktop.
    pub y: i32,
    /// Total width across all monitors.
    pub width: i32,
    /// Total height across all monitors.
    pub height: i32,
}

/// Summary of a finished recording.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct RecordingSummary {
    /// Clip length in seconds (source timeline).
    pub duration: f64,
    /// Number of captured frames.
    pub frames: usize,
    /// Number of auto-planned zoom segments.
    pub zooms: usize,
}

/// One zoom segment on the source timeline.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ZoomSpan {
    /// Segment start (source seconds).
    pub start: f64,
    /// Segment end (source seconds).
    pub end: f64,
    /// Zoom multiplier while active.
    pub amount: f64,
    /// Focus point: the fixed point for `"manual"`, the rect centre for `"rect"`, and
    /// `None` for `"auto"` (the camera follows the cursor).
    pub focus: Option<(f64, f64)>,
    /// How this segment picks its focus: `"auto"`, `"manual"`, or `"rect"`.
    pub mode: String,
    /// The framed region `(x, y, w, h)` in normalized space when `mode == "rect"`, else `None`.
    pub rect: Option<(f64, f64, f64, f64)>,
}

/// One cut (removed span) on the source timeline.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CutSpan {
    /// Cut start (source seconds).
    pub start: f64,
    /// Cut end (source seconds).
    pub end: f64,
}

/// One speed region on the source timeline.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SpeedSpan {
    /// Region start (source seconds).
    pub start: f64,
    /// Region end (source seconds).
    pub end: f64,
    /// Playback factor (e.g. 4.0 = 4× faster).
    pub factor: f64,
}

/// What the engine is currently doing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecordState {
    /// No recording running, no clip loaded.
    Idle,
    /// Actively capturing.
    Recording,
    /// Capturing, but paused (the span will become a cut).
    Paused,
    /// A finished clip is loaded and editable/exportable.
    ClipReady,
}

/// Reply to [`ControlRequest::Status`].
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct StatusInfo {
    /// The engine state.
    pub state: RecordState,
    /// Seconds since the recording started (present while recording/paused).
    pub elapsed: Option<f64>,
}

/// The full editable state of the current clip — everything the agent needs to critique
/// and repair a take without re-recording.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClipInfo {
    /// Source-timeline length in seconds.
    pub duration: f64,
    /// Output-timeline length (after trim + cuts + speed) — what the export produces and
    /// the timeline [`ControlRequest::GetFrames`]/[`ControlRequest::Seek`] sample in.
    pub output_duration: f64,
    /// The zoom segments, sorted by start (edit ops address them by this index).
    pub zooms: Vec<ZoomSpan>,
    /// The cuts, sorted by start.
    pub cuts: Vec<CutSpan>,
    /// The speed regions, sorted by start.
    pub speeds: Vec<SpeedSpan>,
    /// Coarsely-sampled camera path `(t, cx, cy, zoom)` on the **output timeline** (~4 Hz,
    /// capped ~200 samples) so the agent can detect wander or poor framing without pulling
    /// full frames. `t` is output-timeline seconds (consistent with `get_frames`/`seek`);
    /// `cx`/`cy` are the normalized camera centre and `zoom` the multiplier.
    pub camera: Vec<(f64, f64, f64, f64)>,
}

/// A single composited frame, returned for the agent to inspect.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FrameShot {
    /// The time (seconds, output timeline) this frame was sampled at; `0.0` for live shots.
    pub t: f64,
    /// Frame width in pixels.
    pub width: u32,
    /// Frame height in pixels.
    pub height: u32,
    /// The frame encoded as a base64 PNG (no data-URL prefix).
    pub png_base64: String,
}

/// Progress of an asynchronous export job.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExportState {
    /// Frames composited so far.
    pub done: u64,
    /// Total frames to composite (0 until known).
    pub total: u64,
    /// Whether the job has finished (successfully or not).
    pub finished: bool,
    /// The failure reason, if the job failed.
    pub error: Option<String>,
    /// The output path the job writes to.
    pub path: String,
}

/// The server's reply to a [`ControlRequest`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ControlResponse {
    /// The command succeeded with no payload.
    Ok,
    /// The command failed; `message` explains why.
    Error {
        /// Human-readable failure reason.
        message: String,
    },
    /// Reply to [`ControlRequest::ScreenGeometry`].
    Geometry(ScreenGeometry),
    /// Reply to [`ControlRequest::StopRecording`].
    Recording(RecordingSummary),
    /// Reply to [`ControlRequest::ClipState`].
    Clip(ClipInfo),
    /// Reply to [`ControlRequest::Status`].
    Status(StatusInfo),
    /// Reply to [`ControlRequest::Screenshot`].
    Shot(FrameShot),
    /// Reply to [`ControlRequest::GetFrames`].
    Frames {
        /// The sampled frames, in request order.
        frames: Vec<FrameShot>,
    },
    /// Reply to the zoom edit ops — the updated, sorted segment list.
    Zooms {
        /// All zoom segments after the edit.
        zooms: Vec<ZoomSpan>,
    },
    /// Reply to the cut edit ops — the updated, sorted cut list.
    Cuts {
        /// All cuts after the edit.
        cuts: Vec<CutSpan>,
    },
    /// Reply to [`ControlRequest::AutoSpeed`] — the new speed regions.
    Speeds {
        /// All speed regions after the edit.
        speeds: Vec<SpeedSpan>,
    },
    /// Reply to [`ControlRequest::ExportGif`]/[`ControlRequest::ExportMp4`] — the job id.
    Job {
        /// Pass to [`ControlRequest::ExportStatus`] to poll progress.
        id: u64,
    },
    /// Reply to [`ControlRequest::ExportStatus`].
    Export(ExportState),
    /// Reply to [`ControlRequest::EstimateGif`] — estimated size in bytes.
    Size {
        /// Estimated export size in bytes.
        bytes: u64,
    },
}

impl ControlResponse {
    /// Build an [`ControlResponse::Error`] from anything string-like.
    pub fn error(message: impl Into<String>) -> Self {
        Self::Error {
            message: message.into(),
        }
    }
}

/// On-disk record of the control server endpoint, for sidecar discovery.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct PortFile {
    port: u16,
    #[serde(default)]
    token: String,
}

/// The well-known file the control server writes its endpoint to, so the standalone
/// MCP sidecar (a separate process) can find it: `%TEMP%/vuoom-control.json`.
#[must_use]
pub fn port_file_path() -> PathBuf {
    std::env::temp_dir().join("vuoom-control.json")
}

/// Generate a fresh 128-bit hex auth token.
///
/// Uses the OS-seeded per-process randomness behind `RandomState` — no extra dependency,
/// and unguessable by other local processes.
#[must_use]
pub fn generate_token() -> String {
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hasher};
    let a = RandomState::new().build_hasher().finish();
    let b = RandomState::new().build_hasher().finish();
    format!("{a:016x}{b:016x}")
}

/// Write the endpoint to `path` as `{"port": N, "token": "..."}`.
///
/// # Errors
/// Returns any serialization or I/O error.
pub fn write_port_file_at(path: &Path, port: u16, token: &str) -> io::Result<()> {
    let bytes = serde_json::to_vec(&PortFile {
        port,
        token: token.to_string(),
    })?;
    std::fs::write(path, bytes)
}

/// Write the control endpoint to the well-known [`port_file_path`].
///
/// # Errors
/// Returns any serialization or I/O error.
pub fn write_port_file(port: u16, token: &str) -> io::Result<()> {
    write_port_file_at(&port_file_path(), port, token)
}

/// Delete the discovery file (server shutdown) so a stale endpoint never points a future
/// sidecar at a dead — or someone else's — port. Missing file is fine.
pub fn remove_port_file() {
    let _ = std::fs::remove_file(port_file_path());
}

/// Read an endpoint from `path`, returning `None` if it is missing or malformed.
#[must_use]
pub fn read_endpoint_at(path: &Path) -> Option<(u16, String)> {
    let data = std::fs::read(path).ok()?;
    let pf: PortFile = serde_json::from_slice(&data).ok()?;
    Some((pf.port, pf.token))
}

/// Discover the control endpoint: the `VUOOM_CONTROL_PORT` env var overrides the port
/// (the token still comes from the discovery file, if present).
#[must_use]
pub fn discover_endpoint() -> Option<(u16, String)> {
    let file = read_endpoint_at(&port_file_path());
    if let Ok(p) = std::env::var("VUOOM_CONTROL_PORT") {
        if let Ok(port) = p.trim().parse::<u16>() {
            let token = file.map(|(_, t)| t).unwrap_or_default();
            return Some((port, token));
        }
    }
    file
}

/// Write `msg` as a single newline-terminated JSON line and flush.
///
/// # Errors
/// Returns any serialization or I/O error.
pub fn write_message<W: Write, T: Serialize>(w: &mut W, msg: &T) -> io::Result<()> {
    let line = serde_json::to_string(msg)?;
    w.write_all(line.as_bytes())?;
    w.write_all(b"\n")?;
    w.flush()
}

/// A blocking client for Vuoom's control server, used by the MCP sidecar.
pub struct Client {
    stream: TcpStream,
    reader: BufReader<TcpStream>,
}

impl Client {
    /// Connect to the control server on `127.0.0.1:port` and authenticate with `token`.
    ///
    /// Applies [`CONNECT_TIMEOUT`] and [`READ_TIMEOUT`] so a hung server can never wedge
    /// the sidecar (and with it, the agent's turn) forever.
    ///
    /// # Errors
    /// Returns an [`io::Error`] if the connection cannot be established.
    pub fn connect(port: u16, token: &str) -> io::Result<Self> {
        let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
        let stream = TcpStream::connect_timeout(&addr, CONNECT_TIMEOUT)?;
        stream.set_read_timeout(Some(READ_TIMEOUT))?;
        let mut stream = stream;
        stream.write_all(token.as_bytes())?;
        stream.write_all(b"\n")?;
        stream.flush()?;
        let reader = BufReader::new(stream.try_clone()?);
        Ok(Self { stream, reader })
    }

    /// Send one request and read exactly one response.
    ///
    /// # Errors
    /// Returns a message on serialization, I/O, or connection-closed errors. A
    /// [`ControlResponse::Error`] from the server is returned as `Ok(Error { .. })`, not `Err`
    /// — only transport failures are `Err`.
    pub fn call(&mut self, req: &ControlRequest) -> Result<ControlResponse, String> {
        write_message(&mut self.stream, req).map_err(|e| e.to_string())?;
        let mut line = String::new();
        let n = self
            .reader
            .read_line(&mut line)
            .map_err(|e| e.to_string())?;
        if n == 0 {
            return Err(
                "control server closed the connection (bad auth token or server shutdown)".into(),
            );
        }
        serde_json::from_str(&line).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every request variant must survive a JSON round-trip unchanged.
    #[test]
    fn requests_round_trip() {
        let cases = vec![
            ControlRequest::Ping,
            ControlRequest::ScreenGeometry,
            ControlRequest::Screenshot { width: Some(800) },
            ControlRequest::SetRegion {
                x: Some(10),
                y: Some(20),
                w: Some(640),
                h: Some(480),
            },
            ControlRequest::SetRegion {
                x: None,
                y: None,
                w: None,
                h: None,
            },
            ControlRequest::SetZoomAmount { amount: 2.0 },
            ControlRequest::SetZoomStyle {
                hold: Some(2.5),
                pre_roll: None,
                hl_zoom: Some(0.25),
                hl_pan: None,
                merge_gap: Some(1.2),
                merge_radius: None,
                min_rezoom_interval: Some(0.5),
                dead_zone: Some(0.2),
            },
            ControlRequest::StartRecording {
                auto_zoom_on_click: Some(true),
            },
            ControlRequest::StopRecording,
            ControlRequest::CancelRecording,
            ControlRequest::SetPaused { paused: true },
            ControlRequest::Status,
            ControlRequest::MoveCursor {
                x: -5,
                y: 100,
                duration_ms: Some(300),
            },
            ControlRequest::Click {
                x: 1,
                y: 2,
                button: Button::Right,
                double: true,
                glide_ms: None,
            },
            ControlRequest::Drag {
                x1: 10,
                y1: 20,
                x2: 300,
                y2: 400,
                button: Button::Left,
                duration_ms: Some(600),
            },
            ControlRequest::TypeText {
                text: "hello world".into(),
                cps: Some(20.0),
            },
            ControlRequest::KeyChord {
                keys: vec!["ctrl".into(), "c".into()],
            },
            ControlRequest::Scroll {
                x: 3,
                y: 4,
                delta: -2,
                step_ms: None,
            },
            ControlRequest::Seek { t: 1.5 },
            ControlRequest::ClipState,
            ControlRequest::GetFrames {
                times: vec![0.0, 1.0, 2.5],
                width: Some(800),
            },
            ControlRequest::AddZoom {
                t: 3.5,
                rect_x: Some(0.1),
                rect_y: Some(0.2),
                rect_w: Some(0.3),
                rect_h: Some(0.25),
                hl_zoom_in: Some(0.4),
                hl_zoom_out: None,
            },
            ControlRequest::UpdateZoom {
                index: 1,
                start: 2.0,
                end: 4.5,
                amount: 2.2,
                hl_zoom_in: Some(0.35),
                hl_zoom_out: Some(0.0),
            },
            ControlRequest::SetZoomFocus {
                index: 0,
                x: Some(0.25),
                y: Some(0.75),
                rect_x: None,
                rect_y: None,
                rect_w: None,
                rect_h: None,
            },
            ControlRequest::RemoveZoom { index: 2 },
            ControlRequest::AddCut {
                start: 1.0,
                end: 2.0,
            },
            ControlRequest::UpdateCut {
                index: 0,
                start: 1.5,
                end: 2.5,
            },
            ControlRequest::RemoveCut { index: 0 },
            ControlRequest::AutoSpeed { factor: 4.0 },
            ControlRequest::ClearSpeed,
            ControlRequest::SetTrim {
                start: 0.5,
                end: 9.5,
            },
            ControlRequest::ExportGif {
                path: "C:/tmp/out.gif".into(),
                fps: 20,
                width: Some(900),
                quality: 80,
            },
            ControlRequest::ExportMp4 {
                path: "C:/tmp/out.mp4".into(),
                fps: 30,
                width: None,
                quality: 75,
            },
            ControlRequest::ExportStatus { id: 7 },
            ControlRequest::EstimateGif {
                fps: 20,
                width: None,
                quality: 80,
            },
        ];
        for req in cases {
            let line = serde_json::to_string(&req).expect("serialize");
            assert!(!line.contains('\n'), "requests must be single-line");
            let back: ControlRequest = serde_json::from_str(&line).expect("deserialize");
            assert_eq!(req, back);
        }
    }

    /// New optional pacing fields may be omitted on the wire (older callers still work).
    #[test]
    fn optional_fields_default_when_missing() {
        let click: ControlRequest =
            serde_json::from_str(r#"{"op":"click","x":1,"y":2}"#).expect("parse");
        assert_eq!(
            click,
            ControlRequest::Click {
                x: 1,
                y: 2,
                button: Button::Left,
                double: false,
                glide_ms: None,
            }
        );
        let start: ControlRequest =
            serde_json::from_str(r#"{"op":"start_recording"}"#).expect("parse");
        assert_eq!(
            start,
            ControlRequest::StartRecording {
                auto_zoom_on_click: None
            }
        );
        let text: ControlRequest =
            serde_json::from_str(r#"{"op":"type_text","text":"hi"}"#).expect("parse");
        assert_eq!(
            text,
            ControlRequest::TypeText {
                text: "hi".into(),
                cps: None
            }
        );
    }

    /// Pre-existing callers omit the new zoom fields entirely — they must still parse, with
    /// every new optional field defaulting to `None`.
    #[test]
    fn new_zoom_fields_default_when_missing() {
        let add: ControlRequest =
            serde_json::from_str(r#"{"op":"add_zoom","t":3.5}"#).expect("parse");
        assert_eq!(
            add,
            ControlRequest::AddZoom {
                t: 3.5,
                rect_x: None,
                rect_y: None,
                rect_w: None,
                rect_h: None,
                hl_zoom_in: None,
                hl_zoom_out: None,
            }
        );
        let upd: ControlRequest = serde_json::from_str(
            r#"{"op":"update_zoom","index":1,"start":2.0,"end":4.5,"amount":2.2}"#,
        )
        .expect("parse");
        assert_eq!(
            upd,
            ControlRequest::UpdateZoom {
                index: 1,
                start: 2.0,
                end: 4.5,
                amount: 2.2,
                hl_zoom_in: None,
                hl_zoom_out: None,
            }
        );
        let focus: ControlRequest =
            serde_json::from_str(r#"{"op":"set_zoom_focus","index":0,"x":0.25,"y":0.75}"#)
                .expect("parse");
        assert_eq!(
            focus,
            ControlRequest::SetZoomFocus {
                index: 0,
                x: Some(0.25),
                y: Some(0.75),
                rect_x: None,
                rect_y: None,
                rect_w: None,
                rect_h: None,
            }
        );
        let style: ControlRequest =
            serde_json::from_str(r#"{"op":"set_zoom_style","hold":2.0}"#).expect("parse");
        assert_eq!(
            style,
            ControlRequest::SetZoomStyle {
                hold: Some(2.0),
                pre_roll: None,
                hl_zoom: None,
                hl_pan: None,
                merge_gap: None,
                merge_radius: None,
                min_rezoom_interval: None,
                dead_zone: None,
            }
        );
    }

    /// Every response variant must survive a JSON round-trip unchanged.
    #[test]
    fn responses_round_trip() {
        let cases = vec![
            ControlResponse::Ok,
            ControlResponse::error("boom"),
            ControlResponse::Geometry(ScreenGeometry {
                x: -1920,
                y: 0,
                width: 3840,
                height: 1080,
            }),
            ControlResponse::Recording(RecordingSummary {
                duration: 12.5,
                frames: 750,
                zooms: 3,
            }),
            ControlResponse::Clip(ClipInfo {
                duration: 12.5,
                output_duration: 9.75,
                zooms: vec![ZoomSpan {
                    start: 1.0,
                    end: 3.5,
                    amount: 1.8,
                    focus: Some((0.3, 0.6)),
                    mode: "rect".into(),
                    rect: Some((0.2, 0.5, 0.2, 0.2)),
                }],
                cuts: vec![CutSpan {
                    start: 5.0,
                    end: 6.0,
                }],
                speeds: vec![SpeedSpan {
                    start: 7.0,
                    end: 9.0,
                    factor: 4.0,
                }],
                camera: vec![(0.0, 0.5, 0.5, 1.0), (0.25, 0.4, 0.55, 1.8)],
            }),
            ControlResponse::Status(StatusInfo {
                state: RecordState::Recording,
                elapsed: Some(4.2),
            }),
            ControlResponse::Shot(FrameShot {
                t: 0.0,
                width: 800,
                height: 450,
                png_base64: "AAAA".into(),
            }),
            ControlResponse::Frames {
                frames: vec![FrameShot {
                    t: 1.0,
                    width: 4,
                    height: 2,
                    png_base64: "AAAA".into(),
                }],
            },
            ControlResponse::Zooms {
                zooms: vec![ZoomSpan {
                    start: 0.5,
                    end: 2.0,
                    amount: 2.0,
                    focus: None,
                    mode: "auto".into(),
                    rect: None,
                }],
            },
            ControlResponse::Cuts { cuts: vec![] },
            ControlResponse::Speeds {
                speeds: vec![SpeedSpan {
                    start: 0.0,
                    end: 1.0,
                    factor: 2.0,
                }],
            },
            ControlResponse::Job { id: 42 },
            ControlResponse::Export(ExportState {
                done: 120,
                total: 400,
                finished: false,
                error: None,
                path: "C:/tmp/out.gif".into(),
            }),
            ControlResponse::Size { bytes: 1_234_567 },
        ];
        for resp in cases {
            let line = serde_json::to_string(&resp).expect("serialize");
            let back: ControlResponse = serde_json::from_str(&line).expect("deserialize");
            assert_eq!(resp, back);
        }
    }

    /// The endpoint file round-trips, and a missing/garbage file reads back as `None`.
    #[test]
    fn port_file_round_trips() {
        let dir = std::env::temp_dir();
        let path = dir.join(format!("vuoom-control-test-{}.json", std::process::id()));
        let _ = std::fs::remove_file(&path);
        assert_eq!(read_endpoint_at(&path), None);
        write_port_file_at(&path, 54321, "cafe").expect("write");
        assert_eq!(read_endpoint_at(&path), Some((54321, "cafe".to_string())));
        std::fs::write(&path, b"not json").expect("write garbage");
        assert_eq!(read_endpoint_at(&path), None);
        // A pre-token file (port only) still reads, with an empty token.
        std::fs::write(&path, br#"{"port": 1234}"#).expect("write old format");
        assert_eq!(read_endpoint_at(&path), Some((1234, String::new())));
        let _ = std::fs::remove_file(&path);
    }

    /// Tokens are 128-bit hex and unique across calls.
    #[test]
    fn tokens_are_hex_and_unique() {
        let a = generate_token();
        let b = generate_token();
        assert_eq!(a.len(), 32);
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(a, b, "two tokens must not collide");
    }

    /// `write_message` emits exactly one trailing newline and a parseable body.
    #[test]
    fn write_message_is_one_line() {
        let mut buf = Vec::new();
        write_message(&mut buf, &ControlRequest::Ping).expect("write");
        assert_eq!(buf.last(), Some(&b'\n'));
        let text = String::from_utf8(buf).expect("utf8");
        assert_eq!(text.matches('\n').count(), 1);
        let req: ControlRequest = serde_json::from_str(text.trim_end()).expect("parse");
        assert_eq!(req, ControlRequest::Ping);
    }
}
