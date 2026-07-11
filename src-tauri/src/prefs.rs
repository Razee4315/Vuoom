//! Durable key/value preferences, stored as a flat JSON object in the app data dir.
//!
//! The frontend's first-run flags (e.g. "seen the welcome tour") lived in `localStorage`,
//! which does not survive reliably on every machine — the tour kept reappearing every
//! launch. These commands persist a small `prefs.json` on disk instead. Best-effort by
//! design: any IO/parse error is logged and treated as "no value" / a silent no-op rather
//! than surfaced to the user.

use std::collections::BTreeMap;
use std::path::PathBuf;

use tauri::{AppHandle, Manager};

/// `app_data_dir()/prefs.json`, if the data dir can be resolved.
fn prefs_path(app: &AppHandle) -> Option<PathBuf> {
    app.path().app_data_dir().ok().map(|d| d.join("prefs.json"))
}

/// Load the whole flat map (empty on any missing-file / parse error).
fn load(app: &AppHandle) -> BTreeMap<String, String> {
    let Some(path) = prefs_path(app) else {
        return BTreeMap::new();
    };
    match std::fs::read_to_string(&path) {
        Ok(text) => serde_json::from_str(&text).unwrap_or_else(|e| {
            tracing::warn!("prefs.json parse failed, ignoring: {e}");
            BTreeMap::new()
        }),
        Err(_) => BTreeMap::new(), // not written yet — normal first run
    }
}

/// Read a stored preference; `None` if unset or unreadable.
#[tauri::command]
pub fn get_pref(app: AppHandle, key: String) -> Option<String> {
    load(&app).get(&key).cloned()
}

/// Write a preference. Best-effort: a failure to create the dir or write the file is logged
/// and swallowed (the frontend treats a lost write as "not seen yet", never an error).
#[tauri::command]
pub fn set_pref(app: AppHandle, key: String, value: String) {
    let Some(path) = prefs_path(&app) else {
        tracing::warn!("set_pref: could not resolve app data dir; dropping {key}");
        return;
    };
    let mut map = load(&app);
    map.insert(key, value);
    if let Some(dir) = path.parent() {
        if let Err(e) = std::fs::create_dir_all(dir) {
            tracing::warn!("set_pref: create_dir_all failed: {e}");
            return;
        }
    }
    match serde_json::to_string_pretty(&map) {
        Ok(text) => {
            if let Err(e) = std::fs::write(&path, text) {
                tracing::warn!("set_pref: write failed: {e}");
            }
        }
        Err(e) => tracing::warn!("set_pref: serialize failed: {e}"),
    }
}
