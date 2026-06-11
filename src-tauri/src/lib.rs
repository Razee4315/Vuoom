//! Vuoom desktop app shell. The webview is the cockpit; Rust is the engine.
//!
//! This thin Tauri layer manages the recording [`session::Session`] and registers the
//! command surface in [`commands`] (record/preview/export/annotations). See
//! `docs/02-Architecture.md`.

mod commands;
mod hotkey;
mod live_preview;
mod session;
mod windows_ext;
mod zoom_chord;

use std::sync::{Arc, OnceLock};
use tauri::Manager;

/// Lazily-booted engine, held as Tauri managed state.
///
/// Starting the engine (GPU compositor + preview WebSocket server) takes real time, so it
/// boots on a background thread while the window paints its launch splash. Until it's
/// ready, commands fail with the sentinel `"engine-starting"`, which the frontend retries.
pub struct Engine(Arc<OnceLock<Result<session::Session, String>>>);

impl Engine {
    /// The booted session, an `"engine-starting"` retry hint, or the boot error.
    pub fn session(&self) -> Result<&session::Session, String> {
        match self.0.get() {
            None => Err("engine-starting".into()),
            Some(Ok(s)) => Ok(s),
            Some(Err(e)) => Err(format!("engine failed to start: {e}")),
        }
    }
}

/// System-tray icon with a small Open / Quit menu.
fn build_tray(app: &tauri::AppHandle) -> tauri::Result<()> {
    use tauri::menu::{Menu, MenuItem};
    use tauri::tray::TrayIconBuilder;

    let show = MenuItem::with_id(app, "show", "Open Vuoom", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit Vuoom", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &quit])?;

    let mut tray = TrayIconBuilder::new().menu(&menu).tooltip("Vuoom");
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.on_menu_event(|app, event| match event.id.as_ref() {
        "show" => {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.show();
                let _ = w.unminimize();
                let _ = w.set_focus();
            }
        }
        "quit" => app.exit(0),
        _ => {}
    })
    .build(app)?;
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Per-monitor DPI awareness must be set before any window exists, so the capture crop
    // and cursor coordinates are in true physical pixels on scaled displays.
    let _ = vuoom_input::set_per_monitor_aware_v2();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            // Boot the engine off the main thread: blocking here would stall the event
            // loop and the launch splash would never paint.
            let cell: Arc<OnceLock<Result<session::Session, String>>> = Arc::new(OnceLock::new());
            app.manage(Engine(Arc::clone(&cell)));
            app.manage(hotkey::RecordingHotkey::default());
            std::thread::spawn(move || {
                let _ = cell.set(session::Session::new());
            });
            if let Some(main) = app.get_webview_window("main") {
                let _ = main.maximize();
            }
            build_tray(app.handle())?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::preview_port,
            commands::start_recording,
            commands::seek,
            commands::export_gif,
            commands::add_text,
            commands::add_arrow,
            commands::add_box,
            commands::list_annotations,
            commands::clip_state,
            commands::set_trim,
            commands::auto_speed,
            commands::clear_speed,
            commands::add_speed,
            commands::update_speed,
            commands::delete_speed,
            commands::add_zoom,
            commands::update_zoom,
            commands::delete_zoom,
            commands::estimate_gif,
            commands::copy_gif_to_clipboard,
            commands::update_text,
            commands::update_arrow,
            commands::update_box,
            commands::set_annotation_color,
            commands::update_annotation_range,
            commands::delete_annotation,
            commands::enter_overlay,
            commands::enter_stopbar,
            commands::finish_recording,
            commands::cancel_record_flow,
            commands::set_region,
            commands::screenshot,
            commands::set_zoom_amount,
            commands::save_project_bundle,
            commands::open_project_bundle,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
