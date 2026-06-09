//! Tauri command surface — the thin bridge from the SolidJS editor to the Rust engine.
//!
//! These expose the verified pure-logic core (zoom config, project model, GIF settings
//! and size estimation). Capture / render / preview / export commands are added as those
//! crates come online. See `docs/02-Architecture.md`.

use crate::session::{AnnotationSet, RecordingSummary, Session};
use crate::windows_ext::exclude_from_capture;
use std::sync::Mutex;
use tauri::{AppHandle, LogicalSize, Manager, PhysicalPosition, PhysicalSize};
use vuoom_capture::CropRegion;
use vuoom_encode::{estimate_total_bytes, GifSettings};
use vuoom_project::{Project, SourceInfo, ZoomConfig};

// ── Recording flow ───────────────────────────────────────────────────────────────
//
// The whole flow (region selector → countdown → stop bar) runs as overlays INSIDE the
// single main window — no extra WebView2 windows. Spawning a second webview and navigating
// it to the bundled app proved unreliable (a blank-white window that never loaded the page
// on some machines), so we drive the proven main webview instead: go fullscreen for the
// region picker, shrink to a small bar for recording, then restore. The window is excluded
// from the capture the entire time, so none of this overlay UI lands in the recording.

/// The editor window's bounds (physical px) saved before the overlay takes over, so we can
/// put the editor back exactly where it was.
#[derive(Default)]
pub struct WindowStash(pub Mutex<Option<(i32, i32, u32, u32)>>);

/// Restore the editor window to its saved bounds (always runs, even on an error path).
fn restore_editor(app: &AppHandle, stash: &tauri::State<'_, WindowStash>) {
    if let Some(main) = app.get_webview_window("main") {
        let _ = main.set_fullscreen(false);
        let _ = main.set_always_on_top(false);
        if let Some((x, y, w, h)) = *stash.0.lock().unwrap_or_else(|e| e.into_inner()) {
            let _ = main.set_size(PhysicalSize::new(w, h));
            let _ = main.set_position(PhysicalPosition::new(x, y));
        }
        let _ = main.unminimize();
        let _ = main.show();
        let _ = main.set_focus();
    }
}

/// Step 1 — the user clicked Record: hide the editor from the capture, remember where it
/// was, and blow it up to fullscreen so the in-window region selector covers the display.
#[tauri::command]
pub fn enter_overlay(app: AppHandle, stash: tauri::State<'_, WindowStash>) -> Result<(), String> {
    let main = app.get_webview_window("main").ok_or("no main window")?;
    let _ = exclude_from_capture(&main);
    if let (Ok(pos), Ok(size)) = (main.outer_position(), main.outer_size()) {
        *stash.0.lock().map_err(|_| "lock poisoned")? =
            Some((pos.x, pos.y, size.width, size.height));
    }
    main.set_always_on_top(true).map_err(|e| e.to_string())?;
    main.set_fullscreen(true).map_err(|e| e.to_string())?;
    let _ = main.set_focus();
    Ok(())
}

/// Step 2 — a region (or full screen) was confirmed: drop out of fullscreen and shrink the
/// window to a small always-on-top bar parked bottom-center, ready for the countdown + Stop.
#[tauri::command]
pub fn enter_stopbar(app: AppHandle) -> Result<(), String> {
    let main = app.get_webview_window("main").ok_or("no main window")?;
    main.set_fullscreen(false).map_err(|e| e.to_string())?;
    main.set_size(LogicalSize::new(360.0, 84.0))
        .map_err(|e| e.to_string())?;
    main.set_always_on_top(true).map_err(|e| e.to_string())?;
    if let Ok(Some(mon)) = main.current_monitor() {
        let scale = mon.scale_factor();
        let mp = mon.position();
        let ms = mon.size();
        let bar_w = (360.0 * scale) as i32;
        let bar_h = (84.0 * scale) as i32;
        let margin = (28.0 * scale) as i32;
        let x = mp.x + (ms.width as i32 - bar_w) / 2;
        let y = mp.y + ms.height as i32 - bar_h - margin;
        let _ = main.set_position(PhysicalPosition::new(x, y));
    }
    Ok(())
}

/// Step 3 — the user stopped: finish capture, restore the editor, and return the clip
/// summary so the editor can load it.
#[tauri::command]
pub fn finish_recording(
    app: AppHandle,
    session: tauri::State<'_, Session>,
    stash: tauri::State<'_, WindowStash>,
) -> Result<RecordingSummary, String> {
    let result = session.stop_recording();
    restore_editor(&app, &stash); // always, even if stop failed
    result
}

/// Abort the flow (Cancel / Esc): restore the editor without producing a clip.
#[tauri::command]
pub fn cancel_record_flow(
    app: AppHandle,
    stash: tauri::State<'_, WindowStash>,
) -> Result<(), String> {
    restore_editor(&app, &stash);
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

/// Capture a still of the full display for the region selector's backdrop (data-URL PNG).
#[tauri::command]
pub fn screenshot(session: tauri::State<'_, Session>) -> Result<String, String> {
    session.screenshot()
}

/// Set the zoom multiplier (1.0 = no zoom) applied to the next recording.
#[tauri::command]
pub fn set_zoom_amount(session: tauri::State<'_, Session>, amount: f64) -> Result<(), String> {
    session.set_zoom_amount(amount)
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

/// Begin capturing the primary display + global input. `border` (default true) toggles the
/// OS capture highlight drawn around the recorded area.
#[tauri::command]
pub fn start_recording(
    session: tauri::State<'_, Session>,
    border: Option<bool>,
) -> Result<(), String> {
    session.start_recording(border.unwrap_or(true))
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
