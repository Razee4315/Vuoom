//! Vuoom desktop app shell. The webview is the cockpit; Rust is the engine.
//!
//! This thin Tauri layer registers the command surface in [`commands`] and (as they come
//! online) will own the capture / render / preview / export pipeline. See
//! `docs/02-Architecture.md`.

mod commands;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            commands::default_zoom_config,
            commands::new_project,
            commands::save_project,
            commands::load_project,
            commands::gif_presets,
            commands::estimate_gif_size,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
