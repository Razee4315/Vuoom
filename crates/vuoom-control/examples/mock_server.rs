//! A mock Vuoom control server for testing the `vuoom-mcp` sidecar without the full app.
//!
//! It speaks the real [`vuoom_control`] protocol over a `127.0.0.1` port and writes the
//! discovery file, so the sidecar connects to it exactly as it would to Vuoom — but it returns
//! canned responses instead of recording. Lets the whole agent → MCP → protocol path be
//! exercised on any machine (no GPU/capture). Run: `cargo run -p vuoom-control --example mock_server`.

use std::io::{BufRead, BufReader};
use std::net::{TcpListener, TcpStream};

use vuoom_control::{
    write_message, ClipInfo, ControlRequest, ControlResponse, FrameShot, RecordingSummary,
    ScreenGeometry,
};

/// A 1×1 transparent PNG (base64) — stands in for a sampled frame.
const TINY_PNG: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==";

fn main() -> std::io::Result<()> {
    let listener = TcpListener::bind(("127.0.0.1", 0))?;
    let port = listener.local_addr()?.port();
    vuoom_control::write_port_file(port)?;
    eprintln!("mock control server: listening on 127.0.0.1:{port}");
    for stream in listener.incoming() {
        match stream {
            Ok(s) => serve(s),
            Err(e) => eprintln!("mock: accept failed: {e}"),
        }
    }
    Ok(())
}

fn serve(stream: TcpStream) {
    let Ok(mut writer) = stream.try_clone() else {
        return;
    };
    let reader = BufReader::new(stream);
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

/// Canned replies covering every request variant.
fn respond(req: ControlRequest) -> ControlResponse {
    match req {
        ControlRequest::Ping
        | ControlRequest::SetRegion { .. }
        | ControlRequest::SetZoomAmount { .. }
        | ControlRequest::StartRecording
        | ControlRequest::SetPaused { .. }
        | ControlRequest::MoveCursor { .. }
        | ControlRequest::Click { .. }
        | ControlRequest::TypeText { .. }
        | ControlRequest::KeyChord { .. }
        | ControlRequest::Scroll { .. }
        | ControlRequest::Seek { .. }
        | ControlRequest::ExportGif { .. }
        | ControlRequest::ExportMp4 { .. } => ControlResponse::Ok,
        ControlRequest::ScreenGeometry => ControlResponse::Geometry(ScreenGeometry {
            x: 0,
            y: 0,
            width: 1920,
            height: 1080,
        }),
        ControlRequest::StopRecording => ControlResponse::Recording(RecordingSummary {
            duration: 3.0,
            frames: 180,
            zooms: 2,
        }),
        ControlRequest::ClipState => ControlResponse::Clip(ClipInfo {
            duration: 3.0,
            zooms: 2,
            cuts: 0,
            speed_regions: 1,
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
        ControlRequest::EstimateGif { .. } => ControlResponse::Size { bytes: 1_234_567 },
    }
}
