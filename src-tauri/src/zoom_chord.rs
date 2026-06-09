//! Poll-based recorder for the manual zoom chord (Ctrl+Shift+Z).
//!
//! The recording pipeline's low-level keyboard hook can miss keystrokes — most notably
//! when an elevated (admin) window has focus, where Windows withholds hook callbacks but
//! `GetAsyncKeyState` still reports key state. The live preview already detects the chord
//! by polling, which is why a zoom can show live yet be missing from the final edit. This
//! poller records the same chord presses the live preview sees, and the session merges
//! them with the hook-detected marks at stop time.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use vuoom_input::Clock;
use windows::Win32::Foundation::POINT;
use windows::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;
use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;

const VK_SHIFT: i32 = 0x10;
const VK_CONTROL: i32 = 0x11;
const VK_Z: i32 = 0x5A;

/// One polled chord press: QPC timestamp + physical cursor position.
#[derive(Debug, Clone, Copy)]
pub struct ChordMark {
    pub qpc: i64,
    pub x: i32,
    pub y: i32,
}

/// A running chord poller. [`Self::finish`] stops it and returns the presses.
pub struct ZoomChordPoller {
    stop: Arc<AtomicBool>,
    marks: Arc<Mutex<Vec<ChordMark>>>,
    handle: Option<JoinHandle<()>>,
}

impl ZoomChordPoller {
    #[must_use]
    pub fn start() -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let marks = Arc::new(Mutex::new(Vec::new()));
        let (stop_w, marks_w) = (Arc::clone(&stop), Arc::clone(&marks));
        let handle = std::thread::spawn(move || {
            let clock = Clock::new();
            let mut prev = true; // require a clean press after recording starts
            while !stop_w.load(Ordering::Relaxed) {
                let down = key_down(VK_CONTROL) && key_down(VK_SHIFT) && key_down(VK_Z);
                if down && !prev {
                    let mut p = POINT::default();
                    if unsafe { GetCursorPos(&mut p) }.is_ok() {
                        if let Ok(mut v) = marks_w.lock() {
                            v.push(ChordMark {
                                qpc: clock.now(),
                                x: p.x,
                                y: p.y,
                            });
                        }
                    }
                }
                prev = down;
                std::thread::sleep(Duration::from_millis(10));
            }
        });
        Self {
            stop,
            marks,
            handle: Some(handle),
        }
    }

    /// Stop polling and return every chord press seen during the recording.
    pub fn finish(mut self) -> Vec<ChordMark> {
        self.halt();
        self.marks.lock().map(|v| v.clone()).unwrap_or_default()
    }

    fn halt(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

impl Drop for ZoomChordPoller {
    fn drop(&mut self) {
        self.halt();
    }
}

fn key_down(vk: i32) -> bool {
    // The high-order bit of GetAsyncKeyState is set while the key is down.
    (unsafe { GetAsyncKeyState(vk) } as u16 & 0x8000) != 0
}
