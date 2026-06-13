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

#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use std::io::{self, BufRead, BufReader, Write};
use std::net::TcpStream;

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
/// to learn the desktop bounds first.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum ControlRequest {
    /// Liveness check — the server answers [`ControlResponse::Ok`].
    Ping,
    /// Ask for the virtual-desktop bounds (so the agent can reason in screen pixels).
    ScreenGeometry,
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
    /// Begin capturing the screen + global input.
    StartRecording,
    /// Stop capturing and build the editable project; answered with [`ControlResponse::Recording`].
    StopRecording,
    /// Pause or resume the running recording (a paused span becomes a cut).
    SetPaused {
        /// `true` to pause, `false` to resume.
        paused: bool,
    },
    /// Move the cursor to `(x, y)` without clicking.
    MoveCursor {
        /// Target x (physical px).
        x: i32,
        /// Target y (physical px).
        y: i32,
    },
    /// Click at `(x, y)`. These synthetic clicks flow through Vuoom's input hook exactly like
    /// real ones, so they drive both the target app and the cinematic auto-zoom.
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
    },
    /// Type a string of Unicode text into the focused control.
    TypeText {
        /// The text to type.
        text: String,
    },
    /// Press a key chord, e.g. `["ctrl", "c"]` or `["enter"]`. Modifiers are held while the
    /// final key is tapped, then released in reverse order.
    KeyChord {
        /// Key names (modifiers first); see [`crate::key`] for accepted names.
        keys: Vec<String>,
    },
    /// Scroll the wheel at `(x, y)`; positive `delta` scrolls up.
    Scroll {
        /// Pointer x (physical px).
        x: i32,
        /// Pointer y (physical px).
        y: i32,
        /// Wheel delta in notches (positive = up).
        delta: i32,
    },
    /// Composite and publish the preview frame at time `t` (seconds).
    Seek {
        /// Time from the clip start, in seconds.
        t: f64,
    },
    /// Ask for a summary of the current clip (duration, zoom/cut counts).
    ClipState,
    /// Composite the given times and return them as PNGs so the agent can *see* the result
    /// and critique it. Sample sparsely (e.g. around each zoom) to keep cost down.
    GetFrames {
        /// Times (seconds) to sample.
        times: Vec<f64>,
        /// Optional max width (px) to downscale each returned frame.
        width: Option<u32>,
    },
    /// Export the edited clip to an animated GIF.
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
    /// Export the edited clip to an H.264 MP4.
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
    /// Clip length in seconds.
    pub duration: f64,
    /// Number of captured frames.
    pub frames: usize,
    /// Number of auto-planned zoom segments.
    pub zooms: usize,
}

/// A compact view of the current clip's editable state.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ClipInfo {
    /// Clip length in seconds.
    pub duration: f64,
    /// Number of zoom segments.
    pub zooms: usize,
    /// Number of cuts.
    pub cuts: usize,
    /// Number of speed regions.
    pub speed_regions: usize,
}

/// A single composited frame, returned for the agent to inspect.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FrameShot {
    /// The time (seconds) this frame was sampled at.
    pub t: f64,
    /// Frame width in pixels.
    pub width: u32,
    /// Frame height in pixels.
    pub height: u32,
    /// The frame encoded as a base64 PNG (no data-URL prefix).
    pub png_base64: String,
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
    /// Reply to [`ControlRequest::GetFrames`].
    Frames {
        /// The sampled frames, in request order.
        frames: Vec<FrameShot>,
    },
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
    /// Connect to the control server on `127.0.0.1:port`.
    ///
    /// # Errors
    /// Returns an [`io::Error`] if the connection cannot be established.
    pub fn connect(port: u16) -> io::Result<Self> {
        let stream = TcpStream::connect(("127.0.0.1", port))?;
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
            return Err("control server closed the connection".into());
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
            ControlRequest::StartRecording,
            ControlRequest::StopRecording,
            ControlRequest::SetPaused { paused: true },
            ControlRequest::MoveCursor { x: -5, y: 100 },
            ControlRequest::Click {
                x: 1,
                y: 2,
                button: Button::Right,
                double: true,
            },
            ControlRequest::TypeText {
                text: "hello world".into(),
            },
            ControlRequest::KeyChord {
                keys: vec!["ctrl".into(), "c".into()],
            },
            ControlRequest::Scroll {
                x: 3,
                y: 4,
                delta: -2,
            },
            ControlRequest::Seek { t: 1.5 },
            ControlRequest::ClipState,
            ControlRequest::GetFrames {
                times: vec![0.0, 1.0, 2.5],
                width: Some(800),
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
                zooms: 3,
                cuts: 1,
                speed_regions: 2,
            }),
            ControlResponse::Frames {
                frames: vec![FrameShot {
                    t: 1.0,
                    width: 4,
                    height: 2,
                    png_base64: "AAAA".into(),
                }],
            },
            ControlResponse::Size { bytes: 1_234_567 },
        ];
        for resp in cases {
            let line = serde_json::to_string(&resp).expect("serialize");
            let back: ControlResponse = serde_json::from_str(&line).expect("deserialize");
            assert_eq!(resp, back);
        }
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
