//! Global mouse + keyboard log on macOS: a Quartz event tap on its own run-loop thread.
//!
//! The same contract as the Windows low-level hooks: every event is stamped on the master
//! clock the instant it arrives, positions are in the pixel space of `super`, keys are Win32
//! VK codes, and Vuoom's own hotkeys are counted and swallowed so the recorded app never
//! sees them (`crate::chords`).
//!
//! An active tap (one that can swallow keys) needs the Accessibility permission. Without it
//! the recorder falls back to a listen-only tap (Input Monitoring): everything is still
//! logged, but a hotkey also reaches the app being recorded. With neither, it logs nothing
//! and says so once.

use super::{mac_to_vk, to_pixels, Modifiers};
use crate::clock::Clock;
use crate::event::{MouseButton, RawEvent, RawEventKind};
use objc2_core_foundation::{
    kCFRunLoopCommonModes, kCFRunLoopDefaultMode, CFMachPort, CFRetained, CFRunLoop,
};
use objc2_core_graphics::{
    CGEvent, CGEventField, CGEventMask, CGEventTapLocation, CGEventTapOptions, CGEventTapPlacement,
    CGEventTapProxy, CGEventType,
};
use std::cell::RefCell;
use std::ffi::c_void;
use std::ptr::NonNull;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

struct Tap {
    tx: Sender<RawEvent>,
    clock: Clock,
    port: Option<CFRetained<CFMachPort>>,
    /// Modifiers last seen, to turn `flagsChanged` into key downs and ups.
    mods: Modifiers,
    /// Whether the tap may swallow (an active tap).
    active: bool,
    /// When the pointer's shape was last checked (it is read at most every 50 ms).
    last_shape_check: Option<Instant>,
}

thread_local! {
    static TAP: RefCell<Option<Tap>> = const { RefCell::new(None) };
}

fn emit(tap: &Tap, kind: RawEventKind, x: i32, y: i32) {
    let qpc = tap.clock.now();
    let _ = tap.tx.send(RawEvent { qpc, x, y, kind });
}

fn mask(types: &[CGEventType]) -> CGEventMask {
    types.iter().fold(0, |m, t| m | (1u64 << t.0))
}

/// The modifier key codes macOS reports in `flagsChanged`, with which flag they move.
fn modifier_key(code: u16) -> Option<fn(&Modifiers) -> bool> {
    Some(match code {
        0x38 | 0x3C => |m: &Modifiers| m.shift,
        0x3B | 0x3E => |m: &Modifiers| m.ctrl,
        0x3A | 0x3D => |m: &Modifiers| m.alt,
        0x37 | 0x36 => |m: &Modifiers| m.cmd,
        _ => return None,
    })
}

fn button(event: &CGEvent, ty: CGEventType) -> MouseButton {
    match ty {
        CGEventType::LeftMouseDown | CGEventType::LeftMouseUp => MouseButton::Left,
        CGEventType::RightMouseDown | CGEventType::RightMouseUp => MouseButton::Right,
        _ => {
            match CGEvent::integer_value_field(Some(event), CGEventField::MouseEventButtonNumber) {
                3 => MouseButton::X1,
                4 => MouseButton::X2,
                _ => MouseButton::Middle,
            }
        }
    }
}

/// Scroll in Windows wheel units (120 a notch, positive away from the user).
fn scroll_delta(event: &CGEvent) -> i32 {
    let continuous =
        CGEvent::integer_value_field(Some(event), CGEventField::ScrollWheelEventIsContinuous) != 0;
    if continuous {
        // Trackpads report pixels; ~10 px of travel is roughly a wheel notch.
        let px = CGEvent::integer_value_field(
            Some(event),
            CGEventField::ScrollWheelEventPointDeltaAxis1,
        );
        (px * 12) as i32
    } else {
        let lines =
            CGEvent::integer_value_field(Some(event), CGEventField::ScrollWheelEventDeltaAxis1);
        (lines * 120) as i32
    }
}

/// Handle one event on the tap thread. Returns `true` when it must be swallowed.
fn handle(tap: &mut Tap, ty: CGEventType, event: &CGEvent) -> bool {
    let p = CGEvent::location(Some(event));
    let (x, y) = to_pixels(p.x, p.y);
    let kind = match ty {
        CGEventType::MouseMoved
        | CGEventType::LeftMouseDragged
        | CGEventType::RightMouseDragged
        | CGEventType::OtherMouseDragged => Some(RawEventKind::Move),
        CGEventType::LeftMouseDown | CGEventType::RightMouseDown | CGEventType::OtherMouseDown => {
            Some(RawEventKind::ButtonDown(button(event, ty)))
        }
        CGEventType::LeftMouseUp | CGEventType::RightMouseUp | CGEventType::OtherMouseUp => {
            Some(RawEventKind::ButtonUp(button(event, ty)))
        }
        CGEventType::ScrollWheel => Some(RawEventKind::Scroll(scroll_delta(event))),
        _ => None,
    };
    if let Some(kind) = kind {
        emit(tap, kind, x, y);
        // The pointer changes shape as it moves over things: note each change.
        let due = tap
            .last_shape_check
            .is_none_or(|t| t.elapsed() >= Duration::from_millis(50));
        if due {
            tap.last_shape_check = Some(Instant::now());
            if let Some(shape) = super::cursor_changed() {
                emit(tap, RawEventKind::Cursor(shape), x, y);
            }
        }
        return false;
    }
    let code = CGEvent::integer_value_field(Some(event), CGEventField::KeyboardEventKeycode) as u16;
    match ty {
        CGEventType::FlagsChanged => {
            let now = Modifiers::from_flags(CGEvent::flags(Some(event)));
            if let (Some(held), Some(vk)) = (modifier_key(code), mac_to_vk(code)) {
                let (was, is) = (held(&tap.mods), held(&now));
                if was != is {
                    let kind = if is {
                        RawEventKind::KeyDown(vk)
                    } else {
                        RawEventKind::KeyUp(vk)
                    };
                    emit(tap, kind, 0, 0);
                }
            }
            tap.mods = now;
            false
        }
        CGEventType::KeyDown | CGEventType::KeyUp => {
            let Some(vk) = mac_to_vk(code) else {
                return false;
            };
            let down = ty == CGEventType::KeyDown;
            tap.mods = Modifiers::from_flags(CGEvent::flags(Some(event)));
            let kind = if down {
                RawEventKind::KeyDown(vk)
            } else {
                RawEventKind::KeyUp(vk)
            };
            emit(tap, kind, 0, 0);
            // One of Vuoom's own hotkeys: logged and counted, and stops here (when the tap
            // is allowed to), so the app being recorded never gets it.
            crate::chords::hook_key(vk, down) && tap.active
        }
        _ => false,
    }
}

unsafe extern "C-unwind" fn callback(
    _proxy: CGEventTapProxy,
    ty: CGEventType,
    event: NonNull<CGEvent>,
    _info: *mut c_void,
) -> *mut CGEvent {
    // SAFETY: the tap hands us a live event for the duration of the callback.
    let ev = unsafe { event.as_ref() };
    let swallow = TAP.with(|t| {
        let mut t = t.borrow_mut();
        let Some(tap) = t.as_mut() else {
            return false;
        };
        if ty == CGEventType::TapDisabledByTimeout || ty == CGEventType::TapDisabledByUserInput {
            // The system turns a tap off when a callback runs long; turn it straight back on.
            if let Some(port) = &tap.port {
                CGEvent::tap_enable(port, true);
            }
            return false;
        }
        handle(tap, ty, ev)
    });
    if swallow {
        std::ptr::null_mut()
    } else {
        event.as_ptr()
    }
}

fn create_tap(options: CGEventTapOptions) -> Option<CFRetained<CFMachPort>> {
    let interest = mask(&[
        CGEventType::MouseMoved,
        CGEventType::LeftMouseDown,
        CGEventType::LeftMouseUp,
        CGEventType::RightMouseDown,
        CGEventType::RightMouseUp,
        CGEventType::OtherMouseDown,
        CGEventType::OtherMouseUp,
        CGEventType::LeftMouseDragged,
        CGEventType::RightMouseDragged,
        CGEventType::OtherMouseDragged,
        CGEventType::ScrollWheel,
        CGEventType::KeyDown,
        CGEventType::KeyUp,
        CGEventType::FlagsChanged,
    ]);
    // SAFETY: `callback` matches CGEventTapCallBack and touches only thread-local state.
    unsafe {
        CGEvent::tap_create(
            CGEventTapLocation::SessionEventTap,
            CGEventTapPlacement::HeadInsertEventTap,
            options,
            interest,
            Some(callback),
            std::ptr::null_mut(),
        )
    }
}

/// A running global input recorder. Forwards clock-stamped events to its [`Receiver`];
/// dropping it (or calling [`InputRecorder::stop`]) ends capture.
pub struct InputRecorder {
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl InputRecorder {
    /// Install the event tap on a dedicated run-loop thread, returning the recorder and the
    /// receiver of stamped events.
    #[must_use]
    pub fn start() -> (Self, Receiver<RawEvent>) {
        let (tx, rx) = channel::<RawEvent>();
        let stop = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&stop);
        let (ready_tx, ready_rx) = channel::<()>();
        crate::chords::hook_reset();
        let handle = thread::spawn(move || {
            let (port, active) = match create_tap(CGEventTapOptions::Default) {
                Some(p) => (Some(p), true),
                None => {
                    let listen = create_tap(CGEventTapOptions::ListenOnly);
                    if listen.is_some() {
                        tracing::warn!(
                            "no Accessibility access: input is logged, but Vuoom's hotkeys \
                             also reach the recorded app"
                        );
                    } else {
                        tracing::warn!(
                            "no Input Monitoring or Accessibility access: clicks and keys \
                             are not logged (no auto-zoom, no keystroke overlay)"
                        );
                    }
                    (listen, false)
                }
            };
            let source = port
                .as_deref()
                .and_then(|p| CFMachPort::new_run_loop_source(None, Some(p), 0));
            let run_loop = CFRunLoop::current();
            if let (Some(rl), Some(src)) = (&run_loop, &source) {
                // SAFETY: a framework constant.
                rl.add_source(Some(src), unsafe { kCFRunLoopCommonModes });
            }
            TAP.with(|t| {
                *t.borrow_mut() = Some(Tap {
                    tx,
                    clock: Clock::new(),
                    port: port.clone(),
                    mods: super::modifiers_down(),
                    active,
                    last_shape_check: None,
                });
            });
            let _ = ready_tx.send(());
            while !flag.load(Ordering::Relaxed) {
                if port.is_some() {
                    // SAFETY: a framework constant.
                    CFRunLoop::run_in_mode(unsafe { kCFRunLoopDefaultMode }, 0.1, false);
                } else {
                    thread::sleep(Duration::from_millis(100));
                }
            }
            if let Some(p) = &port {
                CGEvent::tap_enable(p, false);
                p.invalidate();
            }
            TAP.with(|t| *t.borrow_mut() = None);
        });
        let _ = ready_rx.recv_timeout(Duration::from_secs(2));
        (
            Self {
                stop,
                handle: Some(handle),
            },
            rx,
        )
    }

    /// Stop capture and join the tap thread.
    pub fn stop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
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
