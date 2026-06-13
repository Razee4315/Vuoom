//! Synthetic input injection (mouse + keyboard) via `SendInput`.
//!
//! This is the half that lets an AI agent *drive* a target app: move/click/type/scroll.
//! Crucially, injected events are seen by the global low-level hook in [`crate::recorder`]
//! exactly like hardware events (Vuoom does no injected-input filtering), so an injected
//! click both operates the target app **and** drives the cinematic auto-zoom — one mechanism,
//! two effects. See `docs/13-AI-Demo-Director-Research.md`.
//!
//! Coordinates are **virtual-desktop physical pixels** (the capture/zoom space). The pure
//! [`normalize_abs`] / [`key_to_vk`] helpers carry the fiddly math and are unit-tested;
//! the `SendInput` wrappers are Windows-only and runtime-verified on a real machine.

/// Which mouse button to inject.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum InjectButton {
    /// Primary (left) button.
    #[default]
    Left,
    /// Secondary (right) button.
    Right,
    /// Middle (wheel) button.
    Middle,
}

/// Map a physical virtual-desktop pixel to a `SendInput` absolute coordinate (`0..=65535`).
///
/// `SendInput` with `MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_VIRTUALDESK` expects the point
/// normalized across the whole virtual desktop, where `65535` maps to the far edge. `vx`/`vy`
/// are the desktop origin and `vw`/`vh` its size (from [`virtual_screen`]). The result is
/// clamped into range; degenerate sizes collapse to `0`.
#[must_use]
pub fn normalize_abs(x: i32, y: i32, vx: i32, vy: i32, vw: i32, vh: i32) -> (i32, i32) {
    let map = |v: i32, origin: i32, size: i32| -> i32 {
        if size <= 1 {
            return 0;
        }
        let rel = i64::from(v - origin);
        let scaled = (rel * 65535) / i64::from(size - 1);
        scaled.clamp(0, 65535) as i32
    };
    (map(x, vx, vw), map(y, vy, vh))
}

/// Resolve a key name (case-insensitive) to a Win32 virtual-key code.
///
/// Accepts modifiers (`ctrl`/`control`, `shift`, `alt`, `win`/`super`/`cmd`/`meta`), common
/// named keys (`enter`/`return`, `tab`, `esc`/`escape`, `space`, `backspace`, `delete`, arrows,
/// `home`/`end`, `pageup`/`pagedown`), the function keys `f1`..`f12`, single letters `a`..`z`,
/// and digits `0`..`9`. Returns `None` for anything else.
#[must_use]
pub fn key_to_vk(name: &str) -> Option<u16> {
    let n = name.trim().to_ascii_lowercase();
    let vk = match n.as_str() {
        "ctrl" | "control" => 0x11,
        "shift" => 0x10,
        "alt" | "menu" => 0x12,
        "win" | "super" | "cmd" | "meta" => 0x5B,
        "enter" | "return" => 0x0D,
        "tab" => 0x09,
        "esc" | "escape" => 0x1B,
        "space" | "spacebar" => 0x20,
        "backspace" | "back" => 0x08,
        "delete" | "del" => 0x2E,
        "insert" | "ins" => 0x2D,
        "up" => 0x26,
        "down" => 0x28,
        "left" => 0x25,
        "right" => 0x27,
        "home" => 0x24,
        "end" => 0x23,
        "pageup" | "pgup" => 0x21,
        "pagedown" | "pgdn" => 0x22,
        _ => return single_key_vk(&n),
    };
    Some(vk)
}

/// Letters, digits, and `f1`..`f12` — the single-token keys not in the named-key table.
fn single_key_vk(n: &str) -> Option<u16> {
    let bytes = n.as_bytes();
    match bytes {
        [c @ b'a'..=b'z'] => Some(u16::from(c.to_ascii_uppercase())),
        [d @ b'0'..=b'9'] => Some(u16::from(*d)),
        [b'f', ..] => {
            let num: u16 = n[1..].parse().ok()?;
            (1..=12).contains(&num).then(|| 0x70 + (num - 1))
        }
        _ => None,
    }
}

#[cfg(windows)]
pub use platform::{click, key_chord, move_cursor, scroll, type_text, virtual_screen};

#[cfg(windows)]
mod platform {
    use super::{normalize_abs, InjectButton};
    use std::mem::size_of;
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE, KEYBDINPUT, KEYBD_EVENT_FLAGS,
        KEYEVENTF_KEYUP, KEYEVENTF_UNICODE, MOUSEEVENTF_ABSOLUTE, MOUSEEVENTF_LEFTDOWN,
        MOUSEEVENTF_LEFTUP, MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP, MOUSEEVENTF_MOVE,
        MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP, MOUSEEVENTF_VIRTUALDESK, MOUSEEVENTF_WHEEL,
        MOUSEINPUT, MOUSE_EVENT_FLAGS, VIRTUAL_KEY,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        GetSystemMetrics, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN,
        SM_YVIRTUALSCREEN,
    };

    /// One wheel "notch".
    const WHEEL_DELTA: i32 = 120;

    /// The virtual-desktop bounds in physical pixels: `(x, y, width, height)`.
    #[must_use]
    pub fn virtual_screen() -> (i32, i32, i32, i32) {
        // SAFETY: GetSystemMetrics is a pure read of system configuration.
        unsafe {
            (
                GetSystemMetrics(SM_XVIRTUALSCREEN),
                GetSystemMetrics(SM_YVIRTUALSCREEN),
                GetSystemMetrics(SM_CXVIRTUALSCREEN),
                GetSystemMetrics(SM_CYVIRTUALSCREEN),
            )
        }
    }

    fn send(inputs: &[INPUT]) {
        // SAFETY: `inputs` is a valid, initialized slice; cbSize is the element size.
        let _sent = unsafe { SendInput(inputs, size_of::<INPUT>() as i32) };
    }

    fn mouse_event(flags: MOUSE_EVENT_FLAGS, dx: i32, dy: i32, data: i32) -> INPUT {
        INPUT {
            r#type: INPUT_MOUSE,
            Anonymous: INPUT_0 {
                mi: MOUSEINPUT {
                    dx,
                    dy,
                    mouseData: data as u32,
                    dwFlags: flags,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        }
    }

    fn abs_move(x: i32, y: i32) -> INPUT {
        let (vx, vy, vw, vh) = virtual_screen();
        let (ax, ay) = normalize_abs(x, y, vx, vy, vw, vh);
        mouse_event(
            MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_VIRTUALDESK,
            ax,
            ay,
            0,
        )
    }

    /// Move the cursor to `(x, y)` (physical px) without pressing a button.
    pub fn move_cursor(x: i32, y: i32) {
        send(&[abs_move(x, y)]);
    }

    /// Click at `(x, y)` with `button`; `double` issues a second down/up pair.
    pub fn click(x: i32, y: i32, button: InjectButton, double: bool) {
        let (down, up) = match button {
            InjectButton::Left => (MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP),
            InjectButton::Right => (MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP),
            InjectButton::Middle => (MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP),
        };
        let mut inputs = vec![
            abs_move(x, y),
            mouse_event(down, 0, 0, 0),
            mouse_event(up, 0, 0, 0),
        ];
        if double {
            inputs.push(mouse_event(down, 0, 0, 0));
            inputs.push(mouse_event(up, 0, 0, 0));
        }
        send(&inputs);
    }

    /// Scroll the wheel `delta` notches at `(x, y)` (positive = up).
    pub fn scroll(x: i32, y: i32, delta: i32) {
        send(&[
            abs_move(x, y),
            mouse_event(MOUSEEVENTF_WHEEL, 0, 0, delta * WHEEL_DELTA),
        ]);
    }

    fn key_unit(scan: u16, up: bool) -> INPUT {
        let mut flags = KEYEVENTF_UNICODE;
        if up {
            flags |= KEYEVENTF_KEYUP;
        }
        INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: VIRTUAL_KEY(0),
                    wScan: scan,
                    dwFlags: flags,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        }
    }

    fn vk_event(vk: u16, up: bool) -> INPUT {
        let flags = if up {
            KEYEVENTF_KEYUP
        } else {
            KEYBD_EVENT_FLAGS(0)
        };
        INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: VIRTUAL_KEY(vk),
                    wScan: 0,
                    dwFlags: flags,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        }
    }

    /// Type `text` as Unicode into the focused control (one down/up per UTF-16 code unit).
    pub fn type_text(text: &str) {
        let mut inputs = Vec::with_capacity(text.len() * 2);
        for unit in text.encode_utf16() {
            inputs.push(key_unit(unit, false));
            inputs.push(key_unit(unit, true));
        }
        if !inputs.is_empty() {
            send(&inputs);
        }
    }

    /// Press a chord of virtual-key codes: hold all in order, then release in reverse. Pass
    /// modifiers first (e.g. Ctrl, then C).
    pub fn key_chord(vks: &[u16]) {
        if vks.is_empty() {
            return;
        }
        let mut inputs = Vec::with_capacity(vks.len() * 2);
        for &vk in vks {
            inputs.push(vk_event(vk, false));
        }
        for &vk in vks.iter().rev() {
            inputs.push(vk_event(vk, true));
        }
        send(&inputs);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_maps_corners_to_full_range() {
        // A 1920x1080 desktop at origin (0,0): top-left → 0, bottom-right → 65535.
        assert_eq!(normalize_abs(0, 0, 0, 0, 1920, 1080), (0, 0));
        assert_eq!(normalize_abs(1919, 1079, 0, 0, 1920, 1080), (65535, 65535));
        // Centre is roughly half.
        let (cx, cy) = normalize_abs(960, 540, 0, 0, 1920, 1080);
        assert!((32000..34000).contains(&cx), "cx={cx}");
        assert!((32000..34000).contains(&cy), "cy={cy}");
    }

    #[test]
    fn normalize_handles_negative_origin_and_clamps() {
        // Secondary monitor to the left: origin x = -1920, total width 3840.
        let (ax, _) = normalize_abs(-1920, 0, -1920, 0, 3840, 1080);
        assert_eq!(ax, 0);
        // Out-of-bounds points clamp into range rather than overflow.
        assert_eq!(
            normalize_abs(10_000, 10_000, 0, 0, 1920, 1080),
            (65535, 65535)
        );
        assert_eq!(normalize_abs(-50, -50, 0, 0, 1920, 1080), (0, 0));
    }

    #[test]
    fn degenerate_screen_collapses_to_zero() {
        assert_eq!(normalize_abs(5, 5, 0, 0, 0, 0), (0, 0));
        assert_eq!(normalize_abs(5, 5, 0, 0, 1, 1), (0, 0));
    }

    #[test]
    fn key_names_map_to_expected_vks() {
        assert_eq!(key_to_vk("ctrl"), Some(0x11));
        assert_eq!(key_to_vk("CONTROL"), Some(0x11));
        assert_eq!(key_to_vk("shift"), Some(0x10));
        assert_eq!(key_to_vk("enter"), Some(0x0D));
        assert_eq!(key_to_vk("return"), Some(0x0D));
        assert_eq!(key_to_vk("esc"), Some(0x1B));
        assert_eq!(key_to_vk(" Tab "), Some(0x09));
    }

    #[test]
    fn letters_digits_and_function_keys() {
        assert_eq!(key_to_vk("a"), Some(0x41));
        assert_eq!(key_to_vk("Z"), Some(0x5A));
        assert_eq!(key_to_vk("0"), Some(0x30));
        assert_eq!(key_to_vk("9"), Some(0x39));
        assert_eq!(key_to_vk("f1"), Some(0x70));
        assert_eq!(key_to_vk("f12"), Some(0x7B));
    }

    #[test]
    fn unknown_keys_are_none() {
        assert_eq!(key_to_vk(""), None);
        assert_eq!(key_to_vk("f13"), None);
        assert_eq!(key_to_vk("f0"), None);
        assert_eq!(key_to_vk("hello"), None);
        assert_eq!(key_to_vk("ab"), None);
    }
}
