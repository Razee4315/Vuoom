//! Vuoom MCP server — the "AI Demo Director" sidecar.
//!
//! An AI agent (e.g. Claude) launches this binary over stdio. It exposes MCP **tools** that
//! drive Vuoom's localhost control server (see [`vuoom_control`]): set the region, start/stop
//! recording, inject mouse/keyboard into a target app, sample frames the agent can *see*, and
//! export a GIF/MP4. The agent itself is the director and the verify loop — it drives, looks
//! at the sampled frames, critiques, and re-records. See `docs/13-AI-Demo-Director-Research.md`.
//!
//! Vuoom must be running with `VUOOM_ENABLE_CONTROL=1` (the control server is opt-in for
//! safety, since these tools inject real input). The port is discovered via
//! [`vuoom_control::discover_port`].

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
            let port = vuoom_control::discover_port().ok_or_else(|| {
                "Vuoom control server not found — launch Vuoom with VUOOM_ENABLE_CONTROL=1"
                    .to_string()
            })?;
            let client =
                Client::connect(port).map_err(|e| format!("connect to Vuoom failed: {e}"))?;
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
struct StartParams {
    /// Seed a cinematic zoom on every click you inject (default true — the agent drives via
    /// clicks). Set false for a static recording where you'll add zooms manually.
    auto_zoom_on_click: Option<bool>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct PointParams {
    /// X in virtual-desktop physical px.
    x: i32,
    /// Y in virtual-desktop physical px.
    y: i32,
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
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct TypeParams {
    /// The Unicode text to type into the focused control.
    text: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct KeyChordParams {
    /// Key names, modifiers first, e.g. ["ctrl","c"] or ["enter"].
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
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct PausedParams {
    /// True to pause, false to resume.
    paused: bool,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct SeekParams {
    /// Time from clip start, seconds.
    t: f64,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct WaitParams {
    /// Milliseconds to wait (e.g. to let the target app settle before the next action).
    ms: u64,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct GetFramesParams {
    /// Times (seconds) to sample and return as PNG images for inspection.
    times: Vec<f64>,
    /// Optional max width (px) to downscale each returned frame (keeps the payload small).
    width: Option<u32>,
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
            other => Ok(text(format!("unexpected: {other:?}"))),
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
        description = "Start recording the screen + global input. By default every click you inject seeds a cinematic auto-zoom; pass auto_zoom_on_click=false for a static recording."
    )]
    async fn start_recording(
        &self,
        Parameters(p): Parameters<StartParams>,
    ) -> Result<CallToolResult, McpError> {
        self.call(ControlRequest::SetAutoZoomOnClick {
            on: p.auto_zoom_on_click.unwrap_or(true),
        })
        .await?;
        self.call(ControlRequest::StartRecording).await?;
        Ok(text("recording started"))
    }

    #[tool(
        description = "Stop recording and build the editable clip. Returns {duration, frames, zooms}."
    )]
    async fn stop_recording(&self) -> Result<CallToolResult, McpError> {
        match self.call(ControlRequest::StopRecording).await? {
            ControlResponse::Recording(s) => Ok(json(&s)),
            other => Ok(text(format!("unexpected: {other:?}"))),
        }
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
        description = "Move the cursor to (x, y) in physical px without clicking. Coordinates are virtual-desktop pixels (see screen_geometry)."
    )]
    async fn move_cursor(
        &self,
        Parameters(p): Parameters<PointParams>,
    ) -> Result<CallToolResult, McpError> {
        self.call(ControlRequest::MoveCursor { x: p.x, y: p.y })
            .await?;
        Ok(text("moved"))
    }

    #[tool(
        description = "Click at (x, y) in physical px. Drives the target app AND triggers cinematic auto-zoom. button: left|right|middle; double: true for double-click."
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
        })
        .await?;
        Ok(text("clicked"))
    }

    #[tool(description = "Type Unicode text into the currently focused control.")]
    async fn type_text(
        &self,
        Parameters(p): Parameters<TypeParams>,
    ) -> Result<CallToolResult, McpError> {
        self.call(ControlRequest::TypeText { text: p.text }).await?;
        Ok(text("typed"))
    }

    #[tool(
        description = "Press a key chord, e.g. keys=[\"ctrl\",\"c\"] or [\"enter\"]. Modifiers first."
    )]
    async fn key_chord(
        &self,
        Parameters(p): Parameters<KeyChordParams>,
    ) -> Result<CallToolResult, McpError> {
        self.call(ControlRequest::KeyChord { keys: p.keys }).await?;
        Ok(text("pressed"))
    }

    #[tool(description = "Scroll the wheel at (x, y); positive delta scrolls up.")]
    async fn scroll(
        &self,
        Parameters(p): Parameters<ScrollParams>,
    ) -> Result<CallToolResult, McpError> {
        self.call(ControlRequest::Scroll {
            x: p.x,
            y: p.y,
            delta: p.delta,
        })
        .await?;
        Ok(text("scrolled"))
    }

    #[tool(
        description = "Wait N milliseconds — use between actions to let the target app settle (prefer this over assuming instant UI updates)."
    )]
    async fn wait(
        &self,
        Parameters(p): Parameters<WaitParams>,
    ) -> Result<CallToolResult, McpError> {
        tokio::time::sleep(std::time::Duration::from_millis(p.ms)).await;
        Ok(text("waited"))
    }

    #[tool(description = "Composite and publish the preview frame at time t (seconds).")]
    async fn seek(
        &self,
        Parameters(p): Parameters<SeekParams>,
    ) -> Result<CallToolResult, McpError> {
        self.call(ControlRequest::Seek { t: p.t }).await?;
        Ok(text("ok"))
    }

    #[tool(
        description = "Get a summary of the current clip: {duration, zooms, cuts, speed_regions}."
    )]
    async fn clip_state(&self) -> Result<CallToolResult, McpError> {
        match self.call(ControlRequest::ClipState).await? {
            ControlResponse::Clip(c) => Ok(json(&c)),
            other => Ok(text(format!("unexpected: {other:?}"))),
        }
    }

    #[tool(
        description = "Sample the clip at the given times (seconds) and return PNG images so you can SEE the output and critique it (zoom centring, legibility, timing). Sample sparsely, e.g. around each zoom."
    )]
    async fn get_frames(
        &self,
        Parameters(p): Parameters<GetFramesParams>,
    ) -> Result<CallToolResult, McpError> {
        match self
            .call(ControlRequest::GetFrames {
                times: p.times,
                width: p.width,
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
            other => Ok(text(format!("unexpected: {other:?}"))),
        }
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
            other => Ok(text(format!("unexpected: {other:?}"))),
        }
    }

    #[tool(description = "Export the edited clip to an animated GIF at the given absolute path.")]
    async fn export_gif(
        &self,
        Parameters(p): Parameters<ExportParams>,
    ) -> Result<CallToolResult, McpError> {
        self.call(ControlRequest::ExportGif {
            path: p.path.clone(),
            fps: p.fps.unwrap_or(20),
            width: p.width,
            quality: p.quality.unwrap_or(80),
        })
        .await?;
        Ok(text(format!("exported GIF to {}", p.path)))
    }

    #[tool(description = "Export the edited clip to an H.264 MP4 at the given absolute path.")]
    async fn export_mp4(
        &self,
        Parameters(p): Parameters<ExportParams>,
    ) -> Result<CallToolResult, McpError> {
        self.call(ControlRequest::ExportMp4 {
            path: p.path.clone(),
            fps: p.fps.unwrap_or(30),
            width: p.width,
            quality: p.quality.unwrap_or(80),
        })
        .await?;
        Ok(text(format!("exported MP4 to {}", p.path)))
    }
}

/// Guidance shown to the agent on connect.
const INSTRUCTIONS: &str = "Vuoom AI Demo Director. Workflow: (1) screen_geometry, \
    (2) set_region/set_zoom_amount, (3) start_recording, (4) drive the target app with \
    click/type/key_chord/scroll (clicks also trigger cinematic auto-zoom) using wait between \
    steps, (5) stop_recording, (6) get_frames to SEE the result and critique it, re-recording \
    if needed, (7) export_gif/export_mp4. Vuoom must run with VUOOM_ENABLE_CONTROL=1.";

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
