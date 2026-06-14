//! Localhost control server — the in-app half of the AI Demo Director.
//!
//! When enabled, Vuoom binds a `127.0.0.1` TCP port and answers newline-delimited
//! [`vuoom_control`] requests: set region, start/stop recording, inject mouse/keyboard,
//! sample frames for the agent to *see*, and export. The standalone `vuoom-mcp` sidecar
//! connects here on behalf of an AI agent (Claude). See `docs/13-AI-Demo-Director-Research.md`.
//!
//! **Safety:** the server can inject real input, so it is **opt-in** — it only starts when the
//! `VUOOM_ENABLE_CONTROL` environment variable is set, and it binds loopback only.

use crate::Engine;
use std::io::{BufRead, BufReader};
use std::net::{TcpListener, TcpStream};
use tauri::{AppHandle, Manager};
use vuoom_capture::CropRegion;
use vuoom_control::{
    write_message, Button, ControlRequest, ControlResponse, RecordingSummary, ScreenGeometry,
};
use vuoom_input::InjectButton;

/// Set this env var (to any value) to enable the control server. Off by default because it
/// can drive real mouse/keyboard input.
const ENABLE_VAR: &str = "VUOOM_ENABLE_CONTROL";

/// Start the control server on a background thread, if enabled. No-op otherwise.
pub fn start(app: AppHandle) {
    if std::env::var(ENABLE_VAR).is_err() {
        return;
    }
    std::thread::spawn(move || {
        let listener = match TcpListener::bind(("127.0.0.1", 0)) {
            Ok(l) => l,
            Err(e) => {
                eprintln!("vuoom control server: bind failed: {e}");
                return;
            }
        };
        let port = listener.local_addr().map_or(0, |a| a.port());
        if let Err(e) = vuoom_control::write_port_file(port) {
            eprintln!("vuoom control server: port file write failed: {e}");
        }
        eprintln!("vuoom control server: listening on 127.0.0.1:{port}");
        for stream in listener.incoming() {
            match stream {
                Ok(s) => {
                    let app = app.clone();
                    std::thread::spawn(move || serve(&app, s));
                }
                Err(e) => eprintln!("vuoom control server: accept failed: {e}"),
            }
        }
    });
}

/// Serve one client connection: read requests line-by-line, dispatch, write replies.
fn serve(app: &AppHandle, stream: TcpStream) {
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
            Ok(req) => dispatch(app, req),
            Err(e) => ControlResponse::error(format!("bad request: {e}")),
        };
        if write_message(&mut writer, &resp).is_err() {
            break;
        }
    }
}

/// Map a protocol [`Button`] to the injection [`InjectButton`].
fn map_button(b: Button) -> InjectButton {
    match b {
        Button::Left => InjectButton::Left,
        Button::Right => InjectButton::Right,
        Button::Middle => InjectButton::Middle,
    }
}

/// Wrap a `Result<(), String>` as an [`ControlResponse`].
fn unit(r: Result<(), String>) -> ControlResponse {
    match r {
        Ok(()) => ControlResponse::Ok,
        Err(e) => ControlResponse::error(e),
    }
}

/// Turn one request into a response, calling into injection (no engine needed) or the
/// recording/edit/export [`Engine`] session.
fn dispatch(app: &AppHandle, req: ControlRequest) -> ControlResponse {
    // Stateless ops: liveness, geometry, and input injection don't need the engine.
    match &req {
        ControlRequest::Ping => return ControlResponse::Ok,
        ControlRequest::ScreenGeometry => {
            let (x, y, width, height) = vuoom_input::virtual_screen();
            return ControlResponse::Geometry(ScreenGeometry {
                x,
                y,
                width,
                height,
            });
        }
        ControlRequest::MoveCursor { x, y } => {
            vuoom_input::move_cursor(*x, *y);
            return ControlResponse::Ok;
        }
        ControlRequest::Click {
            x,
            y,
            button,
            double,
        } => {
            vuoom_input::click(*x, *y, map_button(*button), *double);
            return ControlResponse::Ok;
        }
        ControlRequest::TypeText { text } => {
            vuoom_input::type_text(text);
            return ControlResponse::Ok;
        }
        ControlRequest::Scroll { x, y, delta } => {
            vuoom_input::scroll(*x, *y, *delta);
            return ControlResponse::Ok;
        }
        ControlRequest::KeyChord { keys } => {
            let mut vks = Vec::with_capacity(keys.len());
            for k in keys {
                match vuoom_input::key_to_vk(k) {
                    Some(vk) => vks.push(vk),
                    None => return ControlResponse::error(format!("unknown key: {k}")),
                }
            }
            vuoom_input::key_chord(&vks);
            return ControlResponse::Ok;
        }
        _ => {}
    }

    // Stateful ops need the booted session.
    let engine = app.state::<Engine>();
    let session = match engine.session() {
        Ok(s) => s,
        Err(e) => return ControlResponse::error(e),
    };
    match req {
        ControlRequest::SetRegion { x, y, w, h } => {
            let region = match (x, y, w, h) {
                (Some(x), Some(y), Some(w), Some(h)) if w > 0 && h > 0 => {
                    Some(CropRegion { x, y, w, h })
                }
                _ => None,
            };
            unit(session.set_region(region))
        }
        ControlRequest::SetZoomAmount { amount } => unit(session.set_zoom_amount(amount)),
        ControlRequest::SetAutoZoomOnClick { on } => unit(session.set_auto_zoom_on_click(on)),
        ControlRequest::StartRecording => unit(session.start_recording()),
        ControlRequest::StopRecording => match session.stop_recording() {
            Ok(s) => ControlResponse::Recording(RecordingSummary {
                duration: s.duration,
                frames: s.frames,
                zooms: s.zooms,
            }),
            Err(e) => ControlResponse::error(e),
        },
        ControlRequest::SetPaused { paused } => unit(session.set_record_paused(paused)),
        ControlRequest::Seek { t } => unit(session.seek(t)),
        ControlRequest::ClipState => match session.clip_info() {
            Ok(c) => ControlResponse::Clip(c),
            Err(e) => ControlResponse::error(e),
        },
        ControlRequest::GetFrames { times, width } => match session.sample_frames(&times, width) {
            Ok(frames) => ControlResponse::Frames { frames },
            Err(e) => ControlResponse::error(e),
        },
        ControlRequest::ExportGif {
            path,
            fps,
            width,
            quality,
        } => unit(session.export_gif(path, fps, width, quality, &|_, _| {})),
        ControlRequest::ExportMp4 {
            path,
            fps,
            width,
            quality,
        } => unit(session.export_mp4(path, fps, width, quality, &|_, _| {})),
        ControlRequest::EstimateGif {
            fps,
            width,
            quality,
        } => match session.estimate_gif(fps, width, quality) {
            Ok(bytes) => ControlResponse::Size { bytes },
            Err(e) => ControlResponse::error(e),
        },
        // Ping + injection + geometry already handled above.
        _ => ControlResponse::error("unhandled request"),
    }
}
