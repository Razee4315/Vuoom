//! Tauri command surface — the thin bridge from the SolidJS editor to the Rust engine.
//!
//! These expose the verified pure-logic core (zoom config, project model, GIF settings
//! and size estimation). Capture / render / preview / export commands are added as those
//! crates come online. See `docs/02-Architecture.md`.

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
