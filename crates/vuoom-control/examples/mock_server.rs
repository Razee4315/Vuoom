//! A mock Vuoom control server for testing the `vuoom-mcp` sidecar without the full app.
//!
//! It speaks the real [`vuoom_control`] protocol over a `127.0.0.1` port and writes the
//! discovery file (port + auth token), so the sidecar connects to it exactly as it would to
//! Vuoom — but it returns canned responses instead of recording. Lets the whole agent → MCP
//! → protocol path be exercised on any machine (no GPU/capture).
//! Run: `cargo run -p vuoom-control --example mock_server`.

use std::io::{BufRead, BufReader};
use std::net::{TcpListener, TcpStream};

use vuoom_control::{
    write_message, ClipInfo, ControlRequest, ControlResponse, CutSpan, ExportState, FrameShot,
    PreviewInfo, RecordState, RecordingSummary, RegionRect, ScreenGeometry, SpeedSpan, StatusInfo,
    WindowRect, ZoomSpan,
};

/// A 1×1 transparent PNG (base64) — stands in for a sampled frame.
const TINY_PNG: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==";

fn main() -> std::io::Result<()> {
    let listener = TcpListener::bind(("127.0.0.1", 0))?;
    let port = listener.local_addr()?.port();
    let token = vuoom_control::generate_token();
    vuoom_control::write_port_file(port, &token)?;
    eprintln!("mock control server: listening on 127.0.0.1:{port}");
    for stream in listener.incoming() {
        match stream {
            Ok(s) => serve(s, &token),
            Err(e) => eprintln!("mock: accept failed: {e}"),
        }
    }
    Ok(())
}

fn serve(stream: TcpStream, token: &str) {
    let Ok(mut writer) = stream.try_clone() else {
        return;
    };
    let mut reader = BufReader::new(stream);
    // First line must be the auth token, exactly like the real server.
    let mut auth = String::new();
    if reader.read_line(&mut auth).is_err() || auth.trim() != token {
        eprintln!("mock: rejected connection (bad token)");
        return;
    }
    for line in reader.lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        let resp = match serde_json::from_str::<ControlRequest>(&line) {
            Ok(req) => respond(req),
            Err(e) => ControlResponse::error(format!("bad request: {e}")),
        };
        if write_message(&mut writer, &resp).is_err() {
            break;
        }
    }
}

fn mock_zooms() -> Vec<ZoomSpan> {
    vec![
        ZoomSpan {
            start: 0.5,
            end: 2.0,
            amount: 1.8,
            focus: None,
            mode: "auto".into(),
            rect: None,
        },
        ZoomSpan {
            start: 4.0,
            end: 6.0,
            amount: 1.8,
            focus: Some((0.3, 0.6)),
            mode: "manual".into(),
            rect: None,
        },
    ]
}

/// Canned replies covering every request variant.
fn respond(req: ControlRequest) -> ControlResponse {
    match req {
        ControlRequest::Ping
        | ControlRequest::SetRegion { .. }
        | ControlRequest::SetZoomAmount { .. }
        | ControlRequest::SetZoomStyle { .. }
        | ControlRequest::StartRecording { .. }
        | ControlRequest::CancelRecording
        | ControlRequest::SetPaused { .. }
        | ControlRequest::MoveCursor { .. }
        | ControlRequest::Click { .. }
        | ControlRequest::Drag { .. }
        | ControlRequest::TypeText { .. }
        | ControlRequest::KeyChord { .. }
        | ControlRequest::Scroll { .. }
        | ControlRequest::Seek { .. }
        | ControlRequest::ClearSpeed
        | ControlRequest::SetTrim { .. } => ControlResponse::Ok,
        ControlRequest::ScreenGeometry => ControlResponse::Geometry(ScreenGeometry {
            x: 0,
            y: 0,
            width: 1920,
            height: 1080,
        }),
        ControlRequest::Screenshot { .. } => ControlResponse::Shot(FrameShot {
            t: 0.0,
            width: 1,
            height: 1,
            png_base64: TINY_PNG.to_string(),
        }),
        ControlRequest::Status => ControlResponse::Status(StatusInfo {
            state: RecordState::Idle,
            elapsed: None,
        }),
        ControlRequest::StopRecording => ControlResponse::Recording(RecordingSummary {
            duration: 3.0,
            frames: 180,
            zooms: 2,
        }),
        ControlRequest::ClipState => ControlResponse::Clip(ClipInfo {
            duration: 8.0,
            output_duration: 7.0,
            zooms: mock_zooms(),
            cuts: vec![CutSpan {
                start: 6.5,
                end: 7.5,
            }],
            speeds: vec![SpeedSpan {
                start: 2.0,
                end: 3.5,
                factor: 4.0,
            }],
            camera: vec![(0.0, 0.5, 0.5, 1.0), (0.25, 0.42, 0.55, 1.8)],
        }),
        ControlRequest::GetFrames { times, .. } => ControlResponse::Frames {
            frames: times
                .into_iter()
                .map(|t| FrameShot {
                    t,
                    width: 1,
                    height: 1,
                    png_base64: TINY_PNG.to_string(),
                })
                .collect(),
        },
        ControlRequest::AddZoom { .. }
        | ControlRequest::UpdateZoom { .. }
        | ControlRequest::SetZoomFocus { .. }
        | ControlRequest::RemoveZoom { .. } => ControlResponse::Zooms {
            zooms: mock_zooms(),
        },
        ControlRequest::AddCut { .. }
        | ControlRequest::UpdateCut { .. }
        | ControlRequest::RemoveCut { .. } => ControlResponse::Cuts {
            cuts: vec![CutSpan {
                start: 6.5,
                end: 7.5,
            }],
        },
        ControlRequest::AutoSpeed { factor, .. } => ControlResponse::Speeds {
            speeds: vec![SpeedSpan {
                start: 2.0,
                end: 3.5,
                factor,
            }],
        },
        ControlRequest::PreviewClip { .. } => ControlResponse::Preview(PreviewInfo {
            gif_base64: TINY_PNG.into(),
            frame_count: 10,
            width: 480,
            height: 270,
            duration: 2.0,
        }),
        ControlRequest::ListWindows => ControlResponse::Windows {
            windows: vec![WindowRect {
                title: "Mock Window".into(),
                x: 100,
                y: 80,
                w: 1280,
                h: 720,
            }],
        },
        ControlRequest::SetRegionToWindow { .. } => ControlResponse::Region(RegionRect {
            x: 100,
            y: 80,
            w: 1280,
            h: 720,
        }),
        ControlRequest::ExportGif { .. } | ControlRequest::ExportMp4 { .. } => {
            ControlResponse::Job { id: 1 }
        }
        ControlRequest::ExportStatus { id: _ } => ControlResponse::Export(ExportState {
            done: 180,
            total: 180,
            finished: true,
            error: None,
            path: "C:/tmp/out.gif".into(),
        }),
        ControlRequest::EstimateGif { .. } => ControlResponse::Size { bytes: 1_234_567 },
    }
}
