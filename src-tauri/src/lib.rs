//! Vuoom desktop app shell. The webview is the cockpit; Rust is the engine.
//!
//! This thin Tauri layer manages the recording [`session::Session`] and registers the
//! command surface in [`commands`] (record/preview/export/annotations). See
//! `docs/02-Architecture.md`.

mod commands;
mod frame_store;
mod hotkey;
mod live_preview;
mod mp4;
mod region_border;
mod session;
mod windows_ext;
mod zoom_chord;

use std::path::PathBuf;
use std::sync::{Arc, OnceLock};
use tauri::Manager;
use tracing_appender::non_blocking::WorkerGuard;

/// Keeps the non-blocking file-log worker thread alive for the whole process.
/// Dropping the guard flushes and shuts the writer down, so we park it here for
/// the app's lifetime rather than in a local that would drop at end of `setup`.
static LOG_GUARD: OnceLock<WorkerGuard> = OnceLock::new();

/// Wire up `tracing` before anything interesting happens.
///
/// Two sinks: stderr (visible under `cargo run` / a console build) and a rotating
/// daily file under `log_dir` — the latter matters because release builds set
/// `windows_subsystem = "windows"` and have no console, so every engine
/// `tracing::warn!` would otherwise vanish. Level honors `RUST_LOG`; the default
/// is info for the Vuoom crates and warn for everything else. Uses `try_init`, so
/// a second call (e.g. from a test harness) is a harmless no-op.
fn init_logging(log_dir: PathBuf) {
    use tracing_subscriber::{fmt, prelude::*, EnvFilter};

    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        EnvFilter::new(
            "warn,vuoom=info,vuoom_lib=info,vuoom_capture=info,vuoom_input=info,\
             vuoom_zoom=info,vuoom_render=info,vuoom_encode=info,vuoom_project=info,\
             vuoom_preview=info",
        )
    });

    // Best-effort file sink: if the log dir can't be created (or the appender can't be
    // built) we still keep stderr rather than losing diagnostics outright. Option<Layer>
    // is itself a Layer, so a `None` here is simply a no-op in the stack below.
    //
    // Daily rotation with `max_log_files` so the history is bounded: the appender prunes the
    // oldest files as it rolls over, keeping roughly the last week instead of growing forever.
    let file_layer = std::fs::create_dir_all(&log_dir).ok().and_then(|()| {
        tracing_appender::rolling::Builder::new()
            .rotation(tracing_appender::rolling::Rotation::DAILY)
            .filename_prefix("vuoom.log")
            .max_log_files(7)
            .build(&log_dir)
            .ok()
            .map(|appender| {
                let (writer, guard) = tracing_appender::non_blocking(appender);
                let _ = LOG_GUARD.set(guard);
                fmt::layer().with_ansi(false).with_writer(writer)
            })
    });

    let _ = tracing_subscriber::registry()
        .with(filter)
        .with(fmt::layer().with_writer(std::io::stderr))
        .with(file_layer)
        .try_init();

    tracing::info!(version = env!("CARGO_PKG_VERSION"), "Vuoom starting");
}

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

    // Route worker-thread panics to the log — release builds set `windows_subsystem =
    // "windows"` and have no console, so an un-hooked panic on the capture/drain/preview
    // threads would vanish silently. Chain to the previous hook so the default stderr
    // message still prints. Events emitted before the subscriber is installed (inside
    // `.setup()`) go nowhere, which is acceptable for a crash this early.
    let prev_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let loc = info.location().map_or_else(
            || "unknown".to_string(),
            |l| format!("{}:{}", l.file(), l.line()),
        );
        let msg = info
            .payload()
            .downcast_ref::<&str>()
            .map(|s| (*s).to_string())
            .or_else(|| info.payload().downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "<non-string panic payload>".to_string());
        tracing::error!(location = %loc, "thread panicked: {msg}");
        prev_hook(info);
    }));

    tauri::Builder::default()
        // Single-instance must be registered first (documented requirement). A second
        // launch fires this callback in the running instance — instead of spinning up a
        // rival that would race on the shared %TEMP%/vuoom-recovery store — then exits.
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.show();
                let _ = w.unminimize();
                let _ = w.set_focus();
            }
        }))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(|app| {
            // Stand logging up first so engine-boot warnings are captured. Logs land in
            // the OS app-log dir (Windows: %LOCALAPPDATA%\dev.vuoom.desktop\logs); if that
            // can't be resolved we fall back to a `logs/` dir under the working dir.
            let log_dir = app
                .path()
                .app_log_dir()
                .unwrap_or_else(|_| PathBuf::from("logs"));
            init_logging(log_dir);

            // Boot the engine off the main thread: blocking here would stall the event
            // loop and the launch splash would never paint.
            let cell: Arc<OnceLock<Result<session::Session, String>>> = Arc::new(OnceLock::new());
            app.manage(Engine(Arc::clone(&cell)));
            app.manage(hotkey::RecordingHotkey::default());
            app.manage(commands::BorderState::default());
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
            commands::set_record_paused,
            commands::seek,
            commands::export_gif,
            commands::export_mp4,
            commands::add_text,
            commands::add_arrow,
            commands::add_box,
            commands::add_ellipse,
            commands::add_highlighter,
            commands::list_annotations,
            commands::clip_state,
            commands::set_show_clicks,
            commands::set_show_keys,
            commands::set_frame_preset,
            commands::set_trim,
            commands::auto_speed,
            commands::clear_speed,
            commands::add_speed,
            commands::update_speed,
            commands::delete_speed,
            commands::add_cut,
            commands::update_cut,
            commands::delete_cut,
            commands::add_zoom,
            commands::update_zoom,
            commands::set_zoom_focus,
            commands::delete_zoom,
            commands::estimate_gif,
            commands::copy_export_to_clipboard,
            commands::update_text,
            commands::update_arrow,
            commands::update_box,
            commands::set_annotation_color,
            commands::set_annotation_style,
            commands::set_annotation_opacity,
            commands::set_highlight_shape,
            commands::set_arrow_style,
            commands::update_annotation_range,
            commands::duplicate_annotation,
            commands::delete_annotation,
            commands::undo,
            commands::redo,
            commands::enter_overlay,
            commands::enter_stopbar,
            commands::finish_recording,
            commands::cancel_record_flow,
            commands::set_region,
            commands::screenshot,
            commands::set_zoom_amount,
            commands::save_project_bundle,
            commands::open_project_bundle,
            commands::check_recovery,
            commands::recover_session,
            commands::recovery_storage,
            commands::clear_recovery_storage,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
