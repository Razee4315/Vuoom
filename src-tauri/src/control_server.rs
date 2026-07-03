//! Localhost control server — the in-app half of the AI Demo Director.
//!
//! When enabled, Vuoom binds a `127.0.0.1` TCP port and answers newline-delimited
//! [`vuoom_control`] requests: set region, start/stop recording, inject mouse/keyboard,
//! sample frames for the agent to *see*, edit the clip, and export. The standalone
//! `vuoom-mcp` sidecar connects here on behalf of an AI agent (Claude).
//! See `docs/13-AI-Demo-Director-Research.md`.
//!
//! **Safety:** the server can inject real input, so it is **opt-in** — it only starts when
//! the `VUOOM_ENABLE_CONTROL` environment variable is set to a truthy value, it binds
//! loopback only, and every connection must present the random auth token from the
//! discovery file as its first line (loopback alone is not a trust boundary: any local
//! process could otherwise type into whatever window is focused).

use crate::session::ZoomStyle;
use crate::Engine;
use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use tauri::{AppHandle, Manager};
use vuoom_capture::CropRegion;
use vuoom_control::{
    write_message, Button, ControlRequest, ControlResponse, ExportState, RecordingSummary,
    ScreenGeometry,
};
use vuoom_input::InjectButton;

/// Set this env var to enable the control server (`0`/`false`/`off`/empty stay disabled).
/// Off by default because it can drive real mouse/keyboard input.
const ENABLE_VAR: &str = "VUOOM_ENABLE_CONTROL";

/// Whether this process wrote the discovery file (so shutdown cleanup never deletes a
/// file that belongs to another Vuoom instance).
static WROTE_PORT_FILE: AtomicBool = AtomicBool::new(false);

/// Whether the enable var is set to something truthy.
fn enabled() -> bool {
    match std::env::var(ENABLE_VAR) {
        Ok(v) => !matches!(
            v.trim().to_ascii_lowercase().as_str(),
            "" | "0" | "false" | "off"
        ),
        Err(_) => false,
    }
}

/// Start the control server on a background thread, if enabled. No-op otherwise.
pub fn start(app: AppHandle) {
    if !enabled() {
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
        let token = vuoom_control::generate_token();
        match vuoom_control::write_port_file(port, &token) {
            Ok(()) => WROTE_PORT_FILE.store(true, Ordering::Relaxed),
            Err(e) => eprintln!("vuoom control server: port file write failed: {e}"),
        }
        eprintln!("vuoom control server: listening on 127.0.0.1:{port}");
        let token = Arc::new(token);
        for stream in listener.incoming() {
            match stream {
                Ok(s) => {
                    let app = app.clone();
                    let token = Arc::clone(&token);
                    std::thread::spawn(move || serve(&app, s, &token));
                }
                Err(e) => eprintln!("vuoom control server: accept failed: {e}"),
            }
        }
    });
}

/// Delete the discovery file on app shutdown (only if this process wrote it), so a stale
/// endpoint never points a future sidecar at a dead — or someone else's — port.
pub fn cleanup() {
    if WROTE_PORT_FILE.load(Ordering::Relaxed) {
        vuoom_control::remove_port_file();
    }
}

/// Serve one client connection: authenticate, then read requests line-by-line, dispatch,
/// write replies.
fn serve(app: &AppHandle, stream: TcpStream, token: &str) {
    let Ok(mut writer) = stream.try_clone() else {
        return;
    };
    let mut reader = BufReader::new(stream);
    // The first line must be the auth token from the discovery file; drop the connection
    // otherwise (no reply — an unauthenticated peer learns nothing).
    let mut auth = String::new();
    if reader.read_line(&mut auth).is_err() || auth.trim() != token {
        eprintln!("vuoom control server: rejected connection (bad auth token)");
        return;
    }
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

/// Collapse the four optional `rect_*` components into a single rect, present only when all
/// four are given (the protocol treats a partial rect as "no rect").
fn rect_from(
    x: Option<f64>,
    y: Option<f64>,
    w: Option<f64>,
    h: Option<f64>,
) -> Option<(f64, f64, f64, f64)> {
    match (x, y, w, h) {
        (Some(x), Some(y), Some(w), Some(h)) => Some((x, y, w, h)),
        _ => None,
    }
}

/// Record where the agent just pointed (physical px) into the running session, so injected
/// typing can be given a caret focus at stop time. Silent no-op if the engine isn't booted
/// or nothing is recording — this must never fail an injection.
fn note_pointer(app: &AppHandle, x: i32, y: i32) {
    if let Ok(session) = app.state::<Engine>().session() {
        session.note_injected_pointer(x, y);
    }
}

// ── Export jobs ─────────────────────────────────────────────────────────────────────
//
// Exports take minutes; running them inline would block the connection (and the agent's
// tool call) past any sane timeout. Instead `ExportGif`/`ExportMp4` return a job id
// immediately and the encode runs on its own thread, publishing progress the agent polls
// with `ExportStatus`.

/// Progress/result of one export job, shared with its worker thread.
struct ExportJob {
    done: AtomicU64,
    total: AtomicU64,
    finished: AtomicBool,
    error: Mutex<Option<String>>,
    path: String,
}

fn jobs() -> &'static Mutex<HashMap<u64, Arc<ExportJob>>> {
    static JOBS: OnceLock<Mutex<HashMap<u64, Arc<ExportJob>>>> = OnceLock::new();
    JOBS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Which container an export job writes.
#[derive(Clone, Copy)]
enum ExportKind {
    Gif,
    Mp4,
}

/// Spawn an export worker and hand back its job id.
fn start_export(
    app: &AppHandle,
    kind: ExportKind,
    path: String,
    fps: u32,
    width: Option<u32>,
    quality: u8,
) -> ControlResponse {
    static NEXT_ID: AtomicU64 = AtomicU64::new(1);
    let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    let job = Arc::new(ExportJob {
        done: AtomicU64::new(0),
        total: AtomicU64::new(0),
        finished: AtomicBool::new(false),
        error: Mutex::new(None),
        path: path.clone(),
    });
    jobs()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(id, Arc::clone(&job));
    let app = app.clone();
    std::thread::spawn(move || {
        let result = (|| -> Result<(), String> {
            let engine = app.state::<Engine>();
            let session = engine.session()?;
            let progress = |done: u32, total: u32| {
                job.done.store(u64::from(done), Ordering::Relaxed);
                job.total.store(u64::from(total), Ordering::Relaxed);
            };
            match kind {
                ExportKind::Gif => session.export_gif(path, fps, width, quality, &progress),
                ExportKind::Mp4 => session.export_mp4(path, fps, width, quality, &progress),
            }
        })();
        if let Err(e) = result {
            *job.error.lock().unwrap_or_else(|e| e.into_inner()) = Some(e);
        }
        job.finished.store(true, Ordering::Relaxed);
    });
    ControlResponse::Job { id }
}

/// Snapshot an export job for `ExportStatus`.
fn export_status(id: u64) -> ControlResponse {
    let jobs = jobs().lock().unwrap_or_else(|e| e.into_inner());
    match jobs.get(&id) {
        Some(j) => ControlResponse::Export(ExportState {
            done: j.done.load(Ordering::Relaxed),
            total: j.total.load(Ordering::Relaxed),
            finished: j.finished.load(Ordering::Relaxed),
            error: j.error.lock().unwrap_or_else(|e| e.into_inner()).clone(),
            path: j.path.clone(),
        }),
        None => ControlResponse::error(format!("no such export job: {id}")),
    }
}

/// Turn one request into a response, calling into injection (no engine needed) or the
/// recording/edit/export [`Engine`] session.
#[allow(clippy::too_many_lines)]
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
        ControlRequest::MoveCursor { x, y, duration_ms } => {
            let r = vuoom_input::move_cursor_smooth(*x, *y, *duration_ms);
            if r.is_ok() {
                note_pointer(app, *x, *y);
            }
            return unit(r);
        }
        ControlRequest::Click {
            x,
            y,
            button,
            double,
            glide_ms,
        } => {
            let r = vuoom_input::click(*x, *y, map_button(*button), *double, *glide_ms);
            if r.is_ok() {
                note_pointer(app, *x, *y);
            }
            return unit(r);
        }
        ControlRequest::Drag {
            x1,
            y1,
            x2,
            y2,
            button,
            duration_ms,
        } => {
            let r = vuoom_input::drag(*x1, *y1, *x2, *y2, map_button(*button), *duration_ms);
            // The caret ends at the drag's release point.
            if r.is_ok() {
                note_pointer(app, *x2, *y2);
            }
            return unit(r);
        }
        ControlRequest::TypeText { text, cps } => {
            return unit(vuoom_input::type_text(text, *cps));
        }
        ControlRequest::Scroll {
            x,
            y,
            delta,
            step_ms,
        } => {
            return unit(vuoom_input::scroll(*x, *y, *delta, *step_ms));
        }
        ControlRequest::KeyChord { keys } => {
            let mut vks = Vec::with_capacity(keys.len());
            for k in keys {
                match vuoom_input::key_to_vk(k) {
                    Some(vk) => vks.push(vk),
                    None => return ControlResponse::error(format!("unknown key: {k}")),
                }
            }
            return unit(vuoom_input::key_chord(&vks));
        }
        ControlRequest::ExportStatus { id } => return export_status(*id),
        _ => {}
    }

    // Stateful ops need the booted session.
    let engine = app.state::<Engine>();
    let session = match engine.session() {
        Ok(s) => s,
        Err(e) => return ControlResponse::error(e),
    };
    match req {
        ControlRequest::Screenshot { width } => match session.screenshot_shot(width) {
            Ok(shot) => ControlResponse::Shot(shot),
            Err(e) => ControlResponse::error(e),
        },
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
        ControlRequest::SetZoomStyle {
            hold,
            pre_roll,
            hl_zoom,
            hl_pan,
            merge_gap,
            merge_radius,
            min_rezoom_interval,
            dead_zone,
        } => unit(session.set_zoom_style(ZoomStyle {
            hold,
            pre_roll,
            hl_zoom,
            hl_pan,
            merge_gap,
            merge_radius,
            min_rezoom_interval,
            dead_zone,
        })),
        ControlRequest::StartRecording { auto_zoom_on_click } => {
            // The agent drives via clicks, so click-zoom defaults ON for agent recordings.
            if let Err(e) = session.set_auto_zoom_on_click(auto_zoom_on_click.unwrap_or(true)) {
                return ControlResponse::error(e);
            }
            match session.start_recording() {
                Ok(()) => ControlResponse::Ok,
                Err(e) => {
                    // A failed start must not leave the agent flag armed for a later
                    // interactive recording.
                    session.reset_agent_pending();
                    ControlResponse::error(e)
                }
            }
        }
        ControlRequest::StopRecording => match session.stop_recording() {
            Ok(s) => ControlResponse::Recording(RecordingSummary {
                duration: s.duration,
                frames: s.frames,
                zooms: s.zooms,
            }),
            Err(e) => ControlResponse::error(e),
        },
        ControlRequest::CancelRecording => unit(session.cancel_recording()),
        ControlRequest::SetPaused { paused } => unit(session.set_record_paused(paused)),
        ControlRequest::Status => ControlResponse::Status(session.status()),
        ControlRequest::Seek { t } => unit(session.seek_output(t)),
        ControlRequest::ClipState => match session.clip_info() {
            Ok(c) => ControlResponse::Clip(c),
            Err(e) => ControlResponse::error(e),
        },
        ControlRequest::GetFrames { times, width } => match session.sample_frames(&times, width) {
            Ok(frames) => ControlResponse::Frames { frames },
            Err(e) => ControlResponse::error(e),
        },
        // Edit ops answer with the updated span lists (via clip_info, the single source of
        // truth for the index ↔ span mapping the agent edits by).
        ControlRequest::AddZoom {
            t,
            rect_x,
            rect_y,
            rect_w,
            rect_h,
            hl_zoom_in,
            hl_zoom_out,
        } => {
            let rect = rect_from(rect_x, rect_y, rect_w, rect_h);
            zooms_after(
                session,
                session
                    .add_zoom(t, rect, hl_zoom_in, hl_zoom_out)
                    .map(|_| ()),
            )
        }
        ControlRequest::UpdateZoom {
            index,
            start,
            end,
            amount,
            hl_zoom_in,
            hl_zoom_out,
        } => zooms_after(
            session,
            session
                .update_zoom(index, start, end, amount, hl_zoom_in, hl_zoom_out)
                .map(|_| ()),
        ),
        ControlRequest::SetZoomFocus {
            index,
            x,
            y,
            rect_x,
            rect_y,
            rect_w,
            rect_h,
        } => {
            let focus = match (x, y) {
                (Some(x), Some(y)) => Some((x, y)),
                _ => None,
            };
            let rect = rect_from(rect_x, rect_y, rect_w, rect_h);
            zooms_after(
                session,
                session.set_zoom_focus(index, focus, rect).map(|_| ()),
            )
        }
        ControlRequest::RemoveZoom { index } => {
            zooms_after(session, session.delete_zoom(index).map(|_| ()))
        }
        ControlRequest::AddCut { start, end } => {
            cuts_after(session, session.add_cut(start, end).map(|_| ()))
        }
        ControlRequest::UpdateCut { index, start, end } => {
            cuts_after(session, session.update_cut(index, start, end).map(|_| ()))
        }
        ControlRequest::RemoveCut { index } => {
            cuts_after(session, session.delete_cut(index).map(|_| ()))
        }
        ControlRequest::AutoSpeed { factor } => match session.auto_speed(factor) {
            Ok(_) => match session.clip_info() {
                Ok(c) => ControlResponse::Speeds { speeds: c.speeds },
                Err(e) => ControlResponse::error(e),
            },
            Err(e) => ControlResponse::error(e),
        },
        ControlRequest::ClearSpeed => unit(session.clear_speed()),
        ControlRequest::SetTrim { start, end } => unit(session.set_trim(start, end)),
        ControlRequest::ExportGif {
            path,
            fps,
            width,
            quality,
        } => start_export(app, ExportKind::Gif, path, fps, width, quality),
        ControlRequest::ExportMp4 {
            path,
            fps,
            width,
            quality,
        } => start_export(app, ExportKind::Mp4, path, fps, width, quality),
        ControlRequest::EstimateGif {
            fps,
            width,
            quality,
        } => match session.estimate_gif(fps, width, quality) {
            Ok(bytes) => ControlResponse::Size { bytes },
            Err(e) => ControlResponse::error(e),
        },
        // Ping + injection + geometry + export status already handled above.
        _ => ControlResponse::error("unhandled request"),
    }
}

/// After a zoom edit, answer with the updated zoom spans (or the edit's error).
fn zooms_after(session: &crate::session::Session, r: Result<(), String>) -> ControlResponse {
    match r.and_then(|()| session.clip_info()) {
        Ok(c) => ControlResponse::Zooms { zooms: c.zooms },
        Err(e) => ControlResponse::error(e),
    }
}

/// After a cut edit, answer with the updated cut spans (or the edit's error).
fn cuts_after(session: &crate::session::Session, r: Result<(), String>) -> ControlResponse {
    match r.and_then(|()| session.clip_info()) {
        Ok(c) => ControlResponse::Cuts { cuts: c.cuts },
        Err(e) => ControlResponse::error(e),
    }
}
