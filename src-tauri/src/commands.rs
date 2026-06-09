//! Tauri command surface — the thin bridge from the SolidJS editor to the Rust engine.
//!
//! These expose the verified pure-logic core (zoom config, project model, GIF settings
//! and size estimation). Capture / render / preview / export commands are added as those
//! crates come online. See `docs/02-Architecture.md`.

use crate::session::{AnnotationSet, RecordingSummary, Session};
use crate::windows_ext::exclude_from_capture;
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder};
use vuoom_capture::CropRegion;
use vuoom_encode::{estimate_total_bytes, GifSettings};
use vuoom_project::{Project, SourceInfo, ZoomConfig};

// ── Recording flow: selector → countdown → stop bar, all hidden from the capture ──

/// Restore the main editor window (always runs, even on an error path).
fn restore_main(app: &AppHandle) {
    if let Some(main) = app.get_webview_window("main") {
        let _ = main.unminimize();
        let _ = main.show();
        let _ = main.set_focus();
    }
}

/// Step 1 — the user clicked Record: hide the editor from the capture and open the
/// full-screen region selector.
#[tauri::command]
pub fn start_record_flow(app: AppHandle) -> Result<(), String> {
    if let Some(main) = app.get_webview_window("main") {
        let _ = exclude_from_capture(&main);
        let _ = main.minimize();
    }
    let selector = WebviewWindowBuilder::new(
        &app,
        "selector",
        WebviewUrl::App("index.html#selector".into()),
    )
    .title("Select area")
    .decorations(false)
    .transparent(true)
    .always_on_top(true)
    .skip_taskbar(true)
    .fullscreen(true)
    .build()
    .map_err(|e| e.to_string())?;
    let _ = exclude_from_capture(&selector);
    Ok(())
}

/// Step 2 — the selector confirmed a region (or full screen): close it and open the
/// recorder overlay, which runs the countdown and becomes the Stop bar.
#[tauri::command]
pub fn begin_countdown(app: AppHandle) -> Result<(), String> {
    if let Some(sel) = app.get_webview_window("selector") {
        let _ = sel.close();
    }
    let recorder = WebviewWindowBuilder::new(
        &app,
        "recorder",
        WebviewUrl::App("index.html#recorder".into()),
    )
    .title("Recording")
    .decorations(false)
    .transparent(true)
    .always_on_top(true)
    .skip_taskbar(true)
    .resizable(false)
    .inner_size(360.0, 96.0)
    .build()
    .map_err(|e| e.to_string())?;
    let _ = exclude_from_capture(&recorder);
    Ok(())
}

/// Step 3 — the user stopped: finish capture, close the recorder, restore the editor, and
/// hand the clip summary to the main window via an event.
#[tauri::command]
pub fn finish_recording(app: AppHandle, session: tauri::State<'_, Session>) -> Result<(), String> {
    let result = session.stop_recording();
    if let Some(rec) = app.get_webview_window("recorder") {
        let _ = rec.close();
    }
    restore_main(&app); // always, even if stop failed
    let summary = result?;
    app.emit("recording-finished", summary)
        .map_err(|e| e.to_string())
}

/// Abort the flow (Cancel / closed overlay): tear down overlays and restore the editor.
#[tauri::command]
pub fn cancel_record_flow(app: AppHandle) -> Result<(), String> {
    for label in ["selector", "recorder"] {
        if let Some(w) = app.get_webview_window(label) {
            let _ = w.close();
        }
    }
    restore_main(&app);
    Ok(())
}

/// Set the capture region for the next recording (physical px); omit fields for full screen.
#[tauri::command]
pub fn set_region(
    session: tauri::State<'_, Session>,
    x: Option<u32>,
    y: Option<u32>,
    w: Option<u32>,
    h: Option<u32>,
) -> Result<(), String> {
    let region = match (x, y, w, h) {
        (Some(x), Some(y), Some(w), Some(h)) if w > 0 && h > 0 => Some(CropRegion { x, y, w, h }),
        _ => None,
    };
    session.set_region(region)
}

/// The default auto-zoom tuning (Screen-Studio-quality starting point).
#[tauri::command]
pub fn default_zoom_config() -> ZoomConfig {
    ZoomConfig::default()
}

/// A fresh project for a freshly captured recording.
#[tauri::command]
pub fn new_project(width: u32, height: u32, fps: f64, duration: f64, path: String) -> Project {
    Project::new(SourceInfo {
        path,
        width,
        height,
        fps,
        duration,
    })
}

/// Persist a project to a `.vuoom` JSON manifest.
#[tauri::command]
pub fn save_project(project: Project, path: String) -> Result<(), String> {
    let json = project.to_json().map_err(|e| e.to_string())?;
    std::fs::write(&path, json).map_err(|e| e.to_string())
}

/// Load a `.vuoom` JSON manifest.
#[tauri::command]
pub fn load_project(path: String) -> Result<Project, String> {
    let json = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    Project::from_json(&json).map_err(|e| e.to_string())
}

/// Save the current recording as a `.vuoom` bundle (manifest + frames) at `dir`.
#[tauri::command]
pub fn save_project_bundle(session: tauri::State<'_, Session>, dir: String) -> Result<(), String> {
    session.save_bundle(std::path::Path::new(&dir))
}

/// Open a `.vuoom` bundle and rehydrate the editor; returns a summary like `stop_recording`.
#[tauri::command]
pub fn open_project_bundle(
    session: tauri::State<'_, Session>,
    dir: String,
) -> Result<RecordingSummary, String> {
    session.open_bundle(std::path::Path::new(&dir))
}

/// The two GIF export presets: `(readme, high_quality)`.
#[tauri::command]
pub fn gif_presets() -> (GifSettings, GifSettings) {
    (GifSettings::readme(), GifSettings::high_quality())
}

/// Estimate the final GIF size (bytes) from an encoded sample (sample-and-extrapolate).
#[tauri::command]
pub fn estimate_gif_size(
    sample_bytes: u64,
    sample_frames: usize,
    total_frames: usize,
    motion_factor: f64,
) -> u64 {
    estimate_total_bytes(sample_bytes, sample_frames, total_frames, motion_factor)
}

// ── Recording / preview / export ──────────────────────────────────────────────

/// The localhost port the webview connects to for the live preview stream.
#[tauri::command]
pub fn preview_port(session: tauri::State<'_, Session>) -> u16 {
    session.preview_port()
}

/// Begin capturing the primary display + global input.
#[tauri::command]
pub fn start_recording(session: tauri::State<'_, Session>) -> Result<(), String> {
    session.start_recording()
}

/// Stop capturing and build the editable project; returns a summary for the UI.
#[tauri::command]
pub fn stop_recording(session: tauri::State<'_, Session>) -> Result<RecordingSummary, String> {
    session.stop_recording()
}

/// Composite the frame at time `t` (seconds) and push it to the preview.
#[tauri::command]
pub fn seek(session: tauri::State<'_, Session>, t: f64) -> Result<(), String> {
    session.seek(t)
}

/// Export the recording to an optimized GIF at `path`.
#[tauri::command]
pub fn export_gif(
    session: tauri::State<'_, Session>,
    path: String,
    fps: u32,
    width: Option<u32>,
    quality: u8,
) -> Result<(), String> {
    session.export_gif(path, fps, width, quality)
}

/// Add a text label at normalized `(x, y)` from time `t`.
#[tauri::command]
pub fn add_text(
    session: tauri::State<'_, Session>,
    text: String,
    x: f64,
    y: f64,
    t: f64,
) -> Result<u32, String> {
    session.add_text(text, x, y, t)
}

/// Add an arrow between normalized points from time `t`.
#[tauri::command]
pub fn add_arrow(
    session: tauri::State<'_, Session>,
    fx: f64,
    fy: f64,
    tx: f64,
    ty: f64,
    t: f64,
) -> Result<u32, String> {
    session.add_arrow(fx, fy, tx, ty, t)
}

/// Add a highlight box (normalized rect) from time `t`.
#[tauri::command]
pub fn add_box(
    session: tauri::State<'_, Session>,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    t: f64,
) -> Result<u32, String> {
    session.add_box(x, y, w, h, t)
}

/// Snapshot every annotation (for the editor overlay).
#[tauri::command]
pub fn list_annotations(session: tauri::State<'_, Session>) -> Result<AnnotationSet, String> {
    session.annotations()
}

/// Move/edit a text label (omit a field to leave it unchanged).
#[tauri::command]
pub fn update_text(
    session: tauri::State<'_, Session>,
    id: u32,
    x: Option<f64>,
    y: Option<f64>,
    text: Option<String>,
    font_size: Option<f32>,
) -> Result<(), String> {
    session.update_text(id, x, y, text, font_size)
}

/// Move an arrow's endpoints.
#[tauri::command]
pub fn update_arrow(
    session: tauri::State<'_, Session>,
    id: u32,
    fx: f64,
    fy: f64,
    tx: f64,
    ty: f64,
) -> Result<(), String> {
    session.update_arrow(id, fx, fy, tx, ty)
}

/// Move/resize a highlight box.
#[tauri::command]
pub fn update_box(
    session: tauri::State<'_, Session>,
    id: u32,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
) -> Result<(), String> {
    session.update_box(id, x, y, w, h)
}

/// Tint an annotation (text, arrow, or box) by id.
#[tauri::command]
pub fn set_annotation_color(
    session: tauri::State<'_, Session>,
    id: u32,
    r: f64,
    g: f64,
    b: f64,
) -> Result<(), String> {
    session.set_annotation_color(id, r, g, b)
}

/// Delete an annotation (text, arrow, or box) by id.
#[tauri::command]
pub fn delete_annotation(session: tauri::State<'_, Session>, id: u32) -> Result<(), String> {
    session.delete_annotation(id)
}
