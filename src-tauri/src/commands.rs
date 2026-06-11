//! Tauri command surface — the thin bridge from the SolidJS editor to the Rust engine.
//!
//! Commands that need the engine go through [`crate::Engine`], which boots on a background
//! thread at launch; until it's ready they fail with the `"engine-starting"` sentinel and
//! the frontend retries. Heavy commands (capture, composite, encode, disk I/O) are `async`
//! so they run off the main thread and never freeze the UI.

use crate::hotkey::{RecordingHotkey, StopHotkey};
use crate::region_border::RegionBorder;
use crate::session::{AnnotationSet, ClipState, RecordingSummary};
use crate::windows_ext::{copy_file_to_clipboard, exclude_from_capture};
use crate::Engine;
use serde::Serialize;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, LogicalSize, Manager, PhysicalPosition};
use vuoom_capture::CropRegion;
use vuoom_project::{SpeedRegion, Trim, ZoomKeyframe};

/// The visible frame around the recorded region, plus the region it should frame.
/// Held as Tauri managed state so the record-flow commands can show/clear it.
#[derive(Default)]
pub struct BorderState {
    /// A copy of the region chosen by the selector (physical px); `None` = full screen.
    pub region: Mutex<Option<CropRegion>>,
    /// The live strips while recording; dropping them removes the frame.
    pub border: Mutex<Option<RegionBorder>>,
}

// ── Recording flow ───────────────────────────────────────────────────────────────
//
// The whole flow (region selector → countdown → stop bar) runs as overlays INSIDE the
// single main window — no extra WebView2 windows. Spawning a second webview and navigating
// it to the bundled app proved unreliable (a blank-white window that never loaded the page
// on some machines), so we drive the proven main webview instead: go fullscreen for the
// region picker, shrink to a small bar for recording, then restore. The window is excluded
// from the capture the entire time, so none of this overlay UI lands in the recording.

/// Restore the editor window after the overlay flow: drop fullscreen / always-on-top and
/// bring it back maximized (the editor's default state). Always runs, even on an error path.
fn restore_editor(app: &AppHandle) {
    if let Some(main) = app.get_webview_window("main") {
        let _ = main.set_fullscreen(false);
        let _ = main.set_always_on_top(false);
        let _ = main.unminimize();
        let _ = main.maximize();
        let _ = main.show();
        let _ = main.set_focus();
    }
}

/// Step 1 — the user clicked Record: hide the editor from the capture and blow it up to
/// fullscreen so the in-window region selector covers the display.
#[tauri::command]
pub fn enter_overlay(app: AppHandle) -> Result<(), String> {
    let main = app.get_webview_window("main").ok_or("no main window")?;
    let _ = exclude_from_capture(&main);
    main.set_always_on_top(true).map_err(|e| e.to_string())?;
    main.set_fullscreen(true).map_err(|e| e.to_string())?;
    let _ = main.set_focus();
    Ok(())
}

/// Width/height (logical px) of the recording panel — a live zoom preview + Stop controls.
const PANEL_W: f64 = 384.0;
const PANEL_H: f64 = 300.0;

/// Step 2 — a region (or full screen) was confirmed: drop out of fullscreen and shrink the
/// window to the always-on-top recording panel (live preview + Stop), parked bottom-right
/// so it stays out of the way of what is being recorded.
#[tauri::command]
pub fn enter_stopbar(app: AppHandle) -> Result<(), String> {
    let main = app.get_webview_window("main").ok_or("no main window")?;
    main.set_fullscreen(false).map_err(|e| e.to_string())?;
    main.set_size(LogicalSize::new(PANEL_W, PANEL_H))
        .map_err(|e| e.to_string())?;
    main.set_always_on_top(true).map_err(|e| e.to_string())?;
    if let Ok(Some(mon)) = main.current_monitor() {
        let scale = mon.scale_factor();
        let mp = mon.position();
        let ms = mon.size();
        let panel_w = (PANEL_W * scale) as i32;
        let panel_h = (PANEL_H * scale) as i32;
        let margin = (24.0 * scale) as i32;
        let x = mp.x + ms.width as i32 - panel_w - margin;
        let y = mp.y + ms.height as i32 - panel_h - margin;
        let _ = main.set_position(PhysicalPosition::new(x, y));
    }
    Ok(())
}

/// Step 3 — the user stopped: finish capture, restore the editor, and return the clip
/// summary so the editor can load it.
#[tauri::command]
pub async fn finish_recording(
    app: AppHandle,
    engine: tauri::State<'_, Engine>,
    hotkey: tauri::State<'_, RecordingHotkey>,
    border: tauri::State<'_, BorderState>,
) -> Result<RecordingSummary, String> {
    drop_hotkey(&hotkey);
    drop_border(&border);
    let result = engine.session()?.stop_recording();
    restore_editor(&app); // always, even if stop failed
    result
}

/// Abort the flow (Cancel / Esc): restore the editor without producing a clip.
#[tauri::command]
pub fn cancel_record_flow(
    app: AppHandle,
    hotkey: tauri::State<'_, RecordingHotkey>,
    border: tauri::State<'_, BorderState>,
) -> Result<(), String> {
    drop_hotkey(&hotkey);
    drop_border(&border);
    restore_editor(&app);
    Ok(())
}

/// Remove the recorded-region frame, if one is showing.
fn drop_border(border: &BorderState) {
    if let Ok(mut slot) = border.border.lock() {
        *slot = None;
    }
}

/// Stop the Ctrl+Shift+X watcher, if one is running.
fn drop_hotkey(hotkey: &RecordingHotkey) {
    if let Ok(mut slot) = hotkey.0.lock() {
        *slot = None;
    }
}

/// Set the capture region for the next recording (physical px); omit fields for full screen.
#[tauri::command]
pub fn set_region(
    engine: tauri::State<'_, Engine>,
    border: tauri::State<'_, BorderState>,
    x: Option<u32>,
    y: Option<u32>,
    w: Option<u32>,
    h: Option<u32>,
) -> Result<(), String> {
    let region = match (x, y, w, h) {
        (Some(x), Some(y), Some(w), Some(h)) if w > 0 && h > 0 => Some(CropRegion { x, y, w, h }),
        _ => None,
    };
    if let Ok(mut slot) = border.region.lock() {
        *slot = region;
    }
    engine.session()?.set_region(region)
}

/// Capture a still of the full display for the region selector's backdrop (data-URL PNG).
#[tauri::command]
pub async fn screenshot(engine: tauri::State<'_, Engine>) -> Result<String, String> {
    engine.session()?.screenshot()
}

/// Set the zoom multiplier (1.0 = no zoom) applied to the next recording.
#[tauri::command]
pub fn set_zoom_amount(engine: tauri::State<'_, Engine>, amount: f64) -> Result<(), String> {
    engine.session()?.set_zoom_amount(amount)
}

/// Save the current recording as a `.vuoom` bundle (manifest + frames) at `dir`.
#[tauri::command]
pub async fn save_project_bundle(
    engine: tauri::State<'_, Engine>,
    dir: String,
) -> Result<(), String> {
    engine.session()?.save_bundle(std::path::Path::new(&dir))
}

/// Open a `.vuoom` bundle and rehydrate the editor; returns a summary like `finish_recording`.
#[tauri::command]
pub async fn open_project_bundle(
    engine: tauri::State<'_, Engine>,
    dir: String,
) -> Result<RecordingSummary, String> {
    engine.session()?.open_bundle(std::path::Path::new(&dir))
}

// ── Recording / preview / export ──────────────────────────────────────────────

/// The localhost port the webview connects to for the live preview stream.
#[tauri::command]
pub fn preview_port(engine: tauri::State<'_, Engine>) -> Result<u16, String> {
    Ok(engine.session()?.preview_port())
}

/// Begin capturing the primary display + global input, and arm the global
/// Ctrl+Shift+X stop hotkey for the duration of the recording.
#[tauri::command]
pub async fn start_recording(
    app: AppHandle,
    engine: tauri::State<'_, Engine>,
    hotkey: tauri::State<'_, RecordingHotkey>,
    border: tauri::State<'_, BorderState>,
) -> Result<(), String> {
    engine.session()?.start_recording()?;
    // Frame the recorded region so the user always sees what's being captured. The strips
    // sit outside the crop AND are capture-excluded, so they never land in the recording.
    // Full-screen recordings skip the frame — the screen edge is the region.
    let region = border.region.lock().ok().and_then(|r| *r);
    if let (Some(r), Ok(mut slot)) = (region, border.border.lock()) {
        *slot = RegionBorder::show(r.x as i32, r.y as i32, r.w as i32, r.h as i32);
    }
    if let Ok(mut slot) = hotkey.0.lock() {
        *slot = Some(StopHotkey::watch(app));
    }
    Ok(())
}

/// Pause / resume the running recording (the paused span becomes a cut).
#[tauri::command]
pub fn set_record_paused(engine: tauri::State<'_, Engine>, paused: bool) -> Result<(), String> {
    engine.session()?.set_record_paused(paused)
}

/// Composite the frame at time `t` (seconds) and push it to the preview.
#[tauri::command]
pub async fn seek(engine: tauri::State<'_, Engine>, t: f64) -> Result<(), String> {
    engine.session()?.seek(t)
}

/// Payload for the `export-progress` event the export panel listens to.
#[derive(Clone, Copy, Serialize)]
struct ExportProgress {
    done: u32,
    total: u32,
}

/// Export the recording to an optimized GIF at `path`, emitting `export-progress`
/// events as frames composite and the encode completes.
#[tauri::command]
pub async fn export_gif(
    app: AppHandle,
    engine: tauri::State<'_, Engine>,
    path: String,
    fps: u32,
    width: Option<u32>,
    quality: u8,
) -> Result<(), String> {
    engine
        .session()?
        .export_gif(path, fps, width, quality, &|done, total| {
            let _ = app.emit("export-progress", ExportProgress { done, total });
        })
}

/// Export the recording to an H.264 MP4 at `path`, emitting `export-progress` events.
#[tauri::command]
pub async fn export_mp4(
    app: AppHandle,
    engine: tauri::State<'_, Engine>,
    path: String,
    fps: u32,
    width: Option<u32>,
    quality: u8,
) -> Result<(), String> {
    engine
        .session()?
        .export_mp4(path, fps, width, quality, &|done, total| {
            let _ = app.emit("export-progress", ExportProgress { done, total });
        })
}

/// Estimate the export size in bytes for the given settings (sample-and-extrapolate).
#[tauri::command]
pub async fn estimate_gif(
    engine: tauri::State<'_, Engine>,
    fps: u32,
    width: Option<u32>,
    quality: u8,
) -> Result<u64, String> {
    engine.session()?.estimate_gif(fps, width, quality)
}

/// Put the exported GIF on the clipboard as a file (CF_HDROP), so pasting into Slack /
/// Discord / a GitHub comment uploads the animated file.
#[tauri::command]
pub fn copy_gif_to_clipboard(path: String) -> Result<(), String> {
    copy_file_to_clipboard(&path)
}

// ── Timeline edits ─────────────────────────────────────────────────────────────

/// Set the trim range in seconds (full range clears the trim).
#[tauri::command]
pub fn set_trim(engine: tauri::State<'_, Engine>, start: f64, end: f64) -> Result<(), String> {
    engine.session()?.set_trim(start, end)
}

/// Detect idle stretches and mark them to play at `factor`×; returns the regions.
#[tauri::command]
pub fn auto_speed(
    engine: tauri::State<'_, Engine>,
    factor: f64,
) -> Result<Vec<SpeedRegion>, String> {
    engine.session()?.auto_speed(factor)
}

/// Remove all speed regions.
#[tauri::command]
pub fn clear_speed(engine: tauri::State<'_, Engine>) -> Result<(), String> {
    engine.session()?.clear_speed()
}

/// Manually mark `[start, end]` to play at `factor`×; returns the updated region list.
#[tauri::command]
pub fn add_speed(
    engine: tauri::State<'_, Engine>,
    start: f64,
    end: f64,
    factor: f64,
) -> Result<Vec<SpeedRegion>, String> {
    engine.session()?.add_speed_region(start, end, factor)
}

/// Retime / re-level the speed region at `index`; returns the updated region list.
#[tauri::command]
pub fn update_speed(
    engine: tauri::State<'_, Engine>,
    index: usize,
    start: f64,
    end: f64,
    factor: f64,
) -> Result<Vec<SpeedRegion>, String> {
    engine
        .session()?
        .update_speed_region(index, start, end, factor)
}

/// Delete the speed region at `index`; returns the updated region list.
#[tauri::command]
pub fn delete_speed(
    engine: tauri::State<'_, Engine>,
    index: usize,
) -> Result<Vec<SpeedRegion>, String> {
    engine.session()?.delete_speed_region(index)
}

/// Remove `[start, end]` from the output entirely; returns the updated cut list.
#[tauri::command]
pub fn add_cut(
    engine: tauri::State<'_, Engine>,
    start: f64,
    end: f64,
) -> Result<Vec<Trim>, String> {
    engine.session()?.add_cut(start, end)
}

/// Retime the cut at `index`; returns the updated cut list.
#[tauri::command]
pub fn update_cut(
    engine: tauri::State<'_, Engine>,
    index: usize,
    start: f64,
    end: f64,
) -> Result<Vec<Trim>, String> {
    engine.session()?.update_cut(index, start, end)
}

/// Restore the cut at `index`; returns the updated cut list.
#[tauri::command]
pub fn delete_cut(engine: tauri::State<'_, Engine>, index: usize) -> Result<Vec<Trim>, String> {
    engine.session()?.delete_cut(index)
}

/// Insert a manual zoom segment at time `t`; returns the updated segment list.
#[tauri::command]
pub async fn add_zoom(
    engine: tauri::State<'_, Engine>,
    t: f64,
) -> Result<Vec<ZoomKeyframe>, String> {
    engine.session()?.add_zoom(t)
}

/// Retime / re-level the zoom segment at `index`; returns the updated segment list.
#[tauri::command]
pub async fn update_zoom(
    engine: tauri::State<'_, Engine>,
    index: usize,
    start: f64,
    end: f64,
    amount: f64,
) -> Result<Vec<ZoomKeyframe>, String> {
    engine.session()?.update_zoom(index, start, end, amount)
}

/// Set a zoom segment's focus: pass `x` + `y` (normalized) to hold a fixed point, or
/// omit both to follow the cursor. Returns the updated segment list.
#[tauri::command]
pub async fn set_zoom_focus(
    engine: tauri::State<'_, Engine>,
    index: usize,
    x: Option<f64>,
    y: Option<f64>,
) -> Result<Vec<ZoomKeyframe>, String> {
    let focus = match (x, y) {
        (Some(x), Some(y)) => Some((x, y)),
        _ => None,
    };
    engine.session()?.set_zoom_focus(index, focus)
}

/// Delete the zoom segment at `index`; returns the updated segment list.
#[tauri::command]
pub async fn delete_zoom(
    engine: tauri::State<'_, Engine>,
    index: usize,
) -> Result<Vec<ZoomKeyframe>, String> {
    engine.session()?.delete_zoom(index)
}

/// Add a text label at normalized `(x, y)` from time `t`.
#[tauri::command]
pub fn add_text(
    engine: tauri::State<'_, Engine>,
    text: String,
    x: f64,
    y: f64,
    t: f64,
) -> Result<u32, String> {
    engine.session()?.add_text(text, x, y, t)
}

/// Add an arrow between normalized points from time `t`.
#[tauri::command]
pub fn add_arrow(
    engine: tauri::State<'_, Engine>,
    fx: f64,
    fy: f64,
    tx: f64,
    ty: f64,
    t: f64,
) -> Result<u32, String> {
    engine.session()?.add_arrow(fx, fy, tx, ty, t)
}

/// Add a highlight box (normalized rect) from time `t`.
#[tauri::command]
pub fn add_box(
    engine: tauri::State<'_, Engine>,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    t: f64,
) -> Result<u32, String> {
    engine.session()?.add_box(x, y, w, h, t)
}

/// Add an ellipse highlight inscribed in the normalized rect from time `t`.
#[tauri::command]
pub fn add_ellipse(
    engine: tauri::State<'_, Engine>,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    t: f64,
) -> Result<u32, String> {
    engine.session()?.add_ellipse(x, y, w, h, t)
}

/// Snapshot every annotation (for the editor overlay).
#[tauri::command]
pub fn list_annotations(engine: tauri::State<'_, Engine>) -> Result<AnnotationSet, String> {
    engine.session()?.annotations()
}

/// Snapshot everything the editor timeline binds to (duration, trim, speed, zooms).
#[tauri::command]
pub fn clip_state(engine: tauri::State<'_, Engine>) -> Result<ClipState, String> {
    engine.session()?.clip_state()
}

/// Toggle click ripples — expanding rings at every recorded mouse click.
#[tauri::command]
pub fn set_show_clicks(engine: tauri::State<'_, Engine>, on: bool) -> Result<(), String> {
    engine.session()?.set_show_clicks(on)
}

/// Apply a framing preset ("none" | "subtle" | "studio") to preview and export.
#[tauri::command]
pub fn set_frame_preset(engine: tauri::State<'_, Engine>, preset: String) -> Result<(), String> {
    engine.session()?.set_frame_preset(&preset)
}

/// Move/edit a text label (omit a field to leave it unchanged).
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub fn update_text(
    engine: tauri::State<'_, Engine>,
    id: u32,
    x: Option<f64>,
    y: Option<f64>,
    text: Option<String>,
    font_size: Option<f32>,
    bold: Option<bool>,
    italic: Option<bool>,
) -> Result<(), String> {
    engine
        .session()?
        .update_text(id, x, y, text, font_size, bold, italic)
}

/// Move an arrow's endpoints.
#[tauri::command]
pub fn update_arrow(
    engine: tauri::State<'_, Engine>,
    id: u32,
    fx: f64,
    fy: f64,
    tx: f64,
    ty: f64,
) -> Result<(), String> {
    engine.session()?.update_arrow(id, fx, fy, tx, ty)
}

/// Move/resize a highlight box.
#[tauri::command]
pub fn update_box(
    engine: tauri::State<'_, Engine>,
    id: u32,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
) -> Result<(), String> {
    engine.session()?.update_box(id, x, y, w, h)
}

/// Tint an annotation (text, arrow, or box) by id.
#[tauri::command]
pub fn set_annotation_color(
    engine: tauri::State<'_, Engine>,
    id: u32,
    r: f64,
    g: f64,
    b: f64,
) -> Result<(), String> {
    engine.session()?.set_annotation_color(id, r, g, b)
}

/// Restyle an arrow or highlight: thickness and (for highlights) filled vs outlined.
#[tauri::command]
pub fn set_annotation_style(
    engine: tauri::State<'_, Engine>,
    id: u32,
    thickness: Option<f64>,
    filled: Option<bool>,
) -> Result<(), String> {
    engine
        .session()?
        .set_annotation_style(id, thickness, filled)
}

/// Retime an annotation (text, arrow, or box): when it appears / disappears.
#[tauri::command]
pub fn update_annotation_range(
    engine: tauri::State<'_, Engine>,
    id: u32,
    start: f64,
    end: f64,
) -> Result<(), String> {
    engine.session()?.update_annotation_range(id, start, end)
}

/// Revert the most recent edit; returns whether anything was undone.
#[tauri::command]
pub async fn undo(engine: tauri::State<'_, Engine>) -> Result<bool, String> {
    engine.session()?.undo()
}

/// Re-apply the most recently undone edit; returns whether anything was redone.
#[tauri::command]
pub async fn redo(engine: tauri::State<'_, Engine>) -> Result<bool, String> {
    engine.session()?.redo()
}

/// Duplicate an annotation (same style/timing, nudged); returns the new id.
#[tauri::command]
pub fn duplicate_annotation(engine: tauri::State<'_, Engine>, id: u32) -> Result<u32, String> {
    engine.session()?.duplicate_annotation(id)
}

/// Delete an annotation (text, arrow, or box) by id.
#[tauri::command]
pub fn delete_annotation(engine: tauri::State<'_, Engine>, id: u32) -> Result<(), String> {
    engine.session()?.delete_annotation(id)
}
