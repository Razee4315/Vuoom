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
use tauri::{AppHandle, Emitter, LogicalSize, Manager, PhysicalPosition, PhysicalSize};
use vuoom_capture::CropRegion;
use vuoom_project::{SpeedRegion, Trim, ZoomKeyframe, ZoomStyle};

/// The visible frame around the recorded region, plus the region it should frame.
/// Held as Tauri managed state so the record-flow commands can show/clear it.
#[derive(Default)]
pub struct BorderState {
    /// A copy of the region chosen by the selector (monitor-relative physical px);
    /// `None` = full screen.
    pub region: Mutex<Option<CropRegion>>,
    /// Virtual-desktop origin of the monitor being recorded (physical px).
    pub origin: Mutex<(i32, i32)>,
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

/// Step 1 — the user clicked Record: capture the desktop for the selector backdrop, then
/// blow the editor up to fullscreen (excluded from capture) so the in-window region
/// selector covers the display. Returns the backdrop as a `data:image/png;base64,…` URL
/// (empty string if the grab failed — the selector then just shows a dark canvas).
///
/// The recording targets the monitor the editor is on right now — fullscreen lands there,
/// so drag Vuoom to another monitor to record that one.
///
/// Why the screenshot is taken while the window is **hidden** rather than relying on
/// `WDA_EXCLUDEFROMCAPTURE`: on Windows 11 a *fullscreen* excluded window is composited
/// through the direct-flip path and Windows Graphics Capture records its area as solid
/// black (the desktop behind it is never sampled) — that produced the blank selector
/// backdrop. Windows 10 happens to composite the desktop behind the excluded window, so it
/// worked there. Hiding the window for the single grab is correct on both.
#[tauri::command]
pub fn enter_overlay(
    app: AppHandle,
    engine: tauri::State<'_, Engine>,
    border: tauri::State<'_, BorderState>,
) -> Result<String, String> {
    let main = app.get_webview_window("main").ok_or("no main window")?;
    let monitor = main.current_monitor().ok().flatten();
    if let Ok(mut slot) = border.origin.lock() {
        *slot = monitor
            .as_ref()
            .map_or((0, 0), |m| (m.position().x, m.position().y));
    }
    if let Ok(session) = engine.session() {
        let info = monitor.as_ref().and_then(|m| {
            m.name().map(|n| crate::session::MonitorInfo {
                name: n.clone(),
                x: m.position().x,
                y: m.position().y,
                w: m.size().width,
                h: m.size().height,
            })
        });
        let _ = session.set_monitor(info);
    }

    // Hide the window and let DWM recompose without it before grabbing the clean desktop.
    let _ = main.hide();
    std::thread::sleep(std::time::Duration::from_millis(120));
    // A failed grab yields an empty backdrop (the selector still works, just without the
    // still). Only worth a line once the engine is actually up — an "engine-starting" retry
    // here isn't a failure. A blank selector was previously silent, so log the real cause.
    let backdrop = match engine.session() {
        Ok(session) => session.screenshot().unwrap_or_else(|e| {
            tracing::warn!("region selector backdrop screenshot failed: {e}");
            String::new()
        }),
        Err(_) => String::new(),
    };

    // Best-effort from here on: the window is currently hidden, so `show()` MUST run on
    // every path — bailing early with `?` would strand an invisible window.
    let _ = exclude_from_capture(&main);
    // The editor sits maximized, and an undecorated maximized window only covers the WORK
    // area (screen minus taskbar). Entering fullscreen straight from that state can leave
    // the window work-area sized: the backdrop screenshot (full monitor) gets squashed
    // upward, the taskbar stays uncovered, and every drawn selection lands offset from
    // where it looked. Unmaximize and pin the window to the monitor's true physical bounds
    // before going fullscreen.
    let _ = main.unmaximize();
    if let Some(m) = monitor.as_ref() {
        let _ = main.set_position(PhysicalPosition::new(m.position().x, m.position().y));
        let _ = main.set_size(PhysicalSize::new(m.size().width, m.size().height));
    }
    let _ = main.set_always_on_top(true);
    let _ = main.set_fullscreen(true);
    let _ = main.show();
    let _ = main.set_focus();
    Ok(backdrop)
}

/// Width/height (logical px) of the recording panel — a live zoom preview + Stop controls.
const PANEL_W: f64 = 384.0;
const PANEL_H: f64 = 300.0;

/// Breathing room (physical px) kept between the panel and the recorded region, so the
/// panel also clears the 3px border strips drawn just outside the region.
const REGION_PAD: i32 = 16;

/// An axis-aligned rect in virtual-desktop physical px: `(x, y, w, h)`.
type Rect = (i32, i32, i32, i32);

fn inflate((x, y, w, h): Rect, pad: i32) -> Rect {
    (x - pad, y - pad, w + 2 * pad, h + 2 * pad)
}

fn rects_overlap(a: Rect, b: Rect) -> bool {
    a.0 < b.0 + b.2 && b.0 < a.0 + a.2 && a.1 < b.1 + b.3 && b.1 < a.1 + a.3
}

/// The pending capture region in virtual-desktop physical px (`None` = full screen).
fn region_virtual(border: &BorderState) -> Option<Rect> {
    let r = border.region.lock().ok().and_then(|r| *r)?;
    let (ox, oy) = border.origin.lock().map_or((0, 0), |o| *o);
    Some((ox + r.x as i32, oy + r.y as i32, r.w as i32, r.h as i32))
}

/// The window's current on-screen rect, if it can be read.
fn window_rect(win: &tauri::WebviewWindow) -> Option<Rect> {
    let pos = win.outer_position().ok()?;
    let size = win.outer_size().ok()?;
    Some((pos.x, pos.y, size.width as i32, size.height as i32))
}

/// First monitor corner where a `panel`-sized window stays clear of `region`
/// (bottom-right → bottom-left → top-right → top-left). `None` when every corner
/// overlaps; full-screen recordings get bottom-right (the panel only needs to sit
/// somewhere for the countdown — `start_recording` minimizes it before capture matters).
fn pick_panel_spot(
    mon: Rect,
    panel: (i32, i32),
    margin: i32,
    region: Option<Rect>,
) -> Option<(i32, i32)> {
    let (mx, my, mw, mh) = mon;
    let (pw, ph) = panel;
    let corners = [
        (mx + mw - pw - margin, my + mh - ph - margin), // bottom-right
        (mx + margin, my + mh - ph - margin),           // bottom-left
        (mx + mw - pw - margin, my + margin),           // top-right
        (mx + margin, my + margin),                     // top-left
    ];
    let Some(r) = region else {
        return Some(corners[0]);
    };
    let r = inflate(r, REGION_PAD);
    corners
        .into_iter()
        .find(|&(px, py)| !rects_overlap((px, py, pw, ph), r))
}

/// Step 2 — a region (or full screen) was confirmed: drop out of fullscreen and shrink the
/// window to the always-on-top recording panel (live preview + Stop). The panel is parked
/// on a monitor corner OUTSIDE the recorded region — capture exclusion alone isn't enough
/// (on Windows 10 an excluded window can still land in the recording as a black box), so
/// the panel must physically stay out of the captured area.
#[tauri::command]
pub fn enter_stopbar(app: AppHandle, border: tauri::State<'_, BorderState>) -> Result<(), String> {
    let main = app.get_webview_window("main").ok_or("no main window")?;
    main.set_fullscreen(false).map_err(|e| e.to_string())?;
    main.set_size(LogicalSize::new(PANEL_W, PANEL_H))
        .map_err(|e| e.to_string())?;
    main.set_always_on_top(true).map_err(|e| e.to_string())?;
    if let Ok(Some(mon)) = main.current_monitor() {
        let scale = mon.scale_factor();
        let mp = mon.position();
        let ms = mon.size();
        let panel = ((PANEL_W * scale) as i32, (PANEL_H * scale) as i32);
        let margin = (24.0 * scale) as i32;
        let mon_rect = (mp.x, mp.y, ms.width as i32, ms.height as i32);
        let (x, y) = pick_panel_spot(mon_rect, panel, margin, region_virtual(&border)).unwrap_or((
            mp.x + mon_rect.2 - panel.0 - margin,
            mp.y + mon_rect.3 - panel.1 - margin,
        ));
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
        (None, None, None, None) => None, // no fields → full screen
        (Some(x), Some(y), Some(w), Some(h)) => Some(CropRegion { x, y, w, h }),
        // A partial spec is a caller bug — don't silently fall back to full screen.
        _ => return Err("set_region: provide all of x, y, w, h — or none for full screen".into()),
    };
    // Validate against the target monitor first; only mirror an accepted region into the
    // border state, so a rejected rect can't leave a stale frame behind.
    engine.session()?.set_region(region)?;
    if let Ok(mut slot) = border.region.lock() {
        *slot = region;
    }
    Ok(())
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

/// Duration (s) of a recoverable session left by a crash/close, if one exists.
#[tauri::command]
pub fn check_recovery(engine: tauri::State<'_, Engine>) -> Result<Option<f64>, String> {
    Ok(engine.session()?.recovery_available())
}

/// Reload the recoverable session into the editor.
#[tauri::command]
pub async fn recover_session(engine: tauri::State<'_, Engine>) -> Result<RecordingSummary, String> {
    engine.session()?.recover_session()
}

/// Bytes held in the recovery store plus how many sessions those bytes back.
#[derive(Clone, Copy, Serialize)]
pub struct RecoveryStorage {
    bytes: u64,
    sessions: usize,
}

/// Report the recovery store's disk usage for the storage readout.
#[tauri::command]
pub fn recovery_storage(engine: tauri::State<'_, Engine>) -> Result<RecoveryStorage, String> {
    let (bytes, sessions) = engine.session()?.recovery_storage();
    Ok(RecoveryStorage { bytes, sessions })
}

/// Delete saved recovery data (except the currently-loaded clip's store); returns bytes freed.
#[tauri::command]
pub fn clear_recovery_storage(engine: tauri::State<'_, Engine>) -> Result<u64, String> {
    engine.session()?.clear_recovery_storage()
}

// ── Recording / preview / export ──────────────────────────────────────────────

/// How the webview reaches the live preview stream: the localhost port plus the per-session
/// auth token it must include in the WS URL path (`ws://127.0.0.1:{port}/ws/{token}`). The
/// token gates the socket so no other local process can read the user's screen preview.
#[derive(Clone, Serialize)]
pub struct PreviewConn {
    pub port: u16,
    pub token: String,
}

/// The localhost port + auth token the webview connects with for the live preview stream.
#[tauri::command]
pub fn preview_port(engine: tauri::State<'_, Engine>) -> Result<PreviewConn, String> {
    let session = engine.session()?;
    Ok(PreviewConn {
        port: session.preview_port(),
        token: session.preview_token(),
    })
}

/// One-shot engine health snapshot, queried by the frontend right after boot.
///
/// `gpu: false` means the compositor failed to initialize (no adapter, driver failure):
/// preview and export won't work, though recording to disk still does. Surfacing this up
/// front lets the UI warn once instead of every seek/export failing with a cryptic string.
#[derive(Clone, Copy, Serialize)]
pub struct EngineHealth {
    pub gpu: bool,
}

#[tauri::command]
pub fn engine_health(engine: tauri::State<'_, Engine>) -> Result<EngineHealth, String> {
    Ok(EngineHealth {
        gpu: engine.session()?.has_gpu(),
    })
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
    let (mx, my) = border.origin.lock().map_or((0, 0), |o| *o);
    if let (Some(r), Ok(mut slot)) = (region, border.border.lock()) {
        *slot = RegionBorder::show(mx + r.x as i32, my + r.y as i32, r.w as i32, r.h as i32);
    }
    // Capture exclusion isn't airtight: on Windows 10 an excluded window can still show up
    // in the recording as a black rectangle. If the panel would sit inside the captured
    // area (always true for full-screen recordings), minimize it for the duration —
    // Ctrl+Shift+X still stops the recording and restores the window.
    if let Some(main) = app.get_webview_window("main") {
        let covered = match region {
            None => true,
            Some(r) => {
                let rect = inflate(
                    (mx + r.x as i32, my + r.y as i32, r.w as i32, r.h as i32),
                    REGION_PAD,
                );
                match window_rect(&main) {
                    Some(p) => rects_overlap(p, rect),
                    None => true,
                }
            }
        };
        if covered {
            let _ = main.minimize();
        }
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

/// Abort an in-flight GIF/MP4 export. The export loop bails at its next frame check with an
/// `"export cancelled"` error and deletes its partial output file. A no-op if nothing is
/// exporting (the flag resets when the next export starts).
#[tauri::command]
pub async fn cancel_export(engine: tauri::State<'_, Engine>) -> Result<(), String> {
    engine.session()?.cancel_export();
    Ok(())
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

/// Put an exported file (GIF or MP4) on the clipboard as CF_HDROP, so pasting into Slack /
/// Discord / a GitHub comment uploads the actual file.
#[tauri::command]
pub fn copy_export_to_clipboard(path: String) -> Result<(), String> {
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

/// Set a zoom segment's easing "feel": `style` is one of `"smooth"` (default), `"snappy"`,
/// or `"slow"` (case-insensitive). Returns the updated segment list. Also the backend for
/// the MCP `set_zoom_style` control command.
#[tauri::command]
pub async fn set_zoom_style(
    engine: tauri::State<'_, Engine>,
    index: usize,
    style: String,
) -> Result<Vec<ZoomKeyframe>, String> {
    let style = ZoomStyle::from_label(&style)
        .ok_or_else(|| format!("unknown zoom style: {style}"))?;
    engine.session()?.set_zoom_style(index, style)
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

/// Add a translucent filled highlighter rectangle from time `t`.
#[tauri::command]
pub fn add_highlighter(
    engine: tauri::State<'_, Engine>,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    t: f64,
) -> Result<u32, String> {
    engine.session()?.add_highlighter(x, y, w, h, t)
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

/// Toggle the keystroke overlay — shortcut chips (Ctrl+C…) at the bottom of the frame.
#[tauri::command]
pub fn set_show_keys(engine: tauri::State<'_, Engine>, on: bool) -> Result<(), String> {
    engine.session()?.set_show_keys(on)
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
    background: Option<bool>,
    font: Option<String>,
) -> Result<(), String> {
    engine
        .session()?
        .update_text(id, x, y, text, font_size, bold, italic, background, font)
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

/// Set the alpha/opacity (0..1) of an annotation's color.
#[tauri::command]
pub fn set_annotation_opacity(
    engine: tauri::State<'_, Engine>,
    id: u32,
    a: f64,
) -> Result<(), String> {
    engine.session()?.set_annotation_opacity(id, a)
}

/// Switch a highlight between rectangle (`false`) and ellipse (`true`).
#[tauri::command]
pub fn set_highlight_shape(
    engine: tauri::State<'_, Engine>,
    id: u32,
    ellipse: bool,
) -> Result<(), String> {
    engine.session()?.set_highlight_shape(id, ellipse)
}

/// Set an arrow's head style: "arrow", "line", or "double".
#[tauri::command]
pub fn set_arrow_style(
    engine: tauri::State<'_, Engine>,
    id: u32,
    style: String,
) -> Result<(), String> {
    engine.session()?.set_arrow_style(id, &style)
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
