//! Global low-level mouse + keyboard hooks on a dedicated thread → QPC-stamped events.
//!
//! `WH_MOUSE_LL` / `WH_KEYBOARD_LL` require a message pump on the installing thread, so the
//! recorder owns a thread that pumps and forwards events over a channel. Events are stamped
//! with QPC the instant they arrive. See `docs/04-Input-and-AutoZoom.md` Part A.
//! (Compile-verified on CI; runtime needs a real interactive Windows session.)

use crate::clock::Clock;
use crate::event::{MouseButton, RawEvent, RawEventKind};
use std::cell::RefCell;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::thread::{self, JoinHandle};
use windows::Win32::Foundation::{HINSTANCE, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DispatchMessageW, GetMessageW, PostThreadMessageW, SetWindowsHookExW,
    TranslateMessage, UnhookWindowsHookEx, HC_ACTION, KBDLLHOOKSTRUCT, MSG, MSLLHOOKSTRUCT,
    WH_KEYBOARD_LL, WH_MOUSE_LL, WM_KEYDOWN, WM_KEYUP, WM_LBUTTONDOWN, WM_LBUTTONUP,
    WM_MBUTTONDOWN, WM_MBUTTONUP, WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_QUIT, WM_RBUTTONDOWN,
    WM_RBUTTONUP, WM_SYSKEYDOWN, WM_SYSKEYUP, WM_XBUTTONDOWN, WM_XBUTTONUP,
};

thread_local! {
    static TX: RefCell<Option<Sender<RawEvent>>> = const { RefCell::new(None) };
    static CLOCK: Clock = Clock::new();
}

fn emit(kind: RawEventKind, x: i32, y: i32) {
    let qpc = CLOCK.with(Clock::now);
    TX.with(|t| {
        if let Some(tx) = t.borrow().as_ref() {
            let _ = tx.send(RawEvent { qpc, x, y, kind });
        }
    });
}

fn xbutton(mouse_data: u32) -> MouseButton {
    if (mouse_data >> 16) & 0xffff == 1 {
        MouseButton::X1
    } else {
        MouseButton::X2
    }
}

unsafe extern "system" fn mouse_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        let info = &*(lparam.0 as *const MSLLHOOKSTRUCT);
        let kind = match wparam.0 as u32 {
            WM_MOUSEMOVE => Some(RawEventKind::Move),
            WM_LBUTTONDOWN => Some(RawEventKind::ButtonDown(MouseButton::Left)),
            WM_LBUTTONUP => Some(RawEventKind::ButtonUp(MouseButton::Left)),
            WM_RBUTTONDOWN => Some(RawEventKind::ButtonDown(MouseButton::Right)),
            WM_RBUTTONUP => Some(RawEventKind::ButtonUp(MouseButton::Right)),
            WM_MBUTTONDOWN => Some(RawEventKind::ButtonDown(MouseButton::Middle)),
            WM_MBUTTONUP => Some(RawEventKind::ButtonUp(MouseButton::Middle)),
            WM_XBUTTONDOWN => Some(RawEventKind::ButtonDown(xbutton(info.mouseData))),
            WM_XBUTTONUP => Some(RawEventKind::ButtonUp(xbutton(info.mouseData))),
            WM_MOUSEWHEEL => {
                let delta = i32::from(((info.mouseData >> 16) & 0xffff) as i16);
                Some(RawEventKind::Scroll(delta))
            }
            _ => None,
        };
        if let Some(kind) = kind {
            emit(kind, info.pt.x, info.pt.y);
        }
    }
    CallNextHookEx(None, code, wparam, lparam)
}

unsafe extern "system" fn key_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        let info = &*(lparam.0 as *const KBDLLHOOKSTRUCT);
        let vk = info.vkCode as u16;
        let kind = match wparam.0 as u32 {
            WM_KEYDOWN | WM_SYSKEYDOWN => Some(RawEventKind::KeyDown(vk)),
            WM_KEYUP | WM_SYSKEYUP => Some(RawEventKind::KeyUp(vk)),
            _ => None,
        };
        if let Some(kind) = kind {
            emit(kind, 0, 0);
        }
    }
    CallNextHookEx(None, code, wparam, lparam)
}

/// A running global input recorder. Forwards QPC-stamped events to its [`Receiver`];
/// dropping it (or calling [`InputRecorder::stop`]) ends capture.
pub struct InputRecorder {
    thread_id: u32,
    handle: Option<JoinHandle<()>>,
}

impl InputRecorder {
    /// Install global mouse + keyboard hooks on a dedicated pump thread, returning the
    /// recorder and the receiver of QPC-stamped events.
    #[must_use]
    pub fn start() -> (Self, Receiver<RawEvent>) {
        let (tx, rx) = channel::<RawEvent>();
        let (id_tx, id_rx) = channel::<u32>();

        let handle = thread::spawn(move || {
            TX.with(|t| *t.borrow_mut() = Some(tx));
            // SAFETY: standard Win32 low-level hook install + message pump on this thread.
            unsafe {
                let hmod = GetModuleHandleW(None).unwrap_or_default();
                let hinst = HINSTANCE(hmod.0);
                let mouse = SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_proc), Some(hinst), 0).ok();
                let kbd = SetWindowsHookExW(WH_KEYBOARD_LL, Some(key_proc), Some(hinst), 0).ok();

                let _ = id_tx.send(GetCurrentThreadId());

                let mut msg = MSG::default();
                while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }

                if let Some(h) = mouse {
                    let _ = UnhookWindowsHookEx(h);
                }
                if let Some(h) = kbd {
                    let _ = UnhookWindowsHookEx(h);
                }
            }
        });

        let thread_id = id_rx.recv().unwrap_or(0);
        (
            Self {
                thread_id,
                handle: Some(handle),
            },
            rx,
        )
    }

    /// Stop capture and join the pump thread.
    pub fn stop(&mut self) {
        if self.thread_id != 0 {
            // SAFETY: posts WM_QUIT to our own pump thread to break GetMessageW.
            unsafe {
                let _ = PostThreadMessageW(self.thread_id, WM_QUIT, WPARAM(0), LPARAM(0));
            }
            self.thread_id = 0;
        }
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

impl Drop for InputRecorder {
    fn drop(&mut self) {
        self.stop();
    }
}
