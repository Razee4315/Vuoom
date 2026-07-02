//! Vuoom MCP server — the "AI Demo Director" sidecar.
//!
//! An AI agent (e.g. Claude) launches this binary over stdio. It exposes MCP **tools** that
//! drive Vuoom's localhost control server (see [`vuoom_control`]): see the screen, set the
//! region, start/stop recording, inject humanized mouse/keyboard into a target app, sample
//! frames the agent can *see*, repair the clip (zooms/cuts/speed) without re-recording, and
//! export a GIF/MP4 as a polled job. The agent itself is the director and the verify loop —
//! it drives, looks at the sampled frames, critiques, edits, and re-records only when the
//! content is wrong. See `docs/13-AI-Demo-Director-Research.md`.
//!
//! Vuoom must be running with `VUOOM_ENABLE_CONTROL=1` (the control server is opt-in for
//! safety, since these tools inject real input). The endpoint + auth token are discovered
//! via [`vuoom_control::discover_endpoint`].

use std::sync::{Arc, Mutex};

use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::*,
    schemars, tool, tool_handler, tool_router,
    transport::stdio,
    ErrorData as McpError, ServerHandler, ServiceExt,
};
use serde::Deserialize;
use vuoom_control::{Button, Client, ControlRequest, ControlResponse};

/// The most frames one `get_frames` call may sample — each is a vision-model image, so an
/// over-eager request would flood the agent's context.
const MAX_FRAMES_PER_CALL: usize = 16;
/// Default downscale width for returned frames/screenshots (px) when none is given.
const DEFAULT_FRAME_WIDTH: u32 = 800;
/// The longest a single `wait` may sleep; call it repeatedly for longer waits.
const MAX_WAIT_MS: u64 = 15_000;

/// A lazily-(re)connected bridge to Vuoom's control server. Blocking I/O; tools call it via
/// `spawn_blocking`. On a transport error the connection is dropped so the next call reconnects.
struct Bridge {
    conn: Mutex<Option<Client>>,
}

impl Bridge {
    fn new() -> Self {
        Self {
            conn: Mutex::new(None),
        }
    }

    fn call_blocking(&self, req: &ControlRequest) -> Result<ControlResponse, String> {
        let mut guard = self
            .conn
            .lock()
            .map_err(|_| "bridge lock poisoned".to_string())?;
        if guard.is_none() {
            let (port, token) = vuoom_control::discover_endpoint().ok_or_else(|| {
                "Vuoom control server not found — launch Vuoom with VUOOM_ENABLE_CONTROL=1"
                    .to_string()
            })?;
            let client = Client::connect(port, &token)
                .map_err(|e| format!("connect to Vuoom failed: {e} — is Vuoom still running?"))?;
            *guard = Some(client);
        }
        let client = guard.as_mut().expect("connection present");
        match client.call(req) {
            Ok(resp) => Ok(resp),
            Err(e) => {
                *guard = None; // force a reconnect next time
                Err(e)
            }
        }
    }
}

/// The MCP server: holds the tool router and the bridge to Vuoom.
#[derive(Clone)]
struct VuoomMcp {
    // Read at runtime by the `#[tool_handler]`-generated `call_tool`/`list_tools` (verified by a
    // tools/list smoke test); the dead-code lint can't see through the macro, so allow it.
    #[allow(dead_code)]
    tool_router: ToolRouter<Self>,
    bridge: Arc<Bridge>,
}

impl VuoomMcp {
    fn new() -> Self {
        Self {
            tool_router: Self::tool_router(),
            bridge: Arc::new(Bridge::new()),
        }
    }

    /// Run one control request on the blocking pool, mapping failures to MCP errors and a
    /// control-level `Error` response into an MCP error too (so the agent sees the reason).
    async fn call(&self, req: ControlRequest) -> Result<ControlResponse, McpError> {
        let bridge = Arc::clone(&self.bridge);
        let resp = tokio::task::spawn_blocking(move || bridge.call_blocking(&req))
            .await
            .map_err(|e| McpError::internal_error(format!("join error: {e}"), None))?
            .map_err(|e| McpError::internal_error(e, None))?;
        match resp {
            ControlResponse::Error { message } => Err(McpError::internal_error(message, None)),
            other => Ok(other),
        }
    }
}

/// `Content::text` for a plain string.
fn text(s: impl Into<String>) -> CallToolResult {
    CallToolResult::success(vec![Content::text(s.into())])
}

/// `Content::text` for any serializable payload, as compact JSON.
fn json(value: &impl serde::Serialize) -> CallToolResult {
    let body =
        serde_json::to_string(value).unwrap_or_else(|e| format!("{{\"serialize_error\":\"{e}\"}}"));
    text(body)
}

/// A protocol reply of the wrong shape is a server bug — surface it as an error, never as
/// tool output the agent might try to parse.
fn unexpected(other: &ControlResponse) -> McpError {
    McpError::internal_error(format!("unexpected control response: {other:?}"), None)
}

fn parse_button(s: Option<&str>) -> Button {
    match s.map(str::to_ascii_lowercase).as_deref() {
        Some("right") => Button::Right,
        Some("middle") => Button::Middle,
        _ => Button::Left,
    }
}

// ── Tool parameter structs ───────────────────────────────────────────────────────

/// Capture-region rectangle (physical px); omit fields for full screen.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct RegionParams {
    /// Left edge (physical px). Omit for full screen.
    x: Option<u32>,
    /// Top edge (physical px).
    y: Option<u32>,
    /// Width (physical px).
    w: Option<u32>,
    /// Height (physical px).
    h: Option<u32>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct ZoomParams {
    /// Zoom multiplier, 1.0 (off) to 4.0.
    amount: f64,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct ZoomStyleParams {
    /// Seconds of inactivity before the camera zooms back out (default 1.8).
    hold: Option<f64>,
    /// Seconds of anticipation before a click that the zoom-in begins (default 0.3).
    pre_roll: Option<f64>,
    /// Half-life (s) of the zoom spring — smaller = snappier (default 0.30).
    hl_zoom: Option<f64>,
    /// Half-life (s) of the pan spring — smaller = snappier follow (default 0.22).
    hl_pan: Option<f64>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct StartParams {
    /// Seed a cinematic zoom on every click you inject (default true — the agent drives via
    /// clicks). Set false for a static recording where you'll add zooms manually.
    auto_zoom_on_click: Option<bool>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct ScreenshotParams {
    /// Optional max width (px) to downscale the shot (default 800).
    width: Option<u32>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct MoveParams {
    /// X in virtual-desktop physical px.
    x: i32,
    /// Y in virtual-desktop physical px.
    y: i32,
    /// Glide duration in ms; omit for distance-scaled, 0 for an instant warp.
    duration_ms: Option<u32>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct ClickParams {
    /// X in virtual-desktop physical px.
    x: i32,
    /// Y in virtual-desktop physical px.
    y: i32,
    /// "left" (default), "right", or "middle".
    button: Option<String>,
    /// Double-click when true.
    double: Option<bool>,
    /// Glide duration in ms; omit for distance-scaled (recommended), 0 for instant.
    glide_ms: Option<u32>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct DragParams {
    /// Drag start x (physical px).
    x1: i32,
    /// Drag start y (physical px).
    y1: i32,
    /// Drag end x (physical px).
    x2: i32,
    /// Drag end y (physical px).
    y2: i32,
    /// "left" (default), "right", or "middle".
    button: Option<String>,
    /// Duration of the dragging portion in ms; omit for distance-scaled.
    duration_ms: Option<u32>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct TypeParams {
    /// The Unicode text to type into the focused control. \n and \t press real Enter/Tab.
    text: String,
    /// Typing speed in characters per second (default ~15, humanlike jitter).
    cps: Option<f64>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct KeyChordParams {
    /// Key names, modifiers first, e.g. ["ctrl","c"] or ["enter"]. Punctuation like "=",
    /// "-", "/" works too (so ["ctrl","="] zooms a browser).
    keys: Vec<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct ScrollParams {
    /// X in virtual-desktop physical px.
    x: i32,
    /// Y in virtual-desktop physical px.
    y: i32,
    /// Wheel notches; positive scrolls up.
    delta: i32,
    /// Gap between notches in ms (default 40; 0 = all at once).
    step_ms: Option<u32>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct PausedParams {
    /// True to pause, false to resume.
    paused: bool,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct SeekParams {
    /// Time from clip start, seconds (output timeline — what the export shows).
    t: f64,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct WaitParams {
    /// Milliseconds to wait (clamped to 15000 — call again for longer waits).
    ms: u64,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct GetFramesParams {
    /// Times (seconds, output timeline) to sample and return as PNG images — at most 16
    /// per call. Use clip_state's zoom spans to pick times worth seeing.
    times: Vec<f64>,
    /// Optional max width (px) to downscale each returned frame (default 800).
    width: Option<u32>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct AddZoomParams {
    /// Source-timeline start (seconds); the segment gets a ~2 s default length.
    t: f64,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct UpdateZoomParams {
    /// Index into clip_state's sorted zoom list.
    index: usize,
    /// New start (source seconds).
    start: f64,
    /// New end (source seconds).
    end: f64,
    /// New zoom multiplier (1.1–4.0).
    amount: f64,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct ZoomFocusParams {
    /// Index into clip_state's sorted zoom list.
    index: usize,
    /// Normalized focus x in [0,1]; omit BOTH x and y to follow the cursor again.
    x: Option<f64>,
    /// Normalized focus y in [0,1].
    y: Option<f64>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct IndexParams {
    /// Index into the relevant sorted list from clip_state.
    index: usize,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct SpanParams {
    /// Start (source seconds).
    start: f64,
    /// End (source seconds).
    end: f64,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct UpdateCutParams {
    /// Index into clip_state's sorted cut list.
    index: usize,
    /// New start (source seconds).
    start: f64,
    /// New end (source seconds).
    end: f64,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct AutoSpeedParams {
    /// Speed-up factor for idle stretches (1.5–16.0; 4.0 is a good default).
    factor: f64,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct ExportParams {
    /// Absolute output path.
    path: String,
    /// Output frame rate (e.g. 20 for a README GIF).
    fps: Option<u32>,
    /// Optional max width (px).
    width: Option<u32>,
    /// Quality 1–100 (default 80).
    quality: Option<u8>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct ExportStatusParams {
    /// The job id returned by export_gif / export_mp4.
    id: u64,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct EstimateParams {
    /// Output frame rate.
    fps: Option<u32>,
    /// Optional max width (px).
    width: Option<u32>,
    /// Quality 1–100 (default 80).
    quality: Option<u8>,
}

// ── Tools ────────────────────────────────────────────────────────────────────────

#[tool_router]
impl VuoomMcp {
    #[tool(description = "Check that the Vuoom control server is reachable.")]
    async fn ping(&self) -> Result<CallToolResult, McpError> {
        self.call(ControlRequest::Ping).await?;
        Ok(text("ok"))
    }

    #[tool(
        description = "Get the virtual-desktop bounds (x, y, width, height) in physical pixels. Use this first to reason about where to click."
    )]
    async fn screen_geometry(&self) -> Result<CallToolResult, McpError> {
        match self.call(ControlRequest::ScreenGeometry).await? {
            ControlResponse::Geometry(g) => Ok(json(&g)),
            other => Err(unexpected(&other)),
        }
    }

    #[tool(
        description = "See the screen RIGHT NOW (PNG of the recording monitor) — use before and between actions to locate what to click and to verify the UI reacted. Works whether or not a recording is running."
    )]
    async fn screenshot(
        &self,
        Parameters(p): Parameters<ScreenshotParams>,
    ) -> Result<CallToolResult, McpError> {
        match self
            .call(ControlRequest::Screenshot {
                width: Some(p.width.unwrap_or(DEFAULT_FRAME_WIDTH)),
            })
            .await?
        {
            ControlResponse::Shot(shot) => Ok(CallToolResult::success(vec![
                Content::text(format!("{}x{} px", shot.width, shot.height)),
                Content::image(shot.png_base64, "image/png".to_string()),
            ])),
            other => Err(unexpected(&other)),
        }
    }

    #[tool(
        description = "What is the engine doing? Returns {state: idle|recording|paused|clip_ready, elapsed}. Use to recover when unsure whether a recording is running."
    )]
    async fn status(&self) -> Result<CallToolResult, McpError> {
        match self.call(ControlRequest::Status).await? {
            ControlResponse::Status(s) => Ok(json(&s)),
            other => Err(unexpected(&other)),
        }
    }

    #[tool(
        description = "Set the capture region (physical px) for the NEXT recording. Omit x/y/w/h for full screen. Call before start_recording."
    )]
    async fn set_region(
        &self,
        Parameters(p): Parameters<RegionParams>,
    ) -> Result<CallToolResult, McpError> {
        self.call(ControlRequest::SetRegion {
            x: p.x,
            y: p.y,
            w: p.w,
            h: p.h,
        })
        .await?;
        Ok(text("region set"))
    }

    #[tool(
        description = "Set the auto-zoom multiplier (1.0 = off, up to 4.0) for the NEXT recording. Call before start_recording."
    )]
    async fn set_zoom_amount(
        &self,
        Parameters(p): Parameters<ZoomParams>,
    ) -> Result<CallToolResult, McpError> {
        self.call(ControlRequest::SetZoomAmount { amount: p.amount })
            .await?;
        Ok(text("zoom amount set"))
    }

    #[tool(
        description = "Tune the auto-zoom FEEL for the NEXT recording (hold, pre_roll, spring half-lives). Omitted fields keep defaults; resets after each recording."
    )]
    async fn set_zoom_style(
        &self,
        Parameters(p): Parameters<ZoomStyleParams>,
    ) -> Result<CallToolResult, McpError> {
        self.call(ControlRequest::SetZoomStyle {
            hold: p.hold,
            pre_roll: p.pre_roll,
            hl_zoom: p.hl_zoom,
            hl_pan: p.hl_pan,
        })
        .await?;
        Ok(text("zoom style set"))
    }

    #[tool(
        description = "Start recording the screen + global input. By default every click you inject seeds a cinematic auto-zoom; pass auto_zoom_on_click=false for a static recording."
    )]
    async fn start_recording(
        &self,
        Parameters(p): Parameters<StartParams>,
    ) -> Result<CallToolResult, McpError> {
        self.call(ControlRequest::StartRecording {
            auto_zoom_on_click: p.auto_zoom_on_click,
        })
        .await?;
        Ok(text("recording started"))
    }

    #[tool(
        description = "Stop recording and build the editable clip. Returns {duration, frames, zooms}. Wait ~2500 ms after the last action first, so the final zoom-out completes."
    )]
    async fn stop_recording(&self) -> Result<CallToolResult, McpError> {
        match self.call(ControlRequest::StopRecording).await? {
            ControlResponse::Recording(s) => Ok(json(&s)),
            other => Err(unexpected(&other)),
        }
    }

    #[tool(
        description = "Stop recording and DISCARD the take (no clip is built). The cheap way to abandon a botched take before re-recording."
    )]
    async fn cancel_recording(&self) -> Result<CallToolResult, McpError> {
        self.call(ControlRequest::CancelRecording).await?;
        Ok(text("recording cancelled and discarded"))
    }

    #[tool(description = "Pause or resume the running recording (a paused span becomes a cut).")]
    async fn set_paused(
        &self,
        Parameters(p): Parameters<PausedParams>,
    ) -> Result<CallToolResult, McpError> {
        self.call(ControlRequest::SetPaused { paused: p.paused })
            .await?;
        Ok(text("ok"))
    }

    #[tool(
        description = "Glide the cursor to (x, y) in physical px without clicking — a smooth, humanlike path (the cursor is visible in the recording). Coordinates are virtual-desktop pixels (see screen_geometry)."
    )]
    async fn move_cursor(
        &self,
        Parameters(p): Parameters<MoveParams>,
    ) -> Result<CallToolResult, McpError> {
        self.call(ControlRequest::MoveCursor {
            x: p.x,
            y: p.y,
            duration_ms: p.duration_ms,
        })
        .await?;
        Ok(text("moved"))
    }

    #[tool(
        description = "Click at (x, y) in physical px: the cursor GLIDES there smoothly, settles, then clicks — driving the target app AND triggering cinematic auto-zoom. button: left|right|middle; double: true for double-click."
    )]
    async fn click(
        &self,
        Parameters(p): Parameters<ClickParams>,
    ) -> Result<CallToolResult, McpError> {
        self.call(ControlRequest::Click {
            x: p.x,
            y: p.y,
            button: parse_button(p.button.as_deref()),
            double: p.double.unwrap_or(false),
            glide_ms: p.glide_ms,
        })
        .await?;
        Ok(text("clicked"))
    }

    #[tool(
        description = "Press-drag from (x1,y1) to (x2,y2): glide there, hold the button, drag along a smooth path, release. For sliders, drag-and-drop, text selection."
    )]
    async fn drag(
        &self,
        Parameters(p): Parameters<DragParams>,
    ) -> Result<CallToolResult, McpError> {
        self.call(ControlRequest::Drag {
            x1: p.x1,
            y1: p.y1,
            x2: p.x2,
            y2: p.y2,
            button: parse_button(p.button.as_deref()),
            duration_ms: p.duration_ms,
        })
        .await?;
        Ok(text("dragged"))
    }

    #[tool(
        description = "Type Unicode text into the currently focused control at a human cadence (default ~15 chars/sec). \\n and \\t press real Enter/Tab."
    )]
    async fn type_text(
        &self,
        Parameters(p): Parameters<TypeParams>,
    ) -> Result<CallToolResult, McpError> {
        self.call(ControlRequest::TypeText {
            text: p.text,
            cps: p.cps,
        })
        .await?;
        Ok(text("typed"))
    }

    #[tool(
        description = "Press a key chord, e.g. keys=[\"ctrl\",\"c\"], [\"enter\"], or [\"ctrl\",\"=\"]. Modifiers first."
    )]
    async fn key_chord(
        &self,
        Parameters(p): Parameters<KeyChordParams>,
    ) -> Result<CallToolResult, McpError> {
        self.call(ControlRequest::KeyChord { keys: p.keys }).await?;
        Ok(text("pressed"))
    }

    #[tool(
        description = "Scroll the wheel at (x, y), one notch per step so the motion records smoothly; positive delta scrolls up."
    )]
    async fn scroll(
        &self,
        Parameters(p): Parameters<ScrollParams>,
    ) -> Result<CallToolResult, McpError> {
        self.call(ControlRequest::Scroll {
            x: p.x,
            y: p.y,
            delta: p.delta,
            step_ms: p.step_ms,
        })
        .await?;
        Ok(text("scrolled"))
    }

    #[tool(
        description = "Wait N milliseconds (max 15000 per call) — use between actions so the target app can settle, and wait ~2500 ms before stop_recording so the final zoom-out completes."
    )]
    async fn wait(
        &self,
        Parameters(p): Parameters<WaitParams>,
    ) -> Result<CallToolResult, McpError> {
        let ms = p.ms.min(MAX_WAIT_MS);
        tokio::time::sleep(std::time::Duration::from_millis(ms)).await;
        Ok(text(format!("waited {ms} ms")))
    }

    #[tool(
        description = "Composite and publish the preview frame at time t (seconds, output timeline)."
    )]
    async fn seek(
        &self,
        Parameters(p): Parameters<SeekParams>,
    ) -> Result<CallToolResult, McpError> {
        self.call(ControlRequest::Seek { t: p.t }).await?;
        Ok(text("ok"))
    }

    #[tool(
        description = "The clip's full editable state: {duration, output_duration, zooms: [{start,end,amount,focus}], cuts, speeds}. Zoom/cut indices here are what the edit tools address. Call after stop_recording to know WHERE to sample frames."
    )]
    async fn clip_state(&self) -> Result<CallToolResult, McpError> {
        match self.call(ControlRequest::ClipState).await? {
            ControlResponse::Clip(c) => Ok(json(&c)),
            other => Err(unexpected(&other)),
        }
    }

    #[tool(
        description = "Sample the clip at the given OUTPUT-timeline times (seconds; max 16) and return PNG images — exactly what the export will show, so you can critique zoom centring, legibility, and timing. Sample around each zoom span from clip_state."
    )]
    async fn get_frames(
        &self,
        Parameters(p): Parameters<GetFramesParams>,
    ) -> Result<CallToolResult, McpError> {
        if p.times.is_empty() {
            return Err(McpError::invalid_params("times must not be empty", None));
        }
        if p.times.len() > MAX_FRAMES_PER_CALL {
            return Err(McpError::invalid_params(
                format!(
                    "asked for {} frames; max {MAX_FRAMES_PER_CALL} per call — sample sparsely",
                    p.times.len()
                ),
                None,
            ));
        }
        match self
            .call(ControlRequest::GetFrames {
                times: p.times,
                width: Some(p.width.unwrap_or(DEFAULT_FRAME_WIDTH)),
            })
            .await?
        {
            ControlResponse::Frames { frames } => {
                let mut content = vec![Content::text(format!("{} frame(s)", frames.len()))];
                for f in frames {
                    content.push(Content::text(format!("t={:.3}s", f.t)));
                    content.push(Content::image(f.png_base64, "image/png".to_string()));
                }
                Ok(CallToolResult::success(content))
            }
            other => Err(unexpected(&other)),
        }
    }

    #[tool(
        description = "Insert a zoom segment at source time t (~2 s default length, follows the cursor). Returns the updated zoom list."
    )]
    async fn add_zoom(
        &self,
        Parameters(p): Parameters<AddZoomParams>,
    ) -> Result<CallToolResult, McpError> {
        match self.call(ControlRequest::AddZoom { t: p.t }).await? {
            ControlResponse::Zooms { zooms } => Ok(json(&zooms)),
            other => Err(unexpected(&other)),
        }
    }

    #[tool(
        description = "Retime / re-level the zoom at `index` (from clip_state). Fixes 'zoom leaves too early' or 'too strong'. Returns the updated zoom list."
    )]
    async fn update_zoom(
        &self,
        Parameters(p): Parameters<UpdateZoomParams>,
    ) -> Result<CallToolResult, McpError> {
        match self
            .call(ControlRequest::UpdateZoom {
                index: p.index,
                start: p.start,
                end: p.end,
                amount: p.amount,
            })
            .await?
        {
            ControlResponse::Zooms { zooms } => Ok(json(&zooms)),
            other => Err(unexpected(&other)),
        }
    }

    #[tool(
        description = "Re-centre the zoom at `index`: pass normalized x,y in [0,1] to hold a fixed focus (fixes off-centre zooms), or omit both to follow the cursor again. Returns the updated zoom list."
    )]
    async fn set_zoom_focus(
        &self,
        Parameters(p): Parameters<ZoomFocusParams>,
    ) -> Result<CallToolResult, McpError> {
        match self
            .call(ControlRequest::SetZoomFocus {
                index: p.index,
                x: p.x,
                y: p.y,
            })
            .await?
        {
            ControlResponse::Zooms { zooms } => Ok(json(&zooms)),
            other => Err(unexpected(&other)),
        }
    }

    #[tool(
        description = "Delete the zoom at `index` (from clip_state) — e.g. a zoom on a misclick. Returns the updated zoom list."
    )]
    async fn remove_zoom(
        &self,
        Parameters(p): Parameters<IndexParams>,
    ) -> Result<CallToolResult, McpError> {
        match self
            .call(ControlRequest::RemoveZoom { index: p.index })
            .await?
        {
            ControlResponse::Zooms { zooms } => Ok(json(&zooms)),
            other => Err(unexpected(&other)),
        }
    }

    #[tool(
        description = "Remove [start, end] (source seconds) from the output — dead air, loading spinners, mistakes. Returns the updated cut list."
    )]
    async fn add_cut(
        &self,
        Parameters(p): Parameters<SpanParams>,
    ) -> Result<CallToolResult, McpError> {
        match self
            .call(ControlRequest::AddCut {
                start: p.start,
                end: p.end,
            })
            .await?
        {
            ControlResponse::Cuts { cuts } => Ok(json(&cuts)),
            other => Err(unexpected(&other)),
        }
    }

    #[tool(
        description = "Retime the cut at `index` (from clip_state). Returns the updated cut list."
    )]
    async fn update_cut(
        &self,
        Parameters(p): Parameters<UpdateCutParams>,
    ) -> Result<CallToolResult, McpError> {
        match self
            .call(ControlRequest::UpdateCut {
                index: p.index,
                start: p.start,
                end: p.end,
            })
            .await?
        {
            ControlResponse::Cuts { cuts } => Ok(json(&cuts)),
            other => Err(unexpected(&other)),
        }
    }

    #[tool(
        description = "Restore the cut at `index` (the section plays again). Returns the updated cut list."
    )]
    async fn remove_cut(
        &self,
        Parameters(p): Parameters<IndexParams>,
    ) -> Result<CallToolResult, McpError> {
        match self
            .call(ControlRequest::RemoveCut { index: p.index })
            .await?
        {
            ControlResponse::Cuts { cuts } => Ok(json(&cuts)),
            other => Err(unexpected(&other)),
        }
    }

    #[tool(
        description = "Auto-detect idle stretches and speed them up by `factor` (replaces existing speed regions) — the one-call fix for 'the demo drags'. Returns the new speed regions."
    )]
    async fn auto_speed(
        &self,
        Parameters(p): Parameters<AutoSpeedParams>,
    ) -> Result<CallToolResult, McpError> {
        match self
            .call(ControlRequest::AutoSpeed { factor: p.factor })
            .await?
        {
            ControlResponse::Speeds { speeds } => Ok(json(&speeds)),
            other => Err(unexpected(&other)),
        }
    }

    #[tool(description = "Remove all speed regions (play everything at 1×).")]
    async fn clear_speed(&self) -> Result<CallToolResult, McpError> {
        self.call(ControlRequest::ClearSpeed).await?;
        Ok(text("speed regions cleared"))
    }

    #[tool(
        description = "Trim the clip to [start, end] (source seconds) — drop an awkward first/last second without re-recording."
    )]
    async fn set_trim(
        &self,
        Parameters(p): Parameters<SpanParams>,
    ) -> Result<CallToolResult, McpError> {
        self.call(ControlRequest::SetTrim {
            start: p.start,
            end: p.end,
        })
        .await?;
        Ok(text("trim set"))
    }

    #[tool(description = "Estimate the GIF export size in bytes for the given settings.")]
    async fn estimate_gif(
        &self,
        Parameters(p): Parameters<EstimateParams>,
    ) -> Result<CallToolResult, McpError> {
        match self
            .call(ControlRequest::EstimateGif {
                fps: p.fps.unwrap_or(20),
                width: p.width,
                quality: p.quality.unwrap_or(80),
            })
            .await?
        {
            ControlResponse::Size { bytes } => Ok(text(bytes.to_string())),
            other => Err(unexpected(&other)),
        }
    }

    #[tool(
        description = "Start exporting the edited clip to an animated GIF (absolute path). Returns a job id IMMEDIATELY — poll export_status until finished:true."
    )]
    async fn export_gif(
        &self,
        Parameters(p): Parameters<ExportParams>,
    ) -> Result<CallToolResult, McpError> {
        match self
            .call(ControlRequest::ExportGif {
                path: p.path.clone(),
                fps: p.fps.unwrap_or(20),
                width: p.width,
                quality: p.quality.unwrap_or(80),
            })
            .await?
        {
            ControlResponse::Job { id } => Ok(text(format!(
                "export started: job {id} -> {} — poll export_status until finished",
                p.path
            ))),
            other => Err(unexpected(&other)),
        }
    }

    #[tool(
        description = "Start exporting the edited clip to an H.264 MP4 (absolute path). Returns a job id IMMEDIATELY — poll export_status until finished:true."
    )]
    async fn export_mp4(
        &self,
        Parameters(p): Parameters<ExportParams>,
    ) -> Result<CallToolResult, McpError> {
        match self
            .call(ControlRequest::ExportMp4 {
                path: p.path.clone(),
                fps: p.fps.unwrap_or(30),
                width: p.width,
                quality: p.quality.unwrap_or(80),
            })
            .await?
        {
            ControlResponse::Job { id } => Ok(text(format!(
                "export started: job {id} -> {} — poll export_status until finished",
                p.path
            ))),
            other => Err(unexpected(&other)),
        }
    }

    #[tool(
        description = "Poll an export job: {done, total, finished, error, path}. Wait ~1-2 s between polls; the file is complete when finished:true and error is null."
    )]
    async fn export_status(
        &self,
        Parameters(p): Parameters<ExportStatusParams>,
    ) -> Result<CallToolResult, McpError> {
        match self.call(ControlRequest::ExportStatus { id: p.id }).await? {
            ControlResponse::Export(state) => Ok(json(&state)),
            other => Err(unexpected(&other)),
        }
    }
}

/// Guidance shown to the agent on connect.
const INSTRUCTIONS: &str = "Vuoom AI Demo Director — record cinematic, auto-zoomed demo \
    GIFs/MP4s of a Windows app you drive. Vuoom must run with VUOOM_ENABLE_CONTROL=1.\n\
    Workflow: (1) screen_geometry, then screenshot to SEE the screen and locate targets. \
    (2) set_region / set_zoom_amount (2.0 is a good default). (3) start_recording. \
    (4) Drive the target: click / type_text / key_chord / scroll / drag — the cursor glides \
    smoothly and clicks trigger cinematic auto-zoom. Rhythm: wait 800–1500 ms after each \
    action so the UI reacts on camera; screenshot mid-take if unsure what state the app is \
    in; finish with wait 2500 so the final zoom-out completes. (5) stop_recording (or \
    cancel_recording to discard a botched take). (6) clip_state for the zoom spans, then \
    get_frames at times around each span to SEE the result; critique centring, legibility, \
    dead air. (7) REPAIR in place — update_zoom / set_zoom_focus / remove_zoom / add_cut / \
    auto_speed / set_trim; re-record only when the on-screen content itself is wrong, and \
    cap the loop at 3–4 takes. (8) estimate_gif, then export_gif / export_mp4 and poll \
    export_status until finished.";

#[tool_handler]
impl ServerHandler for VuoomMcp {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::from_build_env())
            .with_instructions(INSTRUCTIONS.to_string())
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let service = VuoomMcp::new().serve(stdio()).await?;
    service.waiting().await?;
    Ok(())
}
