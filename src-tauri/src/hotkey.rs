//! Global stop-recording hotkey (Ctrl+Shift+X unless rebound).
//!
//! While a recording runs, the editor window is a small always-on-top panel that usually
//! doesn't have focus, so a normal keydown listener can't stop the recording. This watcher
//! checks for the chord on a background thread (the keyboard hook's count of presses it
//! swallowed, and the key state for the ones it can't see; see `vuoom_input::ChordWatch`)
//! and emits a `stop-hotkey` event to the webview on each press; the overlay UI then runs
//! its normal Stop path.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tauri::{AppHandle, Emitter};
use vuoom_input::{ChordWatch, Hotkey};

/// Managed Tauri state: the watcher for the recording in progress, if any.
#[derive(Default)]
pub struct RecordingHotkey(pub Mutex<Option<StopHotkey>>);

/// A running hotkey watcher. Dropping it (or replacing it in the state) stops the poll.
pub struct StopHotkey {
    stop: Arc<AtomicBool>,
}

impl StopHotkey {
    /// Start watching for the stop chord; emits `stop-hotkey` to the webview when pressed.
    #[must_use]
    pub fn watch(app: AppHandle) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&stop);
        std::thread::spawn(move || {
            // A chord held while recording begins must be released first.
            let mut watch = ChordWatch::new(Hotkey::Stop);
            while !flag.load(Ordering::Relaxed) {
                if watch.pressed() {
                    let _ = app.emit("stop-hotkey", ());
                }
                std::thread::sleep(Duration::from_millis(30));
            }
        });
        Self { stop }
    }
}

impl Drop for StopHotkey {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}
