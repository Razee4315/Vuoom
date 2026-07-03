//! Synthetic input injection (mouse + keyboard) via `SendInput` — humanized.
//!
//! This is the half that lets an AI agent *drive* a target app: move/click/type/scroll/drag.
//! Crucially, injected events are seen by the global low-level hook in [`crate::recorder`]
//! exactly like hardware events (Vuoom does no injected-input filtering), so an injected
//! click both operates the target app **and** drives the cinematic auto-zoom — one mechanism,
//! two effects. See `docs/13-AI-Demo-Director-Research.md`.
//!
//! **Humanized motion.** The hardware cursor is baked into the captured pixels (WGC draws
//! it), so an instant warp would *teleport* in the recording. Instead, moves glide along a
//! minimum-jerk profile at ~125 Hz with a distance-scaled duration, clicks settle briefly
//! before pressing, text types at a paced (jittered) cadence, and scrolls step one notch at
//! a time. The glide also feeds a continuous stream of real move events to the hook, so the
//! auto-zoom camera follows a path instead of a step function.
//!
//! Coordinates are **virtual-desktop physical pixels** (the capture/zoom space). The pure
//! helpers ([`normalize_abs`], [`key_to_vk`], [`min_jerk`], [`glide_points`],
//! [`glide_duration_ms`], [`jitter_factor`], [`is_extended_vk`]) carry the fiddly math and
//! are unit-tested; the `SendInput` wrappers are Windows-only and runtime-verified.
//!
//! All injection functions return `Err` when Windows accepts fewer events than sent —
//! the classic silent failure is UIPI blocking injection into an elevated window, and the
//! agent must hear about it instead of recording a demo of nothing happening.

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

/// Minimum-jerk ease: `s(p) = 10p³ − 15p⁴ + 6p⁵`, clamped to `[0, 1]`.
///
/// Starts and ends with zero velocity *and* zero acceleration — the profile human reaching
/// movements follow, and why the glide reads as deliberate rather than mechanical.
#[must_use]
pub fn min_jerk(p: f64) -> f64 {
    let p = p.clamp(0.0, 1.0);
    p * p * p * (10.0 + p * (6.0 * p - 15.0))
}

/// Distance-scaled glide duration (ms): `clamp(150 + px/3, 200, 900)`.
///
/// Fitts-flavored: short hops are quick, cross-screen moves take under a second. Callers
/// can always override with an explicit duration.
#[must_use]
pub fn glide_duration_ms(distance_px: f64) -> u32 {
    let d = distance_px.max(0.0);
    (150.0 + d / 3.0).clamp(200.0, 900.0).round() as u32
}

/// Sample `steps` points along the minimum-jerk path from `from` to `to`.
///
/// Returns exactly `steps.max(1)` points, excluding the start and ending precisely at `to`
/// (so the final position is never off by rounding).
#[must_use]
pub fn glide_points(from: (i32, i32), to: (i32, i32), steps: u32) -> Vec<(i32, i32)> {
    let n = steps.max(1);
    (1..=n)
        .map(|k| {
            let s = min_jerk(f64::from(k) / f64::from(n));
            (
                (f64::from(from.0) + f64::from(to.0 - from.0) * s).round() as i32,
                (f64::from(from.1) + f64::from(to.1 - from.1) * s).round() as i32,
            )
        })
        .collect()
}

/// Deterministic per-index pacing jitter in `[1 − spread, 1 + spread]`.
///
/// Keystroke cadence with zero variance reads as robotic; true randomness would make
/// recordings non-reproducible. A hash of the index gives both: humanlike spread, same
/// timing every take.
#[must_use]
pub fn jitter_factor(i: usize, spread: f64) -> f64 {
    let h = (i as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    let unit = (h >> 11) as f64 / (1u64 << 53) as f64; // uniform 0..1
    1.0 + spread * (2.0 * unit - 1.0)
}

/// Whether a virtual-key code is an *extended* key (arrows, nav cluster, Win keys).
///
/// These must be injected with `KEYEVENTF_EXTENDEDKEY`, or scan-code-aware apps
/// (terminals, RDP, games) read e.g. "down arrow" as "numpad 2".
#[must_use]
pub fn is_extended_vk(vk: u16) -> bool {
    // 0x21..=0x28: PgUp, PgDn, End, Home, arrows. 0x2D/0x2E: Insert/Delete. 0x5B/0x5C: Win.
    matches!(vk, 0x21..=0x28 | 0x2D | 0x2E | 0x5B | 0x5C)
}

/// Resolve a key name (case-insensitive) to a Win32 virtual-key code.
///
/// Accepts modifiers (`ctrl`/`control`, `shift`, `alt`, `win`/`super`/`cmd`/`meta`), common
/// named keys (`enter`/`return`, `tab`, `esc`/`escape`, `space`, `backspace`, `delete`, arrows,
/// `home`/`end`, `pageup`/`pagedown`), the function keys `f1`..`f12`, single letters `a`..`z`,
/// digits `0`..`9`, and the OEM punctuation keys (`- = [ ] ; ' , . / \ ` `` and their names,
/// e.g. `plus`/`minus`/`slash` — so chords like Ctrl+= work). Returns `None` for anything else.
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
        // OEM punctuation (US layout VKs) — chords like ctrl+= (browser zoom) need these.
        "-" | "minus" | "dash" => 0xBD,
        "=" | "equals" | "plus" => 0xBB,
        "[" | "openbracket" => 0xDB,
        "]" | "closebracket" => 0xDD,
        ";" | "semicolon" => 0xBA,
        "'" | "quote" | "apostrophe" => 0xDE,
        "," | "comma" => 0xBC,
        "." | "period" | "dot" => 0xBE,
        "/" | "slash" => 0xBF,
        "\\" | "backslash" => 0xDC,
        "`" | "grave" | "backtick" => 0xC0,
        // Numpad cluster — VK_ADD emits a literal `+` unshifted (unlike VK_OEM_PLUS above, which
        // types `=`), so a chord like `["+"]` produces `+`. `multiply`/`subtract`/`divide`/`decimal`
        // and `numpad0`..`numpad9` (0x60..0x69) round out the pad.
        "+" | "add" => 0x6B,
        "*" | "multiply" | "asterisk" | "star" => 0x6A,
        "subtract" => 0x6D,
        "divide" => 0x6F,
        "decimal" => 0x6E,
        _ => return numpad_vk(&n).or_else(|| single_key_vk(&n)),
    };
    Some(vk)
}

/// `numpad0`..`numpad9` → VK_NUMPAD0..VK_NUMPAD9 (0x60..0x69).
fn numpad_vk(n: &str) -> Option<u16> {
    let digit: u16 = n.strip_prefix("numpad")?.parse().ok()?;
    (digit <= 9).then_some(0x60 + digit)
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
pub use platform::{
    click, cursor_pos, drag, key_chord, move_cursor, move_cursor_smooth, scroll, type_text,
    virtual_screen,
};

#[cfg(windows)]
mod platform {
    use super::{
        glide_duration_ms, glide_points, is_extended_vk, jitter_factor, normalize_abs, InjectButton,
    };
    use std::mem::size_of;
    use std::time::Duration;
    use windows::Win32::Foundation::POINT;
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE, KEYBDINPUT, KEYBD_EVENT_FLAGS,
        KEYEVENTF_EXTENDEDKEY, KEYEVENTF_KEYUP, KEYEVENTF_UNICODE, MOUSEEVENTF_ABSOLUTE,
        MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP, MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP,
        MOUSEEVENTF_MOVE, MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP, MOUSEEVENTF_VIRTUALDESK,
        MOUSEEVENTF_WHEEL, MOUSEINPUT, MOUSE_EVENT_FLAGS, VIRTUAL_KEY,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        GetCursorPos, GetSystemMetrics, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN,
        SM_YVIRTUALSCREEN,
    };

    /// One wheel "notch".
    const WHEEL_DELTA: i32 = 120;
    /// Glide sample interval (~125 Hz) — smooth at any capture rate.
    const GLIDE_STEP_MS: u64 = 8;
    /// Pause after arriving at a target before pressing — reads as deliberate, and gives
    /// the auto-zoom pre-roll a stationary point to zoom toward.
    const SETTLE_MS: u64 = 120;
    /// Button hold time for a click (hardware clicks are ~60–100 ms, never 0).
    const PRESS_MS: u64 = 60;
    /// Gap between the two presses of a double-click (well inside GetDoubleClickTime).
    const DOUBLE_GAP_MS: u64 = 80;
    /// Default gap between scroll notches.
    const SCROLL_STEP_MS: u64 = 40;
    /// Default typing speed when the caller does not specify one.
    const DEFAULT_CPS: f64 = 15.0;

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

    /// Current cursor position in virtual-desktop physical pixels.
    #[must_use]
    pub fn cursor_pos() -> (i32, i32) {
        let mut pt = POINT::default();
        // SAFETY: GetCursorPos writes into a valid POINT.
        let _ = unsafe { GetCursorPos(&mut pt) };
        (pt.x, pt.y)
    }

    /// Inject `inputs`, verifying Windows accepted every event.
    ///
    /// UIPI silently swallows injection into elevated windows — surfacing the short count
    /// is the only way the agent learns its click never happened.
    fn send(inputs: &[INPUT]) -> Result<(), String> {
        // SAFETY: `inputs` is a valid, initialized slice; cbSize is the element size.
        let sent = unsafe { SendInput(inputs, size_of::<INPUT>() as i32) };
        if sent as usize == inputs.len() {
            Ok(())
        } else {
            Err(format!(
                "input injection blocked ({sent}/{} events accepted) — is the target app \
                 running elevated?",
                inputs.len()
            ))
        }
    }

    fn sleep_ms(ms: u64) {
        if ms > 0 {
            std::thread::sleep(Duration::from_millis(ms));
        }
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

    /// Warp the cursor to `(x, y)` instantly, without pressing a button.
    ///
    /// # Errors
    /// When Windows rejects the injection (see [`send`]).
    pub fn move_cursor(x: i32, y: i32) -> Result<(), String> {
        send(&[abs_move(x, y)])
    }

    /// Glide the cursor to `(x, y)` along a minimum-jerk path.
    ///
    /// `duration_ms`: `None` = distance-scaled ([`glide_duration_ms`]); `Some(0)` = instant
    /// warp. The glide emits real move events at ~125 Hz, so both the recorded pixels and
    /// the auto-zoom camera see a smooth path.
    ///
    /// # Errors
    /// When Windows rejects the injection (see [`send`]).
    pub fn move_cursor_smooth(x: i32, y: i32, duration_ms: Option<u32>) -> Result<(), String> {
        let from = cursor_pos();
        let dist = f64::from(x - from.0).hypot(f64::from(y - from.1));
        let dur = duration_ms.unwrap_or_else(|| glide_duration_ms(dist));
        if dur == 0 || dist < 1.0 {
            return move_cursor(x, y);
        }
        let steps = (u64::from(dur) / GLIDE_STEP_MS).max(1) as u32;
        for (px, py) in glide_points(from, (x, y), steps) {
            send(&[abs_move(px, py)])?;
            sleep_ms(GLIDE_STEP_MS);
        }
        Ok(())
    }

    fn button_flags(button: InjectButton) -> (MOUSE_EVENT_FLAGS, MOUSE_EVENT_FLAGS) {
        match button {
            InjectButton::Left => (MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP),
            InjectButton::Right => (MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP),
            InjectButton::Middle => (MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP),
        }
    }

    /// Glide to `(x, y)`, settle, and click; `double` issues a second press.
    ///
    /// `glide_ms` as in [`move_cursor_smooth`]. The settle + press pauses keep the cadence
    /// of a hardware click and give the zoom pre-roll a stationary target.
    ///
    /// # Errors
    /// When Windows rejects the injection (see [`send`]).
    pub fn click(
        x: i32,
        y: i32,
        button: InjectButton,
        double: bool,
        glide_ms: Option<u32>,
    ) -> Result<(), String> {
        move_cursor_smooth(x, y, glide_ms)?;
        sleep_ms(SETTLE_MS);
        let (down, up) = button_flags(button);
        send(&[mouse_event(down, 0, 0, 0)])?;
        sleep_ms(PRESS_MS);
        send(&[mouse_event(up, 0, 0, 0)])?;
        if double {
            sleep_ms(DOUBLE_GAP_MS);
            send(&[mouse_event(down, 0, 0, 0)])?;
            sleep_ms(PRESS_MS);
            send(&[mouse_event(up, 0, 0, 0)])?;
        }
        Ok(())
    }

    /// Press-drag from `(x1, y1)` to `(x2, y2)`: glide there, hold `button`, glide the
    /// pressed pointer along the path, release.
    ///
    /// `duration_ms` is the *dragging* portion; `None` = distance-scaled with a 400 ms
    /// floor (drags read better slightly slower than free moves).
    ///
    /// # Errors
    /// When Windows rejects the injection (see [`send`]).
    pub fn drag(
        x1: i32,
        y1: i32,
        x2: i32,
        y2: i32,
        button: InjectButton,
        duration_ms: Option<u32>,
    ) -> Result<(), String> {
        move_cursor_smooth(x1, y1, None)?;
        sleep_ms(SETTLE_MS);
        let (down, up) = button_flags(button);
        send(&[mouse_event(down, 0, 0, 0)])?;
        sleep_ms(100);
        let dist = f64::from(x2 - x1).hypot(f64::from(y2 - y1));
        let dur = duration_ms.unwrap_or_else(|| glide_duration_ms(dist).max(400));
        let steps = (u64::from(dur.max(1)) / GLIDE_STEP_MS).max(1) as u32;
        for (px, py) in glide_points((x1, y1), (x2, y2), steps) {
            send(&[abs_move(px, py)])?;
            sleep_ms(GLIDE_STEP_MS);
        }
        sleep_ms(80);
        send(&[mouse_event(up, 0, 0, 0)])
    }

    /// Scroll `delta` notches at `(x, y)` (positive = up), one notch per step so smooth-
    /// scrolling apps animate and the recording reads as intentional.
    ///
    /// `step_ms`: gap between notches (`None` = 40 ms; `Some(0)` = all at once).
    ///
    /// # Errors
    /// When Windows rejects the injection (see [`send`]).
    pub fn scroll(x: i32, y: i32, delta: i32, step_ms: Option<u32>) -> Result<(), String> {
        send(&[abs_move(x, y)])?;
        if delta == 0 {
            return Ok(());
        }
        let gap = step_ms.map_or(SCROLL_STEP_MS, u64::from);
        if gap == 0 {
            return send(&[mouse_event(MOUSEEVENTF_WHEEL, 0, 0, delta * WHEEL_DELTA)]);
        }
        let notch = if delta > 0 { WHEEL_DELTA } else { -WHEEL_DELTA };
        for i in 0..delta.unsigned_abs() {
            send(&[mouse_event(MOUSEEVENTF_WHEEL, 0, 0, notch)])?;
            if i + 1 < delta.unsigned_abs() {
                sleep_ms(gap);
            }
        }
        Ok(())
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
        let mut flags = if up {
            KEYEVENTF_KEYUP
        } else {
            KEYBD_EVENT_FLAGS(0)
        };
        if is_extended_vk(vk) {
            flags |= KEYEVENTF_EXTENDEDKEY;
        }
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

    /// Type `text` at a human cadence into the focused control.
    ///
    /// `cps`: characters per second (`None` = 15; clamped to `0.5..=200`). Each character
    /// is followed by a deterministically jittered pause, so typing sustains the auto-zoom
    /// hold like real typing does. `\n` and `\t` are pressed as real Enter/Tab (Unicode
    /// 0x0A does not activate default buttons); `\r` is skipped.
    ///
    /// # Errors
    /// When Windows rejects the injection (see [`send`]).
    pub fn type_text(text: &str, cps: Option<f64>) -> Result<(), String> {
        let cps = match cps {
            Some(c) if c.is_finite() && c > 0.0 => c.clamp(0.5, 200.0),
            _ => DEFAULT_CPS,
        };
        let base_ms = 1000.0 / cps;
        let mut units = [0u16; 2];
        let mut first = true;
        for (i, ch) in text.chars().enumerate() {
            if ch == '\r' {
                continue;
            }
            if !first {
                sleep_ms((base_ms * jitter_factor(i, 0.3)).round() as u64);
            }
            first = false;
            match ch {
                '\n' => send(&[vk_event(0x0D, false), vk_event(0x0D, true)])?,
                '\t' => send(&[vk_event(0x09, false), vk_event(0x09, true)])?,
                _ => {
                    let mut inputs = Vec::with_capacity(4);
                    for &unit in ch.encode_utf16(&mut units).iter() {
                        inputs.push(key_unit(unit, false));
                        inputs.push(key_unit(unit, true));
                    }
                    send(&inputs)?;
                }
            }
        }
        Ok(())
    }

    /// Press a chord of virtual-key codes: hold all in order, brief pause, release in
    /// reverse. Pass modifiers first (e.g. Ctrl, then C). Extended keys get the extended
    /// flag automatically.
    ///
    /// # Errors
    /// When Windows rejects the injection (see [`send`]).
    pub fn key_chord(vks: &[u16]) -> Result<(), String> {
        if vks.is_empty() {
            return Ok(());
        }
        let downs: Vec<INPUT> = vks.iter().map(|&vk| vk_event(vk, false)).collect();
        send(&downs)?;
        sleep_ms(50);
        let ups: Vec<INPUT> = vks.iter().rev().map(|&vk| vk_event(vk, true)).collect();
        send(&ups)
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
    fn min_jerk_is_monotone_and_bounded() {
        assert_eq!(min_jerk(0.0), 0.0);
        assert!((min_jerk(1.0) - 1.0).abs() < 1e-12);
        assert!(
            (min_jerk(0.5) - 0.5).abs() < 1e-12,
            "odd symmetry at midpoint"
        );
        let mut prev = 0.0;
        for k in 1..=100 {
            let s = min_jerk(f64::from(k) / 100.0);
            assert!(s >= prev, "min_jerk must be monotone");
            assert!((0.0..=1.0).contains(&s));
            prev = s;
        }
        // Clamps outside the unit interval.
        assert_eq!(min_jerk(-1.0), 0.0);
        assert!((min_jerk(2.0) - 1.0).abs() < 1e-12);
    }

    #[test]
    fn min_jerk_starts_and_ends_slow() {
        // Near the endpoints progress is much slower than linear (zero end velocities).
        assert!(min_jerk(0.05) < 0.01);
        assert!(min_jerk(0.95) > 0.99);
    }

    #[test]
    fn glide_points_end_exactly_at_target() {
        let pts = glide_points((0, 0), (1000, -500), 37);
        assert_eq!(pts.len(), 37);
        assert_eq!(*pts.last().unwrap(), (1000, -500));
        // Progress along x is monotone for a monotone profile.
        let mut prev = 0;
        for &(x, _) in &pts {
            assert!(x >= prev, "x must be monotone");
            prev = x;
        }
        // Degenerate step count still yields the target.
        assert_eq!(glide_points((3, 4), (3, 4), 0), vec![(3, 4)]);
    }

    #[test]
    fn glide_duration_scales_with_distance() {
        assert_eq!(glide_duration_ms(0.0), 200); // floor
        assert_eq!(glide_duration_ms(3000.0), 900); // ceiling
        let mid = glide_duration_ms(600.0);
        assert!((300..=400).contains(&mid), "600px → ~350ms, got {mid}");
    }

    #[test]
    fn jitter_is_deterministic_and_bounded() {
        for i in 0..1000 {
            let f = jitter_factor(i, 0.3);
            assert!((0.7..=1.3).contains(&f), "i={i} f={f}");
            assert_eq!(f, jitter_factor(i, 0.3), "must be deterministic");
        }
        // Not constant — at least two distinct values in a small window.
        assert!((0..10)
            .map(|i| jitter_factor(i, 0.3))
            .any(|f| f != jitter_factor(0, 0.3)));
    }

    #[test]
    fn extended_vk_set_covers_nav_cluster() {
        for vk in [
            0x21, 0x22, 0x23, 0x24, 0x25, 0x26, 0x27, 0x28, 0x2D, 0x2E, 0x5B,
        ] {
            assert!(is_extended_vk(vk), "vk={vk:#x} must be extended");
        }
        for vk in [0x0D, 0x09, 0x41, 0x30, 0x11, 0x10] {
            assert!(!is_extended_vk(vk), "vk={vk:#x} must not be extended");
        }
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
    fn oem_punctuation_maps() {
        assert_eq!(key_to_vk("="), Some(0xBB));
        assert_eq!(key_to_vk("plus"), Some(0xBB));
        assert_eq!(key_to_vk("-"), Some(0xBD));
        assert_eq!(key_to_vk("minus"), Some(0xBD));
        assert_eq!(key_to_vk("/"), Some(0xBF));
        assert_eq!(key_to_vk("slash"), Some(0xBF));
        assert_eq!(key_to_vk(","), Some(0xBC));
        assert_eq!(key_to_vk("."), Some(0xBE));
        assert_eq!(key_to_vk(";"), Some(0xBA));
        assert_eq!(key_to_vk("'"), Some(0xDE));
        assert_eq!(key_to_vk("["), Some(0xDB));
        assert_eq!(key_to_vk("]"), Some(0xDD));
        assert_eq!(key_to_vk("\\"), Some(0xDC));
        assert_eq!(key_to_vk("`"), Some(0xC0));
    }

    #[test]
    fn numpad_and_symbol_keys_map() {
        // VK_ADD emits a literal `+` (VK_OEM_PLUS / "plus" stays 0xBB for ctrl+= chords).
        assert_eq!(key_to_vk("+"), Some(0x6B));
        assert_eq!(key_to_vk("add"), Some(0x6B));
        assert_eq!(key_to_vk("plus"), Some(0xBB));
        assert_eq!(key_to_vk("*"), Some(0x6A));
        assert_eq!(key_to_vk("multiply"), Some(0x6A));
        assert_eq!(key_to_vk("star"), Some(0x6A));
        assert_eq!(key_to_vk("subtract"), Some(0x6D));
        assert_eq!(key_to_vk("divide"), Some(0x6F));
        assert_eq!(key_to_vk("decimal"), Some(0x6E));
        // Numpad digit cluster (0x60..0x69).
        assert_eq!(key_to_vk("numpad0"), Some(0x60));
        assert_eq!(key_to_vk("numpad5"), Some(0x65));
        assert_eq!(key_to_vk("numpad9"), Some(0x69));
        assert_eq!(key_to_vk("NUMPAD9"), Some(0x69));
        // Out-of-range / malformed numpad names fall through to None.
        assert_eq!(key_to_vk("numpad10"), None);
        assert_eq!(key_to_vk("numpad"), None);
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
