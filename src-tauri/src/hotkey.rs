//! Global stop-recording hotkey (Ctrl+Shift+X).
//!
//! While a recording runs, the editor window is a small always-on-top panel that usually
//! doesn't have focus, so a normal keydown listener can't stop the recording. This watcher
//! polls the async key state on a background thread (the same pattern as the live
//! preview's Ctrl+Shift+Z zoom chord) and emits a `stop-hotkey` event to the webview on
//! the chord's rising edge; the overlay UI then runs its normal Stop path.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tauri::{AppHandle, Emitter};
use windows::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;

const VK_SHIFT: i32 = 0x10;
const VK_CONTROL: i32 = 0x11;
const VK_X: i32 = 0x58;

/// Managed Tauri state: the watcher for the recording in progress, if any.
#[derive(Default)]
pub struct RecordingHotkey(pub Mutex<Option<StopHotkey>>);

/// A running hotkey watcher. Dropping it (or replacing it in the state) stops the poll.
pub struct StopHotkey {
    stop: Arc<AtomicBool>,
}

impl StopHotkey {
    /// Start polling for Ctrl+Shift+X; emits `stop-hotkey` to the webview when pressed.
    #[must_use]
    pub fn watch(app: AppHandle) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&stop);
        std::thread::spawn(move || {
            // Start "pressed" so a chord held while recording begins must be released first.
            let mut prev = true;
            while !flag.load(Ordering::Relaxed) {
                let down = chord_down();
                if down && !prev {
                    let _ = app.emit("stop-hotkey", ());
                }
                prev = down;
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

/// True while Ctrl AND Shift AND X are all held.
fn chord_down() -> bool {
    key_down(VK_CONTROL) && key_down(VK_SHIFT) && key_down(VK_X)
}

fn key_down(vk: i32) -> bool {
    // The high-order bit of GetAsyncKeyState is set while the key is down.
    (unsafe { GetAsyncKeyState(vk) } as u16 & 0x8000) != 0
}
