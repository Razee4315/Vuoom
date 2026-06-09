//! Vuoom desktop app shell. The webview is the cockpit; Rust is the engine.
//!
//! This thin Tauri layer manages the recording [`session::Session`] and registers the
//! command surface in [`commands`] (pure-logic helpers + record/preview/export). See
//! `docs/02-Architecture.md`.

mod commands;
mod session;
mod windows_ext;

use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Per-monitor DPI awareness must be set before any window exists, so the capture crop
    // and cursor coordinates are in true physical pixels on scaled displays.
    let _ = vuoom_input::set_per_monitor_aware_v2();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            app.manage(session::Session::new());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::default_zoom_config,
            commands::new_project,
            commands::save_project,
            commands::load_project,
            commands::gif_presets,
            commands::estimate_gif_size,
            commands::preview_port,
            commands::start_recording,
            commands::stop_recording,
            commands::seek,
            commands::export_gif,
            commands::add_text,
            commands::add_arrow,
            commands::add_box,
            commands::list_annotations,
            commands::update_text,
            commands::update_arrow,
            commands::update_box,
            commands::set_annotation_color,
            commands::delete_annotation,
            commands::start_record_flow,
            commands::begin_countdown,
            commands::finish_recording,
            commands::cancel_record_flow,
            commands::set_region,
            commands::screenshot,
            commands::save_project_bundle,
            commands::open_project_bundle,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
