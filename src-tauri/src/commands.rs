//! Tauri command surface — the thin bridge from the SolidJS editor to the Rust engine.
//!
//! These expose the verified pure-logic core (zoom config, project model, GIF settings
//! and size estimation). Capture / render / preview / export commands are added as those
//! crates come online. See `docs/02-Architecture.md`.

use crate::session::{RecordingSummary, Session};
use vuoom_encode::{estimate_total_bytes, GifSettings};
use vuoom_project::{Project, SourceInfo, ZoomConfig};

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
) -> Result<(), String> {
    session.export_gif(path, fps, width)
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
